//! 00488CC0 animation radius, 00465110 terrain/water classification (through
//! 00465550) and
//! 0048A6B0 debris response. This class does not query actors or furniture.

use inf_formats::FrontendExecutable;

use super::{
    ParticleSoundRequest, ParticleTerrainContext, ParticleUpdateError, ParticleUpdateHost,
    ParticleUpdateRequest,
};
use crate::{
    name_entry::name_entry_rng_next,
    process_particles::{births::ParticleBirthRequest, ParticleId, ProcessParticleRuntime},
    terrain_runtime::TerrainWaterSurfaceRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParticleTerrainContact {
    pub selector: u32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParticleWaterClassification {
    pub previous: u16,
    pub current: u16,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParticleWaterMeasurement {
    pub classification: i32,
    pub height: i32,
}

impl ProcessParticleRuntime {
    pub(super) fn collide_particle<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        movement: [u16; 3],
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<bool, ParticleUpdateError<A::Error>> {
        let vtable = self.record(id)?.read_dword(0)?;
        match vtable {
            // Original +0C=00402720: XOR AL,AL; RET4.
            0x0052_6f80 | 0x0052_87b4 | 0x0052_8814 | 0x0052_88b4 | 0x0052_88d4 | 0x0052_88f4
            | 0x0052_8914 | 0x0052_8954 | 0x0052_8854 | 0x0052_8874 | 0x0052_8894 | 0x0052_8974 => {
                Ok(false)
            }
            // 00465FA0 (+0C) is shared by the projectile and the factory
            // classes 00526F20/00526F60; only their +10 contacts differ.
            0x0052_6efc | 0x0052_6f20 | 0x0052_6f60 => {
                let radius = self.animation_radius(id, executable)?;
                audio.collide_projectile(self, id, movement, radius, request, executable, rng)
            }
            0x0052_8934 => {
                let radius = self.animation_radius(id, executable)?;
                self.collide_terrain(id, radius, request, executable, rng, audio)
            }
            0x0052_87f4 => {
                let radius = self.animation_radius(id, executable)?;
                let Some(contact) = self.classify_terrain_contact(id, radius, request)? else {
                    return Ok(false);
                };
                self.material_sprite_contact(id, contact, radius, request, executable, rng, audio)
            }
            0x0052_8834 => {
                let radius = self.animation_radius(id, executable)?;
                let Some(contact) = self.classify_terrain_contact(id, radius, request)? else {
                    return Ok(false);
                };
                self.falling_drop_contact(id, contact, request, executable, rng, audio)
            }
            _ => Err(ParticleUpdateError::Collision { id, vtable }),
        }
    }

    fn animation_radius<E>(
        &self,
        id: ParticleId,
        executable: FrontendExecutable<'_>,
    ) -> Result<i32, ParticleUpdateError<E>> {
        let record = self.record(id)?;
        let table = record.read_dword(0x28)?;
        let bytes = executable.mapped(table.wrapping_add(10), 2)?.bytes;
        let size = u16::from_le_bytes(bytes.try_into().expect("WORD"));
        let frame = super::super::draw::animation::frame_address(
            executable,
            table,
            record.read_word(0x10)?,
        )
        .map_err(|error| match error {
            super::super::draw::animation::ParticleAnimationError::Executable(error) => {
                ParticleUpdateError::Executable(error)
            }
            super::super::draw::animation::ParticleAnimationError::ZeroFrames { table } => {
                ParticleUpdateError::ZeroAnimationFrames { table }
            }
        })?;
        let bytes = executable.mapped(frame.wrapping_add(8), 2)?.bytes;
        let frame_size = i32::from(i16::from_le_bytes(bytes.try_into().expect("WORD")));
        Ok(frame_size
            .wrapping_mul(i32::from(size))
            .wrapping_mul(record.read_dword(0x2c)? as i32)
            >> 17)
    }

    fn cached_floor<E>(
        &mut self,
        id: ParticleId,
        terrain: &ParticleTerrainContext<'_>,
    ) -> Result<i16, ParticleUpdateError<E>> {
        Ok(self.cached_terrain_floor(id, terrain.terrain)?)
    }

    fn collide_terrain<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        radius: i32,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<bool, ParticleUpdateError<A::Error>> {
        let Some(contact) = self.classify_terrain_contact(id, radius, request)? else {
            return Ok(false);
        };
        self.debris_contact(
            id,
            contact.selector,
            contact.height,
            request,
            executable,
            rng,
            audio,
        )
    }

    /// 00465110 classification and reached floor cache publication. The
    /// caller owns its real class-specific virtual+10 response.
    pub(crate) fn classify_terrain_contact<E>(
        &mut self,
        id: ParticleId,
        radius: i32,
        request: &ParticleUpdateRequest<'_>,
    ) -> Result<Option<ParticleTerrainContact>, ParticleUpdateError<E>> {
        let terrain = request
            .terrain
            .as_ref()
            .ok_or(ParticleUpdateError::MissingTerrain { id })?;
        let water = self.classify_particle_water(id, radius, terrain)?;
        let [x, y, z] = self.record(id)?.position()?;
        let y = i32::from(y as i16);
        let top = y.wrapping_add(radius);
        let bottom = y.wrapping_sub(radius);
        let coarse = i32::from(terrain.terrain.raw_height_at([x, z]));
        let ground_contact = if coarse > top.wrapping_add(384) {
            true
        } else if bottom.wrapping_sub(384) <= coarse {
            bottom <= i32::from(terrain.terrain.height_at([x, z]))
        } else {
            false
        };
        if ground_contact {
            let height = self.cached_floor(id, terrain)?;
            // 00465070: the H12 cell's material at (x + 64, z + 64) picks
            // H15/0's DWORD; water contact takes its +10 (004650E0).
            let material = terrain
                .terrain
                .cell_at([x.wrapping_add(64), z.wrapping_add(64)])
                .material();
            Ok(Some(ParticleTerrainContact {
                selector: terrain.impact_selectors[usize::from(material)],
                height: i32::from(height),
            }))
        } else if water.previous == 2 && water.current < 2 {
            Ok(Some(ParticleTerrainContact {
                selector: terrain.impact_selectors[4],
                height: water.height,
            }))
        } else {
            Ok(None)
        }
    }

    /// Shared water-volume classification in 00465110 and 004896E0. Radius is
    /// supplied explicitly by each native caller, not inferred from its class.
    pub(crate) fn classify_particle_water<E>(
        &mut self,
        id: ParticleId,
        radius: i32,
        terrain: &ParticleTerrainContext<'_>,
    ) -> Result<ParticleWaterClassification, ParticleUpdateError<E>> {
        let ParticleWaterMeasurement {
            classification,
            height: water_height,
        } = self.measure_particle_water(id, radius, terrain)?;
        let flags = self.record(id)?.read_word(0x12)?;
        let old_class = (flags & 0x0c) >> 2;
        let new_class = (classification * 4 + 4) as u16;
        self.record_mut(id)?
            .write_word(0x12, (flags & !0x0c) | new_class)?;
        Ok(ParticleWaterClassification {
            previous: old_class,
            current: new_class >> 2,
            height: water_height,
        })
    }

    pub(crate) fn measure_particle_water<E>(
        &self,
        id: ParticleId,
        radius: i32,
        terrain: &ParticleTerrainContext<'_>,
    ) -> Result<ParticleWaterMeasurement, ParticleUpdateError<E>> {
        let [x, y, z] = self.record(id)?.position()?;
        let y = i32::from(y as i16);
        let top = y.wrapping_add(radius);
        let bottom = y.wrapping_sub(radius);
        let shift = terrain.terrain.plane_shift();
        let plane = terrain.terrain.plane_base() >> (shift & 31);
        let margin = 512_i32 >> (shift.wrapping_sub(8) & 31);
        let (classification, water_height): (i32, i32) = if top.wrapping_add(margin) < plane {
            (-1, plane)
        } else if bottom.wrapping_sub(margin) > plane {
            (1, plane)
        } else {
            // First 00437AF0 comparison retains the full signed shifted byte;
            // its second read supplies only signed low AX to 00410F60.
            let height_shift = match terrain.terrain.sampling().height_scale {
                crate::terrain_runtime::TerrainHeightScale::Units16 => 4,
                crate::terrain_runtime::TerrainHeightScale::Units32 => 5,
            };
            let coarse =
                i32::from(terrain.terrain.cell_at([x, z]).height_i8()).wrapping_shl(height_shift);
            if coarse >= plane {
                (1, plane)
            } else {
                let water = i32::from(terrain.terrain.water_surface_height(
                    TerrainWaterSurfaceRequest {
                        xz: [x, z],
                        wave_clock: terrain.wave_clock,
                        world_flags: terrain.world_flags,
                        terrain_height: coarse as i16,
                    },
                    terrain.sine_quarter,
                ));
                (
                    if bottom > water {
                        1
                    } else if top >= water {
                        0
                    } else {
                        -1
                    },
                    water,
                )
            }
        };
        Ok(ParticleWaterMeasurement {
            classification,
            height: water_height,
        })
    }

    ///00488D10 is distinct from debris 0048A6B0: ground bounces lose seven
    ///eighths of Y velocity, while water emits a splash and retires the sprite.
    pub(super) fn material_sprite_contact<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        contact: ParticleTerrainContact,
        radius: i32,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<bool, ParticleUpdateError<A::Error>> {
        if contact.selector != 6 {
            self.record_mut(id)?
                .write_word(6, contact.height.wrapping_add(radius) as u16)?;
            let y = self.record(id)?.read_word(0x0c)? as i16;
            let bounce = (-(i32::from(y) / 8)) as i16;
            self.record_mut(id)?.write_word(0x0c, bounce as u16)?;
            let drag = (80 - i32::from(bounce)).max(0).wrapping_shl(7);
            for offset in [0x0a, 0x0e] {
                let old = self.record(id)?.read_word(offset)?;
                let decrease = i32::from(old as i16).wrapping_mul(drag) >> 15;
                self.record_mut(id)?
                    .write_word(offset, old.wrapping_sub(decrease as u16))?;
            }
            return Ok(false);
        }
        if name_entry_rng_next(rng) & 3 == 0 {
            // 004655B0: sound 9 or 10 by the draw's bit 12, then the pitch.
            let sound = if name_entry_rng_next(rng) & 0x1000 != 0 {
                9
            } else {
                10
            };
            let pitch = ((name_entry_rng_next(rng) & 0xffff) >> 2) + 0xc000;
            let position = self.record(id)?.position()?;
            audio
                .impact_sound(
                    ParticleSoundRequest {
                        sound,
                        position,
                        volume: 65536,
                        pitch,
                        flags: 0,
                    },
                    self,
                    rng,
                )
                .map_err(ParticleUpdateError::Audio)?;
        }
        self.record_mut(id)?.write_word(6, contact.height as u16)?;
        let position = self.record(id)?.position()?;
        self.birth_splash(
            ParticleBirthRequest {
                particle_type: 0,
                position,
                velocity: None,
                flags: 0,
                inputs: request.birth_inputs.borrowed(),
            },
            executable,
            rng,
        )?;
        Ok(true)
    }

    /// 0048A6B0 always returns false; selector6 owns sound and splash births.
    fn debris_contact<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        selector: u32,
        height: i32,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<bool, ParticleUpdateError<A::Error>> {
        self.record_mut(id)?.write_word(6, height as u16)?;
        if selector == 6 {
            if name_entry_rng_next(rng) & 3 == 0 {
                let sound = if name_entry_rng_next(rng) & 0x1000 != 0 {
                    9
                } else {
                    10
                };
                let pitch = ((name_entry_rng_next(rng) & 0xffff) >> 2) + 0xc000;
                let position = self.record(id)?.position()?;
                audio
                    .impact_sound(
                        ParticleSoundRequest {
                            sound,
                            position,
                            volume: 65536,
                            pitch,
                            flags: 0,
                        },
                        self,
                        rng,
                    )
                    .map_err(ParticleUpdateError::Audio)?;
            }
            let position = self.record(id)?.position()?;
            self.birth_splash(
                ParticleBirthRequest {
                    particle_type: 0,
                    position,
                    velocity: None,
                    flags: 0,
                    inputs: request.birth_inputs.borrowed(),
                },
                executable,
                rng,
            )?;
        } else {
            let velocity = self.record(id)?.read_word(0x0c)? as i16;
            if velocity < 0 {
                self.record_mut(id)?
                    .write_word(0x0c, (-(i32::from(velocity) * 3) / 4) as u16)?;
            }
        }
        Ok(false)
    }

    /// Trail12=0048AF70 settles Y; trail13=0048AFD0 decelerates XYZ and
    /// clamps Y before deciding whether to move X/Z. Cached height is live.
    pub(super) fn update_ground_trail<E>(
        &mut self,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
    ) -> Result<(), ParticleUpdateError<E>> {
        let terrain = request
            .terrain
            .as_ref()
            .ok_or(ParticleUpdateError::MissingTerrain { id })?;
        let floor = self.cached_floor(id, terrain)?;
        let y = self.record(id)?.read_word(6)? as i16;
        if y <= floor {
            return Ok(());
        }
        if self.record(id)?.read_dword(0)? == 0x0052_88d4 {
            let gravity = self.update_globals.gravity;
            let step = if gravity & !15 < 16 { 1 } else { gravity >> 4 };
            let next = (y as u16).wrapping_sub(step as u16) as i16;
            self.record_mut(id)?.write_word(6, next.max(floor) as u16)?;
        } else {
            let delta = request.delta_microseconds;
            let mut velocities = [0_u16; 3];
            for (axis, velocity) in velocities.iter_mut().enumerate() {
                let offset = 0x0a + axis * 2;
                let previous = self.record(id)?.read_word(offset)?;
                let mut change = i32::from(previous as i16).wrapping_mul(delta) >> 19;
                if change == 0 {
                    change = 1;
                }
                *velocity = previous.wrapping_sub(change as u16);
                self.record_mut(id)?.write_word(offset, *velocity)?;
            }
            let movement = i32::from(velocities[1] as i16).wrapping_mul(delta >> 5) >> 15;
            let next = (y as u16).wrapping_add(movement as u16) as i16;
            self.record_mut(id)?.write_word(6, next.max(floor) as u16)?;
            if next >= floor {
                for axis in [0, 2] {
                    let movement =
                        i32::from(velocities[axis] as i16).wrapping_mul(delta >> 5) >> 15;
                    let position = self.record(id)?.read_word(4 + axis * 2)?;
                    self.record_mut(id)?
                        .write_word(4 + axis * 2, position.wrapping_add(movement as u16))?;
                }
            }
        }
        Ok(())
    }
}
