use pastel_layout::{
    Desktop, Direction as Dir, LayoutOp, LayoutOptions, Placement, Rect, WindowHint, WindowId,
    WorkspaceTarget::{Down, Index, Up},
    create,
};

const SCREEN: Rect = Rect::new(0.0, 0.0, 1000.0, 600.0);
const RIGHT: Rect = Rect::new(1000.0, 0.0, 1000.0, 600.0);
const A: WindowId = WindowId(1);
const B: WindowId = WindowId(2);

fn desktop() -> Desktop {
    let mut d = Desktop::new(LayoutOptions::default()).expect("scrolling exists");
    d.add_monitor(1, SCREEN);
    d
}

fn add(d: &mut Desktop, id: WindowId) {
    d.add_window(id, WindowHint::default());
}

fn workspace_count(d: &Desktop) -> usize {
    d.focused_monitor().expect("monitor").workspaces.len()
}

fn active(d: &Desktop) -> usize {
    d.focused_monitor().expect("monitor").active
}

fn places(d: &Desktop) -> Vec<Placement> {
    let monitor = d.focused_monitor().expect("monitor");
    let mut out = Vec::new();
    Desktop::place(monitor, monitor.active_workspace(), &mut out);
    out
}

#[test]
fn unknown_layouts_are_refused() {
    let options = LayoutOptions {
        kind: "spiral".into(),
        ..LayoutOptions::default()
    };
    assert!(create(&options).is_none());
    assert!(Desktop::new(options).is_none());
}

#[test]
fn a_monitor_starts_with_one_empty_workspace() {
    assert_eq!(workspace_count(&desktop()), 1);
}

#[test]
fn filling_the_last_workspace_adds_an_empty_one() {
    let mut d = desktop();
    add(&mut d, A);
    assert_eq!(workspace_count(&d), 2);
    assert_eq!(active(&d), 0);
}

#[test]
fn the_empty_workspace_is_reachable_and_stays_single() {
    let mut d = desktop();
    add(&mut d, A);
    assert!(d.focus_workspace(Down));
    assert_eq!(active(&d), 1);
    assert_eq!(workspace_count(&d), 2);
    assert!(!d.focus_workspace(Down), "nothing below the empty one");
}

#[test]
fn leaving_an_empty_workspace_removes_it() {
    let mut d = desktop();
    add(&mut d, A);
    d.focus_workspace(Down);
    add(&mut d, B);
    assert_eq!(workspace_count(&d), 3);
    d.focus_workspace(Index(0));
    d.remove_window(A);
    assert_eq!(workspace_count(&d), 3, "still looked at, so it stays");
    d.focus_workspace(Down);
    assert_eq!(workspace_count(&d), 2);
    assert_eq!(active(&d), 0);
    assert_eq!(d.focused_window(), Some(B));
}

#[test]
fn removing_a_workspace_above_reports_the_view_shift() {
    let mut d = desktop();
    add(&mut d, A);
    d.focus_workspace(Down);
    add(&mut d, B);
    d.focus_workspace(Index(0));
    d.remove_window(A);
    d.focus_workspace(Down);
    let monitor = &mut d.monitors_mut()[0];
    assert_eq!(monitor.take_view_shift(), -1.0);
    assert_eq!(monitor.take_view_shift(), 0.0);
}

#[test]
fn closing_the_last_window_on_a_lower_workspace_keeps_it_while_looked_at() {
    let mut d = desktop();
    add(&mut d, A);
    d.focus_workspace(Down);
    add(&mut d, B);
    d.remove_window(B);
    assert_eq!(active(&d), 1);
    assert_eq!(workspace_count(&d), 2, "no trailing pair of empties");
}

#[test]
fn index_past_the_end_lands_on_the_empty_workspace() {
    let mut d = desktop();
    add(&mut d, A);
    assert!(d.focus_workspace(Index(9)));
    assert_eq!(active(&d), 1);
}

#[test]
fn moving_a_window_to_a_workspace_follows_it() {
    let mut d = desktop();
    add(&mut d, A);
    add(&mut d, B);
    assert!(d.move_to_workspace(Down));
    assert_eq!(active(&d), 1);
    assert_eq!(d.focused_window(), Some(B));
    assert_eq!(workspace_count(&d), 3);
    assert!(d.focus_workspace(Up));
    assert_eq!(d.focused_window(), Some(A));
}

#[test]
fn focusing_a_window_goes_to_its_workspace() {
    let mut d = desktop();
    add(&mut d, A);
    d.focus_workspace(Down);
    add(&mut d, B);
    assert!(d.focus_window(A));
    assert_eq!(active(&d), 0);
    assert_eq!(d.focused_window(), Some(A));
    assert!(!d.focus_window(WindowId(77)));
}

#[test]
fn fullscreen_covers_the_whole_monitor() {
    let mut d = desktop();
    add(&mut d, A);
    assert!(d.toggle_fullscreen());
    assert!(d.is_fullscreen(A));
    let p = places(&d)[0];
    assert!(p.fullscreen);
    assert_eq!(p.rect, SCREEN);
    assert!(d.toggle_fullscreen());
    assert!(!d.is_fullscreen(A));
    assert!(!places(&d)[0].fullscreen);
}

#[test]
fn fullscreen_ends_when_focus_leaves_or_a_window_opens() {
    let mut d = desktop();
    add(&mut d, A);
    add(&mut d, B);
    d.toggle_fullscreen();
    assert!(d.apply(LayoutOp::Focus(Dir::Left)));
    assert!(!d.is_fullscreen(B));
    d.toggle_fullscreen();
    add(&mut d, WindowId(3));
    assert!(!d.is_fullscreen(A));
}

#[test]
fn fullscreen_ends_with_the_window() {
    let mut d = desktop();
    add(&mut d, A);
    d.toggle_fullscreen();
    d.remove_window(A);
    assert_eq!(d.focused_window(), None);
}

#[test]
fn windows_wait_for_a_monitor() {
    let mut d = Desktop::new(LayoutOptions::default()).expect("scrolling exists");
    add(&mut d, A);
    assert_eq!(d.focused_window(), None);
    d.add_monitor(1, SCREEN);
    assert_eq!(d.focused_window(), Some(A));
    assert!(places(&d)[0].rect.w > 1.0);
    assert_eq!(workspace_count(&d), 2);
}

#[test]
fn panels_shrink_every_workspace() {
    let mut d = desktop();
    add(&mut d, A);
    d.set_monitor_geometry(1, SCREEN, Rect::new(0.0, 30.0, 1000.0, 570.0));
    assert_eq!(places(&d)[0].rect.y, 42.0);
    d.focus_workspace(Down);
    add(&mut d, B);
    assert_eq!(
        places(&d)[0].rect.y,
        42.0,
        "new workspaces inherit the usable area"
    );
}

#[test]
fn monitors_are_navigated_by_position() {
    let mut d = desktop();
    d.add_monitor(2, RIGHT);
    assert!(d.focus_monitor(Dir::Right));
    assert_eq!(d.focused_monitor().expect("monitor").id, 2);
    assert!(!d.focus_monitor(Dir::Right));
    assert!(!d.focus_monitor(Dir::Up));
    assert!(d.focus_monitor(Dir::Left));
    assert_eq!(d.focused_monitor().expect("monitor").id, 1);
}

#[test]
fn moving_to_a_monitor_follows_the_window() {
    let mut d = desktop();
    d.add_monitor(2, RIGHT);
    add(&mut d, A);
    assert!(d.move_to_monitor(Dir::Right));
    assert_eq!(d.focused_monitor().expect("monitor").id, 2);
    assert_eq!(d.focused_window(), Some(A));
    assert!(places(&d)[0].rect.x >= 1000.0);
    assert!(
        d.monitor(1)
            .expect("monitor")
            .active_workspace()
            .layout
            .is_empty()
    );
}

#[test]
fn the_pointer_can_change_the_focused_monitor() {
    let mut d = desktop();
    d.add_monitor(2, RIGHT);
    d.focus_monitor_at(1500.0, 300.0);
    assert_eq!(d.focused_monitor().expect("monitor").id, 2);
    d.focus_monitor_at(-5.0, -5.0);
    assert_eq!(
        d.focused_monitor().expect("monitor").id,
        2,
        "nowhere keeps the old one"
    );
}

#[test]
fn unplugging_moves_windows_to_the_remaining_monitor() {
    let mut d = desktop();
    d.add_monitor(2, RIGHT);
    d.focus_monitor(Dir::Right);
    add(&mut d, A);
    d.remove_monitor(2);
    assert_eq!(d.monitors().len(), 1);
    assert!(d.focus_window(A));
    assert!(places(&d)[0].rect.x < 1000.0);
    let last = d.monitors()[0].workspaces.last().expect("workspace");
    assert!(last.layout.is_empty(), "the spare stays at the bottom");
}

#[test]
fn unplugging_everything_parks_windows_until_a_monitor_returns() {
    let mut d = desktop();
    add(&mut d, A);
    d.remove_monitor(1);
    assert!(d.monitors().is_empty());
    assert_eq!(d.focused_window(), None);
    assert!(!d.remove_window(WindowId(9)));
    d.add_monitor(3, RIGHT);
    assert_eq!(d.focused_window(), Some(A));
    assert!(places(&d)[0].rect.x >= 1000.0);
}

#[test]
fn workspace_ids_survive_reordering() {
    let mut d = desktop();
    add(&mut d, A);
    d.focus_workspace(Down);
    add(&mut d, B);
    let id = d.focused_monitor().expect("monitor").active_workspace().id;
    d.focus_workspace(Index(0));
    d.remove_window(A);
    d.focus_workspace(Down);
    assert_eq!(
        d.focused_monitor().expect("monitor").active_workspace().id,
        id
    );
}

#[test]
fn acting_with_no_monitor_or_window_is_harmless() {
    let mut d = Desktop::new(LayoutOptions::default()).expect("scrolling exists");
    assert!(!d.apply(LayoutOp::CycleWidth));
    assert!(!d.toggle_fullscreen());
    assert!(!d.focus_workspace(Down));
    assert!(!d.move_to_workspace(Down));
    assert!(!d.focus_monitor(Dir::Left));
    assert!(!d.move_to_monitor(Dir::Left));
    let mut d = desktop();
    assert!(!d.apply(LayoutOp::CycleWidth));
    assert!(!d.move_to_workspace(Down));
    assert!(!d.toggle_fullscreen());
}
