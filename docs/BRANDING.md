# Xnote branding

Xnote belongs to the Skale Club sibling app family: freestanding icy-white/cobalt-blue symbols on transparent backgrounds, with shared lighting and visual weight. Product icons have no navy tile or decorative outer blue rim.

- Xink: fountain-pen nib and ink stroke.
- Xpecs: processor and telemetry pulse.
- Xcare: maintenance brush and cleaning sweep.
- Xnote: note page and audio waveform.

The raster master is `icon-source.png` (1254 x 1254 RGBA, transparent outside the product symbol). It was generated with the built-in image generation tool on October 26, 2026. The original product artwork was edited with image_gen to remove the tile and outer rim while retaining the symbol identity. It is not a vector source.

Run `npm ci` when dependencies are missing, then `./make-icon.ps1` to regenerate the Tauri PNG/ICO assets and `src/logo.png`. The script checks the conversion command's exit code. Keep the master, generated assets and in-app mark in sync. Changes to executable icons appear in newly built binaries; Windows may retain cached icons for existing shortcuts.

The organization mark `src/skaleclub.png` is separate from the product icon and remains unchanged. App behavior is unchanged by this asset update. Regeneration must preserve transparency and small-size legibility.

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

## Transparent product symbols

The current direction is symbol-only: no navy square, surrounding frame or blue
decorative border. Blue bevels that belong to the object remain. Preserve the RGBA
alpha when exporting Windows PNG/ICO and PortableApps AppInfo artwork. Review the
symbols on both light and dark surfaces and at 16/24/32 px. Xpace joins the family with a folder/treemap symbol.
