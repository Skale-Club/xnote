//! Request-scoped, cancellable recording preparation. A cancellation that
//! arrives before the blocking worker starts is remembered, and a cancelled
//! worker can never install a recording session.
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
struct Attempt {
    request_id: String,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
struct ActiveState {
    attempt: Option<Attempt>,
    early_cancel: VecDeque<String>,
    finished: VecDeque<String>,
}
#[derive(Default)]
pub struct Coordinator {
    state: Mutex<ActiveState>,
}
pub struct Guard<'a> {
    owner: &'a Coordinator,
    request_id: String,
    cancelled: Arc<AtomicBool>,
}
fn remember(queue: &mut VecDeque<String>, id: &str) {
    if !queue.iter().any(|s| s == id) {
        queue.push_back(id.into());
    }
    while queue.len() > 64 {
        queue.pop_front();
    }
}
impl Coordinator {
    pub fn begin(&self, request_id: &str) -> Result<Guard<'_>, String> {
        if request_id.is_empty() || request_id.len() > 128 {
            return Err("startup_invalid_request".into());
        }
        let mut state = self.state.lock().map_err(|_| "startup_state_unavailable")?;
        if state.attempt.is_some() {
            return Err("startup_busy".into());
        }
        if let Some(index) = state.early_cancel.iter().position(|s| s == request_id) {
            state.early_cancel.remove(index);
            remember(&mut state.finished, request_id);
            return Err("startup_cancelled".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        state.attempt = Some(Attempt {
            request_id: request_id.into(),
            cancelled: cancelled.clone(),
        });
        Ok(Guard {
            owner: self,
            request_id: request_id.into(),
            cancelled,
        })
    }
    pub fn cancel(&self, request_id: &str) -> bool {
        if request_id.is_empty() || request_id.len() > 128 {
            return false;
        }
        if let Ok(mut state) = self.state.lock() {
            if let Some(attempt) = state.attempt.as_ref() {
                if attempt.request_id != request_id {
                    return false;
                }
                attempt.cancelled.store(true, Ordering::Release);
                return true;
            }
            if state.finished.iter().any(|s| s == request_id) {
                return false;
            }
            remember(&mut state.early_cancel, request_id);
            return true;
        }
        false
    }
    pub fn cancel_current(&self) {
        if let Ok(state) = self.state.lock() {
            if let Some(a) = state.attempt.as_ref() {
                a.cancelled.store(true, Ordering::Release);
            }
        }
    }
}
impl Guard<'_> {
    pub fn flag(&self) -> &AtomicBool {
        &self.cancelled
    }
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("startup_cancelled".into())
        } else {
            Ok(())
        }
    }
    pub fn commit(&self, install: impl FnOnce()) -> Result<(), String> {
        let mut state = self
            .owner
            .state
            .lock()
            .map_err(|_| "startup_state_unavailable")?;
        self.check()?;
        if !state
            .attempt
            .as_ref()
            .is_some_and(|a| Arc::ptr_eq(&a.cancelled, &self.cancelled))
        {
            return Err("startup_cancelled".into());
        }
        install();
        state.attempt = None;
        remember(&mut state.finished, &self.request_id);
        Ok(())
    }
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.state.lock() {
            if state
                .attempt
                .as_ref()
                .is_some_and(|a| Arc::ptr_eq(&a.cancelled, &self.cancelled))
            {
                state.attempt = None;
                remember(&mut state.finished, &self.request_id);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_attempt_can_prepare() {
        let c = Coordinator::default();
        let _first = c.begin("one").unwrap();
        assert!(matches!(c.begin("two"),Err(e)if e=="startup_busy"));
    }
    #[test]
    fn cancellation_prevents_session_creation_and_allows_retry() {
        let c = Coordinator::default();
        let first = c.begin("one").unwrap();
        assert!(!c.cancel("unrelated"));
        assert!(c.cancel("one"));
        let mut installed = false;
        assert_eq!(
            first.commit(|| installed = true).unwrap_err(),
            "startup_cancelled"
        );
        assert!(!installed);
        drop(first);
        assert!(c.begin("retry").is_ok());
    }
    #[test]
    fn successful_commit_rejects_late_cancellation() {
        let c = Coordinator::default();
        let first = c.begin("one").unwrap();
        let mut installed = false;
        first.commit(|| installed = true).unwrap();
        assert!(installed);
        assert!(!c.cancel("one"));
    }
    #[test]
    fn dropping_an_old_guard_cannot_clear_a_new_attempt() {
        let c = Coordinator::default();
        let old = c.begin("same").unwrap();
        old.commit(|| {}).unwrap();
        let next = c.begin("same").unwrap();
        drop(old);
        assert!(c.cancel("same"));
        assert!(next.check().is_err());
    }
    #[test]
    fn errors_release_the_attempt_and_exit_can_cancel_it() {
        let c = Coordinator::default();
        {
            let guard = c.begin("error").unwrap();
            c.cancel_current();
            assert!(guard.check().is_err());
        }
        assert!(c.begin("retry").is_ok());
    }
    #[test]
    fn cancellation_before_worker_start_is_not_lost() {
        let c = Coordinator::default();
        assert!(c.cancel("early"));
        assert!(matches!(c.begin("early"),Err(e)if e=="startup_cancelled"));
        assert!(c.begin("next").is_ok());
    }
    #[test]
    fn cancellation_history_is_bounded() {
        let c = Coordinator::default();
        for i in 0..200 {
            c.cancel(&i.to_string());
        }
        assert_eq!(c.state.lock().unwrap().early_cancel.len(), 64);
    }
}
