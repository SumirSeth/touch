use gestures_engine::Action;
use gestures_engine::*;
use gpui::{prelude::*, *};
use gpui_component::{
    Selectable, Sizable,
    button::Button,
    input::{Input, InputEvent, InputState},
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
pub struct Model {
    pub config: Config,
    pub warning: String,
    pub engine: Rc<RefCell<Option<Engine>>>,
    pub autostart: bool,
    pub appearance: (bool, u32),
    pub conflicts: Vec<Conflict>,
    pub events: async_channel::Sender<crate::AppEvent>,
    pub flash: Option<(String, Instant)>,
    pub writer: ConfigWriter,
}
/// Writes the config file on a background thread, coalescing bursts (slider drags, typing) into one write
/// shortly after the last change, so the UI thread never waits on the disk.
pub struct ConfigWriter {
    tx: Option<std::sync::mpsc::Sender<Config>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl ConfigWriter {
    const DEBOUNCE: Duration = Duration::from_millis(400);
    pub fn new(events: async_channel::Sender<crate::AppEvent>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Config>();
        let thread = std::thread::Builder::new()
            .name("config-writer".into())
            .spawn(move || {
                while let Ok(mut latest) = rx.recv() {
                    // Keep taking newer configs until changes stop (or the app quits).
                    while let Ok(next) = rx.recv_timeout(Self::DEBOUNCE) {
                        latest = next;
                    }
                    if let Err(e) = save_config(&latest) {
                        let error = EngineEvent::Error(format!("Couldn't save settings: {e}"));
                        let _ = events.send_blocking(crate::AppEvent::Engine(error));
                    }
                }
            })
            .expect("config writer thread");
        Self {
            tx: Some(tx),
            thread: Some(thread),
        }
    }
    fn write(&self, config: Config) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(config);
        }
    }
    /// Writes any pending change now and stops the thread. Call before quitting.
    pub fn flush(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Model {
    pub fn refresh_platform(&mut self) {
        self.autostart = crate::platform::autostart();
        self.appearance = crate::platform::appearance();
        self.conflicts = windows_conflicts(&self.config);
        let _ = self.events.try_send(crate::AppEvent::Refresh);
    }
    pub fn save(&mut self) {
        self.conflicts = windows_conflicts(&self.config);
        let _ = self.events.try_send(crate::AppEvent::Refresh);
        let errors = validate(&self.config);
        if !errors.is_empty() {
            self.warning = errors.join(" · ");
            return;
        }
        self.warning.clear();
        if let Some(e) = self.engine.borrow().as_ref() {
            e.set_config(self.config.clone());
        }
        self.writer.write(self.config.clone());
    }
}
pub struct Settings {
    model: Rc<RefCell<Model>>,
    selected: String,
    general: bool,
    recording: bool,
    win: bool,
    focus: FocusHandle,
    fields: Vec<Entity<InputState>>,
    slider: Entity<SliderState>,
    editor_scroll: ScrollHandle,
    subscriptions: Vec<Subscription>,
}
impl Settings {
    pub fn new(model: Rc<RefCell<Model>>, w: &mut Window, cx: &mut Context<Self>) -> Self {
        let fields = (0..3)
            .map(|_| cx.new(|cx| InputState::new(w, cx)))
            .collect::<Vec<_>>();
        let slider = cx.new(|_| SliderState::new().min(0.1).max(30.).step(0.1));
        let mut this = Self {
            model,
            selected: "edge_right".into(),
            general: false,
            recording: false,
            win: false,
            focus: cx.focus_handle(),
            fields,
            slider,
            editor_scroll: ScrollHandle::new(),
            subscriptions: vec![],
        };
        for i in 0..3 {
            this.subscriptions.push(cx.subscribe_in(
                &this.fields[i],
                w,
                move |this, _, event, w, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.apply_fields(w, cx);
                    }
                },
            ));
        }
        this.subscriptions
            .push(cx.subscribe(&this.slider, |this, _, event, cx| {
                let SliderEvent::Change(value) = event;
                let mut m = this.model.borrow_mut();
                if let Some(Action::Adjust { step, .. }) = m.config.bindings.get_mut(&this.selected)
                {
                    *step = value.start() as f64;
                    m.save();
                }
                cx.notify();
            }));
        this.subscriptions
            .push(cx.observe_window_activation(w, |this, w, cx| {
                if w.is_window_active() {
                    this.model.borrow_mut().refresh_platform();
                    cx.notify();
                }
            }));
        this.subscriptions
            .push(cx.observe_window_appearance(w, |this, _, cx| {
                this.model.borrow_mut().appearance = crate::platform::appearance();
                cx.notify();
            }));
        this.sync_fields(w, cx);
        this
    }
    fn sync_fields(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let a = self.action();
        if let Action::Adjust { step, target, .. } = &a {
            let (min, max, tick) = step_range(target, &self.selected);
            self.slider.update(cx, |s, cx| {
                *s = SliderState::new()
                    .min(min.min(*step as f32))
                    .max(max.max(*step as f32))
                    .step(tick);
                s.set_value(*step as f32, w, cx);
            });
        }
        let vals = match a {
            Action::Keys { keys } => vec![keys],
            Action::Launch { path, args } => vec![path, args.unwrap_or_default()],
            Action::Url { url } => vec![url],
            Action::Adjust {
                target: AdjustTarget::Custom { increase, decrease },
                ..
            } => vec![increase, decrease],
            _ => vec![],
        };
        for i in 0..3 {
            self.fields[i].update(cx, |s, cx| {
                s.set_value(vals.get(i).cloned().unwrap_or_default(), w, cx)
            });
        }
    }
    fn action(&self) -> Action {
        self.model
            .borrow()
            .config
            .bindings
            .get(&self.selected)
            .cloned()
            .unwrap_or(Action::None)
    }
    fn bind(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        self.model
            .borrow_mut()
            .config
            .bindings
            .insert(self.selected.clone(), a);
        self.model.borrow_mut().save();
        self.sync_fields(w, cx);
        cx.notify();
    }
    fn apply_fields(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
        let v: Vec<_> = self
            .fields
            .iter()
            .map(|f| f.read(cx).value().to_string())
            .collect();
        let a = match self.action() {
            Action::Keys { .. } => Action::Keys { keys: v[0].clone() },
            Action::Launch { .. } => Action::Launch {
                path: v[0].clone(),
                args: Some(v[1].clone()),
            },
            Action::Url { .. } => Action::Url { url: v[0].clone() },
            Action::Adjust {
                target: AdjustTarget::Custom { .. },
                step,
                reverse,
            } => Action::Adjust {
                target: AdjustTarget::Custom {
                    increase: v[0].clone(),
                    decrease: v[1].clone(),
                },
                step,
                reverse,
            },
            _ => return,
        };
        self.model
            .borrow_mut()
            .config
            .bindings
            .insert(self.selected.clone(), a);
        self.model.borrow_mut().save();
        cx.notify();
    }
    fn toggle(&mut self, id: &str) {
        let mut m = self.model.borrow_mut();
        match id {
            "enabled" => m.config.enabled = !m.config.enabled,
            "drag" => m.config.drag3 = !m.config.drag3,
            "freeze" => m.config.freeze_cursor = !m.config.freeze_cursor,
            _ => {}
        }
        m.save();
    }
}
fn caption(a: &Action) -> String {
    match a {
        Action::None => "Not assigned".into(),
        Action::Preset { name } => presets()
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.label.to_string())
            .unwrap_or(name.clone()),
        Action::Keys { keys } => keys.clone(),
        Action::Launch { .. } => "Launch app".into(),
        Action::Url { .. } => "Open URL".into(),
        Action::Adjust { target, .. } => adjust_info(target).label.into(),
    }
}
impl Render for Settings {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (light, accent) = self.model.borrow().appearance;
        let (bg, card, fg, muted, line) = if light {
            (0xf3f3f3, 0xffffff, 0x202020, 0x666666, 0xdddddd)
        } else {
            (0x202020, 0x2b2b2b, 0xf5f5f5, 0xb6b6b6, 0x444444)
        };
        let mode = if light {
            gpui_component::ThemeMode::Light
        } else {
            gpui_component::ThemeMode::Dark
        };
        if cx.global::<gpui_component::Theme>().mode != mode {
            gpui_component::Theme::change(mode, Some(w), cx);
        }
        let primary: Hsla = rgb(accent).into();
        let control: Hsla = rgb(if light { 0xffffff } else { 0x383838 }).into();
        if cx.global::<gpui_component::Theme>().colors.primary != primary
            || cx.global::<gpui_component::Theme>().colors.background != control
        {
            let colors = &mut cx.global_mut::<gpui_component::Theme>().colors;
            colors.background = control;
            colors.foreground = rgb(fg).into();
            colors.border = rgb(line).into();
            colors.input = rgb(line).into();
            colors.secondary = control;
            colors.secondary_foreground = rgb(fg).into();
            colors.secondary_hover = rgb(if light { 0xf0f0f0 } else { 0x454545 }).into();
            colors.secondary_active = rgb(if light { 0xe5e5e5 } else { 0x4b4b4b }).into();
            colors.slider_bar = primary;
            colors.slider_thumb = rgb(0xffffff).into();
            colors.switch_thumb = rgb(0xffffff).into();
            colors.primary = primary;
            colors.primary_hover = primary;
            colors.primary_active = primary;
            colors.primary_foreground =
                rgb(if primary.l > 0.6 { 0x101010 } else { 0xffffff }).into();
        }
        let selected_action = self.action();
        let model = self.model.borrow();
        let config = &model.config;
        let mut nav = div()
            .id("gestures")
            .w(px(288.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .border_r_1()
            .border_color(rgb(line));
        nav = nav.child(
            Button::new("general")
                .outline()
                .selected(self.general)
                .when(self.general, |b| {
                    b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                })
                .label("General settings")
                .on_click(cx.listener(|s, _, _, cx| {
                    s.general = true;
                    s.editor_scroll.set_offset(point(px(0.), px(0.)));
                    cx.notify();
                })),
        );
        for group in unique_groups(triggers().iter().map(|t| t.group)) {
            nav = nav.child(
                div()
                    .mt_3()
                    .text_xs()
                    .text_color(rgb(muted))
                    .child(group.to_uppercase()),
            );
            for t in triggers().iter().filter(|t| t.group == group) {
                let id = t.id;
                let selected = !self.general && self.selected == id;
                let flash = self.model.borrow().flash.as_ref().is_some_and(|(i, time)| {
                    i == id && time.elapsed() < Duration::from_millis(650)
                });
                let mut diagram = div()
                    .w(px(40.))
                    .flex_shrink_0()
                    .h(px(30.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(2.));
                for _ in 0..t.fingers {
                    diagram =
                        diagram.child(div().w(px(4.)).h(px(8.)).rounded_full().bg(rgb(accent)));
                }
                if id.starts_with("edge_") {
                    diagram = diagram.child(
                        match id {
                            "edge_left" => div().absolute().left_0().top_0().h(px(28.)).w(px(3.)),
                            "edge_right" => div().absolute().right_0().top_0().h(px(28.)).w(px(3.)),
                            "edge_top" => div().absolute().top_0().left_0().w(px(38.)).h(px(3.)),
                            _ => div().absolute().bottom_0().left_0().w(px(38.)).h(px(3.)),
                        }
                        .bg(rgb(accent)),
                    );
                }
                if id.starts_with("corner_") {
                    let zone = div().absolute().size(px(9.)).rounded_sm().bg(rgb(accent));
                    let zone = if id.contains("top") {
                        zone.top_0()
                    } else {
                        zone.bottom_0()
                    };
                    let zone = if id.contains("left") {
                        zone.left_0()
                    } else {
                        zone.right_0()
                    };
                    diagram = diagram.child(zone);
                }
                nav =
                    nav.child(
                        div()
                            .id(id)
                            .h(px(60.))
                            .flex_shrink_0()
                            .relative()
                            .rounded_lg()
                            .p_2()
                            .flex()
                            .items_center()
                            .gap_3()
                            .bg(rgb(if selected {
                                if light { 0xe7e7e7 } else { 0x333333 }
                            } else {
                                bg
                            }))
                            .hover(|s| s.bg(rgb(if light { 0xeaeaea } else { 0x303030 })))
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top(px(16.))
                                    .w(px(3.))
                                    .h(px(28.))
                                    .rounded_full()
                                    .bg(rgb(accent))
                                    .opacity(if selected { 1. } else { 0. }),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .rounded_lg()
                                    .bg(rgb(accent))
                                    .opacity(if flash {
                                        self.model
                                            .borrow()
                                            .flash
                                            .as_ref()
                                            .map(|(_, time)| {
                                                0.22 * (1. - time.elapsed().as_secs_f32() / 0.65)
                                                    .max(0.)
                                            })
                                            .unwrap_or(0.)
                                    } else {
                                        0.
                                    }),
                            )
                            .cursor_pointer()
                            .on_click(cx.listener(move |s, _, w, cx| {
                                s.editor_scroll.set_offset(point(px(0.), px(0.)));
                                s.selected = id.into();
                                s.general = false;
                                s.recording = false;
                                s.sync_fields(w, cx);
                                cx.notify();
                            }))
                            .child(diagram.relative())
                            .child(div().flex().flex_col().child(t.label).child(
                                div().text_xs().text_color(rgb(muted)).child(caption(
                                    config.bindings.get(id).unwrap_or(&Action::None),
                                )),
                            )),
                    );
            }
        }
        let mut panel = div()
            .id("editor")
            .track_scroll(&self.editor_scroll)
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_3()
            .min_w_0();
        if self.general {
            panel = panel.child(div().text_2xl().child("General")).child(
                div()
                    .text_color(rgb(muted))
                    .child("Make your touchpad feel like yours."),
            );
            for (id, label, description, on) in [
                (
                    "enabled",
                    "Enable gestures",
                    "Pause or resume all Touch gestures.",
                    config.enabled,
                ),
                (
                    "drag",
                    "Three-finger drag",
                    "Replaces three-finger swipes and tap with dragging.",
                    config.drag3,
                ),
                (
                    "freeze",
                    "Keep pointer still",
                    "Freeze the pointer during edge slides.",
                    config.freeze_cursor,
                ),
            ] {
                panel = panel.child(
                    setting_card(card, line)
                        .child(setting_text(label, description, muted))
                        .child(Switch::new(id).checked(on).on_click(cx.listener(
                            move |s, _, _, cx| {
                                s.toggle(id);
                                cx.notify();
                            },
                        ))),
                );
            }
            for (id, label, value) in [
                ("edge", "Edge width", config.edge_width_mm),
                ("swipe", "Swipe distance", config.swipe_distance_mm),
            ] {
                panel = panel.child(
                    setting_card(card, line)
                        .child(setting_text(
                            label,
                            if id == "edge" {
                                "Width of the active zone along each edge."
                            } else {
                                "Distance before a swipe is recognised."
                            },
                            muted,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(format!("{value:.1} mm"))
                                .children([-1., 1.].into_iter().enumerate().map(|(i, delta)| {
                                    Button::new((id, i))
                                        .outline()
                                        .label(if delta < 0. { "-" } else { "+" })
                                        .on_click(cx.listener(move |s, _, _, cx| {
                                            let mut m = s.model.borrow_mut();
                                            let v = if id == "edge" {
                                                &mut m.config.edge_width_mm
                                            } else {
                                                &mut m.config.swipe_distance_mm
                                            };
                                            *v = (*v + delta).clamp(1., 50.);
                                            m.save();
                                            cx.notify();
                                        }))
                                })),
                        ),
                );
            }
            panel = panel
                .child(
                    setting_card(card, line)
                        .child(setting_text(
                            "Start with Windows",
                            "Keep Touch available in the notification area.",
                            muted,
                        ))
                        .child(Switch::new("autostart").checked(model.autostart).on_click(
                            cx.listener(|s, on, _, cx| {
                                if let Err(e) = crate::platform::set_autostart(*on) {
                                    s.model.borrow_mut().warning = e.to_string();
                                }
                                let mut m = s.model.borrow_mut();
                                m.autostart = crate::platform::autostart();
                                let _ = m.events.try_send(crate::AppEvent::Refresh);
                                cx.notify();
                            }),
                        )),
                )
                .child(
                    setting_card(card, line)
                        .child(setting_text(
                            "Restore defaults",
                            "Reset all gestures and preferences.",
                            muted,
                        ))
                        .child(
                            Button::new("reset")
                                .outline()
                                .label("Reset to defaults")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.model.borrow_mut().config = Config::default();
                                    s.model.borrow_mut().save();
                                    s.sync_fields(w, cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    setting_card(card, line)
                        .child(setting_text(
                            "Configuration",
                            "Your bindings are saved locally as JSON.",
                            muted,
                        ))
                        .child(
                            Button::new("folder")
                                .outline()
                                .label("Open folder")
                                .on_click(|_, _, cx| {
                                    cx.open_url(&config_path().parent().unwrap().to_string_lossy())
                                }),
                        ),
                )
                .child(concat!("Touch ", env!("CARGO_PKG_VERSION")));
        } else {
            let t = triggers().iter().find(|t| t.id == self.selected).unwrap();
            panel = panel
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(muted))
                        .child(t.group.to_uppercase()),
                )
                .child(div().text_2xl().child(t.label))
                .child(div().text_color(rgb(muted)).child(t.description));
            if config.drag3 && (t.id.starts_with("swipe3") || t.id == "tap3") {
                panel = panel.child("Disabled while three-finger drag is on.");
            }
            for conflict in model
                .conflicts
                .iter()
                .filter(|c| t.id.starts_with(c.trigger_prefix))
            {
                panel = panel.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(rgb(if light { 0xfff4ce } else { 0x493c20 }))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .border_1()
                        .border_color(rgb(if light { 0xe6d9a8 } else { 0x6a5932 }))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Windows gestures overlap"),
                        )
                        .child(div().text_sm().child(format!(
                            "{}. Otherwise, both actions may run.",
                            conflict.message
                        )))
                        .child(
                            Button::new("windows-settings")
                                .outline()
                                .label("Open touchpad settings")
                                .on_click(|_, _, cx| cx.open_url("ms-settings:devices-touchpad")),
                        ),
                );
            }
            if !t.continuous {
                panel = panel.child(
                    setting_card(card, line)
                        .child(setting_text("Action", &caption(&selected_action), muted))
                        .child(Button::new("test").outline().label("Test action").on_click(
                            cx.listener(|s, _, _, _| {
                                if let Some(e) = s.model.borrow().engine.borrow().as_ref() {
                                    e.run_action(&s.action());
                                }
                            }),
                        )),
                );
            }
            if t.continuous {
                let mut targets = div().flex().flex_wrap().gap_2().child(
                    Button::new("unbound")
                        .w(px(148.))
                        .h(px(38.))
                        .outline()
                        .selected(matches!(selected_action, Action::None))
                        .when(matches!(selected_action, Action::None), |b| {
                            b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                        })
                        .label("Not assigned")
                        .on_click(cx.listener(|s, _, w, cx| s.bind(Action::None, w, cx))),
                );
                for info in adjust_targets() {
                    let target = info.target;
                    let selected = matches!(selected_action, Action::Adjust { target: ref current, .. } if adjust_info(current).target == target);
                    targets = targets.child(
                        Button::new(target)
                            .outline()
                            .selected(selected)
                            .when(selected, |b| {
                                b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                            })
                            .w(px(148.))
                            .h(px(38.))
                            .label(info.label)
                            .on_click(cx.listener(move |s, _, w, cx| {
                                let target = match target {
                                    "brightness" => AdjustTarget::Brightness,
                                    "seek" => AdjustTarget::Seek,
                                    "scroll_h" => AdjustTarget::ScrollH,
                                    "zoom" => AdjustTarget::Zoom,
                                    "tabs" => AdjustTarget::Tabs,
                                    "desktops" => AdjustTarget::Desktops,
                                    "custom" => AdjustTarget::Custom {
                                        increase: "Right".into(),
                                        decrease: "Left".into(),
                                    },
                                    _ => AdjustTarget::Volume,
                                };
                                s.bind(
                                    Action::Adjust {
                                        target,
                                        step: if s.selected == "twist4" {
                                            12.
                                        } else {
                                            info.default_step
                                        },
                                        reverse: false,
                                    },
                                    w,
                                    cx,
                                );
                            })),
                    );
                }
                panel = panel.child(
                    div()
                        .flex_shrink_0()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(line))
                        .bg(rgb(card))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child("Adjust action")
                        .child(targets),
                );
                if let Action::Adjust {
                    step,
                    reverse,
                    target,
                } = self.action()
                {
                    let info = adjust_info(&target);
                    let unit = if t.id == "twist4" {
                        "degrees"
                    } else {
                        info.unit
                    };
                    let hint = if t.id == "twist4" {
                        format!("About {:.0} steps per quarter turn", 90. / step)
                    } else if matches!(target, AdjustTarget::Brightness) {
                        format!("About {:.0} % per finger-width (10 mm)", 10. / step)
                    } else {
                        format!("About {:.1} steps per finger-width (10 mm)", 10. / step)
                    };
                    let (min, max, _) = step_range(&target, t.id);
                    let min = min.min(step as f32);
                    let max = max.max(step as f32);
                    panel = panel
                        .child(
                            setting_card(card, line)
                                .child(setting_text("Step distance", &hint, muted))
                                .child(
                                    div()
                                        .w(px(190.))
                                        .flex_shrink_0()
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(div().text_right().child(format!(
                                            "{step:.2} {unit} / {}",
                                            if matches!(target, AdjustTarget::Brightness) {
                                                "1 %"
                                            } else {
                                                "step"
                                            }
                                        )))
                                        .child(Slider::new(&self.slider))
                                        .child(
                                            div()
                                                .flex()
                                                .justify_between()
                                                .text_xs()
                                                .text_color(rgb(muted))
                                                .child(format!("{min} {unit}"))
                                                .child(format!("{max} {unit}")),
                                        ),
                                ),
                        )
                        .child(
                            setting_card(card, line)
                                .child(setting_text(
                                    "Reverse direction",
                                    "Swap increase and decrease.",
                                    muted,
                                ))
                                .child(Switch::new("reverse").checked(reverse).on_click(
                                    cx.listener(|s, _, w, cx| {
                                        if let Action::Adjust {
                                            target,
                                            step,
                                            reverse,
                                        } = s.action()
                                        {
                                            s.bind(
                                                Action::Adjust {
                                                    target,
                                                    step,
                                                    reverse: !reverse,
                                                },
                                                w,
                                                cx,
                                            );
                                        }
                                    }),
                                )),
                        );
                    if matches!(target, AdjustTarget::Custom { .. }) {
                        panel = panel
                            .child("Increase chord")
                            .child(Input::new(&self.fields[0]))
                            .child("Decrease chord")
                            .child(Input::new(&self.fields[1]));
                    }
                }
            } else {
                panel = panel.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("none")
                                .outline()
                                .selected(matches!(selected_action, Action::None))
                                .when(matches!(selected_action, Action::None), |b| {
                                    b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                                })
                                .label("Not assigned")
                                .on_click(cx.listener(|s, _, w, cx| s.bind(Action::None, w, cx))),
                        )
                        .child(
                            Button::new("keys")
                                .outline()
                                .selected(matches!(selected_action, Action::Keys { .. }))
                                .when(matches!(selected_action, Action::Keys { .. }), |b| {
                                    b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                                })
                                .label("Custom keys")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.bind(
                                        Action::Keys {
                                            keys: "Ctrl+Shift+T".into(),
                                        },
                                        w,
                                        cx,
                                    )
                                })),
                        )
                        .child(
                            Button::new("launch")
                                .outline()
                                .selected(matches!(selected_action, Action::Launch { .. }))
                                .when(matches!(selected_action, Action::Launch { .. }), |b| {
                                    b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                                })
                                .label("Launch app")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.bind(
                                        Action::Launch {
                                            path: "explorer.exe".into(),
                                            args: None,
                                        },
                                        w,
                                        cx,
                                    )
                                })),
                        )
                        .child(
                            Button::new("url")
                                .outline()
                                .selected(matches!(selected_action, Action::Url { .. }))
                                .when(matches!(selected_action, Action::Url { .. }), |b| {
                                    b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                                })
                                .label("Open URL")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.bind(
                                        Action::Url {
                                            url: "https://example.com".into(),
                                        },
                                        w,
                                        cx,
                                    )
                                })),
                        ),
                );
                match &selected_action {
                    Action::Keys { .. } => {
                        panel = panel
                            .child(Input::new(&self.fields[0]))
                            .child(
                                Button::new("record")
                                    .outline()
                                    .label(if self.recording {
                                        "Press a key combination… (Esc cancels)"
                                    } else {
                                        "Record shortcut"
                                    })
                                    .on_click(cx.listener(|s, _, w, cx| {
                                        s.recording = true;
                                        w.focus(&s.focus);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new("win")
                                    .label("Include Win key (for shortcuts Windows intercepts)")
                                    .checked(self.win)
                                    .on_click(cx.listener(|s, v, _, cx| {
                                        s.win = *v;
                                        cx.notify();
                                    })),
                            );
                    }
                    Action::Launch { .. } => {
                        panel = panel
                            .child(Input::new(&self.fields[0]))
                            .child(Input::new(&self.fields[1]))
                            .child(
                                Button::new("browse")
                                    .outline()
                                    .label("Choose application or document…")
                                    .on_click(cx.listener(|_s, _, w, cx| {
                                        let paths = cx.prompt_for_paths(PathPromptOptions {
                                            files: true,
                                            directories: false,
                                            multiple: false,
                                            prompt: Some("Choose".into()),
                                        });
                                        cx.spawn_in(w, async move |this, cx| {
                                            if let Ok(Ok(Some(paths))) = paths.await {
                                                let _ = this.update_in(cx, |s, w, cx| {
                                                    s.bind(
                                                        Action::Launch {
                                                            path: paths[0].to_string_lossy().into(),
                                                            args: None,
                                                        },
                                                        w,
                                                        cx,
                                                    );
                                                });
                                            }
                                        })
                                        .detach();
                                    })),
                            );
                    }
                    Action::Url { .. } => panel = panel.child(Input::new(&self.fields[0])),
                    _ => {}
                }
                let mut groups = unique_groups(presets().iter().map(|p| p.group));
                // Put common corner actions first; keep group order stable while editing.
                groups.sort_by_key(|group| *group != "Media");
                for group in groups {
                    panel = panel.child(div().mt_3().text_sm().text_color(rgb(muted)).child(group));
                    let mut buttons = div()
                        .flex_shrink_0()
                        .p_3()
                        .bg(rgb(card))
                        .border_1()
                        .border_color(rgb(line))
                        .rounded_lg()
                        .flex()
                        .flex_wrap()
                        .gap_2();
                    for p in presets().iter().filter(|p| p.group == group) {
                        let name = p.name;
                        let selected = matches!(selected_action, Action::Preset { name: ref current } if current == name);
                        buttons = buttons.child(
                            Button::new(name)
                                .outline()
                                .selected(selected)
                                .when(selected, |b| {
                                    b.border_color(rgb(accent)).bg(rgba((accent << 8) | 0x1f))
                                })
                                .small()
                                .label(p.label)
                                .on_click(cx.listener(move |s, _, w, cx| {
                                    s.bind(Action::Preset { name: name.into() }, w, cx)
                                })),
                        );
                    }
                    panel = panel.child(buttons);
                }
            }
        }
        let warning = self.model.borrow().warning.clone();
        div()
            .size_full()
            .font_family("Segoe UI Variable")
            .text_size(px(14.))
            .text_color(rgb(fg))
            .bg(rgb(bg))
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|s, e: &KeyDownEvent, w, cx| {
                if !s.recording {
                    return;
                }
                cx.stop_propagation();
                let key = &e.keystroke.key;
                if key == "escape" {
                    s.recording = false;
                    cx.notify();
                    return;
                }
                if ["shift", "control", "alt", "win", "super"].contains(&key.as_str()) {
                    return;
                }
                let m = e.keystroke.modifiers;
                let mut keys = Vec::new();
                if m.control {
                    keys.push("Ctrl".into());
                }
                if m.alt {
                    keys.push("Alt".into());
                }
                if m.shift {
                    keys.push("Shift".into());
                }
                if m.platform || s.win {
                    keys.push("Win".into());
                }
                keys.push(match key.as_str() {
                    "escape" => "Esc".into(),
                    "pageup" => "PageUp".into(),
                    "pagedown" => "PageDown".into(),
                    " " => "Space".into(),
                    _ => {
                        let mut c = key.chars();
                        c.next()
                            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                            .unwrap_or_default()
                    }
                });
                s.recording = false;
                s.bind(
                    Action::Keys {
                        keys: keys.join("+"),
                    },
                    w,
                    cx,
                );
            }))
            .child(
                div()
                    .h(px(64.))
                    .px_6()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(line))
                    .child(div().text_lg().child(crate::PRODUCT_NAME))
                    .child(
                        Switch::new("global-enabled")
                            .checked(config.enabled)
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.toggle("enabled");
                                cx.notify();
                            }))
                            .label(if config.enabled {
                                "Gestures enabled"
                            } else {
                                "Gestures paused"
                            }),
                    ),
            )
            .child(div().flex().flex_1().min_h_0().child(nav).child(panel))
            .child(div().px_6().py_2().text_xs().text_color(rgb(muted)).child(
                if warning.is_empty() {
                    "Changes save automatically".into()
                } else {
                    warning
                },
            ))
    }
}

fn unique_groups(items: impl Iterator<Item = &'static str>) -> Vec<&'static str> {
    let mut groups = Vec::new();
    for g in items {
        if !groups.contains(&g) {
            groups.push(g);
        }
    }
    groups
}

fn setting_card(card: u32, line: u32) -> Div {
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .p_4()
        .rounded_lg()
        .bg(rgb(card))
        .border_1()
        .border_color(rgb(line))
}
fn setting_text(title: &str, description: &str, muted: u32) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(title.to_owned()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(muted))
                .child(description.to_owned()),
        )
}
fn adjust_info(target: &AdjustTarget) -> &'static AdjustInfo {
    let id = match target {
        AdjustTarget::Volume => "volume",
        AdjustTarget::Brightness => "brightness",
        AdjustTarget::Seek => "seek",
        AdjustTarget::ScrollH => "scroll_h",
        AdjustTarget::Zoom => "zoom",
        AdjustTarget::Tabs => "tabs",
        AdjustTarget::Desktops => "desktops",
        AdjustTarget::Custom { .. } => "custom",
    };
    adjust_targets()
        .iter()
        .find(|info| info.target == id)
        .unwrap()
}
fn step_range(target: &AdjustTarget, trigger: &str) -> (f32, f32, f32) {
    if trigger == "twist4" {
        (5., 30., 0.5)
    } else if matches!(target, AdjustTarget::Brightness) {
        (0.3, 3., 0.01)
    } else {
        let default = adjust_info(target).default_step as f32;
        (1., (default * 2.5).max(10.), 0.1)
    }
}
