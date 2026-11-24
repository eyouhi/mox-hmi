use crate::Rect;

const SNAP_GUIDE_EPSILON: f32 = 0.01;
pub const DISPLAY_AXIS_TO_STORAGE: [usize; 3] = [0, 2, 1];

/// 3D nodes are stored in the renderer's Y-up space. The editor exposes a
/// conventional Z-up frame instead, so keep the conversion in one place.
pub fn scene_to_display_axes(values: [f32; 3]) -> [f32; 3] {
    [values[0], values[2], values[1]]
}

pub fn display_to_scene_axes(values: [f32; 3]) -> [f32; 3] {
    [values[0], values[2], values[1]]
}

pub fn display_axis_to_scene(axis: usize) -> usize {
    DISPLAY_AXIS_TO_STORAGE.get(axis).copied().unwrap_or(axis)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CenterSnap {
    pub rect: Rect,
    pub x: Option<f32>,
    pub y: Option<f32>,
}

pub fn design_to_world(point: [f32; 2], design_size: [u32; 2]) -> [f32; 3] {
    [
        point[0] - design_size[0] as f32 * 0.5,
        design_size[1] as f32 * 0.5 - point[1],
        0.0,
    ]
}

pub fn world_to_design(point: [f32; 2], design_size: [u32; 2]) -> [f32; 2] {
    [
        point[0] + design_size[0] as f32 * 0.5,
        design_size[1] as f32 * 0.5 - point[1],
    ]
}

pub fn point_in_rotated_rect(point: [f32; 2], rect: Rect, rotation_deg: f32) -> bool {
    if !point[0].is_finite()
        || !point[1].is_finite()
        || !rotation_deg.is_finite()
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return false;
    }
    let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    let offset = [point[0] - center[0], point[1] - center[1]];
    let angle = -rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let local = [
        offset[0] * cos - offset[1] * sin,
        offset[0] * sin + offset[1] * cos,
    ];
    local[0].abs() <= rect.width * 0.5 + f32::EPSILON
        && local[1].abs() <= rect.height * 0.5 + f32::EPSILON
}

pub fn point_near_polyline(point: [f32; 2], points: &[[f32; 2]], tolerance: f32) -> bool {
    if points.len() < 2 || !tolerance.is_finite() || tolerance < 0.0 {
        return false;
    }
    let tolerance_sq = tolerance * tolerance;
    points.windows(2).any(|segment| {
        let start = segment[0];
        let end = segment[1];
        let segment_x = end[0] - start[0];
        let segment_y = end[1] - start[1];
        let length_sq = segment_x * segment_x + segment_y * segment_y;
        let t = if length_sq <= f32::EPSILON {
            0.0
        } else {
            (((point[0] - start[0]) * segment_x + (point[1] - start[1]) * segment_y) / length_sq)
                .clamp(0.0, 1.0)
        };
        let nearest = [start[0] + segment_x * t, start[1] + segment_y * t];
        let dx = point[0] - nearest[0];
        let dy = point[1] - nearest[1];
        dx * dx + dy * dy <= tolerance_sq
    })
}

/// Returns the point on a rotated rectangle's boundary nearest to `point`.
///
/// The rectangle is expressed in design coordinates and rotates around its
/// center using the same clockwise screen-space convention as `UiNode`.
pub fn nearest_point_on_rect_boundary(
    point: [f32; 2],
    rect: Rect,
    rotation_deg: f32,
) -> Option<[f32; 2]> {
    if !point[0].is_finite()
        || !point[1].is_finite()
        || !rotation_deg.is_finite()
        || !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || rect.width <= 0.0
        || rect.height <= 0.0
    {
        return None;
    }

    let center = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    let angle = -rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    let offset = [point[0] - center[0], point[1] - center[1]];
    let local = [
        offset[0] * cos - offset[1] * sin,
        offset[0] * sin + offset[1] * cos,
    ];
    let half = [rect.width * 0.5, rect.height * 0.5];
    let clamped = [
        local[0].clamp(-half[0], half[0]),
        local[1].clamp(-half[1], half[1]),
    ];
    let boundary = if (local[0] - clamped[0]).abs() > f32::EPSILON
        || (local[1] - clamped[1]).abs() > f32::EPSILON
    {
        clamped
    } else {
        let distances = [
            (local[0] + half[0]).abs(),
            (half[0] - local[0]).abs(),
            (local[1] + half[1]).abs(),
            (half[1] - local[1]).abs(),
        ];
        let edge = distances
            .into_iter()
            .enumerate()
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index)
            .unwrap_or(0);
        match edge {
            0 => [-half[0], local[1]],
            1 => [half[0], local[1]],
            2 => [local[0], -half[1]],
            _ => [local[0], half[1]],
        }
    };

    let angle = rotation_deg.to_radians();
    let (sin, cos) = angle.sin_cos();
    Some([
        center[0] + boundary[0] * cos - boundary[1] * sin,
        center[1] + boundary[0] * sin + boundary[1] * cos,
    ])
}

pub fn resize_rect(start: Rect, corner: ResizeCorner, delta: [f32; 2], min_size: f32) -> Rect {
    let min_size = if min_size.is_finite() && min_size > 0.0 {
        min_size
    } else {
        1.0
    };
    if !start.x.is_finite()
        || !start.y.is_finite()
        || !start.width.is_finite()
        || !start.height.is_finite()
        || !delta[0].is_finite()
        || !delta[1].is_finite()
        || start.width <= 0.0
        || start.height <= 0.0
    {
        return Rect {
            x: if start.x.is_finite() { start.x } else { 0.0 },
            y: if start.y.is_finite() { start.y } else { 0.0 },
            width: min_size,
            height: min_size,
        };
    }

    let left = start.x;
    let top = start.y;
    let right = start.right();
    let bottom = start.bottom();

    let (new_left, new_top, new_right, new_bottom) = match corner {
        ResizeCorner::TopLeft => (
            (left + delta[0]).min(right - min_size),
            (top + delta[1]).min(bottom - min_size),
            right,
            bottom,
        ),
        ResizeCorner::TopRight => (
            left,
            (top + delta[1]).min(bottom - min_size),
            (right + delta[0]).max(left + min_size),
            bottom,
        ),
        ResizeCorner::BottomLeft => (
            (left + delta[0]).min(right - min_size),
            top,
            right,
            (bottom + delta[1]).max(top + min_size),
        ),
        ResizeCorner::BottomRight => (
            left,
            top,
            (right + delta[0]).max(left + min_size),
            (bottom + delta[1]).max(top + min_size),
        ),
    };

    Rect {
        x: new_left,
        y: new_top,
        width: (new_right - new_left).max(min_size),
        height: (new_bottom - new_top).max(min_size),
    }
}

pub fn snap_rect_center(
    rect: Rect,
    x_targets: &[f32],
    y_targets: &[f32],
    tolerance: f32,
    bounds: Rect,
) -> CenterSnap {
    let valid_rect = rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width > 0.0
        && rect.height > 0.0;
    let valid_bounds = bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.width > 0.0
        && bounds.height > 0.0;
    if !valid_rect || !valid_bounds || !tolerance.is_finite() || tolerance < 0.0 {
        return CenterSnap {
            rect,
            x: None,
            y: None,
        };
    }

    let center_x = rect.x + rect.width * 0.5;
    let center_y = rect.y + rect.height * 0.5;
    let target_x = nearest_center(center_x, x_targets, tolerance);
    let target_y = nearest_center(center_y, y_targets, tolerance);
    let mut snapped = rect;
    if let Some(target) = target_x {
        snapped.x = target - rect.width * 0.5;
    }
    if let Some(target) = target_y {
        snapped.y = target - rect.height * 0.5;
    }

    snapped.x = snapped
        .x
        .clamp(bounds.x, (bounds.right() - rect.width).max(bounds.x));
    snapped.y = snapped
        .y
        .clamp(bounds.y, (bounds.bottom() - rect.height).max(bounds.y));

    let x = target_x
        .filter(|target| (snapped.x + rect.width * 0.5 - target).abs() <= SNAP_GUIDE_EPSILON);
    let y = target_y
        .filter(|target| (snapped.y + rect.height * 0.5 - target).abs() <= SNAP_GUIDE_EPSILON);
    CenterSnap {
        rect: snapped,
        x,
        y,
    }
}

fn nearest_center(center: f32, targets: &[f32], tolerance: f32) -> Option<f32> {
    targets
        .iter()
        .copied()
        .filter(|target| target.is_finite())
        .filter_map(|target| {
            let distance = (target - center).abs();
            (distance <= tolerance).then_some((distance, target))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, target)| target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_rect_boundaries_are_centered() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 80.0,
        };
        assert!(point_in_rotated_rect([200.0, 140.0], rect, 0.0));
        assert!(point_in_rotated_rect([200.0, 60.0], rect, 90.0));
        assert!(point_in_rotated_rect([200.0, 220.0], rect, 90.0));
        assert!(!point_in_rotated_rect([260.0, 180.0], rect, 90.0));
    }

    #[test]
    fn center_snap_uses_nearest_target_on_both_axes() {
        let rect = Rect {
            x: 395.0,
            y: 300.0,
            width: 100.0,
            height: 50.0,
        };
        let snap = snap_rect_center(
            rect,
            &[420.0, 450.0],
            &[310.0, 325.0],
            8.0,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1600.0,
                height: 900.0,
            },
        );
        assert_eq!(snap.rect.x, 400.0);
        assert_eq!(snap.rect.y, 300.0);
        assert_eq!(snap.x, Some(450.0));
        assert_eq!(snap.y, Some(325.0));
    }

    #[test]
    fn center_snap_does_nothing_outside_tolerance() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 80.0,
            height: 40.0,
        };
        let snap = snap_rect_center(
            rect,
            &[200.0],
            &[200.0],
            8.0,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1600.0,
                height: 900.0,
            },
        );
        assert_eq!(snap.rect, rect);
        assert_eq!(snap.x, None);
        assert_eq!(snap.y, None);
    }

    #[test]
    fn center_snap_does_not_report_guide_when_bounds_prevent_alignment() {
        let rect = Rect {
            x: 0.0,
            y: 100.0,
            width: 100.0,
            height: 40.0,
        };
        let snap = snap_rect_center(
            rect,
            &[20.0],
            &[],
            30.0,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1600.0,
                height: 900.0,
            },
        );
        assert_eq!(snap.rect.x, 0.0);
        assert_eq!(snap.x, None);
    }

    #[test]
    fn center_snap_guide_survives_float_rounding() {
        let rect = Rect {
            x: 20.3,
            y: 30.7,
            width: 123.3,
            height: 57.9,
        };
        let target_x = rect.x + rect.width * 0.5;
        let target_y = rect.y + rect.height * 0.5;
        let snap = snap_rect_center(
            rect,
            &[target_x],
            &[target_y],
            1.0,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1600.0,
                height: 900.0,
            },
        );
        assert_eq!(snap.x, Some(target_x));
        assert_eq!(snap.y, Some(target_y));
    }

    #[test]
    fn nearest_boundary_uses_the_closest_side() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 80.0,
        };
        assert_eq!(
            nearest_point_on_rect_boundary([125.0, 140.0], rect, 0.0),
            Some([100.0, 140.0])
        );
        assert_eq!(
            nearest_point_on_rect_boundary([200.0, 140.0], rect, 0.0),
            Some([200.0, 100.0])
        );
    }

    #[test]
    fn nearest_boundary_follows_rotation() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 80.0,
        };
        assert_eq!(
            nearest_point_on_rect_boundary([200.0, 20.0], rect, 90.0),
            Some([200.0, 40.0])
        );
    }
}
