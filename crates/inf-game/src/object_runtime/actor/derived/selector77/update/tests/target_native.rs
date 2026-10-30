//! Complete native controls and wrapper prefixes. Only directB4 and48D0D0
//! return segments are supplied; their independent production owners are not
//! replaced by fixture verdicts in the actual runtime.
use super::*;
use crate::object_runtime::{
    ActorBehaviorGroup, ActorBehaviorTable, ActorWeaponFireContext, FollowTargetPoint,
    FollowTargetRequest,
};
use inf_formats::{native_basis::NativeBasis, FrontendExecutable};
use serde_json::{json, Value};

struct NativeHost<'a> {
    tables: ProcessActorBehaviorTables,
    rng: u32,
    sine: &'a [u16; 4096],
    events: Vec<Value>,
    stop: Option<&'a str>,
    execute_queue: bool,
}
impl FamilyUpdateHost for NativeHost<'_> {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(NULL)
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        NativeFamilyUpdateHost {
            process_rng: &mut self.rng,
            sine_quarter: self.sine,
        }
        .effect(arena, actor, effect)
        .map_err(|_| "unexpected native status/weapon boundary")
    }
}
impl Selector77UpdateHost for NativeHost<'_> {
    fn behavior_tables(&self) -> Option<&ProcessActorBehaviorTables> {
        Some(&self.tables)
    }
    fn virtual_b4(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Selector77UpdateError<Self::Error>> {
        self.events.push(json!({"kind":"damage","amount":callback.adjusted_damage as u32,"strength":callback.packet.strength as u16,"channel":callback.packet.channel,"source":callback.source.raw()}));
        assert_eq!(callback.packet.byte_03, None);
        if self.stop == Some("damage") {
            Err(Selector77UpdateError::Service {
                address: 0x407cd2,
                error: "damage",
            })
        } else {
            Ok(())
        }
    }
    fn queue_direction(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        shot: Selector77Shot,
    ) -> Result<(), Selector77UpdateError<Self::Error>> {
        self.events.push(json!({"kind":"queue","delta":shot.microseconds,"direction":shot.direction_bits,"fraction":shot.speed_scale}));
        if self.stop == Some("queue") {
            Err(Selector77UpdateError::Service {
                address: 0x407e45,
                error: "queue",
            })
        } else if self.execute_queue {
            arena.queue_actor_weapon_direction(
                actor,
                shot.microseconds,
                shot.direction_bits,
                shot.speed_scale,
                &mut ActorWeaponFireContext {
                    tables: &mut self.tables,
                    rng: &mut self.rng,
                    process_velocity: None,
                },
            )?;
            Ok(())
        } else {
            Ok(())
        }
    }
}
fn unsigned(input: &Value, key: &str) -> u32 {
    input[key].as_i64().unwrap() as u32
}
fn words<const N: usize>(value: &Value) -> [u16; N] {
    std::array::from_fn(|axis| value[axis].as_i64().unwrap() as u16)
}

fn prepare(input: &Value) -> (ObjectArena, ObjectId) {
    let (mut arena, actor) = subject();
    if input["behavior"] == true {
        arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
        arena.admit_derived_actor(actor).unwrap();
        let b = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        b.table_14 = ActorBehaviorTable::Mode01;
        b.byte_0c = unsigned(input, "predictive") as u8;
        b.word_04 = unsigned(input, "spread");
        b.byte_0d = 0;
        b.byte_0e = 1;
        b.byte_0f = 0;
        b.byte_10 = 0;
        b.byte_11 = 0;
        b.groups_1c.clear();
        b.entries_18 = vec![std::array::from_fn(|i| {
            input["entry"][i].as_u64().unwrap() as u32
        })];
    }
    let target = NativeTarget::FixedPosition(
        arena
            .fixed_position_target(Some(words(&input["focus"])))
            .unwrap(),
    );
    let reference = arena.import_fixture_target(target);
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(reference);
    arena.derived_actor_mut(actor).unwrap().word_14c = Some(0xcccccccc);
    let a = arena.actor_mut(actor).unwrap();
    a.word_c4 = unsigned(input, "lifecycle");
    a.word_c8 = 0;
    a.word_cc = 0;
    a.rotating.basis = NativeBasis {
        words: std::array::from_fn(|i| input["basis"][i].as_u64().unwrap() as u32),
    };
    let eye = arena.selector77_update_state_mut(actor).unwrap();
    eye.words_184_190 = words(&input["pose"]);
    eye.position_192 = Some(words(&input["origin"]));
    eye.word_198 = unsigned(input, "timer");
    eye.word_19c = Some(unsigned(input, "phase") as u16);
    eye.word_1a0 = Some(0x12345678);
    eye.word_1a4 = unsigned(input, "remaining");
    eye.word_1a8 = unsigned(input, "speed");
    eye.word_1ac = Some(unsigned(input, "limit"));
    eye.handle_1b0 = ActorHandle::from_raw(unsigned(input, "damage_source"));
    (arena, actor)
}

#[test]
fn selector77_target_controls_and_wrapper_match_38_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../../tests/fixtures/selector77_target_native.json"
    ))
    .unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let raw = exe.mapped(0x53ff00, 2048).unwrap().bytes;
    let inverse: [i16; 1024] =
        std::array::from_fn(|i| i16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]));
    for (mode, index, case) in ["controls", "updates"].into_iter().flat_map(|mode| {
        fixture[mode]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(move |(i, c)| (mode, i, c))
    }) {
        let input = &case["input"];
        let (mut arena, actor) = prepare(input);
        let mut tables = ProcessActorBehaviorTables::from_executable(exe).unwrap();
        let speed = (unsigned(input, "projectile_speed") as u16).to_le_bytes();
        tables
            .write_process_bytes(
                0x530e60,
                &[Some(2), Some(0), Some(speed[0]), Some(speed[1])],
            )
            .unwrap();
        tables
            .write_process_bytes(
                0x53dbb4 + 32,
                &unsigned(input, "gravity_flag").to_le_bytes().map(Some),
            )
            .unwrap();
        let mut host = NativeHost {
            tables,
            rng: unsigned(input, "rng"),
            sine: &sine,
            events: Vec::new(),
            stop: input["stop"].as_str(),
            execute_queue: false,
        };
        let request = Selector77TickRequest {
            microseconds: unsigned(input, "delta"),
            process_null_handle: Some(NULL),
            status_load_factor: None,
            sine_quarter: &sine,
            inverse_sine: &inverse,
            settings_7c: 0,
            process_velocity: Some(words(&input["velocity"])),
        };
        let result = if mode == "controls" {
            arena
                .selector77_target_controls(actor, &request, &mut host)
                .map(|(world, accepted)| {
                    assert_eq!(
                        json!(world.map(|v| v as u16)),
                        case["world"],
                        "{mode}{index}"
                    );
                    assert_eq!(
                        accepted,
                        case["accepted"].as_bool().unwrap(),
                        "{mode}{index}"
                    );
                })
        } else {
            arena.tick_selector77(actor, request, &mut host)
        };
        if case["division_trap"] == true {
            assert!(
                matches!(result, Err(Selector77UpdateError::Division(_))),
                "{mode}{index}: {result:?}"
            );
        } else if !case["stopped"].is_null() {
            assert!(
                matches!(result, Err(Selector77UpdateError::Service { .. })),
                "{mode}{index}: {result:?}"
            );
        } else {
            result.unwrap_or_else(|e| panic!("{mode}{index}: {e:?}"));
        }
        let eye = arena.selector77_actor(actor).unwrap();
        let entry = arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .map(|b| json!(b.entries_18[0]))
            .unwrap_or_else(|| input["entry"].clone());
        let output = json!({"distance":arena.derived_actor(actor).unwrap().word_14c.unwrap(),"pose":eye.words_184_190,"phase":eye.word_19c.unwrap(),"remaining":eye.word_1a4,"speed":eye.word_1a8,"limit":eye.word_1ac.unwrap(),"timer":eye.word_198,"entry":entry,"rng":host.rng});
        assert_eq!(output, case["output"], "{mode}{index}");
        assert_eq!(json!(host.events), case["events"], "{mode}{index}");
    }
}

#[test]
fn selector77_live_follow_refresh_reaches_real_weapon_queue_with_same_rng() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../../../tests/fixtures/selector77_target_native.json"
    ))
    .unwrap();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let raw = exe.mapped(0x53ff00, 2048).unwrap().bytes;
    let inverse = std::array::from_fn(|i| i16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]));
    for settings_7c in [0, 1] {
        let mut input = fixture["updates"][0]["input"].clone();
        input["behavior"] = json!(true);
        input["entry"] = json!([1, 0, 0, 0, 0, 0, 0, 0]);
        let (mut arena, actor) = prepare(&input);
        let followed = primary(&mut arena, Some([10, 20, 30]));
        let handle = arena.actor(followed).unwrap().handle;
        let target = arena
            .follow_target_resource(FollowTargetRequest {
                handle,
                point: FollowTargetPoint::ResolveActor,
            })
            .unwrap()
            .unwrap();
        let captured_point = arena.actor_follow_target(target).unwrap().focus_14;
        let old = arena
            .derived_actor_mut(actor)
            .unwrap()
            .target_148
            .replace(target)
            .unwrap();
        arena.release_actor_service(old).unwrap();
        arena.owned_mut(followed).unwrap().state.base.position_words = Some([100, 100, 2300]);
        arena.actor_mut(followed).unwrap().word_74 = 100;
        arena.actor_mut(followed).unwrap().word_c4 = 3;
        arena.actor_mut(followed).unwrap().words_8e_92 = [12, 34, 56];
        let flags = arena.object(followed).unwrap().base.flags | 0x0004_0000;
        arena.apply_payload_flags(followed, flags).unwrap();
        let behavior = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.byte_0d = 1;
        behavior.word_08 = 0;
        behavior.groups_1c = vec![ActorBehaviorGroup {
            coordinates_00_0a: [None; 6],
            word_0c: 2,
        }];
        let mut tables = ProcessActorBehaviorTables::from_executable(exe).unwrap();
        for (offset, bytes) in [
            (6, 1_u16.to_le_bytes().to_vec()),
            (8, 40_000_u32.to_le_bytes().to_vec()),
            (12, 50_000_u32.to_le_bytes().to_vec()),
            (16, 60_000_u32.to_le_bytes().to_vec()),
            (28, 7_u32.to_le_bytes().to_vec()),
        ] {
            tables
                .write_process_bytes(
                    0x530e60 + offset,
                    &bytes.into_iter().map(Some).collect::<Vec<_>>(),
                )
                .unwrap();
        }
        let mut host = NativeHost {
            tables,
            rng: unsigned(&input, "rng"),
            sine: &sine,
            events: Vec::new(),
            stop: None,
            execute_queue: true,
        };
        let mut expected_rng = host.rng;
        let fraction = ((name_entry_rng_next(&mut expected_rng) & 0xffff) >> 1) + 0xc000;
        arena
            .tick_selector77(
                actor,
                Selector77TickRequest {
                    microseconds: 33_333,
                    process_null_handle: Some(NULL),
                    status_load_factor: None,
                    sine_quarter: &sine,
                    inverse_sine: &inverse,
                    settings_7c,
                    process_velocity: None,
                },
                &mut host,
            )
            .unwrap();
        let refreshed = arena.actor_follow_target(target).unwrap();
        assert_eq!(refreshed.focus_08, Some([100, 200, 2300]));
        assert_eq!(
            refreshed.velocity_1a,
            Some([12_u16, 34, 56].map(u16::wrapping_neg))
        );
        assert_eq!(
            refreshed.focus_14,
            if settings_7c == 0 {
                captured_point
            } else {
                refreshed.focus_08
            }
        );
        assert_eq!(
            arena.selector77_actor(actor).unwrap().word_198,
            3_000_000 - 33_333
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, Some(2000));
        let behavior = arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        let node = arena
            .actor_service_heap()
            .weapon_fire_node(behavior.fire_head_00.unwrap())
            .unwrap();
        assert_eq!(node.direction_bits, [0, 0, 1_f32.to_bits()]);
        assert_eq!(node.speed_scale, fraction);
        assert_eq!((node.compensation, node.group, node.next), (0, 0, None));
        assert_eq!(behavior.entries_18[0], [0, 50_000, 0, 0, 0, 100_000, 0, 7]);
        assert_eq!(behavior.groups_1c[0].word_0c, 4);
        assert_ne!(arena.object(actor).unwrap().base.flags & 0x0100_0000, 0);
        assert_eq!(host.rng, expected_rng);
        assert_eq!(host.events.len(), 1);
    }
}
