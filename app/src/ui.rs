use gestures_engine::Action;
use gestures_engine::*;
use gpui::{prelude::*, *};
use gpui_component::{
    Sizable,
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
    pub flash: Option<(String, Instant)>,
}
impl Model {
    pub fn save(&mut self) {
        let errors = validate(&self.config);
        if !errors.is_empty() {
            self.warning = errors.join(" · ");
            return;
        }
        match save_config(&self.config) {
            Ok(()) => {
                self.warning.clear();
                if let Some(e) = self.engine.borrow().as_ref() {
                    e.set_config(self.config.clone());
                }
            }
            Err(e) => self.warning = e.to_string(),
        }
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
        this.sync_fields(w, cx);
        this
    }
    fn sync_fields(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let a = self.action();
        if let Action::Adjust { step, .. } = &a {
            self.slider
                .update(cx, |s, cx| s.set_value(*step as f32, w, cx));
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
        Action::Adjust { target, .. } => format!("Adjust {target:?}"),
    }
}
impl Render for Settings {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (light, accent) = crate::platform::appearance();
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
        let config = self.model.borrow().config.clone();
        let mut nav = div()
            .id("gestures")
            .w(px(325.))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .border_r_1()
            .border_color(rgb(line));
        nav = nav.child(
            Button::new("general")
                .label("General settings")
                .on_click(cx.listener(|s, _, _, cx| {
                    s.general = true;
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
                            "edge_left" => div().absolute().left_0().h_full().w(px(3.)),
                            "edge_right" => div().absolute().right_0().h_full().w(px(3.)),
                            "edge_top" => div().absolute().top_0().w_full().h(px(3.)),
                            _ => div().absolute().bottom_0().w_full().h(px(3.)),
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
                        Button::new(id)
                            .h(px(60.))
                            .relative()
                            .rounded_lg()
                            .p_2()
                            .flex()
                            .items_center()
                            .gap_3()
                            .bg(rgb(if flash {
                                accent
                            } else if selected {
                                if light { 0xe3eef8 } else { 0x30475c }
                            } else {
                                bg
                            }))
                            .cursor_pointer()
                            .on_click(cx.listener(move |s, _, w, cx| {
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
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .p_8()
            .flex()
            .flex_col()
            .gap_4();
        if self.general {
            panel = panel.child(div().text_2xl().child("General")).child(
                div()
                    .text_color(rgb(muted))
                    .child("Make your touchpad feel like yours."),
            );
            for (id, label, on) in [
                ("enabled", "Enable gestures", config.enabled),
                (
                    "drag",
                    "Three-finger drag (disables 3-finger swipes and tap)",
                    config.drag3,
                ),
                (
                    "freeze",
                    "Freeze pointer during edge slides",
                    config.freeze_cursor,
                ),
            ] {
                panel = panel.child(
                    div().p_4().bg(rgb(card)).rounded_lg().child(
                        Switch::new(id)
                            .label(label)
                            .checked(on)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                s.toggle(id);
                                cx.notify();
                            })),
                    ),
                );
            }
            for (id, label, value) in [
                ("edge", "Edge width", config.edge_width_mm),
                ("swipe", "Swipe distance", config.swipe_distance_mm),
            ] {
                panel = panel.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(format!("{label}: {value:.1} mm"))
                        .children([-1., 1.].into_iter().enumerate().map(|(i, delta)| {
                            Button::new((id, i))
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
                );
            }
            panel =
                panel
                    .child(
                        Switch::new("autostart")
                            .label("Start with Windows")
                            .checked(crate::platform::autostart())
                            .on_click(cx.listener(|s, on, _, cx| {
                                if let Err(e) = crate::platform::set_autostart(*on) {
                                    s.model.borrow_mut().warning = e.to_string();
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("reset")
                            .label("Reset to defaults")
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.model.borrow_mut().config = Config::default();
                                s.model.borrow_mut().save();
                                s.sync_fields(w, cx);
                            })),
                    )
                    .child(Button::new("folder").label("Open config folder").on_click(
                        |_, _, cx| cx.open_url(&config_path().parent().unwrap().to_string_lossy()),
                    ))
                    .child(concat!("Touch ", env!("CARGO_PKG_VERSION")));
        } else {
            let t = triggers().iter().find(|t| t.id == self.selected).unwrap();
            panel = panel
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(accent))
                        .child(t.group.to_uppercase()),
                )
                .child(div().text_2xl().child(t.label))
                .child(div().text_color(rgb(muted)).child(t.description));
            if config.drag3 && (t.id.starts_with("swipe3") || t.id == "tap3") {
                panel = panel.child("Disabled while three-finger drag is on.");
            }
            for conflict in windows_conflicts(&config)
                .iter()
                .filter(|c| t.id.starts_with(c.trigger_prefix))
            {
                panel = panel.child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(rgb(if light { 0xfff4ce } else { 0x493c20 }))
                        .child(conflict.message.clone())
                        .child(
                            Button::new("windows-settings")
                                .label("Open Windows touchpad settings")
                                .on_click(|_, _, cx| cx.open_url("ms-settings:devices-touchpad")),
                        ),
                );
            }
            panel = panel.child(
                div()
                    .text_sm()
                    .child(format!("Current action: {}", caption(&self.action()))),
            );
            if t.continuous {
                panel = panel.child(
                    Button::new("unbound")
                        .label("Not assigned")
                        .on_click(cx.listener(|s, _, w, cx| s.bind(Action::None, w, cx))),
                );
                let mut targets = div().flex().flex_wrap().gap_2();
                for info in adjust_targets() {
                    let target = info.target;
                    targets = targets.child(
                        Button::new(target)
                            .outline()
                            .w(px(154.))
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
                                        step: if s.selected == "twist4" { 12. } else { 4. },
                                        reverse: false,
                                    },
                                    w,
                                    cx,
                                );
                            })),
                    );
                }
                panel = panel.child(targets);
                if let Action::Adjust {
                    step,
                    reverse,
                    target,
                } = self.action()
                {
                    panel = panel
                        .child(format!(
                            "Step: {step:.2} {}",
                            if t.id == "twist4" { "degrees" } else { "mm" }
                        ))
                        .child(Slider::new(&self.slider))
                        .child(
                            Switch::new("reverse")
                                .label("Reverse direction")
                                .checked(reverse)
                                .on_click(cx.listener(|s, _, w, cx| {
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
                                })),
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
                                .label("None")
                                .on_click(cx.listener(|s, _, w, cx| s.bind(Action::None, w, cx))),
                        )
                        .child(
                            Button::new("keys")
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
                        .child(Button::new("url").label("Open URL").on_click(cx.listener(
                            |s, _, w, cx| {
                                s.bind(
                                    Action::Url {
                                        url: "https://example.com".into(),
                                    },
                                    w,
                                    cx,
                                )
                            },
                        ))),
                );
                match self.action() {
                    Action::Keys { .. } => {
                        panel = panel
                            .child(Input::new(&self.fields[0]))
                            .child(
                                Button::new("record")
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
                panel = panel.child(Button::new("test").label("Test action").on_click(
                    cx.listener(|s, _, _, _| {
                        if let Some(e) = s.model.borrow().engine.borrow().as_ref() {
                            e.run_action(&s.action());
                        }
                    }),
                ));
                for group in unique_groups(presets().iter().map(|p| p.group)) {
                    panel = panel.child(div().mt_3().text_sm().text_color(rgb(muted)).child(group));
                    let mut buttons = div().flex().flex_wrap().gap_2();
                    for p in presets().iter().filter(|p| p.group == group) {
                        let name = p.name;
                        buttons = buttons.child(Button::new(name).small().label(p.label).on_click(
                            cx.listener(move |s, _, w, cx| {
                                s.bind(Action::Preset { name: name.into() }, w, cx)
                            }),
                        ));
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
                        div()
                            .text_sm()
                            .text_color(rgb(muted))
                            .child(if config.enabled {
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
