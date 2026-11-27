use hmi_core::*;

#[test]
fn bas_demo_is_valid_and_has_two_screens() {
    let project = bas_demo();
    validate_project(&project).unwrap();
    assert_eq!(project.screens.len(), 2);
    assert_eq!(project.screens[0].id(), "screen.bas_2d");
    assert_eq!(project.screens[1].id(), "screen.bas_3d");
    assert!(project.screens[0].is_two_d());
}

#[test]
fn bas_demo_covers_the_required_tags_and_equipment() {
    let project = bas_demo();
    let ids: Vec<_> = project.tags.iter().map(|tag| tag.id.as_str()).collect();
    for required in [
        "bas.ahu.supply_temp",
        "bas.station.humidity",
        "bas.station.co2",
        "bas.supply_fan.run",
        "bas.supply_fan.fault",
        "bas.supply_fan.freq",
        "bas.chilled_pump.run",
        "bas.ahu.valve.opening",
    ] {
        assert!(ids.contains(&required), "missing tag {required}");
    }
    let Screen::TwoD { nodes, .. } = &project.screens[0] else {
        unreachable!()
    };
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node.widget, Widget2D::Button { .. }))
    );
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node.widget, Widget2D::BarGauge { .. }))
    );
    assert!(nodes.iter().any(|node| matches!(
        node.widget,
        Widget2D::EquipmentSymbol {
            equipment: EquipmentKind::Fan,
            ..
        }
    )));
    for kind in [
        EquipmentKind::Chiller,
        EquipmentKind::CoolingTower,
        EquipmentKind::HeatExchanger,
        EquipmentKind::ControlCabinet,
    ] {
        assert!(
            nodes.iter().any(|node| matches!(
                &node.widget,
                Widget2D::EquipmentSymbol { equipment, .. } if *equipment == kind
            )),
            "missing 2D symbol {kind:?}"
        );
    }
    let Screen::ThreeD { nodes, .. } = &project.screens[1] else {
        unreachable!()
    };
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node.model, DeviceModel::Ahu { .. }))
    );
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node.model, DeviceModel::Fan { .. }))
    );
    assert!(
        nodes
            .iter()
            .any(|node| matches!(node.model, DeviceModel::Pump { .. }))
    );
    let predicates: [fn(&DeviceModel) -> bool; 4] = [
        |model: &DeviceModel| matches!(model, DeviceModel::Chiller { .. }),
        |model: &DeviceModel| matches!(model, DeviceModel::CoolingTower { .. }),
        |model: &DeviceModel| matches!(model, DeviceModel::HeatExchanger { .. }),
        |model: &DeviceModel| matches!(model, DeviceModel::ControlCabinet { .. }),
    ];
    for predicate in predicates {
        assert!(nodes.iter().any(|node| predicate(&node.model)));
    }
}

#[test]
fn demo_bindings_are_editable_and_explainable() {
    let project = bas_demo();
    let all_bindings = project
        .screens
        .iter()
        .flat_map(|screen| match screen {
            Screen::TwoD { nodes, .. } => nodes
                .iter()
                .flat_map(|node| node.bindings.iter())
                .collect::<Vec<_>>(),
            Screen::ThreeD { nodes, .. } => nodes
                .iter()
                .flat_map(|node| node.bindings.iter())
                .collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>();
    assert!(all_bindings.len() >= 10);
    assert!(
        all_bindings
            .iter()
            .any(|binding| binding.target == BindingTarget::RotationSpeed)
    );
    assert!(
        all_bindings
            .iter()
            .any(|binding| binding.target == BindingTarget::Text)
    );
}
