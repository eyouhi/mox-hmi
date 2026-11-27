use crate::{
    BarDirection, Binding, BindingMapping, BindingTarget, ButtonAction, DeviceModel, DeviceNode,
    EquipmentKind, HexColor, ProjectDocument, Rect, Screen, SimulatorKind, TagDefinition, TagValue,
    TagValueType, TextAlign, UiNode, Widget2D,
};

#[allow(clippy::too_many_arguments)]
fn tag(
    id: &str,
    name: &str,
    value_type: TagValueType,
    unit: &str,
    min: Option<f32>,
    max: Option<f32>,
    precision: u8,
    initial: TagValue,
    writable: bool,
    simulator: SimulatorKind,
) -> TagDefinition {
    TagDefinition {
        id: id.into(),
        name: name.into(),
        value_type,
        unit: unit.into(),
        min,
        max,
        precision,
        initial,
        writable,
        simulator,
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn ui(
    id: &str,
    name: &str,
    rect: Rect,
    z: i32,
    widget: Widget2D,
    bindings: Vec<Binding>,
) -> UiNode {
    UiNode {
        id: id.into(),
        name: name.into(),
        rect,
        rotation_deg: 0.0,
        z,
        visible: true,
        locked: false,
        widget,
        bindings,
    }
}

fn device(
    id: &str,
    name: &str,
    position: [f32; 3],
    model: DeviceModel,
    bindings: Vec<Binding>,
) -> DeviceNode {
    DeviceNode {
        id: id.into(),
        name: name.into(),
        position,
        rotation_deg: [0.0, 0.0, 0.0],
        scale: [1.0, 1.0, 1.0],
        visible: true,
        locked: false,
        label: Some(name.into()),
        model,
        bindings,
    }
}

fn bind(tag_id: &str, target: BindingTarget, mapping: BindingMapping) -> Binding {
    Binding {
        tag_id: tag_id.into(),
        target,
        mapping,
    }
}

fn color_bool(tag_id: &str, on: &str, off: &str) -> Binding {
    bind(
        tag_id,
        BindingTarget::Color,
        BindingMapping::BoolColor {
            true_color: HexColor(on.into()),
            false_color: HexColor(off.into()),
        },
    )
}

fn text_format(tag_id: &str) -> Binding {
    bind(
        tag_id,
        BindingTarget::Text,
        BindingMapping::Format {
            precision: None,
            unit: None,
        },
    )
}

fn range(
    tag_id: &str,
    input_min: f32,
    input_max: f32,
    output_min: f32,
    output_max: f32,
) -> Binding {
    bind(
        tag_id,
        BindingTarget::Progress,
        BindingMapping::Range {
            input_min,
            input_max,
            output_min,
            output_max,
            clamp: true,
        },
    )
}

fn rotation_range(
    tag_id: &str,
    input_min: f32,
    input_max: f32,
    output_min: f32,
    output_max: f32,
) -> Binding {
    bind(
        tag_id,
        BindingTarget::RotationSpeed,
        BindingMapping::Range {
            input_min,
            input_max,
            output_min,
            output_max,
            clamp: true,
        },
    )
}

fn pipe(id: &str, name: &str, rect: Rect, color: &str, thickness: f32) -> UiNode {
    ui(
        id,
        name,
        Rect {
            width: rect.width,
            ..rect
        },
        15,
        Widget2D::Pipe {
            points: vec![[0.0, rect.height * 0.5], [rect.width, rect.height * 0.5]],
            thickness,
            color: HexColor(color.into()),
        },
        vec![],
    )
}

fn equipment(
    id: &str,
    name: &str,
    rect: Rect,
    kind: EquipmentKind,
    label: &str,
    bindings: Vec<Binding>,
) -> UiNode {
    ui(
        id,
        name,
        rect,
        20,
        Widget2D::EquipmentSymbol {
            equipment: kind,
            label: label.into(),
        },
        bindings,
    )
}

fn card(id: &str, name: &str, rect: Rect, label: &str, tag_id: &str) -> UiNode {
    ui(
        id,
        name,
        rect,
        30,
        Widget2D::ValueCard {
            label: label.into(),
        },
        vec![text_format(tag_id)],
    )
}

pub fn bas_demo() -> ProjectDocument {
    let tags = vec![
        tag(
            "bas.ahu.supply_temp",
            "送风温度",
            TagValueType::Number,
            "°C",
            Some(18.0),
            Some(30.0),
            1,
            TagValue::Number(23.5),
            false,
            SimulatorKind::Sine,
        ),
        tag(
            "bas.station.humidity",
            "车站湿度",
            TagValueType::Number,
            "%",
            Some(30.0),
            Some(80.0),
            0,
            TagValue::Number(55.0),
            false,
            SimulatorKind::Sine,
        ),
        tag(
            "bas.station.co2",
            "CO2 浓度",
            TagValueType::Number,
            "ppm",
            Some(400.0),
            Some(1200.0),
            0,
            TagValue::Number(650.0),
            false,
            SimulatorKind::Sine,
        ),
        tag(
            "bas.supply_fan.run",
            "送风机运行",
            TagValueType::Bool,
            "",
            None,
            None,
            0,
            TagValue::Bool(false),
            true,
            SimulatorKind::Manual,
        ),
        tag(
            "bas.supply_fan.fault",
            "送风机故障",
            TagValueType::Bool,
            "",
            None,
            None,
            0,
            TagValue::Bool(false),
            false,
            SimulatorKind::Constant,
        ),
        tag(
            "bas.supply_fan.freq",
            "送风机频率",
            TagValueType::Number,
            "Hz",
            Some(0.0),
            Some(50.0),
            1,
            TagValue::Number(35.0),
            false,
            SimulatorKind::Sine,
        ),
        tag(
            "bas.chilled_pump.run",
            "冷冻水泵运行",
            TagValueType::Bool,
            "",
            None,
            None,
            0,
            TagValue::Bool(true),
            false,
            SimulatorKind::Constant,
        ),
        tag(
            "bas.ahu.valve.opening",
            "冷冻水阀开度",
            TagValueType::Number,
            "%",
            Some(0.0),
            Some(100.0),
            0,
            TagValue::Number(62.0),
            false,
            SimulatorKind::Sine,
        ),
    ];

    let fan_on = || color_bool("bas.supply_fan.run", "#37e6d4", "#31424b");
    let pump_on = || color_bool("bas.chilled_pump.run", "#37e6d4", "#31424b");
    let valve_progress = || range("bas.ahu.valve.opening", 0.0, 100.0, 0.0, 1.0);

    let nodes_2d = vec![
        ui(
            "node.bg",
            "画面背景",
            rect(0.0, 0.0, 1600.0, 900.0),
            0,
            Widget2D::Panel {
                fill: HexColor("#071217".into()),
                stroke: HexColor("#132b34".into()),
                stroke_width: 1.0,
            },
            vec![],
        ),
        ui(
            "node.title",
            "标题",
            rect(48.0, 24.0, 620.0, 48.0),
            10,
            Widget2D::Label {
                text: "车站 BAS 环控工艺".into(),
                font_size: 30.0,
                color: HexColor("#e8f2f4".into()),
                align: TextAlign::Left,
            },
            vec![],
        ),
        equipment(
            "node.ahu",
            "AHU-01",
            rect(650.0, 250.0, 260.0, 180.0),
            EquipmentKind::Ahu,
            "AHU-01",
            vec![fan_on()],
        ),
        equipment(
            "node.supply_fan",
            "SF-01",
            rect(380.0, 460.0, 180.0, 120.0),
            EquipmentKind::Fan,
            "SF-01",
            vec![fan_on()],
        ),
        equipment(
            "node.exhaust_fan",
            "EF-01",
            rect(1040.0, 460.0, 180.0, 120.0),
            EquipmentKind::Fan,
            "EF-01",
            vec![pump_on()],
        ),
        equipment(
            "node.chilled_pump",
            "CHP-01",
            rect(650.0, 640.0, 160.0, 120.0),
            EquipmentKind::Pump,
            "CHP-01",
            vec![pump_on()],
        ),
        equipment(
            "node.cooling_pump",
            "CWP-01",
            rect(880.0, 640.0, 160.0, 120.0),
            EquipmentKind::Pump,
            "CWP-01",
            vec![pump_on()],
        ),
        equipment(
            "node.control_cabinet",
            "控制柜 CP-01",
            rect(910.0, 80.0, 110.0, 150.0),
            EquipmentKind::ControlCabinet,
            "CP-01",
            vec![],
        ),
        equipment(
            "node.chiller",
            "冷水机组 CH-01",
            rect(1080.0, 80.0, 240.0, 155.0),
            EquipmentKind::Chiller,
            "CH-01",
            vec![pump_on()],
        ),
        equipment(
            "node.cooling_tower",
            "冷却塔 CT-01",
            rect(1360.0, 80.0, 180.0, 210.0),
            EquipmentKind::CoolingTower,
            "CT-01",
            vec![fan_on()],
        ),
        equipment(
            "node.heat_exchanger",
            "板式换热器 HX-01",
            rect(1260.0, 390.0, 230.0, 150.0),
            EquipmentKind::HeatExchanger,
            "HX-01",
            vec![pump_on()],
        ),
        equipment(
            "node.valve_fresh",
            "新风阀",
            rect(330.0, 260.0, 90.0, 80.0),
            EquipmentKind::Valve,
            "新风阀",
            vec![],
        ),
        equipment(
            "node.valve_return",
            "回风阀",
            rect(1160.0, 260.0, 90.0, 80.0),
            EquipmentKind::Valve,
            "回风阀",
            vec![],
        ),
        equipment(
            "node.valve_chilled",
            "冷冻水阀",
            rect(800.0, 560.0, 90.0, 80.0),
            EquipmentKind::Valve,
            "冷冻水阀",
            vec![valve_progress()],
        ),
        pipe(
            "node.duct.supply",
            "送风管",
            rect(560.0, 330.0, 260.0, 24.0),
            "#2c6672",
            12.0,
        ),
        pipe(
            "node.duct.return",
            "回风管",
            rect(960.0, 330.0, 200.0, 24.0),
            "#31505a",
            12.0,
        ),
        pipe(
            "node.duct.chilled",
            "冷冻水管",
            rect(700.0, 430.0, 200.0, 24.0),
            "#2d6d77",
            10.0,
        ),
        pipe(
            "node.pipe.chilled",
            "冷冻水回水",
            rect(650.0, 760.0, 160.0, 20.0),
            "#2d6d77",
            8.0,
        ),
        pipe(
            "node.pipe.return",
            "回水管",
            rect(300.0, 560.0, 160.0, 20.0),
            "#31505a",
            8.0,
        ),
        card(
            "node.card.temp",
            "送风温度",
            rect(40.0, 170.0, 230.0, 104.0),
            "送风温度",
            "bas.ahu.supply_temp",
        ),
        card(
            "node.card.humidity",
            "车站湿度",
            rect(40.0, 292.0, 230.0, 104.0),
            "车站湿度",
            "bas.station.humidity",
        ),
        card(
            "node.card.co2",
            "CO2 浓度",
            rect(40.0, 414.0, 230.0, 104.0),
            "CO2 浓度",
            "bas.station.co2",
        ),
        card(
            "node.card.freq",
            "送风机频率",
            rect(40.0, 536.0, 230.0, 104.0),
            "送风机频率",
            "bas.supply_fan.freq",
        ),
        ui(
            "node.lamp.fan",
            "送风机运行",
            rect(300.0, 720.0, 230.0, 72.0),
            30,
            Widget2D::StatusLamp {
                label: "送风机运行".into(),
            },
            vec![color_bool("bas.supply_fan.run", "#37e6d4", "#31424b")],
        ),
        ui(
            "node.lamp.fault",
            "送风机故障",
            rect(560.0, 720.0, 230.0, 72.0),
            30,
            Widget2D::StatusLamp {
                label: "送风机故障".into(),
            },
            vec![color_bool("bas.supply_fan.fault", "#ff4d5e", "#31424b")],
        ),
        ui(
            "node.gauge.valve",
            "冷冻水阀开度",
            rect(1220.0, 600.0, 300.0, 90.0),
            30,
            Widget2D::BarGauge {
                min: 0.0,
                max: 100.0,
                direction: BarDirection::Horizontal,
            },
            vec![valve_progress()],
        ),
        ui(
            "node.button.fan",
            "启动/停止送风机",
            rect(840.0, 720.0, 260.0, 72.0),
            40,
            Widget2D::Button {
                label: "启动/停止送风机".into(),
                action: ButtonAction::ToggleTag {
                    tag_id: "bas.supply_fan.run".into(),
                },
            },
            vec![],
        ),
    ];

    let nodes_3d = vec![
        device(
            "node.3d.ahu",
            "AHU-01",
            [0.0, 1.4, 0.0],
            DeviceModel::Ahu {
                size: [4.0, 2.8, 2.2],
                color: HexColor("#23434d".into()),
            },
            vec![text_format("bas.ahu.supply_temp"), fan_on()],
        ),
        device(
            "node.3d.supply_fan",
            "SF-01",
            [-4.2, 1.0, 1.0],
            DeviceModel::Fan {
                size: [1.8, 1.8, 1.8],
                color: HexColor("#31505a".into()),
            },
            vec![
                fan_on(),
                rotation_range("bas.supply_fan.freq", 0.0, 50.0, 0.0, 15.0),
            ],
        ),
        device(
            "node.3d.exhaust_fan",
            "EF-01",
            [4.2, 1.0, 1.0],
            DeviceModel::Fan {
                size: [1.8, 1.8, 1.8],
                color: HexColor("#31505a".into()),
            },
            vec![pump_on()],
        ),
        device(
            "node.3d.chilled_pump",
            "CHP-01",
            [-2.0, 0.55, 3.0],
            DeviceModel::Pump {
                size: [1.4, 1.1, 1.4],
                color: HexColor("#31505a".into()),
            },
            vec![
                pump_on(),
                bind(
                    "bas.chilled_pump.run",
                    BindingTarget::RotationSpeed,
                    BindingMapping::Direct,
                ),
            ],
        ),
        device(
            "node.3d.cooling_pump",
            "CWP-01",
            [2.0, 0.55, 3.0],
            DeviceModel::Pump {
                size: [1.4, 1.1, 1.4],
                color: HexColor("#31505a".into()),
            },
            vec![pump_on()],
        ),
        device(
            "node.3d.duct_supply",
            "送风管",
            [-2.8, 2.2, -0.5],
            DeviceModel::Duct {
                length: 3.6,
                diameter: 0.55,
                color: HexColor("#24434f".into()),
            },
            vec![],
        ),
        device(
            "node.3d.duct_return",
            "回风管",
            [2.8, 2.2, -0.5],
            DeviceModel::Duct {
                length: 3.0,
                diameter: 0.55,
                color: HexColor("#24434f".into()),
            },
            vec![],
        ),
        device(
            "node.3d.pipe_chilled",
            "冷冻水管",
            [-2.0, 0.25, 1.9],
            DeviceModel::Pipe {
                length: 4.0,
                diameter: 0.18,
                color: HexColor("#2d6d77".into()),
            },
            vec![],
        ),
        device(
            "node.3d.pipe_condensate",
            "冷凝水管",
            [2.0, 0.25, 1.9],
            DeviceModel::Pipe {
                length: 3.0,
                diameter: 0.18,
                color: HexColor("#2d6d77".into()),
            },
            vec![],
        ),
        device(
            "node.3d.valve_fresh",
            "新风阀",
            [0.0, 1.9, -1.4],
            DeviceModel::Valve {
                size: 0.7,
                color: HexColor("#31505a".into()),
            },
            vec![],
        ),
        device(
            "node.3d.valve_return",
            "回风阀",
            [0.0, 1.9, 1.4],
            DeviceModel::Valve {
                size: 0.7,
                color: HexColor("#31505a".into()),
            },
            vec![valve_progress()],
        ),
        device(
            "node.3d.valve_chilled",
            "冷冻水阀",
            [0.0, 0.9, 1.9],
            DeviceModel::Valve {
                size: 0.7,
                color: HexColor("#31505a".into()),
            },
            vec![valve_progress()],
        ),
        device(
            "node.3d.chiller",
            "冷水机组 CH-01",
            [-4.4, 1.0, -2.8],
            DeviceModel::Chiller {
                size: [3.2, 2.0, 2.2],
                color: HexColor("#23434d".into()),
            },
            vec![
                pump_on(),
                rotation_range("bas.supply_fan.freq", 0.0, 50.0, 0.0, 12.0),
            ],
        ),
        device(
            "node.3d.cooling_tower",
            "冷却塔 CT-01",
            [6.3, 1.6, 2.5],
            DeviceModel::CoolingTower {
                size: [2.2, 3.2, 2.2],
                color: HexColor("#264850".into()),
            },
            vec![
                fan_on(),
                rotation_range("bas.supply_fan.freq", 0.0, 50.0, 0.0, 15.0),
            ],
        ),
        device(
            "node.3d.heat_exchanger",
            "板式换热器 HX-01",
            [1.5, 0.8, 6.5],
            DeviceModel::HeatExchanger {
                size: [2.4, 1.6, 1.2],
                color: HexColor("#31505a".into()),
            },
            vec![pump_on()],
        ),
        device(
            "node.3d.control_cabinet",
            "控制柜 CP-01",
            [-7.4, 1.0, -4.2],
            DeviceModel::ControlCabinet {
                size: [1.4, 2.0, 0.9],
                color: HexColor("#2b4a54".into()),
            },
            vec![],
        ),
    ];

    ProjectDocument {
        schema_version: 1,
        id: "project.bas_demo".into(),
        name: "车站 BAS 环控示例".into(),
        design_size: [1600, 900],
        tags,
        startup_screen: "screen.bas_2d".into(),
        screens: vec![
            Screen::TwoD {
                id: "screen.bas_2d".into(),
                name: "环控工艺图".into(),
                background: HexColor("#071217".into()),
                nodes: nodes_2d,
            },
            Screen::ThreeD {
                id: "screen.bas_3d".into(),
                name: "环控机房".into(),
                background: HexColor("#061016".into()),
                nodes: nodes_3d,
            },
        ],
    }
}
