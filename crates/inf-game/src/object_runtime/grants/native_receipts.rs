//! Actual public live-grant parity. Original instruction/ABI/read cadence and
//! raw support masks belong to native_evidence; no portable observer is used.
//! Boot is constructed and checked here independently of the Python capture.

use std::{collections::BTreeSet, path::Path};

use inf_formats::{GrantTable, HandlerFraming, OverlayKey, RequestKind, GRANT_TABLE_ADDRESS};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    grant_text::{
        GrantTextArgument, GrantTextLiteral, GrantTextNullBinding, GrantTextPairClassification,
    },
    object_runtime::{
        ActorConstructionRequest, ActorExtension, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, DerivedActorClass, DerivedActorConstructionRequest, Family428650Class,
        Family429e20Class, ObjectClass, ObjectClassKind, ObjectState, Selector12GrantContext,
        Selector12State, WorldPlayer,
    },
    HandlerResources, Installation, ResidencyRequest,
};

const DIAGNOSTICS: [&str; 3] = [
    "diagnostic-copy-late-unknown-source-word",
    "diagnostic-mask192-recipient-alias",
    "diagnostic-offline191-adjacent-record",
];
const ADMITTED: [&str; 21] = [
    "already-owned-raw-dword-bypasses-all-lower-data",
    "copy-six-words-and-same-owner",
    "current-network-and-suppression-between-requests",
    "low-word-zero-no-owner-or-context",
    "mask191-online-no-record-consumption",
    "merge-empty-source-invalid-target-lazy",
    "merge-low-high-boundary-and-same-owner",
    "online-multidependency-first-satisfied",
    "online-recursive-60-95-96-97",
    "pair13b-two-canonical-holes",
    "pair13c-occupied-record-category",
    "prefix-bit-before-missing-executable",
    "prefix-bit-before-missing-process",
    "prefix-bit-before-missing-residency",
    "prefix-completed-format-before-enabled-message",
    "prefix-null-callback-before-missing-substitute",
    "prefix-pair-category-copy-before-missing-classification",
    "selector12-null-binding-bypasses-record-and-residency",
    "single-canonical-hole-actual-null-callback",
    "single-occupied-original-record-second-recipient",
    "text-zero-bypasses-flags-residency-formatter-message",
];
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn array(value: &Value) -> &[Value] {
    value.as_array().expect("native array")
}

fn number(value: &Value) -> u32 {
    u32::try_from(value.as_u64().expect("native unsigned DWORD")).unwrap()
}

fn word(bytes: &[Value]) -> Option<u32> {
    assert_eq!(bytes.len(), 4);
    if bytes.iter().all(Value::is_null) {
        return None;
    }
    Some(u32::from_le_bytes(std::array::from_fn(|index| {
        u8::try_from(number(&bytes[index])).expect("admitted whole known DWORD")
    })))
}

fn bytes(value: &Value) -> Vec<u8> {
    array(value)
        .iter()
        .map(|v| u8::try_from(number(v)).unwrap())
        .collect()
}

fn supplied_mask(value: &Value) -> [u32; 6] {
    array(value)
        .iter()
        .map(number)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn state_mask(state: &Value, owner: &str) -> [u32; 6] {
    let start = if owner == "actor" { 0x250 } else { 0xa0 };
    std::array::from_fn(|index| {
        word(&array(&state[owner])[start + index * 4..start + index * 4 + 4])
            .expect("all admitted native mask words are known")
    })
}

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn phase(value: &Value) -> RequestKind {
    match value.as_str().unwrap() {
        "base" => RequestKind::Base,
        "language" => RequestKind::Language,
        other => panic!("unadmitted finite Boot phase {other}"),
    }
}

/// Validate actual parser/residency allocations and every final H0 slot. The
/// native addresses are used only to interpret its declared registration fold;
/// they never become an Arc/slice/object/message address in the runtime.
fn verify_boot(installation: &Installation, boot: &ResidentRuntime, capture: &Value) {
    assert_eq!(
        digest(installation.preload_bytes()),
        capture["preload_sha256"]
    );
    assert_eq!(
        installation.preload_bytes().len(),
        number(&capture["preload_length"]) as usize
    );
    assert_eq!(boot.resource_capacity(0), Some(950));
    let summary = boot.summary();
    assert_eq!(summary.selection.difficulty, 0);
    assert_eq!(summary.selection.language, 0);
    assert_eq!(summary.active_leaves, [0, 2]);
    assert_eq!(summary.active_slots[0], 691);
    let declared_slots = array(&capture["slots"]);
    assert_eq!(declared_slots.len(), 950);
    let mut final_slots: Vec<Option<(u8, RequestKind, usize, u16, u16)>> = vec![None; 950];
    let mut declared_pointers = vec![0_u32; 950];
    let mut record_count = 0;
    let mut overwrites = Vec::new();

    for allocation in array(&capture["allocations"]) {
        let level = u8::try_from(number(&allocation["level"])).unwrap();
        let request_phase = phase(&allocation["phase"]);
        let entries = array(&allocation["entries"]);
        // One surviving registration locates the actual owner allocation;
        // all entries, including overwritten local346, are checked below.
        let survivor = entries
            .iter()
            .find(|entry| {
                declared_slots[number(&entry["global_index"]) as usize] == entry["native_pointer"]
            })
            .expect("each captured allocation has a surviving actual registration");
        let resource = boot
            .resource(0, number(&survivor["global_index"]) as u16)
            .unwrap();
        assert_eq!(resource.registration.level, level);
        assert_eq!(resource.registration.phase, request_phase);
        let overlay = resource.overlay;
        assert_eq!(
            overlay.key(),
            &OverlayKey::parse_filename(allocation["filename"].as_str().unwrap()).unwrap()
        );
        assert_eq!(overlay.key().request_kind(), request_phase);
        assert_eq!(digest(overlay.bytes()), allocation["overlay_sha256"]);
        assert_eq!(
            overlay.bytes().len(),
            number(&allocation["overlay_length"]) as usize
        );
        let region_index = number(&allocation["region_index"]) as usize;
        assert_eq!(region_index, 0, "this finite original capture has H0 first");
        let region = &overlay.layout().regions[region_index];
        assert_eq!(region.handler.get(), 0);
        let HandlerFraming::Standard { payload } = &region.framing else {
            panic!("actual standard H0 framing")
        };
        assert_eq!(
            [payload.start, payload.end],
            array(&allocation["payload_span"])
                .iter()
                .map(|v| number(v) as usize)
                .collect::<Vec<_>>()
                .as_slice()
        );
        assert_eq!(
            payload.get(overlay.bytes()).unwrap(),
            bytes(&allocation["payload"])
        );
        let request = overlay.requests().get(0).unwrap();
        assert_eq!(
            i64::from(request.count),
            allocation["request"]["count"].as_i64().unwrap()
        );
        assert_eq!(
            request
                .segments
                .iter()
                .map(|segment| segment.packed())
                .collect::<Vec<_>>(),
            array(&allocation["request"]["segments"])
                .iter()
                .map(number)
                .collect::<Vec<_>>()
        );
        assert_eq!(region.requested_count, entries.len() as u32);
        let HandlerResources::Strings(bank) = &overlay.handlers()[region_index] else {
            panic!("actual parsed H0 strings")
        };
        assert_eq!(bank.payload, *payload);
        assert_eq!(bank.entries.len(), entries.len());
        assert_eq!(bank.trailing.start, bank.trailing.end);
        for (local, (native_entry, actual_entry)) in entries.iter().zip(&bank.entries).enumerate() {
            let global = number(&native_entry["global_index"]) as u16;
            assert_eq!(number(&native_entry["local_index"]), local as u32);
            assert_eq!(actual_entry.local_index, local as u32);
            assert_eq!(request.global_index(local as u16), Some(global));
            assert_eq!(
                [actual_entry.record.start, actual_entry.record.end],
                array(&native_entry["record_span"])
                    .iter()
                    .map(|v| number(v) as usize)
                    .collect::<Vec<_>>()
                    .as_slice()
            );
            let record = actual_entry.record.get(overlay.bytes()).unwrap();
            assert_eq!(record.last(), Some(&0));
            assert!(!record[..record.len() - 1].contains(&0));
            let pointer = number(&native_entry["native_pointer"]);
            assert_eq!(
                pointer,
                number(&allocation["native_address"])
                    + (actual_entry.record.start - payload.start) as u32
            );
            if declared_pointers[usize::from(global)] != 0 {
                overwrites.push((
                    global,
                    declared_pointers[usize::from(global)],
                    pointer,
                    local as u16,
                ));
            }
            declared_pointers[usize::from(global)] = pointer;
            final_slots[usize::from(global)] =
                Some((level, request_phase, region_index, local as u16, global));
            record_count += 1;
        }
    }
    assert_eq!(record_count, 692);
    assert_eq!(
        declared_pointers,
        declared_slots.iter().map(number).collect::<Vec<_>>()
    );
    assert_eq!(overwrites.len(), 1);
    assert_eq!(overwrites[0].0, 355);
    assert_eq!(overwrites[0].3, 359);
    assert_eq!(
        final_slots[355],
        Some((0, RequestKind::Language, 0, 359, 355))
    );
    for (global, expected) in final_slots.into_iter().enumerate() {
        let actual = boot.resource(0, global as u16).map(|r| {
            assert_eq!(r.registration.handler, 0);
            (
                r.registration.level,
                r.registration.phase,
                r.registration.region_index,
                r.registration.local_index,
                r.registration.global_index,
            )
        });
        assert_eq!(actual, expected, "actual current H0 slot {global}");
    }
}

fn resident_custody(resident: &ResidentRuntime) -> String {
    let mut allocations = BTreeSet::new();
    let slots: Vec<_> = (0..950)
        .map(|global| {
            resident.resource(0, global).map(|resource| {
                // Actual Rust allocation identity is checked within this test only;
                // it is not converted to or compared with a native pointer.
                allocations.insert((
                    format!("{:?}", resource.overlay.key()),
                    std::sync::Arc::as_ptr(resource.overlay) as usize,
                    digest(resource.overlay.bytes()),
                ));
                resource.registration.clone()
            })
        })
        .collect();
    format!(
        "{:?}{:?}{:?}{:?}",
        resident.summary(),
        resident.revision(),
        slots,
        allocations
    )
}

fn allocate_selector12(arena: &mut ObjectArena) -> ObjectId {
    let grants = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let mut rng = 0;
    arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: None,
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap()
}

fn registry_arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap()
}

struct Owners {
    arena: ObjectArena,
    first: PlayerId,
    second: PlayerId,
    actor: ObjectId,
    absent: ObjectId,
    // Retains a genuine missing owner from another arena, without fabricating
    // ObjectId/PlayerId/native-pointer identity or changing its public policy.
    foreign: ObjectArena,
}

impl Owners {
    fn new(input: &Value) -> Self {
        let mut arena = registry_arena();
        arena.allocate(ObjectClassKind::VisualBase);
        let first = arena.add_world_player(supplied_mask(&input["first"]));
        let second = arena.add_world_player(supplied_mask(&input["second"]));
        let actor = allocate_selector12(&mut arena);
        arena
            .selector12_actor_mut(actor)
            .unwrap()
            .grants_24c
            .mask_04 = supplied_mask(&input["actor"]);
        for (name, player) in [("first", first), ("second", second)] {
            let byte = number(&input["suppression"][name]);
            assert!(
                byte <= 1,
                "current native corpus admits only real literal stores"
            );
            if byte == 0 {
                let recipient = arena.player_message(player).unwrap().identity();
                arena
                    .apply_player_message(PlayerMessageRequest {
                        recipient,
                        operation: PlayerMessageOperation::Enable,
                    })
                    .unwrap();
            }
        }
        let mut foreign = registry_arena();
        let absent = allocate_selector12(&mut foreign);
        Self {
            arena,
            first,
            second,
            actor,
            absent,
            foreign,
        }
    }

    fn owner(&self, name: &Value) -> GrantOwner {
        match name.as_str().unwrap() {
            "first" => GrantOwner::Player9c(self.first),
            "second" => GrantOwner::Player9c(self.second),
            "actor" => GrantOwner::Selector12Actor24c(self.actor),
            "unmapped" => GrantOwner::Selector12Actor24c(self.absent),
            other => panic!("unadmitted owner {other}"),
        }
    }

    fn apply_effect(&mut self, effect: &Value, process: &mut Option<ParticleFactoryProcessState>) {
        let data = array(&effect["bytes"]);
        match effect["region"].as_str().unwrap() {
            "network" => {
                assert_eq!(number(&effect["offset"]), 0);
                process
                    .as_mut()
                    .expect("existing live process")
                    .network_flag = word(data).unwrap();
            }
            "second" => {
                assert_eq!(number(&effect["offset"]), 0x89);
                assert_eq!(data, &[Value::from(0)]);
                let recipient = self.arena.player_message(self.second).unwrap().identity();
                self.arena
                    .apply_player_message(PlayerMessageRequest {
                        recipient,
                        operation: PlayerMessageOperation::Enable,
                    })
                    .unwrap();
            }
            other => panic!("unadmitted between-request effect {other}"),
        }
    }
}

fn selector_in_state(state: &mut ObjectState) -> &mut Selector12State {
    let ObjectClass::Actor(actor) = &mut state.class else {
        panic!("actual actor")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual derived")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual common family")
    };
    let Family428650Class::Family429e20(probe) = &mut family.class else {
        panic!("actual probe family")
    };
    let Family429e20Class::Selector12(selector) = &mut probe.class else {
        panic!("actual selector12")
    };
    selector
}

#[derive(Debug, PartialEq, Eq)]
struct ArenaSnapshot {
    objects: Vec<(u64, Option<(ObjectState, [usize; 3])>)>,
    players: Vec<WorldPlayer>,
    other_owners: String,
}

impl ArenaSnapshot {
    fn capture(arena: &ObjectArena) -> Self {
        Self {
            objects: arena
                .slots
                .iter()
                .map(|slot| {
                    (
                        slot.generation,
                        slot.value
                            .as_ref()
                            .map(|owned| (owned.state.clone(), owned.memberships)),
                    )
                })
                .collect(),
            players: arena.world_players().to_vec(),
            // No sibling visibility is added for private WorldPlayers.next_id;
            // full actual records/public identities and camera are retained.
            other_owners: format!(
                "{:?}{:?}{:?}",
                (
                    &arena.free,
                    arena.live_count,
                    &arena.lists,
                    &arena.list_owners,
                    &arena.routes,
                    &arena.actors,
                    &arena.map_annotations
                ),
                (
                    &arena.actor_handles,
                    &arena.actor_payloads,
                    &arena.constructor_allocations,
                    &arena.selector60_rows,
                    arena.world_load
                ),
                (arena.identity, arena.camera_handle())
            ),
        }
    }

    fn expected_masks(&mut self, owners: &Owners, state: &Value) {
        for (name, id) in [("first", owners.first), ("second", owners.second)] {
            self.players
                .iter_mut()
                .find(|p| p.id == id)
                .unwrap()
                .grants_9c = state_mask(state, name);
        }
        let actor = &mut self.objects[owners.actor.slot].1.as_mut().unwrap().0;
        selector_in_state(actor).grants_24c.mask_04 = state_mask(state, "actor");
    }
}

fn text_data<'a>(input: &Value, startup_null: &'a [u8]) -> GrantTextData<'a> {
    let null_substitute = if input["null_pointer"].is_null() {
        None
    } else {
        assert_eq!(number(&input["null_pointer"]), 0x5299f4);
        Some(GrantTextNullBinding {
            record: Some(startup_null),
        })
    };
    let pair_classification = if input["ctype_pointer"].is_null() {
        None
    } else {
        assert_eq!(number(&input["ctype_pointer"]), 0x545c7e);
        Some(GrantTextPairClassification {
            colon_high: (!input["colon_high"].is_null())
                .then(|| number(&input["colon_high"]) as u8),
            space_high: (!input["space_high"].is_null())
                .then(|| number(&input["space_high"]) as u8),
        })
    };
    GrantTextData {
        null_substitute,
        pair_classification,
    }
}

fn assert_result(
    result: Result<(), LiveGrantError>,
    input: &Value,
    receipt: &Value,
    target: GrantOwner,
    recipient: PlayerMessage88,
) {
    let name = input["name"].as_str().unwrap();
    match name {
        "prefix-bit-before-missing-process" => assert!(
            matches!(result, Err(LiveGrantError::MissingProcess { owner }) if owner == target)
        ),
        "prefix-bit-before-missing-executable" => assert!(
            matches!(result, Err(LiveGrantError::MissingExecutable { id: 0x17, address, bytes: 2 }) if address == resources::record_address(0x17))
        ),
        "prefix-bit-before-missing-residency" => assert!(matches!(
            result,
            Err(LiveGrantError::MissingResidency { index: 0x1cc })
        )),
        "prefix-null-callback-before-missing-substitute" => assert!(matches!(
            result,
            Err(LiveGrantError::Text(GrantTextError::MissingNullBinding(
                GrantTextArgument::Record
            )))
        )),
        "prefix-pair-category-copy-before-missing-classification" => assert!(matches!(
            result,
            Err(LiveGrantError::Text(
                GrantTextError::MissingClassificationBinding(GrantTextLiteral::Colon)
            ))
        )),
        "prefix-completed-format-before-enabled-message" => assert!(
            matches!(result, Err(LiveGrantError::Message(PlayerMessageError::EnabledMessageBoundary(id))) if id == recipient)
        ),
        _ => {
            assert_eq!(receipt["outcome"], "original_body_complete");
            result.unwrap();
        }
    }
    assert!(
        receipt["result"].is_null(),
        "actual bodies return void, not incidental EAX"
    );
}

#[test]
fn live_grants_match_actual_boot_and_public_native_histories() {
    let Some(fixture) = super::native_evidence::load_and_validate_fixture() else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let installation = Installation::open(&root).unwrap();
    let mut boot = ResidentRuntime::new(installation.clone()).unwrap();
    boot.apply(ResidencyRequest::Boot).unwrap();
    verify_boot(&installation, &boot, &fixture["asset_capture"]);
    let inactive = ResidentRuntime::new(installation).unwrap();
    assert_eq!(inactive.resource_capacity(0), Some(950));
    assert!((0..950).all(|index| inactive.resource(0, index).is_none()));
    let boot_before = resident_custody(&boot);
    let inactive_before = resident_custody(&inactive);
    let assets = FrontendAssets::open(&root).unwrap();
    let executable_bytes = assets.executable_bytes();
    let executable_before = digest(executable_bytes);
    let executable = FrontendExecutable::parse(executable_bytes).unwrap();
    let startup_null = executable.mapped(0x5299f4, 7).unwrap().bytes;
    assert_eq!(startup_null, b"(null)\0");

    let mut admitted = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut calls = 0;
    let mut prefixes = BTreeSet::new();
    for case in array(&fixture["cases"]) {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        if DIAGNOSTICS.contains(&name) {
            assert_eq!(case["evidence"], "source_diagnostic");
            assert!(skipped.insert(name.to_owned()));
            continue; // before constructors, masks or semantic data preparation
        }
        assert!(ADMITTED.contains(&name));
        assert!(admitted.insert(name.to_owned()));
        let mut owners = Owners::new(input);
        let foreign_before = ArenaSnapshot::capture(&owners.foreign);
        let mut current_process =
            (!input["network"].is_null()).then(|| ParticleFactoryProcessState {
                network_flag: number(&input["network"]),
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: None,
            });
        let current_text = text_data(input, startup_null);
        let text_before = current_text;
        let resident = if input["resident"].is_null() {
            None
        } else {
            match input["resident"].as_str().unwrap() {
                "boot" => Some(&boot),
                "inactive" => Some(&inactive),
                other => panic!("unadmitted resident owner {other}"),
            }
        };
        let requests = array(&input["steps"]);
        let receipts = array(&case["steps"]);
        assert_eq!(requests.len(), receipts.len());
        for (request, receipt) in requests.iter().zip(receipts) {
            for effect in array(&request["patches"]) {
                owners.apply_effect(effect, &mut current_process);
            }
            let target = owners.owner(&request["target"]);
            let operation = match request["operation"].as_str().unwrap() {
                "set" => LiveGrantOperation::SetRaw {
                    id: number(&request["raw_id"]),
                },
                "merge" => LiveGrantOperation::Merge {
                    source: owners.owner(&request["source"]),
                },
                "copy" => LiveGrantOperation::Copy {
                    source: owners.owner(&request["source"]),
                },
                other => panic!("unadmitted actual method {other}"),
            };
            let process_before = current_process;
            let mut expected = ArenaSnapshot::capture(&owners.arena);
            let recipient = owners
                .arena
                .player_message(owners.second)
                .unwrap()
                .identity();
            let context = LiveGrantContext {
                process: current_process.as_ref(),
                executable: input["executable"].as_bool().unwrap().then_some(executable),
                resident,
                text_data: Some(&current_text),
            };
            let result = owners
                .arena
                .mutate_live_grants(LiveGrantRequest { target, operation }, &context);
            assert_result(result, input, receipt, target, recipient);
            // Native post-state supplies only EXPECTED mask data after the real
            // method. No result, write or callback is replayed into the arena.
            expected.expected_masks(&owners, &receipt["state"]);
            assert_eq!(
                ArenaSnapshot::capture(&owners.arena),
                expected,
                "actual typed custody {name}"
            );
            assert_eq!(ArenaSnapshot::capture(&owners.foreign), foreign_before);
            assert_eq!(current_process, process_before);
            assert_eq!(current_text, text_before);
            assert_eq!(digest(executable_bytes), executable_before);
            assert_eq!(resident_custody(&boot), boot_before);
            assert_eq!(resident_custody(&inactive), inactive_before);
            if case["evidence"] == "semantic_prefix" {
                prefixes.insert(name.to_owned());
            }
            calls += 1;
        }
    }
    assert_eq!(
        admitted,
        ADMITTED.iter().map(|name| (*name).to_owned()).collect()
    );
    assert_eq!(
        skipped,
        DIAGNOSTICS.iter().map(|name| (*name).to_owned()).collect()
    );
    assert_eq!(
        prefixes,
        ADMITTED
            .iter()
            .filter(|name| name.starts_with("prefix-"))
            .map(|name| (*name).to_owned())
            .collect()
    );
    assert_eq!(prefixes.len(), 6);
    assert_eq!(admitted.len(), 21);
    assert_eq!(calls, 25);
}
