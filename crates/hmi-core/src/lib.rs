pub const SCHEMA_VERSION: u32 = 1;

pub mod bindings;
pub mod commands;
pub mod demo;
pub mod error;
pub mod geometry;
pub mod model;
pub mod persistence;
pub mod simulation;
pub mod tags;
pub mod validation;

pub use bindings::{
    VisualPatch, VisualValue, default_mapping, format_tag_value, resolve_binding, resolve_bindings,
};
pub use commands::{
    BatchCommandResult, CommandResult, EditorCommand, EditorSession, UNDO_LIMIT, unique_node_id,
};
pub use demo::bas_demo;
pub use error::HmiError;
pub use geometry::{
    CenterSnap, DISPLAY_AXIS_TO_STORAGE, ResizeCorner, design_to_world, display_axis_to_scene,
    display_to_scene_axes, nearest_point_on_rect_boundary, point_in_rotated_rect,
    point_near_polyline, resize_rect, scene_to_display_axes, snap_rect_center, world_to_design,
};
pub use model::*;
pub use persistence::{PersistenceError, STORAGE_KEY, export_filename, from_json, to_json};
pub use simulation::{SIMULATION_TICK_MS, SINE_PERIOD_MS, TagSimulation};
pub use tags::*;
pub use validation::{
    MAX_IMPORT_BYTES, MAX_NODES_2D, MAX_NODES_3D, MAX_TAGS, ValidationIssue, is_valid_glb_path,
    validate_project, validate_tag_value,
};
