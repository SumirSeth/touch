//! Signals between the headless agent (`touch.exe`) and the settings window (`touch.exe --settings`).
//!
//! Requests are named auto-reset events. Gesture flashes go from the agent to an open settings window as
//! `WM_APP` messages to a message-only window, carrying the trigger's index in `gestures_engine::triggers()`.
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, WPARAM},
        System::Threading::{CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, SetEvent},
        UI::WindowsAndMessaging::{FindWindowExW, HWND_MESSAGE, PostMessageW, WM_APP},
    },
    core::{HSTRING, PCWSTR},
};

pub const AGENT: &str = r"Local\Touch.Agent.v1";
pub const AGENT_OPEN: &str = r"Local\Touch.Agent.v1.Open";
pub const AGENT_QUIT: &str = r"Local\Touch.Agent.v1.Quit";
pub const AGENT_RELOAD: &str = r"Local\Touch.Agent.v1.Reload";
pub const SETTINGS: &str = r"Local\Touch.Settings.v1";
pub const SETTINGS_FOCUS: &str = r"Local\Touch.Settings.v1.Focus";
pub const SETTINGS_RELOAD: &str = r"Local\Touch.Settings.v1.Reload";
pub const FLASH_CLASS: &str = "Touch.Settings.v1.Flash";
pub const WM_FLASH: u32 = WM_APP + 1;

/// A named mutex held for the life of the process.
pub struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
/// Takes the named single-instance mutex, or returns `None` if another process holds it.
pub fn instance(name: &str) -> Option<Instance> {
    unsafe {
        let mutex = CreateMutexW(None, false, &HSTRING::from(name)).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            None
        } else {
            Some(Instance(mutex))
        }
    }
}
/// Creates (or opens) a named auto-reset event; the handle lives as long as the process.
pub fn event(name: &str) -> HANDLE {
    unsafe { CreateEventW(None, false, false, &HSTRING::from(name)) }.expect("named event")
}
/// Sets a named event if its owner is running.
pub fn signal(name: &str) {
    unsafe {
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, &HSTRING::from(name)) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
    }
}
/// Tells an open settings window that a gesture fired.
pub fn flash(trigger: &str) {
    let Some(index) = gestures_engine::triggers()
        .iter()
        .position(|t| t.id == trigger)
    else {
        return;
    };
    unsafe {
        if let Ok(hwnd) = FindWindowExW(
            Some(HWND_MESSAGE),
            None,
            &HSTRING::from(FLASH_CLASS),
            PCWSTR::null(),
        ) {
            let _ = PostMessageW(Some(hwnd), WM_FLASH, WPARAM(index), LPARAM(0));
        }
    }
}
