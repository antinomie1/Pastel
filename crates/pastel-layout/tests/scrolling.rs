use pastel_layout::{
    CenterFocus, Direction::*, Layout, LayoutOp, LayoutOptions, Placement, Rect, Scrolling,
    WindowHint, WindowId,
};

const W: WindowId = WindowId(1);

fn area() -> Rect {
    Rect::new(0.0, 0.0, 1000.0, 600.0)
}

fn options(center_focus: CenterFocus) -> LayoutOptions {
    LayoutOptions { gap: 10.0, center_focus, ..LayoutOptions::default() }
}

fn layout_with(count: u64, center: CenterFocus) -> Scrolling {
    let mut layout = Scrolling::new(options(center));
    layout.set_area(area());
    for i in 1..=count {
        layout.add(WindowId(i), WindowHint::default());
    }
    layout
}

fn places(layout: &Scrolling) -> Vec<Placement> {
    let mut out = Vec::new();
    layout.place(&mut out);
    out
}

fn rect_of(layout: &Scrolling, id: u64) -> Rect {
    places(layout).iter().find(|p| p.id == WindowId(id)).expect("placed").rect
}

fn order(layout: &Scrolling) -> Vec<u64> {
    places(layout).iter().map(|p| p.id.0).collect()
}

#[test]
fn empty_layout_places_nothing() {
    let layout = layout_with(0, CenterFocus::Never);
    assert!(layout.is_empty());
    assert_eq!(layout.focused(), None);
    assert!(places(&layout).is_empty());
}

#[test]
fn one_window_gets_half_the_area_inside_the_gaps() {
    let layout = layout_with(1, CenterFocus::Never);
    assert_eq!(rect_of(&layout, 1), Rect::new(10.0, 10.0, 485.0, 580.0));
}

#[test]
fn two_half_columns_fill_the_area_exactly() {
    let layout = layout_with(2, CenterFocus::Never);
    let (a, b) = (rect_of(&layout, 1), rect_of(&layout, 2));
    assert_eq!(a.x, 10.0);
    assert_eq!(b.x, a.right() + 10.0);
    assert_eq!(b.right(), 990.0);
}

#[test]
fn opening_a_window_does_not_resize_the_others() {
    let mut layout = layout_with(1, CenterFocus::Never);
    let before = rect_of(&layout, 1);
    layout.add(WindowId(2), WindowHint::default());
    layout.add(WindowId(3), WindowHint::default());
    assert_eq!(rect_of(&layout, 1).w, before.w);
    assert_eq!(rect_of(&layout, 1).h, before.h);
}

#[test]
fn a_new_column_opens_right_of_the_focused_one_and_takes_focus() {
    let mut layout = layout_with(3, CenterFocus::Never);
    layout.focus(WindowId(1));
    layout.add(WindowId(9), WindowHint::default());
    assert_eq!(order(&layout), [1, 9, 2, 3]);
    assert_eq!(layout.focused(), Some(WindowId(9)));
}

#[test]
fn width_hint_overrides_the_default() {
    let mut layout = layout_with(0, CenterFocus::Never);
    layout.add(W, WindowHint { width: Some(1.0) });
    assert_eq!(rect_of(&layout, 1).w, 980.0);
}

#[test]
fn the_view_scrolls_the_minimum_to_show_the_focused_column() {
    let layout = layout_with(3, CenterFocus::Never);
    let focused = rect_of(&layout, 3);
    assert_eq!(focused.right(), 990.0, "right-aligned inside the gap");
    assert!(rect_of(&layout, 1).right() <= 0.0, "first column is off screen");
}

#[test]
fn focusing_a_visible_column_does_not_scroll() {
    let mut layout = layout_with(3, CenterFocus::Never);
    let before = rect_of(&layout, 2);
    assert!(layout.apply(LayoutOp::Focus(Left)));
    assert_eq!(rect_of(&layout, 2), before);
}

#[test]
fn focus_stops_at_the_ends() {
    let mut layout = layout_with(2, CenterFocus::Never);
    assert!(!layout.apply(LayoutOp::Focus(Right)));
    assert!(layout.apply(LayoutOp::FocusFirst));
    assert!(!layout.apply(LayoutOp::Focus(Left)));
    assert!(!layout.apply(LayoutOp::FocusFirst));
    assert!(layout.apply(LayoutOp::FocusLast));
    assert_eq!(layout.focused(), Some(WindowId(2)));
}

#[test]
fn always_centers_the_focused_column() {
    let layout = layout_with(3, CenterFocus::Always);
    let (cx, _) = rect_of(&layout, 3).center();
    assert!((cx - 500.0).abs() <= 1.0);
}

#[test]
fn on_overflow_centers_only_when_the_view_had_to_move() {
    let mut layout = layout_with(2, CenterFocus::OnOverflow);
    assert_eq!(rect_of(&layout, 1).x, 10.0, "both fit, nothing moved");
    layout.add(WindowId(3), WindowHint::default());
    let (cx, _) = rect_of(&layout, 3).center();
    assert!((cx - 500.0).abs() <= 1.0);
}

#[test]
fn moving_a_column_reorders_and_keeps_focus() {
    let mut layout = layout_with(3, CenterFocus::Never);
    assert!(layout.apply(LayoutOp::Move(Left)));
    assert_eq!(order(&layout), [1, 3, 2]);
    assert!(layout.apply(LayoutOp::MoveFirst));
    assert_eq!(order(&layout), [3, 1, 2]);
    assert!(!layout.apply(LayoutOp::Move(Left)));
    assert!(layout.apply(LayoutOp::MoveLast));
    assert_eq!(order(&layout), [1, 2, 3]);
    assert_eq!(layout.focused(), Some(WindowId(3)));
}

#[test]
fn consume_stacks_into_the_neighbour_sharing_the_height() {
    let mut layout = layout_with(2, CenterFocus::Never);
    assert!(layout.apply(LayoutOp::ConsumeOrExpel(Left)));
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.focused(), Some(WindowId(2)));
    let (a, b) = (rect_of(&layout, 1), rect_of(&layout, 2));
    assert_eq!(a.x, b.x);
    assert_eq!(a.bottom() + 10.0, b.y);
    assert_eq!(b.bottom(), 590.0);
}

#[test]
fn consume_to_the_right_joins_the_right_neighbour() {
    let mut layout = layout_with(3, CenterFocus::Never);
    layout.focus(WindowId(1));
    assert!(layout.apply(LayoutOp::ConsumeOrExpel(Right)));
    assert_eq!(rect_of(&layout, 1).x, rect_of(&layout, 2).x);
    assert_eq!(layout.focused(), Some(WindowId(1)));
    assert_eq!(order(&layout), [2, 1, 3]);
}

#[test]
fn consume_at_the_edge_does_nothing() {
    let mut layout = layout_with(1, CenterFocus::Never);
    assert!(!layout.apply(LayoutOp::ConsumeOrExpel(Left)));
    assert!(!layout.apply(LayoutOp::ConsumeOrExpel(Right)));
}

#[test]
fn expel_gives_a_window_its_own_column_back() {
    let mut layout = layout_with(2, CenterFocus::Never);
    layout.apply(LayoutOp::ConsumeOrExpel(Left));
    assert!(layout.apply(LayoutOp::ConsumeOrExpel(Right)));
    assert_ne!(rect_of(&layout, 1).x, rect_of(&layout, 2).x);
    assert_eq!(rect_of(&layout, 1).h, 580.0);
    assert_eq!(layout.focused(), Some(WindowId(2)));
}

#[test]
fn vertical_focus_and_move_stay_inside_the_column() {
    let mut layout = layout_with(2, CenterFocus::Never);
    layout.apply(LayoutOp::ConsumeOrExpel(Left));
    assert!(layout.apply(LayoutOp::Focus(Up)));
    assert_eq!(layout.focused(), Some(WindowId(1)));
    assert!(!layout.apply(LayoutOp::Focus(Up)));
    assert!(layout.apply(LayoutOp::Move(Down)));
    assert_eq!(order(&layout), [2, 1]);
    assert_eq!(layout.focused(), Some(WindowId(1)));
    assert!(!layout.apply(LayoutOp::Move(Down)));
}

#[test]
fn closing_the_focused_column_focuses_a_neighbour() {
    let mut layout = layout_with(3, CenterFocus::Never);
    layout.focus(WindowId(2));
    assert!(layout.remove(WindowId(2)));
    assert_eq!(layout.focused(), Some(WindowId(3)));
    assert!(layout.remove(WindowId(3)));
    assert_eq!(layout.focused(), Some(WindowId(1)));
    assert!(layout.remove(WindowId(1)));
    assert_eq!(layout.focused(), None);
    assert!(!layout.remove(WindowId(1)));
}

#[test]
fn closing_a_column_left_of_focus_keeps_focus() {
    let mut layout = layout_with(3, CenterFocus::Never);
    assert!(layout.remove(WindowId(1)));
    assert_eq!(layout.focused(), Some(WindowId(3)));
}

#[test]
fn closing_inside_a_column_keeps_focus_on_the_right_window() {
    let mut layout = layout_with(3, CenterFocus::Never);
    layout.apply(LayoutOp::ConsumeOrExpel(Left));
    layout.focus(WindowId(2));
    layout.apply(LayoutOp::ConsumeOrExpel(Right));
    layout.apply(LayoutOp::ConsumeOrExpel(Left));
    // Column: [3, 2], window 2 focused and last.
    assert!(layout.remove(WindowId(3)));
    assert_eq!(layout.focused(), Some(WindowId(2)));
    assert_eq!(rect_of(&layout, 2).h, 580.0);
}

#[test]
fn the_view_closes_the_gap_when_the_strip_shrinks() {
    let mut layout = layout_with(4, CenterFocus::Never);
    layout.remove(WindowId(4));
    layout.remove(WindowId(3));
    assert_eq!(rect_of(&layout, 1).x, 10.0);
}

#[test]
fn cycle_width_steps_through_presets_and_wraps() {
    let mut layout = layout_with(1, CenterFocus::Never);
    let width = |l: &Scrolling| rect_of(l, 1).w;
    let half = width(&layout);
    layout.apply(LayoutOp::CycleWidth);
    assert!(width(&layout) > half);
    layout.apply(LayoutOp::CycleWidth);
    assert!(width(&layout) < half);
    layout.apply(LayoutOp::CycleWidth);
    assert_eq!(width(&layout), half);
}

#[test]
fn adjust_width_is_clamped() {
    let mut layout = layout_with(1, CenterFocus::Never);
    assert!(layout.apply(LayoutOp::AdjustWidth(0.1)));
    assert!(layout.apply(LayoutOp::AdjustWidth(5.0)));
    assert!(!layout.apply(LayoutOp::AdjustWidth(1.0)));
    assert_eq!(rect_of(&layout, 1).w, 980.0);
}

#[test]
fn maximize_toggles_the_column_to_the_full_width() {
    let mut layout = layout_with(1, CenterFocus::Never);
    let before = rect_of(&layout, 1).w;
    layout.apply(LayoutOp::ToggleMaximize);
    assert_eq!(rect_of(&layout, 1).w, 980.0);
    layout.apply(LayoutOp::ToggleMaximize);
    assert_eq!(rect_of(&layout, 1).w, before);
}

#[test]
fn center_is_a_one_shot() {
    let mut layout = layout_with(3, CenterFocus::Never);
    assert!(layout.apply(LayoutOp::Center));
    let (cx, _) = rect_of(&layout, 3).center();
    assert!((cx - 500.0).abs() <= 1.0);
}

#[test]
fn scroll_pans_without_moving_focus_and_stops_at_the_ends() {
    let mut layout = layout_with(3, CenterFocus::Never);
    let before = rect_of(&layout, 3).x;
    assert!(layout.apply(LayoutOp::Scroll(-100.0)));
    assert_eq!(rect_of(&layout, 3).x, before + 100.0);
    assert_eq!(layout.focused(), Some(WindowId(3)));
    assert!(layout.apply(LayoutOp::Scroll(-1e6)));
    assert_eq!(rect_of(&layout, 1).x, 10.0);
    assert!(!layout.apply(LayoutOp::Scroll(-1.0)));
}

#[test]
fn a_new_area_is_respected_including_its_origin() {
    let mut layout = layout_with(1, CenterFocus::Never);
    layout.set_area(Rect::new(1920.0, 30.0, 1000.0, 600.0));
    let rect = rect_of(&layout, 1);
    assert_eq!((rect.x, rect.y), (1930.0, 40.0));
}

#[test]
fn window_sizes_are_whole_pixels() {
    let mut layout = layout_with(3, CenterFocus::Never);
    layout.set_area(Rect::new(0.0, 0.0, 1367.0, 769.0));
    layout.apply(LayoutOp::ConsumeOrExpel(Left));
    for p in places(&layout) {
        assert_eq!(p.rect.w.fract(), 0.0);
        assert_eq!(p.rect.h.fract(), 0.0);
    }
}

#[test]
fn degenerate_areas_stay_finite_and_positive() {
    let mut layout = layout_with(5, CenterFocus::Always);
    layout.set_area(Rect::default());
    for p in places(&layout) {
        assert!(p.rect.w >= 1.0 && p.rect.h >= 1.0 && p.rect.x.is_finite());
    }
}
