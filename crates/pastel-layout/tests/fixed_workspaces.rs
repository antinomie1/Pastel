//! With a fixed count, every monitor keeps that many workspaces, as dwm's tags.

use pastel_layout::{
    Desktop, LayoutOptions, Rect, WindowHint, WindowId,
    WorkspaceTarget::{Down, Index},
};

const SCREEN: Rect = Rect::new(0.0, 0.0, 1000.0, 600.0);
const RIGHT: Rect = Rect::new(1000.0, 0.0, 1000.0, 600.0);
const A: WindowId = WindowId(1);
const B: WindowId = WindowId(2);

fn fixed(count: usize) -> Desktop {
    let options = LayoutOptions {
        workspaces: count,
        ..LayoutOptions::default()
    };
    let mut d = Desktop::new(options).expect("scrolling exists");
    d.add_monitor(1, SCREEN);
    d
}

fn count(d: &Desktop) -> usize {
    d.focused_monitor().expect("monitor").workspaces.len()
}

fn active(d: &Desktop) -> usize {
    d.focused_monitor().expect("monitor").active
}

#[test]
fn every_monitor_has_its_workspaces_empty_or_not() {
    let mut d = fixed(9);
    assert_eq!(count(&d), 9);
    d.add_window(A, WindowHint::default());
    assert_eq!(count(&d), 9, "no spare is added");
    assert!(d.focus_workspace(Index(6)));
    assert_eq!(active(&d), 6);
    assert!(d.focus_workspace(Index(2)));
    assert_eq!((count(&d), active(&d)), (9, 2), "empty ones stay");
    assert!(d.focus_workspace(Index(20)));
    assert_eq!(active(&d), 8, "past the end is the last one");
    assert!(!d.focus_workspace(Down), "nothing below the last");
}

#[test]
fn windows_move_between_fixed_workspaces_by_number() {
    let mut d = fixed(9);
    d.add_window(A, WindowHint::default());
    d.add_window(B, WindowHint::default());
    assert!(d.move_to_workspace(Index(4)));
    let workspaces = &d.focused_monitor().unwrap().workspaces;
    assert!(workspaces[4].contains(B));
    assert!(workspaces[0].contains(A));
    assert_eq!(workspaces.len(), 9);
}

#[test]
fn an_unplugged_monitors_windows_follow_the_fixed_ones() {
    let mut d = fixed(3);
    d.add_monitor(2, RIGHT);
    d.add_window(A, WindowHint::default());
    // A is on the first monitor's first workspace; put B on the second monitor.
    assert!(d.focus_monitor(pastel_layout::Direction::Right));
    d.add_window(B, WindowHint::default());
    d.remove_monitor(2);
    let workspaces = &d.monitors()[0].workspaces;
    assert_eq!(workspaces.len(), 4, "the adopted one comes after the fixed ones");
    assert!(workspaces[3].contains(B));
    // Once emptied and left, it goes; the fixed ones stay.
    assert!(d.focus_workspace(Index(3)));
    assert!(d.remove_window(B));
    assert!(d.focus_workspace(Index(0)));
    assert_eq!(count(&d), 3);
}

#[test]
fn changing_the_count_takes_effect_at_once() {
    let mut d = Desktop::new(LayoutOptions::default()).unwrap();
    d.add_monitor(1, SCREEN);
    assert_eq!(count(&d), 1);
    let options = LayoutOptions {
        workspaces: 5,
        ..LayoutOptions::default()
    };
    assert!(d.set_options(options));
    assert_eq!(count(&d), 5);
    assert!(d.set_options(LayoutOptions::default()));
    assert_eq!(count(&d), 1, "back to one spare");
}
