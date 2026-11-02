# Touch

Custom touchpad gestures for Windows 11. Touch runs in the tray and lets you choose what each gesture does: slide
along an edge to change volume, brightness or seek a video, hold a corner to play/pause, swipe with three or four
fingers to switch desktops, pinch with five fingers for Task View, or bind any gesture to a shortcut, an app or a URL.

Pointer movement, scrolling and clicks stay Windows' own; Touch only adds gestures on top.

## Gestures

| Gesture | Default |
|---|---|
| Slide along the right / top / bottom / left edge (one finger) | brightness / volume / seek (arrow keys) / unassigned |
| Hold still in a corner (one finger) | top-left play/pause, top-right mute |
| 3- and 4-finger swipes (up, down, left, right) and taps | unassigned |
| 4-finger twist | volume knob |
| 5-finger pinch in/out | Task View |
| 5-finger twist clockwise / anticlockwise | reopen tab / close tab |

Edge slides and the 4-finger twist can drive volume, brightness, seek, horizontal scroll, zoom, tabs, virtual
desktops or your own pair of shortcuts, with adjustable sensitivity. The other gestures can run a preset (Task View,
show desktop, snap, tabs, media, screenshot, lock...), any key combination, an app or a URL. There is also an optional
3-finger drag.

**If you rebind a 3- or 4-finger gesture, set Windows' own gesture for that finger count to "Nothing"**
(Settings > Bluetooth & devices > Touchpad), otherwise both run. Touch shows a warning with a shortcut to that page.

## Install

Download `touch_<version>_x64-setup.exe` from [Releases](https://github.com/SumirSeth/touch/releases) and run it
(per-user install, no administrator needed; an `.msi` is also provided). The installers aren't code-signed yet, so
SmartScreen may warn: choose **More info > Run anyway**.

Requires Windows 10 1809 or later with a Precision Touchpad (most laptops from the last several years).

Touch starts in the tray. Click the icon for settings; right-click for Pause, Start with Windows and Quit. Settings
save immediately to `%APPDATA%\Touch\config.json`.

## Build from source

Needs x64 Windows, stable Rust (MSVC), and Visual Studio Build Tools with the C++ workload and a Windows SDK.

```powershell
cargo run -p touch                 # run
cargo test --workspace             # tests (gesture recognisers run on synthetic frames, no hardware needed)
powershell -ExecutionPolicy Bypass -File app/scripts/package.ps1   # NSIS + MSI installers in target/installers/
```

Layout: `crates/engine` reads the touchpad through Raw Input, recognises gestures and runs actions; `app` is the tray
and the GPUI settings window. GPUI 0.2.2 is vendored in `app/vendor/gpui` with two small patches (see `PATCH.md`
there). `docs/app-spec.md` describes the design. `trackpad/` holds the original C# prototype and research notes.

Releases: push a `vX.Y.Z` tag; GitHub Actions builds the installers into a draft release.

## License

MIT
