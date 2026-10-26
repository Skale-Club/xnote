//! Records the microphone ("me") and what the computer plays ("them", via
//! WASAPI loopback, so any call app works and no bot joins the meeting) as
//! two separate streams. Each is turned into 16 kHz mono and cut into speech
//! segments at pauses, ready for the transcriber.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Sample rate whisper.cpp expects.
pub const RATE: usize = 16_000;
const FRAME: usize = RATE * 30 / 1000; // 30 ms
const PREROLL: usize = RATE * 300 / 1000; // keep 300 ms before speech starts

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Who {
    Me,
    Them,
}

pub struct Segment {
    pub who: Who,
    /// Milliseconds since the recording started.
    pub start_ms: u64,
    pub samples: Vec<f32>,
}

/// Current input levels (RMS) for the meters, as f32 bits.
#[derive(Default)]
pub struct Levels {
    me: AtomicU32,
    them: AtomicU32,
}

impl Levels {
    pub fn get(&self) -> (f32, f32) {
        (f32::from_bits(self.me.load(Ordering::Relaxed)), f32::from_bits(self.them.load(Ordering::Relaxed)))
    }

    fn set(&self, who: Who, rms: f32) {
        let slot = if who == Who::Me { &self.me } else { &self.them };
        slot.store(rms.to_bits(), Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.set(Who::Me, 0.0);
        self.set(Who::Them, 0.0);
    }
}

/// Linear resampler to 16 kHz, with a one-pole low-pass in front so
/// downsampling doesn't fold high frequencies into the speech band.
struct Resampler {
    step: f64,
    pos: f64,
    alpha: f32,
    smooth: f32,
    buf: Vec<f32>,
}

impl Resampler {
    fn new(rate: u32) -> Self {
        let cutoff = 7_000.0f32.min(rate as f32 / 2.0);
        let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff / rate as f32).exp();
        Self { step: rate as f64 / RATE as f64, pos: 0.0, alpha, smooth: 0.0, buf: Vec::new() }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &x in input {
            self.smooth += self.alpha * (x - self.smooth);
            self.buf.push(self.smooth);
        }
        while self.pos + 1.0 < self.buf.len() as f64 {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            out.push(self.buf[i] * (1.0 - frac) + self.buf[i + 1] * frac);
            self.pos += self.step;
        }
        let used = self.pos as usize;
        self.buf.drain(..used.min(self.buf.len()));
        self.pos -= used as f64;
    }
}

/// Energy-based voice activity detection: a segment starts when a frame is
/// clearly above the noise floor and ends at a pause, or at a length cap so
/// text keeps flowing during long monologues.
struct Chunker {
    who: Who,
    channels: usize,
    resampler: Resampler,
    pending: Vec<f32>,
    preroll: VecDeque<f32>,
    current: Option<(u64, Vec<f32>)>,
    speech_ms: u32,
    silence_ms: u32,
    noise: f32,
    started: Instant,
    out: Sender<Segment>,
    levels: Arc<Levels>,
}

impl Chunker {
    fn push<T>(&mut self, data: &[T])
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let mono: Vec<f32> = data
            .chunks(self.channels)
            .map(|frame| frame.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / frame.len() as f32)
            .collect();
        let mut resampled = Vec::with_capacity(mono.len() / 2 + 1);
        self.resampler.process(&mono, &mut resampled);
        self.pending.extend_from_slice(&resampled);
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            self.frame(&frame);
        }
    }

    fn frame(&mut self, frame: &[f32]) {
        let rms = (frame.iter().map(|x| x * x).sum::<f32>() / frame.len() as f32).sqrt();
        self.levels.set(self.who, rms);
        let speech = rms > (self.noise * 3.0).max(0.006);

        match &mut self.current {
            None => {
                if speech {
                    let mut samples: Vec<f32> = self.preroll.drain(..).collect();
                    let lead_ms = (samples.len() * 1000 / RATE) as u64;
                    samples.extend_from_slice(frame);
                    let now = self.started.elapsed().as_millis() as u64;
                    self.current = Some((now.saturating_sub(lead_ms), samples));
                    self.speech_ms = 30;
                    self.silence_ms = 0;
                } else {
                    // Track the background level only while nobody speaks.
                    self.noise = (self.noise * 0.97 + rms * 0.03).max(0.0005);
                    self.preroll.extend(frame.iter().copied());
                    while self.preroll.len() > PREROLL {
                        self.preroll.pop_front();
                    }
                }
            }
            Some((_, samples)) => {
                samples.extend_from_slice(frame);
                if speech {
                    self.speech_ms += 30;
                    self.silence_ms = 0;
                } else {
                    self.silence_ms += 30;
                }
                let secs = samples.len() / RATE;
                let pause = self.silence_ms >= 800 && secs >= 1;
                let long_with_breath = secs >= 12 && self.silence_ms >= 240;
                if pause || long_with_breath || secs >= 25 {
                    self.emit();
                }
            }
        }
    }

    fn emit(&mut self) {
        let Some((start_ms, mut samples)) = self.current.take() else { return };
        // Keep 200 ms of the trailing silence, drop the rest.
        let extra = (self.silence_ms.saturating_sub(200) as usize * RATE) / 1000;
        samples.truncate(samples.len().saturating_sub(extra));
        if self.speech_ms >= 400 {
            let _ = self.out.send(Segment { who: self.who, start_ms, samples });
        }
        self.speech_ms = 0;
        self.silence_ms = 0;
    }
}

impl Drop for Chunker {
    /// Stopping the recording drops the stream, which flushes the last words.
    fn drop(&mut self) {
        self.emit();
        self.levels.set(self.who, 0.0);
    }
}

fn build<T>(device: &cpal::Device, config: &StreamConfig, mut chunker: Chunker) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| chunker.push(data),
            |e| eprintln!("audio stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Opens a device as a capture stream. For an output device (loopback),
/// WASAPI records what is being played on it.
fn open(
    device: &cpal::Device,
    loopback: bool,
    who: Who,
    started: Instant,
    out: Sender<Segment>,
    levels: Arc<Levels>,
) -> Result<Stream, String> {
    let supported = if loopback { device.default_output_config() } else { device.default_input_config() }
        .map_err(|e| e.to_string())?;
    let config: StreamConfig = supported.config();
    let chunker = Chunker {
        who,
        channels: config.channels.max(1) as usize,
        resampler: Resampler::new(config.sample_rate.0),
        pending: Vec::new(),
        preroll: VecDeque::new(),
        current: None,
        speech_ms: 0,
        silence_ms: 0,
        noise: 0.002,
        started,
        out,
        levels,
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(device, &config, chunker),
        SampleFormat::I16 => build::<i16>(device, &config, chunker),
        SampleFormat::I32 => build::<i32>(device, &config, chunker),
        SampleFormat::U16 => build::<u16>(device, &config, chunker),
        other => Err(format!("unsupported sample format {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

/// A running recording. cpal streams must stay on the thread that made them,
/// so they live on their own thread until `stop`.
pub struct Recorder {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Sources that could not be opened (e.g. no microphone).
    pub warnings: Vec<String>,
}

impl Recorder {
    pub fn start(out: Sender<Segment>, levels: Arc<Levels>) -> Result<Recorder, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<String>, String>>();
        let stop_flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let started = Instant::now();
                let host = cpal::default_host();
                let mut streams = Vec::new();
                let mut warnings = Vec::new();
                for (who, loopback, device) in [
                    (Who::Me, false, host.default_input_device()),
                    (Who::Them, true, host.default_output_device()),
                ] {
                    let name = if loopback { "speakers" } else { "microphone" };
                    match device {
                        Some(d) => match open(&d, loopback, who, started, out.clone(), levels.clone()) {
                            Ok(s) => streams.push(s),
                            Err(e) => warnings.push(format!("{name}: {e}")),
                        },
                        None => warnings.push(format!("{name}: not found")),
                    }
                }
                drop(out); // only the streams' chunkers keep the channel open
                if streams.is_empty() {
                    let _ = ready_tx.send(Err(warnings.join("; ")));
                    return;
                }
                let _ = ready_tx.send(Ok(warnings));
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                drop(streams);
            })
            .map_err(|e| e.to_string())?;
        let warnings = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Recorder { stop, thread: Some(thread), warnings })
    }

    /// Stops both streams; their last segments are flushed before this returns.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunker(out: Sender<Segment>) -> Chunker {
        Chunker {
            who: Who::Me,
            channels: 1,
            resampler: Resampler::new(RATE as u32),
            pending: Vec::new(),
            preroll: VecDeque::new(),
            current: None,
            speech_ms: 0,
            silence_ms: 0,
            noise: 0.002,
            started: Instant::now(),
            out,
            levels: Arc::new(Levels::default()),
        }
    }

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize).map(|i| amp * (i as f32 * 0.07).sin()).collect()
    }

    #[test]
    fn cuts_speech_at_pauses_and_ignores_silence() {
        let (tx, rx) = mpsc::channel();
        let mut c = chunker(tx);
        c.push(&vec![0.0f32; RATE]); // 1 s silence: nothing
        c.push(&tone(2.0, 0.3)); // 2 s of "speech"
        c.push(&vec![0.0f32; RATE]); // pause ends it
        c.push(&tone(1.5, 0.3));
        drop(c); // stop flushes the second one
        let segments: Vec<Segment> = rx.iter().collect();
        assert_eq!(segments.len(), 2);
        assert!(segments[0].samples.len() >= 2 * RATE);
        assert!(segments[1].start_ms >= segments[0].start_ms);
    }

    #[test]
    fn long_speech_is_split() {
        let (tx, rx) = mpsc::channel();
        let mut c = chunker(tx);
        c.push(&tone(40.0, 0.3));
        drop(c);
        assert!(rx.iter().count() >= 2, "a 40 s monologue becomes several segments");
    }

    #[test]
    fn resamples_48k_to_16k() {
        let mut r = Resampler::new(48_000);
        let mut out = Vec::new();
        r.process(&vec![0.5f32; 48_000], &mut out);
        assert!((out.len() as i64 - 16_000).abs() <= 2);
    }
}
