use gestures_engine::*;
#[test]
fn defaults_match_spec() {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/app-spec.md"
    ))
    .unwrap();
    let json = spec
        .split("```json")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let expected: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(
        default_config(),
        serde_json::from_value::<Config>(expected).unwrap()
    );
    assert!(validate(&Config::default()).is_empty());
}
#[test]
fn roundtrip_custom_and_unknown_fields() {
    let mut c = Config::default();
    c.bindings.insert(
        "edge_left".into(),
        Action::Adjust {
            target: AdjustTarget::Custom {
                increase: "Ctrl+Shift+T".into(),
                decrease: "Ctrl+W".into(),
            },
            step: 2.,
            reverse: true,
        },
    );
    let mut json = serde_json::to_value(&c).unwrap();
    assert_eq!(json["bindings"]["edge_left"]["target"], "custom");
    assert_eq!(json["bindings"]["edge_left"]["increase"], "Ctrl+Shift+T");
    json["future"] = true.into();
    assert_eq!(serde_json::from_value::<Config>(json).unwrap(), c);
}
#[test]
fn validation() {
    for (id, a) in [
        (
            "edge_left",
            Action::Keys {
                keys: "Ctrl+A".into(),
            },
        ),
        (
            "tap3",
            Action::Adjust {
                target: AdjustTarget::Volume,
                step: 0.,
                reverse: false,
            },
        ),
        (
            "tap3",
            Action::Keys {
                keys: "Ctrl+NotAKey".into(),
            },
        ),
        (
            "tap3",
            Action::Preset {
                name: "bogus".into(),
            },
        ),
        ("bogus", Action::None),
    ] {
        let mut c = Config::default();
        c.bindings.insert(id.into(), a);
        assert!(!validate(&c).is_empty());
    }
    let c = Config {
        edge_width_mm: f64::NAN,
        ..Config::default()
    };
    assert!(!validate(&c).is_empty());
}
#[test]
fn missing_bad_and_atomic_replacement() {
    let dir = std::env::temp_dir().join(format!("gestures-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    assert_eq!(load_from(&path), (Config::default(), None));
    std::fs::write(&path, "{broken").unwrap();
    let (c, w) = load_from(&path);
    assert_eq!(c, Config::default());
    assert!(w.is_some());
    assert!(path.with_extension("json.bad").exists());
    save_to(&c, &path).unwrap();
    let mut c = c;
    c.drag3 = true;
    save_to(&c, &path).unwrap();
    assert_eq!(load_from(&path), (c, None));
    assert!(!path.with_extension("json.tmp").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
