//! 00429FF0: quantized horizontal basis, ordered six-point scan, and conditional
//! farther/rear probes. Each 00454ED0 boundary reloads the actual +234 owner.

use thiserror::Error;

use super::super::kinematics::ftol;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

pub trait FamilyObstacleProbeHost {
    type Error;

    /// 00454ED0 against the reached, nonnull +234 cache. The supplied X/Z are
    /// the unsigned low WORDs consumed by that routine; upper stack bits have
    /// no coordinate meaning. Probe and miss callbacks may mutate the actor.
    fn probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        position_words: [u16; 2],
    ) -> Result<u32, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyObstacleScanError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {actor:?} requires position at {address:08X}")]
    MissingPosition { actor: ObjectId, address: u32 },
    #[error("actor {actor:?} has no +234 cache at probe {ordinal} (00454ED0)")]
    MissingCache { actor: ObjectId, ordinal: u8 },
    #[error("obstacle probe {ordinal} failed: {error}")]
    Host { ordinal: u8, error: E },
}

fn forward_offset(x: f32, z: f32, reverse: bool) -> [u16; 2] {
    let x = ftol(f64::from(x) * 4096.0);
    let z = ftol(f64::from(z) * 4096.0);
    // Native IMUL/ADD keeps only the DWORD and FILD reads it with a zero
    // high DWORD. The length is unsigned even when its low DWORD is negative.
    let squared = x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)) as u32;
    let length = f64::from(squared).sqrt();
    let (x, z) = if length == 0.0 {
        (1.0, 0.0_f32)
    } else {
        // The denominator and normalized Z spill to f32; normalized X stays
        // on x87 until multiplication. Both numerators use signed low WORDs.
        let denominator = f64::from(length as f32);
        (
            f64::from(x as i16) / denominator,
            (f64::from(z as i16) / denominator) as f32,
        )
    };
    let distance = if reverse { -224.0 } else { 224.0 };
    [
        ftol(x * distance) as u16,
        ftol(f64::from(z) * distance) as u16,
    ]
}

fn add(a: [u16; 2], b: [u16; 2]) -> [u16; 2] {
    std::array::from_fn(|axis| a[axis].wrapping_add(b[axis]))
}

fn subtract(a: [u16; 2], b: [u16; 2]) -> [u16; 2] {
    std::array::from_fn(|axis| a[axis].wrapping_sub(b[axis]))
}

impl ObjectArena {
    /// Owns the complete shared 00429FF0 scan, without selecting a scene or
    /// inventing terrain/collision results. Errors retain all reached writes.
    pub fn scan_family_obstacles<H: FamilyObstacleProbeHost>(
        &mut self,
        actor: ObjectId,
        reverse: bool,
        host: &mut H,
    ) -> Result<(), FamilyObstacleScanError<H::Error>> {
        self.family429e20_actor(actor)?;
        self.family428650_actor_mut(actor)?.word_22e &= 0x1f;
        let basis = self.actor(actor)?.rotating.basis;
        let forward = forward_offset(
            f32::from_bits(basis.words[7]),
            f32::from_bits(basis.words[9]),
            reverse,
        );
        let perpendicular = [forward[1], forward[0].wrapping_neg()];
        let position = self.object(actor)?.base.position_words.ok_or(
            FamilyObstacleScanError::MissingPosition {
                actor,
                address: 0x0042_a0bc,
            },
        )?;
        let center = [position[0], position[2]];
        let ahead = add(center, forward);
        let probes = [
            (center, 0x0040),
            (add(center, perpendicular), 0x0100),
            (subtract(center, perpendicular), 0x0080),
            (ahead, 0x0200),
            (add(ahead, perpendicular), 0x0800),
            (subtract(ahead, perpendicular), 0x0400),
        ];
        let mut ordinal = 0;
        for (point, flag) in probes {
            self.family_obstacle_probe(actor, point, flag, ordinal, host)?;
            ordinal += 1;
        }
        let near = self.family428650_actor(actor)?.word_22e & 0xfc0;
        if near == 0 {
            return Ok(());
        }
        if near == 0xfc0 {
            self.family_obstacle_probe(actor, add(ahead, forward), 0x1000, ordinal, host)?;
            ordinal += 1;
        }
        let flags = self.family428650_actor(actor)?.word_22e;
        if flags & 0x40 != 0 || flags & 0x1c0 == 0x1c0 || flags & 0xe00 == 0xe00 {
            // Unlike the near row, the native rear row reads current XYZ after
            // all callbacks. A probe may have moved the actor in the meantime.
            let position = self.object(actor)?.base.position_words.ok_or(
                FamilyObstacleScanError::MissingPosition {
                    actor,
                    address: 0x0042_a238,
                },
            )?;
            let behind = subtract([position[0], position[2]], forward);
            let half_perpendicular = perpendicular.map(|word| ((word as i16) >> 1) as u16);
            self.family_obstacle_probe(
                actor,
                add(behind, half_perpendicular),
                0x4000,
                ordinal,
                host,
            )?;
            self.family_obstacle_probe(
                actor,
                subtract(behind, half_perpendicular),
                0x2000,
                ordinal + 1,
                host,
            )?;
        }
        Ok(())
    }

    fn family_obstacle_probe<H: FamilyObstacleProbeHost>(
        &mut self,
        actor: ObjectId,
        point: [u16; 2],
        flag: u16,
        ordinal: u8,
        host: &mut H,
    ) -> Result<(), FamilyObstacleScanError<H::Error>> {
        // No native null guard exists: 00454ED0 immediately reads [ECX+4].
        // Report this feature boundary instead of bypassing the cache.
        if self.family429e20_actor(actor)?.probe_234.is_none() {
            return Err(FamilyObstacleScanError::MissingCache { actor, ordinal });
        }
        let blocked = host
            .probe(self, actor, point)
            .map_err(|error| FamilyObstacleScanError::Host { ordinal, error })?;
        if blocked != 0 {
            self.family428650_actor_mut(actor)?.word_22e |= flag;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
