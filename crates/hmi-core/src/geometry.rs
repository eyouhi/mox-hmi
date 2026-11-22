use crate::Rect;

const SNAP_GUIDE_EPSILON: f32 = 0.01;

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
}
