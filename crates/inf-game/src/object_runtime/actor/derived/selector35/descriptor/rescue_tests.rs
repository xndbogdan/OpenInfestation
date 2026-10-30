//! Selector 35's rescue against the original under Unicorn
//! (`check_selector35_rescue.py`): the Rescue and Go to
//! space scores, entries and updates, and the transport's pickup `+A0`. The
//! allocator, `00450A30` and `0047AD90` are supplied there; the grants,
//! disposals, placement records and slot flashes are recorded and compared
//! with the port's.

use std::sync::Arc;

use inf_formats::ByteSpan;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::factory_shop::FactoryShopTables;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainHost, ActorTerrainInputs, DerivedActorConstructionRequest, FactoryStaffHost,
    FamilyDescriptorScoreKind, FamilyDescriptorScoreRequest, FamilyDescriptorScoreRng,
    FixedPositionTarget, FixtureActorServiceAllocation, LiveGrantContext, NativeHeapRef,
    NativeTarget, ObjectArena, ObjectClassKind, ObjectId, ObjectList, ObjectPositionTarget,
    RuntimeAllocation, Selector35DescriptorContext, Selector35DescriptorRequest,
    Selector35RouteProcessState, WeightedTargetConstructorNull,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::terrain_runtime::TerrainRuntime;

const SINE: [u16; 4096] = [0; 4096];
const FIELDS: [u16; 14] = [
    0x144, 0x14c, 0x184, 0x18c, 0x190, 0x194, 0x208, 0x228, 0x230, 0x234, 0x244, 0x258, 0x25c,
    0x260,
];

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}

fn words(v: &Value) -> [u16; 3] {
    std::array::from_fn(|n| u(&v[n]) as u16)
}

fn request<'a>() -> DerivedActorConstructionRequest<'a> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3; 2],
    }
}

fn set_word(arena: &mut ObjectArena, actor: ObjectId, at: u16, value: u32) {
    match at {
        0x144 => arena.derived_actor_mut(actor).unwrap().word_144 = value as i32,
        0x14c => arena.derived_actor_mut(actor).unwrap().word_14c = Some(value),
        0x184 => arena.family428650_actor_mut(actor).unwrap().word_184 = value as i32,
        0x18c..=0x194 => {
            arena.family428650_actor_mut(actor).unwrap().words_188_19c
                [usize::from((at - 0x188) / 4)] = Some(value)
        }
        0x208 => arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(value),
        0x228 => arena.family428650_actor_mut(actor).unwrap().word_228 = value,
        0x230 => arena.selector35_actor_mut(actor).unwrap().word_230 = value,
        0x234 => arena.selector35_actor_mut(actor).unwrap().word_234 = value,
        0x244 => arena.selector35_actor_mut(actor).unwrap().word_244 = value,
        0x258 => arena.selector35_actor_mut(actor).unwrap().word_258 = Some(value),
        0x25c => arena.selector35_actor_mut(actor).unwrap().word_25c = Some(value),
        0x260 => arena.selector35_actor_mut(actor).unwrap().word_260 = Some(value),
        _ => panic!("unexpected field {at:x}"),
    }
}

fn get_word(arena: &ObjectArena, actor: ObjectId, at: u16) -> u32 {
    let family = arena.family428650_actor(actor).unwrap();
    let state = arena.selector35_actor(actor).unwrap();
    let derived = arena.derived_actor(actor).unwrap();
    match at {
        0x144 => derived.word_144 as u32,
        0x14c => derived.word_14c.unwrap(),
        0x184 => family.word_184 as u32,
        0x18c..=0x194 => family.words_188_19c[usize::from((at - 0x188) / 4)].unwrap(),
        0x208 => family.words_204_208[1].unwrap(),
        0x228 => family.word_228,
        0x230 => state.word_230,
        0x234 => state.word_234,
        0x244 => state.word_244,
        0x258 => state.word_258.unwrap(),
        0x25c => state.word_25c.unwrap(),
        0x260 => state.word_260.unwrap(),
        _ => panic!("unexpected field {at:x}"),
    }
}

fn target_value(arena: &ObjectArena, reference: Option<NativeHeapRef>) -> Value {
    reference.map_or(Value::Null, |reference| {
        let target = arena.native_target(reference).unwrap();
        json!({"handle": target.handle_04().raw(),
               "focus": target.raw_focus_08_words().unwrap(),
               "kind": target.kind_word()})
    })
}

/// A whole grid at one height, so 00450A30 answers it anywhere.
struct TerrainHost {
    terrain: TerrainRuntime,
    calls: usize,
}

impl ActorTerrainHost for TerrainHost {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        Ok(&self.terrain)
    }
}

fn terrain(height: i8) -> TerrainHost {
    use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
    TerrainHost {
        terrain: TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
            0,
            std::iter::repeat(u16::from(height as u8)).take(512 * 512),
        ),
        calls: 0,
    }
}

/// The pickup's placement record and slot flash.
struct Staff {
    process: ParticleFactoryProcessState,
    events: Vec<Value>,
}

impl FactoryStaffHost for Staff {
    type Error = String;
    fn process(&self) -> &ParticleFactoryProcessState {
        &self.process
    }
    fn grants(&self) -> LiveGrantContext<'_> {
        LiveGrantContext {
            process: Some(&self.process),
            executable: None,
            resident: None,
            text_data: None,
        }
    }
    fn shop_tables(&self) -> Result<&FactoryShopTables, String> {
        Err("the pickup reads no shop".into())
    }
    fn flash(&mut self, _: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        self.events
            .push(json!({"kind": "flash", "selector": 2, "position": position, "flag": 0}));
        Ok(())
    }
    fn complete_placement(&mut self, index: i8) -> Result<(), String> {
        self.events
            .push(json!({"kind": "record", "mission": true, "index": index, "flag": 0}));
        Ok(())
    }
}

#[test]
fn selector35_rescue_matches_the_original() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_rescue_native.json"
    )))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let raw = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 155);
    for (index, case) in cases.iter().enumerate() {
        let i = &case["input"];
        let out = &case["output"];
        let mode = i["mode"].as_str().unwrap();
        let label = format!("case {index} {mode}");
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 4,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let ship = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
        let base = &mut arena.owned_mut(ship).unwrap().state.base;
        base.position_words = Some(words(&i["position"]));
        base.flags = u(&i["flags2c"]);
        base.dirty_word = u(&i["dirty38"]) as u16;
        let actor = arena.actor_mut(ship).unwrap();
        actor.word_c4 = u(&i["c4"]);
        actor.byte_78 = u(&i["byte78"]) as u8;
        actor.word_7a = u(&i["word7a"]) as u16;
        let basis: Vec<u32> = i["basis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| (value.as_f64().unwrap() as f32).to_bits())
            .collect();
        actor.rotating.basis.words[1..].copy_from_slice(&basis);
        arena.family428650_actor_mut(ship).unwrap().word_22e = u(&i["word22e"]) as u16;
        let bytes: Vec<u8> = i["bytes240"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| u(byte) as u8)
            .collect();
        let state = arena.selector35_actor_mut(ship).unwrap();
        state.bytes_240 = std::array::from_fn(|n| Some(bytes[n]));
        state.byte_238 = u(&i["byte238"]) as u8;
        for at in FIELDS {
            set_word(&mut arena, ship, at, u(&i["words"][format!("0x{at:x}")]));
        }
        // +23C with the coefficient at +12.
        let mut retained = vec![0_u8; 32];
        retained[0x12..0x14].copy_from_slice(&(u(&i["coefficient"]) as u16).to_le_bytes());
        let allocation =
            Arc::new(RuntimeAllocation::new(retained.into(), ByteSpan::new(0, 32)).unwrap());
        arena.set_selector35_retained(ship, &allocation, 0).unwrap();
        let current = match i["current_target"].as_str().unwrap() {
            "none" => None,
            kind => {
                let focus = words(&i["current_focus"]);
                let handle = ActorHandle::from_raw(9);
                let target = if kind == "point" {
                    NativeTarget::FixedPosition(FixedPositionTarget::from_point(
                        handle,
                        Some(focus),
                    ))
                } else {
                    let route = arena.allocate(ObjectClassKind::RouteNode);
                    NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(
                        handle, focus, route,
                    ))
                };
                Some(arena.import_fixture_target(target))
            }
        };
        arena.derived_actor_mut(ship).unwrap().target_148 = current;
        // The players; the first one's unit through +D4.
        let players: Vec<_> = (0..u(&i["players"]))
            .map(|_| arena.add_world_player([0; inf_formats::GRANT_MASK_WORDS]))
            .collect();
        if let (Some(first), Some(position)) = (players.first(), i["unit"].as_array()) {
            let unit = arena.allocate_selector35_actor(request(), &mut 0).unwrap();
            arena.owned_mut(unit).unwrap().state.base.position_words =
                Some(std::array::from_fn(|n| u(&position[n]) as u16));
            let handle = arena.actor(unit).unwrap().handle;
            arena.world_player_mut(*first).unwrap().handle_d4 = handle;
        }
        let partner = if i["partner"] == "scientist" {
            arena.allocate_selector12_actor(
                request(),
                &mut 0,
                crate::object_runtime::Selector12GrantContext {
                    table: &grants,
                    first_player_grants: None,
                },
            )
        } else {
            arena.allocate_selector35_actor(request(), &mut 0)
        }
        .unwrap();
        arena.owned_mut(partner).unwrap().state.base.position_words =
            Some(words(&i["partner_position"]));
        if !i["alloc"].as_bool().unwrap() {
            arena
                .actor_service_heap_mut()
                .script([FixtureActorServiceAllocation::Null]);
        }
        let attempts = arena.actor_service_heap().attempts().len();
        // A networked process grants without a notice; 004750C0 is
        // recorded in the harness.
        let process = ParticleFactoryProcessState {
            network_flag: 1,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: None,
        };
        let mut terrain = terrain((i["terrain_height"].as_i64().unwrap() / 32) as i8);
        let mut staff = Staff {
            process,
            events: Vec::new(),
        };
        let delta = u(&i["delta"]);
        let entering = i["entering"].as_bool().unwrap();
        let mut eax = Value::Null;
        match mode {
            "score_rescue" | "score_space" => {
                let score = arena
                    .score_family_descriptor_in_process(
                        ship,
                        FamilyDescriptorScoreRequest {
                            kind: if mode == "score_rescue" {
                                FamilyDescriptorScoreKind::Source43d730
                            } else {
                                FamilyDescriptorScoreKind::Source43d8f0
                            },
                            current: i["current"].as_bool().unwrap(),
                            bias: i["bias"].as_i64().unwrap() as i32,
                        },
                        FamilyDescriptorScoreRng::RequiredAtReach,
                        None,
                        Some(u(&i["clock"])),
                    )
                    .unwrap_or_else(|error| panic!("{label}: {error:?}"));
                eax = json!(score as u32);
            }
            "pickup" => {
                let took = arena
                    .selector35_picks_up(ship, partner, &mut staff)
                    .unwrap_or_else(|error| panic!("{label}: {error:?}"));
                eax = json!(u8::from(took));
            }
            _ => {
                let request = match mode {
                    "change_rescue" => Selector35DescriptorRequest::ChangeRescue { entering },
                    "update_rescue" => Selector35DescriptorRequest::UpdateRescue {
                        microseconds: delta,
                    },
                    "change_space" => Selector35DescriptorRequest::ChangeSpace { entering },
                    "update_space" => Selector35DescriptorRequest::UpdateSpace {
                        microseconds: delta,
                    },
                    other => panic!("{other}"),
                };
                let mut rng = 0;
                let mut route = Selector35RouteProcessState::default();
                let mut context = Selector35DescriptorContext {
                    rng: &mut rng,
                    process: &mut route,
                    constructor_null: WeightedTargetConstructorNull::Supplied(Some(
                        ActorHandle::from_raw(u(&i["null"])),
                    )),
                    settings_7c: 0,
                    // 004750C0's dependent grants read the executable.
                    live_grants: LiveGrantContext {
                        process: Some(&process),
                        executable: Some(executable),
                        resident: None,
                        text_data: None,
                    },
                };
                arena
                    .run_selector35_descriptor(ship, request, &mut context, &mut terrain)
                    .unwrap_or_else(|error| panic!("{label}: {error:?}"));
            }
        }
        // 00479FA0 queues disposal: the port's flags carry it, the
        // harness's recorded call does not.
        let events = case["events"].as_array().unwrap();
        let disposed = |object: &str| {
            events
                .iter()
                .any(|event| event["kind"] == "dispose" && event["object"] == object)
        };
        let lifecycle = |arena: &ObjectArena, object: ObjectId| {
            arena
                .list(ObjectList::Disposal)
                .iter()
                .any(|node| node.object == object)
        };
        assert_eq!(lifecycle(&arena, ship), disposed("ship"), "{label}");
        assert_eq!(lifecycle(&arena, partner), disposed("partner"), "{label}");
        let derived = arena.derived_actor(ship).unwrap();
        let family = arena.family428650_actor(ship).unwrap();
        let state = arena.selector35_actor(ship).unwrap();
        let base = &arena.object(ship).unwrap().base;
        let actual = json!({
            "words": FIELDS
                .into_iter()
                .map(|at| (format!("0x{at:x}"), json!(get_word(&arena, ship, at))))
                .collect::<serde_json::Map<_, _>>(),
            "bytes240": state.bytes_240.map(Option::unwrap),
            "byte238": state.byte_238,
            "word22e": family.word_22e,
            "flags2c": if disposed("ship") { out["flags2c"].clone() } else { json!(base.flags) },
            "dirty38": if disposed("ship") { out["dirty38"].clone() } else { json!(base.dirty_word) },
            "eax": eax,
            "current": target_value(&arena, derived.target_148),
            "current_is_original": derived.target_148.is_some() && derived.target_148 == current,
        });
        assert_eq!(&actual, out, "{label}");
        assert_eq!(
            arena.actor_service_heap().attempts().len() - attempts,
            events.iter().filter(|e| e["kind"] == "allocate").count(),
            "{label} allocations"
        );
        assert_eq!(
            terrain.calls,
            events.iter().filter(|e| e["kind"] == "terrain").count(),
            "{label} terrain"
        );
        let expected: Vec<&Value> = events
            .iter()
            .filter(|e| e["kind"] == "record" || e["kind"] == "flash")
            .collect();
        assert_eq!(
            staff.events.iter().collect::<Vec<_>>(),
            expected,
            "{label} staff"
        );
        // 004750C0 per player, in list order.
        for (n, player) in players.iter().enumerate() {
            let granted = events
                .iter()
                .any(|e| e["kind"] == "grant" && u(&e["player"]) as usize == n);
            let id = usize::from(u(&i["word7a"]) as u16);
            let mask = arena.world_player(*player).unwrap().grants_9c;
            assert_eq!(
                mask[id >> 5] & (1 << (id & 31)) != 0,
                granted,
                "{label} player {n}"
            );
        }
    }
}

#[test]
fn selector35_rescue_bodies_are_guarded() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_rescue_native.json"
    )))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        fixture["provenance"]["exe_sha256"].as_str().unwrap()
    );
    for guard in fixture["guards"].as_array().unwrap() {
        // .text's file offsets are its addresses less the image base.
        let at = (u(&guard["address"]) - 0x0040_0000) as usize;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[at..at + u(&guard["size"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
}
