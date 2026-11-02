# Trackpad Gestures: release app spec (v1)

A small Windows 11 tray app that adds custom Precision Touchpad gestures and lets the user choose what each gesture
does. It works alongside Windows' own touchpad handling (pointer, scrolling and clicks stay Windows'). No animation or
overlay work: `trackpad/overlay`, `VirtualTouchpad.cs`, `OverlayLink.cs` and the 3-finger replay are not part of the
release.

The C# prototype in `trackpad/gestures/` is the reference implementation: raw-input decoding (`Touchpad.cs`),
recognisers (`Gestures.cs`), brightness through WMI (`Brightness.cs`), key injection (`Native.cs`), tests
(`test.ps1`). Background on why things are the way they are: `trackpad/research/`. Rule from the user: no third-party
mods or hooks (no Windhawk etc.). Ordinary crates are fine.

Working product name: **Trackpad Gestures** (may change before release; keep it in one place per file type:
`tauri.conf.json` `productName`, and the `APP_NAME` const in the engine).

## Stack and layout

Rust, Tauri v2 for the settings window and tray. The UI is plain HTML/CSS/JS (ES modules, no framework, no npm build
step) served from `app/ui/`. Bundled with `cargo tauri build` (NSIS installer + MSI).

```
Cargo.toml                 workspace: crates/engine, app/src-tauri
crates/engine/             library crate `gestures_engine`: raw input, recognisers, actions, config. No UI, no Tauri.
app/src-tauri/             Tauri app: tray, settings window, autostart, glue to the engine
app/ui/                    settings UI (index.html, *.js, *.css)
.github/workflows/         CI (build + test) and release (tag -> installer artifacts on a draft GitHub release)
```

Target: x86_64-pc-windows-msvc, Windows 10 1809+ (tested on Windows 11). Rust stable (installed: 1.99).

## Process model

One process. Tauri owns the main thread. The engine runs on its own thread with a hidden (not message-only) window
that registers Raw Input for usage page 0x0D / usage 0x05 with `RIDEV_INPUTSINK`, and its own message loop. Actions
run on a separate worker thread so slow ones (WMI brightness, launching apps) never stall input. The settings window
is created on demand and destroyed when closed (so WebView2 isn't resident while idle); the app keeps running in the
tray. Single instance: a second launch opens the settings window of the running instance.

## Gestures (triggers)

Two kinds: **discrete** triggers fire once per touch; **continuous** triggers emit signed steps while the finger
moves (an "adjust" action turns steps into repeated increase/decrease).

| id | kind | recogniser notes |
|---|---|---|
| `swipe3_up` `swipe3_down` `swipe3_left` `swipe3_right` | discrete | 3 fingers, centroid travels >= 12 mm (configurable `swipe_distance_mm`) with the dominant axis > 1.5x the other; fires once at the threshold. Finger count change before firing cancels. |
| `swipe4_up` `swipe4_down` `swipe4_left` `swipe4_right` | discrete | same with 4 fingers |
| `tap3` `tap4` | discrete | all fingers down within 150 ms, touch < 250 ms, movement < 2 mm, then lift |
| `pinch5_in` `pinch5_out` | discrete | from `FiveFingerGestures` (mean radius change) |
| `twist5_cw` `twist5_ccw` | discrete | from `FiveFingerGestures` |
| `corner_top_left` `corner_top_right` `corner_bottom_left` `corner_bottom_right` | discrete | 1 finger held still 450 ms in a 12 mm corner (`CornerHolds`) |
| `edge_left` `edge_right` | continuous | 1 finger lands within `edge_width_mm` (6) of the edge, slides vertically; + = up |
| `edge_top` `edge_bottom` | continuous | same, horizontal; + = right |
| `twist4` | continuous | 4-finger rotation (`FourFingerKnob`); + = clockwise, one step per 12 degrees |

Rules carried over from the prototype: a second finger, moving into the pad, or pressing the pad (button usage page
0x09 in the report) cancels an edge slide; a 4-finger swipe that starts as a translation is left alone by the knob;
only one recogniser owns a touch once it fires. While an edge slide is active the cursor is frozen with `ClipCursor`
(setting `freeze_cursor`, default on) and released at the end.

`drag3` is a mode, not a trigger: 3-finger drag (left button held while 3 fingers move, `ThreeFingerDrag`). When on,
`swipe3_*` and `tap3` are disabled.

Unbound triggers cost nothing and don't claim touches. Windows still sees every touch; for 3/4-finger swipes and taps
the user must set Windows' own gesture for that finger count to "Nothing" or both will run. The engine reports
conflicts from `HKCU\Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad` (values present on the dev machine:
`ThreeFingerSlideEnabled`, `ThreeFingerTapEnabled`, `FourFingerTapEnabled`; `FourFingerSlideEnabled` may be absent,
meaning default = on). The UI shows a warning with a button that opens `ms-settings:devices-touchpad`. We never write
those settings ourselves.

## Actions

Discrete actions (JSON `"type"`):

- `none`
- `preset` with `"name"`: `task_view` (Win+Tab), `show_desktop` (Win+D), `desktop_left` / `desktop_right`
  (Win+Ctrl+Left/Right), `new_desktop` (Win+Ctrl+D), `alt_tab`, `close_window` (Alt+F4), `minimize` (Win+Down),
  `maximize` (Win+Up), `snap_left` / `snap_right` (Win+Left/Right), `close_tab` (Ctrl+W), `reopen_tab`
  (Ctrl+Shift+T), `next_tab` (Ctrl+Tab), `prev_tab` (Ctrl+Shift+Tab), `back` (Alt+Left), `forward` (Alt+Right),
  `refresh` (F5), `copy`, `paste`, `undo`, `redo`, `search` (Win+S), `notifications` (Win+N), `quick_settings`
  (Win+A), `screenshot` (Win+Shift+S), `lock` (Win+L via `LockWorkStation`), `file_explorer` (Win+E), `play_pause`,
  `next_track`, `prev_track`, `mute`, `volume_up`, `volume_down`
- `keys` with `"keys"`: a chord like `"Ctrl+Shift+T"`, `"Win+Alt+R"`, `"F11"`. Modifiers Ctrl, Alt, Shift, Win;
  keys A-Z, 0-9, F1-F24, arrows, Tab, Enter, Esc, Space, Backspace, Delete, Home, End, PageUp, PageDown, media keys
  by name. Parse errors are reported, not guessed.
- `launch` with `"path"` and optional `"args"` (ShellExecute, so documents and `ms-settings:` URIs work)
- `url` with `"url"` (ShellExecute)

Continuous actions: `adjust` with `"target"`, `"step"` (mm per step for edges, degrees per step for twist4) and
`"reverse"` (bool):

- `volume` (volume up/down media keys, shows Windows' OSD), `brightness` (WMI, coalesced as in `Brightness.cs`,
  1.5 % per mm by default: one step = 1 %), `seek` (Right/Left arrows), `scroll_h` (horizontal wheel),
  `zoom` (Ctrl+= / Ctrl+-), `tabs` (next/prev tab), `desktops` (Win+Ctrl+Right/Left), `custom` with
  `"increase"` and `"decrease"` chords.

A discrete action bound to a continuous trigger isn't allowed (the UI only offers adjust actions there) and vice
versa.

## Config

`%APPDATA%\TrackpadGestures\config.json`, written atomically (temp file + rename). Missing file = defaults. Unknown
fields are ignored; a file that fails to parse is renamed to `config.json.bad` and defaults are used (and reported).

```json
{
  "version": 1,
  "enabled": true,
  "freeze_cursor": true,
  "edge_width_mm": 6,
  "swipe_distance_mm": 12,
  "drag3": false,
  "bindings": {
    "edge_right":       { "type": "adjust", "target": "brightness", "step": 0.67, "reverse": false },
    "edge_top":         { "type": "adjust", "target": "volume", "step": 4, "reverse": false },
    "edge_bottom":      { "type": "adjust", "target": "seek", "step": 3, "reverse": false },
    "twist4":           { "type": "adjust", "target": "volume", "step": 12, "reverse": false },
    "corner_top_left":  { "type": "preset", "name": "play_pause" },
    "corner_top_right": { "type": "preset", "name": "mute" },
    "pinch5_in":        { "type": "preset", "name": "task_view" },
    "pinch5_out":       { "type": "preset", "name": "task_view" },
    "twist5_cw":        { "type": "preset", "name": "reopen_tab" },
    "twist5_ccw":       { "type": "preset", "name": "close_tab" }
  }
}
```

The above is exactly `default_config()` (today's prototype behaviour). Triggers missing from `bindings` are unbound.
Start-with-Windows is not in this file: it's the `HKCU\...\Run` value, owned by the app.

## Engine API (crate `gestures_engine`)

The contract between the two halves. Types are serde (de)serialisable with the JSON shapes above
(`#[serde(tag = "type", rename_all = "snake_case")]` for actions).

```rust
pub const APP_NAME: &str = "TrackpadGestures";

pub struct Config { pub version: u32, pub enabled: bool, pub freeze_cursor: bool, pub edge_width_mm: f64,
                    pub swipe_distance_mm: f64, pub drag3: bool, pub bindings: BTreeMap<String, Action> }
pub enum Action { None, Preset { name: String }, Keys { keys: String }, Launch { path: String, args: Option<String> },
                  Url { url: String }, Adjust { target: AdjustTarget, step: f64, reverse: bool } }
pub enum AdjustTarget { Volume, Brightness, Seek, ScrollH, Zoom, Tabs, Desktops,
                        Custom { increase: String, decrease: String } }   // serde: "target": "custom", plus fields

impl Default for Config                       // = default_config()
pub fn config_path() -> PathBuf
pub fn load_config() -> (Config, Option<String>)   // warning text if the file was bad
pub fn save_config(c: &Config) -> io::Result<()>
pub fn validate(c: &Config) -> Vec<String>    // human-readable problems (bad chord, wrong action kind...)

// Catalogue for the UI (single source of truth for ids, labels, grouping, descriptions)
pub struct TriggerInfo { pub id: &'static str, pub label: &'static str, pub group: &'static str,
                         pub continuous: bool, pub description: &'static str, pub fingers: u8 }
pub struct PresetInfo  { pub name: &'static str, pub label: &'static str, pub group: &'static str }
pub struct AdjustInfo  { pub target: &'static str, pub label: &'static str, pub default_step: f64, pub unit: &'static str }
pub fn triggers() -> &'static [TriggerInfo]
pub fn presets() -> &'static [PresetInfo]
pub fn adjust_targets() -> &'static [AdjustInfo]

pub struct Conflict { pub trigger_prefix: &'static str, pub message: String }  // e.g. "swipe3", "Windows' 3-finger swipes are on"
pub fn windows_conflicts(c: &Config) -> Vec<Conflict>

pub enum EngineEvent { Touchpad { width_mm: f64, height_mm: f64 }, NoTouchpad,
                       Fired { trigger: String }, Adjusted { trigger: String, steps: i32 }, Error(String) }

pub struct Engine;
impl Engine {
    pub fn start(config: Config, events: Option<std::sync::mpsc::Sender<EngineEvent>>) -> io::Result<Engine>;
    pub fn set_config(&self, config: Config);  // takes effect at the next touch
    pub fn set_paused(&self, paused: bool);
    pub fn run_action(&self, action: &Action); // "Test" button in the UI (discrete actions only)
    pub fn stop(self);
}
```

Recognisers are pure (frames in, events out) and unit-tested without hardware, like `test.ps1`. All `test.ps1`
cases that apply carry over as Rust tests.

## Settings UI

One window (~900x640, resizable, follows the Windows light/dark theme, Mica if easy):

- Left: gesture list grouped (Edges, Corners, 3 fingers, 4 fingers, 5 fingers), each row showing the trigger label
  and its current action, with a small diagram of the touchpad highlighting the zone/finger count.
- Right: editor for the selected trigger: action picker (presets grouped, Custom keys with a "press keys" recorder,
  Launch app with file picker, Open URL; or for continuous triggers the adjust target, step slider, reverse toggle),
  a Test button, and the conflict warning when relevant.
- Live indicator: when a gesture fires, its row flashes (from `EngineEvent`), which doubles as a way to learn
  gestures.
- General page: enable/disable all, 3-finger drag, freeze cursor, edge width, swipe distance, start with Windows,
  reset to defaults, open config folder, version + about.
- Changes save immediately (no Save button) and are pushed to the engine.

Tray: left-click opens settings; menu: Settings, Pause gestures (checkable), Start with Windows (checkable), Quit.

## Release

- `cargo tauri build` produces NSIS `.exe` and `.msi` in `target/release/bundle/`.
- Version from the workspace `Cargo.toml` / `tauri.conf.json` (start at 0.1.0).
- GitHub Actions: `ci.yml` (fmt check, clippy, tests, build on windows-latest), `release.yml` on `v*` tags builds the
  installers and uploads them to a **draft** release. Unsigned for now (SmartScreen will warn; signing is a later
  decision). No secrets required.
- README at the repo root: what it does, install, the "set Windows' gesture to Nothing" note, build from source.
