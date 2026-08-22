use super::Fx;

#[test]
fn from_pt_rounds_half_away() {
    // 1/128 pt = exactly half a fixed unit: rounds away from zero.
    assert_eq!(Fx::from_pt(1.0 / 128.0), Fx(1));
    assert_eq!(Fx::from_pt(-1.0 / 128.0), Fx(-1));
    // Just under half a unit stays at zero.
    assert_eq!(Fx::from_pt(0.99 / 128.0), Fx::ZERO);
    assert_eq!(Fx::from_pt(-0.99 / 128.0), Fx::ZERO);
}

#[test]
fn from_pt_boundaries() {
    assert_eq!(Fx::from_pt(0.0), Fx::ZERO);
    assert_eq!(Fx::from_pt(1.0), Fx(64));
    assert_eq!(Fx::from_pt(-1.0), Fx(-64));
    // 1.5 units → 2 (away from zero), -1.5 units → -2.
    assert_eq!(Fx::from_pt(1.5 / 64.0), Fx(2));
    assert_eq!(Fx::from_pt(-1.5 / 64.0), Fx(-2));
    // 2.5 units rounds away to 3, not banker's 2.
    assert_eq!(Fx::from_pt(2.5 / 64.0), Fx(3));
    // Non-finite and huge inputs never panic.
    assert_eq!(Fx::from_pt(f64::NAN), Fx::ZERO);
    assert_eq!(Fx::from_pt(f64::INFINITY), Fx(i32::MAX));
    assert_eq!(Fx::from_pt(f64::NEG_INFINITY), Fx(i32::MIN));
    assert_eq!(Fx::from_pt(1e18), Fx(i32::MAX));
}

#[test]
fn to_f32_is_exact_for_small_values() {
    assert_eq!(Fx(64).to_f32(), 1.0);
    assert_eq!(Fx(-96).to_f32(), -1.5);
    assert_eq!(Fx::ZERO.to_f32(), 0.0);
}

#[test]
fn scale_font_units_matches_reference() {
    // upem 1000, size 16 pt = 1024 units: value = (u * 1024 + 500) / 1000.
    let size = Fx::from_pt(16.0);
    assert_eq!(size, Fx(1024));
    let scale = |u: i32| Fx::scale_font_units(u, size, 1000);
    assert_eq!(scale(0), Fx(0));
    assert_eq!(scale(1000), Fx(1024));
    assert_eq!(scale(500), Fx(((500i64 * 1024 + 500) / 1000) as i32));
    assert_eq!(scale(500), Fx(512));
    assert_eq!(scale(1), Fx(1)); // (1024 + 500) / 1000 = 1
    assert_eq!(scale(333), Fx(341)); // (340992 + 500) / 1000 = 341
    // Negatives: round half up (toward +∞); exact multiples stay exact.
    assert_eq!(scale(-1000), Fx(-1024));
    // -333 × 1024 / 1000 = -340.992 → -341.
    assert_eq!(scale(-333), Fx(-341));
    // Zero upem is a guarded impossibility, not a panic.
    assert_eq!(Fx::scale_font_units(100, size, 0), Fx::ZERO);
}

#[test]
fn saturating_never_wraps() {
    assert_eq!(Fx(i32::MAX).saturating_add(Fx(1)), Fx(i32::MAX));
    assert_eq!(Fx(i32::MAX) + Fx(i32::MAX), Fx(i32::MAX));
    assert_eq!(Fx(i32::MIN) - Fx(1), Fx(i32::MIN));
    assert_eq!(Fx(i32::MIN).saturating_sub(Fx(i32::MAX)), Fx(i32::MIN));
    assert_eq!(-Fx(i32::MIN), Fx(i32::MAX));
    assert_eq!(Fx(i32::MAX).mul_ratio(i32::MAX, 1), Fx(i32::MAX));
    assert_eq!(Fx(i32::MAX).mul_ratio(i32::MIN, 1), Fx(i32::MIN));
}

#[test]
fn mul_ratio_rounds_half_up() {
    // 3 × 1/2 = 1.5 → 2 (half up).
    assert_eq!(Fx(3).mul_ratio(1, 2), Fx(2));
    // -3 × 1/2 = -1.5 → -1 (half up = toward +∞).
    assert_eq!(Fx(-3).mul_ratio(1, 2), Fx(-1));
    // Sign normalization: negative denominator.
    assert_eq!(Fx(3).mul_ratio(-1, -2), Fx(2));
    assert_eq!(Fx(3).mul_ratio(1, -2), Fx(-1));
    // Zero denominator saturates by sign.
    assert_eq!(Fx(1).mul_ratio(1, 0), Fx(i32::MAX));
    assert_eq!(Fx(-1).mul_ratio(1, 0), Fx(i32::MIN));
    assert_eq!(Fx(0).mul_ratio(1, 0), Fx::ZERO);
}
