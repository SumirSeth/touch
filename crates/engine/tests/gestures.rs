use gestures_engine::{gestures::*, *};
fn frame(n: usize, r: f64, deg: f64, cx: f64, cy: f64, time: u64) -> Frame {
    Frame {
        touching: (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64 + deg.to_radians();
                Contact {
                    id: i as u32,
                    x: cx + r * a.cos(),
                    y: cy + r * a.sin(),
                }
            })
            .collect(),
        width_mm: 126.9,
        height_mm: 79.3,
        time_ms: time,
        button: false,
    }
}
fn hand(n: usize, r: f64, deg: f64) -> Frame {
    frame(n, r, deg, 60., 40., 0)
}
fn one(x: f64, y: f64, t: u64) -> Frame {
    frame(1, 0., 0., x, y, t)
}
fn lift(t: u64) -> Frame {
    frame(0, 0., 0., 0., 0., t)
}
fn all() -> Config {
    let mut c = Config::default();
    for t in triggers() {
        c.bindings.insert(
            t.id.into(),
            if t.continuous {
                Action::Adjust {
                    target: AdjustTarget::Volume,
                    step: if t.id == "twist4" { 12. } else { 1. },
                    reverse: false,
                }
            } else {
                Action::Preset {
                    name: "mute".into(),
                }
            },
        );
    }
    c
}
fn run(c: Config, frames: impl IntoIterator<Item = Frame>) -> Vec<GestureEvent> {
    let mut g = Recognizer::new(c);
    frames.into_iter().flat_map(|f| g.on_frame(&f)).collect()
}
fn fires(events: &[GestureEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|e| {
            if let GestureEvent::Fired(id) = e {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect()
}
fn steps(events: &[GestureEvent]) -> i32 {
    events
        .iter()
        .map(|e| {
            if let GestureEvent::Adjusted { steps, .. } = e {
                *steps
            } else {
                0
            }
        })
        .sum()
}
#[test]
fn five_finger_prototype_cases() {
    for (start, end, expected) in [
        (30, 20, Some("pinch5_in")),
        (30, 21, None),
        (15, 23, Some("pinch5_out")),
    ] {
        let range: Vec<i32> = if start > end {
            (end..=start).rev().collect()
        } else {
            (start..=end).collect()
        };
        let e = run(all(), range.into_iter().map(|r| hand(5, r as f64, 0.)));
        assert_eq!(fires(&e), expected.into_iter().collect::<Vec<_>>());
    }
    for (degrees, expected) in [
        (30i32, Some("twist5_cw")),
        (-30, Some("twist5_ccw")),
        (20, None),
    ] {
        let e = run(
            all(),
            (0..=degrees.abs()).map(|d| hand(5, 25., (d * degrees.signum()) as f64)),
        );
        assert_eq!(fires(&e), expected.into_iter().collect::<Vec<_>>());
    }
    let e = run(
        all(),
        (10..=30)
            .rev()
            .map(|r| hand(5, r as f64, 0.))
            .chain([lift(0)])
            .chain((0..=30).map(|d| hand(5, 25., d as f64))),
    );
    assert_eq!(fires(&e), ["pinch5_in", "twist5_cw"]);
    assert!(fires(&run(all(), (10..=30).rev().map(|r| hand(4, r as f64, 0.)))).is_empty());
}
#[test]
fn knob_prototype_cases() {
    for (degrees, expected) in [(50i32, 4), (-40, -3), (10, 0)] {
        let e = run(
            all(),
            (0..=degrees.abs()).map(|d| hand(4, 20., (d * degrees.signum()) as f64)),
        );
        assert_eq!(steps(&e), expected);
    }
    let e = run(
        all(),
        (0..=40)
            .chain((-2..=40).rev())
            .map(|d| hand(4, 20., d as f64)),
    );
    let v: Vec<_> = e
        .iter()
        .filter_map(|e| {
            if let GestureEvent::Adjusted { steps, .. } = e {
                Some(*steps)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(v.iter().filter(|v| **v > 0).sum::<i32>(), 3);
    assert_eq!(v.iter().filter(|v| **v < 0).sum::<i32>(), -3);
    let e = run(
        all(),
        (0..=15)
            .map(|x| frame(4, 20., 0., 60. + x as f64, 40., 0))
            .chain((0..=40).map(|d| frame(4, 20., d as f64, 75., 40., 0))),
    );
    assert_eq!(steps(&e), 0);
}
#[test]
fn drag_prototype_cases() {
    let mut c = all();
    c.drag3 = true;
    let e = run(
        c.clone(),
        (0..=10)
            .map(|x| frame(3, 10., 0., 60. + x as f64, 40., 0))
            .chain([lift(0)]),
    );
    assert_eq!(e.first(), Some(&GestureEvent::DragStart));
    assert!(e.contains(&GestureEvent::DragMove(18, 0)));
    assert_eq!(e.last(), Some(&GestureEvent::DragEnd));
    assert!(fires(&e).is_empty());
    assert!(run(c.clone(), [hand(3, 10., 0.), lift(100)]).is_empty());
    assert!(run(
        c.clone(),
        [hand(4, 10., 0.), frame(4, 10., 0., 65., 40., 0)]
    )
    .is_empty());
    let e = run(
        c,
        [
            hand(3, 10., 0.),
            frame(3, 10., 0., 65., 40., 0),
            hand(2, 10., 0.),
            hand(3, 10., 0.),
        ],
    );
    assert_eq!(e.iter().filter(|e| **e == GestureEvent::DragEnd).count(), 1);
    assert_eq!(
        e.iter().filter(|e| **e == GestureEvent::DragStart).count(),
        1
    );
}
#[test]
fn all_swipes_and_thresholds() {
    for n in [3, 4] {
        for (dx, dy, d) in [
            (12.1, 0., "right"),
            (-12.1, 0., "left"),
            (0., -12.1, "up"),
            (0., 12.1, "down"),
        ] {
            let e = run(
                all(),
                [
                    frame(n, 10., 0., 60., 40., 0),
                    frame(n, 10., 0., 60. + dx, 40. + dy, 100),
                    frame(n, 10., 0., 60. + 2. * dx, 40. + 2. * dy, 150),
                    lift(200),
                ],
            );
            assert_eq!(fires(&e), [format!("swipe{n}_{d}")]);
        }
        for (dx, dy) in [(11.9, 0.), (12., 10.)] {
            assert!(fires(&run(
                all(),
                [hand(n, 10., 0.), frame(n, 10., 0., 60. + dx, 40. + dy, 100)]
            ))
            .is_empty());
        }
        let e = run(
            all(),
            [
                hand(n, 10., 0.),
                hand(n - 1, 10., 0.),
                frame(n, 10., 0., 80., 40., 100),
            ],
        );
        assert!(fires(&e).is_empty());
    }
    assert!(run(all(), [hand(2, 10., 0.), frame(2, 10., 0., 80., 40., 0)]).is_empty());
}
#[test]
fn taps_timing_motion_landing_and_lifts() {
    for n in [3, 4] {
        assert_eq!(
            fires(&run(all(), [hand(n, 10., 0.), lift(200)])),
            [format!("tap{n}")]
        );
        for time in [250, 300] {
            assert!(fires(&run(all(), [hand(n, 10., 0.), lift(time)])).is_empty());
        }
        assert!(fires(&run(
            all(),
            [
                hand(n, 10., 0.),
                frame(n, 10., 0., 62.1, 40., 100),
                lift(150)
            ]
        ))
        .is_empty());
        // Stable IDs during gradual landing and lifting.
        let full = hand(n, 10., 0.);
        let mut partial = full.clone();
        partial.touching.truncate(1);
        let mut late = full.clone();
        late.time_ms = 150;
        let mut lifting = full.clone();
        lifting.time_ms = 180;
        lifting.touching.truncate(2);
        assert_eq!(
            fires(&run(
                all(),
                [partial.clone(), late.clone(), lifting, lift(200)]
            )),
            [format!("tap{n}")]
        );
        late.time_ms = 151;
        assert!(fires(&run(all(), [partial, late, lift(200)])).is_empty());
    }
}
#[test]
fn edge_prototype_and_new_left() {
    for (id, start, end, expected) in [
        ("edge_right", (123.9, 60.), (123.9, 30.), 44),
        ("edge_right", (123.9, 30.), (123.9, 50.), -29),
        ("edge_top", (40., 3.), (60., 3.), 5),
        ("edge_bottom", (60., 76.3), (45., 76.3), -5),
    ] {
        let e = run(
            Config::default(),
            [one(start.0, start.1, 0), one(end.0, end.1, 100), lift(200)],
        );
        assert_eq!(steps(&e), expected, "{id}");
        assert!(e.contains(&GestureEvent::Freeze(true)));
        assert!(e.contains(&GestureEvent::Freeze(false)));
    }
    let e = run(all(), [one(3., 60., 0), one(3., 30., 100), lift(200)]);
    assert_eq!(steps(&e), 30);
    for frames in [
        [one(40., 40., 0), one(60., 40., 100)],
        [one(123.9, 40., 0), one(103.9, 40., 100)],
    ] {
        assert!(run(all(), frames).is_empty());
    }
    for end in [hand(2, 10., 0.), one(100., 30., 100)] {
        let e = run(all(), [one(123.9, 60., 0), one(123.9, 40., 50), end]);
        assert_eq!(e.last(), Some(&GestureEvent::Freeze(false)));
    }
}
#[test]
fn corners_all_four_and_cancels() {
    for (x, y, id) in [
        (5., 5., "corner_top_left"),
        (121.9, 5., "corner_top_right"),
        (5., 74.3, "corner_bottom_left"),
        (121.9, 74.3, "corner_bottom_right"),
    ] {
        assert_eq!(
            fires(&run(all(), [one(x, y, 0), one(x, y, 450), one(x, y, 500)])),
            [id]
        );
        assert!(fires(&run(all(), [one(x, y, 0), one(x, y, 300), lift(300)])).is_empty());
        assert!(fires(&run(
            all(),
            [one(x, y, 0), one(x + 2., y, 200), one(x, y, 500)]
        ))
        .is_empty());
    }
}
#[test]
fn button_cancel_and_reset_cleanup() {
    let mut pressed = one(123.9, 30., 100);
    pressed.button = true;
    let mut g = Recognizer::new(all());
    g.on_frame(&one(123.9, 60., 0));
    assert!(g
        .on_frame(&one(123.9, 50., 50))
        .contains(&GestureEvent::Freeze(true)));
    assert_eq!(g.on_frame(&pressed), [GestureEvent::Freeze(false)]);
    assert!(g.on_frame(&one(123.9, 20., 200)).is_empty());
    g.on_frame(&lift(250));
    g.on_frame(&one(123.9, 60., 300));
    g.on_frame(&one(123.9, 50., 350));
    assert_eq!(g.reset(), [GestureEvent::Freeze(false)]);
    for n in [3, 4, 5] {
        let mut f = hand(n, 30., 0.);
        f.button = true;
        assert!(run(all(), [f, hand(n, 15., 40.), lift(100)]).is_empty());
    }
}
#[test]
fn unbound_triggers_do_not_claim() {
    let mut c = all();
    c.bindings.remove("swipe4_right");
    let e = run(
        c,
        [
            hand(4, 20., 0.),
            frame(4, 20., 0., 72., 40., 100),
            frame(4, 20., 40., 72., 40., 200),
        ],
    );
    assert!(fires(&e).is_empty()); // translation disables knob independently
    let mut c = all();
    c.bindings.remove("pinch5_in");
    let e = run(c, [hand(5, 30., 0.), hand(5, 15., 0.), hand(5, 15., 30.)]);
    assert_eq!(fires(&e), ["twist5_cw"]);
    let mut c = all();
    c.bindings.clear();
    assert!(run(
        c,
        [
            one(3., 60., 0),
            one(3., 30., 100),
            lift(200),
            hand(5, 30., 0.),
            hand(5, 10., 40.)
        ]
    )
    .is_empty());
}
#[test]
fn reverse_and_fractional_steps() {
    let mut c = all();
    c.bindings.insert(
        "edge_left".into(),
        Action::Adjust {
            target: AdjustTarget::Volume,
            step: 4.,
            reverse: true,
        },
    );
    assert_eq!(
        steps(&run(
            c,
            [
                one(3., 60., 0),
                one(3., 57., 10),
                one(3., 54., 20),
                one(3., 52., 30)
            ]
        )),
        -2
    );
}
