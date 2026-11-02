//! The background agent: the gesture engine and the tray icon, on a plain Win32 message loop. It never loads
//! the UI; settings run in a separate `touch.exe --settings` process that exists only while its window is open.
use crate::{PRODUCT_NAME, ipc, platform};
use gestures_engine::{Engine, EngineEvent, load_config, save_config};
use std::sync::mpsc;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::{GetCurrentThreadId, INFINITE, WaitForMultipleObjects},
    UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_APP,
    },
};

/// Thread message carrying a request index: 0 open settings, 1 quit, 2 reload the config.
const WM_REQUEST: u32 = WM_APP + 7;
const OPEN: usize = 0;
const QUIT: usize = 1;
const RELOAD: usize = 2;

pub fn run(_instance: ipc::Instance, show_settings: bool) {
    let main = unsafe { GetCurrentThreadId() };
    let post = move |request: usize| unsafe {
        let _ = PostThreadMessageW(main, WM_REQUEST, WPARAM(request), LPARAM(0));
    };

    // Requests from other processes: a second launch (open/quit) and the settings window (reload).
    let handles =
        [ipc::AGENT_OPEN, ipc::AGENT_QUIT, ipc::AGENT_RELOAD].map(|n| ipc::event(n).0 as usize);
    std::thread::Builder::new()
        .name("agent-requests".into())
        .spawn(move || {
            let handles = handles.map(|h| windows::Win32::Foundation::HANDLE(h as *mut _));
            loop {
                let index = unsafe { WaitForMultipleObjects(&handles, false, INFINITE) }.0 as usize;
                if index >= handles.len() {
                    break;
                }
                post(index);
            }
        })
        .expect("request thread");

    let mut config = load_config().0;
    let (events, rx) = mpsc::channel();
    let engine = Engine::start(config.clone(), Some(events)).expect("start gesture engine");
    std::thread::Builder::new()
        .name("agent-events".into())
        .spawn(move || {
            for event in rx {
                if let EngineEvent::Fired { trigger } | EngineEvent::Adjusted { trigger, .. } =
                    event
                {
                    ipc::flash(&trigger);
                }
            }
        })
        .expect("engine event thread");

    let menu = Menu::new();
    let settings = MenuItem::new("Settings", true, None);
    let pause = CheckMenuItem::new("Pause gestures", true, !config.enabled, None);
    let startup = CheckMenuItem::new("Start with Windows", true, platform::autostart(), None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&settings, &pause, &startup, &quit])
        .expect("tray menu");
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(PRODUCT_NAME)
        .with_icon(tray_icon::Icon::from_resource(1, None).expect("embedded tray icon"))
        .build()
        .expect("create tray");

    if show_settings {
        open_settings();
    }
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if msg.hwnd.is_invalid() && msg.message == WM_REQUEST {
            match msg.wParam.0 {
                OPEN => open_settings(),
                QUIT => break,
                RELOAD => {
                    config = load_config().0;
                    engine.set_config(config.clone());
                    pause.set_checked(!config.enabled);
                    startup.set_checked(platform::autostart());
                }
                _ => {}
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // Tray and menu events are produced while dispatching the tray's window messages.
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                open_settings();
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == settings.id() {
                open_settings();
            } else if event.id == quit.id() {
                post(QUIT);
            } else if event.id == pause.id() {
                config = load_config().0;
                config.enabled = !pause.is_checked();
                let _ = save_config(&config);
                engine.set_config(config.clone());
                ipc::signal(ipc::SETTINGS_RELOAD);
            } else if event.id == startup.id() {
                let _ = platform::set_autostart(startup.is_checked());
                startup.set_checked(platform::autostart());
                ipc::signal(ipc::SETTINGS_RELOAD);
            }
        }
    }
    engine.stop();
}

/// Brings up the settings window: focuses it if it's already open, otherwise starts `touch.exe --settings`.
fn open_settings() {
    if ipc::instance(ipc::SETTINGS).is_none() {
        ipc::signal(ipc::SETTINGS_FOCUS);
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--settings").spawn();
    }
}
