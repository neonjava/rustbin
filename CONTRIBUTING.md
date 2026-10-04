# Contributing

Fork and clone the repository, create a branch such as `feat/...`, `fix/...`, `docs/...`, `refactor/...`, or `test/...`, and follow the README local setup. Use Application Default Credentials or the Firestore emulator; never commit credentials or paste content. Keep changes focused and add tests for behavior changes.

Before opening a pull request, run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `cargo build --release`. Commit your work, push your branch, and open a PR describing the reason, tests, and screenshots for UI changes.
