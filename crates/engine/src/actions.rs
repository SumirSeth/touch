//! Strict chord parsing shared by config validation and the action worker.
pub fn parse_chord(chord: &str) -> Result<Vec<u16>, String> {
    let parts: Vec<_> = chord.split('+').map(str::trim).collect();
    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        return Err(format!("Invalid chord: {chord}"));
    }
    let mut out = vec![];
    for (i, p) in parts.iter().enumerate() {
        let key = p.to_ascii_lowercase();
        let modifier = match key.as_str() {
            "ctrl" => Some(0x11),
            "alt" => Some(0x12),
            "shift" => Some(0x10),
            "win" => Some(0x5b),
            _ => None,
        };
        let vk = if let Some(m) = modifier {
            if i == parts.len() - 1 || out.contains(&m) {
                return Err(format!("Invalid modifier in {chord}"));
            }
            m
        } else {
            if i != parts.len() - 1 {
                return Err(format!("Only one non-modifier key allowed: {chord}"));
            }
            match key.as_str() {
                "left" => 0x25,
                "up" => 0x26,
                "right" => 0x27,
                "down" => 0x28,
                "tab" => 9,
                "enter" => 13,
                "esc" => 27,
                "space" => 32,
                "backspace" => 8,
                "delete" => 46,
                "home" => 36,
                "end" => 35,
                "pageup" => 33,
                "pagedown" => 34,
                "play_pause" | "playpause" => 0xb3,
                "next_track" | "nexttrack" => 0xb0,
                "prev_track" | "prevtrack" => 0xb1,
                "mute" => 0xad,
                "volume_up" | "volumeup" => 0xaf,
                "volume_down" | "volumedown" => 0xae,
                "=" => 0xbb,
                "-" => 0xbd,
                _ if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() => {
                    key.as_bytes()[0].to_ascii_uppercase() as u16
                }
                _ if key.starts_with('f') => {
                    let n = key[1..]
                        .parse::<u16>()
                        .map_err(|_| format!("Unknown key: {p}"))?;
                    if !(1..=24).contains(&n) {
                        return Err(format!("Unknown key: {p}"));
                    }
                    0x6f + n
                }
                _ => return Err(format!("Unknown key: {p}")),
            }
        };
        out.push(vk);
    }
    Ok(out)
}
#[cfg(windows)]
pub(crate) fn preset_chord(name: &str) -> Option<&'static str> {
    Some(match name {
        "task_view" => "Win+Tab",
        "show_desktop" => "Win+D",
        "desktop_left" => "Win+Ctrl+Left",
        "desktop_right" => "Win+Ctrl+Right",
        "new_desktop" => "Win+Ctrl+D",
        "alt_tab" => "Alt+Tab",
        "close_window" => "Alt+F4",
        "minimize" => "Win+Down",
        "maximize" => "Win+Up",
        "snap_left" => "Win+Left",
        "snap_right" => "Win+Right",
        "close_tab" => "Ctrl+W",
        "reopen_tab" => "Ctrl+Shift+T",
        "next_tab" => "Ctrl+Tab",
        "prev_tab" => "Ctrl+Shift+Tab",
        "back" => "Alt+Left",
        "forward" => "Alt+Right",
        "refresh" => "F5",
        "copy" => "Ctrl+C",
        "paste" => "Ctrl+V",
        "undo" => "Ctrl+Z",
        "redo" => "Ctrl+Y",
        "search" => "Win+S",
        "notifications" => "Win+N",
        "quick_settings" => "Win+A",
        "screenshot" => "Win+Shift+S",
        "file_explorer" => "Win+E",
        "play_pause" => "play_pause",
        "next_track" => "next_track",
        "prev_track" => "prev_track",
        "mute" => "mute",
        "volume_up" => "volume_up",
        "volume_down" => "volume_down",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chords() {
        for chord in [
            "Ctrl+Shift+T",
            "Win+Alt+R",
            "F11",
            "F24",
            "0",
            "Left",
            "PageUp",
            "play_pause",
            "Ctrl+=",
            "Ctrl+-",
        ] {
            assert!(parse_chord(chord).is_ok(), "{chord}");
        }
        for chord in [
            "",
            "Ctrl",
            "Ctrl+Ctrl+A",
            "A+B",
            "F25",
            "F0",
            "Ctrl++",
            "Hyper+A",
            "Ctrl+wat",
        ] {
            assert!(parse_chord(chord).is_err(), "{chord}");
        }
    }
    #[test]
    fn every_preset_has_a_chord() {
        #[cfg(windows)]
        for p in crate::presets() {
            if p.name != "lock" {
                assert!(parse_chord(preset_chord(p.name).unwrap()).is_ok());
            }
        }
    }
}
