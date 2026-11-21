# Contributing

Thanks for helping. Xnote aims to stay **light, simple and robust**: plain HTML/CSS/JS for the window,
Rust for everything else, no frontend framework and no build step for the UI.

1. Read the [development guide](docs/DEVELOPMENT.md) to build and run the app on Windows.
2. Open an issue first for anything larger than a small fix, so we can agree on the approach.
3. Keep the English, Portuguese and Spanish texts in `src/i18n.js` in step.
4. Before opening a pull request, run:

   ~~~powershell
   cargo fmt --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo test --manifest-path src-tauri/Cargo.toml --locked
   npm run test:ui
   ~~~

Never commit API keys, meeting files, the `Data` folder or signing keys. By contributing you agree
that your work is released under the [MIT License](LICENSE).
