use hmi_core::*;

#[test]
fn design_world_round_trip() {
    for point in [[0.0, 0.0], [800.0, 450.0], [1600.0, 900.0]] {
        let world = design_to_world(point, [1600, 900]);
        let back = world_to_design([world[0], world[1]], [1600, 900]);
        assert!((back[0] - point[0]).abs() < 0.001);
        assert!((back[1] - point[1]).abs() < 0.001);
    }
    assert_eq!(
        design_to_world([800.0, 450.0], [1600, 900]),
        [0.0, 0.0, 0.0]
    );
}

#[test]
fn display_axes_keep_z_vertical() {
    let scene = [1.0, 2.0, 3.0];
    let display = scene_to_display_axes(scene);
    assert_eq!(display, [1.0, 3.0, 2.0]);
    assert_eq!(display_to_scene_axes(display), scene);
    assert_eq!(display_axis_to_scene(2), 1);
    assert_eq!(display_axis_to_scene(1), 2);
}

#[test]
fn rotated_rect_hit_test_respects_rotation() {
    let rect = Rect {
        x: 100.0,
        y: 100.0,
        width: 200.0,
        height: 80.0,
    };
    assert!(point_in_rotated_rect([200.0, 140.0], rect, 0.0));
    assert!(!point_in_rotated_rect([200.0, 10.0], rect, 0.0));
    assert!(point_in_rotated_rect([200.0, 60.0], rect, 90.0));
    assert!(!point_in_rotated_rect([260.0, 180.0], rect, 90.0));
}

#[test]
fn polyline_hit_test_accepts_segments_and_joints() {
    let points = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
    assert!(point_near_polyline([50.0, 3.0], &points, 4.0));
    assert!(point_near_polyline([100.0, 50.0], &points, 4.0));
    assert!(!point_near_polyline([50.0, 30.0], &points, 4.0));
}

#[test]
fn resize_rect_clamps_to_min_size() {
    let start = Rect {
        x: 100.0,
        y: 100.0,
        width: 200.0,
        height: 120.0,
    };
    let moved = resize_rect(start, ResizeCorner::BottomRight, [30.0, 20.0], 16.0);
    assert_eq!(
        moved,
        Rect {
            x: 100.0,
            y: 100.0,
            width: 230.0,
            height: 140.0
        }
    );
    let clamped = resize_rect(start, ResizeCorner::TopLeft, [500.0, 500.0], 16.0);
    assert_eq!(clamped.width, 16.0);
    assert_eq!(clamped.height, 16.0);
}

#[test]
fn rotated_rect_hit_test_matches_expected_corners() {
    let rect = Rect {
        x: 100.0,
        y: 100.0,
        width: 200.0,
        height: 100.0,
    };
    assert!(point_in_rotated_rect([100.0, 100.0], rect, 0.0));
    assert!(point_in_rotated_rect([300.0, 200.0], rect, 0.0));
    assert!(!point_in_rotated_rect([100.0, 200.5], rect, 0.0));
}

#[test]
fn resize_rect_keeps_opposite_corner_fixed() {
    let start = Rect {
        x: 100.0,
        y: 100.0,
        width: 200.0,
        height: 120.0,
    };
    let resized = resize_rect(start, ResizeCorner::TopLeft, [-20.0, -10.0], 16.0);
    assert_eq!(resized.right(), start.right());
    assert_eq!(resized.bottom(), start.bottom());
    assert_eq!(resized.x, 80.0);
    assert_eq!(resized.y, 90.0);
}
