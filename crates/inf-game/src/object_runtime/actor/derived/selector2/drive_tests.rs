use super::*;
use crate::{
    frontend_assets::FrontendAssets, process_sound::ProcessSoundRuntime,
    world_backend::WorldSession, ResidentRuntime,
};
use serde_json::Value;

use super::super::test_support::{controls_from, controls_words, loaded, words, NoVoices, Receipt};

struct Host<'a> {
    sounds: &'a mut ProcessSoundRuntime,
    resident: &'a ResidentRuntime,
    events: Vec<String>,
}

impl Host<'_> {
    fn create(&mut self, sound: u32, gain: u32, pitch: u32) -> Result<ProcessSoundRef, String> {
        self.sounds
            .create_persistent(self.resident, sound as i16, gain, pitch, 0, || {
                Ok::<_, std::convert::Infallible>([0, 0, 0])
            })
            .map_err(|error| format!("{error:?}"))
    }
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
        self.create(sound, gain, pitch)
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

/// Writes the captured entry state into every field the drive reads.
fn inject(arena: &mut ObjectArena, vehicle: ObjectId, receipt: &Receipt<'_>, host: &mut Host<'_>) {
    assert_eq!(
        arena.object(vehicle).unwrap().base.word_28,
        receipt.low(0x28)
    );
    {
        let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
        base.flags = (base.flags & !0x6000_0000) | (receipt.dword(0x2c) & 0x6000_0000);
        base.position_words = Some([receipt.high(0x30), receipt.low(0x34), receipt.high(0x34)]);
    }
    {
        let actor = arena.actor_mut(vehicle).unwrap();
        for index in 0..10 {
            actor.rotating.basis.words[index] = receipt.dword(0x44 + index as u32 * 4);
        }
        actor.words_8e_92 = [receipt.high(0x8c), receipt.low(0x90), receipt.high(0x90)];
        actor.words_94_9c = [
            receipt.dword(0x94),
            receipt.dword(0x98),
            receipt.dword(0x9c),
        ];
    }
    arena.derived_actor_mut(vehicle).unwrap().byte_160 = receipt.dword(0x160) as u8;
    let live_sound = arena.selector2_actor(vehicle).unwrap().drive.sound_548;
    let captured_sound = receipt.dword(0x548) != 0;
    let sound = match (live_sound, captured_sound) {
        (Some(reference), true) => Some(reference),
        (Some(reference), false) => {
            host.release_sound(reference).unwrap();
            None
        }
        (None, true) => Some(host.create(0x15, 0, 0).unwrap()),
        (None, false) => None,
    };
    let state = arena.selector2_actor_mut(vehicle).unwrap();
    state.words_188_194 = [0x188, 0x18c, 0x190, 0x194].map(|offset| receipt.dword(offset));
    state.drive.normal_198 = Some([0x198, 0x19c, 0x1a0].map(|offset| receipt.dword(offset)));
    state.byte_1c0 = receipt.dword(0x1c0) as u8;
    state.dwords_1e8_1f4 = [0x1e8, 0x1ec, 0x1f0, 0x1f4].map(|offset| receipt.dword(offset));
    state.drive.dword_1fc = Some(receipt.dword(0x1fc));
    state.bytes_20a_20c[2] = receipt.dword(0x20c) as u8;
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
    state.drive.sound_548 = sound;
    state.dwords_538_554[6] = receipt.dword(0x550);
    state.drive.dword_55c = Some(receipt.dword(0x55c));
    state.drive.dword_560 = Some(receipt.dword(0x560));
    state.dword_564 = receipt.dword(0x564);
    state.dword_568 = receipt.dword(0x568);
    state.drive.dword_574 = Some(receipt.dword(0x574));
    state.drive.dword_578 = Some(receipt.dword(0x578));
    state.drive.dword_57c = Some(receipt.dword(0x57c));
    state.drive.word_58c = Some(receipt.low(0x58c));
    state.dwords_5ac_5b0[0] = receipt.dword(0x5ac);
}

/// Every field the drive writes, against the captured return state.
fn mismatches(arena: &ObjectArena, vehicle: ObjectId, receipt: &Receipt<'_>) -> Vec<String> {
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
        "+1fc",
        state.drive.dword_1fc.unwrap().into(),
        receipt.dword(0x1fc).into(),
    );
    check(
        "+20c",
        state.bytes_20a_20c[2].into(),
        (receipt.dword(0x20c) & 0xff).into(),
    );
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
    let dust = state.drive.words_22c_236.unwrap();
    for (index, native) in [
        receipt.low(0x22c),
        receipt.high(0x22c),
        receipt.low(0x230),
        receipt.high(0x230),
        receipt.low(0x234),
        receipt.high(0x234),
    ]
    .into_iter()
    .enumerate()
    {
        check(
            &format!("+{:x}", 0x22c + index * 2),
            dust[index].into(),
            native.into(),
        );
    }
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
        "+548 live",
        u64::from(state.drive.sound_548.is_some()),
        u64::from(receipt.dword(0x548) != 0),
    );
    check(
        "+550",
        state.dwords_538_554[6].into(),
        receipt.dword(0x550).into(),
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
        "+57c",
        state.drive.dword_57c.unwrap().into(),
        receipt.dword(0x57c).into(),
    );
    check(
        "+58c",
        state.drive.word_58c.unwrap().into(),
        receipt.low(0x58c).into(),
    );
    check(
        "+5ac",
        state.dwords_5ac_5b0[0].into(),
        receipt.dword(0x5ac).into(),
    );
    out
}

/// One Level-1 world per terrain detail, with its own sound records.
struct DriveWorld {
    resident: ResidentRuntime,
    assets: FrontendAssets,
    world: WorldSession,
    sounds: ProcessSoundRuntime,
}

impl DriveWorld {
    fn new(spacing: crate::terrain_runtime::TerrainSpacing) -> Self {
        let (resident, assets, world) = loaded(18, spacing);
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let sounds = ProcessSoundRuntime::new(executable).unwrap();
        Self {
            resident,
            assets,
            world,
            sounds,
        }
    }
}

/// Runs every tick of a receipt fixture from its captured entry state and
/// returns the ticks whose return state differs. A tick's `spacing` is the
/// recording's 005372F4 at entry: 7F fine (128-unit cells), FF coarse.
fn drive_receipt_failures(text: &str) -> (usize, Vec<String>) {
    use crate::terrain_runtime::TerrainSpacing;
    let fixture: Value = serde_json::from_str(text).unwrap();
    let offsets = fixture["offsets"].as_array().unwrap();
    let ticks = fixture["ticks"].as_array().unwrap();
    let mut fine: Option<DriveWorld> = None;
    let mut coarse: Option<DriveWorld> = None;
    let mut failures = Vec::new();
    for (index, tick) in ticks.iter().enumerate() {
        let spacing = tick
            .get("spacing")
            .map_or(0x7f, |value| value.as_u64().unwrap());
        let owner = if spacing == 0x7f {
            fine.get_or_insert_with(|| DriveWorld::new(TerrainSpacing::Units128))
        } else {
            assert_eq!(spacing, 0xff, "tick {index}: unknown terrain spacing");
            coarse.get_or_insert_with(|| DriveWorld::new(TerrainSpacing::Units256))
        };
        let DriveWorld {
            resident,
            assets,
            world,
            sounds,
        } = owner;
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let sine = assets.sine_table().values;
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        let player = world.arena.first_player().unwrap();
        let entry = Receipt {
            offsets,
            values: tick["v_in"].as_array().unwrap(),
        };
        let exit = Receipt {
            offsets,
            values: tick["v_out"].as_array().unwrap(),
        };
        let mut host = Host {
            sounds,
            resident,
            events: Vec::new(),
        };
        inject(&mut world.arena, vehicle, &entry, &mut host);
        let player_in = words(&tick["p_in"]);
        // Only the jump (+3A) reads the message byte the port cannot inject.
        let jump = words(&tick["c_in"])[13] >> 16 & 0xff != 0;
        assert!(
            !jump || player_in[0] & 0xff == 0,
            "tick {index}: a jump under a message"
        );
        {
            let state = world.arena.world_player_mut(player).unwrap();
            state.bits_100[0] = player_in[1];
            state.dword_12c = player_in[2];
        }
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
        if let Err(error) =
            world
                .arena
                .tick_vehicle_ground_drive(vehicle, &mut controls, &request, &mut host)
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
        let charges = world.arena.world_player(player).unwrap().dword_12c;
        if charges != words(&tick["p_out"])[2] {
            found.push(format!("player +12c: port {charges:x}"));
        }
        if !found.is_empty() {
            failures.push(format!("tick {index}: {}", found.join("; ")));
        }
    }
    (ticks.len(), failures)
}

fn assert_no_failures((count, failures): (usize, Vec<String>)) {
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

/// Infest01's 699 Level-1 calls of 004195F0 (fine terrain), each run from
/// its captured entry state on the port's own Level-1 terrain.
#[test]
fn ground_drive_matches_every_infest01_level1_tick() {
    let (count, failures) = drive_receipt_failures(include_str!(
        "../../../../../tests/fixtures/vehicle_drive_infest01.json"
    ));
    assert_eq!(count, 699);
    assert_no_failures((count, failures));
}

/// Infest02's first 517 Level-1 calls: 34 on fine terrain, then 483 after
/// the player switched to low detail in game (coarse, 256-unit cells). The
/// in-game switch keeps fine-prepared flag bits on odd cells, which coarse
/// sampling never reads. From call 517 selector 37's terrain-strip update
/// 00409A20, not yet ported, raises cells the vehicle later crosses.
#[test]
fn ground_drive_matches_infest02_on_fine_and_coarse_terrain() {
    let (count, failures) = drive_receipt_failures(include_str!(
        "../../../../../tests/fixtures/vehicle_drive_infest02.json"
    ));
    assert_eq!(count, 517);
    assert_no_failures((count, failures));
}

/// 00423200 against its original body under Unicorn
/// (`check_analog_turn.py`): the dead zone, the hold's
/// threshold and cap, the curve's knee and cap, the shifted division, and
/// the rate's clamp.
#[test]
fn analog_turn_matches_every_native_stick_hold_and_clamp() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_analog_turn_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 319);
    let word = |value: &Value| value.as_u64().unwrap() as u32;
    for case in cases {
        let input = &case["input"];
        let (rate, hold) = (word(&input["rate_220"]), word(&input["hold_248"]));
        let actual = if word(&input["analog"]) as u8 == 0 {
            (rate, hold)
        } else {
            let (rate, hold) = analog_turn(
                word(&input["step"]),
                word(&input["x"]) as i32,
                hold,
                rate as i32,
            );
            (rate as u32, hold)
        };
        let output = &case["output"];
        assert_eq!(
            actual,
            (word(&output["rate_220"]), word(&output["hold_248"])),
            "{input}"
        );
    }
}

/// With the Controls menu's Joystick on Analogue, 00419050 ends in the
/// analog turn: a stick hard over turns the buggy and starts the hold.
#[test]
fn an_analogue_stick_turns_the_buggy() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    assert_eq!(
        world
            .arena
            .selector2_actor(vehicle)
            .unwrap()
            .drive
            .dword_248,
        Some(VEHICLE_ANALOG_HOLD_APPROXIMATION)
    );
    {
        let drive = &mut world.arena.selector2_actor_mut(vehicle).unwrap().drive;
        drive.dword_21c = Some(0);
        drive.dword_240 = Some(0);
    }
    let mut controls = controls_from(&[0; 17]);
    controls.analog_04 = 1;
    controls.axes_08[0] = -60_000;
    world
        .arena
        .vehicle_turn_rate::<String>(vehicle, 125_000, &controls, 0)
        .unwrap();
    let state = world.arena.selector2_actor(vehicle).unwrap();
    let base = state.drive.dword_21c.unwrap() as i32;
    assert_eq!(state.drive.dword_248, Some(500));
    assert_eq!(
        state.dwords_220_224[0] as i32,
        analog_turn(125_000, -60_000, 0, base).0
    );
    assert_eq!(state.dwords_220_224[0] as i32, base - 77_475);
}
