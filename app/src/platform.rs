use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
};
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub fn autostart() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN)
        .and_then(|k| k.get_value::<String, _>("Touch"))
        .is_ok()
}
pub fn set_autostart(on: bool) -> std::io::Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN, KEY_SET_VALUE)?;
    if on {
        key.set_value(
            "Touch",
            &format!("\"{}\" --background", std::env::current_exe()?.display()),
        )
    } else {
        match key.delete_value("Touch") {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
}
pub fn appearance() -> (bool, u32) {
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    let light = hk
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .unwrap_or(1)
        != 0;
    let abgr = hk
        .open_subkey(r"Software\Microsoft\Windows\DWM")
        .and_then(|k| k.get_value::<u32, _>("AccentColor"))
        .unwrap_or(0xffc06700);
    let light = match std::env::var("TRACKPAD_GESTURES_THEME").as_deref() {
        Ok("light") => true,
        Ok("dark") => false,
        _ => light,
    };
    (
        light,
        ((abgr & 255) << 16) | (abgr & 0xff00) | ((abgr >> 16) & 255),
    )
}
