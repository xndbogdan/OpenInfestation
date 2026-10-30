//! Shared support for the mode 3..5 receipt tests: Infest03's worlds on
//! coarse terrain, the captured vehicle state written into the port's
//! vehicle, the comparison with the captured return state, and a host whose
//! spray and RNG run the port's own owners over the recording's RNG word.

use std::collections::HashMap;

use inf_formats::FrontendExecutable;
use serde_json::Value;

use super::drive::{
    VehicleDriveError, VehicleDriveHost, VehicleDriveRequest, VehicleGround, VehiclePositionStep,
};
use super::test_support::{controls_from, controls_words, words, Receipt};
use super::update::VehicleUpdateHost;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::derived::update_prefix::FamilyUpdateEffect, ActorHandle, ObjectArena, ObjectId,
        PlayerControlBlock,
    },
    process_particles::{births::ParticleBirthInputs, factories::ParticleFactoryProcessState},
    process_sound::ProcessSoundRef,
    world_backend::WorldSession,
    ResidentRuntime,
};

/// One Infest03 world, loaded as production loads it, on coarse terrain.
pub(super) struct ModeWorld {
    pub(super) _resident: ResidentRuntime,
    pub(super) assets: FrontendAssets,
    pub(super) world: WorldSession,
}

#[derive(Default)]
pub(super) struct ModeWorlds {
    worlds: HashMap<u32, ModeWorld>,
}

impl ModeWorlds {
    pub(super) fn get(&mut self, world: u32) -> &mut ModeWorld {
        self.worlds.entry(world).or_insert_with(|| {
            let (resident, assets, world) = crate::test_worlds::offline_world_retained(
                world,
                crate::terrain_runtime::TerrainSpacing::Units256,
            );
            ModeWorld {
                _resident: resident,
                assets,
                world,
            }
        })
    }
}

/// Writes the captured entry state into every field the mode drives read.
pub(super) fn inject(arena: &mut ObjectArena, vehicle: ObjectId, receipt: &Receipt<'_>) {
    assert_eq!(
        arena.object(vehicle).unwrap().base.word_28,
        receipt.low(0x28)
    );
    arena
        .inject_vehicle_drive_state(vehicle, |offset| receipt.dword(offset))
        .unwrap();
}

/// Every injected field against the captured return state.
pub(super) fn mismatches(
    arena: &ObjectArena,
    vehicle: ObjectId,
    receipt: &Receipt<'_>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |name: &str, port: u64, native: u64| {
        if port != native {
            out.push(format!("{name}: port {port:x} native {native:x}"));
        }
    };
    let base = &arena.object(vehicle).unwrap().base;
    let position = base.position_words.unwrap();
    for (axis, native) in [receipt.high(0x30), receipt.low(0x34), receipt.high(0x34)]
        .into_iter()
        .enumerate()
    {
        check(
            &format!("position[{axis}]"),
            position[axis].into(),
            native.into(),
        );
    }
    let actor = arena.actor(vehicle).unwrap();
    for index in 0..10 {
        check(
            &format!("basis[{index}]"),
            actor.rotating.basis.words[index].into(),
            receipt.dword(0x44 + index as u32 * 4).into(),
        );
    }
    for (axis, native) in [receipt.high(0x8c), receipt.low(0x90), receipt.high(0x90)]
        .into_iter()
        .enumerate()
    {
        check(
            &format!("velocity[{axis}]"),
            actor.words_8e_92[axis].into(),
            native.into(),
        );
    }
    for (axis, offset) in [0x94, 0x98, 0x9c].into_iter().enumerate() {
        check(
            &format!("angular[{axis}]"),
            actor.words_94_9c[axis].into(),
            receipt.dword(offset).into(),
        );
    }
    check(
        "+160",
        arena.derived_actor(vehicle).unwrap().byte_160.into(),
        (receipt.dword(0x160) & 0xff).into(),
    );
    let state = arena.selector2_actor(vehicle).unwrap();
    let dwords: [(u32, u64); 26] = [
        (0x188, state.words_188_194[0].into()),
        (0x18c, state.words_188_194[1].into()),
        (0x190, state.words_188_194[2].into()),
        (0x194, state.words_188_194[3].into()),
        (0x198, state.drive.normal_198.unwrap()[0].into()),
        (0x19c, state.drive.normal_198.unwrap()[1].into()),
        (0x1a0, state.drive.normal_198.unwrap()[2].into()),
        (0x1e8, state.dwords_1e8_1f4[0].into()),
        (0x1ec, state.dwords_1e8_1f4[1].into()),
        (0x1f0, state.dwords_1e8_1f4[2].into()),
        (0x1f4, state.dwords_1e8_1f4[3].into()),
        (0x1fc, state.drive.dword_1fc.unwrap().into()),
        (0x21c, state.drive.dword_21c.unwrap().into()),
        (0x220, state.dwords_220_224[0].into()),
        (0x224, state.dwords_220_224[1].into()),
        (0x228, state.dword_228.unwrap().into()),
        (0x238, state.dword_238.into()),
        (0x23c, state.drive.dword_23c.unwrap().into()),
        (0x240, state.drive.dword_240.unwrap().into()),
        (0x244, state.dword_244.into()),
        (0x250, state.dwords_250_25c[0].into()),
        (0x254, state.dwords_250_25c[1].into()),
        (0x258, state.dwords_250_25c[2].into()),
        (0x25c, state.dwords_250_25c[3].into()),
        (0x260, state.dword_260.unwrap().into()),
        (0x57c, state.drive.dword_57c.unwrap().into()),
    ];
    for (offset, port) in dwords {
        check(&format!("+{offset:x}"), port, receipt.dword(offset).into());
    }
    check(
        "+24c",
        state.words_24c_24e[0].into(),
        receipt.low(0x24c).into(),
    );
    check(
        "+24e",
        state.words_24c_24e[1].into(),
        receipt.high(0x24c).into(),
    );
    check(
        "+55c",
        state.drive.dword_55c.unwrap().into(),
        receipt.dword(0x55c).into(),
    );
    check(
        "+560",
        state.drive.dword_560.unwrap().into(),
        receipt.dword(0x560).into(),
    );
    check("+564", state.dword_564.into(), receipt.dword(0x564).into());
    check("+568", state.dword_568.into(), receipt.dword(0x568).into());
    check(
        "+58c",
        state.drive.word_58c.unwrap().into(),
        receipt.low(0x58c).into(),
    );
    out
}

/// The recording's RNG word drives 0048F060 and the spray's births land in
/// a fresh particle pool; sounds are recorded.
pub(super) struct ModeHost<'a> {
    pub(super) rng: u32,
    pub(super) particles: crate::process_particles::ProcessParticleRuntime,
    pub(super) process: ParticleFactoryProcessState,
    pub(super) executable: FrontendExecutable<'a>,
    pub(super) sine: [u16; 4096],
    pub(super) plane: i16,
    pub(super) load_factor: i32,
    pub(super) events: Vec<String>,
}

impl VehicleDriveHost for ModeHost<'_> {
    type Error = String;
    fn play_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.events
            .push(format!("play {sound:x} {gain:x} {pitch:x}"));
        Ok(())
    }
    fn create_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        sound: u32,
        _: u32,
        _: u32,
    ) -> Result<ProcessSoundRef, String> {
        Err(format!("create sound {sound:x}"))
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Err("update sound".into())
    }
    fn release_sound(&mut self, _: ProcessSoundRef) -> Result<(), String> {
        Err("release sound".into())
    }
}

impl VehicleUpdateHost for ModeHost<'_> {
    fn tow_beam(&mut self, _: u32, _: [u16; 3]) -> Result<(), Self::Error> {
        unreachable!("no tow in this test")
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn hull_spray(
        &mut self,
        arena: &ObjectArena,
        vehicle: ObjectId,
        strength: i32,
    ) -> Result<(), String> {
        let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
        let actor = arena.actor(vehicle).unwrap();
        self.particles
            .emit_oriented_particles(
                crate::process_particles::oriented::OrientedParticleRequest {
                    position: &position,
                    velocity: &actor.words_8e_92,
                    basis: &actor.rotating.basis,
                    strength,
                    load_factor: self.load_factor,
                    plane: self.plane,
                    sine_quarter: &self.sine,
                },
                ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &self.process,
                },
                self.executable,
                &mut self.rng,
            )
            .map_err(|error| format!("{error:?}"))
    }
    fn water_splash(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        Err("water splash".into())
    }
    fn wake_particle(
        &mut self,
        kind: crate::object_runtime::WakeParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        let request = crate::process_particles::births::ParticleBirthRequest {
            particle_type: 0,
            position,
            velocity: Some(velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &self.process,
            },
        };
        match kind {
            crate::object_runtime::WakeParticle::Splash => self
                .particles
                .birth_impact(request, self.executable, &mut self.rng)
                .map(|_| ()),
            crate::object_runtime::WakeParticle::Thrown => self
                .particles
                .birth_material_sprite(request, true, self.executable, &mut self.rng)
                .map(|_| ()),
        }
        .map_err(|error| format!("{error:?}"))
    }
    fn status_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), String> {
        Err(format!("status {effect:?}"))
    }
    fn timed_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: crate::object_runtime::ActorTimedDamageRequest,
    ) -> Result<(), String> {
        Err(format!("timed damage {request:?}"))
    }
    fn drop_item(&mut self, _: &mut ObjectArena, _: ObjectId, index: usize) -> Result<(), String> {
        Err(format!("drop {index}"))
    }
    fn carry_item(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        item: ObjectId,
    ) -> Result<bool, String> {
        Err(format!("carry {item:?}"))
    }
    fn player_text(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        text: u16,
    ) -> Result<(), String> {
        Err(format!("text {text:#x}"))
    }
    fn fire_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: crate::object_runtime::ActorWeaponFireRequest,
    ) -> Result<(), String> {
        Err(format!("fire {request:?}"))
    }
    fn queue_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: [u32; 3],
        _: u32,
    ) -> Result<(), String> {
        Err("queue weapon".into())
    }
    fn sight(
        &mut self,
        _: &ObjectArena,
        _: [u16; 3],
        _: [u16; 3],
    ) -> Result<crate::world_sight::Sight, String> {
        Err("sight".into())
    }
    fn damage_predicate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, String> {
        Err("damage predicate".into())
    }
}

/// Runs every tick of a mode fixture through `drive` from its captured
/// entry state and returns the tick count and the ticks that differ.
pub(super) fn mode_receipt_failures(
    text: &str,
    mut drive: impl FnMut(
        &mut ObjectArena,
        ObjectId,
        &mut PlayerControlBlock,
        &VehicleDriveRequest<'_>,
        &mut ModeHost<'_>,
    ) -> Result<(), VehicleDriveError<String>>,
) -> (usize, Vec<String>) {
    let fixture: Value = serde_json::from_str(text).unwrap();
    let offsets = fixture["offsets"].as_array().unwrap();
    let ticks = fixture["ticks"].as_array().unwrap();
    let mut worlds = ModeWorlds::default();
    let mut failures = Vec::new();
    for (index, tick) in ticks.iter().enumerate() {
        assert_eq!(
            tick["spacing"].as_u64(),
            Some(0xff),
            "tick {index}: coarse terrain"
        );
        let world_id = tick["world"].as_u64().unwrap() as u32;
        let ModeWorld { assets, world, .. } = worlds.get(world_id);
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let sine = assets.sine_table().values;
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        let entry = Receipt {
            offsets,
            values: tick["v_in"].as_array().unwrap(),
        };
        let exit = Receipt {
            offsets,
            values: tick["v_out"].as_array().unwrap(),
        };
        inject(&mut world.arena, vehicle, &entry);
        let rng = tick["rng"].as_array().unwrap();
        let mut process = ParticleFactoryProcessState::native_startup();
        process.process_null_handle = Some(ActorHandle::from_raw(0));
        let mut host = ModeHost {
            rng: rng[0].as_u64().unwrap() as u32,
            particles: Default::default(),
            process,
            executable,
            sine,
            plane: (world.terrain.plane_base() >> (world.terrain.plane_shift() & 31)) as i16,
            load_factor: tick["load_factor"].as_u64().unwrap() as i32,
            events: Vec::new(),
        };
        let mut controls = controls_from(&words(&tick["c_in"]));
        let request = VehicleDriveRequest {
            delta: tick["delta"].as_u64().unwrap() as u32,
            executable,
            ground: VehicleGround {
                terrain: &world.terrain,
                world_flags: world.world_flags,
                clock: tick["clock"].as_u64().unwrap() as u32,
                sine_quarter: &sine,
            },
            process_null: ActorHandle::from_raw(0),
            position_step: VehiclePositionStep::Retail,
            view_zoom_5c0: &std::cell::Cell::new(0),
            network: None,
        };
        if let Err(error) = drive(
            &mut world.arena,
            vehicle,
            &mut controls,
            &request,
            &mut host,
        ) {
            failures.push(format!("tick {index} (world {world_id}): {error}"));
            continue;
        }
        let mut found = mismatches(&world.arena, vehicle, &exit);
        if controls_words(&controls) != words(&tick["c_out"]) {
            found.push(format!(
                "controls: port {:x?} native {:x?}",
                controls_words(&controls),
                words(&tick["c_out"])
            ));
        }
        let native_rng = rng[1].as_u64().unwrap() as u32;
        if host.rng != native_rng {
            found.push(format!("rng: port {:x} native {native_rng:x}", host.rng));
        }
        if !found.is_empty() {
            failures.push(format!(
                "tick {index} (world {world_id}): {}",
                found.join("; ")
            ));
        }
    }
    (ticks.len(), failures)
}

pub(super) fn assert_no_failures((count, failures): (usize, Vec<String>)) {
    assert!(
        failures.is_empty(),
        "{} of {count} ticks differ:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
