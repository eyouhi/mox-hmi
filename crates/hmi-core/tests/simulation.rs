use hmi_core::*;

fn project() -> ProjectDocument {
    let tags = vec![
        TagDefinition {
            id: "tag.sine".into(),
            name: "正弦".into(),
            value_type: TagValueType::Number,
            unit: "°C".into(),
            min: Some(10.0),
            max: Some(30.0),
            precision: 1,
            initial: TagValue::Number(20.0),
            writable: true,
            simulator: SimulatorKind::Sine,
        },
        TagDefinition {
            id: "tag.fixed".into(),
            name: "只读".into(),
            value_type: TagValueType::Number,
            unit: String::new(),
            min: Some(0.0),
            max: Some(100.0),
            precision: 0,
            initial: TagValue::Number(1.0),
            writable: false,
            simulator: SimulatorKind::Sine,
        },
        TagDefinition {
            id: "tag.pulse".into(),
            name: "脉冲".into(),
            value_type: TagValueType::Bool,
            unit: String::new(),
            min: None,
            max: None,
            precision: 0,
            initial: TagValue::Bool(false),
            writable: false,
            simulator: SimulatorKind::Pulse,
        },
        TagDefinition {
            id: "tag.walk".into(),
            name: "随机游走".into(),
            value_type: TagValueType::Number,
            unit: String::new(),
            min: Some(0.0),
            max: Some(100.0),
            precision: 0,
            initial: TagValue::Number(50.0),
            writable: false,
            simulator: SimulatorKind::RandomWalk,
        },
    ];
    ProjectDocument {
        schema_version: 1,
        id: "project.sim".into(),
        name: "仿真".into(),
        design_size: [1600, 900],
        tags,
        startup_screen: "screen".into(),
        screens: vec![Screen::TwoD {
            id: "screen".into(),
            name: "画面".into(),
            background: HexColor("#000000".into()),
            nodes: vec![],
        }],
    }
}

#[test]
fn two_simulations_with_same_fixed_ticks_are_identical() {
    let mut a = TagSimulation::new(&project());
    let mut b = TagSimulation::new(&project());
    let initial = a.snapshot().get("tag.sine").cloned();
    let mut changed = false;
    for _ in 0..40 {
        assert_eq!(a.tick(250), b.tick(250));
        assert_eq!(a.snapshot(), b.snapshot());
        if a.snapshot().get("tag.sine") != initial.as_ref() {
            changed = true;
        }
    }
    assert!(changed);
}

#[test]
fn pulse_has_fixed_period_and_toggles() {
    let mut sim = TagSimulation::new(&project());
    assert_eq!(
        sim.snapshot().get("tag.pulse"),
        Some(&TagValue::Bool(false))
    );
    sim.tick(3_750);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(true)));
    sim.tick(250);
    assert_eq!(
        sim.snapshot().get("tag.pulse"),
        Some(&TagValue::Bool(false))
    );
    sim.tick(3_750);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(true)));
    sim.tick(250);
    assert_eq!(
        sim.snapshot().get("tag.pulse"),
        Some(&TagValue::Bool(false))
    );
}

#[test]
fn writable_sine_tag_pauses_after_write_until_resume() {
    let mut sim = TagSimulation::new(&project());
    sim.write("tag.sine", TagValue::Number(12.5)).unwrap();
    for _ in 0..12 {
        sim.tick(250);
    }
    assert_eq!(
        sim.snapshot().get("tag.sine"),
        Some(&TagValue::Number(12.5))
    );
    sim.resume("tag.sine").unwrap();
    for _ in 0..12 {
        sim.tick(250);
    }
    assert_ne!(
        sim.snapshot().get("tag.sine"),
        Some(&TagValue::Number(12.5))
    );
}

#[test]
fn remote_snapshot_replaces_values_and_revision() {
    let mut sim = TagSimulation::new(&project());
    let mut values = sim.snapshot().values().clone();
    values.insert("tag.pulse".into(), TagValue::Bool(true));
    values.insert("tag.sine".into(), TagValue::Number(22.5));

    assert!(sim.replace_snapshot(values, 42));
    assert_eq!(sim.snapshot().revision(), 42);
    assert_eq!(sim.snapshot().get("tag.pulse"), Some(&TagValue::Bool(true)));
    assert_eq!(
        sim.snapshot().get("tag.sine"),
        Some(&TagValue::Number(22.5))
    );
}

#[test]
fn write_errors_are_explicit() {
    let mut sim = TagSimulation::new(&project());
    assert!(matches!(
        sim.write("missing", TagValue::Bool(true)),
        Err(HmiError::UnknownTag(id)) if id == "missing"
    ));
    assert!(matches!(
        sim.write("tag.fixed", TagValue::Number(12.0)),
        Err(HmiError::TagNotWritable(id)) if id == "tag.fixed"
    ));
    assert!(matches!(
        sim.write("tag.sine", TagValue::Bool(true)),
        Err(HmiError::InvalidCommand(_))
    ));
}

#[test]
fn random_walk_is_deterministic_and_stays_in_range() {
    let mut sim = TagSimulation::new(&project());
    let mut previous = 50.0;
    for _ in 0..40 {
        sim.tick(250);
        let TagValue::Number(current) = sim.snapshot().get("tag.walk").unwrap() else {
            panic!()
        };
        assert!((0.0..=100.0).contains(current));
        assert_ne!(*current, previous);
        previous = *current;
    }
}
