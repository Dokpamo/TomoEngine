// SPDX-License-Identifier: MIT
use tomo_physics::BodyId;

#[test]
fn invalid_default_and_roundtrip() {
    let id = BodyId::default();
    assert!(id.is_invalid());
    assert_eq!(id.raw(), u32::MAX);
    assert_eq!(BodyId::from_raw(u32::MAX), Some(id));
    assert_eq!(id.index(), BodyId::MAX_INDEX);
    assert_eq!(id.sequence(), 255);
}

#[test]
fn packing_at_boundaries() {
    let id = BodyId::new(BodyId::MAX_INDEX, 255).unwrap();
    assert_eq!(id.raw(), 0x7fff_ffff);
    assert!(!id.is_invalid());
    assert_eq!(id.index(), BodyId::MAX_INDEX);
    assert_eq!(id.sequence(), 255);
}

#[test]
fn out_of_range_index_and_reserved_bit_are_rejected() {
    assert_eq!(BodyId::new(BodyId::MAX_INDEX + 1, 0), None);
    assert_eq!(BodyId::new(u32::MAX, 255), None);
    assert_eq!(BodyId::from_raw(0x8000_0000), None);
    assert_eq!(BodyId::from_raw(0xffff_fffe), None);
}

#[test]
fn all_sequences_roundtrip() {
    for sequence in 0..=255 {
        for index in [0, 1, 12345, BodyId::MAX_INDEX] {
            let id = BodyId::new(index, sequence).unwrap();
            assert_eq!(id.index(), index);
            assert_eq!(id.sequence(), sequence);
            assert_eq!(id.raw() & BodyId::BROAD_PHASE_BIT, 0);
            assert_eq!(BodyId::from_raw(id.raw()), Some(id));
        }
    }
}

#[test]
fn comparisons_use_packed_value() {
    let a = BodyId::new(1, 0).unwrap();
    let b = BodyId::new(0, 1).unwrap();
    assert!(a < b);
    assert_ne!(a, b);
    assert_eq!(std::mem::size_of::<BodyId>(), 4);
}
