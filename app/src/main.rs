#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
mod ui;
use gestures_engine::{Engine, EngineEvent, load_config};
use gpui::*;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use tray_icon::{
    TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
};
pub enum AppEvent {
    Open,
    Quit,
    Menu(MenuEvent),
    Engine(EngineEvent),
    Refresh,
}
pub const PRODUCT_NAME: &str = "Touch";
fn main() {
    let Some(_instance) = platform::single_instance() else {
        return;
    };
    if std::env::args().any(|a| a == "--quit") {
        return;
    }
    Application::new().run(|cx| {
        gpui_component::init(cx);
        cx.global_mut::<gpui_component::Theme>().font_family = "Segoe UI Variable".into();
        let (config, warning) = load_config();
        let (event_tx, event_rx) = async_channel::unbounded();
        let (tx, rx) = mpsc::channel();
        let engine = Rc::new(RefCell::new(Some(
            Engine::start(config.clone(), Some(tx)).expect("start engine"),
        )));
        let model = Rc::new(RefCell::new(ui::Model {
            config,
            warning: warning.unwrap_or_default(),
            engine: engine.clone(),
            flash: None,
            autostart: platform::autostart(),
            appearance: (true, 0x0067c0),
            conflicts: Vec::new(),
            events: event_tx.clone(),
        }));
        let menu = Menu::new();
        let settings = MenuItem::new("Settings", true, None);
        let pause = CheckMenuItem::new("Pause gestures", true, false, None);
        let startup =
            CheckMenuItem::new("Start with Windows", true, model.borrow().autostart, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[&settings, &pause, &startup, &quit])
            .expect("tray menu");
        let icon = tray_icon::Icon::from_resource(1, None).expect("embedded tray icon");
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(PRODUCT_NAME)
            .with_icon(icon)
            .build()
            .expect("create tray");
        if !std::env::args().any(|a| a == "--background") {
            open(model.clone(), cx);
        }
        let tray_tx = event_tx.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = tray_tx.try_send(AppEvent::Open);
            }
        }));
        let menu_tx = event_tx.clone();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = menu_tx.try_send(AppEvent::Menu(event));
        }));
        platform::forward_requests(event_tx.clone());
        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                if event_tx.send_blocking(AppEvent::Engine(event)).is_err() {
                    break;
                }
            }
        });
        cx.spawn(async move |cx| {
            let _tray = tray;
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
                if cx
                    .update(|cx| {
                        match event {
                            AppEvent::Quit => {
                                if let Some(e) = engine.borrow_mut().take() {
                                    e.stop();
                                }
                                cx.quit();
                                return;
                            }
                            AppEvent::Open => open(model.clone(), cx),
                            AppEvent::Menu(event) => {
                                if event.id == settings.id() {
                                    open(model.clone(), cx);
                                }
                                if event.id == quit.id() {
                                    if let Some(e) = engine.borrow_mut().take() {
                                        e.stop();
                                    }
                                    cx.quit();
                                    return;
                                }
                                if event.id == pause.id() {
                                    let mut m = model.borrow_mut();
                                    m.config.enabled = !pause.is_checked();
                                    m.save();
                                }
                                if event.id == startup.id() {
                                    let mut m = model.borrow_mut();
                                    if let Err(e) = platform::set_autostart(startup.is_checked()) {
                                        m.warning = e.to_string();
                                    }
                                    m.autostart = platform::autostart();
                                }
                            }
                            AppEvent::Engine(event) => match event {
                                EngineEvent::Fired { trigger }
                                | EngineEvent::Adjusted { trigger, .. } => {
                                    model.borrow_mut().flash = Some((trigger, Instant::now()));
                                }
                                EngineEvent::Error(e) => model.borrow_mut().warning = e,
                                _ => {}
                            },
                            AppEvent::Refresh => {}
                        }
                        let mut m = model.borrow_mut();
                        if pause.is_checked() == m.config.enabled {
                            pause.set_checked(!m.config.enabled);
                        }
                        if startup.is_checked() != m.autostart {
                            startup.set_checked(m.autostart);
                        }
                        if m.flash
                            .as_ref()
                            .is_some_and(|(_, time)| time.elapsed() >= Duration::from_millis(650))
                        {
                            m.flash = None;
                        }
                        fading = m.flash.is_some() && !cx.windows().is_empty();
                        if !cx.windows().is_empty() {
                            cx.refresh_windows();
                        }
                    })
                    .is_err()
                {
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
    .expect("open GPUI settings");
    cx.activate(true);
}
