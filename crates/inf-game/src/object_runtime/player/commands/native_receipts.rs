//! Actual public command parity on complete live owners. Original instruction,
//! address, ABI, stack and read cadence evidence belongs to native_evidence.
//! Native diagnostics are rejected before typed source or player preparation.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use inf_formats::{HandlerFraming, OverlayKey, RequestKind};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{ActorHandle, ActorRegistryConfig, ObjectClassKind, ObjectState, WorldPlayer},
    HandlerResources, Installation, ResidencyEvent, ResidencyRequest,
};

const DIAGNOSTICS: [&str; 8] = [
    "diagnostic-unknown-command-word",
    "diagnostic-negative32-aliases-player12c",
    "diagnostic-command96-aliases-adjacent-word",
    "diagnostic-full-dword-id-not-lowword",
    "diagnostic-nonsilent73-adjacent-table",
    "diagnostic-h0-count-equality-before-unknown-slot",
    "diagnostic-h0-count-greater-null-without-slot",
    "diagnostic-unknown-message-suppression",
];
const ADMITTED: [&str; 17] = [
    "constructor-query-zero-is-not-owned",
    "silent-three-word-boundaries",
    "signed-minus-one-minus31-share-word-zero",
    "owned-bypasses-silent-exe-residency-message",
    "network100-bypasses-unmapped-player-and-raw-id",
    "lowbyte-silent100-versus101",
    "boot-holes-original-null-callback",
    "boot-retained-bank0-nonlinear-command41",
    "activate48-original-leaf1-occupied-records",
    "requested-first-and-second-player-independent",
    "current-network-and-suppression-reborrow",
    "prefix-missing-process-before-owned-query",
    "prefix-commit-before-missing-residency",
    "prefix-occupied-source-before-enabled-message",
    "prefix-null-source-before-enabled-message",
    "prefix-retained-commit-retry-missing-executable",
    "silent73-and-owned73-never-read-adjacent-table",
];

fn array(value: &Value) -> &[Value] {
    value.as_array().expect("native array")
}

fn number(value: &Value) -> u32 {
    u32::try_from(value.as_u64().expect("native unsigned DWORD")).unwrap()
}

fn bytes(value: &Value) -> Vec<u8> {
    array(value)
        .iter()
        .map(|v| number(v).try_into().unwrap())
        .collect()
}

fn word(values: &[Value]) -> Option<u32> {
    assert_eq!(values.len(), 4);
    if values.iter().all(Value::is_null) {
        return None;
    }
    Some(u32::from_le_bytes(std::array::from_fn(|index| {
        u8::try_from(number(&values[index])).expect("admitted complete known DWORD")
    })))
}

fn mask(value: &Value) -> [u32; 3] {
    array(value)
        .iter()
        .map(number)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn state_mask(state: &Value, name: &str) -> [u32; 3] {
    std::array::from_fn(|index| {
        let start = 0x130 + index * 4;
        word(&array(&state[name])[start..start + 4]).unwrap()
    })
}

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn phase(value: &Value) -> RequestKind {
    match value.as_str().unwrap() {
        "base" => RequestKind::Base,
        "language" => RequestKind::Language,
        other => panic!("unadmitted captured allocation phase {other}"),
    }
}

type Binding = (u8, RequestKind, usize, u16, u16);

/// Check an actual current allocation, including every original record and
/// NUL, even its overwritten local record. Native pointers only interpret the
/// independent declared slot fold; they never become Rust allocation addresses.
fn verify_allocation(
    runtime: &ResidentRuntime,
    allocation: &Value,
    declared_slots: &[Value],
    bindings: &mut [Option<Binding>],
    pointers: &mut [u32],
) -> usize {
    let level = u8::try_from(number(&allocation["level"])).unwrap();
    let request_phase = phase(&allocation["phase"]);
    let entries = array(&allocation["entries"]);
    let survivor = entries
        .iter()
        .find(|entry| {
            declared_slots[number(&entry["global_index"]) as usize] == entry["native_pointer"]
        })
        .expect("captured current allocation has an actual surviving registration");
    let resource = runtime
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
    assert_eq!(region_index, 0);
    let region = &overlay.layout().regions[region_index];
    assert_eq!(region.handler.get(), 0);
    let HandlerFraming::Standard { payload } = &region.framing else {
        panic!("actual standard H0 payload")
    };
    assert_eq!(
        vec![payload.start, payload.end],
        array(&allocation["payload_span"])
            .iter()
            .map(|v| number(v) as usize)
            .collect::<Vec<_>>()
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
            .map(|s| s.packed())
            .collect::<Vec<_>>(),
        array(&allocation["request"]["segments"])
            .iter()
            .map(number)
            .collect::<Vec<_>>()
    );
    assert_eq!(region.requested_count, entries.len() as u32);
    let HandlerResources::Strings(bank) = &overlay.handlers()[region_index] else {
        panic!("actual H0 parsed strings")
    };
    assert_eq!(bank.payload, *payload);
    assert_eq!(bank.entries.len(), entries.len());
    assert_eq!(bank.trailing.start, bank.trailing.end);
    for (local, (native, actual)) in entries.iter().zip(&bank.entries).enumerate() {
        let global = number(&native["global_index"]) as u16;
        assert_eq!(number(&native["local_index"]), local as u32);
        assert_eq!(actual.local_index, local as u32);
        assert_eq!(request.global_index(local as u16), Some(global));
        assert_eq!(
            vec![actual.record.start, actual.record.end],
            array(&native["record_span"])
                .iter()
                .map(|v| number(v) as usize)
                .collect::<Vec<_>>()
        );
        let record = actual.record.get(overlay.bytes()).unwrap();
        assert_eq!(record.last(), Some(&0));
        assert!(!record[..record.len() - 1].contains(&0));
        let pointer = number(&native["native_pointer"]);
        assert_eq!(
            pointer,
            number(&allocation["native_address"]) + (actual.record.start - payload.start) as u32
        );
        pointers[usize::from(global)] = pointer;
        bindings[usize::from(global)] =
            Some((level, request_phase, region_index, local as u16, global));
    }
    entries.len()
}

fn verify_profile(
    installation: &Installation,
    runtime: &ResidentRuntime,
    capture: &Value,
    activated: bool,
) {
    assert_eq!(
        digest(installation.preload_bytes()),
        capture["preload_sha256"]
    );
    assert_eq!(
        installation.preload_bytes().len(),
        number(&capture["preload_length"]) as usize
    );
    assert_eq!(runtime.resource_capacity(0), Some(950));
    let summary = runtime.summary();
    assert_eq!(summary.selection.difficulty, 0);
    assert_eq!(summary.selection.language, 0);
    assert_eq!(
        summary.active_leaves,
        if activated {
            vec![0, 1, 48]
        } else {
            vec![0, 2]
        }
    );
    assert_eq!(summary.active_slots[0], if activated { 869 } else { 691 });
    let declared_slots = array(if activated {
        &capture["activation48"]["slots"]
    } else {
        &capture["slots"]
    });
    assert_eq!(declared_slots.len(), 950);
    let mut bindings = vec![None; 950];
    let mut pointers = vec![0; 950];
    let mut records = 0;
    for allocation in array(&capture["allocations"]) {
        // Leaf2 is released by the actual tree transition, not retained as a
        // parallel current string bank. Its full allocation is checked in Boot.
        if activated && number(&allocation["level"]) == 2 {
            continue;
        }
        records += verify_allocation(
            runtime,
            allocation,
            declared_slots,
            &mut bindings,
            &mut pointers,
        );
    }
    if activated {
        records += verify_allocation(
            runtime,
            &capture["leaf1_language"],
            declared_slots,
            &mut bindings,
            &mut pointers,
        );
    }
    assert_eq!(records, if activated { 870 } else { 692 });
    assert_eq!(
        pointers,
        declared_slots.iter().map(number).collect::<Vec<_>>()
    );
    assert_eq!(bindings[355], Some((0, RequestKind::Language, 0, 359, 355)));
    for (global, expected) in bindings.iter().enumerate() {
        let actual = runtime.resource(0, global as u16).map(|r| {
            assert_eq!(r.registration.handler, 0);
            (
                r.registration.level,
                r.registration.phase,
                r.registration.region_index,
                r.registration.local_index,
                r.registration.global_index,
            )
        });
        assert_eq!(
            actual.as_ref(),
            expected.as_ref(),
            "actual current H0 slot {global}"
        );
    }
    if activated {
        for global in [872, 873] {
            assert!(runtime.resource(0, global).is_none());
            assert_eq!(pointers[usize::from(global)], 0);
        }
    }
}

fn resident_custody(runtime: &ResidentRuntime) -> String {
    let mut allocations = BTreeMap::new();
    let slots: Vec<_> = (0..950)
        .map(|global| {
            runtime.resource(0, global).map(|r| {
                // Compare actual allocation identity before/after within Rust only.
                allocations
                    .entry((
                        format!("{:?}", r.overlay.key()),
                        std::sync::Arc::as_ptr(r.overlay) as usize,
                    ))
                    .or_insert_with(|| digest(r.overlay.bytes()));
                r.registration.clone()
            })
        })
        .collect();
    format!(
        "{:?}{:?}{:?}{:?}",
        runtime.summary(),
        runtime.revision(),
        slots,
        allocations
    )
}

fn registry_arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap();
    let base = arena.allocate(ObjectClassKind::Base);
    arena.set_flag0(base, true).unwrap();
    arena.allocate(ObjectClassKind::VisualBase);
    arena
}

struct Owners {
    arena: ObjectArena,
    first: PlayerId,
    second: PlayerId,
    absent: PlayerId,
    foreign: ObjectArena,
}

impl Owners {
    fn new(input: &Value) -> Self {
        assert!(
            array(&input["patches"]).is_empty(),
            "no admitted arbitrary initial raw effects"
        );
        let mut arena = registry_arena();
        let first = arena.add_world_player([0x1357_9bdf; 6]);
        let second = arena.add_world_player([0x2468_ace0; 6]);
        for (name, player) in [("first", first), ("second", second)] {
            // Declared whole initial backing only; no native post-state is
            // ever used to update actual live owners.
            arena.world_player_mut(player).unwrap().command_mask_130 = mask(&input[name]);
            let byte = number(&input["suppression"][name]);
            assert!(byte <= 1, "canonical literal suppression input");
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
        foreign.add_world_player([0; 6]);
        foreign.add_world_player([0; 6]);
        let absent = foreign.add_world_player([0; 6]);
        Self {
            arena,
            first,
            second,
            absent,
            foreign,
        }
    }

    fn player(&self, value: &Value) -> PlayerId {
        match value.as_str().unwrap() {
            "first" => self.first,
            "second" => self.second,
            "unmapped" => self.absent,
            other => panic!("unadmitted requested player {other}"),
        }
    }

    fn apply_effect(&mut self, effect: &Value, process: &mut Option<ParticleFactoryProcessState>) {
        let values = array(&effect["bytes"]);
        match effect["region"].as_str().unwrap() {
            "network" => {
                assert_eq!(number(&effect["offset"]), 0);
                process
                    .as_mut()
                    .expect("actual existing process owner")
                    .network_flag = word(values).unwrap();
            }
            "second" => {
                assert_eq!(number(&effect["offset"]), 0x89);
                assert_eq!(values, &[Value::from(0)]);
                let recipient = self.arena.player_message(self.second).unwrap().identity();
                self.arena
                    .apply_player_message(PlayerMessageRequest {
                        recipient,
                        operation: PlayerMessageOperation::Enable,
                    })
                    .unwrap();
            }
            other => panic!("unadmitted between-request current-owner effect {other}"),
        }
    }

    fn compare_native_fields(&self, state: &Value, process: &Option<ParticleFactoryProcessState>) {
        for (name, player) in [("first", self.first), ("second", self.second)] {
            let actual = self.arena.world_player(player).unwrap();
            let native = array(&state[name]);
            assert_eq!(actual.command_mask_130, state_mask(state, name));
            assert_eq!(
                self.arena
                    .player_message(player)
                    .unwrap()
                    .suppression_byte(),
                number(&native[0x89]) as u8
            );
            for (offset, value) in [
                (0x12c, actual.dword_12c),
                (0x144, actual.dword_144),
                (0x148, actual.dword_148),
                (0x14c, actual.dword_14c),
            ] {
                assert_eq!(word(&native[offset..offset + 4]), Some(value));
            }
            // Raw save-copy ownership of +13C..143 does not extend this
            // command receipt's field parity or admit command word index3.
            // Other unknown native bytes stay outside semantic field parity.
        }
        assert_eq!(self.arena.first_player(), Some(self.first));
        assert_eq!(
            self.arena
                .world_players()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            [self.first, self.second]
        );
        match process {
            Some(actual) => {
                assert_eq!(word(array(&state["network"])), Some(actual.network_flag));
                assert_eq!(
                    word(array(&state["process_null"])),
                    actual.process_null_handle.map(ActorHandle::raw)
                );
                //The native command receipt owns network/NULL parity above;
                //manager phase is independently owned and not in this carrier.
            }
            None => {
                assert!(array(&state["network"]).iter().all(Value::is_null));
                assert!(array(&state["process_null"]).iter().all(Value::is_null));
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ArenaSnapshot {
    objects: Vec<(u64, Option<(ObjectState, [usize; 3])>)>,
    players: Vec<WorldPlayer>,
    other: String,
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
                            .map(|o| (o.state.clone(), o.memberships)),
                    )
                })
                .collect(),
            players: arena.world_players().to_vec(),
            other: format!(
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
                (arena.identity, arena.players.next_id, arena.camera_handle())
            ),
        }
    }

    fn expected_masks(&mut self, owners: &Owners, state: &Value) {
        for (name, player) in [("first", owners.first), ("second", owners.second)] {
            self.players
                .iter_mut()
                .find(|p| p.id == player)
                .unwrap()
                .command_mask_130 = state_mask(state, name);
        }
    }
}

fn assert_result(
    actual: Result<PlayerCommandOutcome, PlayerCommandError>,
    receipt: &Value,
    request: &Value,
    owners: &Owners,
    table: &[Value],
) -> bool {
    let player = owners.player(&request["player"]);
    let raw_id = number(&request["raw_id"]);
    if receipt["native_completed"].as_bool().unwrap() {
        assert_eq!(receipt["outcome"], "original_body_complete");
        assert!(receipt["stop"].is_null());
        let actual = actual.expect("admitted original completion");
        match request["operation"].as_str().unwrap() {
            "query" => assert_eq!(
                actual,
                PlayerCommandOutcome::Owned(receipt["result"].as_bool().unwrap())
            ),
            "set" => {
                assert_eq!(actual, PlayerCommandOutcome::Applied);
                assert!(receipt["result"].is_null(), "native setter is void");
            }
            other => panic!("unadmitted operation {other}"),
        }
        return false;
    }
    assert_eq!(receipt["outcome"], "semantic_prefix_stop");
    assert_eq!(request["operation"], "set");
    assert!(receipt["result"].is_null());
    match (
        receipt["stop"].as_str().unwrap(),
        actual.expect_err("required reached boundary"),
    ) {
        ("unknown_network", PlayerCommandError::MissingProcess { player: actual }) => {
            assert_eq!(actual, player)
        }
        (
            "unknown_command_table",
            PlayerCommandError::MissingExecutable {
                raw_id: actual,
                address,
                bytes,
            },
        ) => {
            assert_eq!(actual, raw_id);
            assert_eq!(
                address,
                COMMAND_TEXT_ADDRESS.wrapping_add(raw_id.wrapping_mul(2))
            );
            assert_eq!(bytes, 2);
        }
        ("unknown_h0", PlayerCommandError::MissingResidency { index }) => {
            assert_eq!(u32::from(index), number(&table[raw_id as usize]))
        }
        (
            "enabled_message",
            PlayerCommandError::Message(PlayerMessageError::EnabledMessageBoundary(recipient)),
        ) => {
            assert_eq!(
                recipient,
                owners.arena.player_message(player).unwrap().identity()
            );
        }
        (stop, error) => panic!("unadmitted prefix mapping {stop}: {error:?}"),
    }
    true
}

#[test]
fn player_commands_match_actual_residency_and_public_native_histories() {
    let Some(fixture) = super::native_evidence::load_and_validate_fixture() else {
        return;
    };
    let mut admitted = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut typed_cases = Vec::new();
    for case in array(&fixture["cases"]) {
        let name = case["input"]["name"].as_str().unwrap();
        if DIAGNOSTICS.contains(&name) {
            assert_eq!(case["evidence"], "source_diagnostic");
            assert!(skipped.insert(name.to_owned()));
            continue; // before any typed constructors, masks or source setup
        }
        assert!(ADMITTED.contains(&name));
        assert!(admitted.insert(name.to_owned()));
        typed_cases.push(case);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let installation = Installation::open(&root).unwrap();
    let inactive = ResidentRuntime::new(installation.clone()).unwrap();
    assert_eq!(inactive.resource_capacity(0), Some(950));
    assert!(inactive.summary().active_leaves.is_empty());
    assert!((0..950).all(|global| inactive.resource(0, global).is_none()));
    let mut boot = ResidentRuntime::new(installation.clone()).unwrap();
    boot.apply(ResidencyRequest::Boot).unwrap();
    verify_profile(&installation, &boot, &fixture["asset_capture"], false);
    let mut activated = boot.clone();
    let change = activated
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let leaves = change
        .events
        .iter()
        .filter_map(|event| match event {
            ResidencyEvent::LeafReleased { level } => Some((false, *level)),
            ResidencyEvent::LeafActivated { level } => Some((true, *level)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(leaves, [(false, 2), (true, 1), (true, 48)]);
    verify_profile(&installation, &activated, &fixture["asset_capture"], true);
    let inactive_before = resident_custody(&inactive);
    let boot_before = resident_custody(&boot);
    let activated_before = resident_custody(&activated);
    let assets = FrontendAssets::open(&root).unwrap();
    let executable_bytes = assets.executable_bytes();
    let executable_before = digest(executable_bytes);
    assert_eq!(executable_before, fixture["metadata"]["executable_sha256"]);
    let executable = FrontendExecutable::parse(executable_bytes).unwrap();
    let table = executable
        .mapped(COMMAND_TEXT_ADDRESS, COMMAND_TEXT_WORDS as usize * 2)
        .unwrap()
        .bytes;
    assert_eq!(
        digest(table),
        fixture["metadata"]["command_table"]["sha256"]
    );
    let original_words: Vec<_> = table
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(
        original_words
            .iter()
            .copied()
            .map(u32::from)
            .collect::<Vec<_>>(),
        array(&fixture["metadata"]["command_table"]["words"])
            .iter()
            .map(number)
            .collect::<Vec<_>>()
    );
    let mut prefixes = BTreeSet::new();
    let mut calls = 0;
    let mut completed = 0;
    let mut prefix_calls = 0;
    for case in typed_cases {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert_eq!(number(&input["capacity"]), 950);
        let mut owners = Owners::new(input);
        let foreign_before = ArenaSnapshot::capture(&owners.foreign);
        let mut process = if input["process_owner_present"].as_bool().unwrap() {
            Some(ParticleFactoryProcessState {
                network_flag: number(&input["network"]),
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: (!input["process_null"].is_null())
                    .then(|| ActorHandle::from_raw(number(&input["process_null"]))),
            })
        } else {
            assert!(input["network"].is_null());
            None
        };
        let resident = if input["resident"].is_null() {
            None
        } else {
            match input["resident"].as_str().unwrap() {
                "boot" => Some(&boot),
                "activate48" => Some(&activated),
                other => panic!("unadmitted current residency {other}"),
            }
        };
        owners.compare_native_fields(&case["initial_state"], &process);
        let requests = array(&input["steps"]);
        let receipts = array(&case["steps"]);
        assert_eq!(requests.len(), receipts.len());
        for (request, receipt) in requests.iter().zip(receipts) {
            for effect in array(&request["patches"]) {
                owners.apply_effect(effect, &mut process);
            }
            let player = owners.player(&request["player"]);
            let raw_id = number(&request["raw_id"]);
            let operation = match request["operation"].as_str().unwrap() {
                "query" => PlayerCommandOperation::Query { raw_id },
                "set" => PlayerCommandOperation::Set {
                    raw_id,
                    silent: number(&request["silent"]),
                },
                other => panic!("unadmitted public operation {other}"),
            };
            let before_process = process;
            let mut expected = ArenaSnapshot::capture(&owners.arena);
            let context = PlayerCommandContext {
                process: process.as_ref(),
                executable: input["executable"].as_bool().unwrap().then_some(executable),
                resident,
            };
            let actual = owners
                .arena
                .apply_player_command(PlayerCommandRequest { player, operation }, &context);
            let prefix = assert_result(
                actual,
                receipt,
                request,
                &owners,
                array(&fixture["metadata"]["command_table"]["words"]),
            );
            // Native output updates only the expected clone AFTER the actual
            // public call. No writes, source result or notification is replayed.
            expected.expected_masks(&owners, &receipt["state"]);
            assert_eq!(
                ArenaSnapshot::capture(&owners.arena),
                expected,
                "actual whole typed custody {name}"
            );
            owners.compare_native_fields(&receipt["state"], &process);
            assert_eq!(ArenaSnapshot::capture(&owners.foreign), foreign_before);
            assert_eq!(process, before_process);
            assert_eq!(digest(executable_bytes), executable_before);
            assert_eq!(resident_custody(&inactive), inactive_before);
            assert_eq!(resident_custody(&boot), boot_before);
            assert_eq!(resident_custody(&activated), activated_before);
            if prefix {
                assert!(name.starts_with("prefix-"));
                prefixes.insert(name.to_owned());
                prefix_calls += 1;
            } else {
                completed += 1;
            }
            calls += 1;
        }
    }
    assert_eq!(
        admitted,
        ADMITTED
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        skipped,
        DIAGNOSTICS
            .iter()
            .map(|v| (*v).to_owned())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        prefixes,
        ADMITTED
            .iter()
            .filter(|name| name.starts_with("prefix-"))
            .map(|v| (*v).to_owned())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        (admitted.len(), calls, completed, prefix_calls),
        (17, 54, 49, 5)
    );
}
