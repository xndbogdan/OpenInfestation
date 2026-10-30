//! 004019E0 segment A against supplied complete actors. The retarget
//! 004015F0 is outside this segment; every recorded boundary is a host call.

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorBehaviorGroup, ActorBehaviorTable, ActorConstructionRequest, ActorHandle,
        ActorRegistryConfig, ActorTerrainInputs, DerivedActorConstructionRequest,
        FixedPositionTarget, NativeTarget,
    },
    process_tasks::ProcessTaskRuntime,
    terrain_runtime::TerrainRuntime,
};
use inf_formats::{native_basis::NativeBasis, FrontendExecutable};
use serde_json::{json, Value};
use std::path::Path;

mod retarget;

const SINE_ZERO_FIXTURE: [u16; 4096] = [0; 4096];
const NULL: ActorHandle = ActorHandle::from_raw(0x1234);
/// The handle the native harness stores at actor +84.
const NATIVE_HANDLE: u32 = 0x10005;

fn construction(position: [u16; 3]) -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE_ZERO_FIXTURE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn get(case: &Value, key: &str, default: i64) -> i64 {
    case[key].as_i64().unwrap_or(default)
}

fn words<const N: usize>(value: &Value) -> [u16; N] {
    std::array::from_fn(|index| value[index].as_i64().unwrap() as u16)
}

struct Host {
    tables: ProcessActorBehaviorTables,
    tasks: ProcessTaskRuntime,
    rng: u32,
    subject: ActorHandle,
    events: Vec<Value>,
}

impl FamilyUpdateHost for Host {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(NULL)
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        self.events.push(match effect {
            FamilyUpdateEffect::Retire => json!({"kind": "retire"}),
            FamilyUpdateEffect::DelayedPhysics { microseconds, mask } => {
                json!({"kind": "physics", "delta": microseconds, "mask": mask})
            }
            FamilyUpdateEffect::WeaponTick { microseconds } => {
                json!({"kind": "weapon_tick", "delta": microseconds})
            }
            FamilyUpdateEffect::StatusTrail {
                particle_type,
                position,
                velocity,
                flags,
            } => json!({"kind": "trail", "type": particle_type, "position": position,
                "velocity": velocity, "flags": flags}),
            FamilyUpdateEffect::Drowning(_) => return Err("004019E0 has no drowning consumer"),
        });
        Ok(())
    }
}

impl Selector11ScanHost for Host {
    fn scan_tasks(&mut self) -> Option<&mut ProcessTaskRuntime> {
        panic!("retarget 004015F0 is outside segment A")
    }
    fn scan_network_flag(&self) -> Option<u32> {
        panic!("retarget 004015F0 is outside segment A")
    }
    fn scan_terrain(&self) -> Option<&TerrainRuntime> {
        panic!("retarget 004015F0 is outside segment A")
    }
}

impl Selector11UpdateHost for Host {
    fn behavior_tables(&self) -> Option<&ProcessActorBehaviorTables> {
        Some(&self.tables)
    }
    fn tasks(&self) -> Option<&ProcessTaskRuntime> {
        Some(&self.tasks)
    }
    fn player_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: ActorTimedDamageRequest,
    ) -> Result<(), Self::Error> {
        assert_eq!(request.packet.byte_03, None);
        assert_eq!(request.source, self.subject);
        self.events.push(json!({"kind": "damage",
            "strength": request.packet.strength as u16, "channel": request.packet.channel,
            "source": NATIVE_HANDLE, "delta": request.microseconds as u32,
            "period": request.period_microseconds as u32}));
        Ok(())
    }
    fn player_notice(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        source: ObjectId,
        distance_squared: u32,
    ) -> Result<(), Self::Error> {
        assert_eq!(arena.actor(source).unwrap().handle, self.subject);
        self.events.push(
            json!({"kind": "notice", "squared": distance_squared, "code": 0x17, "argument": 0}),
        );
        Ok(())
    }
    fn create_child(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        resource: u32,
        position: [u16; 3],
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind": "child", "resource": resource, "position": position}));
        Ok(())
    }
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error> {
        let model = arena.actor(actor).unwrap().rotating.visual.model_word;
        self.events
            .push(json!({"kind": "refresh_model", "model": model}));
        Ok(())
    }
    fn retaliation(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
    ) -> Result<(), Self::Error> {
        panic!("retarget 004015F0 is outside segment A")
    }
    fn queue_shot(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        microseconds: u32,
        direction_bits: [u32; 3],
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!({"kind": "shot", "delta": microseconds, "fraction": 0,
            "direction": direction_bits}));
        Ok(())
    }
}

/// Builds the receipt's actor, behavior, kind-8 target, team task and player.
fn prepare(case: &Value) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    let actor = arena
        .allocate_selector11_actor(construction(words(&case["position"])), &mut 0)
        .unwrap();
    if case["behavior"].as_bool().unwrap_or(true) {
        arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
        arena.admit_derived_actor(actor).unwrap();
        let behavior = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.word_04 = get(case, "inaccuracy", 0) as u32;
        behavior.word_08 = 0;
        [
            behavior.byte_0c,
            behavior.byte_0d,
            behavior.byte_0e,
            behavior.byte_0f,
            behavior.byte_10,
            behavior.byte_11,
        ] = [get(case, "lead", 1) as u8, 0, 1, 0, 0, 0];
        behavior.table_14 = ActorBehaviorTable::Mode01;
        behavior.entries_18 = vec![case["entry"]
            .as_array()
            .map_or([5, 0, 0, 0, 0, 0, 0, 0], |entry| {
                std::array::from_fn(|index| entry[index].as_u64().unwrap() as u32)
            })];
        let group: Vec<u32> = case["group"].as_array().map_or(vec![0; 7], |group| {
            group
                .iter()
                .map(|word| word.as_u64().unwrap() as u32)
                .collect()
        });
        behavior.groups_1c = vec![ActorBehaviorGroup {
            coordinates_00_0a: std::array::from_fn(|index| Some(group[index] as u16)),
            word_0c: group[6],
        }];
    }
    if case["target"].as_bool().unwrap_or(true) {
        let target = NativeTarget::FixedPosition(FixedPositionTarget::from_point(
            ActorHandle::from_raw(get(case, "target_handle", 0x99) as u32),
            Some(words(&case["focus"])),
        ));
        let reference = arena.import_fixture_target(target);
        arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    }
    arena.owned_mut(actor).unwrap().state.base.word_28 = get(case, "size", 0) as u16;
    let basis: Vec<f64> = case["basis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();
    let state = arena.actor_mut(actor).unwrap();
    state.rotating.basis = NativeBasis {
        words: std::array::from_fn(|index| match index {
            0 => get(case, "counter", 0) as u32,
            _ => (basis[index - 1] as f32).to_bits(),
        }),
    };
    state.rotating.visual.model_word = 0x30;
    state.word_74 = 0x77;
    state.word_b4 = get(case, "team", 2) as u32;
    state.word_c4 = get(case, "state", 3) as u32;
    state.word_c8 = 0;
    state.word_cc = 0;
    state.word_ce = 0;
    state.words_8e_92 = [0; 3];
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.word_144 = get(case, "delay", 0) as i32;
    derived.word_164 = get(case, "range", 2500) as i32;
    derived.word_17a = get(case, "timer_17a", 0) as u16;
    let state = arena.selector11_mut(actor).unwrap();
    state.word_188 = get(case, "pitch", 0) as u32;
    state.word_18c = get(case, "yaw", 0) as u32;
    state.word_190 = get(case, "timer_190", 0x7fff_ffff) as u32;
    state.word_194 = get(case, "word_194", 1000) as u32;
    state.word_19c = Some(get(case, "word_19c", 0) as u32);
    state.word_1aa = Some(get(case, "angle_1aa", 0) as u16);
    state.word_1ac = Some(get(case, "angle_1ac", 0) as u16);
    state.word_1ae = get(case, "flags_1ae", 0) as u16;
    state.word_1b0 = get(case, "timer_1b0", 0) as u16;
    state.word_1b2 = get(case, "death", 0) as u16;
    // 00479F20 is a recorded native boundary; the port executes it, so the
    // node starts in the flag-1 list to make that execution observable.
    arena.set_flag1(actor, true).unwrap();
    arena.owned_mut(actor).unwrap().state.base.dirty_word = 0;
    if case["player"].is_array() {
        let primary = arena
            .allocate_derived_actor(construction(words(&case["player"])), &mut 0)
            .unwrap();
        let player = arena.add_world_player([0; 6]);
        let handle = arena.actor(primary).unwrap().handle;
        arena.world_player_mut(player).unwrap().handle_d4 = handle;
    }
    (arena, actor)
}

/// 004019E0 over 68 supplied actors: aim, lead, gravity, inaccuracy and
/// wobble arithmetic, both pitch clamps, every firing gate, the timers, the
/// delay and water prefixes, all three death branches and the player
/// proximity pair. One-second steps leave the turns unclamped, so their shots
/// expose the exact aim vector.
#[test]
fn selector11_update_matches_native_segment_a() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector11_update_native.json"
    ))
    .unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let raw = executable.mapped(0x53ff00, 2048).unwrap().bytes;
    let inverse: [i16; 1024] =
        std::array::from_fn(|index| i16::from_le_bytes([raw[index * 2], raw[index * 2 + 1]]));
    let cases = fixture["cases"].as_array().unwrap();
    let mut shots = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let (mut arena, actor) = prepare(case);
        let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
        let speed = (get(case, "speed", 400) as u16).to_le_bytes();
        tables
            .write_process_bytes(
                0x530e60,
                &[Some(2), Some(0), Some(speed[0]), Some(speed[1])],
            )
            .unwrap();
        tables
            .write_process_bytes(
                0x53dbb4 + 32,
                &(get(case, "gravity", 0) as u32).to_le_bytes().map(Some),
            )
            .unwrap();
        let mut tasks = ProcessTaskRuntime::default();
        tasks
            .construct(
                get(case, "team", 2) as u32,
                ActorHandle::from_raw(get(case, "task_source", 0x55) as u32),
            )
            .unwrap();
        let mut host = Host {
            tables,
            tasks,
            rng: case["rng"].as_u64().unwrap() as u32,
            subject: arena.actor(actor).unwrap().handle,
            events: Vec::new(),
        };
        let velocity = case["velocity"]
            .is_array()
            .then(|| words(&case["velocity"]))
            .unwrap_or([0; 3]);
        let request = Selector11TickRequest {
            microseconds: get(case, "delta_us", 33333) as u32,
            status_load_factor: Some(get(case, "load_factor", 0x10000) as i32),
            water_plane: ActorWaterPlane {
                height_fixed: get(case, "plane", -0x100_0000) as i32,
                coordinate_shift: get(case, "shift", 8) as u32,
            },
            sine_quarter: &sine,
            inverse_sine: &inverse,
            settings_7c: 0,
            process_velocity: Some(velocity),
        };
        arena
            .tick_selector11(actor, &request, &mut host)
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        let native: Vec<&Value> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] != "flag1")
            .collect();
        assert_eq!(host.events.iter().collect::<Vec<_>>(), native, "{name}");
        shots += native
            .iter()
            .filter(|event| event["kind"] == "shot")
            .count();
        let cleared = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| *event == json!({"kind": "flag1", "value": 0}));
        let after = &case["after"];
        let flags = arena.object(actor).unwrap().base.flags;
        assert_eq!(flags & 2 == 0, cleared, "{name} flag 1");
        // An executed 00479F20 marks dirty bit 2 when the flag changes.
        let dirty = after["dirty"].as_u64().unwrap() as u16 | if cleared { 2 } else { 0 };
        assert_eq!(
            arena.object(actor).unwrap().base.dirty_word,
            dirty,
            "{name}"
        );

        let state = arena.selector11_actor(actor).unwrap();
        let derived = arena.derived_actor(actor).unwrap();
        let object = arena.actor(actor).unwrap();
        let group = derived
            .behavior_140
            .as_ref()
            .map_or(vec![0; 6], |behavior| {
                behavior.groups_1c[0]
                    .coordinates_00_0a
                    .iter()
                    .map(|word| word.unwrap())
                    .collect()
            });
        let port = json!({
            "word_144": derived.word_144 as u32,
            "word_17a": derived.word_17a,
            "word_188": state.word_188,
            "word_18c": state.word_18c,
            "word_190": state.word_190,
            "word_194": state.word_194,
            "word_19c": state.word_19c.unwrap(),
            "word_1aa": state.word_1aa.unwrap(),
            "word_1ac": state.word_1ac.unwrap(),
            "word_1ae": state.word_1ae,
            "word_1b0": state.word_1b0,
            "word_1b2": state.word_1b2,
            "word_74": object.word_74,
            "word_c8": object.word_c8,
            "model": object.rotating.visual.model_word,
            "dirty": after["dirty"],
            "basis": object.rotating.basis.words,
            "rng": host.rng,
            "group": group,
        });
        assert_eq!(&port, after, "{name}");
    }
    assert_eq!(cases.len(), 68);
    assert_eq!(
        shots,
        fixture["coverage"]["shots"].as_u64().unwrap() as usize
    );
}

/// Selector11WobbleAngleApproximation: a turret whose wobble angles were
/// never written wobbles as one whose angles hold 0. The native case
/// `wobble-lead-skipped` starts its wobble from 0 and 0; run with the angles
/// unwritten, it raises the same events and ends in the same state, where
/// the read used to stop the update for good.
#[test]
fn an_unwritten_wobble_starts_from_zero_angles() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector11_update_native.json"
    ))
    .unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let raw = executable.mapped(0x53ff00, 2048).unwrap().bytes;
    let inverse: [i16; 1024] =
        std::array::from_fn(|index| i16::from_le_bytes([raw[index * 2], raw[index * 2 + 1]]));
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "wobble-lead-skipped")
        .unwrap();
    assert_eq!(
        (get(case, "angle_1aa", 0), get(case, "angle_1ac", 0)),
        (0, 0)
    );
    let run = |unwritten: bool| {
        let (mut arena, actor) = prepare(case);
        if unwritten {
            let state = arena.selector11_mut(actor).unwrap();
            state.word_1aa = None;
            state.word_1ac = None;
        }
        let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
        let speed = (get(case, "speed", 400) as u16).to_le_bytes();
        tables
            .write_process_bytes(
                0x530e60,
                &[Some(2), Some(0), Some(speed[0]), Some(speed[1])],
            )
            .unwrap();
        tables
            .write_process_bytes(
                0x53dbb4 + 32,
                &(get(case, "gravity", 0) as u32).to_le_bytes().map(Some),
            )
            .unwrap();
        let mut tasks = ProcessTaskRuntime::default();
        tasks
            .construct(
                get(case, "team", 2) as u32,
                ActorHandle::from_raw(get(case, "task_source", 0x55) as u32),
            )
            .unwrap();
        let mut host = Host {
            tables,
            tasks,
            rng: case["rng"].as_u64().unwrap() as u32,
            subject: arena.actor(actor).unwrap().handle,
            events: Vec::new(),
        };
        let velocity = case["velocity"]
            .is_array()
            .then(|| words(&case["velocity"]))
            .unwrap_or([0; 3]);
        let request = Selector11TickRequest {
            microseconds: get(case, "delta_us", 33333) as u32,
            status_load_factor: Some(get(case, "load_factor", 0x10000) as i32),
            water_plane: ActorWaterPlane {
                height_fixed: get(case, "plane", -0x100_0000) as i32,
                coordinate_shift: get(case, "shift", 8) as u32,
            },
            sine_quarter: &sine,
            inverse_sine: &inverse,
            settings_7c: 0,
            process_velocity: Some(velocity),
        };
        arena.tick_selector11(actor, &request, &mut host).unwrap();
        let state = arena.selector11_actor(actor).unwrap();
        (
            host.events,
            host.rng,
            state.word_1aa,
            state.word_1ac,
            state.word_188,
            state.word_18c,
        )
    };
    let written = run(false);
    assert!(written.2.is_some_and(|angle| angle != 0), "{written:?}");
    assert_eq!(run(true), written);
}

/// 00402630: another machine's turret hurts the first player's vehicle
/// within 500 (00401920), then drifts on its velocity as the shared
/// 0047EBF0 moves any remote actor.
#[test]
fn another_machines_turret_hurts_a_near_player_then_drifts() {
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let case = json!({
        "position": [1000, 0, 1000],
        "player": [1100, 0, 1000],
        "basis": [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        "behavior": false,
        "target": false,
    });
    let run = |whole: bool| {
        let (mut arena, actor) = prepare(&case);
        arena.actor_mut(actor).unwrap().words_8e_92 = [0x40, 0, 0];
        let mut host = Host {
            tables: ProcessActorBehaviorTables::from_executable(executable).unwrap(),
            tasks: ProcessTaskRuntime::default(),
            rng: 1,
            subject: arena.actor(actor).unwrap().handle,
            events: Vec::new(),
        };
        if whole {
            arena
                .tick_remote_selector11(actor, 40_000, &sine, Some(0x100), &mut host)
                .unwrap();
        } else {
            arena
                .tick_remote_actor(
                    actor,
                    40_000,
                    &sine,
                    Some(0x100),
                    &mut crate::object_runtime::actor::common_update::FamilyRemoteHost(&mut host),
                )
                .unwrap();
        }
        let object = arena.object(actor).unwrap();
        (
            object.base.position_words,
            format!("{object:?}"),
            host.events,
        )
    };
    let (position, object, events) = run(true);
    let (_, shared, none) = run(false);
    assert_eq!(object, shared);
    assert!(none.is_empty());
    assert_ne!(position, Some([1000, 0, 1000]));
    assert_eq!(
        events,
        [
            json!({"kind": "damage", "strength": 4000, "channel": 3, "source": NATIVE_HANDLE,
                "delta": 40_000, "period": 2_000_000}),
            json!({"kind": "notice", "squared": 10_000, "code": 0x17, "argument": 0}),
        ]
    );
}
