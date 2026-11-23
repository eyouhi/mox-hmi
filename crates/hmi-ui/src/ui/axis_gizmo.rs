use crate::bridge;
use dioxus::prelude::*;

#[component]
pub fn AxisGizmo3D() -> Element {
    let Some(gizmo) = bridge::axis_gizmo_3d() else {
        return rsx! {};
    };
    let axes = [
        ("X", gizmo.x, "is-x"),
        ("Y", gizmo.y, "is-y"),
        ("Z", gizmo.z, "is-z"),
    ];

    rsx! {
        div {
            class: "axis-gizmo",
            "data-testid": "axis-gizmo",
            role: "img",
            aria_label: "3D 坐标方位：X 左右，Y 前后，Z 垂直地面",
            div { class: "axis-gizmo-title", "坐标" }
            div { class: "axis-gizmo-stage", aria_hidden: "true",
                for (label, vector, class_name) in axes {
                    {
                        let dx = vector[0];
                        let dy = vector[1];
                        let length = (dx * dx + dy * dy).sqrt() * 29.0;
                        let angle = dy.atan2(dx).to_degrees();
                        let end_x = 42.0 + dx * 29.0;
                        let end_y = 42.0 + dy * 29.0;
                        rsx! {
                            div {
                                class: "axis-gizmo-line {class_name}",
                                style: "width: {length.max(2.0)}px; transform: rotate({angle}deg);"
                            }
                            span {
                                class: "axis-gizmo-label {class_name}",
                                style: "left: {end_x}px; top: {end_y}px;",
                                "{label}"
                            }
                        }
                    }
                }
                span { class: "axis-gizmo-origin" }
            }
            span { class: "axis-gizmo-foot", "Z 垂直地面" }
        }
    }
}
