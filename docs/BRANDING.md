# Xnote branding

Xnote is the meeting-notes app in the Skale Club Windows app family (skale.css v2). Its signal is
voice violet `#a77bff` and its mark is a pair of quote marks, every corner lightly rounded, in ink
`#04070d` on a rounded violet tile.

The source of truth is `skaleclub-apps/design/skale-css/marks/xnote.svg`; the family rules (tile,
glyph scale, signals) are in that folder's `README.md`. Never redraw the mark by hand here.

Run `./make-icon.ps1` to regenerate the Tauri PNG/ICO assets and `src/logo.png`. It dot-sources the
shared `render-mark.ps1` from `skaleclub-apps` (no npm needed): the glyph fills 64% of the tile on
desktop sizes and 74% at 24 px and below. `src-tauri/icons/icon.icns`, `android/` and `ios/` are not
used by the Windows build and still hold the previous symbol. The organization mark
`src/skaleclub.png` is separate and unchanged.

## Embedded Windows icon verification

`src-tauri/build.rs` explicitly tracks the ICO and window PNGs and selects
`icons/icon.ico` for the Windows resource compiler. Changing the artwork now
invalidates the cached Windows resource library, including in existing checkouts.

`npm run build` automatically runs `npm run verify:icon` after a successful build.
The verifier reads the executable's PE resource directory without executing it and
compares every embedded icon frame with the approved ICO: dimensions, bit depth,
length and SHA-256 of the image payload. A mismatch fails the build. The release
workflow uses this same npm build command, so it receives the verification too.

Do not diagnose an old Explorer icon as a Windows cache issue until this check
passes. An Explorer thumbnail is not evidence that the executable resources match.

## History

- 2026-11-20: the family mark replaced the earlier transparent note-and-waveform symbol at the
  owner's request, as part of the skale.css v2 rollout.
