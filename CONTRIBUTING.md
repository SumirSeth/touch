# Contributing to Touch

Thanks for helping! Touch is a small project, so the process is light.

## Reporting bugs

Open an [issue](https://github.com/SumirSeth/touch/issues/new/choose) with:

- your Windows version (`winver`) and laptop / touchpad model
- what you did, what you expected and what happened
- your `%APPDATA%\Touch\config.json` if the bug involves a specific binding

Touchpads report fingers differently from one model to another, so the hardware details matter.

## Suggesting gestures or actions

Open a feature request and describe the motion and what it should do. Gestures must not get in the way of normal
pointing, scrolling and clicking. Proposals that explain how a gesture is told apart from ordinary use are the
easiest to accept.

## Pull requests

1. Fork, create a branch, and keep the change focused.
2. Make sure these pass (CI runs the same):
   ```powershell
   cargo fmt --check
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
3. Gesture recognisers in `crates/engine/src/gestures.rs` are pure (frames in, events out). Please add tests in
   `crates/engine/tests/` that feed synthetic frames for new or changed gestures.
4. Try the change on a real touchpad if you can, and say which one in the PR.
5. For UI changes, include a screenshot in light and dark themes.

Design notes are in [`docs/app-spec.md`](docs/app-spec.md). The engine API in that document is what the app depends
on. Keep the two in sync when you change either.

## Releases

Maintainers bump the version in the workspace `Cargo.toml`, update `CHANGELOG.md`, and push a `vX.Y.Z` tag. The
release workflow builds the installers into a draft GitHub release, which is published after a quick check.

By contributing you agree that your work is licensed under the [MIT License](LICENSE).
