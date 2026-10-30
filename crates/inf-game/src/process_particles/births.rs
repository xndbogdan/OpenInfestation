//! Native 0048A3A0/0048A0F0 and 0048A720/0048A8B0 producers, plus the
//! impact sparks 0048AB10/0048ABD0. All six authored type selectors use
//! executable tables; no mission IDs.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::factories::ParticleFactoryProcessState;
use super::{
    audio::ParticleSoundRequest, ParticleAllocationRequest, ParticleConstruction, ParticleError,
    ParticleId, ProcessParticleRuntime,
};
use crate::name_entry::name_entry_rng_next;

/// Current process/world operands. Allocation reads the real phase/network
/// gate. The implicit +14 source is read from the actual process owner only
/// after allocation; current audio/reclaim services cannot mutate this borrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBirthInputs<'a> {
    pub network_active: bool,
    pub phase: Option<u32>,
    pub process: &'a ParticleFactoryProcessState,
}

/// Distinct native read sites reached by the implicit-source producers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleBirthProducer {
    Central,
    Debris,
    Trail,
    Trail12,
    Trail13,
    Splash,
    Impact,
    GroundImpact,
    FallingDrop,
    MaterialDust,
    PickupFlash,
    ShortSmoke,
    TerrainFlash,
    ImpactSpark,
}

impl ParticleBirthProducer {
    pub const fn source_read_address(self) -> u32 {
        match self {
            Self::Central => 0x0048_a215,
            Self::Debris => 0x0048_a435,
            Self::Trail => 0x0048_9eb2,
            Self::Trail12 => 0x0048_9f3a,
            Self::Trail13 => 0x0048_9fbe,
            Self::Splash => 0x0048_9409,
            Self::Impact => 0x0048_9657,
            Self::GroundImpact => 0x0048_9521,
            Self::FallingDrop => 0x0048_99cc,
            Self::MaterialDust => 0x0048_9cfd,
            Self::PickupFlash => 0x0048_9b0c,
            Self::ShortSmoke => 0x0048_9c29,
            Self::TerrainFlash => 0x0048_add9,
            Self::ImpactSpark => 0x0048_ac59,
        }
    }
}

/// A portable required-input stop at the native DWORD read, not a claim that
/// the original mapped process global faults or has a fallback value.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("particle {producer:?} source read {address:08X} requires live process-null handle", address = .producer.source_read_address())]
pub struct ParticleBirthSourceError {
    pub producer: ParticleBirthProducer,
}

/// Shared native producer argument shape; type selection is independent of
/// the world, phase and campaign. Nullable velocity preserves each consumer's
/// distinct policy (central copies zeroes; debris delegates null to base).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBirthRequest<'a> {
    pub particle_type: u32,
    pub position: [u16; 3],
    pub velocity: Option<[u16; 3]>,
    pub flags: u32,
    pub inputs: ParticleBirthInputs<'a>,
}

/// 0048AB10's operands from the actor-pair kernel 0047B610: the contact
/// midpoint and the impact strength (the spark count).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleImpactSparksRequest<'a> {
    pub position: [u16; 3],
    pub strength: i32,
    pub inputs: ParticleBirthInputs<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBurstRequest<'a> {
    pub burst: ParticleBurst,
    pub load_factor: u32,
    pub inputs: ParticleBirthInputs<'a>,
}

impl ParticleBirthInputs<'_> {
    fn allocation(self, bytes: u32) -> ParticleAllocationRequest {
        ParticleAllocationRequest {
            payload_bytes: bytes,
            last_reclaim_category: 1,
            network_active: self.network_active,
            phase: self.phase,
        }
    }

    pub(super) fn source_handle(
        self,
        producer: ParticleBirthProducer,
    ) -> Result<u32, ParticleBirthSourceError> {
        self.process
            .process_null_handle
            .map(|handle| handle.raw())
            .ok_or(ParticleBirthSourceError { producer })
    }
}

/// Required 00487F10 -> 0047FF60 positional enqueue for central type 1/2,
/// and the 00487EC0 one for the impact sparks. 004923C0 dispatches
/// destruction of a nullable returned error; it does not play a sound. A
/// host must retain the copied request for 00480110 service.
pub trait ParticleBirthAudio {
    type Error;
    fn explosion_sound(
        &mut self,
        request: ParticleSoundRequest,
        particles: &mut ProcessParticleRuntime,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum ParticleBirthError<E> {
    #[error(transparent)]
    Source(#[from] ParticleBirthSourceError),
    #[error(transparent)]
    Particle(#[from] ParticleError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("particle birth audio service failed")]
    Audio(E),
}

/// Consumed fields of the 32-byte 0048A8B0 record. Its +0A WORD is omitted:
/// neither reached producer reads it, so callers need not invent its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBurst {
    pub debris_count: u16,
    pub position: [u16; 3],
    pub placement_radius: u16,
    pub central_type: u32,
    pub debris_types: [u32; 2],
    /// Only the low WORD is consumed by the velocity IMUL instructions.
    pub speed: u32,
    pub flags: u32,
}

impl ProcessParticleRuntime {
    /// 0048A3A0. Native type bounds return null before allocation; scratch
    /// remains a nonnull result and executes all three reached RNG draws.
    pub fn birth_debris(
        &mut self,
        request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<Option<ParticleId>, ParticleBirthError<std::convert::Infallible>> {
        let ParticleBirthRequest {
            particle_type,
            position,
            velocity,
            flags,
            inputs,
        } = request;
        let (table, counter, fade) = match particle_type {
            0 => (0x0053_d328, 2, 0_u16),
            1 => (0x0053_d338, 2, 0),
            2 => (0x0053_d338, 3, 0),
            3 => (0x0053_d348, 2, 0),
            4 => (0x0053_d338, 6, 0),
            5 => (0x0053_d338, 12, 150),
            _ => return Ok(None),
        };
        let id = self.allocate(inputs.allocation(0x3c))?;
        let source_handle = inputs.source_handle(ParticleBirthProducer::Debris)?;
        self.construct(
            id,
            ParticleConstruction {
                category: 1,
                position: position.into(),
                velocity: velocity.map(Into::into),
                source_handle,
                flags,
            },
        )?;
        let record = self.record_mut(id)?;
        record.write_dword(0x28, table)?;
        record.write_dword(0, 0x0052_6f40)?;
        let lifetime = table_lifetime(executable, table)?.wrapping_sub(1);
        record.write_word(0x10, lifetime)?;
        record.write_dword(0, 0x0052_87d4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_dword(0x30, 256)?;
        record.write_dword(0x2c, (random >> 9) + 192)?;
        record.write_dword(0, 0x0052_87f4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_word(
            0x10,
            lifetime.wrapping_add(((random >> 1) * u32::from(lifetime >> 1) >> 15) as u16),
        )?;
        record.write_dword(0x34, counter)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_word(0x3a, fade)?;
        record.write_word(
            0x38,
            ((random as i32).wrapping_mul(i32::from(fade as i16)) >> 16) as u16,
        )?;
        record.write_dword(0, 0x0052_8934)?;
        Ok(Some(id))
    }

    /// 0048A0F0. Position is copied before audio; each birth uses the prior
    /// iteration's jittered copy, and each type-1/2 jitter reads original XYZ.
    pub fn birth_central<A: ParticleBirthAudio>(
        &mut self,
        request: ParticleBirthRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<Option<ParticleId>, ParticleBirthError<A::Error>> {
        let ParticleBirthRequest {
            particle_type,
            position,
            velocity,
            flags,
            inputs,
        } = request;
        let (table, scale, count) = match particle_type {
            0 => (0x0053_d4e0, 0xc00_i32, 1_i32),
            1 => (0x0053_d4f0, 0x600, 8),
            2 => (0x0053_d270, 0x600, 8),
            3 => (0x0053_d5e0, 0x400, 1),
            4 => (0x0053_d5e0, 0x200, 1),
            5 => (0x0053_d2b0, 0x200, 1),
            _ => return Ok(None),
        };
        // The original null pointer explicitly constructs zero velocity.
        let velocity = velocity.unwrap_or([0; 3]);
        if matches!(particle_type, 1 | 2) {
            audio
                .explosion_sound(
                    ParticleSoundRequest {
                        sound: 6,
                        position,
                        volume: 65536,
                        pitch: 65536,
                        flags: 0,
                    },
                    self,
                    rng,
                )
                .map_err(ParticleBirthError::Audio)?;
        }
        let mut current_position = position;
        let mut final_id = None;
        for index in 0..count {
            let id = self.allocate(inputs.allocation(0x34))?;
            let source_handle = inputs.source_handle(ParticleBirthProducer::Central)?;
            self.construct(
                id,
                ParticleConstruction {
                    category: 1,
                    position: current_position.into(),
                    velocity: Some(velocity.into()),
                    source_handle,
                    flags,
                },
            )?;
            let record = self.record_mut(id)?;
            record.write_dword(0, 0x0052_6f40)?;
            record.write_dword(0x28, table)?;
            let lifetime = table_lifetime(executable, table)?.wrapping_sub(1);
            record.write_word(0x10, lifetime)?;
            record.write_dword(0, 0x0052_87d4)?;
            let random = name_entry_rng_next(rng) & 0xffff;
            record.write_dword(0x30, 0)?;
            record.write_dword(0, 0x0052_8914)?;
            record.write_dword(0x2c, scale.wrapping_add((random >> 9) as i32 - 64) as u32)?;
            if matches!(particle_type, 1 | 2) {
                let random = name_entry_rng_next(rng) & 0xffff;
                // Native IMUL wraps before signed IDIV and arithmetic SAR.
                let product = random
                    .wrapping_mul(u32::from(lifetime))
                    .wrapping_mul((count - index) as u32) as i32;
                let decrease = (product / count) >> 17;
                record.write_word(0x10, lifetime.wrapping_sub(decrease as u16))?;
                let radius = index.wrapping_mul(300) / count + 1;
                current_position = position;
                for word in &mut current_position {
                    let random = name_entry_rng_next(rng) & 0xffff;
                    let offset = (random as i32) % (radius * 2) - radius;
                    *word = word.wrapping_add(offset as u16);
                }
            }
            final_id = Some(id);
        }
        Ok(final_id)
    }

    /// 0048A720. The vector cursor and all successful prior births remain
    /// reached on failure. Invalid debris type ends its loop on native null.
    pub fn birth_burst_debris(
        &mut self,
        request: ParticleBurstRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleBirthError<std::convert::Infallible>> {
        let ParticleBurstRequest {
            burst,
            load_factor,
            inputs,
        } = request;
        let requested = u32::from(burst.debris_count);
        let scaled = (requested.wrapping_mul(load_factor) as i32) >> 16;
        let count = if scaled == 0 {
            requested as i32
        } else {
            scaled
        };
        let radius = i32::from(burst.placement_radius.max(16));
        for index in 0..count.max(0) {
            self.vector_cursor = self.vector_cursor.wrapping_add(1);
            let address = 0x0053_9960 + (self.vector_cursor % 100) * 6;
            let bytes = executable.mapped(address, 6)?.bytes;
            let direction: [u16; 3] = std::array::from_fn(|axis| {
                u16::from_le_bytes([bytes[axis * 2], bytes[axis * 2 + 1]])
            });
            let speed = burst.speed as u16;
            let mut velocity =
                direction.map(|word| ((speed.wrapping_mul(word) as i16) >> 4) as u16);
            if (velocity[1] as i16) < 0 {
                velocity[1] = velocity[1].wrapping_neg();
            }
            let length = direction
                .iter()
                .map(|word| i32::from(*word as i16).abs())
                .sum::<i32>() as i16;
            let mut offset = direction;
            if length != 0 {
                offset = direction.map(|word| {
                    (i32::from(word as i16).wrapping_mul(radius) / i32::from(length)) as u16
                });
            }
            let position =
                std::array::from_fn(|axis| burst.position[axis].wrapping_add(offset[axis]));
            if self
                .birth_debris(
                    ParticleBirthRequest {
                        particle_type: burst.debris_types[(index & 1) as usize],
                        position,
                        velocity: Some(velocity),
                        flags: burst.flags,
                        inputs,
                    },
                    executable,
                    rng,
                )?
                .is_none()
            {
                break;
            }
        }
        Ok(())
    }

    /// Full 0048A8B0 admission. The central burst follows debris even when
    /// an invalid debris type returned null. Missing real audio is an error.
    pub fn birth_burst<A: ParticleBirthAudio>(
        &mut self,
        request: ParticleBurstRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<(), ParticleBirthError<A::Error>> {
        self.birth_burst_debris(request, executable, rng)
            .map_err(|error| match error {
                ParticleBirthError::Source(error) => ParticleBirthError::Source(error),
                ParticleBirthError::Particle(error) => ParticleBirthError::Particle(error),
                ParticleBirthError::Executable(error) => ParticleBirthError::Executable(error),
                ParticleBirthError::Audio(never) => match never {},
            })?;
        self.birth_central(
            ParticleBirthRequest {
                particle_type: request.burst.central_type,
                position: request.burst.position,
                velocity: None,
                flags: request.burst.flags,
                inputs: request.inputs,
            },
            executable,
            rng,
            audio,
        )?;
        Ok(())
    }

    /// 0048AB10 with ECX 0, as its only caller passes. Sound 0x47 is
    /// enqueued at the midpoint through 00487EC0 before any spark. Each
    /// spark is the midpoint plus (rng & 0x1F) - 0x10 per axis; three more
    /// draws build a velocity that 0048ABD0 never reads.
    pub fn birth_impact_sparks<A: ParticleBirthAudio>(
        &mut self,
        request: ParticleImpactSparksRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<(), ParticleBirthError<A::Error>> {
        let ParticleImpactSparksRequest {
            position,
            strength,
            inputs,
        } = request;
        audio
            .explosion_sound(
                ParticleSoundRequest {
                    sound: 0x47,
                    position,
                    volume: 0x8000,
                    pitch: 0x10000,
                    flags: 0,
                },
                self,
                rng,
            )
            .map_err(ParticleBirthError::Audio)?;
        // 0048AB3A: JLE, so a zero or negative strength births nothing.
        for _ in 0..strength.max(0) {
            let mut spark = position;
            for word in &mut spark {
                let random = name_entry_rng_next(rng) & 0x1f;
                *word = word.wrapping_add(random as u16).wrapping_sub(0x10);
            }
            for _ in 0..3 {
                name_entry_rng_next(rng);
            }
            self.birth_impact_spark(spark, 0, inputs, executable, rng)?;
        }
        Ok(())
    }

    /// 0048ABD0 type 0; its nonzero types return NULL before any draw and
    /// are never passed. The jitter drawn before allocation is the velocity.
    /// Table, lifetime and scale match central type 5; the final vtable
    /// 00528954 differs from 00528914 only in its update 0048ACE0.
    fn birth_impact_spark<E>(
        &mut self,
        position: [u16; 3],
        flags: u32,
        inputs: ParticleBirthInputs<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<ParticleId, ParticleBirthError<E>> {
        let mut velocity = [0_u16; 3];
        for word in &mut velocity {
            *word = ((name_entry_rng_next(rng) & 0x3f) as u16).wrapping_sub(0x20);
        }
        let id = self.allocate(inputs.allocation(0x34))?;
        let source_handle = inputs.source_handle(ParticleBirthProducer::ImpactSpark)?;
        self.construct(
            id,
            ParticleConstruction {
                category: 1,
                position: position.into(),
                velocity: Some(velocity.into()),
                source_handle,
                flags,
            },
        )?;
        let table = 0x0053_d2b0;
        let record = self.record_mut(id)?;
        record.write_dword(0x28, table)?;
        record.write_dword(0, 0x0052_6f40)?;
        let lifetime = table_lifetime(executable, table)?.wrapping_sub(1);
        record.write_word(0x10, lifetime)?;
        record.write_dword(0, 0x0052_87d4)?;
        let random = name_entry_rng_next(rng) & 0xffff;
        record.write_dword(0x30, 0)?;
        record.write_dword(0, 0x0052_8954)?;
        record.write_dword(0x2c, (random >> 9) + 0x1c0)?;
        Ok(id)
    }
}

#[cfg(test)]
mod null_source_native_receipts;
#[cfg(test)]
mod source_tests;

fn table_lifetime(
    executable: FrontendExecutable<'_>,
    table: u32,
) -> Result<u16, FrontendExecutableError> {
    let bytes = executable.mapped(table + 0x0c, 2)?.bytes;
    Ok(u16::from_le_bytes(bytes.try_into().expect("mapped WORD")))
}
