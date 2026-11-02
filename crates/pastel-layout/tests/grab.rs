use pastel_layout::{Edges, Grab, GrabKind, Rect, WindowId};

const NONE: (f64, f64) = (0.0, 0.0);

fn grab(kind: GrabKind) -> Grab {
    Grab {
        id: WindowId(1),
        kind,
        start: (500.0, 500.0),
        rect: Rect::new(100.0, 100.0, 400.0, 300.0),
    }
}

#[test]
fn moving_keeps_the_size() {
    let rect = grab(GrabKind::Move).rect_at(530.0, 480.0, NONE, NONE);
    assert_eq!(rect, Rect::new(130.0, 80.0, 400.0, 300.0));
}

#[test]
fn the_right_and_bottom_grow_away_from_a_fixed_corner() {
    let edges = Edges {
        right: true,
        bottom: true,
        ..Edges::default()
    };
    let rect = grab(GrabKind::Resize(edges)).rect_at(550.0, 520.0, NONE, NONE);
    assert_eq!(rect, Rect::new(100.0, 100.0, 450.0, 320.0));
}

#[test]
fn the_left_and_top_move_and_the_far_corner_stays() {
    let edges = Edges {
        left: true,
        top: true,
        ..Edges::default()
    };
    let rect = grab(GrabKind::Resize(edges)).rect_at(450.0, 470.0, NONE, NONE);
    assert_eq!(rect, Rect::new(50.0, 70.0, 450.0, 330.0));
}

#[test]
fn limits_stop_the_size_with_the_held_side_following_only_that_far() {
    let edges = Edges {
        left: true,
        ..Edges::default()
    };
    let far = grab(GrabKind::Resize(edges)).rect_at(1000.0, 500.0, (200.0, 100.0), NONE);
    assert_eq!(far, Rect::new(300.0, 100.0, 200.0, 300.0));
    let edges = Edges {
        right: true,
        ..Edges::default()
    };
    let big = grab(GrabKind::Resize(edges)).rect_at(2000.0, 500.0, NONE, (500.0, 0.0));
    assert_eq!(big.w, 500.0);
}

#[test]
fn an_edge_not_held_stays_put() {
    let edges = Edges {
        right: true,
        ..Edges::default()
    };
    let rect = grab(GrabKind::Resize(edges)).rect_at(600.0, 900.0, NONE, NONE);
    assert_eq!(rect.h, 300.0);
}

#[test]
fn the_nearest_corner_is_the_one_grabbed() {
    let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let edges = Edges::nearest(rect, 10.0, 90.0);
    assert!(edges.left && edges.bottom && !edges.right && !edges.top);
}
