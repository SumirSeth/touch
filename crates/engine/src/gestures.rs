//! Deterministic recognizers. Coordinates are millimetres from the top left;
//! timestamps are monotonic milliseconds supplied by the caller.
use crate::{Action, Config};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub id: u32,
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone)]
pub struct Frame {
    pub touching: Vec<Contact>,
    pub width_mm: f64,
    pub height_mm: f64,
    pub time_ms: u64,
    pub button: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub enum GestureEvent {
    Fired(String),
    Adjusted { trigger: String, steps: i32 },
    Freeze(bool),
    DragStart,
    DragMove(i32, i32),
    DragEnd,
}
fn center(c: &[Contact]) -> (f64, f64) {
    let n = c.len() as f64;
    (
        c.iter().map(|p| p.x).sum::<f64>() / n,
        c.iter().map(|p| p.y).sum::<f64>() / n,
    )
}
fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}
fn spread(c: &[Contact]) -> f64 {
    let m = center(c);
    c.iter().map(|p| distance((p.x, p.y), m)).sum::<f64>() / c.len() as f64
}
fn rotation(a: &[Contact], b: &[Contact]) -> f64 {
    let ca = center(a);
    let cb = center(b);
    let (mut sum, mut weight) = (0., 0.);
    for p in b {
        if let Some(q) = a.iter().find(|q| q.id == p.id) {
            let r = distance((p.x, p.y), cb);
            let d = (p.y - cb.1).atan2(p.x - cb.0) - (q.y - ca.1).atan2(q.x - ca.0);
            sum += d.sin().atan2(d.cos()) * r;
            weight += r;
        }
    }
    if weight == 0. {
        0.
    } else {
        (sum / weight).to_degrees()
    }
}
#[derive(Default)]
struct Multi {
    start: Option<(f64, f64)>,
    off: bool,
}
/// Owns a touch only after a bound trigger fires. Config is fixed for a session.
pub struct Recognizer {
    config: Config,
    active: bool,
    owner: Option<String>,
    cancelled: bool,
    first: Option<Frame>,
    n: usize,
    swipes: [Multi; 2],
    max_spread: f64,
    min_spread: f64,
    five_rotation: f64,
    five_prev: Vec<Contact>,
    knob_prev: Vec<Contact>,
    knob_start: Option<(f64, f64)>,
    knob_off: bool,
    knob_rotation: f64,
    edge_candidates: Vec<&'static str>,
    edge_start: (f64, f64),
    edge_last: f64,
    carry: f64,
    edge_off: bool,
    frozen: bool,
    corner: Option<String>,
    corner_off: bool,
    tap_points: BTreeMap<u32, (f64, f64)>,
    tap_count: usize,
    tap_off: bool,
    tap_lifting: bool,
    drag_start: Option<(f64, f64)>,
    drag_last: (f64, f64),
    drag_carry: (f64, f64),
    drag_off: bool,
    dragging: bool,
}
impl Recognizer {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            active: false,
            owner: None,
            cancelled: false,
            first: None,
            n: 0,
            swipes: Default::default(),
            max_spread: 0.,
            min_spread: 0.,
            five_rotation: 0.,
            five_prev: vec![],
            knob_prev: vec![],
            knob_start: None,
            knob_off: false,
            knob_rotation: 0.,
            edge_candidates: vec![],
            edge_start: (0., 0.),
            edge_last: 0.,
            carry: 0.,
            edge_off: false,
            frozen: false,
            corner: None,
            corner_off: false,
            tap_points: BTreeMap::new(),
            tap_count: 0,
            tap_off: false,
            tap_lifting: false,
            drag_start: None,
            drag_last: (0., 0.),
            drag_carry: (0., 0.),
            drag_off: false,
            dragging: false,
        }
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) -> Vec<GestureEvent> {
        let mut out = vec![];
        if self.frozen {
            out.push(GestureEvent::Freeze(false));
        }
        if self.dragging {
            out.push(GestureEvent::DragEnd);
        }
        *self = Self::new(self.config.clone());
        out
    }
    fn bound(&self, id: &str) -> bool {
        self.config
            .bindings
            .get(id)
            .is_some_and(|a| !matches!(a, Action::None))
    }
    fn fire(&mut self, id: &str, out: &mut Vec<GestureEvent>) {
        if self.owner.is_none() && self.bound(id) {
            self.owner = Some(id.into());
            out.push(GestureEvent::Fired(id.into()));
        }
    }
    fn adjust(&mut self, id: &str, amount: f64, out: &mut Vec<GestureEvent>) {
        if let Some(Action::Adjust { step, reverse, .. }) = self.config.bindings.get(id) {
            if *step <= 0. || !step.is_finite() {
                return;
            }
            self.carry += amount / step;
            let steps = self.carry.trunc() as i32;
            self.carry -= steps as f64;
            if steps != 0 {
                out.push(GestureEvent::Adjusted {
                    trigger: id.into(),
                    steps: if *reverse { -steps } else { steps },
                });
            }
        }
    }
    pub fn on_frame(&mut self, f: &Frame) -> Vec<GestureEvent> {
        let mut out = vec![];
        let n = f.touching.len();
        if n == 0 {
            if !self.active {
                return out;
            }
            if self.active
                && !self.cancelled
                && self.owner.is_none()
                && !self.tap_off
                && f.time_ms
                    .saturating_sub(self.first.as_ref().unwrap().time_ms)
                    < 250
            {
                let id = format!("tap{}", self.tap_count);
                if self.tap_count == 4 || (self.tap_count == 3 && !self.config.drag3) {
                    self.fire(&id, &mut out);
                }
            }
            out.extend(self.reset());
            return out;
        }
        if !self.config.enabled {
            return out;
        }
        if !self.active {
            self.active = true;
            self.first = Some(f.clone());
            if n == 1 {
                let p = &f.touching[0];
                self.edge_start = (p.x, p.y);
                for (id, on) in [
                    ("edge_right", p.x >= f.width_mm - self.config.edge_width_mm),
                    ("edge_left", p.x <= self.config.edge_width_mm),
                    ("edge_top", p.y <= self.config.edge_width_mm),
                    (
                        "edge_bottom",
                        p.y >= f.height_mm - self.config.edge_width_mm,
                    ),
                ] {
                    if on && self.bound(id) {
                        self.edge_candidates.push(id);
                    }
                }
                let side = if p.x <= 12. {
                    Some("left")
                } else if p.x >= f.width_mm - 12. {
                    Some("right")
                } else {
                    None
                };
                let vertical = if p.y <= 12. {
                    Some("top")
                } else if p.y >= f.height_mm - 12. {
                    Some("bottom")
                } else {
                    None
                };
                if let (Some(s), Some(v)) = (side, vertical) {
                    self.corner = Some(format!("corner_{v}_{s}"));
                }
            }
        }
        if f.button {
            if self.frozen {
                out.push(GestureEvent::Freeze(false));
                self.frozen = false;
            }
            if self.dragging {
                out.push(GestureEvent::DragEnd);
                self.dragging = false;
            }
            self.cancelled = true;
        }
        if self.cancelled {
            return out;
        }
        if self.bound("tap4") || (!self.config.drag3 && self.bound("tap3")) {
            // A tap may lift fingers in separate reports, but may never add fingers after lifting.
            if n < self.n {
                self.tap_lifting = true;
            }
            for p in &f.touching {
                if !self.tap_points.contains_key(&p.id) {
                    if self.tap_lifting
                        || f.time_ms
                            .saturating_sub(self.first.as_ref().unwrap().time_ms)
                            > 150
                    {
                        self.tap_off = true;
                    }
                    self.tap_points.insert(p.id, (p.x, p.y));
                }
                if distance(self.tap_points[&p.id], (p.x, p.y)) >= 2. {
                    self.tap_off = true;
                }
            }
            self.tap_count = self.tap_count.max(self.tap_points.len());
        }
        let c = center(&f.touching);
        // End continuous ownership when contacts change; ownership persists until all lift.
        if let Some(id) = self.owner.take() {
            if id.starts_with("edge_") {
                if n != 1 || !on_edge(&id, &f.touching[0], f, self.config.edge_width_mm) {
                    if self.frozen {
                        out.push(GestureEvent::Freeze(false));
                        self.frozen = false;
                    }
                    self.cancelled = true;
                } else {
                    let pos = edge_pos(&id, &f.touching[0]);
                    self.adjust(&id, pos - self.edge_last, &mut out);
                    self.edge_last = pos;
                }
            } else if id == "twist4" {
                if n != 4 {
                    self.cancelled = true;
                } else {
                    let r = rotation(&self.knob_prev, &f.touching);
                    self.adjust(&id, r, &mut out);
                    self.knob_prev.clone_from(&f.touching);
                }
            } else if id == "drag3" {
                if n != 3 {
                    out.push(GestureEvent::DragEnd);
                    self.dragging = false;
                    self.cancelled = true;
                } else {
                    self.drag_move(c, &mut out);
                }
            }
            self.owner = Some(id);
            self.n = n;
            return out;
        }
        // Edges engage at 3 mm, with a 2:1 direction bias, preserving travel before engagement.
        if n != 1 {
            self.edge_off = true;
            self.corner_off = true;
        }
        if n == 1 {
            let p = &f.touching[0];
            let dx = p.x - self.edge_start.0;
            let dy = p.y - self.edge_start.1;
            if !self.edge_off {
                self.edge_candidates
                    .retain(|id| on_edge(id, p, f, self.config.edge_width_mm));
                for i in 0..self.edge_candidates.len() {
                    let id = self.edge_candidates[i];
                    if !on_edge(id, p, f, self.config.edge_width_mm) {
                        continue;
                    }
                    let vertical = id == "edge_left" || id == "edge_right";
                    let (a, b) = if vertical { (dy, dx) } else { (dx, dy) };
                    if a.abs() >= 3. && a.abs() > 2. * b.abs() {
                        self.owner = Some(id.to_string());
                        self.frozen = self.config.freeze_cursor;
                        if self.frozen {
                            out.push(GestureEvent::Freeze(true));
                        }
                        self.edge_last = edge_pos(id, p);
                        self.adjust(id, if vertical { -dy } else { dx }, &mut out);
                        break;
                    }
                }
                if dx.hypot(dy) >= 6. && self.owner.is_none() {
                    self.edge_off = true;
                }
            }
            if self.owner.is_none() && !self.corner_off {
                if dx.hypot(dy) > 1.5 {
                    self.corner_off = true;
                } else if f
                    .time_ms
                    .saturating_sub(self.first.as_ref().unwrap().time_ms)
                    >= 450
                {
                    if let Some(id) = self.corner.clone() {
                        self.fire(&id, &mut out);
                    }
                }
            }
        }
        if self.owner.is_some() {
            self.n = n;
            return out;
        }
        if self.config.drag3 && !self.drag_off {
            if n == 3 {
                if let Some(start) = self.drag_start {
                    if distance(c, start) >= 1.5 {
                        self.owner = Some("drag3".into());
                        self.dragging = true;
                        out.push(GestureEvent::DragStart);
                        self.drag_move(c, &mut out);
                    }
                } else {
                    self.drag_start = Some(c);
                    self.drag_last = c;
                }
            } else if n > 3 {
                self.drag_off = true;
            } else {
                self.drag_start = None;
            }
        }
        for fingers in [3, 4] {
            let prefix = if fingers == 3 { "swipe3_" } else { "swipe4_" };
            if !self
                .config
                .bindings
                .iter()
                .any(|(id, action)| id.starts_with(prefix) && !matches!(action, Action::None))
            {
                continue;
            }
            let i = fingers - 3;
            if n != fingers {
                if self.swipes[i].start.is_some() {
                    self.swipes[i].off = true;
                }
                continue;
            }
            if fingers == 3 && self.config.drag3 {
                continue;
            }
            if self.swipes[i].off {
                continue;
            }
            if let Some(start) = self.swipes[i].start {
                let dx = c.0 - start.0;
                let dy = c.1 - start.1;
                let direction = if dx.abs() >= self.config.swipe_distance_mm
                    && dx.abs() > 1.5 * dy.abs()
                {
                    Some(if dx > 0. { "right" } else { "left" })
                } else if dy.abs() >= self.config.swipe_distance_mm && dy.abs() > 1.5 * dx.abs() {
                    Some(if dy > 0. { "down" } else { "up" })
                } else {
                    None
                };
                if let Some(d) = direction {
                    self.fire(&format!("swipe{fingers}_{d}"), &mut out);
                }
            } else {
                self.swipes[i].start = Some(c);
            }
        }
        if n == 4 && !self.knob_off && self.bound("twist4") {
            if let Some(start) = self.knob_start {
                self.knob_rotation += rotation(&self.knob_prev, &f.touching);
                if distance(start, c) > 10. {
                    self.knob_off = true;
                } else if let Some(Action::Adjust { step, .. }) = self.config.bindings.get("twist4")
                {
                    if self.knob_rotation.abs() >= *step && self.owner.is_none() {
                        self.owner = Some("twist4".into());
                        self.carry = 0.;
                        self.adjust("twist4", self.knob_rotation, &mut out);
                    }
                }
            } else {
                self.knob_start = Some(c);
            }
            self.knob_prev.clone_from(&f.touching);
        } else if self.knob_start.is_some() {
            self.knob_off = true;
        }
        if n >= 5
            && ["pinch5_in", "pinch5_out", "twist5_cw", "twist5_ccw"]
                .iter()
                .any(|id| self.bound(id))
        {
            let s = spread(&f.touching);
            if self.five_prev.is_empty() {
                self.max_spread = s;
                self.min_spread = s;
            } else {
                self.five_rotation += rotation(&self.five_prev, &f.touching);
                self.max_spread = self.max_spread.max(s);
                self.min_spread = self.min_spread.min(s);
                let id = if self.bound("pinch5_in")
                    && s <= self.max_spread * 0.685
                    && self.max_spread - s >= 7.2
                {
                    Some("pinch5_in")
                } else if self.bound("pinch5_out")
                    && s * 0.685 >= self.min_spread
                    && s - self.min_spread >= 7.2
                {
                    Some("pinch5_out")
                } else if self.bound("twist5_cw") && self.five_rotation >= 25. {
                    Some("twist5_cw")
                } else if self.bound("twist5_ccw") && self.five_rotation <= -25. {
                    Some("twist5_ccw")
                } else {
                    None
                };
                if let Some(id) = id {
                    self.fire(id, &mut out);
                }
            }
            self.five_prev.clone_from(&f.touching);
        } else {
            self.five_prev.clear();
            self.five_rotation = 0.;
        }
        self.n = n;
        out
    }
    fn drag_move(&mut self, c: (f64, f64), out: &mut Vec<GestureEvent>) {
        self.drag_carry.0 += (c.0 - self.drag_last.0) * 9.;
        self.drag_carry.1 += (c.1 - self.drag_last.1) * 9.;
        self.drag_last = c;
        let (x, y) = (self.drag_carry.0 as i32, self.drag_carry.1 as i32);
        self.drag_carry.0 -= x as f64;
        self.drag_carry.1 -= y as f64;
        if x != 0 || y != 0 {
            out.push(GestureEvent::DragMove(x, y));
        }
    }
}
fn edge_pos(id: &str, p: &Contact) -> f64 {
    if id == "edge_left" || id == "edge_right" {
        -p.y
    } else {
        p.x
    }
}
fn on_edge(id: &str, p: &Contact, f: &Frame, width: f64) -> bool {
    match id {
        "edge_left" => p.x <= width,
        "edge_right" => p.x >= f.width_mm - width,
        "edge_top" => p.y <= width,
        _ => p.y >= f.height_mm - width,
    }
}
