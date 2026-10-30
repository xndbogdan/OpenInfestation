//! The live splashes, as the Vulkan renderer's water shows them: rings
//! spreading where each one fell, and foam that fades, and the blasts, which
//! throw the water up where they go off in it
//! ([the plan](../../../../docs/INFESTATION-RT-IMPLEMENTATION.md#water)).
//! Read without touching the pool, so the game sees nothing of it.

use inf_formats::FrontendExecutable;

use super::ProcessParticleRuntime;

/// `004893A0`'s splash class, born where debris, a projectile or a falling
/// drop meets the water, and by selector 65 as the vehicle leaves it fast.
pub const SPLASH_VTABLE: u32 = 0x0052_87b4;

/// `0048A0F0`'s class, whose types 1 and 2 are the eight fireballs of an
/// explosion (tables `0053D4F0` and `0053D270`); its other types are single
/// puffs.
pub const CENTRAL_VTABLE: u32 = 0x0052_8914;
const BLAST_TABLES: [u32; 2] = [0x0053_d4f0, 0x0053_d270];
/// That class's type 5 table: the sparks the cannon's shot throws along
/// its aim (selector 32's `00486100`).
const SPARK_TABLE: u32 = 0x0053_d2b0;

/// One fireball of an explosion: where it is (world units, wrapping at 16
/// bits), and whether it is the larger type 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleBlast {
    pub position: [u16; 3],
    pub large: bool,
}

/// `00466710`'s class: the guns' bolts (the plasma's among them), each
/// drawn as a streak along its flight. Their animation frames carry no
/// light (the tables `00539D28` to `00539D78` hold 0 in every frame), so
/// retail's bolts light nothing.
pub const BOLT_VTABLE: u32 = 0x0052_6efc;

/// How young a bolt is fresh, in its life's units (1,024 microseconds,
/// [`ProcessParticleRuntime::splashes`]): two ticks at 60 a second.
pub const FRESH: u16 = 32;

/// A bolt in flight: where it is (world units, wrapping at 16 bits), the
/// sprite its animation shows now, and its scale (1 is the sprite's own
/// size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleBolt {
    pub position: [u16; 3],
    /// Its velocity words (+0A..+0E, signed), which `00465660` moves it by:
    /// `velocity * (delta >> 5) >> 15` a step, about 0.954 units a second
    /// each.
    pub velocity: [i16; 3],
    pub sprite: i16,
    pub scale: f32,
    /// Born within the last [`FRESH`]: where it is, its gun is.
    pub fresh: bool,
}

/// One splash: where it fell (world units, wrapping at 16 bits), how far
/// through its life it is (0 at birth, 1 at its end) and its scale (1 is
/// the sprite's own size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSplash {
    pub position: [u16; 3],
    pub age: f32,
    pub scale: f32,
}

impl ProcessParticleRuntime {
    /// The splashes alive now, those born this frame (still pending) too.
    /// A splash's life counts down from its animation table's WORD `+0C`
    /// less one (`birth_animation`), the manager taking `delta >> 10` a
    /// tick; one that does not read is left out.
    pub fn splashes(&self, executable: FrontendExecutable<'_>) -> Vec<ParticleSplash> {
        self.active
            .iter()
            .chain(&self.pending)
            .flatten()
            .filter(|id| self.stopped_phase(**id).is_none())
            .filter_map(|id| {
                let record = self.record(*id).ok()?;
                if record.read_dword(0).ok()? != SPLASH_VTABLE {
                    return None;
                }
                let table = record.read_dword(0x28).ok()?;
                let bytes = executable.mapped(table.checked_add(0x0c)?, 2).ok()?.bytes;
                let life = u16::from_le_bytes(bytes.try_into().ok()?).wrapping_sub(1);
                let left = record.read_word(0x10).ok()?;
                let age = 1.0 - f32::from(left) / f32::from(life.max(1));
                Some(ParticleSplash {
                    position: record.position().ok()?,
                    age: age.clamp(0.0, 1.0),
                    scale: record.read_dword(0x2c).ok()? as i32 as f32 / 256.0,
                })
            })
            .collect()
    }

    /// The bolts in flight now, those born this frame too.
    pub fn bolts(&self, executable: FrontendExecutable<'_>) -> Vec<ParticleBolt> {
        self.animated(executable, |vtable, _| vtable == BOLT_VTABLE)
    }

    /// The cannon's sparks alive now, those born this frame too.
    pub fn sparks(&self, executable: FrontendExecutable<'_>) -> Vec<ParticleBolt> {
        self.animated(executable, |vtable, table| {
            vtable == CENTRAL_VTABLE && table == SPARK_TABLE
        })
    }

    /// The live particles of a class and table `wanted` takes, each where
    /// it is, with its animation's sprite now and its scale.
    fn animated(
        &self,
        executable: FrontendExecutable<'_>,
        wanted: impl Fn(u32, u32) -> bool,
    ) -> Vec<ParticleBolt> {
        use super::draw::animation::{frame_address, ParticleAnimationFrame};
        self.active
            .iter()
            .chain(&self.pending)
            .flatten()
            .filter(|id| self.stopped_phase(**id).is_none())
            .filter_map(|id| {
                let record = self.record(*id).ok()?;
                let table = record.read_dword(0x28).ok()?;
                if !wanted(record.read_dword(0).ok()?, table) {
                    return None;
                }
                let life = record.read_word(0x10).ok()?;
                // Born with its table's lifetime less one, counting down.
                let born = executable.mapped(table.checked_add(0x0c)?, 2).ok()?.bytes;
                let age = u16::from_le_bytes(born.try_into().ok()?)
                    .wrapping_sub(1)
                    .wrapping_sub(life);
                let frame = ParticleAnimationFrame::read(
                    executable,
                    frame_address(executable, table, life).ok()?,
                )
                .ok()?;
                Some(ParticleBolt {
                    position: record.position().ok()?,
                    velocity: [
                        record.read_word(0x0a).ok()? as i16,
                        record.read_word(0x0c).ok()? as i16,
                        record.read_word(0x0e).ok()? as i16,
                    ],
                    sprite: frame.sprite_slot,
                    scale: record.read_dword(0x2c).ok()? as i32 as f32 / 256.0,
                    fresh: age <= FRESH,
                })
            })
            .collect()
    }

    /// The explosions' fireballs alive now, those born this frame too.
    pub fn blasts(&self) -> Vec<ParticleBlast> {
        self.active
            .iter()
            .chain(&self.pending)
            .flatten()
            .filter(|id| self.stopped_phase(**id).is_none())
            .filter_map(|id| {
                let record = self.record(*id).ok()?;
                if record.read_dword(0).ok()? != CENTRAL_VTABLE {
                    return None;
                }
                let table = record.read_dword(0x28).ok()?;
                let kind = BLAST_TABLES.iter().position(|blast| *blast == table)?;
                Some(ParticleBlast {
                    position: record.position().ok()?,
                    large: kind == 1,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        object_runtime::ActorHandle,
        process_particles::{
            births::{ParticleBirthInputs, ParticleBirthRequest},
            factories::ParticleFactoryProcessState,
        },
    };

    /// A splash just born sits where it fell at the start of its life, and
    /// with half its life gone it is half way through; debris beside it is
    /// no splash.
    #[test]
    fn splashes_tell_where_they_fell_and_how_far_through_their_life() {
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
        )
        .expect("the splash test reads the original executable's tables");
        let executable = FrontendExecutable::parse(&raw).unwrap();
        let process = ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
        };
        let request = ParticleBirthRequest {
            particle_type: 0,
            position: [100, 200, 65000],
            velocity: Some([0; 3]),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &process,
            },
        };
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = 7;
        let splash = particles
            .birth_splash(request, executable, &mut rng)
            .unwrap()
            .unwrap();
        particles
            .birth_debris(request, executable, &mut rng)
            .unwrap()
            .unwrap();
        let seen = particles.splashes(executable);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].position, [100, 200, 65000]);
        assert_eq!(seen[0].age, 0.0);
        assert!((seen[0].scale - 1.0).abs() < 0.26, "{}", seen[0].scale);
        particles.transfer_pending();
        let life = particles.record(splash).unwrap().read_word(0x10).unwrap();
        particles
            .record_mut(splash)
            .unwrap()
            .write_word(0x10, life / 2)
            .unwrap();
        let age = particles.splashes(executable)[0].age;
        assert!((age - 0.5).abs() < 0.02, "{age}");
    }

    struct Quiet;
    impl crate::process_particles::births::ParticleBirthAudio for Quiet {
        type Error = ();
        fn explosion_sound(
            &mut self,
            _: crate::process_particles::audio::ParticleSoundRequest,
            _: &mut ProcessParticleRuntime,
            _: &mut u32,
        ) -> Result<(), ()> {
            Ok(())
        }
    }

    /// An explosion's eight fireballs are blasts, the larger type's marked;
    /// a single puff of the same class is not.
    #[test]
    fn blasts_are_the_explosions_fireballs() {
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
        )
        .expect("the blast test reads the original executable's tables");
        let executable = FrontendExecutable::parse(&raw).unwrap();
        let process = ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
        };
        let mut particles = ProcessParticleRuntime::default();
        let mut rng = 7;
        for (particle_type, x) in [(1, 1000), (4, 1000), (2, 9000)] {
            let request = ParticleBirthRequest {
                particle_type,
                position: [x, 65000, 2000],
                velocity: None,
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &process,
                },
            };
            particles
                .birth_central(request, executable, &mut rng, &mut Quiet)
                .unwrap()
                .unwrap();
        }
        let blasts = particles.blasts();
        assert_eq!(blasts.len(), 16);
        // Type 2's, born at 9000, are the larger; type 1's at 1000.
        for blast in blasts {
            let x = i32::from(blast.position[0]);
            let from = if blast.large { 9000 } else { 1000 };
            assert!((x - from).abs() < 400, "{blast:?}");
        }
        // None of them is a cannon's spark; type 5, the spark, is.
        assert!(particles.sparks(executable).is_empty());
        let request = ParticleBirthRequest {
            particle_type: 5,
            position: [3000, 65000, 2000],
            velocity: None,
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &process,
            },
        };
        particles
            .birth_central(request, executable, &mut rng, &mut Quiet)
            .unwrap()
            .unwrap();
        let sparks = particles.sparks(executable);
        assert_eq!(sparks.len(), 1);
        assert_eq!(sparks[0].position, [3000, 65000, 2000]);
        assert!(particles.bolts(executable).is_empty());
    }
}
