use std::rc::Rc;

use pastel_core::{Anim, AnimRect, Curve, Ease, Easing, Rect, Slowed, Snap, Spring};

const CRITICAL: Spring = Spring {
    stiffness: 800.0,
    damping: 1.0,
};
const BOUNCY: Spring = Spring {
    stiffness: 800.0,
    damping: 0.5,
};
const SLUGGISH: Spring = Spring {
    stiffness: 800.0,
    damping: 2.0,
};

fn settle_time(curve: &dyn Curve) -> f64 {
    (0..20_000)
        .map(|i| f64::from(i) / 1000.0)
        .find(|&t| curve.sample(t, 0.0).done)
        .expect("curve never settles")
}

#[test]
fn springs_start_at_zero_and_settle_at_one() {
    for spring in [CRITICAL, BOUNCY, SLUGGISH] {
        let s = spring.sample(0.0, 0.0);
        assert!(s.value.abs() < 1e-9);
        assert!(s.velocity.abs() < 1e-9);
        let t = settle_time(&spring);
        assert!(t > 0.05 && t < 5.0, "{spring:?} settled at {t}");
        assert_eq!(spring.sample(t, 0.0).value, 1.0);
    }
}

#[test]
fn spring_velocity_is_the_derivative_of_value() {
    for spring in [CRITICAL, BOUNCY, SLUGGISH] {
        for t in [0.01, 0.05, 0.1, 0.2] {
            let dt = 1e-6;
            let (a, b) = (spring.sample(t, 3.0), spring.sample(t + dt, 3.0));
            let numeric = (b.value - a.value) / dt;
            assert!((numeric - a.velocity).abs() < 1e-2, "{spring:?} at {t}");
        }
    }
}

#[test]
fn only_an_underdamped_spring_overshoots() {
    let peak = |s: Spring| {
        (0..1000)
            .map(|i| s.sample(f64::from(i) / 500.0, 0.0).value)
            .fold(0.0, f64::max)
    };
    assert!(peak(BOUNCY) > 1.05);
    assert!(peak(CRITICAL) <= 1.0);
    assert!(peak(SLUGGISH) <= 1.0);
}

#[test]
fn eases_hit_their_endpoints() {
    use Easing::*;
    for easing in [Linear, OutQuad, OutCubic, OutQuint, OutExpo, InOutCubic] {
        assert!(easing.apply(0.0).abs() < 1e-3, "{easing:?}");
        assert!((easing.apply(1.0) - 1.0).abs() < 1e-3, "{easing:?}");
        assert!(easing.apply(0.25) < easing.apply(0.75), "{easing:?}");
    }
    let ease = Ease {
        seconds: 0.2,
        easing: Linear,
    };
    assert!((ease.sample(0.1, 0.0).value - 0.5).abs() < 1e-9);
    assert!(ease.sample(0.2, 0.0).done);
}

#[test]
fn slowed_stretches_time() {
    let base = Ease {
        seconds: 0.2,
        easing: Easing::Linear,
    };
    let slow = Slowed {
        inner: base,
        factor: 2.0,
    };
    assert!((slow.sample(0.2, 0.0).value - 0.5).abs() < 1e-9);
    assert!(!slow.sample(0.39, 0.0).done);
    assert!(slow.sample(0.4, 0.0).done);
}

#[test]
fn anim_interpolates_between_any_two_values() {
    let mut a = Anim::new(
        100.0,
        Rc::new(Ease {
            seconds: 1.0,
            easing: Easing::Linear,
        }),
    );
    a.set(300.0, 10.0);
    assert_eq!(a.value(10.0), 100.0);
    assert_eq!(a.value(10.5), 200.0);
    assert_eq!(a.value(11.0), 300.0);
    assert_eq!(a.target(), 300.0);
    assert!(a.is_done(11.0));
}

#[test]
fn anim_at_rest_is_done_and_setting_the_same_target_changes_nothing() {
    let mut a = Anim::new(5.0, Rc::new(CRITICAL));
    assert!(a.is_done(0.0));
    a.set(50.0, 1.0);
    let v = a.value(1.02);
    a.set(50.0, 1.02); // must not restart the clock
    assert_eq!(a.value(1.02), v);
}

#[test]
fn retarget_preserves_position_and_velocity() {
    let mut a = Anim::new(0.0, Rc::new(CRITICAL));
    a.set(1000.0, 0.0);
    let (x, v) = (a.value(0.05), a.velocity(0.05));
    assert!(v > 0.0);
    a.set(2000.0, 0.05);
    assert!((a.value(0.05) - x).abs() < 1e-9);
    assert!((a.velocity(0.05) - v).abs() < 1e-6);
}

#[test]
fn retarget_backwards_keeps_moving_forwards_for_a_moment() {
    let mut a = Anim::new(0.0, Rc::new(CRITICAL));
    a.set(1000.0, 0.0);
    let x = a.value(0.05);
    a.set(0.0, 0.05);
    assert!(a.value(0.051) > x, "the reversal should have momentum");
    assert!(a.value(5.0).abs() < 1.0);
}

#[test]
fn retarget_to_the_current_value_does_not_blow_up() {
    let mut a = Anim::new(0.0, Rc::new(CRITICAL));
    a.set(100.0, 0.0);
    let x = a.value(0.05);
    a.set(x, 0.05);
    assert!(a.value(0.06).is_finite());
}

#[test]
fn snap_curve_jumps() {
    let mut a = Anim::new(0.0, Rc::new(Snap));
    a.set(42.0, 0.0);
    assert_eq!(a.value(0.0), 42.0);
    assert!(!a.tick(0.0));
}

#[test]
fn shift_moves_the_whole_flight() {
    let mut a = Anim::new(
        0.0,
        Rc::new(Ease {
            seconds: 1.0,
            easing: Easing::Linear,
        }),
    );
    a.set(100.0, 0.0);
    a.shift(10.0);
    assert_eq!(a.value(0.5), 60.0);
    assert_eq!(a.target(), 110.0);
}

#[test]
fn anim_rect_settles_only_when_every_edge_has() {
    let start = Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut r = AnimRect::new(start, Rc::new(CRITICAL));
    assert!(!r.tick(0.0));
    let end = Rect::new(500.0, 40.0, 100.0, 300.0);
    r.set(end, 1.0);
    assert!(r.tick(1.05));
    assert!(!r.tick(10.0));
    assert_eq!(r.value(10.0), end);
    assert_eq!(r.target(), end);
}
