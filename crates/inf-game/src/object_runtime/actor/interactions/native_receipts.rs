//! Original canonical actor interaction callbacks and their short caller probes.
//!
//! Each admitted receiver uses an actual portable constructor, followed only by
//! the declared scalar fixture inputs. Native constructors are not executed by
//! this corpus. Unknown/unrepresented virtual bindings remain named exclusions;
//! constant leaf calls with retired raw receivers do not establish typed arena
//! dispatch parity. The scalar AnimationClock is this corpus's supplied clock owner.

use serde_json::{json, Value};

use super::*;
use crate::frontend_camera::AnimationClock;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, CameraSettings, DerivedActorConstructionRequest,
        Selector12GrantContext,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_frontend_sine_table, parse_grant_table, parse_terrain_bank, ByteSpan, FrontendExecutable,
    HandlerFraming, HandlerId, HandlerRegion, FRONTEND_SINE_TABLE_ADDRESS,
    TERRAIN_ENTRY_FIXED_BYTES,
};

const TABLES: [u32; 20] = [
    0x0052_0e58,
    0x0052_0f34,
    0x0052_104c,
    0x0052_1164,
    0x0052_0b84,
    0x0052_0cc8,
    0x0052_1eac,
    0x0052_273c,
    0x0052_7ee0,
    0x0051_f080,
    0x0052_0860,
    0x0052_7de8,
    0x0051_fbb0,
    0x0052_7058,
    0x0052_69c4,
    0x0052_7c00,
    0x0052_04d0,
    0x0052_89c0,
    0x0052_4ac0,
    0x0052_6b50,
];

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().map(|value| value.try_into().unwrap()))
        .collect()
}

fn dword(bytes: &[Option<u8>]) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

fn seeded_clock(value: u32) -> AnimationClock {
    let mut clock = AnimationClock::default();
    // Explicit fixture setup through the clock's actual public owner. No
    // private clock fields or duplicate live process DWORD are introduced.
    advance_clock(&mut clock, value);
    assert_eq!(clock.value(), value);
    clock
}

fn advance_clock(clock: &mut AnimationClock, ticks: u32) {
    let mut remaining = u64::from(ticks) * 20_000;
    while remaining != 0 {
        let delta = remaining.min(u64::from(u32::MAX)) as u32;
        clock.advance(delta);
        remaining -= u64::from(delta);
    }
}

fn construct_actor(
    arena: &mut ObjectArena,
    table: u32,
    executable: FrontendExecutable<'_>,
    sine: &[u16; 4096],
) -> ObjectId {
    let actor = ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: sine,
        },
    };
    let request = DerivedActorConstructionRequest {
        actor,
        world_mask_limits: [3, 3],
    };
    let mut constructor_rng = 0;
    match table {
        0x0052_0e58 => arena.allocate_derived_actor(request, &mut constructor_rng),
        0x0052_0f34 => arena.allocate_family428650_actor(request, &mut constructor_rng),
        0x0052_1164 => arena.allocate_family429e20_actor(request, &mut constructor_rng),
        0x0052_0b84 => arena.allocate_family4236d0_actor(request, &mut constructor_rng),
        0x0052_0cc8 => arena.allocate_selector10_actor(request, &mut constructor_rng),
        0x0052_1eac => arena.allocate_selector34_actor(request, &mut constructor_rng),
        0x0052_273c => arena.allocate_selector35_actor(request, &mut constructor_rng),
        0x0052_7ee0 => arena.allocate_selector36_actor(request, &mut constructor_rng),
        0x0051_f080 => arena.allocate_selector11_actor(request, &mut constructor_rng),
        0x0052_0860 => {
            // Selector2's real constructor requires supplied H12 and origin.
            // These explicit construction carriers are outside the native
            // callback comparison; no missing service/unknown position is
            // converted to a success inside any tested callback.
            let raw = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
            let span = ByteSpan::new(0, raw.len());
            let bank = parse_terrain_bank(
                &raw,
                &HandlerRegion {
                    handler: HandlerId::new(12).unwrap(),
                    requested_count: 1,
                    span,
                    framing: HandlerFraming::Standard { payload: span },
                },
            )
            .unwrap();
            let terrain = TerrainRuntime::new(
                &bank.entries[0],
                TerrainSampling {
                    spacing: TerrainSpacing::Units256,
                    height_scale: TerrainHeightScale::Units16,
                },
            );
            arena.allocate_selector2_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null: crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some([0; 3]),
                        terrain: ActorTerrainInputs {
                            terrain: Some(ActorTerrainSurface {
                                terrain: &terrain,
                                plane_height_fixed: 0,
                            }),
                            world_flags: Some(0),
                            ..actor.terrain
                        },
                        ..actor
                    },
                    ..request
                },
                &mut constructor_rng,
            )
        }
        0x0052_7de8 => arena.allocate_selector60_actor(request, &mut constructor_rng),
        0x0051_fbb0 => arena.allocate_selector77_actor(request, &mut constructor_rng),
        0x0052_7058 => {
            let table = parse_grant_table(executable).unwrap();
            arena.allocate_selector12_actor(
                request,
                &mut constructor_rng,
                Selector12GrantContext {
                    table: &table,
                    first_player_grants: None,
                },
            )
        }
        0x0052_7c00 => arena.allocate_actor(actor),
        0x0052_04d0 => arena.allocate_selector16(
            actor,
            CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
        ),
        0x0052_89c0 => arena.allocate_selector30_actor(actor),
        0x0052_4ac0 => arena.allocate_selector88(actor),
        0x0052_6b50 => arena.allocate_selector90(actor),
        other => panic!("unadmitted canonical actor table {other:08X}"),
    }
    .unwrap()
}

struct Backing {
    actor: Vec<Option<u8>>,
    source: Vec<Option<u8>>,
    clock: Vec<Option<u8>>,
}

fn write_dword(storage: &mut [Option<u8>], offset: usize, value: Option<u32>) {
    let value = value.map(u32::to_le_bytes);
    for byte in 0..4 {
        storage[offset + byte] = value.map(|value| value[byte]);
    }
}

impl Backing {
    fn new(input: &Value, table: u32) -> Self {
        let mut result = Self {
            actor: vec![None; 0x248],
            source: vec![None; 0x248],
            clock: vec![None; 4],
        };
        write_dword(&mut result.actor, 0, Some(table));
        write_dword(&mut result.source, 0, Some(0x0052_7c00));
        for (storage, record) in [
            (&mut result.actor, &input["actor"]),
            (&mut result.source, &input["source"]),
        ] {
            write_dword(
                storage,
                0xb4,
                record["classification"].as_u64().map(|v| v as u32),
            );
        }
        result.actor[0x18e] = input["actor"]["byte18e"].as_u64().map(|v| v as u8);
        result.actor[0x242] = input["actor"]["byte242"].as_u64().map(|v| v as u8);
        write_dword(
            &mut result.actor,
            0x244,
            input["actor"]["word244"].as_u64().map(|v| v as u32),
        );
        write_dword(
            &mut result.clock,
            0,
            input["clock"].as_u64().map(|v| v as u32),
        );
        result
    }

    fn region(&self, name: &str) -> &[Option<u8>] {
        match name {
            "actor" => &self.actor,
            "source" => &self.source,
            "clock" => &self.clock,
            other => panic!("unadmitted interaction region {other}"),
        }
    }

    fn compare(&self, state: &Value, name: &str) {
        for region in ["actor", "source", "clock"] {
            assert_eq!(
                self.region(region),
                bytes(&state["storage"][region]),
                "{name} {region} known spans"
            );
        }
    }
}

fn selector35_state(actor: &mut ActorState) -> &mut super::super::derived::Selector35State {
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual selector35 constructor")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual selector35 family")
    };
    let Family428650Class::Selector35(state) = &mut family.class else {
        panic!("actual selector35 final class")
    };
    state
}

fn supply_actor(arena: &mut ObjectArena, id: ObjectId, table: u32, input: &Value) {
    if let Some(classification) = input["classification"].as_u64() {
        arena.actor_mut(id).unwrap().word_b4 = classification as u32;
    }
    match table {
        0x0052_89c0 => {
            let ActorExtension::Selector30(state) = &mut arena.actor_mut(id).unwrap().extension
            else {
                panic!("actual selector30 constructor")
            };
            state.byte_18e = number(&input["byte18e"]) as u8;
        }
        0x0052_273c => {
            let state = selector35_state(arena.actor_mut(id).unwrap());
            state.bytes_240[2] = input["byte242"].as_u64().map(|v| v as u8);
            state.word_244 = number(&input["word244"]);
        }
        _ => {
            assert!(input["byte18e"].is_null());
            assert!(input["byte242"].is_null());
            assert!(input["word244"].is_null());
        }
    }
}

fn source_id(value: &Value, source: ObjectId) -> Option<ObjectId> {
    match value.as_str() {
        None => {
            assert!(value.is_null());
            None
        }
        Some("source" | "retired_source") => Some(source),
        other => panic!("unadmitted interaction source {other:?}"),
    }
}

fn operation_body(operation: &str, mode: &Value, table: u32) -> u32 {
    match operation {
        "query_component" => match mode.as_str().unwrap() {
            "Self50" => 0x0040_1300,
            "Interaction54" if table == 0x0052_89c0 => 0x0040_4610,
            "Interaction54" => 0x0040_1300,
            other => panic!("unadmitted query mode {other}"),
        },
        "interaction_relation" => {
            assert_eq!(table, 0x0052_89c0);
            0x0048_be30
        }
        "notify_interaction" if table == 0x0052_273c => 0x0043_f860,
        "notify_interaction" => 0x0040_26d0,
        other => panic!("unadmitted operation {other}"),
    }
}

/// Native-side read observations are checked against input custody and exact
/// field sites; they are not fed into any runtime method. The typed methods
/// preserve the same lazy errors and externally visible result/state.
fn compare_native_events(
    input: &Value,
    step: &Value,
    before: &Backing,
    table: u32,
    name: &str,
) -> bool {
    let request = &step["input"];
    let operation = request["operation"].as_str().unwrap();
    let mode = if operation == "query_component" {
        request["mode"].as_str().unwrap()
    } else {
        operation
    };
    let (start, stop, slot) = match mode {
        "Self50" => (0x0046_85c4, 0x0046_85cb, 0x50),
        "Interaction54" => (0x0046_8581, 0x0046_8589, 0x54),
        "notify_interaction" => (0x0046_85a0, 0x0046_85ad, 0x9c),
        "interaction_relation" => (0x0046_85b5, 0x0046_85be, 0xcc),
        _ => unreachable!(),
    };
    let mut reads = vec![("actor", 0usize, 4usize, start)];
    let actor_live = input["actor"]["live"].as_bool().unwrap();
    let mut writes = false;
    if actor_live {
        match operation {
            "interaction_relation" => {
                reads.push(("actor", 0x18e, 1, 0x0048_be30));
                if before.actor[0x18e].unwrap() == 0 && !request["source"].is_null() {
                    assert!(matches!(
                        request["source"].as_str(),
                        Some("source" | "retired_source")
                    ));
                    reads.push(("source", 0xb4, 4, 0x0048_be4a));
                    if input["source"]["live"].as_bool().unwrap() {
                        reads.push(("actor", 0xb4, 4, 0x0048_be50));
                    }
                }
            }
            "notify_interaction" if table == 0x0052_273c => {
                reads.push(("actor", 0x242, 1, 0x0043_f860));
                if before.actor[0x242].is_some_and(|v| v != 0) {
                    reads.push(("clock", 0, 4, 0x0043_f86a));
                    writes = dword(&before.clock).is_some();
                }
            }
            _ => {}
        }
    }
    let events = step["events"].as_array().unwrap();
    let actual_reads: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "read")
        .map(|event| {
            (
                event["region"].as_str().unwrap(),
                number(&event["offset"]) as usize,
                number(&event["bytes"]) as usize,
                number(&event["ip"]),
            )
        })
        .collect();
    assert_eq!(actual_reads, reads, "{name} exact native read order");
    for event in events.iter().filter(|event| event["kind"] == "read") {
        let offset = number(&event["offset"]) as usize;
        let count = number(&event["bytes"]) as usize;
        let known: Vec<_> = before.region(event["region"].as_str().unwrap())
            [offset..offset + count]
            .iter()
            .map(Option::is_some)
            .collect();
        assert_eq!(
            json!(known),
            event["known_mask"],
            "{name} reached read mask"
        );
    }
    let probe_entries: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "caller_probe_entry")
        .collect();
    assert_eq!(probe_entries.len(), 1);
    assert_eq!(probe_entries[0]["address"], start);
    assert_eq!(probe_entries[0]["execution"], "original_fragment");
    let body = operation_body(operation, &request["mode"], table);
    let entries: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "routine_entry")
        .collect();
    assert_eq!(entries.len(), usize::from(actor_live));
    if let Some(entry) = entries.first() {
        assert_eq!(entry["address"], body);
        assert_eq!(entry["receiver"], "actor");
        assert_eq!(entry["execution"], "native_body");
        let raw_source = match request["source"].as_str() {
            None => 0,
            Some("source" | "retired_source") => 0x0074_1000,
            _ => unreachable!(),
        };
        let arguments = match operation {
            "query_component" => vec![],
            "interaction_relation" => vec![raw_source],
            "notify_interaction" => vec![raw_source, number(&request["distance_squared"])],
            _ => unreachable!(),
        };
        assert_eq!(json!(arguments), entry["arguments"]);
    }
    let table_reads: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "executable_read")
        .collect();
    assert_eq!(table_reads.len(), usize::from(actor_live));
    if let Some(read) = table_reads.first() {
        assert_eq!(read["address"], table + slot);
        assert_eq!(read["bytes"], 4);
    }
    let completed = step["stopped"].is_null();
    assert!(step["fault"].is_null(), "{name} no admitted raw fault");
    let stops: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "caller_probe_stop")
        .collect();
    let returns: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "native_body_return")
        .collect();
    assert_eq!(stops.len(), usize::from(completed));
    assert_eq!(returns.len(), usize::from(completed));
    if completed {
        assert_eq!(stops[0]["address"], stop);
        assert_eq!(stops[0]["stack_restored"], true);
        assert_eq!(stops[0]["execution"], "before_next_selection_instruction");
        let ret = number(&returns[0]["address"]);
        assert!(matches!(
            ret,
            0x0040_1302
                | 0x0040_4612
                | 0x0040_26d0
                | 0x0048_be3f
                | 0x0048_be5f
                | 0x0048_be64
                | 0x0043_f87a
        ));
        if operation != "notify_interaction" {
            let native_result = match step["result"].as_str() {
                Some("actor") => 0x0074_0000,
                None if step["result"].is_null() => 0,
                None => number(&step["result"]),
                other => panic!("unexpected return identity {other:?}"),
            };
            assert_eq!(returns[0]["raw_eax"], native_result);
        }
    }
    let actual_writes: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "write")
        .collect();
    assert_eq!(actual_writes.len(), usize::from(writes));
    if writes {
        let write = actual_writes[0];
        assert_eq!(write["region"], "actor");
        assert_eq!(write["offset"], 0x244);
        assert_eq!(write["bytes"], 4);
        assert_eq!(write["ip"], 0x0043_f874);
        assert_eq!(
            dword(&bytes(&write["data"])),
            Some(dword(&before.clock).unwrap().wrapping_add(200))
        );
    }
    writes
}

#[test]
fn actor_interactions_match_original_native_callbacks() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/actor_interactions_native.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(fixture["text_address"], 0x0040_1000);
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(fixture["fpcw"], 0x027f);
    assert_eq!(
        fixture["counts"],
        json!({"histories":36,"steps":78,"kernel_histories":25,"semantic_prefix_histories":5,"diagnostic_histories":6,"kernel_steps":63,"semantic_prefix_steps":5,"diagnostic_steps":10})
    );
    assert_eq!(
        fixture["outcome_counts"],
        json!({"original_probe_complete":63,"semantic_prefix_stop":5,"source_diagnostic":10})
    );
    assert_eq!(
        fixture["event_counts"],
        json!({"caller_probe_entry":77,"caller_probe_stop":69,"executable_read":76,"native_body_return":70,"read":110,"routine_entry":77,"semantic_custody_boundary":2,"supplied_clock_advance":1,"supplied_effect":2,"write":3})
    );
    assert_eq!(
        fixture["body_entry_counts"],
        json!({"4199168":40,"4204240":19,"4212240":1,"4454496":7,"4767280":10})
    );
    assert_eq!(
        fixture["body_return_counts"],
        json!({"4199170":40,"4204240":19,"4212242":1,"4454522":4,"4767295":1,"4767327":2,"4767332":3})
    );
    assert_eq!(
        fixture["probe_entry_counts"],
        json!({"4621697":20,"4621728":26,"4621749":10,"4621764":21})
    );
    assert_eq!(
        fixture["probe_stop_counts"],
        json!({"4621705":20,"4621741":23,"4621758":6,"4621771":20})
    );
    assert_eq!(
        fixture["numeric_relation_binding"],
        json!({"table":0x0052_89c0,"slot":0xcc,"body":0x0048_be30})
    );
    let tables = [
        "derived",
        "family",
        "scalar_base",
        "probe_base",
        "family4236",
        "selector10",
        "selector34",
        "selector35",
        "selector36",
        "selector11",
        "selector2",
        "selector60",
        "selector77",
        "selector12",
        "selector13",
        "shared",
        "selector16",
        "selector30",
        "selector88",
        "selector90",
    ];
    let actual = fixture["tables"].as_object().unwrap();
    assert_eq!(actual.len(), TABLES.len());
    assert_eq!(
        fixture["query_matrix"].as_object().unwrap().len(),
        TABLES.len()
    );
    for (name, table) in tables.into_iter().zip(TABLES) {
        assert_eq!(actual[name], table);
        assert_eq!(
            fixture["query_matrix"][table.to_string()],
            json!({
                "self50":0x0040_1300,
                "interaction54":if table==0x0052_89c0 {0x0040_4610} else {0x0040_1300},
                "notification9c":if table==0x0052_273c {0x0043_f860} else {0x0040_26d0},
            })
        );
    }
    assert_eq!(
        fixture["bodies"],
        json!({
            "4199168":"33c0c3","4212240":"8bc1c3","4204240":"c20800",
            "4767280":"8a818e01000084c07408b801000000c204008b44240485c074188b80b40000008b91b40000003bc27408b801000000c2040033c0c20400",
            "4454496":"8a814202000084c07410a1c071540005c8000000898144020000c20800",
        })
    );
    assert_eq!(
        fixture["probes"],
        json!({
            "Self50":{"start":0x0046_85c4,"stop":0x0046_85cb,"bytes":"8b168bceff5250"},
            "Interaction54":{"start":0x0046_8581,"stop":0x0046_8589,"bytes":"8b55008bcdff5254"},
            "notify_interaction":{"start":0x0046_85a0,"stop":0x0046_85ad,"bytes":"8b550057568bcdff929c000000"},
            "interaction_relation":{"start":0x0046_85b5,"stop":0x0046_85be,"bytes":"8b0156ff90cc000000"},
        })
    );
    assert_eq!(
        fixture["physical_addresses"],
        json!({"actor":[0x0074_0000,0x248],"source":[0x0074_1000,0x248],"clock":[0x0054_71c0,4]})
    );
    const DIAGNOSTICS: [&str; 6] = [
        "raw-null-leaf-retired-receiver",
        "relation-unknown-component-class",
        "relation-unknown-mandatory18e",
        "relation-unknown-source-class",
        "table-scalar_base",
        "table-selector13",
    ];
    const PREFIXES: [&str; 5] = [
        "notify35-missing-clock",
        "notify35-unknown242-known-clock",
        "notify35-unknown242-no-clock",
        "relation-retired-source-reached",
        "retired-query-dispatch-prefix",
    ];
    assert_eq!(fixture["diagnostic_cases"], json!(DIAGNOSTICS));
    assert_eq!(fixture["semantic_prefix_cases"], json!(PREFIXES));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 36);
    assert_eq!(
        cases
            .iter()
            .map(|case| case["steps"].as_array().unwrap().len())
            .sum::<usize>(),
        78
    );
    // Authenticate metadata against recorded observations, including the
    // retained diagnostic rows, without expanding their typed admission.
    for (metadata, event_kind) in [
        ("body_entry_counts", "routine_entry"),
        ("body_return_counts", "native_body_return"),
        ("probe_entry_counts", "caller_probe_entry"),
        ("probe_stop_counts", "caller_probe_stop"),
    ] {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for event in cases
            .iter()
            .flat_map(|case| case["steps"].as_array().unwrap())
            .flat_map(|step| step["events"].as_array().unwrap())
            .filter(|event| event["kind"] == event_kind)
        {
            *counts
                .entry(number(&event["address"]).to_string())
                .or_default() += 1;
        }
        assert_eq!(json!(counts), fixture[metadata]);
    }
    let mut outcomes = std::collections::BTreeMap::<String, usize>::new();
    let mut event_counts = std::collections::BTreeMap::<String, usize>::new();
    for step in cases
        .iter()
        .flat_map(|case| case["steps"].as_array().unwrap())
    {
        *outcomes
            .entry(step["outcome"].as_str().unwrap().to_owned())
            .or_default() += 1;
        for event in step["events"].as_array().unwrap() {
            *event_counts
                .entry(event["kind"].as_str().unwrap().to_owned())
                .or_default() += 1;
        }
    }
    assert_eq!(json!(outcomes), fixture["outcome_counts"]);
    assert_eq!(json!(event_counts), fixture["event_counts"]);
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let sine = parse_frontend_sine_table(executable.data(), FRONTEND_SINE_TABLE_ADDRESS)
        .unwrap()
        .values;
    let (mut compared, mut invocations, mut prefixes, mut excluded) = (0, 0, 0, Vec::new());
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        match input["evidence"].as_str().unwrap() {
            "diagnostic" => {
                assert!(DIAGNOSTICS.contains(&name));
                excluded.push(name);
                continue;
            }
            "kernel" => {
                assert!(!PREFIXES.contains(&name));
            }
            "semantic_boundary_prefix" => {
                assert!(PREFIXES.contains(&name));
                prefixes += 1;
            }
            other => panic!("unadmitted interaction evidence {other}"),
        }
        let table = number(&fixture["tables"][input["binding"].as_str().unwrap()]);
        assert!(!matches!(table, 0x0052_104c | 0x0052_69c4));
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0x1234),
        })
        .unwrap();
        let actor = construct_actor(&mut arena, table, executable, &sine);
        supply_actor(&mut arena, actor, table, &input["actor"]);
        let source = construct_actor(&mut arena, 0x0052_7c00, executable, &sine);
        if let Some(class) = input["source"]["classification"].as_u64() {
            arena.actor_mut(source).unwrap().word_b4 = class as u32;
        } else {
            // An unknown source class is admitted only on an already retired
            // identity that every completed native path bypasses. There is no
            // live unknown scalar converted to a fabricated numeric class.
            assert!(!input["source"]["live"].as_bool().unwrap());
        }
        if !input["source"]["live"].as_bool().unwrap() {
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: source,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
        }
        if !input["actor"]["live"].as_bool().unwrap() {
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: actor,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
        }
        let mut clock = input["clock"].as_u64().map(|v| seeded_clock(v as u32));
        let mut backing = Backing::new(input, table);
        assert_eq!(
            case["steps"].as_array().unwrap().len(),
            input["steps"].as_array().unwrap().len()
        );
        for step in case["steps"].as_array().unwrap() {
            let request = &step["input"];
            assert_eq!(request["dispatch"], "probe");
            assert_eq!(request["eax_seed"], 0xdead_beef_u32);
            for change in request["patches"].as_array().unwrap() {
                // The sole admitted field mutation is a complete fresh
                // source classification between relation calls.
                assert_eq!(change["region"], "source");
                assert_eq!(change["offset"], 0xb4);
                let data = bytes(&change["bytes"]);
                let class = dword(&data).expect("known whole classification patch");
                arena.actor_mut(source).unwrap().word_b4 = class;
                backing.source[0xb4..0xb8].copy_from_slice(&data);
            }
            if !request["clock_update"].is_null() {
                advance_clock(
                    clock.as_mut().unwrap(),
                    number(&request["clock_update"]["advance_ticks"]),
                );
                write_dword(&mut backing.clock, 0, Some(clock.as_ref().unwrap().value()));
            }
            let before_actor = arena.object(actor).ok().cloned();
            let before_source = arena.object(source).ok().cloned();
            let before_clock = clock;
            let list = arena.actor_list().to_vec();
            let operation = request["operation"].as_str().unwrap();
            let source_arg = source_id(&request["source"], source);
            let result: Result<Value, ActorInteractionError> = match operation {
                "query_component" => {
                    let mode = match request["mode"].as_str().unwrap() {
                        "Self50" => ActorComponentQueryMode::Self50,
                        "Interaction54" => ActorComponentQueryMode::Interaction54,
                        other => panic!("unadmitted component mode {other}"),
                    };
                    arena
                        .query_actor_component(ActorComponentQueryRequest { actor, mode })
                        .map(|v| {
                            v.map(|id| {
                                assert_eq!(id, actor);
                                json!("actor")
                            })
                            .unwrap_or(Value::Null)
                        })
                        .map_err(ActorInteractionError::Object)
                }
                "interaction_relation" => arena
                    .query_actor_interaction_relation(ActorInteractionRelationRequest {
                        component: actor,
                        source: source_arg,
                    })
                    .map(|v| json!(v)),
                "notify_interaction" => arena
                    .notify_actor_interaction(
                        ActorInteractionNotificationRequest {
                            candidate: actor,
                            source: source_arg,
                            distance_squared: number(&request["distance_squared"]),
                        },
                        ActorInteractionNotificationContext {
                            animation_clock: clock.as_ref().map(|clock| {
                                clock as &dyn crate::frontend_camera::AnimationClockSource
                            }),
                        },
                    )
                    .map(|()| Value::Null),
                other => panic!("unadmitted interaction operation {other}"),
            };
            match step["stopped"].as_str() {
                None => {
                    assert_eq!(step["outcome"], "original_probe_complete");
                    assert_eq!(
                        result,
                        Ok(step["result"].clone()),
                        "{name} actual callback result"
                    );
                }
                Some("retired_source_read") => assert_eq!(
                    result,
                    Err(ActorInteractionError::Object(ObjectError::UnknownObject(
                        source
                    ))),
                    "{name}"
                ),
                Some("retired_actor_dispatch") => assert_eq!(
                    result,
                    Err(ActorInteractionError::Object(ObjectError::UnknownObject(
                        actor
                    ))),
                    "{name}"
                ),
                Some("unknown_actor_242") => assert_eq!(
                    result,
                    Err(ActorInteractionError::Uninitialized {
                        actor,
                        offset: 0x242
                    }),
                    "{name}"
                ),
                Some("missing_clock") => assert_eq!(
                    result,
                    Err(ActorInteractionError::MissingAnimationClock),
                    "{name}"
                ),
                other => panic!("unadmitted interaction stop {other:?}"),
            }
            if !step["stopped"].is_null() {
                assert_eq!(step["outcome"], "semantic_prefix_stop");
            }
            let writes = compare_native_events(input, step, &backing, table, name);
            let mut expected_actor = before_actor;
            if writes {
                let value = arena.selector35_actor(actor).unwrap().word_244;
                write_dword(&mut backing.actor, 0x244, Some(value));
                let crate::object_runtime::ObjectClass::Actor(state) =
                    &mut expected_actor.as_mut().unwrap().class
                else {
                    unreachable!()
                };
                selector35_state(state).word_244 = value;
            }
            assert_eq!(
                arena.object(actor).ok(),
                expected_actor.as_ref(),
                "{name} entire typed receiver except declared244 write"
            );
            assert_eq!(
                arena.object(source).ok(),
                before_source.as_ref(),
                "{name} entire typed source unchanged"
            );
            assert_eq!(
                arena.actor_list(),
                list,
                "{name} typed list custody unchanged"
            );
            assert_eq!(
                clock, before_clock,
                "{name} actual clock unchanged during callback"
            );
            let state = &step["state"];
            backing.compare(state, name);
            assert_eq!(state["actor_live"], input["actor"]["live"]);
            assert_eq!(state["source_live"], input["source"]["live"]);
            assert_eq!(
                state["clock_ticks"],
                json!(clock.as_ref().map(AnimationClock::value))
            );
            assert_eq!(
                state["clock_accumulated_micros"],
                json!(clock.as_ref().map(AnimationClock::accumulated_micros))
            );
            invocations += 1;
        }
        compared += 1;
    }
    excluded.sort_unstable();
    assert_eq!(excluded, DIAGNOSTICS);
    assert_eq!((compared, invocations, prefixes), (30, 68, 5));
}
