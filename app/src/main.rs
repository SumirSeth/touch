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
pub const PRODUCT_NAME: &str = "Trackpad Gestures";
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
        let (tx, rx) = mpsc::channel();
        let engine = Rc::new(RefCell::new(Some(
            Engine::start(config.clone(), Some(tx)).expect("start engine"),
        )));
        let model = Rc::new(RefCell::new(ui::Model {
            config,
            warning: warning.unwrap_or_default(),
            engine: engine.clone(),
            flash: None,
        }));
        let menu = Menu::new();
        let settings = MenuItem::new("Settings", true, None);
        let pause = CheckMenuItem::new("Pause gestures", true, false, None);
        let startup = CheckMenuItem::new("Start with Windows", true, platform::autostart(), None);
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
        cx.spawn(async move |cx| {
            let _tray = tray;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let result = cx.update(|cx| {
                    if platform::take_quit_request() {
                        if let Some(e) = engine.borrow_mut().take() {
                            e.stop();
                        }
                        cx.quit();
                        return;
                    }
                    let mut show = platform::take_open_request();
                    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                        if matches!(
                            event,
                            TrayIconEvent::Click {
                                button: tray_icon::MouseButton::Left,
                                button_state: tray_icon::MouseButtonState::Up,
                                ..
                            }
                        ) {
                            show = true;
                        }
                    }
                    while let Ok(event) = MenuEvent::receiver().try_recv() {
                        if event.id == settings.id() {
                            show = true;
                        }
                        if event.id == quit.id() {
                            if let Some(e) = engine.borrow_mut().take() {
                                e.stop();
                            }
                            cx.quit();
                        }
                        if event.id == pause.id() {
                            let enabled = !pause.is_checked();
                            model.borrow_mut().config.enabled = enabled;
                            model.borrow_mut().save();
                        }
                        if event.id == startup.id()
                            && let Err(e) = platform::set_autostart(startup.is_checked())
                        {
                            model.borrow_mut().warning = e.to_string();
                        }
                    }
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            EngineEvent::Fired { trigger }
                            | EngineEvent::Adjusted { trigger, .. } => {
                                model.borrow_mut().flash = Some((trigger, Instant::now()))
                            }
                            EngineEvent::Error(e) => model.borrow_mut().warning = e,
                            EngineEvent::NoTouchpad => {}
                            _ => {}
                        }
                    }
                    pause.set_checked(!model.borrow().config.enabled);
                    startup.set_checked(platform::autostart());
                    if show {
                        open(model.clone(), cx);
                    }
                    if !cx.windows().is_empty() {
                        cx.refresh_windows();
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
    });
}
fn open(model: Rc<RefCell<ui::Model>>, cx: &mut App) {
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
