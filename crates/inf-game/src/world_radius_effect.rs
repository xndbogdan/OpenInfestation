//! Shared radius-effect arithmetic, 0042A800, and WORD-vector distance 00462DA0.
//!
//! The ten-byte block is used by selector-11 +1A0 and EXE-authored effects.
//! Consumers deliberately disagree about radius signedness: this falloff uses
//! unsigned WORD radii; terrain traversal 00436390 uses signed low WORD +6.
//! The 004887D0 dispatcher retains the original block/position identity and
//! orders the required terrain, actor and network services. Falloff arithmetic
//! remains independent of those mutable services.

mod dispatch;

pub use dispatch::{
    apply_world_radius_effect, WorldRadiusEffectError, WorldRadiusEffectHost,
    WorldRadiusEffectOutcome, WorldRadiusEffectRequest,
};

use inf_formats::native_basis::isqrt_48efc0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadiusEffectBlock {
    pub strength: i16,
    /// Raw byte +2, consumed as the damage-table channel by 0042A8F0.
    pub channel: u8,
    /// Raw byte +3. Selector-11 construction leaves it unwritten; command 20
    /// supplies it. Copying an unknown byte does not establish its value.
    pub byte_03: Option<u8>,
    pub inner_radius: u16,
    pub outer_radius: u16,
    pub impulse: u16,
}

impl RadiusEffectBlock {
    pub const fn from_authored_bytes(bytes: [u8; 10]) -> Self {
        Self {
            strength: i16::from_le_bytes([bytes[0], bytes[1]]),
            channel: bytes[2],
            byte_03: Some(bytes[3]),
            inner_radius: u16::from_le_bytes([bytes[4], bytes[5]]),
            outer_radius: u16::from_le_bytes([bytes[6], bytes[7]]),
            impulse: u16::from_le_bytes([bytes[8], bytes[9]]),
        }
    }

    /// The ten bytes again, as `0045DFF0` copies them into message 19;
    /// None while byte +3 is unwritten.
    pub fn bytes(&self) -> Option<[u8; 10]> {
        let mut bytes = [0; 10];
        bytes[..2].copy_from_slice(&self.strength.to_le_bytes());
        bytes[2] = self.channel;
        bytes[3] = self.byte_03?;
        bytes[4..6].copy_from_slice(&self.inner_radius.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.outer_radius.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.impulse.to_le_bytes());
        Some(bytes)
    }
}

/// `00492500(1, 0x13, &record)` from `004887D0` when the blast reached
/// another machine's actors: the source, then the position and the block
/// the record points to (`0045DFF0`).
pub fn radius_effect_send(
    source: crate::object_runtime::ActorHandle,
    position: [u16; 3],
    block: RadiusEffectBlock,
) -> Result<crate::network::protocol::GameSend, &'static str> {
    let record = block
        .bytes()
        .ok_or("the blast's block byte +3 is unwritten where 0045DFF0 copies it")?;
    let mut point = [0; 6];
    for (axis, word) in position.iter().enumerate() {
        point[axis * 2..axis * 2 + 2].copy_from_slice(&word.to_le_bytes());
    }
    Ok(crate::network::protocol::GameSend::Effect {
        value: source.raw(),
        point,
        record,
    })
}

/// The first output DWORD and optional impulse WORDs written by 0042A800.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaledRadiusEffect {
    pub strength: i16,
    pub channel: u8,
    pub byte_03: Option<u8>,
    pub impulse_magnitude: u32,
    pub impulse_vector: Option<[i16; 3]>,
}

impl ScaledRadiusEffect {
    /// Native low AL: a supplied impulse destination returns one even when
    /// the resulting strength/vector is zero. Without it, only strength gates.
    pub const fn accepted(self) -> bool {
        self.impulse_vector.is_some() || self.strength != 0
    }
}

/// 0042A800: `None` means the outer-radius gate returned before output writes.
/// A zero-strength `Some` still records the reached first-DWORD publication.
/// Signed distance and signed direction words follow the actual consumers;
/// channel/byte +3 are copied verbatim and never interpreted here.
pub fn scale_radius_effect(
    block: RadiusEffectBlock,
    distance: i32,
    direction: Option<[i16; 3]>,
) -> Option<ScaledRadiusEffect> {
    let outer = i32::from(block.outer_radius);
    if distance >= outer {
        return None;
    }
    let mut strength = block.strength;
    let mut impulse = u32::from(block.impulse);
    let inner = i32::from(block.inner_radius);
    if distance > inner {
        let span = outer - inner;
        let remaining = outer - distance;
        // 0042A843's equality branch retains saturated Q31 unity; normal
        // ordered radii enter the division arm because distance > inner.
        let ratio = if remaining == span {
            i32::MAX
        } else {
            (remaining.wrapping_shl(15) / span).wrapping_shl(16)
        };
        // IMUL followed by SHL low/RCL high selects product bits 31..62.
        strength = ((i64::from(strength) * i64::from(ratio)) >> 31) as i16;
        impulse = ((i64::from(block.impulse) * i64::from(ratio)) >> 31) as u32;
    }
    let impulse_vector = direction.map(|direction| {
        let divisor = if distance == 0 { 1 } else { distance };
        direction.map(|word| (i32::from(word).wrapping_mul(impulse as i32) / divisor) as i16)
    });
    Some(ScaledRadiusEffect {
        strength,
        channel: block.channel,
        byte_03: block.byte_03,
        impulse_magnitude: impulse,
        impulse_vector,
    })
}

/// 00462DA0 sums squares of signed WORDs. When the DWORD sign bit is set,
/// it square-roots the unsigned sum divided by four, then doubles the result.
/// This is distinct from a saturating length or a floating-point square root.
pub fn radius_word_distance(vector: [i16; 3]) -> u32 {
    let sum = vector.into_iter().fold(0_u32, |sum, word| {
        let signed = i32::from(word);
        sum.wrapping_add(signed.wrapping_mul(signed) as u32)
    });
    if sum & 0x8000_0000 != 0 {
        isqrt_48efc0((sum >> 2) as i32).wrapping_mul(2)
    } else {
        isqrt_48efc0(sum as i32)
    }
}

#[cfg(test)]
#[path = "world_radius_effect_tests.rs"]
mod tests;
