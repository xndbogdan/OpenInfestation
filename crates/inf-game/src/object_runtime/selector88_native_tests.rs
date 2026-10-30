//! Native selector 88 receipts have deliberately separate coverage domains:
//! 11 complete supported empty-list mode 0 initializations; constructor/input
//! snapshots for all 16 cases; modes 1/2 and already-initialized rejection remain
//! outside the world factory. Two real native listener callbacks are retained
//! as evidence, while the host requires their still-unimplemented effects.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use inf_formats::{
    parse_constructor_asset_bank, ActorConstructorPayload, ByteSpan, ConstructorPayloadView,
    HandlerFraming, HandlerId, HandlerRegion, InstanceCommand, InstanceField,
    PlacementInstanceAssets, RelocationTail,
};
use serde_json::{json, Value};

use super::*;
use crate::object_runtime::InstanceCommandAllocation;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        actor::native_tests,
        native_model_fixture::{Fixture, Models},
        ActorDamageTable, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
        CreationListener, CreationListenerError, CreationListenerInterface,
    },
};

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn payload(value: &Value) -> (Arc<ActorConstructorPayload>, Arc<RuntimeAllocation>) {
    let mut payload = [0xd3; 0x42];
    for (offset, name) in [(0, "flags"), (0x0c, "selector_0c"), (0x3c, "word_3c")] {
        payload[offset..offset + 4].copy_from_slice(&word(&value[name]).to_le_bytes());
    }
    for (offset, name) in [
        (4, "health"),
        (8, "word_08"),
        (0x38, "word_38"),
        (0x40, "word_40"),
    ] {
        payload[offset..offset + 2].copy_from_slice(&(word(&value[name]) as u16).to_le_bytes());
    }
    for (index, pair) in value["damage_responses"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        for channel in 0..2 {
            let offset = 0x10 + index * 4 + channel * 2;
            payload[offset..offset + 2]
                .copy_from_slice(&(word(&pair[channel]) as u16).to_le_bytes());
        }
    }
    let mut source = vec![0xa5; 9];
    source.extend_from_slice(&87_u16.to_le_bytes());
    source.extend_from_slice(&u16::MAX.to_le_bytes());
    source.extend_from_slice(&payload);
    let main = ByteSpan::new(9, source.len());
    let empty = ByteSpan::new(source.len(), source.len());
    source.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &source,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(9, source.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: empty,
                entry_offsets: ByteSpan::new(main.end, source.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        panic!("selector88 shared schema")
    };
    (
        Arc::new(payload.clone()),
        Arc::new(RuntimeAllocation::new(source.into(), main).unwrap()),
    )
}

struct Commands {
    allocation: Arc<InstanceCommandAllocation>,
    placement: PlacementInstanceAssets,
}

impl Commands {
    fn new(count: u32, input: &[Value]) -> Self {
        let mut source = count.to_le_bytes().to_vec();
        source.resize(4 + input.len() * 4, 0);
        let mut relocations = vec![];
        let commands = input
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let offset = source.len();
                let field = 4 + index * 4;
                source[field..field + 4].copy_from_slice(&(offset as u32).to_le_bytes());
                relocations.push(inf_formats::AssetRelocation {
                    table_span: ByteSpan::new(0, 0),
                    field_offset: field as u32,
                    field_span: ByteSpan::new(field, field + 4),
                    target_offset: offset as u32,
                });
                source.extend_from_slice(&word(&command["code"]).to_le_bytes());
                for value in command["payload_dwords"].as_array().unwrap() {
                    source.extend_from_slice(&word(value).to_le_bytes());
                }
                InstanceCommand {
                    index: index as u32,
                    pointer_span: ByteSpan::new(field, field + 4),
                    allocation_offset: offset as u32,
                    code: InstanceField {
                        span: ByteSpan::new(offset, offset + 4),
                        value: word(&command["code"]) as i32,
                    },
                    payload_offset: (offset + 4) as u32,
                }
            })
            .collect();
        let main = ByteSpan::new(0, source.len());
        Self {
            allocation: Arc::new(
                InstanceCommandAllocation::new(source.into(), main, relocations.into()).unwrap(),
            ),
            placement: PlacementInstanceAssets {
                placement_index: 0,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: count as i32,
                },
                command_pointer_span: ByteSpan::new(4, 4 + input.len() * 4),
                commands,
            },
        }
    }
}

fn snapshot(
    arena: &ObjectArena,
    objects: &BTreeMap<u32, ObjectId>,
    commands: Option<&Commands>,
) -> Value {
    let mut state = native_tests::snapshot(arena, objects);
    for value in state["objects"].as_array_mut().unwrap() {
        let object = objects[&word(&value["id"])];
        value["damage_source"] = json!(match arena.actor(object).unwrap().damage_table {
            ActorDamageTable::ConstructorDefault => "default",
            ActorDamageTable::Payload(_) => "payload",
        });
        let extension = arena.selector88(object).unwrap();
        assert_eq!(
            (extension.word_138, extension.word_140, extension.word_150),
            (None, None, None)
        );
        assert_eq!((extension.actor_154, extension.actor_158), (None, None));
        let script = extension.retained_13c.as_ref().map(|retained| {
            let commands = commands.unwrap();
            assert!(Arc::ptr_eq(retained.allocation(), &commands.allocation));
            commands
                .placement
                .commands
                .iter()
                .position(|command| command.payload_offset == retained.allocation_offset)
                .unwrap()
        });
        value["selector88"] = json!({"script_command":script,"word_144":extension.word_144,
            "word_148":extension.word_148,"word_14c":extension.word_14c,"word_154":0,"word_158":0});
    }
    state
}

#[derive(Default)]
struct RequiredEffects {
    calls: usize,
}

impl CreationListenerEffects for RequiredEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        _actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls += 1;
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

#[test]
fn selector88_supported_initialization_matches_native_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/selector88_initialization_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 16);
    assert_eq!(fixture["coverage"]["snapshots"], 48);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let sine = FrontendAssets::open(&root).unwrap().sine_table().values;
    let math = NativeBasisMath::new(sine);
    let mut compared = 0;
    let mut full_cases = 0;
    let mut excluded_endpoints = 0;
    let mut unsupported_listener_errors = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let resident = Fixture::new(&serde_json::from_value::<Models>(case["h8"].clone()).unwrap());
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: word(&case["owner_count"]) as usize,
            initial_capacity: word(&case["initial_capacity"]) as usize,
            null_handle: ActorHandle::from_raw(word(&case["null_handle"])),
        })
        .unwrap();
        let terrain = ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: Some(0),
            sine_quarter: &sine,
        };
        let construct = &case["steps"][0];
        let allocation = ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: serde_json::from_value(construct["allocation_position"].clone())
                .unwrap(),
            owner: word(&construct["owner"]) as u8,
            terrain,
        };
        let object = arena.allocate_selector88(allocation).unwrap();
        let objects = BTreeMap::from([(1, object)]);
        assert_eq!(
            snapshot(&arena, &objects, None),
            construct["state"],
            "{} constructor",
            case["id"]
        );
        compared += 1;
        let supplied = &case["steps"][1];
        let crate::object_runtime::ObjectClass::Actor(actor) =
            &mut arena.owned_mut(object).unwrap().state.class
        else {
            unreachable!("allocated selector 88 is an actor");
        };
        actor.word_6c = word(&supplied["word_6c"]);
        if supplied["already_initialized"].as_bool().unwrap() {
            arena.mark_initialized(object).unwrap();
        }
        assert_eq!(
            snapshot(&arena, &objects, None),
            supplied["state"],
            "{} supplied state",
            case["id"]
        );
        compared += 1;
        let step = &case["steps"][2];
        let request = &step["request"];
        if word(&request["mode"]) != 0 || supplied["already_initialized"].as_bool().unwrap() {
            // Those native completion endpoints remain outside fresh mode0
            // world construction. Only their ctor/input snapshots compare.
            excluded_endpoints += 1;
            continue;
        }
        let (payload, constructor_allocation) = payload(&request["payload"]);
        let commands = request["commands"]
            .as_array()
            .map(|input| Commands::new(word(&request["count_dword"]), input));
        let mut listeners = CreationListeners::new();
        let kinds = case["listeners"].as_array().unwrap();
        for kind in kinds {
            if kind != "null" {
                // Only host missing-effect behavior is exercised here. This
                // registration does not construct selector18 or copy its callback.
                listeners.register(object, CreationListenerInterface::Selector18);
            }
        }
        let registrations: Vec<_> = listeners.registrations().collect();
        let mut effects = RequiredEffects::default();
        let mut request = ObjectConstructionRequest {
            resource_argument: word(&request["resource_argument"]),
            context: BaseContext {
                position_words: serde_json::from_value(request["position_words"].clone()).unwrap(),
                placement_index: word(&request["placement_index"]),
            },
            suffix: commands
                .as_ref()
                .map(|commands| commands.allocation.commands(&commands.placement)),
            command_allocation: commands.as_ref().map(|commands| &commands.allocation),
            network_active: request["network"].as_bool().unwrap(),
            actor: Some(ActorFactoryInputs {
                terrain: None,
                world_start: None,
                derived: None,
                allocation,
                rotation_words: serde_json::from_value(request["rotation_words"].clone()).unwrap(),
                local_owner: allocation.owner,
                first_player_grants: None,
                grant_table: None,
                camera_settings: None,
                spawn_mode_1: false,
                network_handle: None,
                vehicle_table: None,
                math: &math,
                listeners: &mut listeners,
                listener_effects: &mut effects,
                network_world_active: request["network_host"].as_bool().unwrap(),
            }),
        };
        let result = initialize_actor(
            &mut arena,
            &resident.runtime,
            object,
            &ActorInitializationPlan {
                selector: 88,
                model_word: word(&step["request"]["model_word"]) as u16,
                payload,
                allocation: constructor_allocation,
                payload_offset: 4,
            },
            &mut request,
            crate::object_runtime::ConstructionPositionSource::Context,
        );
        if !kinds.is_empty() {
            assert!(matches!(
                result,
                Err(ObjectConstructionError::CreationListener(
                    CreationListenerError::UnsupportedInterface(
                        CreationListenerInterface::Selector18
                    )
                ))
            ));
            assert_eq!(effects.calls, 1);
            assert_eq!(arena.object(object).unwrap().base.flags & 0x100000, 0);
            assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
            assert_eq!(listeners.registrations().collect::<Vec<_>>(), registrations);
            unsupported_listener_errors += 1;
            excluded_endpoints += 1;
        } else {
            assert_eq!(
                result.unwrap(),
                ObjectConstructionOutcome::Constructed(object)
            );
            assert_eq!(effects.calls, 0);
            assert_eq!(
                snapshot(&arena, &objects, commands.as_ref()),
                step["state"],
                "{} completed",
                case["id"]
            );
            compared += 1;
            full_cases += 1;
        }
    }
    assert_eq!(
        (
            full_cases,
            compared,
            excluded_endpoints,
            unsupported_listener_errors
        ),
        (11, 43, 5, 2)
    );
}
