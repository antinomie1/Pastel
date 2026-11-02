use pastel_core::FrameClock;

const HZ_60: f64 = 1.0 / 60.0;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn with_nothing_presented_it_is_just_now() {
    assert_eq!(FrameClock::default().next_frame(3.0), 3.0);
}

#[test]
fn it_answers_the_next_vblank_after_now() {
    let mut clock = FrameClock::default();
    clock.presented(10.0, HZ_60);
    assert!(near(clock.next_frame(10.001), 10.0 + HZ_60));
    assert!(near(
        clock.next_frame(10.0 + 2.5 * HZ_60),
        10.0 + 3.0 * HZ_60
    ));
}

#[test]
fn drawing_late_in_a_frame_lands_on_the_same_vblank_as_early() {
    let mut clock = FrameClock::default();
    clock.presented(10.0, HZ_60);
    let early = clock.next_frame(10.0 + 0.1 * HZ_60);
    let late = clock.next_frame(10.0 + 0.9 * HZ_60);
    assert!(near(early, late));
}

#[test]
fn it_is_never_in_the_past() {
    let mut clock = FrameClock::default();
    clock.presented(10.0, HZ_60);
    assert!(clock.next_frame(9.0) > 9.0);
    assert!(clock.next_frame(500.0) > 500.0);
}

#[test]
fn an_unknown_refresh_forgets() {
    let mut clock = FrameClock::default();
    clock.presented(10.0, HZ_60);
    clock.presented(11.0, 0.0);
    assert_eq!(clock.next_frame(12.0), 12.0);
}
