//! Invisible drop528834 contact488DC0 and its ground489160/water48A8F0 effects.
use super::collision::ParticleTerrainContact;
use super::*;
use crate::process_particles::births::ParticleBirthRequest;
use inf_formats::native_basis::sine_q15;

impl ProcessParticleRuntime {
    pub(super) fn falling_drop_contact<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        contact: ParticleTerrainContact,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<bool, ParticleUpdateError<A::Error>> {
        if contact.selector == 6 {
            self.birth_drop_water(id, request, executable, rng, audio)?;
        } else {
            let speed = (i32::from(self.record(id)?.read_word(0x0c)? as i16) / 2).abs();
            self.birth_drop_dust(id, speed, contact.height, request, executable, rng)?;
        }
        Ok(true)
    }
    ///489160 retains the terrain owner but rereads the source position for
    ///every emission, including a scratch source overwritten by a child birth.
    fn birth_drop_dust<E>(
        &mut self,
        id: ParticleId,
        speed: i32,
        height: i32,
        request: &ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<E>> {
        let terrain = request
            .terrain
            .as_ref()
            .ok_or(ParticleUpdateError::MissingTerrain { id })?;
        let plane = (terrain.terrain.plane_base() >> (terrain.terrain.plane_shift() & 31)) as i16;
        if (self.record(id)?.read_word(6)? as i16) <= plane {
            return Ok(());
        }
        let factor = request.load_factor;
        let masked = factor & !0xfff;
        let count = if masked > 0x8000 {
            8
        } else if masked < 0x1000 {
            1
        } else {
            factor >> 12
        };
        for _ in 0..count.max(0) {
            let angle = (name_entry_rng_next(rng) & 0xffff) as i32;
            let radius = (((name_entry_rng_next(rng) & 0xffff) << 6) >> 16).max(1) as i32;
            let dx = (sine_q15(angle.wrapping_add(0x4000), terrain.sine_quarter)
                .wrapping_mul(radius)
                >> 15) as u16;
            let dz = (sine_q15(angle, terrain.sine_quarter).wrapping_mul(radius) >> 15) as u16;
            let source = self.record(id)?.position()?;
            let xz = [source[0].wrapping_add(dx), source[2].wrapping_add(dz)];
            let y = i32::from(terrain.terrain.height_at(xz));
            let material = terrain
                .terrain
                .cell_at([xz[0].wrapping_add(64), xz[1].wrapping_add(64)])
                .material();
            let selector = terrain.impact_selectors[usize::from(material)];
            let velocity = [
                (i32::from(dx as i16).wrapping_mul(speed) / radius) as u16,
                (y.wrapping_sub(height).wrapping_mul(speed) / radius) as u16,
                (i32::from(dz as i16).wrapping_mul(speed) / radius) as u16,
            ];
            self.birth_material_dust(
                ParticleBirthRequest {
                    particle_type: selector,
                    position: [xz[0], y as u16, xz[1]],
                    velocity: Some(velocity),
                    flags: 0,
                    inputs: request.birth_inputs.borrowed(),
                },
                executable,
                rng,
            )?;
        }
        Ok(())
    }
    fn birth_drop_water<A: ParticleUpdateHost>(
        &mut self,
        id: ParticleId,
        request: &ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
        audio: &mut A,
    ) -> Result<(), ParticleUpdateError<A::Error>> {
        for _ in 0..(request.load_factor >> 12).max(0) {
            let x_random = name_entry_rng_next(rng);
            let x = self
                .record(id)?
                .read_word(4)?
                .wrapping_add(((x_random >> 8) & 255) as u16)
                .wrapping_sub(128);
            let z_random = name_entry_rng_next(rng);
            let z = self
                .record(id)?
                .read_word(8)?
                .wrapping_add(((z_random >> 8) & 255) as u16)
                .wrapping_sub(128);
            let terrain = request
                .terrain
                .as_ref()
                .ok_or(ParticleUpdateError::MissingTerrain { id })?;
            let ground = i32::from(terrain.terrain.height_at([x, z]));
            let plane = terrain.terrain.plane_base() >> (terrain.terrain.plane_shift() & 31);
            //48A8F0 always computes this wave below plane, independently of
            //the world flag gate belonging to the separate410F60 consumer.
            let wave = if ground < plane {
                let x = u32::from(x);
                let z = u32::from(z);
                let clock = terrain.wave_clock;
                let a = sine_q15(
                    clock
                        .wrapping_mul(64)
                        .wrapping_add(x.wrapping_mul(12))
                        .wrapping_mul(2) as i32,
                    terrain.sine_quarter,
                );
                let b = sine_q15(
                    x.wrapping_add(z)
                        .wrapping_mul(32)
                        .wrapping_add(clock.wrapping_mul(320)) as i32,
                    terrain.sine_quarter,
                );
                let c = sine_q15(
                    clock
                        .wrapping_mul(64)
                        .wrapping_add(z.wrapping_sub(x).wrapping_mul(6))
                        .wrapping_mul(8) as i32,
                    terrain.sine_quarter,
                );
                let amplitude = ((a.wrapping_add(b) >> 5).wrapping_add(c >> 6)) as i16;
                let offset = (i32::from(amplitude)
                    .wrapping_mul(plane.wrapping_sub(ground).wrapping_add(512))
                    >> 15) as i16;
                plane.wrapping_add(i32::from(offset)).max(ground)
            } else {
                plane
            } as i16;
            //The second terrain query is a separate native read.
            if terrain.terrain.height_at([x, z]) > wave {
                continue;
            }
            let particle = self.birth_splash(
                ParticleBirthRequest {
                    particle_type: 1,
                    position: [x, wave as u16, z],
                    velocity: None,
                    flags: 0,
                    inputs: request.birth_inputs.borrowed(),
                },
                executable,
                rng,
            )?;
            if name_entry_rng_next(rng) & 31 == 0 {
                let [first, second] = crate::process_sound::SPLASH_SOUNDS.map(|slot| slot as u32);
                let sound = if name_entry_rng_next(rng) & 0x1000 != 0 {
                    first
                } else {
                    second
                };
                let pitch = ((name_entry_rng_next(rng) & 0xffff) >> 2) + 0xc000;
                let child = particle
                    .expect("4893A0 selector1 always constructs in the native pool/scratch");
                let position = self.record(child)?.position()?;
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
        }
        Ok(())
    }
}
