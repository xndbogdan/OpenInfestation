use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::derived::update_prefix::FamilyUpdateEffect, ObjectArena, PlayerCommandContext,
    },
    process_sound::{ProcessSoundRef, ProcessSoundRuntime},
    terrain_runtime::TerrainSpacing,
    ResidentRuntime,
};
use inf_formats::FrontendExecutable;
use serde_json::Value;

use super::super::drive::{VehicleDriveHost, VehicleGround};
use super::super::test_support::{
    controls_from, controls_words, loaded, words, DamageServices, NoVoices, Receipt,
};

/// Sounds become real persistent records; the RNG is 0048F060 over the
/// recording's word.
struct Host<'a> {
    sounds: &'a mut ProcessSoundRuntime,
    resident: &'a ResidentRuntime,
    rng: u32,
    events: Vec<String>,
    damage: DamageServices,
}

impl VehicleDriveHost for Host<'_> {
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
        gain: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        self.events
            .push(format!("create {sound:x} {gain:x} {pitch:x}"));
        self.sounds
            .create_persistent(self.resident, sound as i16, gain, pitch, 0, || {
                Ok::<_, std::convert::Infallible>([0, 0, 0])
            })
            .map_err(|error| format!("{error:?}"))
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        gain: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.events.push(format!("update {gain:x} {pitch:x}"));
        Ok(())
    }
    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.events.push("release".into());
        self.sounds
            .delete_record(reference, &mut NoVoices)
            .map_err(|error| format!("{error:?}"))
    }
}

impl VehicleUpdateHost for Host<'_> {
    fn tow_beam(&mut self, _: u32, _: [u16; 3]) -> Result<(), Self::Error> {
        unreachable!("no tow in this test")
    }
    fn random(&mut self) -> Result<u32, String> {
        self.rng = self.rng.wrapping_mul(0x13_d56d).wrapping_add(0x13_d573);
        Ok(self.rng >> 16)
    }
    fn hull_spray(&mut self, _: &ObjectArena, _: ObjectId, _: i32) -> Result<(), String> {
        Err("hull spray".into())
    }
    fn water_splash(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        Err("water splash".into())
    }
    fn wake_particle(
        &mut self,
        _: crate::object_runtime::WakeParticle,
        _: [u16; 3],
        _: [u16; 3],
    ) -> Result<(), String> {
        Err("wake particle".into())
    }
    fn status_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), String> {
        self.events.push(format!("status {effect:?}"));
        Ok(())
    }
    fn timed_damage(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        request: crate::object_runtime::ActorTimedDamageRequest,
    ) -> Result<(), String> {
        self.events.push(format!("timed damage {request:?}"));
        arena
            .apply_actor_timed_damage(vehicle, request, &mut self.damage)
            .map_err(|error| error.to_string())
    }
    /// The fixture's other actors are not at Infest01's positions, so a
    /// drop or a take is recorded, not performed.
    fn drop_item(&mut self, _: &mut ObjectArena, _: ObjectId, index: usize) -> Result<(), String> {
        self.events.push(format!("drop {index}"));
        Ok(())
    }
    fn carry_item(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        item: ObjectId,
    ) -> Result<bool, String> {
        self.events.push(format!("carry {item:?}"));
        Ok(true)
    }
    fn player_text(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        text: u16,
    ) -> Result<(), String> {
        self.events.push(format!("text {text:#x}"));
        Ok(())
    }
    /// The fire region's shots belong to its own receipt test: here they
    /// are recorded.
    fn fire_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: crate::object_runtime::ActorWeaponFireRequest,
    ) -> Result<(), String> {
        self.events.push(format!("fire {request:?}"));
        Ok(())
    }
    fn queue_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        microseconds: u32,
        direction: [u32; 3],
        fraction: u32,
    ) -> Result<(), String> {
        self.events
            .push(format!("queue {microseconds} {direction:x?} {fraction:#x}"));
        Ok(())
    }
    /// The aim's world queries belong to the aim's own receipt test: here
    /// every sight is clear and no candidate can be damaged.
    fn sight(
        &mut self,
        _: &ObjectArena,
        origin: [u16; 3],
        target: [u16; 3],
    ) -> Result<crate::world_sight::Sight, String> {
        self.events.push(format!("sight {origin:?} {target:?}"));
        Ok(crate::world_sight::Sight {
            hit: None,
            length: 0,
        })
    }
    fn damage_predicate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        candidate: ObjectId,
    ) -> Result<bool, String> {
        self.events.push(format!("damage predicate {candidate:?}"));
        Ok(false)
    }
}

/// A live persistent record exists exactly when the recording's slot is set.
fn sync_sound(
    slot: &mut Option<ProcessSoundRef>,
    captured: bool,
    sounds: &mut ProcessSoundRuntime,
    resident: &ResidentRuntime,
) {
    match (*slot, captured) {
        (Some(reference), false) => {
            sounds.delete_record(reference, &mut NoVoices).unwrap();
            *slot = None;
        }
        (None, true) => {
            *slot = Some(
                sounds
                    .create_persistent(resident, 0x15, 0, 0, 0, || {
                        Ok::<_, std::convert::Infallible>([0, 0, 0])
                    })
                    .unwrap(),
            );
        }
        _ => {}
    }
}

/// Every field the update reads, from the captured entry state.
fn inject(
    arena: &mut ObjectArena,
    vehicle: ObjectId,
    receipt: &Receipt<'_>,
    sounds: &mut ProcessSoundRuntime,
    resident: &ResidentRuntime,
) {
    assert_eq!(
        arena.object(vehicle).unwrap().base.word_28,
        receipt.low(0x28)
    );
    {
        let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
        base.flags = (base.flags & !0x6000_0100) | (receipt.dword(0x2c) & 0x6000_0100);
        base.position_words = Some([receipt.high(0x30), receipt.low(0x34), receipt.high(0x34)]);
    }
    {
        let actor = arena.actor_mut(vehicle).unwrap();
        for index in 0..10 {
            actor.rotating.basis.words[index] = receipt.dword(0x44 + index as u32 * 4);
        }
        actor.word_8c = receipt.low(0x8c);
        actor.words_8e_92 = [receipt.high(0x8c), receipt.low(0x90), receipt.high(0x90)];
        actor.words_94_9c = [
            receipt.dword(0x94),
            receipt.dword(0x98),
            receipt.dword(0x9c),
        ];
        actor.health = receipt.low(0xa0);
        actor.word_c4 = receipt.dword(0xc4);
        actor.word_c8 = receipt.low(0xc8);
        actor.word_cc = receipt.low(0xcc);
        actor.word_ce = receipt.high(0xcc);
    }
    arena.derived_actor_mut(vehicle).unwrap().byte_160 = receipt.dword(0x160) as u8;
    let mut engine = arena.derived_actor(vehicle).unwrap().sound_168;
    sync_sound(&mut engine, receipt.dword(0x168) != 0, sounds, resident);
    arena.derived_actor_mut(vehicle).unwrap().sound_168 = engine;
    let mut skid = arena.selector2_actor(vehicle).unwrap().drive.sound_548;
    sync_sound(&mut skid, receipt.dword(0x548) != 0, sounds, resident);
    let state = arena.selector2_actor_mut(vehicle).unwrap();
    state.word_184 = Some(receipt.dword(0x184));
    state.words_188_194 = [0x188, 0x18c, 0x190, 0x194].map(|offset| receipt.dword(offset));
    state.drive.normal_198 = Some([0x198, 0x19c, 0x1a0].map(|offset| receipt.dword(offset)));
    state.byte_1c0 = receipt.dword(0x1c0) as u8;
    state.dwords_1e8_1f4 = [0x1e8, 0x1ec, 0x1f0, 0x1f4].map(|offset| receipt.dword(offset));
    state.drive.dword_1fc = Some(receipt.dword(0x1fc));
    state.dwords_200_204 = [receipt.dword(0x200), receipt.dword(0x204)];
    let bytes_208 = receipt.dword(0x208).to_le_bytes();
    state.byte_208 = bytes_208[0];
    state.byte_209 = bytes_208[1];
    state.bytes_20a_20c[0] = bytes_208[2];
    state.bytes_20a_20c[1] = bytes_208[3];
    state.bytes_20a_20c[2] = receipt.dword(0x20c) as u8;
    state.word_20e = receipt.high(0x20c);
    state.dwords_210_218 = [
        receipt.dword(0x210),
        receipt.dword(0x214),
        receipt.dword(0x218),
    ];
    state.drive.dword_21c = Some(receipt.dword(0x21c));
    state.dwords_220_224 = [receipt.dword(0x220), receipt.dword(0x224)];
    state.dword_228 = Some(receipt.dword(0x228));
    state.drive.words_22c_236 = Some([
        receipt.low(0x22c),
        receipt.high(0x22c),
        receipt.low(0x230),
        receipt.high(0x230),
        receipt.low(0x234),
        receipt.high(0x234),
    ]);
    state.dword_238 = receipt.dword(0x238);
    state.drive.dword_23c = Some(receipt.dword(0x23c));
    state.drive.dword_240 = Some(receipt.dword(0x240));
    state.dword_244 = receipt.dword(0x244);
    state.dwords_52c_530 = [receipt.dword(0x52c), receipt.dword(0x530)];
    let bytes_534 = receipt.dword(0x534).to_le_bytes();
    state.bytes_534_535 = [bytes_534[0], bytes_534[1]];
    for index in 0..8 {
        state.dwords_538_554[index] = receipt.dword(0x538 + index as u32 * 4);
    }
    state.drive.sound_548 = skid;
    state.dword_558 = receipt.dword(0x558);
    state.drive.dword_55c = Some(receipt.dword(0x55c));
    state.drive.dword_560 = Some(receipt.dword(0x560));
    state.dword_564 = receipt.dword(0x564);
    state.dword_568 = receipt.dword(0x568);
    state.words_56c_570 = [receipt.low(0x56c), receipt.high(0x56c), receipt.low(0x570)];
    state.drive.dword_574 = Some(receipt.dword(0x574));
    state.drive.dword_578 = Some(receipt.dword(0x578));
    state.drive.dword_57c = Some(receipt.dword(0x57c));
    state.dwords_580_584[0] = receipt.dword(0x580);
    state.drive.word_58c = Some(receipt.low(0x58c));
    state.update.handle_598 = Some(crate::object_runtime::ActorHandle::from_raw(
        receipt.dword(0x598),
    ));
    state.dwords_59c_5a0 = [receipt.dword(0x59c), receipt.dword(0x5a0)];
    state.dwords_5ac_5b0[0] = receipt.dword(0x5ac);
}

/// Every field the update writes, against the captured return state. The
/// aim (0041F9C0, an omission) owns flag 4 and +148, which are not compared.
fn mismatches(arena: &ObjectArena, vehicle: ObjectId, receipt: &Receipt<'_>) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |name: &str, port: u64, native: u64| {
        if port != native {
            out.push(format!("{name}: port {port:x} native {native:x}"));
        }
    };
    let base = &arena.object(vehicle).unwrap().base;
    check(
        "flags 0x60000100",
        (base.flags & 0x6000_0100).into(),
        (receipt.dword(0x2c) & 0x6000_0100).into(),
    );
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
    check("+8c", actor.word_8c.into(), receipt.low(0x8c).into());
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
    check("+a0", actor.health.into(), receipt.low(0xa0).into());
    check("+c8", actor.word_c8.into(), receipt.low(0xc8).into());
    check("+cc", actor.word_cc.into(), receipt.low(0xcc).into());
    check("+ce", actor.word_ce.into(), receipt.high(0xcc).into());
    check("+d8", actor.word_d8.into(), receipt.dword(0xd8).into());
    let derived = arena.derived_actor(vehicle).unwrap();
    check(
        "+160",
        derived.byte_160.into(),
        (receipt.dword(0x160) & 0xff).into(),
    );
    check(
        "+168 live",
        u64::from(derived.sound_168.is_some()),
        u64::from(receipt.dword(0x168) != 0),
    );
    let state = arena.selector2_actor(vehicle).unwrap();
    for (index, offset) in [0x188, 0x18c, 0x190, 0x194].into_iter().enumerate() {
        check(
            &format!("+{offset:x}"),
            state.words_188_194[index].into(),
            receipt.dword(offset).into(),
        );
    }
    let normal = state.drive.normal_198.unwrap();
    for (index, offset) in [0x198, 0x19c, 0x1a0].into_iter().enumerate() {
        check(
            &format!("+{offset:x}"),
            normal[index].into(),
            receipt.dword(offset).into(),
        );
    }
    check(
        "+1c0",
        state.byte_1c0.into(),
        (receipt.dword(0x1c0) & 0xff).into(),
    );
    for (index, offset) in [0x1e8, 0x1ec, 0x1f0, 0x1f4].into_iter().enumerate() {
        check(
            &format!("+{offset:x}"),
            state.dwords_1e8_1f4[index].into(),
            receipt.dword(offset).into(),
        );
    }
    check(
        "+1fc",
        state.drive.dword_1fc.unwrap().into(),
        receipt.dword(0x1fc).into(),
    );
    check(
        "+200",
        state.dwords_200_204[0].into(),
        receipt.dword(0x200).into(),
    );
    check(
        "+204",
        state.dwords_200_204[1].into(),
        receipt.dword(0x204).into(),
    );
    let bytes_208 = receipt.dword(0x208).to_le_bytes();
    check("+209", state.byte_209.into(), bytes_208[1].into());
    check("+20a", state.bytes_20a_20c[0].into(), bytes_208[2].into());
    check(
        "+20c",
        state.bytes_20a_20c[2].into(),
        (receipt.dword(0x20c) & 0xff).into(),
    );
    check("+20e", state.word_20e.into(), receipt.high(0x20c).into());
    for (index, offset) in [0x210, 0x214, 0x218].into_iter().enumerate() {
        check(
            &format!("+{offset:x}"),
            state.dwords_210_218[index].into(),
            receipt.dword(offset).into(),
        );
    }
    check(
        "+21c",
        state.drive.dword_21c.unwrap().into(),
        receipt.dword(0x21c).into(),
    );
    check(
        "+220",
        state.dwords_220_224[0].into(),
        receipt.dword(0x220).into(),
    );
    check(
        "+224",
        state.dwords_220_224[1].into(),
        receipt.dword(0x224).into(),
    );
    check(
        "+228",
        state.dword_228.unwrap().into(),
        receipt.dword(0x228).into(),
    );
    check("+238", state.dword_238.into(), receipt.dword(0x238).into());
    check(
        "+23c",
        state.drive.dword_23c.unwrap().into(),
        receipt.dword(0x23c).into(),
    );
    check(
        "+240",
        state.drive.dword_240.unwrap().into(),
        receipt.dword(0x240).into(),
    );
    check("+244", state.dword_244.into(), receipt.dword(0x244).into());
    check(
        "+52c",
        state.dwords_52c_530[0].into(),
        receipt.dword(0x52c).into(),
    );
    check(
        "+530",
        state.dwords_52c_530[1].into(),
        receipt.dword(0x530).into(),
    );
    let bytes_534 = receipt.dword(0x534).to_le_bytes();
    check("+534", state.bytes_534_535[0].into(), bytes_534[0].into());
    check("+535", state.bytes_534_535[1].into(), bytes_534[1].into());
    for index in [0usize, 1, 2, 3, 6, 7] {
        let offset = 0x538 + index as u32 * 4;
        check(
            &format!("+{offset:x}"),
            state.dwords_538_554[index].into(),
            receipt.dword(offset).into(),
        );
    }
    check(
        "+548 live",
        u64::from(state.drive.sound_548.is_some()),
        u64::from(receipt.dword(0x548) != 0),
    );
    check("+558", state.dword_558.into(), receipt.dword(0x558).into());
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
        "+56c",
        state.words_56c_570[0].into(),
        receipt.low(0x56c).into(),
    );
    check(
        "+56e",
        state.words_56c_570[1].into(),
        receipt.high(0x56c).into(),
    );
    check(
        "+570",
        state.words_56c_570[2].into(),
        receipt.low(0x570).into(),
    );
    check(
        "+574",
        state.drive.dword_574.unwrap().into(),
        receipt.dword(0x574).into(),
    );
    check(
        "+578",
        state.drive.dword_578.unwrap().into(),
        receipt.dword(0x578).into(),
    );
    check(
        "+57c",
        state.drive.dword_57c.unwrap().into(),
        receipt.dword(0x57c).into(),
    );
    check(
        "+580",
        state.dwords_580_584[0].into(),
        receipt.dword(0x580).into(),
    );
    check(
        "+58c",
        state.drive.word_58c.unwrap().into(),
        receipt.low(0x58c).into(),
    );
    check(
        "+594",
        state.update.byte_594.unwrap().into(),
        (receipt.dword(0x594) & 0xff).into(),
    );
    check(
        "+598",
        state.update.handle_598.unwrap().raw().into(),
        receipt.dword(0x598).into(),
    );
    check(
        "+59c",
        state.dwords_59c_5a0[0].into(),
        receipt.dword(0x59c).into(),
    );
    check(
        "+5a0",
        state.dwords_59c_5a0[1].into(),
        receipt.dword(0x5a0).into(),
    );
    check(
        "+5ac",
        state.dwords_5ac_5b0[0].into(),
        receipt.dword(0x5ac).into(),
    );
    out
}

/// Infest01's 707 Level-1 calls of 00416170 (fine terrain), each run from
/// its captured entry state.
#[test]
fn vehicle_update_matches_every_infest01_level1_tick() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_update_infest01.json"
    ))
    .unwrap();
    let offsets = fixture["offsets"].as_array().unwrap();
    let player_offsets: Vec<u32> = words(&fixture["player_offsets"]);
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let assets: &FrontendAssets = &assets;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let mut failures = Vec::new();
    let ticks = fixture["ticks"].as_array().unwrap();
    for (index, tick) in ticks.iter().enumerate() {
        let entry = Receipt {
            offsets,
            values: tick["v_in"].as_array().unwrap(),
        };
        let exit = Receipt {
            offsets,
            values: tick["v_out"].as_array().unwrap(),
        };
        inject(&mut world.arena, vehicle, &entry, &mut sounds, &resident);
        let player_in = words(&tick["p_in"]);
        let field = |values: &[u32], offset: u32| {
            values[player_offsets
                .iter()
                .position(|&value| value == offset)
                .unwrap()]
        };
        {
            let state = world.arena.world_player_mut(player).unwrap();
            // Shielding (+142) read 3 at each of Infest01's 004212F0 calls
            // (the shieldlevel query); the fixture does not carry +13C.
            state.bytes_13c_143[6] = 3;
            for word in 0..6 {
                state.bits_100[word] = field(&player_in, 0x100 + word as u32 * 4);
            }
            state.dword_12c = field(&player_in, 0x12c);
            state.command_mask_130 = [0x130, 0x134, 0x138].map(|offset| field(&player_in, offset));
        }
        let mut controls = controls_from(&words(&tick["c_in"]));
        let rng = words(&tick["rng"]);
        let mut host = Host {
            sounds: &mut sounds,
            resident: &resident,
            rng: rng[0],
            events: Vec::new(),
            damage: DamageServices::new(crate::object_runtime::ActorHandle::from_raw(0)),
        };
        let request = VehicleUpdateRequest {
            drive: VehicleDriveRequest {
                delta: tick["delta"].as_u64().unwrap() as u32,
                executable,
                ground: VehicleGround {
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: tick["clock"].as_u64().unwrap() as u32,
                    sine_quarter: &sine,
                },
                process_null: crate::object_runtime::ActorHandle::from_raw(0),
                position_step: super::super::drive::VehiclePositionStep::Retail,
                view_zoom_5c0: &std::cell::Cell::new(0),
                network: None,
            },
            attract: false,
            current_world: world.mission.current_world_a0,
            status_load_factor: Some(0x10000),
            settings_7c: 1,
            special_view: false,
            carry: None,
            command: PlayerCommandContext {
                process: Some(&process),
                executable: Some(executable),
                resident: Some(&resident),
            },
        };
        if let Err(error) =
            world
                .arena
                .tick_vehicle_update(vehicle, &mut controls, &request, &mut host)
        {
            failures.push(format!("tick {index}: {error}"));
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
        if host.rng != rng[1] {
            found.push(format!("rng: port {:x} native {:x}", host.rng, rng[1]));
        }
        let player_out = words(&tick["p_out"]);
        let state = world.arena.world_player(player).unwrap();
        if state.command_mask_130 != [0x130, 0x134, 0x138].map(|offset| field(&player_out, offset))
        {
            found.push(format!(
                "player commands: port {:x?}",
                state.command_mask_130
            ));
        }
        if !found.is_empty() {
            failures.push(format!("tick {index}: {}", found.join("; ")));
        }
    }
    // Infest01's drop at 565 and take at 581 each birthed a slot flash
    // (00489A90) through RNG draws this replay does not make: the fixture
    // does not carry the hold, so the drop finds it empty, and the host
    // records a take without performing it. The RNG word is all they may
    // differ in.
    const EXPECTED: [&str; 2] = ["tick 565: ", "tick 581: "];
    let unexpected: Vec<&String> = failures
        .iter()
        .filter(|failure| !EXPECTED.iter().any(|tick| failure.starts_with(tick)))
        .collect();
    assert_eq!(
        failures.len() - unexpected.len(),
        EXPECTED.len(),
        "{failures:#?}"
    );
    for failure in &failures {
        if EXPECTED.iter().any(|tick| failure.starts_with(tick)) {
            let found = failure.split_once(": ").unwrap().1;
            assert!(
                found.starts_with("rng: ") && !found.contains("; "),
                "{failure}"
            );
        }
    }
    let failures = unexpected;
    assert!(
        failures.is_empty(),
        "{} of {} ticks differ:\n{}",
        failures.len(),
        ticks.len(),
        failures
            .iter()
            .take(12)
            .map(|failure| failure.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `004178FB..00417953`: L (the block's +39, player +3D) with the
/// Headlights (0x14) owned toggles +208 and clears the byte, as Infest03's
/// two presses switched them on and off; with Infrared vision (0x3D) owned
/// it toggles infrared vision (the world manager's +5C4) instead, and the
/// headlights stay; owning neither, the press only clears. Each update runs
/// from Infest01's first recorded entry.
#[test]
fn l_switches_the_headlights_or_infrared_vision() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_update_infest01.json"
    ))
    .unwrap();
    let offsets = fixture["offsets"].as_array().unwrap();
    let tick = &fixture["ticks"].as_array().unwrap()[0];
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let assets: &FrontendAssets = &assets;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let entry = Receipt {
        offsets,
        values: tick["v_in"].as_array().unwrap(),
    };
    let mut press = |world: &mut crate::world_backend::WorldSession, upgrades: &[u32]| {
        // The entry state, keeping the headlights the last press left.
        let lights = world.arena.selector2_actor(vehicle).unwrap().byte_208;
        inject(&mut world.arena, vehicle, &entry, &mut sounds, &resident);
        world.arena.selector2_actor_mut(vehicle).unwrap().byte_208 = lights;
        {
            // The grant table's "Headlights" and "Infrared vision".
            let state = world.arena.world_player_mut(player).unwrap();
            for id in [0x14, 0x3d] {
                state.bits_100[(id >> 5) as usize] &= !(1 << (id & 31));
            }
            for &id in upgrades {
                state.bits_100[(id >> 5) as usize] |= 1 << (id & 31);
            }
        }
        let mut controls = controls_from(&words(&tick["c_in"]));
        controls.set_byte(0x3d, 1);
        let mut host = Host {
            sounds: &mut sounds,
            resident: &resident,
            rng: words(&tick["rng"])[0],
            events: Vec::new(),
            damage: DamageServices::new(crate::object_runtime::ActorHandle::from_raw(0)),
        };
        let request = VehicleUpdateRequest {
            drive: VehicleDriveRequest {
                delta: tick["delta"].as_u64().unwrap() as u32,
                executable,
                ground: VehicleGround {
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: tick["clock"].as_u64().unwrap() as u32,
                    sine_quarter: &sine,
                },
                process_null: crate::object_runtime::ActorHandle::from_raw(0),
                position_step: super::super::drive::VehiclePositionStep::Retail,
                view_zoom_5c0: &std::cell::Cell::new(0),
                network: None,
            },
            attract: false,
            current_world: world.mission.current_world_a0,
            status_load_factor: Some(0x10000),
            settings_7c: 1,
            special_view: false,
            carry: None,
            command: PlayerCommandContext {
                process: Some(&process),
                executable: Some(executable),
                resident: Some(&resident),
            },
        };
        let report = world
            .arena
            .tick_vehicle_update(vehicle, &mut controls, &request, &mut host)
            .unwrap();
        assert_eq!(controls.byte(0x3d), 0);
        report
    };
    let headlights = |world: &crate::world_backend::WorldSession| {
        world.arena.selector2_actor(vehicle).unwrap().byte_208
    };
    let report = press(&mut world, &[0x14]);
    assert!(!report.toggles_infrared);
    assert_eq!(headlights(&world), 1);
    press(&mut world, &[0x14]);
    assert_eq!(headlights(&world), 0);
    // Infrared alone toggles, as retail's does.
    let report = press(&mut world, &[0x3d]);
    assert!(report.toggles_infrared && !report.steps_lamps);
    assert_eq!(headlights(&world), 0);
    // Port fix: with both, the caller steps through the lamps.
    let report = press(&mut world, &[0x14, 0x3d]);
    assert!(report.steps_lamps && !report.toggles_infrared);
    assert_eq!(headlights(&world), 0);
    let report = press(&mut world, &[]);
    assert!(!report.toggles_infrared);
    assert_eq!(headlights(&world), 0);
}

/// `0041657E..00416668`: without Invulnerability (0x29) the Alien Brain
/// (world 13h) sends a request for modes 3 to 5 back to the buggy with "Only
/// buggy modes are useable after that last warp jump" (H0 16Ah), and a world
/// with flag 400h (Helniazine's) does so for modes 4 and 5 with "Your craft
/// cannot fly on this world." (H0 154h). Owning 0x29 lifts both. Each
/// request runs from Infest01's first recorded entry with every mode owned.
#[test]
fn a_refused_mode_says_why() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_update_infest01.json"
    ))
    .unwrap();
    let offsets = fixture["offsets"].as_array().unwrap();
    let tick = &fixture["ticks"].as_array().unwrap()[0];
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let assets: &FrontendAssets = &assets;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let entry = Receipt {
        offsets,
        values: tick["v_in"].as_array().unwrap(),
    };
    let mut request_mode = |world: &mut crate::world_backend::WorldSession,
                            mode: u32,
                            current_world: u32,
                            world_flags: u32,
                            invulnerable: bool| {
        inject(&mut world.arena, vehicle, &entry, &mut sounds, &resident);
        {
            let state = world.arena.world_player_mut(player).unwrap();
            for mode in 1..6 {
                let bit = read_word::<String>(executable, MODE_UPGRADES + mode * 2).unwrap();
                state.bits_100[(bit >> 5) as usize] |= 1 << (bit & 31);
            }
            let bit = 0x29_u32;
            if invulnerable {
                state.bits_100[(bit >> 5) as usize] |= 1 << (bit & 31);
            } else {
                state.bits_100[(bit >> 5) as usize] &= !(1 << (bit & 31));
            }
        }
        let mut controls = controls_from(&words(&tick["c_in"]));
        controls.dword_34 = mode;
        let mut host = Host {
            sounds: &mut sounds,
            resident: &resident,
            rng: words(&tick["rng"])[0],
            events: Vec::new(),
            damage: DamageServices::new(crate::object_runtime::ActorHandle::from_raw(0)),
        };
        let request = VehicleUpdateRequest {
            drive: VehicleDriveRequest {
                delta: tick["delta"].as_u64().unwrap() as u32,
                executable,
                ground: VehicleGround {
                    terrain: &world.terrain,
                    world_flags,
                    clock: tick["clock"].as_u64().unwrap() as u32,
                    sine_quarter: &sine,
                },
                process_null: crate::object_runtime::ActorHandle::from_raw(0),
                position_step: super::super::drive::VehiclePositionStep::Retail,
                view_zoom_5c0: &std::cell::Cell::new(0),
                network: None,
            },
            attract: false,
            current_world,
            status_load_factor: Some(0x10000),
            settings_7c: 1,
            special_view: false,
            carry: None,
            command: PlayerCommandContext {
                process: Some(&process),
                executable: Some(executable),
                resident: Some(&resident),
            },
        };
        // A refusal drives on as the buggy; an admitted hover or flight
        // goes on to the spray or wake this host refuses, after the request.
        let result = world
            .arena
            .tick_vehicle_update(vehicle, &mut controls, &request, &mut host);
        let mode = world.arena.selector2_actor(vehicle).unwrap().dwords_1e8_1f4[2];
        if mode == 1 {
            result.unwrap();
        }
        let texts: Vec<String> = host
            .events
            .into_iter()
            .filter(|event| event.starts_with("text"))
            .collect();
        (mode, texts)
    };
    let flags = world.world_flags & !0x400;
    let said = |text: u16| vec![format!("text {text:#x}")];
    for mode in [3, 4, 5] {
        assert_eq!(
            request_mode(&mut world, mode, 0x13, flags, false),
            (1, said(WARP_REFUSAL_TEXT)),
            "mode {mode} on the Alien Brain"
        );
        assert_eq!(
            request_mode(&mut world, mode, 0x13, flags, true),
            (mode, vec![])
        );
    }
    assert_eq!(request_mode(&mut world, 2, 0x13, flags, false), (2, vec![]));
    for mode in [4, 5] {
        assert_eq!(
            request_mode(&mut world, mode, 6, flags | 0x400, false),
            (1, said(FLIGHT_REFUSAL_TEXT)),
            "mode {mode} under flag 400h"
        );
        assert_eq!(
            request_mode(&mut world, mode, 6, flags | 0x400, true),
            (mode, vec![])
        );
    }
    assert_eq!(
        request_mode(&mut world, 3, 6, flags | 0x400, false),
        (3, vec![])
    );
    assert_eq!(request_mode(&mut world, 4, 6, flags, false), (4, vec![]));
}

/// Every update Infest02 and Infest03 entered with a weapon cycle key held
/// (`cycle` queries): the panel timers (004163A2..0041646E) and the cycling
/// region (004166CD..0041685A) take the entry state to retail's state at
/// 0041685A. A held key opens the panel or, with a turn, asks for the next or
/// previous weapon; the released key keeps its timer at the clock.
#[test]
fn weapon_cycling_matches_the_recorded_holds() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_weapon_cycle_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let pair = |value: &Value| -> [u32; 2] {
        let values = words(value);
        [values[0], values[1]]
    };
    let mut selections = 0;
    for (index, case) in cases.iter().enumerate() {
        let entry = &case["in"];
        let cycled = &case["cycled"];
        assert_eq!(entry["message"], 0);
        {
            let state = world.arena.selector2_actor_mut(vehicle).unwrap();
            let timers = pair(&entry["timers"]);
            state.dwords_538_554[0] = timers[0];
            state.dwords_538_554[1] = timers[1];
            state.dwords_52c_530 = pair(&entry["panels"]);
            state.bytes_534_535 = pair(&entry["open"]).map(|byte| byte as u8);
            let slides = pair(&entry["slides"]);
            state.dwords_538_554[2] = slides[0];
            state.dwords_538_554[3] = slides[1];
        }
        let mut controls = controls_from(&words(&entry["controls"]));
        let mut host = Host {
            sounds: &mut sounds,
            resident: &resident,
            rng: 0,
            events: Vec::new(),
            damage: DamageServices::new(crate::object_runtime::ActorHandle::from_raw(0)),
        };
        let opened = pair(&entry["open"])[0] == 0;
        world
            .arena
            .vehicle_weapon_panels(vehicle, 125_000, &mut host)
            .unwrap();
        let clock = cycled["clock"].as_u64().unwrap() as u32;
        world
            .arena
            .vehicle_weapon_cycle(vehicle, &mut controls, Some(player), clock, &mut host)
            .unwrap();
        let state = world.arena.selector2_actor(vehicle).unwrap();
        assert_eq!(
            [state.dwords_538_554[0], state.dwords_538_554[1]],
            pair(&cycled["timers"]),
            "case {index} timers"
        );
        assert_eq!(
            state.dwords_52c_530,
            pair(&cycled["panels"]),
            "case {index} panels"
        );
        assert_eq!(
            state.bytes_534_535.map(u32::from),
            pair(&cycled["open"]),
            "case {index} open"
        );
        assert_eq!(
            [state.dwords_538_554[2], state.dwords_538_554[3]],
            pair(&cycled["slides"]),
            "case {index} slides"
        );
        assert_eq!(
            controls_words(&controls),
            words(&cycled["controls"]),
            "case {index} controls"
        );
        let now_open = state.bytes_534_535[0] != 0;
        assert_eq!(
            host.events,
            if opened && now_open {
                vec!["play 16 5555 10000".to_string()]
            } else {
                Vec::new()
            },
            "case {index} sounds"
        );
        selections += usize::from(controls.dword_28 != 0);
    }
    assert!(selections > 0);
}

/// `00416090` on a live vehicle the player left for a unit it holds (the
/// implant's creature, a taken truck): the grenade charge +218 cleared, so
/// it throws nothing, then the controlled update on the zeroed 0x44-byte
/// block whose +3A is 1 (`004160BE`), the player's +3E with the block
/// standing for player +4. That brake gate puts the ground drive's gear
/// +238 in neutral (1); the same update on a plain zero block leaves the
/// buggy in drive (2) and its charge alone. Each run starts from
/// Infest01's second recorded Level 1 entry, the buggy settled in its
/// ground form (at the first, entering it resets the gear to neutral).
#[test]
fn a_vehicle_left_for_a_held_unit_runs_its_update_on_the_braked_block() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_update_infest01.json"
    ))
    .unwrap();
    let offsets = fixture["offsets"].as_array().unwrap();
    let tick = &fixture["ticks"].as_array().unwrap()[1];
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let assets: &FrontendAssets = &assets;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let entry = Receipt {
        offsets,
        values: tick["v_in"].as_array().unwrap(),
    };
    let mut run = |world: &mut crate::world_backend::WorldSession, unheld: bool| {
        inject(&mut world.arena, vehicle, &entry, &mut sounds, &resident);
        {
            let state = world.arena.selector2_actor_mut(vehicle).unwrap();
            state.dwords_210_218[2] = 0x1234;
            state.dword_238 = 2;
        }
        let mut host = Host {
            sounds: &mut sounds,
            resident: &resident,
            rng: words(&tick["rng"])[0],
            events: Vec::new(),
            damage: DamageServices::new(crate::object_runtime::ActorHandle::from_raw(0)),
        };
        let request = VehicleUpdateRequest {
            drive: VehicleDriveRequest {
                delta: tick["delta"].as_u64().unwrap() as u32,
                executable,
                ground: VehicleGround {
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: tick["clock"].as_u64().unwrap() as u32,
                    sine_quarter: &sine,
                },
                process_null: crate::object_runtime::ActorHandle::from_raw(0),
                position_step: super::super::drive::VehiclePositionStep::Retail,
                view_zoom_5c0: &std::cell::Cell::new(0),
                network: None,
            },
            attract: false,
            current_world: world.mission.current_world_a0,
            status_load_factor: Some(0x10000),
            settings_7c: 1,
            special_view: false,
            carry: None,
            command: PlayerCommandContext {
                process: Some(&process),
                executable: Some(executable),
                resident: Some(&resident),
            },
        };
        if unheld {
            world
                .arena
                .tick_vehicle_unheld(vehicle, &request, &mut host)
                .unwrap();
        } else {
            let mut controls = PlayerControlBlock::default();
            world
                .arena
                .tick_vehicle_update(vehicle, &mut controls, &request, &mut host)
                .unwrap();
        }
        let state = world.arena.selector2_actor(vehicle).unwrap();
        (state.dwords_210_218[2], state.dword_238)
    };
    assert_eq!(run(&mut world, true), (0, 1));
    assert_eq!(run(&mut world, false), (0x1234, 2));
}
