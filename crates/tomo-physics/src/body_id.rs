// SPDX-FileCopyrightText: 2021 Jorrit Rouwe
// SPDX-FileCopyrightText: 2026 TomoEngine contributors
// SPDX-License-Identifier: MIT
//! Packed ID port from Jolt/Physics/Body/BodyID.h.

/// Validated packed representation, NOT proof that a body is alive or locked.
/// Sequence numbers wrap after 256 values; storage reuse needs a separate policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct BodyId(u32);

impl Default for BodyId {
    fn default() -> Self {
        Self::INVALID
    }
}

impl BodyId {
    pub const INVALID: Self = Self(u32::MAX);
    pub const BROAD_PHASE_BIT: u32 = 0x8000_0000;
    pub const MAX_INDEX: u32 = 0x007f_ffff;
    pub const MAX_SEQUENCE: u8 = u8::MAX;
    pub const SEQUENCE_SHIFT: u32 = 23;

    /// Checked in all build profiles, unlike an upstream debug-only assertion.
    pub const fn new(index: u32, sequence: u8) -> Option<Self> {
        if index > Self::MAX_INDEX {
            None
        } else {
            Some(Self(((sequence as u32) << Self::SEQUENCE_SHIFT) | index))
        }
    }

    /// Accepts INVALID, but rejects other values with the reserved top bit set.
    pub const fn from_raw(raw: u32) -> Option<Self> {
        if raw & Self::BROAD_PHASE_BIT == 0 || raw == u32::MAX {
            Some(Self(raw))
        } else {
            None
        }
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    /// INVALID returns MAX_INDEX, as in Jolt; check is_invalid before lookup.
    pub const fn index(self) -> u32 {
        self.0 & Self::MAX_INDEX
    }

    pub const fn sequence(self) -> u8 {
        (self.0 >> Self::SEQUENCE_SHIFT) as u8
    }

    pub const fn is_invalid(self) -> bool {
        self.0 == u32::MAX
    }
}
