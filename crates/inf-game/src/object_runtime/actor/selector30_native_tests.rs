//! Complete original selector-30 routines on supplied storage/resources, including
//! real H1/H12 world-start lookup and saved-bit after-load. No allocator claim.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use inf_formats::{native_basis::NativeBasisMath, GRANT_MASK_WORDS, HANDLER_COUNT};
use serde_json::{json, Value};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::native_tests as common,
        native_model_fixture::{Fixture, Models},
        BaseContext, RotatingVisualContext,
    },
    world_start::WorldStartState,
    Installation, ResidencyRequest,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Positions {
    root: PathBuf,
    runtime: ResidentRuntime,
}

impl Positions {
    fn new(words: &[u32]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-selector30-position-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            preload.extend_from_slice(
                &(if handler == 1 { words.len() as u16 } else { 0 }).to_le_bytes(),
            );
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for table in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let populated = table == 0 && handler == 1 && !words.is_empty();
                preload.extend_from_slice(
                    &(if populated { words.len() as i16 } else { 0 }).to_le_bytes(),
                );
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        let mut overlay = vec![];
        if !words.is_empty() {
            overlay.extend_from_slice(&((words.len() * 4) as u32).to_le_bytes());
            for word in words {
                overlay.extend_from_slice(&word.to_le_bytes());
            }
        }
        fs::write(root.join("Overlay/0.ovl"), overlay).unwrap();
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        Self { root, runtime }
    }
}

impl Drop for Positions {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("inf-selector30-position-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn snapshot(arena: &ObjectArena, objects: &BTreeMap<u32, ObjectId>) -> Value {
    let mut snapshot = common::snapshot(arena, objects);
    for value in snapshot["objects"].as_array_mut().unwrap() {
        let state = arena
            .selector30_actor(objects[&word(&value["id"])])
            .unwrap();
        let mid = &state.intermediate;
        assert_eq!((mid.word_158, state.position_16c), (None, None));
        value["selector30"] = json!({
            "byte_138":mid.byte_138,"word_13c":mid.word_13c,"word_140":mid.word_140,
            "word_144":mid.word_144,"words_146_14a":mid.words_146_14a,
            "byte_14c":mid.byte_14c,"byte_14d":mid.byte_14d,"byte_14e":mid.byte_14e,
            "word_150":mid.word_150,"sound_154":0,"word_15c":state.word_15c,
            "word_160":state.word_160,"word_164":state.word_164,"byte_166":state.byte_166,
            "byte_167":state.byte_167,"word_168":state.word_168,"word_174":state.word_174,
            "word_178":state.word_178,"word_17c":state.word_17c,"word_180":state.word_180,
            "handle_184":state.handle_184.raw(),"word_188":state.word_188,"word_18c":state.word_18c,
            "byte_18e":state.byte_18e,"byte_18f":state.byte_18f,"byte_190":state.byte_190,
            "byte_191":state.byte_191,"byte_192":state.byte_192,
        });
    }
    snapshot
}

#[test]
fn selector30_matches_complete_native_constructor_payload_commands_postinit_and_afterload() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector30_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 8);
    assert_eq!(fixture["coverage"]["snapshots"], 232);
    for address in ["0048B170", "0048B220", "0048B230", "0048B390", "0048C4F0"] {
        assert_eq!(fixture["coverage"]["direct_native_entries"][address], 8);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let terrain = tests::terrain(0);
    let positions = Positions::new(&[0]);
    let mut compared = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let resident = Fixture::new(&serde_json::from_value::<Models>(case["h8"].clone()).unwrap());
        let context = ActorCommandContext {
            terrain: None,
            runtime: &resident.runtime,
            first_player_grants: None,
        };
        let mut arena = tests::arena();
        let mut objects = BTreeMap::new();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            match step["op"].as_str().unwrap() {
                "construct" => {
                    let object = arena
                        .allocate_selector30_actor(tests::request(Some([0, 1, 0])))
                        .unwrap();
                    objects.insert(1, object);
                }
                "initialize" => {
                    let bytes: Vec<u8> = serde_json::from_value(step["payload"].clone()).unwrap();
                    let payload = tests::payload(&bytes);
                    arena
                        .initialize_selector30_actor(
                            objects[&1],
                            ActorInitializationRequest {
                                context: RotatingVisualContext {
                                    base: BaseContext {
                                        position_words: Some(
                                            serde_json::from_value(step["position"].clone())
                                                .unwrap(),
                                        ),
                                        placement_index: 123,
                                    },
                                    rotation_words: serde_json::from_value(
                                        step["rotation"].clone(),
                                    )
                                    .unwrap(),
                                    model_argument: Some(0xffff),
                                },
                                payload: &payload,
                                local_owner: 1,
                            },
                            &resident.runtime,
                            &math,
                        )
                        .unwrap();
                }
                "guard" => {} // Vtable +38 is the actual 00401370 constant-true body.
                "command" => {
                    let bytes: Vec<u8> = serde_json::from_value(step["bytes"].clone()).unwrap();
                    let code = step["code"].as_i64().unwrap() as i32;
                    assert_eq!(
                        arena
                            .apply_selector30_command(
                                objects[&1],
                                &tests::command(code, &bytes),
                                context
                            )
                            .unwrap(),
                        step["accepted"].as_bool().unwrap()
                    );
                }
                "post_initialize" => {
                    let mut grants = [0; GRANT_MASK_WORDS];
                    if let Some(bits) = step["grants"].as_array() {
                        for bit in bits {
                            let bit = word(bit);
                            grants[(bit / 32) as usize] |= 1 << (bit & 31);
                        }
                    }
                    let mut world = WorldStartState {
                        pair_index: word(&step["world_before"]["pair_index"]) as u16,
                        override_position: serde_json::from_value(
                            step["world_before"]["override_position"].clone(),
                        )
                        .unwrap(),
                        vehicle_selection: word(&step["world_before"]["vehicle_selection"]) as u8,
                        harness_spawn: None,
                        port_spawn: None,
                    };
                    arena
                        .finish_selector30_initialization(
                            objects[&1],
                            ActorCommandContext {
                                terrain: None,
                                runtime: &resident.runtime,
                                first_player_grants: (!step["grants"].is_null()).then_some(&grants),
                            },
                            &mut WorldStartContext {
                                state: &mut world,
                                runtime: &positions.runtime,
                                terrain: &terrain,
                            },
                        )
                        .unwrap();
                    assert_eq!(
                        json!({"pair_index":world.pair_index,"override_position":world.override_position,"vehicle_selection":world.vehicle_selection}),
                        step["world_after"]
                    );
                }
                "after_load" => {
                    arena.actor_mut(objects[&1]).unwrap().byte_78 =
                        step["byte_78"].as_i64().unwrap() as u8;
                    let bits = serde_json::from_value(step["completion"].clone()).unwrap();
                    arena
                        .initialize_selector30_after_load(objects[&1], &bits)
                        .unwrap();
                }
                "destroy" => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: objects[&1],
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                "network" => {
                    let base = &mut arena.owned_mut(objects[&1]).unwrap().state.base;
                    base.placement_word = word(&step["placement_word"]) as u16;
                    if step["remote"].as_bool().unwrap() {
                        base.flags |= 0x8000_0000;
                    }
                    crate::object_runtime::construction::actor_factory::initialize_network_actor(
                        &mut arena,
                        objects[&1],
                        step["world_active"].as_bool().unwrap(),
                    )
                    .unwrap();
                }
                op => panic!("unknown selector30 operation {op}"),
            }
            assert_eq!(
                snapshot(&arena, &objects),
                step["state"],
                "{} step{index}",
                case["id"]
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 232);
}

#[test]
fn world_start_uses_signed_doubled_h1_slot_and_raw_height_even_for_override() {
    let positions = Positions::new(&[0x1234_ffff, 0, 0xabcd_8000]);
    let terrain = tests::terrain(-3);
    let mut state = WorldStartState {
        pair_index: 1,
        override_position: None,
        vehicle_selection: 7,
        harness_spawn: None,
        port_spawn: None,
    };
    let mut context = WorldStartContext {
        state: &mut state,
        runtime: &positions.runtime,
        terrain: &terrain,
    };
    assert_eq!(
        context.resolve_position().unwrap(),
        [0x8000, (-96_i16 + 60) as u16, 0xabcd]
    );
    context.state.pair_index = 0x4000;
    assert_eq!(
        context.resolve_position(),
        Err(WorldStartError::NegativePositionSlot {
            pair_index: 0x4000,
            slot: i16::MIN
        })
    );
    context.publish_override([1, 0xffff, 2], None);
    assert_eq!(
        context.resolve_position().unwrap(),
        [1, (-36_i16) as u16, 2]
    );
    assert_eq!(context.state.vehicle_selection, 7);
    context.publish_override([3, 4, 5], Some(255));
    assert_eq!(context.state.vehicle_selection, 255);
}
