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

// --- floating -------------------------------------------------------------

const FLOAT: WindowHint = WindowHint {
    width: None,
    floating: true,
};

fn with_one_tiled_and_one_floating() -> Desktop {
    let mut desktop = desktop();
    desktop.add_window(WindowId(1), WindowHint::default());
    desktop.add_window(WindowId(2), FLOAT);
    desktop.fit_floating(WindowId(2), 300.0, 200.0, None);
    desktop
}

#[test]
fn a_floating_window_is_centred_on_top_and_focused() {
    let desktop = with_one_tiled_and_one_floating();
    assert_eq!(desktop.focused_window(), Some(WindowId(2)));
    let rect = desktop.floating(WindowId(2)).expect("floating").rect;
    assert_eq!(rect, Rect::new(350.0, 200.0, 300.0, 200.0));
    let monitor = &desktop.monitors()[0];
    let mut placed = Vec::new();
    Desktop::place(monitor, monitor.active_workspace(), &mut placed);
    let last = placed.last().expect("placements");
    assert!(last.id == WindowId(2) && last.floating);
}

#[test]
fn a_workspace_with_only_a_floating_window_is_not_empty() {
    let mut desktop = desktop();
    desktop.add_window(WindowId(1), FLOAT);
    assert_eq!(desktop.monitors()[0].workspaces.len(), 2);
    assert!(desktop.focus_workspace(Down));
    assert_eq!(
        desktop.monitors()[0].workspaces.len(),
        2,
        "it's still there"
    );
    assert!(desktop.remove_window(WindowId(1)));
    assert_eq!(desktop.monitors()[0].workspaces.len(), 1);
}

#[test]
fn toggling_keeps_the_window_where_it_is_and_back() {
    let mut desktop = desktop();
    desktop.add_window(WindowId(1), WindowHint::default());
    let tiled = desktop.rect_of(WindowId(1)).expect("placed");
    assert!(desktop.toggle_floating());
    assert_eq!(desktop.floating(WindowId(1)).expect("floating").rect, tiled);
    assert_eq!(desktop.focused_window(), Some(WindowId(1)));
    assert!(desktop.toggle_floating());
    assert!(desktop.floating(WindowId(1)).is_none());
    assert_eq!(desktop.focused_window(), Some(WindowId(1)));
}

#[test]
fn focus_moves_between_floating_and_the_layout() {
    let mut desktop = with_one_tiled_and_one_floating();
    desktop.apply(LayoutOp::Focus(Dir::Left));
    assert_eq!(desktop.focused_window(), Some(WindowId(1)));
    assert!(desktop.switch_floating_focus());
    assert_eq!(desktop.focused_window(), Some(WindowId(2)));
    assert!(desktop.switch_floating_focus());
    assert_eq!(desktop.focused_window(), Some(WindowId(1)));
}

#[test]
fn moving_a_focused_floating_window_nudges_it() {
    let mut desktop = with_one_tiled_and_one_floating();
    let before = desktop.floating(WindowId(2)).expect("floating").rect;
    assert!(desktop.apply(LayoutOp::Move(Dir::Right)));
    let after = desktop.floating(WindowId(2)).expect("floating").rect;
    assert!(after.x > before.x && after.y == before.y);
    assert!(!desktop.apply(LayoutOp::CycleWidth), "that's for columns");
}

#[test]
fn focusing_a_floating_window_raises_it() {
    let mut desktop = desktop();
    desktop.add_window(WindowId(1), FLOAT);
    desktop.add_window(WindowId(2), FLOAT);
    assert!(desktop.focus_window(WindowId(1)));
    assert_eq!(desktop.focused_window(), Some(WindowId(1)));
    let workspace = desktop.monitors()[0].active_workspace();
    assert_eq!(workspace.floating.last().map(|f| f.id), Some(WindowId(1)));
}

#[test]
fn a_floating_window_goes_to_another_workspace_still_floating() {
    let mut desktop = with_one_tiled_and_one_floating();
    assert!(desktop.move_to_workspace(Down));
    assert!(desktop.floating(WindowId(2)).is_some());
    assert_eq!(desktop.focused_window(), Some(WindowId(2)));
}

#[test]
fn a_floating_window_comes_along_when_its_monitor_moves_or_goes() {
    let mut desktop = desktop();
    desktop.add_monitor(2, RIGHT);
    desktop.focus_monitor_at(1300.0, 10.0);
    desktop.add_window(WindowId(1), FLOAT);
    desktop.fit_floating(WindowId(1), 100.0, 100.0, None);
    let start = desktop.floating(WindowId(1)).expect("floating").rect;
    assert!(start.x >= 1000.0);
    desktop.remove_monitor(2);
    let adopted = desktop.floating(WindowId(1)).expect("floating").rect;
    assert_eq!(adopted.x, start.x - 1000.0, "still on a screen");
}

#[test]
fn dropping_a_window_on_another_monitor_moves_it_there() {
    let mut desktop = desktop();
    desktop.add_monitor(2, RIGHT);
    desktop.add_window(WindowId(1), FLOAT);
    desktop.fit_floating(WindowId(1), 100.0, 100.0, None);
    desktop.set_floating_rect(WindowId(1), Rect::new(1500.0, 100.0, 100.0, 100.0));
    desktop.rehome_floating(WindowId(1));
    let on_second = desktop.monitors()[1].active_workspace();
    assert!(on_second.contains(WindowId(1)));
    assert_eq!(desktop.focused_monitor().map(|m| m.id), Some(2));
}

#[test]
fn a_dialog_is_centred_over_the_window_it_belongs_to() {
    let mut desktop = desktop();
    desktop.add_window(WindowId(1), WindowHint::default());
    let parent = desktop.rect_of(WindowId(1)).expect("placed");
    desktop.add_window(WindowId(2), FLOAT);
    desktop.fit_floating(WindowId(2), 100.0, 60.0, Some(WindowId(1)));
    let dialog = desktop.floating(WindowId(2)).expect("floating").rect;
    assert_eq!(dialog.center().0.round(), parent.center().0.round());
}

#[test]
fn removing_the_focused_floating_window_gives_focus_back() {
    let mut desktop = with_one_tiled_and_one_floating();
    assert!(desktop.remove_window(WindowId(2)));
    assert_eq!(desktop.focused_window(), Some(WindowId(1)));
}

#[test]
fn a_floating_window_can_be_fullscreen() {
    let mut desktop = with_one_tiled_and_one_floating();
    assert!(desktop.toggle_fullscreen());
    assert!(desktop.is_fullscreen(WindowId(2)));
    let monitor = &desktop.monitors()[0];
    let mut placed = Vec::new();
    Desktop::place(monitor, monitor.active_workspace(), &mut placed);
    let window = placed.iter().find(|p| p.id == WindowId(2)).expect("placed");
    assert!(window.fullscreen && window.rect == monitor.rect);
}
