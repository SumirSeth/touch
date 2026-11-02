//! The settings window (`touch.exe --settings`). It edits the config file and tells the agent to reload it;
//! the process exits when the window closes.
use crate::{PRODUCT_NAME, ipc, platform, ui};
use gestures_engine::{load_config, triggers};
use gpui::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::OnceLock,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::INFINITE},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, HWND_MESSAGE, MSG,
            MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT, RegisterClassW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
        },
    },
    core::{HSTRING, PCWSTR},
};

pub enum AppEvent {
    /// Another launch asked for the window.
    Focus,
    /// The agent changed the config (pause, start with Windows).
    Reload,
    /// A gesture fired; its row flashes.
    Flash(String),
    Error(String),
    Refresh,
}

pub fn run() {
    let Some(_instance) = ipc::instance(ipc::SETTINGS) else {
        ipc::signal(ipc::SETTINGS_FOCUS);
        return;
    };
    Application::new().run(|cx| {
        gpui_component::init(cx);
        cx.global_mut::<gpui_component::Theme>().font_family = "Segoe UI Variable".into();
        let (config, warning) = load_config();
        let (event_tx, event_rx) = async_channel::unbounded();
        let model = Rc::new(RefCell::new(ui::Model {
            config,
            warning: warning.unwrap_or_default(),
            flash: None,
            autostart: platform::autostart(),
            appearance: (true, 0x0067c0),
            conflicts: Vec::new(),
            events: event_tx.clone(),
            writer: ui::ConfigWriter::new(event_tx.clone()),
        }));
        listen(event_tx);
        open(model.clone(), cx);
        let closing = model.clone();
        cx.on_window_closed(move |cx| {
            if cx.windows().is_empty() {
                closing.borrow_mut().writer.flush();
                cx.quit();
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            let mut fading = false;
            loop {
                let event = if fading {
                    futures_lite::future::or(async { event_rx.recv().await.ok() }, async {
                        cx.background_executor()
                            .timer(Duration::from_millis(80))
                            .await;
                        Some(AppEvent::Refresh)
                    })
                    .await
                } else {
                    event_rx.recv().await.ok()
                };
                let Some(event) = event else { break };
                let updated = cx.update(|cx| {
                    match event {
                        AppEvent::Focus => open(model.clone(), cx),
                        AppEvent::Reload => {
                            let mut m = model.borrow_mut();
                            m.config = load_config().0;
                            m.refresh_platform();
                        }
                        AppEvent::Flash(trigger) => {
                            model.borrow_mut().flash = Some((trigger, Instant::now()));
                        }
                        AppEvent::Error(e) => model.borrow_mut().warning = e,
                        AppEvent::Refresh => {}
                    }
                    let mut m = model.borrow_mut();
                    if m.flash
                        .as_ref()
                        .is_some_and(|(_, time)| time.elapsed() >= Duration::from_millis(650))
                    {
                        m.flash = None;
                    }
                    fading = m.flash.is_some();
                    cx.refresh_windows();
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
    });
}

fn open(model: Rc<RefCell<ui::Model>>, cx: &mut App) {
    model.borrow_mut().refresh_platform();
    if let Some(w) = cx.windows().first() {
        let _ = w.update(cx, |_, w, _| w.activate_window());
        return;
    }
    let bounds = Bounds::centered(None, size(px(980.), px(720.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(PRODUCT_NAME.into()),
                ..Default::default()
            }),
            window_min_size: Some(size(px(820.), px(560.))),
            ..Default::default()
        },
        move |w, cx| {
            let view = cx.new(|cx| ui::Settings::new(model, w, cx));
            cx.new(|cx| gpui_component::Root::new(view, w, cx))
        },
    )
    .expect("open settings window");
    cx.activate(true);
}

static EVENTS: OnceLock<async_channel::Sender<AppEvent>> = OnceLock::new();

/// Receives the agent's signals on a background thread: the focus and reload events, and gesture flashes
/// posted to a message-only window.
fn listen(tx: async_channel::Sender<AppEvent>) {
    let _ = EVENTS.set(tx);
    let handles = [ipc::SETTINGS_FOCUS, ipc::SETTINGS_RELOAD].map(|n| ipc::event(n).0 as usize);
    std::thread::Builder::new()
        .name("settings-ipc".into())
        .spawn(move || unsafe {
            let handles = handles.map(|h| windows::Win32::Foundation::HANDLE(h as *mut _));
            let class = HSTRING::from(ipc::FLASH_CLASS);
            let instance = GetModuleHandleW(None).unwrap_or_default();
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(flash_proc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            });
            let _window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                &class,
                &class,
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            );
            let Some(tx) = EVENTS.get() else { return };
            loop {
                let wait = MsgWaitForMultipleObjects(Some(&handles), false, INFINITE, QS_ALLINPUT);
                let event = match wait.0.wrapping_sub(WAIT_OBJECT_0.0) {
                    0 => Some(AppEvent::Focus),
                    1 => Some(AppEvent::Reload),
                    2 => {
                        let mut msg = MSG::default();
                        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                            DispatchMessageW(&msg);
                        }
                        None
                    }
                    _ => return,
                };
                if let Some(event) = event
                    && tx.send_blocking(event).is_err()
                {
                    return;
                }
            }
        })
        .expect("settings ipc thread");
}

unsafe extern "system" fn flash_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == ipc::WM_FLASH {
        if let (Some(tx), Some(t)) = (EVENTS.get(), triggers().get(w.0)) {
            let _ = tx.try_send(AppEvent::Flash(t.id.to_string()));
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
