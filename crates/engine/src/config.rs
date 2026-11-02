use crate::{actions::parse_chord, APP_NAME};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "WireAction", into = "WireAction")]
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
        target: AdjustTarget,
        step: f64,
        reverse: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireAction {
    None,
    Preset {
        name: String,
    },
    Keys {
        keys: String,
    },
    Launch {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<String>,
    },
    Url {
        url: String,
    },
    Adjust {
        target: String,
        step: f64,
        reverse: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        increase: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decrease: Option<String>,
    },
}
impl TryFrom<WireAction> for Action {
    type Error = String;
    fn try_from(a: WireAction) -> Result<Self, String> {
        Ok(match a {
            WireAction::None => Self::None,
            WireAction::Preset { name } => Self::Preset { name },
            WireAction::Keys { keys } => Self::Keys { keys },
            WireAction::Launch { path, args } => Self::Launch { path, args },
            WireAction::Url { url } => Self::Url { url },
            WireAction::Adjust {
                target,
                step,
                reverse,
                increase,
                decrease,
            } => Self::Adjust {
                target: match target.as_str() {
                    "volume" => AdjustTarget::Volume,
                    "brightness" => AdjustTarget::Brightness,
                    "seek" => AdjustTarget::Seek,
                    "scroll_h" => AdjustTarget::ScrollH,
                    "zoom" => AdjustTarget::Zoom,
                    "tabs" => AdjustTarget::Tabs,
                    "desktops" => AdjustTarget::Desktops,
                    "custom" => AdjustTarget::Custom {
                        increase: increase.ok_or("Missing increase chord")?,
                        decrease: decrease.ok_or("Missing decrease chord")?,
                    },
                    _ => return Err(format!("Unknown adjust target: {target}")),
                },
                step,
                reverse,
            },
        })
    }
}
impl From<Action> for WireAction {
    fn from(a: Action) -> Self {
        match a {
            Action::None => Self::None,
            Action::Preset { name } => Self::Preset { name },
            Action::Keys { keys } => Self::Keys { keys },
            Action::Launch { path, args } => Self::Launch { path, args },
            Action::Url { url } => Self::Url { url },
            Action::Adjust {
                target,
                step,
                reverse,
            } => {
                let (target, increase, decrease) = match target {
                    AdjustTarget::Custom { increase, decrease } => {
                        ("custom".into(), Some(increase), Some(decrease))
                    }
                    t => (
                        serde_json::to_value(t)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned(),
                        None,
                        None,
                    ),
                };
                Self::Adjust {
                    target,
                    step,
                    reverse,
                    increase,
                    decrease,
                }
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub continuous: bool,
    pub description: &'static str,
    pub fingers: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    pub name: &'static str,
    pub label: &'static str,
    pub group: &'static str,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustInfo {
    pub target: &'static str,
    pub label: &'static str,
    pub default_step: f64,
    pub unit: &'static str,
}
macro_rules! trigger {
    ($id:literal,$label:literal,$group:literal,$cont:literal,$desc:literal,$n:literal) => {
        TriggerInfo {
            id: $id,
            label: $label,
            group: $group,
            continuous: $cont,
            description: $desc,
            fingers: $n,
        }
    };
}
pub fn triggers() -> &'static [TriggerInfo] {
    &[
        trigger!(
            "edge_left",
            "Left edge",
            "Edges",
            true,
            "Slide vertically; up increases",
            1
        ),
        trigger!(
            "edge_right",
            "Right edge",
            "Edges",
            true,
            "Slide vertically; up increases",
            1
        ),
        trigger!(
            "edge_top",
            "Top edge",
            "Edges",
            true,
            "Slide horizontally; right increases",
            1
        ),
        trigger!(
            "edge_bottom",
            "Bottom edge",
            "Edges",
            true,
            "Slide horizontally; right increases",
            1
        ),
        trigger!(
            "corner_top_left",
            "Top left",
            "Corners",
            false,
            "Hold still for 450 ms",
            1
        ),
        trigger!(
            "corner_top_right",
            "Top right",
            "Corners",
            false,
            "Hold still for 450 ms",
            1
        ),
        trigger!(
            "corner_bottom_left",
            "Bottom left",
            "Corners",
            false,
            "Hold still for 450 ms",
            1
        ),
        trigger!(
            "corner_bottom_right",
            "Bottom right",
            "Corners",
            false,
            "Hold still for 450 ms",
            1
        ),
        trigger!(
            "swipe3_up",
            "Swipe up",
            "3 fingers",
            false,
            "Travel 12 mm with three fingers",
            3
        ),
        trigger!(
            "swipe3_down",
            "Swipe down",
            "3 fingers",
            false,
            "Travel 12 mm with three fingers",
            3
        ),
        trigger!(
            "swipe3_left",
            "Swipe left",
            "3 fingers",
            false,
            "Travel 12 mm with three fingers",
            3
        ),
        trigger!(
            "swipe3_right",
            "Swipe right",
            "3 fingers",
            false,
            "Travel 12 mm with three fingers",
            3
        ),
        trigger!(
            "tap3",
            "Tap",
            "3 fingers",
            false,
            "Lift within 250 ms without moving 2 mm",
            3
        ),
        trigger!(
            "swipe4_up",
            "Swipe up",
            "4 fingers",
            false,
            "Travel 12 mm with four fingers",
            4
        ),
        trigger!(
            "swipe4_down",
            "Swipe down",
            "4 fingers",
            false,
            "Travel 12 mm with four fingers",
            4
        ),
        trigger!(
            "swipe4_left",
            "Swipe left",
            "4 fingers",
            false,
            "Travel 12 mm with four fingers",
            4
        ),
        trigger!(
            "swipe4_right",
            "Swipe right",
            "4 fingers",
            false,
            "Travel 12 mm with four fingers",
            4
        ),
        trigger!(
            "tap4",
            "Tap",
            "4 fingers",
            false,
            "Lift within 250 ms without moving 2 mm",
            4
        ),
        trigger!(
            "twist4",
            "Twist knob",
            "4 fingers",
            true,
            "Rotate; clockwise increases",
            4
        ),
        trigger!(
            "pinch5_in",
            "Pinch in",
            "5 fingers",
            false,
            "Reduce mean radius by 31.5% and 7.2 mm",
            5
        ),
        trigger!(
            "pinch5_out",
            "Pinch out",
            "5 fingers",
            false,
            "Expand mean radius by 46% and 7.2 mm",
            5
        ),
        trigger!(
            "twist5_cw",
            "Twist clockwise",
            "5 fingers",
            false,
            "Rotate 25 degrees",
            5
        ),
        trigger!(
            "twist5_ccw",
            "Twist counterclockwise",
            "5 fingers",
            false,
            "Rotate 25 degrees",
            5
        ),
    ]
}
pub fn presets() -> &'static [PresetInfo] {
    PRESETS
}
macro_rules! preset { ($($name:literal,$label:literal,$group:literal;)* ) => {const PRESETS:&[PresetInfo]=&[$(PresetInfo{name:$name,label:$label,group:$group},)*];} }
preset! {
"task_view","Task view","Windows";"show_desktop","Show desktop","Windows";"desktop_left","Previous desktop","Windows";"desktop_right","Next desktop","Windows";"new_desktop","New desktop","Windows";
"alt_tab","Switch windows","Windows";"close_window","Close window","Windows";"minimize","Minimize","Windows";"maximize","Maximize","Windows";"snap_left","Snap left","Windows";"snap_right","Snap right","Windows";
"close_tab","Close tab","Tabs";"reopen_tab","Reopen tab","Tabs";"next_tab","Next tab","Tabs";"prev_tab","Previous tab","Tabs";
"back","Back","Navigation";"forward","Forward","Navigation";"refresh","Refresh","Navigation";
"copy","Copy","Editing";"paste","Paste","Editing";"undo","Undo","Editing";"redo","Redo","Editing";
"search","Search","Windows";"notifications","Notifications","Windows";"quick_settings","Quick settings","Windows";"screenshot","Screenshot","Windows";"lock","Lock","Windows";"file_explorer","File Explorer","Windows";
"play_pause","Play / pause","Media";"next_track","Next track","Media";"prev_track","Previous track","Media";"mute","Mute","Media";"volume_up","Volume up","Media";"volume_down","Volume down","Media";
}
pub fn adjust_targets() -> &'static [AdjustInfo] {
    &[
        AdjustInfo {
            target: "volume",
            label: "Volume",
            default_step: 4.,
            unit: "mm",
        },
        AdjustInfo {
            target: "brightness",
            label: "Brightness",
            default_step: 0.67,
            unit: "mm",
        },
        AdjustInfo {
            target: "seek",
            label: "Seek",
            default_step: 3.,
            unit: "mm",
        },
        AdjustInfo {
            target: "scroll_h",
            label: "Horizontal scroll",
            default_step: 3.,
            unit: "mm",
        },
        AdjustInfo {
            target: "zoom",
            label: "Zoom",
            default_step: 4.,
            unit: "mm",
        },
        AdjustInfo {
            target: "tabs",
            label: "Tabs",
            default_step: 10.,
            unit: "mm",
        },
        AdjustInfo {
            target: "desktops",
            label: "Desktops",
            default_step: 12.,
            unit: "mm",
        },
        AdjustInfo {
            target: "custom",
            label: "Custom keys",
            default_step: 4.,
            unit: "mm",
        },
    ]
}
pub fn default_config() -> Config {
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
    Config {
        version: 1,
        enabled: true,
        freeze_cursor: true,
        edge_width_mm: 6.,
        swipe_distance_mm: 12.,
        drag3: false,
        bindings,
    }
}
impl Default for Config {
    fn default() -> Self {
        default_config()
    }
}
pub fn validate(c: &Config) -> Vec<String> {
    let mut errors = vec![];
    if c.version != 1 {
        errors.push("Unsupported config version".into());
    }
    for (name, value) in [
        ("edge_width_mm", c.edge_width_mm),
        ("swipe_distance_mm", c.swipe_distance_mm),
    ] {
        if !value.is_finite() || value <= 0. {
            errors.push(format!("{name} must be positive and finite"));
        }
    }
    for (id, a) in &c.bindings {
        let Some(t) = triggers().iter().find(|t| t.id == id) else {
            errors.push(format!("Unknown trigger: {id}"));
            continue;
        };
        if !matches!(a, Action::None) && t.continuous != matches!(a, Action::Adjust { .. }) {
            errors.push(format!("{id}: wrong action kind"));
        }
        let mut chords = vec![];
        match a {
            Action::Keys { keys } => chords.push(keys),
            Action::Preset { name } => {
                if !presets().iter().any(|p| p.name == name) {
                    errors.push(format!("{id}: unknown preset {name}"));
                }
            }
            Action::Adjust { target, step, .. } => {
                if !step.is_finite() || *step <= 0. {
                    errors.push(format!("{id}: step must be positive and finite"));
                }
                if let AdjustTarget::Custom { increase, decrease } = target {
                    chords.extend([increase, decrease]);
                }
            }
            Action::Launch { path, .. } if path.trim().is_empty() => {
                errors.push(format!("{id}: empty launch path"))
            }
            Action::Url { url } if url.trim().is_empty() => errors.push(format!("{id}: empty URL")),
            _ => {}
        }
        for chord in chords {
            if let Err(e) = parse_chord(chord) {
                errors.push(format!("{id}: {e}"));
            }
        }
    }
    errors
}
pub fn config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_NAME)
        .join("config.json")
}
pub fn load_config() -> (Config, Option<String>) {
    load_from(&config_path())
}
pub fn load_from(path: &Path) -> (Config, Option<String>) {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
            Ok(c) if validate(&c).is_empty() => (c, None),
            result => {
                let reason = match result {
                    Err(e) => e.to_string(),
                    Ok(c) => validate(&c).join("; "),
                };
                let bad = path.with_extension("json.bad");
                let rename = replace_file(path, &bad)
                    .err()
                    .map(|e| format!("; backup failed: {e}"))
                    .unwrap_or_default();
                (
                    Config::default(),
                    Some(format!("Invalid config: {reason}{rename}")),
                )
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Config::default(), None),
        Err(e) => (Config::default(), Some(format!("Cannot read config: {e}"))),
    }
}
pub fn save_config(c: &Config) -> io::Result<()> {
    save_to(c, &config_path())
}
pub fn save_to(c: &Config, path: &Path) -> io::Result<()> {
    let errors = validate(c);
    if !errors.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            errors.join("; "),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    {
        use io::Write;
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(&serde_json::to_vec_pretty(c)?)?;
        f.sync_all()?;
    }
    replace_file(&temp, path)
}

fn replace_file(source: &Path, path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let src: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let dst: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                src.as_ptr(),
                dst.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(source, path)
    }
}
