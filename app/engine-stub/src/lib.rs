//! Temporary contract implementation. No hardware or action injection.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread::JoinHandle,
};
pub const APP_NAME: &str = "TrackpadGestures";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub enabled: bool,
    pub freeze_cursor: bool,
    pub edge_width_mm: f64,
    pub swipe_distance_mm: f64,
    pub drag3: bool,
    pub bindings: BTreeMap<String, Action>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    None,
    Preset {
        name: String,
    },
    Keys {
        keys: String,
    },
    Launch {
        path: String,
        args: Option<String>,
    },
    Url {
        url: String,
    },
    Adjust {
        #[serde(flatten)]
        target: AdjustTarget,
        step: f64,
        reverse: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum AdjustTarget {
    Volume,
    Brightness,
    Seek,
    ScrollH,
    Zoom,
    Tabs,
    Desktops,
    Custom { increase: String, decrease: String },
}
impl Default for Config {
    fn default() -> Self {
        let mut bindings = BTreeMap::new();
        for (id, target, step) in [
            ("edge_right", AdjustTarget::Brightness, 0.67),
            ("edge_top", AdjustTarget::Volume, 4.),
            ("edge_bottom", AdjustTarget::Seek, 3.),
            ("twist4", AdjustTarget::Volume, 12.),
        ] {
            bindings.insert(
                id.into(),
                Action::Adjust {
                    target,
                    step,
                    reverse: false,
                },
            );
        }
        for (id, name) in [
            ("corner_top_left", "play_pause"),
            ("corner_top_right", "mute"),
            ("pinch5_in", "task_view"),
            ("pinch5_out", "task_view"),
            ("twist5_cw", "reopen_tab"),
            ("twist5_ccw", "close_tab"),
        ] {
            bindings.insert(id.into(), Action::Preset { name: name.into() });
        }
        Self {
            version: 1,
            enabled: true,
            freeze_cursor: true,
            edge_width_mm: 6.,
            swipe_distance_mm: 12.,
            drag3: false,
            bindings,
        }
    }
}
pub fn config_path() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default())
        .join(APP_NAME)
        .join("config.json")
}
pub fn load_config() -> (Config, Option<String>) {
    let path = config_path();
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
            Ok(c) if validate(&c).is_empty() => (c, None),
            other => {
                let reason = other
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Invalid configuration".into());
                let backup = path.with_extension("json.bad");
                let result = std::fs::rename(&path, &backup);
                (
                    Config::default(),
                    Some(format!(
                        "{reason}. Backup: {} ({result:?})",
                        backup.display()
                    )),
                )
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Config::default(), None),
        Err(e) => (Config::default(), Some(e.to_string())),
    }
}
pub fn save_config(c: &Config) -> io::Result<()> {
    let errors = validate(c);
    if !errors.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            errors.join("; "),
        ));
    }
    let path = config_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(c)?)?;
        f.sync_all()?;
    }
    std::fs::rename(tmp, path)
}
fn chord(s: &str) -> bool {
    let parts: Vec<_> = s.split('+').collect();
    if parts.is_empty() {
        return false;
    }
    let key = parts.last().unwrap();
    let valid = key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric())
        || key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n))
        || [
            "Left",
            "Right",
            "Up",
            "Down",
            "Tab",
            "Enter",
            "Esc",
            "Space",
            "Backspace",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "PlayPause",
            "NextTrack",
            "PrevTrack",
            "Mute",
            "VolumeUp",
            "VolumeDown",
        ]
        .contains(key);
    valid
        && parts[..parts.len() - 1]
            .iter()
            .all(|p| ["Ctrl", "Alt", "Shift", "Win"].contains(p))
}
pub fn validate(c: &Config) -> Vec<String> {
    let mut e = Vec::new();
    if c.version != 1 {
        e.push("Unsupported config version".into());
    }
    for (name, v) in [
        ("Edge width", c.edge_width_mm),
        ("Swipe distance", c.swipe_distance_mm),
    ] {
        if !v.is_finite() || v <= 0. {
            e.push(format!("{name} must be positive"));
        }
    }
    for (id, a) in &c.bindings {
        let Some(t) = triggers().iter().find(|t| t.id == id) else {
            e.push(format!("Unknown trigger: {id}"));
            continue;
        };
        match a {
            Action::None => {}
            Action::Adjust { target, step, .. } => {
                if !t.continuous || !step.is_finite() || *step <= 0. {
                    e.push(format!("{id}: invalid adjust action or step"));
                }
                if let AdjustTarget::Custom { increase, decrease } = target
                    && (!chord(increase) || !chord(decrease))
                {
                    e.push(format!("{id}: invalid custom chord"));
                }
            }
            _ if t.continuous => e.push(format!("{id}: requires adjust")),
            Action::Keys { keys } if !chord(keys) => e.push(format!("{id}: invalid chord")),
            Action::Preset { name } if !presets().iter().any(|p| p.name == name) => {
                e.push(format!("{id}: unknown preset"))
            }
            Action::Launch { path, .. } if path.trim().is_empty() => {
                e.push(format!("{id}: choose a path"))
            }
            Action::Url { url } if !(url.starts_with("https://") || url.starts_with("http://")) => {
                e.push(format!("{id}: URL must start with http:// or https://"))
            }
            _ => {}
        }
    }
    e
}
pub struct TriggerInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub continuous: bool,
    pub description: &'static str,
    pub fingers: u8,
}
pub struct PresetInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub group: &'static str,
}
pub struct AdjustInfo {
    pub target: &'static str,
    pub label: &'static str,
    pub default_step: f64,
    pub unit: &'static str,
}
pub struct Conflict {
    pub trigger_prefix: &'static str,
    pub message: String,
}
pub fn windows_conflicts(c: &Config) -> Vec<Conflict> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad");
    let mut out = Vec::new();
    for (prefix, value) in [
        ("swipe3", "ThreeFingerSlideEnabled"),
        ("tap3", "ThreeFingerTapEnabled"),
        ("swipe4", "FourFingerSlideEnabled"),
        ("tap4", "FourFingerTapEnabled"),
    ] {
        let enabled = key
            .as_ref()
            .ok()
            .and_then(|k| k.get_value::<u32, _>(value).ok())
            .unwrap_or(1)
            != 0;
        if enabled
            && c.bindings
                .iter()
                .any(|(id, a)| id.starts_with(prefix) && !matches!(a, Action::None))
        {
            out.push(Conflict {
                trigger_prefix: prefix,
                message: format!("Windows also handles {prefix}. Set its gesture to Nothing."),
            });
        }
    }
    out
}
pub enum EngineEvent {
    Touchpad { width_mm: f64, height_mm: f64 },
    NoTouchpad,
    Fired { trigger: String },
    Adjusted { trigger: String, steps: i32 },
    Error(String),
}
pub struct Engine {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Engine {
    pub fn start(config: Config, events: Option<Sender<EngineEvent>>) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(!config.enabled));
        let (s, p) = (stop.clone(), paused.clone());
        let thread = std::thread::spawn(move || {
            if let Some(tx) = events {
                let _ = tx.send(EngineEvent::NoTouchpad);
                let mut tick = 0;
                while !s.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    tick += 1;
                    if cfg!(debug_assertions) && tick % 70 == 0 && !p.load(Ordering::Relaxed) {
                        let _ = tx.send(EngineEvent::Fired {
                            trigger: "edge_right".into(),
                        });
                    }
                }
            }
        });
        Ok(Self {
            stop,
            paused,
            thread: Some(thread),
        })
    }
    pub fn set_config(&self, config: Config) {
        self.set_paused(!config.enabled);
    }
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }
    pub fn run_action(&self, _action: &Action) {}
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn triggers() -> &'static [TriggerInfo] {
    &[
        TriggerInfo {
            id: "edge_left",
            label: "Left edge",
            group: "Edges",
            continuous: true,
            fingers: 1,
            description: "Slide along the edge to adjust.",
        },
        TriggerInfo {
            id: "edge_right",
            label: "Right edge",
            group: "Edges",
            continuous: true,
            fingers: 1,
            description: "Slide along the edge to adjust.",
        },
        TriggerInfo {
            id: "edge_top",
            label: "Top edge",
            group: "Edges",
            continuous: true,
            fingers: 1,
            description: "Slide along the edge to adjust.",
        },
        TriggerInfo {
            id: "edge_bottom",
            label: "Bottom edge",
            group: "Edges",
            continuous: true,
            fingers: 1,
            description: "Slide along the edge to adjust.",
        },
        TriggerInfo {
            id: "corner_top_left",
            label: "Top Left",
            group: "Corners",
            continuous: false,
            fingers: 1,
            description: "Hold in the corner for 450 ms.",
        },
        TriggerInfo {
            id: "corner_top_right",
            label: "Top Right",
            group: "Corners",
            continuous: false,
            fingers: 1,
            description: "Hold in the corner for 450 ms.",
        },
        TriggerInfo {
            id: "corner_bottom_left",
            label: "Bottom Left",
            group: "Corners",
            continuous: false,
            fingers: 1,
            description: "Hold in the corner for 450 ms.",
        },
        TriggerInfo {
            id: "corner_bottom_right",
            label: "Bottom Right",
            group: "Corners",
            continuous: false,
            fingers: 1,
            description: "Hold in the corner for 450 ms.",
        },
        TriggerInfo {
            id: "swipe3_up",
            label: "Swipe up",
            group: "3 fingers",
            continuous: false,
            fingers: 3,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "swipe3_down",
            label: "Swipe down",
            group: "3 fingers",
            continuous: false,
            fingers: 3,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "swipe3_left",
            label: "Swipe left",
            group: "3 fingers",
            continuous: false,
            fingers: 3,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "swipe3_right",
            label: "Swipe right",
            group: "3 fingers",
            continuous: false,
            fingers: 3,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "tap3",
            label: "Tap",
            group: "3 fingers",
            continuous: false,
            fingers: 3,
            description: "Tap and lift within 250 ms.",
        },
        TriggerInfo {
            id: "swipe4_up",
            label: "Swipe up",
            group: "4 fingers",
            continuous: false,
            fingers: 4,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "swipe4_down",
            label: "Swipe down",
            group: "4 fingers",
            continuous: false,
            fingers: 4,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "swipe4_left",
            label: "Swipe left",
            group: "4 fingers",
            continuous: false,
            fingers: 4,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "swipe4_right",
            label: "Swipe right",
            group: "4 fingers",
            continuous: false,
            fingers: 4,
            description: "Move together at least 12 mm.",
        },
        TriggerInfo {
            id: "tap4",
            label: "Tap",
            group: "4 fingers",
            continuous: false,
            fingers: 4,
            description: "Tap and lift within 250 ms.",
        },
        TriggerInfo {
            id: "twist4",
            label: "Rotate",
            group: "4 fingers",
            continuous: true,
            fingers: 4,
            description: "Rotate clockwise to increase.",
        },
        TriggerInfo {
            id: "pinch5_in",
            label: "Pinch in",
            group: "5 fingers",
            continuous: false,
            fingers: 5,
            description: "Move five fingers together.",
        },
        TriggerInfo {
            id: "pinch5_out",
            label: "Pinch out",
            group: "5 fingers",
            continuous: false,
            fingers: 5,
            description: "Move five fingers together.",
        },
        TriggerInfo {
            id: "twist5_cw",
            label: "Rotate clockwise",
            group: "5 fingers",
            continuous: false,
            fingers: 5,
            description: "Move five fingers together.",
        },
        TriggerInfo {
            id: "twist5_ccw",
            label: "Rotate anticlockwise",
            group: "5 fingers",
            continuous: false,
            fingers: 5,
            description: "Move five fingers together.",
        },
    ]
}
pub fn presets() -> &'static [PresetInfo] {
    &[
        PresetInfo {
            name: "task_view",
            label: "Task View",
            group: "Windows",
        },
        PresetInfo {
            name: "show_desktop",
            label: "Show Desktop",
            group: "Windows",
        },
        PresetInfo {
            name: "desktop_left",
            label: "Desktop Left",
            group: "Windows",
        },
        PresetInfo {
            name: "desktop_right",
            label: "Desktop Right",
            group: "Windows",
        },
        PresetInfo {
            name: "new_desktop",
            label: "New Desktop",
            group: "Windows",
        },
        PresetInfo {
            name: "alt_tab",
            label: "Alt Tab",
            group: "Windows",
        },
        PresetInfo {
            name: "close_window",
            label: "Close Window",
            group: "Windows",
        },
        PresetInfo {
            name: "minimize",
            label: "Minimize",
            group: "Windows",
        },
        PresetInfo {
            name: "maximize",
            label: "Maximize",
            group: "Windows",
        },
        PresetInfo {
            name: "snap_left",
            label: "Snap Left",
            group: "Windows",
        },
        PresetInfo {
            name: "snap_right",
            label: "Snap Right",
            group: "Windows",
        },
        PresetInfo {
            name: "search",
            label: "Search",
            group: "Windows",
        },
        PresetInfo {
            name: "notifications",
            label: "Notifications",
            group: "Windows",
        },
        PresetInfo {
            name: "quick_settings",
            label: "Quick Settings",
            group: "Windows",
        },
        PresetInfo {
            name: "screenshot",
            label: "Screenshot",
            group: "Windows",
        },
        PresetInfo {
            name: "lock",
            label: "Lock",
            group: "Windows",
        },
        PresetInfo {
            name: "file_explorer",
            label: "File Explorer",
            group: "Windows",
        },
        PresetInfo {
            name: "close_tab",
            label: "Close Tab",
            group: "Browser",
        },
        PresetInfo {
            name: "reopen_tab",
            label: "Reopen Tab",
            group: "Browser",
        },
        PresetInfo {
            name: "next_tab",
            label: "Next Tab",
            group: "Browser",
        },
        PresetInfo {
            name: "prev_tab",
            label: "Prev Tab",
            group: "Browser",
        },
        PresetInfo {
            name: "back",
            label: "Back",
            group: "Browser",
        },
        PresetInfo {
            name: "forward",
            label: "Forward",
            group: "Browser",
        },
        PresetInfo {
            name: "refresh",
            label: "Refresh",
            group: "Browser",
        },
        PresetInfo {
            name: "copy",
            label: "Copy",
            group: "Editing",
        },
        PresetInfo {
            name: "paste",
            label: "Paste",
            group: "Editing",
        },
        PresetInfo {
            name: "undo",
            label: "Undo",
            group: "Editing",
        },
        PresetInfo {
            name: "redo",
            label: "Redo",
            group: "Editing",
        },
        PresetInfo {
            name: "play_pause",
            label: "Play Pause",
            group: "Media",
        },
        PresetInfo {
            name: "next_track",
            label: "Next Track",
            group: "Media",
        },
        PresetInfo {
            name: "prev_track",
            label: "Prev Track",
            group: "Media",
        },
        PresetInfo {
            name: "mute",
            label: "Mute",
            group: "Media",
        },
        PresetInfo {
            name: "volume_up",
            label: "Volume Up",
            group: "Media",
        },
        PresetInfo {
            name: "volume_down",
            label: "Volume Down",
            group: "Media",
        },
    ]
}
pub fn adjust_targets() -> &'static [AdjustInfo] {
    &[
        AdjustInfo {
            target: "volume",
            label: "Volume",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "brightness",
            label: "Brightness",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "seek",
            label: "Seek",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "scroll_h",
            label: "Scroll H",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "zoom",
            label: "Zoom",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "tabs",
            label: "Tabs",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "desktops",
            label: "Desktops",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
        AdjustInfo {
            target: "custom",
            label: "Custom",
            default_step: 4.,
            unit: "mm (edges) / degrees (rotation)",
        },
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_match_contract() {
        let c = Config::default();
        assert!(validate(&c).is_empty());
        assert_eq!(c.bindings.len(), 10);
        assert_eq!(triggers().len(), 23);
        assert!(matches!(
            c.bindings["edge_right"],
            Action::Adjust { step: 0.67, .. }
        ));
    }
    #[test]
    fn json_contract() {
        let a = Action::Adjust {
            target: AdjustTarget::Custom {
                increase: "Win+Right".into(),
                decrease: "Win+Left".into(),
            },
            step: 3.,
            reverse: true,
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["target"], "custom");
        assert_eq!(v["increase"], "Win+Right");
        assert!(serde_json::from_value::<Action>(v).is_ok());
    }
    #[test]
    fn rejects_wrong_kind_and_chords() {
        let mut c = Config::default();
        c.bindings.insert(
            "edge_left".into(),
            Action::Keys {
                keys: "Ctrl+MadeUp".into(),
            },
        );
        assert!(!validate(&c).is_empty());
        assert!(!chord("Ctrl+MadeUp"));
        assert!(chord("Win+Alt+F24"));
    }
}
