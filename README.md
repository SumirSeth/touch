# Trackpad Gestures

A Windows 11 tray app for custom Precision Touchpad gestures, with native GPU-rendered GPUI settings. Bind edge slides, corner holds, swipes, taps, pinches and rotations to shortcuts, apps, URLs, volume, brightness and more. Pointer movement, scrolling and clicks remain Windows' own.

**This branch uses a temporary engine stub.** Configuration and settings work; hardware recognition and action execution arrive when the engine branch is integrated. Debug builds flash the right-edge row periodically to demonstrate gesture feedback. The Test button calls the engine API; the stub deliberately performs no action.

## Install and use

Install the NSIS `.exe` from a release for a per-user installation (no administrator required). The `.msi` uses cargo-packager's default per-machine installation and may request elevation. Installers are unsigned. Start Trackpad Gestures, then use the tray icon to reopen settings. Closing settings destroys the window and leaves the tray running; Quit exits the process. A second launch opens or focuses the existing settings window.

**Set Windows' own 3/4-finger gesture to “Nothing” for each gesture you rebind**, otherwise both Windows and Trackpad Gestures handle it. The editor reports conflicts and links to Windows touchpad settings. Three-finger drag disables three-finger swipes and tap.

Settings save immediately to `%APPDATA%\TrackpadGestures\config.json`. Invalid input displays an error and is not pushed to the engine. Malformed configuration is backed up as `config.json.bad`. Start with Windows is stored separately in the current user's Run registry key and starts with `--background` (tray only).

## Build from source

Requires x64 Windows, stable Rust (tested with 1.99.1, MSVC target), Visual Studio Build Tools with C++ and a Windows SDK. GPUI uses DirectX 11; a functioning GPU driver is required.

```powershell
cargo run -p trackpad-gestures
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --release
```

Generate both installers with one command from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File app/scripts/package.ps1
```

The script generates the original PNG/ICO icon, installs cargo-packager 0.11.8 if missing, builds release, reads the workspace package version, then creates NSIS and MSI installers in `target/installers/`. cargo-packager downloads NSIS and WiX tooling automatically. Release executables use the Windows subsystem with no console. The `v*` tag workflow uploads installers to a draft GitHub release using GitHub's built-in token; it needs no separately configured secret or signing key.

## Implementation and engine integration

GPUI is pinned to **0.2.2** from crates.io; gpui-component is pinned to **0.5.1**, which depends on that GPUI release. Native component inputs, buttons, switches and slider provide keyboard focus and editing; diagrams are GPUI elements. No webview feature is enabled. Segoe UI Variable, Windows light/dark settings and the current accent colour style the window. Mica is not enabled.

GPUI owns the main thread and pumps the tray's native Win32 messages. The vendored GPUI 0.2.2 Windows backend has a one-expression patch in `close_one_window`: return false after removing the window, instead of requesting process quit when the list is empty. Upstream 0.2.2 otherwise posts WM_QUIT on the final close. A second build-script patch discovers installed SDK versions for fxc.exe instead of requiring SDK 26100. A foreground async task retains the tray and engine, drains tray events and checks a named auto-reset event; settings is recreated on demand. The instance mutex is `Local\TrackpadGestures.GPUI.v1.Instance`, distinct from the C# prototype's mutex. Only this application's instance is signalled.

`app/engine-stub` mirrors the API in `docs/app-spec.md`, including JSON serialization, defaults, validation, conflict detection and atomic save. To integrate the real engine, change `gestures_engine = { path = "engine-stub" }` in `app/Cargo.toml` to `gestures_engine = { path = "../crates/engine" }`, and update the workspace members. The app intentionally obtains catalogue data from the engine. The spec's custom adjust target is serialized as `"target":"custom","increase":"…","decrease":"…"` alongside `step` and `reverse`.

The editor lists grouped preset buttons instead of a dropdown. Use Tab/Shift+Tab to move through controls. The chord recorder uses GPUI's modifier state (including the Windows key); enable Include Win for combinations the OS consumes. Windows-reserved shortcuts cannot all be recorded directly.
