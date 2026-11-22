use crate::tags::TagDefinition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HexColor(pub String);

impl HexColor {
    pub fn parse_rgb(&self) -> Option<[u8; 3]> {
        let bytes = self.0.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' {
            return None;
        }
        let r = u8::from_str_radix(std::str::from_utf8(&bytes[1..3]).ok()?, 16).ok()?;
        let g = u8::from_str_radix(std::str::from_utf8(&bytes[3..5]).ok()?, 16).ok()?;
        let b = u8::from_str_radix(std::str::from_utf8(&bytes[5..7]).ok()?, 16).ok()?;
        Some([r, g, b])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentKind {
    Fan,
    Pump,
    Ahu,
    Valve,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ButtonAction {
    ToggleTag {
        tag_id: String,
    },
    SetTag {
        tag_id: String,
        value: crate::tags::TagValue,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThresholdStop {
    pub value: f32,
    pub color: HexColor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingMapping {
    Format {
        #[serde(default)]
        precision: Option<u8>,
        #[serde(default)]
        unit: Option<String>,
    },
    BoolColor {
        true_color: HexColor,
        false_color: HexColor,
    },
    ThresholdColor {
        thresholds: Vec<ThresholdStop>,
    },
    Range {
        input_min: f32,
        input_max: f32,
        output_min: f32,
        output_max: f32,
        #[serde(default)]
        clamp: bool,
    },
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingTarget {
    Text,
    Color,
    Visible,
    Progress,
    RotationSpeed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub tag_id: String,
    pub target: BindingTarget,
    pub mapping: BindingMapping,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Widget2D {
    Panel {
        fill: HexColor,
        stroke: HexColor,
        stroke_width: f32,
    },
    Label {
        text: String,
        font_size: f32,
        color: HexColor,
        align: TextAlign,
    },
    ValueCard {
        label: String,
    },
    StatusLamp {
        label: String,
    },
    BarGauge {
        min: f32,
        max: f32,
        direction: BarDirection,
    },
    Button {
        label: String,
        action: ButtonAction,
    },
    Pipe {
        points: Vec<[f32; 2]>,
        thickness: f32,
        color: HexColor,
    },
    EquipmentSymbol {
        equipment: EquipmentKind,
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiNode {
    pub id: String,
    pub name: String,
    pub rect: Rect,
    pub rotation_deg: f32,
    pub z: i32,
    pub visible: bool,
    pub locked: bool,
    pub widget: Widget2D,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceModel {
    Ahu {
        size: [f32; 3],
        color: HexColor,
    },
    Fan {
        size: [f32; 3],
        color: HexColor,
    },
    Pump {
        size: [f32; 3],
        color: HexColor,
    },
    Duct {
        length: f32,
        diameter: f32,
        color: HexColor,
    },
    Pipe {
        length: f32,
        diameter: f32,
        color: HexColor,
    },
    Valve {
        size: f32,
        color: HexColor,
    },
    Glb {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceNode {
    pub id: String,
    pub name: String,
    pub position: [f32; 3],
    pub rotation_deg: [f32; 3],
    pub scale: [f32; 3],
    pub visible: bool,
    pub locked: bool,
    #[serde(default)]
    pub label: Option<String>,
    pub model: DeviceModel,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Screen {
    TwoD {
        id: String,
        name: String,
        background: HexColor,
        nodes: Vec<UiNode>,
    },
    ThreeD {
        id: String,
        name: String,
        background: HexColor,
        nodes: Vec<DeviceNode>,
    },
}

impl Screen {
    pub fn id(&self) -> &str {
        match self {
            Screen::TwoD { id, .. } | Screen::ThreeD { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Screen::TwoD { name, .. } | Screen::ThreeD { name, .. } => name,
        }
    }

    pub fn is_two_d(&self) -> bool {
        matches!(self, Screen::TwoD { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub design_size: [u32; 2],
    #[serde(default)]
    pub tags: Vec<TagDefinition>,
    pub screens: Vec<Screen>,
    pub startup_screen: String,
}

impl ProjectDocument {
    pub fn screen(&self, id: &str) -> Option<&Screen> {
        self.screens.iter().find(|screen| screen.id() == id)
    }

    pub fn tag(&self, id: &str) -> Option<&TagDefinition> {
        self.tags.iter().find(|tag| tag.id == id)
    }

    pub fn tag_map(&self) -> std::collections::BTreeMap<String, TagDefinition> {
        self.tags
            .iter()
            .cloned()
            .map(|tag| (tag.id.clone(), tag))
            .collect()
    }
}
