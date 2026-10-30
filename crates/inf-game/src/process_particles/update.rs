//! 00464B30 physical-cell walk and the native central/debris/trail callbacks.
//! 00464DA0 remains a separate operation: admitted pending particles also tick.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{
    audio::ParticleSoundRequest, births::ParticleBirthInputs, ParticleError, ParticleId,
    ParticleStopPhase, ProcessParticleRuntime,
};
use crate::{name_entry::name_entry_rng_next, terrain_runtime::TerrainRuntime};

mod collision;
mod drops;
mod emitter;
mod producers;
mod projectile;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleUpdateGlobals {
    /// Signed WORD result published as DWORD 0054ECB0.
    pub gravity: i32,
    /// 00539958, independently computed from current delta and load factor.
    pub motion_microseconds: i32,
}

impl Default for ParticleUpdateGlobals {
    fn default() -> Self {
        // Original virtual zero-fill and initialized .data respectively.
        Self {
            gravity: 0,
            motion_microseconds: 10_000,
        }
    }
}

/// Resident H12/H14/H15 operands, independent of a mission or OVL number.
/// Terrain residency replaces the lazy H12 query; no terrain values are guessed.
pub struct ParticleTerrainContext<'a> {
    pub terrain: &'a mut TerrainRuntime,
    pub sine_quarter: &'a [u16; 4096],
    /// Live 005471C0 used only by the reached water-wave query.
    pub wave_clock: u32,
    pub world_flags: u32,
    /// Actual H15/0 DWORDs +00..+10, aliased by 00548394. Four ground
    /// material codes and the fifth water-contact code (not sound IDs).
    pub impact_selectors: [u32; 5],
}

pub struct ParticleUpdateBirthInputs<'a> {
    pub network_active: bool,
    pub phase: Option<u32>,
    pub process: &'a mut super::factories::ParticleFactoryProcessState,
}

impl ParticleUpdateBirthInputs<'_> {
    pub fn borrowed(&self) -> ParticleBirthInputs<'_> {
        ParticleBirthInputs {
            network_active: self.network_active,
            phase: self.phase,
            process: self.process,
        }
    }
}

pub struct ParticleUpdateRequest<'a> {
    pub delta_microseconds: i32,
    /// Signed process 0055F154, including its original division policy.
    pub load_factor: i32,
    /// Current operands for any trail/splash birth reached during callbacks.
    pub birth_inputs: ParticleUpdateBirthInputs<'a>,
    /// Required only by a reached terrain consumer.
    pub terrain: Option<ParticleTerrainContext<'a>>,
}

/// Required positional enqueue 00487EC0 -> 0047FF60. 004923C0 destroys
/// a nullable returned error; actual sound service belongs to 00480110.
/// RNG selection and pitch are owned here; the host retains copied inputs.
pub trait ParticleUpdateHost {
    type Error;
    fn impact_sound(
        &mut self,
        request: ParticleSoundRequest,
        particles: &mut ProcessParticleRuntime,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;

    fn collide_projectile(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        _movement: [u16; 3],
        _radius: i32,
        _request: &mut ParticleUpdateRequest<'_>,
        _executable: FrontendExecutable<'_>,
        _rng: &mut u32,
    ) -> Result<bool, ParticleUpdateError<Self::Error>> {
        Err(ParticleUpdateError::Collision {
            id,
            vtable: particles.record(id)?.read_dword(0)?,
        })
    }

    fn delete_projectile(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        _request: &mut ParticleUpdateRequest<'_>,
        _executable: FrontendExecutable<'_>,
        _rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<Self::Error>> {
        particles.destroy(id).map_err(Into::into)
    }

    /// 004661B0, factory 004665D0's +1C, before the scalar deletion. A host
    /// with birth audio runs [`ProcessParticleRuntime::retire_factory_particle`].
    fn retire_factory_particle(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        _request: &mut ParticleUpdateRequest<'_>,
        _executable: FrontendExecutable<'_>,
        _rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<Self::Error>> {
        Err(ParticleUpdateError::Retirement {
            id,
            vtable: particles.record(id)?.read_dword(0)?,
        })
    }
}

#[derive(Debug, Error)]
pub enum ParticleUpdateError<E> {
    #[error(transparent)]
    Source(#[from] super::births::ParticleBirthSourceError),
    #[error(transparent)]
    Particle(#[from] ParticleError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("particle {id:?} reached unowned update vtable {vtable:08X}")]
    Update { id: ParticleId, vtable: u32 },
    #[error("particle {id:?} reached unowned collision vtable {vtable:08X}")]
    Collision { id: ParticleId, vtable: u32 },
    #[error("particle {id:?} reached unowned retirement vtable {vtable:08X}")]
    Retirement { id: ParticleId, vtable: u32 },
    #[error("particle {id:?} retirement spawns object 0x36 through 00460310")]
    RetirementObject { id: ParticleId },
    #[error("particle {id:?} requires resident H12/H14/H15 terrain operands")]
    MissingTerrain { id: ParticleId },
    #[error("particle animation table {table:08X} has zero frames")]
    ZeroAnimationFrames { table: u32 },
    #[error("particle impact audio service failed")]
    Audio(E),
    #[error("particle projectile service failed")]
    Projectile(E),
    #[error("particle retirement's object factory failed")]
    RetirementFactory(E),
    #[error("particle collision service failed")]
    CollisionService(E),
    #[error("particle {id:?} reached a nonprogressing native projectile subdivision")]
    PortableNonProgressingProjectileSubdivision { id: ParticleId },
}

impl<E> ParticleUpdateError<E> {
    /// Re-types the host service error; runtime variants are unchanged.
    pub fn map_service<F>(self, map: impl FnOnce(E) -> F) -> ParticleUpdateError<F> {
        match self {
            Self::Source(error) => ParticleUpdateError::Source(error),
            Self::Particle(error) => ParticleUpdateError::Particle(error),
            Self::Executable(error) => ParticleUpdateError::Executable(error),
            Self::Update { id, vtable } => ParticleUpdateError::Update { id, vtable },
            Self::Collision { id, vtable } => ParticleUpdateError::Collision { id, vtable },
            Self::Retirement { id, vtable } => ParticleUpdateError::Retirement { id, vtable },
            Self::RetirementObject { id } => ParticleUpdateError::RetirementObject { id },
            Self::MissingTerrain { id } => ParticleUpdateError::MissingTerrain { id },
            Self::ZeroAnimationFrames { table } => {
                ParticleUpdateError::ZeroAnimationFrames { table }
            }
            Self::Audio(error) => ParticleUpdateError::Audio(map(error)),
            Self::Projectile(error) => ParticleUpdateError::Projectile(map(error)),
            Self::RetirementFactory(error) => ParticleUpdateError::RetirementFactory(map(error)),
            Self::CollisionService(error) => ParticleUpdateError::CollisionService(map(error)),
            Self::PortableNonProgressingProjectileSubdivision { id } => {
                ParticleUpdateError::PortableNonProgressingProjectileSubdivision { id }
            }
        }
    }
}

impl<E> From<super::births::ParticleBirthError<std::convert::Infallible>>
    for ParticleUpdateError<E>
{
    fn from(error: super::births::ParticleBirthError<std::convert::Infallible>) -> Self {
        match error {
            super::births::ParticleBirthError::Source(error) => Self::Source(error),
            super::births::ParticleBirthError::Particle(error) => Self::Particle(error),
            super::births::ParticleBirthError::Executable(error) => Self::Executable(error),
            super::births::ParticleBirthError::Audio(error) => match error {},
        }
    }
}

#[derive(Debug)]
pub struct ParticleUpdateOmission<E> {
    pub id: ParticleId,
    pub phase: ParticleStopPhase,
    pub error: ParticleUpdateError<E>,
}

#[derive(Debug)]
pub struct ParticleUpdateReport<E> {
    pub updated: usize,
    pub destroyed: usize,
    pub stopped: usize,
    pub omissions: Vec<ParticleUpdateOmission<E>>,
}

impl<E> Default for ParticleUpdateReport<E> {
    fn default() -> Self {
        Self {
            updated: 0,
            destroyed: 0,
            stopped: 0,
            omissions: Vec::new(),
        }
    }
}

/// Signed high product assembled by SHL/RCL, i.e. low DWORD of SAR 31.
fn product_shift_31(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}

impl ProcessParticleRuntime {
    /// 004661B0, factory 004665D0's +1C, before the scalar deletion:
    /// selector 1 spawns object 0x36 through 00460310 at the particle, a
    /// typed stop here (the world's host builds it); any other selector
    /// births central particle 4 at the particle with no velocity and no
    /// flags (0048A0F0).
    pub fn retire_factory_particle<A: super::births::ParticleBirthAudio>(
        &mut self,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<(), ParticleUpdateError<A::Error>> {
        if self.record(id)?.read_dword(0x3c)? == 1 {
            return Err(ParticleUpdateError::RetirementObject { id });
        }
        let position = self.record(id)?.position()?;
        self.birth_central(
            super::births::ParticleBirthRequest {
                particle_type: 4,
                position,
                velocity: None,
                flags: 0,
                inputs: request.birth_inputs.borrowed(),
            },
            executable,
            rng,
            audio,
        )
        .map(|_| ())
        .map_err(|error| match error {
            super::births::ParticleBirthError::Source(error) => error.into(),
            super::births::ParticleBirthError::Particle(error) => error.into(),
            super::births::ParticleBirthError::Executable(error) => error.into(),
            super::births::ParticleBirthError::Audio(error) => ParticleUpdateError::Audio(error),
        })
    }

    /// 00464B30. Fresh occupancy lookup on each physical slot allows real
    /// births/reclamation inside callbacks to affect later cells in this walk.
    /// On a missing service, reached writes/births/RNG stay committed.
    pub fn tick_particles<A: ParticleUpdateHost>(
        &mut self,
        mut request: ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> ParticleUpdateReport<A::Error> {
        let delta = request.delta_microseconds;
        self.update_globals.gravity = i32::from(product_shift_31(delta, 0x30_0000) as i16);
        self.update_globals.motion_microseconds = if delta < 10_000 {
            10_000
        } else {
            let factor = (0x7fff_i32.wrapping_sub(request.load_factor / 2)).wrapping_shl(16);
            let adjusted =
                product_shift_31(factor, delta.wrapping_sub(10_000)).wrapping_add(10_000);
            if adjusted < 0 {
                delta
            } else {
                adjusted
            }
        };
        let mut report = ParticleUpdateReport::default();
        for slot in 0..super::PARTICLE_POOL_SIZE {
            let Some(id) = self.occupant(slot) else {
                continue;
            };
            if self.stopped_phase(id).is_some() {
                report.stopped += 1;
                continue;
            }
            let mut phase = ParticleStopPhase::Update;
            let result = (|| -> Result<(), ParticleUpdateError<A::Error>> {
                let lifetime = self.record(id)?.read_word(0x10)?;
                let remaining = i32::from(lifetime).wrapping_sub(delta >> 10);
                let flags = self.record(id)?.read_word(0x12)?;
                let mut destroy = true;
                if remaining > 0 && flags & 0x40 == 0 {
                    self.record_mut(id)?.write_word(0x12, flags & !0x80)?;
                    self.record_mut(id)?.write_word(0x10, remaining as u16)?;
                    let previous = self.record(id)?.position()?;
                    if !self.update_particle(id, &mut request, executable, rng, audio)? {
                        let current = self.record(id)?.position()?;
                        let flags = self.record(id)?.read_word(0x12)?;
                        self.record_mut(id)?.write_word(0x12, flags & !0x20)?;
                        let movement =
                            std::array::from_fn(|axis| current[axis].wrapping_sub(previous[axis]));
                        phase = ParticleStopPhase::Collision;
                        destroy = self.collide_particle(
                            id,
                            movement,
                            &mut request,
                            executable,
                            rng,
                            audio,
                        )?;
                    }
                    report.updated += 1;
                }
                if destroy {
                    phase = ParticleStopPhase::Retirement;
                    // Every other admitted class has RET +1C=00401310, and
                    // factory 004665D0's 00526F20 has 004661B0. The scalar
                    // deletion is still required.
                    let vtable = self.record(id)?.read_dword(0)?;
                    if vtable == 0x0052_6efc {
                        audio.delete_projectile(self, id, &mut request, executable, rng)?;
                    } else {
                        if vtable == 0x0052_6f20 {
                            audio.retire_factory_particle(
                                self,
                                id,
                                &mut request,
                                executable,
                                rng,
                            )?;
                        }
                        self.destroy(id)?;
                    }
                    report.destroyed += 1;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.stop_particle(id, phase);
                report
                    .omissions
                    .push(ParticleUpdateOmission { id, phase, error });
            }
        }
        report
    }

    fn update_particle<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        _audio: &mut A,
    ) -> Result<bool, ParticleUpdateError<A::Error>> {
        let delta = request.delta_microseconds;
        let vtable = self.record(id)?.read_dword(0)?;
        match vtable {
            0x0052_6f80 => return self.update_emitter(id, request, executable, rng),
            //0048B0F0: decrement DWORD+34, return its sign. No delta,
            //velocity, animation frame or RNG is read by this leaf.
            0x0052_8874 => {
                let counter = self.record(id)?.read_dword(0x34)?.wrapping_sub(1);
                self.record_mut(id)?.write_dword(0x34, counter)?;
                return Ok((counter as i32) < 0);
            }
            // 00466250 applies extra underwater lifetime decay before the
            // same signed wrapped velocity integration as 00465660.
            0x0052_6efc => self.update_projectile(id, delta)?,
            0x0052_8814 => return self.update_impact_bubble(id, request, executable, rng),
            //0048B0E0 ->00465620: material sprites and factory 004665D0's class
            //always apply gravity before 465660. The prior water flags select
            //full or half gravity.
            0x0052_87f4 | 0x0052_8834 | 0x0052_6f20 => {
                let gravity = if self.record(id)?.read_word(0x12)? & 0x0c != 0 {
                    self.update_globals.gravity
                } else {
                    self.update_globals.gravity >> 1
                };
                let velocity = self.record(id)?.read_word(0x0c)?;
                self.record_mut(id)?
                    .write_word(0x0c, velocity.wrapping_sub(gravity as u16))?;
                self.integrate_velocity(id, delta, false)?;
            }
            // 00466980, factory 00466850's class: selector 8 lengthens +10
            // by delta >> 12 and leaves a type-13 trail at three quarters of
            // the velocity (truncated), both before 00465660.
            0x0052_6f60 => {
                if self.record(id)?.read_dword(0x3c)? == 8 {
                    let lifetime = self.record(id)?.read_word(0x10)?;
                    self.record_mut(id)?
                        .write_word(0x10, lifetime.wrapping_add((delta >> 12) as u16))?;
                    let mut velocity = [0_u16; 3];
                    for (axis, offset) in [0x0a, 0x0c, 0x0e].into_iter().enumerate() {
                        let value = i32::from(self.record(id)?.read_word(offset)? as i16);
                        velocity[axis] = (value.wrapping_mul(3) / 4) as u16;
                    }
                    let position = self.record(id)?.position()?;
                    self.birth_trail(
                        super::births::ParticleBirthRequest {
                            particle_type: 13,
                            position,
                            velocity: Some(velocity),
                            flags: 0,
                            inputs: request.birth_inputs.borrowed(),
                        },
                        executable,
                        rng,
                    )?;
                }
                self.integrate_velocity(id, delta, false)?;
            }
            // 004027B0 is a proven XOR EAX,EAX; RET 4 (splash).
            0x0052_87b4 | 0x0052_8854 | 0x0052_8974 => {}
            // 0048A390 and 0048B110 -> 00465660, no gravity or random
            // acceleration.
            0x0052_8914 | 0x0052_8894 => self.integrate_velocity(id, delta, false)?,
            0x0052_8954 => self.update_impact_spark(id, delta, executable)?,
            // 0048A060 random trail acceleration, including final 3 draws.
            0x0052_88f4 => {
                for (offset, divisor, bias) in [(0x0c, 10, 0), (0x0a, 18, -10), (0x0e, 18, -10)] {
                    let increment = (name_entry_rng_next(rng) & 0xffff) as i32 % divisor + bias;
                    let old = self.record(id)?.read_word(offset)?;
                    self.record_mut(id)?
                        .write_word(offset, old.wrapping_add(increment as u16))?;
                }
                if self.record(id)?.read_word(0x12)? & 0x800 != 0 {
                    let scale = self.record(id)?.read_dword(0x2c)?;
                    let increase = product_shift_31(scale as i32, delta.wrapping_shl(10));
                    self.record_mut(id)?
                        .write_dword(0x2c, scale.wrapping_add(increase as u32))?;
                }
                self.integrate_velocity(id, delta, false)?;
            }
            0x0052_8934 => self.update_debris(id, request, executable, rng)?,
            0x0052_88d4 | 0x0052_88b4 => self.update_ground_trail(id, request)?,
            _ => return Err(ParticleUpdateError::Update { id, vtable }),
        }
        Ok(false)
    }

    /// 00465660's wrapped signed product, axis order and low WORD addition.
    /// The fading debris path sign-extends each low-WORD movement before ABS.
    fn integrate_velocity(
        &mut self,
        id: ParticleId,
        delta: i32,
        fade: bool,
    ) -> Result<(), ParticleError> {
        for axis in 0..3 {
            let velocity = i32::from(self.record(id)?.read_word(0x0a + axis * 2)? as i16);
            let movement = (velocity.wrapping_mul(delta >> 5) >> 15) as u16;
            let position = self.record(id)?.read_word(4 + axis * 2)?;
            self.record_mut(id)?
                .write_word(4 + axis * 2, position.wrapping_add(movement))?;
            if fade {
                let fade = self.record(id)?.read_word(0x38)?;
                let distance = i32::from(movement as i16).unsigned_abs() as u16;
                self.record_mut(id)?
                    .write_word(0x38, fade.wrapping_sub(distance))?;
            }
        }
        Ok(())
    }

    /// 0048ACE0. The lifetime WORD << 7 is an angle. Its folded sine and
    /// cosine, shifted right 10, add to velocity X, Z and Y (sine minus
    /// cosine) as WORDs before 00465660.
    fn update_impact_spark<E>(
        &mut self,
        id: ParticleId,
        delta: i32,
        executable: FrontendExecutable<'_>,
    ) -> Result<(), ParticleUpdateError<E>> {
        let angle = u32::from(self.record(id)?.read_word(0x10)?) << 7;
        let fold = |angle: u32| -> Result<i32, FrontendExecutableError> {
            let mut quarter = angle & 0x3ffc;
            if angle & 0x4000 != 0 {
                quarter = 0x3ffc - quarter;
            }
            let bytes = executable
                .mapped(0x0053_df00 + (quarter >> 2) * 2, 2)?
                .bytes;
            let mut value = i32::from(u16::from_le_bytes(bytes.try_into().expect("WORD")));
            if angle & 0x8000 != 0 {
                value = -value;
            }
            Ok(value >> 10)
        };
        let sine = fold(angle)?;
        let cosine = fold(angle + 0x4000)?;
        for (offset, increment) in [(0x0a, sine), (0x0e, cosine), (0x0c, sine - cosine)] {
            let velocity = self.record(id)?.read_word(offset)?;
            self.record_mut(id)?
                .write_word(offset, velocity.wrapping_add(increment as u16))?;
        }
        self.integrate_velocity(id, delta, false)?;
        Ok(())
    }

    fn update_debris<E>(
        &mut self,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<E>> {
        let fading = self.record(id)?.read_word(0x3a)? != 0;
        if fading {
            let gravity = if self.record(id)?.read_word(0x12)? & 0x0c != 0 {
                self.update_globals.gravity
            } else {
                self.update_globals.gravity >> 1
            };
            let velocity = self.record(id)?.read_word(0x0c)?;
            self.record_mut(id)?
                .write_word(0x0c, velocity.wrapping_sub(gravity as u16))?;
        }
        self.integrate_velocity(id, request.delta_microseconds, fading)?;
        let emit = if fading {
            (self.record(id)?.read_word(0x38)? as i16) < 0
        } else {
            let mut square = 0_i32;
            for offset in [0x0a, 0x0c, 0x0e] {
                let value = i32::from(self.record(id)?.read_word(offset)? as i16);
                square = square.wrapping_add(value.wrapping_mul(value));
            }
            square > 40_000
        };
        if emit {
            let particle_type = self.record(id)?.read_dword(0x34)?;
            let position = self.record(id)?.position()?;
            self.birth_trail(
                super::births::ParticleBirthRequest {
                    particle_type,
                    position,
                    velocity: Some([0, 64, 0]),
                    flags: 0,
                    inputs: request.birth_inputs.borrowed(),
                },
                executable,
                rng,
            )?;
            if fading {
                // Callback can reuse this same physical cell; native rereads
                // both WORDs after the real birth and then consumes one draw.
                let random = (name_entry_rng_next(rng) & 0xffff) as i32;
                let period = i32::from(self.record(id)?.read_word(0x3a)? as i16);
                let fade = self.record(id)?.read_word(0x38)?;
                self.record_mut(id)?.write_word(
                    0x38,
                    fade.wrapping_add((random.wrapping_mul(period) >> 16) as u16),
                )?;
            }
        }
        Ok(())
    }
}
