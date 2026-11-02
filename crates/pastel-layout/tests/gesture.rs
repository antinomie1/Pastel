use pastel_layout::{LayoutOp, Swipe, WorkspaceTarget};

#[test]
fn a_small_wobble_does_nothing() {
    let mut swipe = Swipe::default();
    assert_eq!(swipe.update(3.0, 2.0), None);
    assert_eq!(swipe.finish(false), None);
}

#[test]
fn sideways_pans_with_the_fingers_and_nothing_is_lost() {
    let mut swipe = Swipe::default();
    assert_eq!(swipe.update(10.0, 0.0), None);
    assert_eq!(swipe.update(10.0, 0.0), Some(LayoutOp::Scroll(-40.0)));
    assert_eq!(swipe.update(5.0, 0.0), Some(LayoutOp::Scroll(-10.0)));
    assert_eq!(swipe.finish(false), None, "no workspace change");
}

#[test]
fn up_and_down_change_workspace_at_the_end() {
    let mut up = Swipe::default();
    assert_eq!(up.update(0.0, -80.0), None);
    assert_eq!(up.finish(false), Some(WorkspaceTarget::Down));
    let mut down = Swipe::default();
    assert_eq!(down.update(0.0, 80.0), None);
    assert_eq!(down.finish(false), Some(WorkspaceTarget::Up));
}

#[test]
fn a_short_or_cancelled_push_stays() {
    let mut short = Swipe::default();
    short.update(0.0, -30.0);
    assert_eq!(short.finish(false), None);
    let mut cancelled = Swipe::default();
    cancelled.update(0.0, -90.0);
    assert_eq!(cancelled.finish(true), None);
}

#[test]
fn the_first_direction_decides() {
    let mut swipe = Swipe::default();
    swipe.update(0.0, 20.0);
    assert_eq!(swipe.update(500.0, 0.0), None, "this one is vertical");
}
