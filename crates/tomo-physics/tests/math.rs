// SPDX-License-Identifier: MIT
use tomo_physics::Vec3;
use tomo_physics::math::{Roots, difference_of_products, find_root};

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0e-6 * expected.abs().max(1.0),
        "actual={actual:?}, expected={expected:?}"
    );
}

#[test]
fn vector_arithmetic() {
    let a = Vec3::new(1.0, -2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, -6.0);
    assert_eq!(a + b, Vec3::new(5.0, 3.0, -3.0));
    assert_eq!(a - b, Vec3::new(-3.0, -7.0, 9.0));
    assert_eq!(a * b, Vec3::new(4.0, -10.0, -18.0));
    assert_eq!(2.0 * a, a * 2.0);
    assert_eq!((a * 2.0) / 2.0, a);
    assert_eq!(-a, Vec3::new(-1.0, 2.0, -3.0));
    assert_eq!(a.to_array(), [1.0, -2.0, 3.0]);
}

#[test]
fn vector_assignments() {
    let mut a = Vec3::ONE;
    a += Vec3::X;
    a -= Vec3::Y;
    a *= 4.0;
    a /= 2.0;
    assert_eq!(a, Vec3::new(4.0, 0.0, 2.0));
}

#[test]
fn dot_and_cross_handedness() {
    assert_eq!(Vec3::X.dot(Vec3::Y), 0.0);
    assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
    assert_eq!(Vec3::Y.cross(Vec3::X), -Vec3::Z);
    assert_eq!(Vec3::Y.cross_precise(Vec3::Z), Vec3::X);
    assert_eq!(Vec3::new(1.0, 2.0, 3.0).dot(Vec3::ONE), 6.0);
}

#[test]
fn component_operations() {
    let a = Vec3::new(-2.0, 5.0, 3.0);
    assert_eq!(a.abs(), Vec3::new(2.0, 5.0, 3.0));
    assert_eq!(a.min(Vec3::ONE), Vec3::new(-2.0, 1.0, 1.0));
    assert_eq!(a.max(Vec3::ONE), Vec3::new(1.0, 5.0, 3.0));
    assert_eq!(a[0], -2.0);
    assert_eq!(a[1], 5.0);
    assert_eq!(a[2], 3.0);
}

#[test]
#[should_panic(expected = "outside 0..3")]
fn invalid_vector_index_panics_safely() {
    let _ = Vec3::ZERO[3];
}

#[test]
fn normalization_and_length() {
    let v = Vec3::new(3.0, 4.0, 0.0);
    assert_eq!(v.length_squared(), 25.0);
    assert_eq!(v.length(), 5.0);
    close(v.normalized_or(Vec3::Z).length(), 1.0);
}

#[test]
fn zero_and_denormal_normalization_use_fallback() {
    assert_eq!(Vec3::ZERO.normalized_or(Vec3::Y), Vec3::Y);
    assert_eq!(Vec3::splat(1.0e-20).normalized_or(Vec3::Z), Vec3::Z);
}

#[test]
fn finite_classification() {
    assert!(Vec3::ONE.is_finite());
    assert!(!Vec3::new(f32::NAN, 0.0, 0.0).is_finite());
    assert!(!Vec3::new(0.0, f32::INFINITY, 0.0).is_finite());
}

#[test]
fn difference_of_products_regular() {
    assert_eq!(difference_of_products(3.0, 4.0, 2.0, 5.0), 2.0);
}

#[test]
fn constant_and_linear_roots() {
    assert_eq!(find_root(0.0, 0.0, 1.0), Roots::None);
    assert_eq!(find_root(0.0, 0.0, 0.0), Roots::One(0.0));
    assert_eq!(find_root(0.0, 2.0, -6.0), Roots::One(3.0));
    assert_eq!(find_root(0.0, -2.0, -6.0), Roots::One(-3.0));
}

#[test]
fn quadratic_roots_preserve_upstream_order() {
    assert_eq!(find_root(1.0, -3.0, 2.0), Roots::Two(2.0, 1.0));
    assert_eq!(find_root(1.0, 3.0, 2.0), Roots::Two(-2.0, -1.0));
    assert_eq!(find_root(1.0, 0.0, 1.0), Roots::None);
}

#[test]
fn repeated_root_count_matches_upstream() {
    assert_eq!(find_root(1.0, 2.0, 1.0), Roots::Two(-1.0, -1.0));
    let Roots::One(root) = find_root(1.0, 0.0, 0.0) else {
        panic!("q == 0 must produce one root");
    };
    assert_eq!(root.to_bits(), (-0.0_f32).to_bits());
}

#[test]
fn negative_zero_b_has_positive_sign() {
    assert_eq!(find_root(1.0, -0.0, -1.0), Roots::Two(-1.0, 1.0));
}

#[test]
fn cancellation_resistant_small_root() {
    let (large, small) = find_root(1.0, -100_000_000.0, 1.0).pair().unwrap();
    close(large, 100_000_000.0);
    assert!((small - 1.0e-8).abs() <= 1.0e-14);
    assert_ne!(small, 0.0);
}

#[test]
fn pair_expands_one_and_preserves_none() {
    assert_eq!(Roots::None.pair(), None);
    assert_eq!(Roots::One(2.0).pair(), Some((2.0, 2.0)));
}

#[test]
fn quadratic_residuals_for_integer_roots() {
    for first in -20..=20 {
        for second in -20..=20 {
            let a = first as f32;
            let b = second as f32;
            let (x, y) = find_root(1.0, -(a + b), a * b).pair().unwrap();
            close((x - a) * (x - b), 0.0);
            close((y - a) * (y - b), 0.0);
        }
    }
}
