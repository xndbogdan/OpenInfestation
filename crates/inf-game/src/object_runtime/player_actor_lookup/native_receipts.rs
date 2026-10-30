//! Independent unchanged 0046B090 receipts over the actual player-list owner.
//!
//! Native manager, nodes and player bytes are declared fixture backing; the
//! native player constructor is not executed. Rust uses its real constructor,
//! followed by the supplied complete +D4/+D8 fields. Registry construction NULL
//! is independent of the borrowed process NULL. No actor, resolver, callback,
//! particle pool or task is needed by this pure lookup body.
//! The no-write claim covers declared process/player/list regions; native
//! PUSH and other stack effects are outside those storage comparisons.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest};
use crate::{
    object_runtime::{ActorHandle, ActorRegistryConfig, ObjectArena, PlayerId},
    process_particles::factories::ParticleFactoryProcessState,
};

// Deliberately unconsumed test carriers, rather than inferred process defaults.
// Every invocation checks the whole owner unchanged.
const UNUSED_NETWORK: u32 = 0x9126_4073;
const PLAYER_NAMES: [&str; 3] = ["first", "second", "third"];
const NATIVE_PLAYERS: [u32; 3] = [0x0074_8800, 0x0074_8a00, 0x0074_8c00];
const DIAGNOSTICS: [&str; 4] = [
    "diagnostic-bypassed-unknown-d4",
    "diagnostic-null-manager",
    "diagnostic-raw-player-links",
    "diagnostic-unknown-player-dwords",
];
const PREFIXES: [&str; 3] = [
    "empty-list-missing-process-owner",
    "empty-list-owned-missing-null",
    "populated-list-missing-process-owner",
];

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

fn optional_number(value: &Value) -> Option<u32> {
    (!value.is_null()).then(|| number(value))
}

fn bytes(value: &Value) -> Vec<Option<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| optional_number(value).map(|value| value.try_into().unwrap()))
        .collect()
}

fn dword(value: &[Option<u8>]) -> Option<u32> {
    assert_eq!(value.len(), 4);
    Some(u32::from_le_bytes(
        value
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .unwrap(),
    ))
}

fn field(value: &Value, offset: usize, width: usize) -> Vec<Option<u8>> {
    let raw = value.as_array().unwrap();
    bytes(&Value::Array(raw[offset..offset + width].to_vec()))
}

struct Fixture {
    arena: ObjectArena,
    players: BTreeMap<String, PlayerId>,
    process: ParticleFactoryProcessState,
    process_owner_present: bool,
}

impl Fixture {
    fn from_supplied_players(
        registry_null: Option<u32>,
        players: &[(String, u32, u32)],
        process_null: Option<u32>,
    ) -> Self {
        let mut arena = match registry_null {
            Some(null) => ObjectArena::with_actor_registry(ActorRegistryConfig {
                owner_count: 3,
                initial_capacity: 8,
                null_handle: ActorHandle::from_raw(null),
            })
            .unwrap(),
            None => ObjectArena::new(),
        };
        let mut identities = BTreeMap::new();
        for (name, d4, d8) in players {
            let id = arena.add_world_player([0; 6]);
            let player = arena.world_player_mut(id).unwrap();
            player.handle_d4 = ActorHandle::from_raw(*d4);
            player.handle_d8 = ActorHandle::from_raw(*d8);
            assert!(identities.insert(name.clone(), id).is_none());
        }
        Self {
            arena,
            players: identities,
            process: ParticleFactoryProcessState {
                network_flag: UNUSED_NETWORK,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: process_null.map(ActorHandle::from_raw),
            },
            process_owner_present: true,
        }
    }

    fn new(input: &Value) -> Self {
        let players = input["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|player| {
                (
                    player["identity"].as_str().unwrap().to_owned(),
                    number(&player["handle_d4"]),
                    number(&player["handle_d8"]),
                )
            })
            .collect::<Vec<_>>();
        let registry_null = input["registry_available"]
            .as_bool()
            .unwrap()
            .then(|| number(&input["registry_null"]));
        let mut fixture = Self::from_supplied_players(
            registry_null,
            &players,
            optional_number(&input["process_null"]),
        );
        fixture.process_owner_present = input["process_owner_present"].as_bool().unwrap();
        for patch in input["patches"].as_array().unwrap() {
            fixture.apply(patch);
        }
        fixture
    }

    fn apply(&mut self, patch: &Value) {
        let region = patch["region"].as_str().unwrap();
        let offset = number(&patch["offset"]);
        let supplied = bytes(&patch["bytes"]);
        if region == "process_null" {
            assert_eq!(offset, 0);
            assert_eq!(supplied.len(), 4);
            assert!(supplied.iter().all(Option::is_some) || supplied.iter().all(Option::is_none));
            self.process.process_null_handle = dword(&supplied).map(ActorHandle::from_raw);
        } else {
            assert!(matches!(region, "first" | "second" | "third"));
            assert!(matches!(offset, 0xd4 | 0xd8));
            let value = ActorHandle::from_raw(dword(&supplied).expect("complete supplied handle"));
            let player = self.arena.world_player_mut(self.players[region]).unwrap();
            match offset {
                0xd4 => player.handle_d4 = value,
                0xd8 => player.handle_d8 = value,
                _ => unreachable!("admitted player handle field"),
            }
        }
    }

    fn lookup(&self, handle: u32) -> Result<Option<PlayerId>, PlayerActorLookupError> {
        self.arena.find_player_for_actor_handle(
            PlayerActorLookupRequest {
                handle: ActorHandle::from_raw(handle),
            },
            PlayerActorLookupContext {
                process: self.process_owner_present.then_some(&self.process),
            },
        )
    }

    fn player_name(&self, id: PlayerId) -> &str {
        self.players
            .iter()
            .find_map(|(name, candidate)| (*candidate == id).then_some(name.as_str()))
            .expect("actual listed player identity")
    }

    fn native_read_order(&self, request: u32) -> Vec<Value> {
        let null = self
            .process_owner_present
            .then_some(self.process.process_null_handle)
            .flatten();
        let mut reads = vec![native_read("process_null", 0, 0x0046_b096, null.is_some())];
        let Some(null) = null else { return reads };
        if request == null.raw() {
            return reads;
        }
        reads.push(native_read("game_pointer", 0, 0x0046_b0a0, true));
        reads.push(native_read("game", 0x60, 0x0046_b0a9, true));
        let players = self.arena.world_players();
        if players.is_empty() {
            reads.push(native_read("game", 0x6c, 0x0046_b0ac, true));
            return reads;
        }
        for (index, player) in players.iter().enumerate() {
            let name = self.player_name(player.id);
            reads.push(native_read(
                name,
                0xc8,
                if index == 0 { 0x0046_b0ac } else { 0x0046_b0d2 },
                true,
            ));
            reads.push(native_read(name, 0xd0, 0x0046_b0b5, true));
            reads.push(native_read(name, 0xd8, 0x0046_b0b8, true));
            let selected = if player.handle_d8 == null {
                reads.push(native_read(name, 0xd4, 0x0046_b0c8, true));
                player.handle_d4
            } else {
                player.handle_d8
            };
            if selected.raw() == request {
                break;
            }
            reads.push(native_read(name, 0xc8, 0x0046_b0d0, true));
            if index + 1 == players.len() {
                reads.push(native_read("game", 0x6c, 0x0046_b0d2, true));
            }
        }
        reads
    }

    fn unchanged_snapshot(&self) -> (String, ParticleFactoryProcessState, bool) {
        // This whole typed-owner guard includes registry/cache/free-chain and
        // constructor fields that the native setup does not authenticate.
        // Their immutability is checked without claiming byte-layout parity.
        (
            format!("{:?}", self.arena),
            self.process,
            self.process_owner_present,
        )
    }
}

fn native_read(region: &str, offset: u32, ip: u32, known: bool) -> Value {
    json!({
        "kind":"read", "region":region, "offset":offset, "bytes":4,
        "known_mask":vec![known; 4], "ip":ip,
    })
}

fn patch_native_state(state: &mut Value, patch: &Value) {
    let region = patch["region"].as_str().unwrap();
    let raw = if region == "process_null" {
        &mut state["process_null"]
    } else {
        assert!(PLAYER_NAMES.contains(&region));
        &mut state["player_storage"][region]
    };
    let start = number(&patch["offset"]) as usize;
    let supplied = patch["bytes"].as_array().unwrap();
    raw.as_array_mut().unwrap()[start..start + supplied.len()].clone_from_slice(supplied);
}

fn compare_state(actual: &Fixture, native: &Value, expected_native: &Value) {
    // Keep all source masks, including bytes unavailable in the typed layout,
    // immutable across the body. Only explicitly supplied between-call patches
    // change this independent backing; no unknown native byte is filled in.
    assert_eq!(native, expected_native);
    assert_eq!(
        native["process_owner_present"],
        actual.process_owner_present
    );
    let null = bytes(&native["process_null"]);
    assert_eq!(null.len(), 4);
    assert!(null.iter().all(Option::is_some) || null.iter().all(Option::is_none));
    assert_eq!(
        dword(&null),
        actual.process.process_null_handle.map(ActorHandle::raw)
    );
    assert_eq!(
        native["registry"]["available"],
        actual.arena.actor_handles().is_some()
    );
    if let Some(registry) = actual.arena.actor_handles() {
        assert_eq!(native["registry"]["null"], registry.null_handle().raw());
    }
    assert_eq!(dword(&bytes(&native["game_pointer"])), Some(0x0074_9000));
    assert_eq!(native["game"].as_array().unwrap().len(), 0x78);
    assert_eq!(native["player_list"]["terminal"], "tail");
    let members: Vec<_> = actual
        .arena
        .world_players()
        .iter()
        .map(|player| json!({"player":actual.player_name(player.id)}))
        .collect();
    assert_eq!(native["player_list"]["members"], json!(members));
    let nodes: Vec<_> = actual
        .arena
        .world_players()
        .iter()
        .map(|player| native_player_pointer(actual.player_name(player.id)) + 0xc8)
        .collect();
    let head = 0x0074_9060;
    let tail = 0x0074_906c;
    assert_eq!(
        dword(&field(&native["game"], 0x60, 4)),
        Some(nodes.first().copied().unwrap_or(tail))
    );
    assert_eq!(dword(&field(&native["game"], 0x64, 4)), Some(0));
    assert_eq!(dword(&field(&native["game"], 0x68, 4)), Some(0));
    assert_eq!(dword(&field(&native["game"], 0x6c, 4)), Some(0));
    assert_eq!(
        dword(&field(&native["game"], 0x70, 4)),
        Some(nodes.last().copied().unwrap_or(head))
    );
    assert_eq!(dword(&field(&native["game"], 0x74, 4)), Some(0));
    assert!(field(&native["game"], 0, 0x60).iter().all(Option::is_none));
    let raw = native["player_storage"].as_object().unwrap();
    assert_eq!(raw.len(), PLAYER_NAMES.len());
    for name in PLAYER_NAMES {
        assert_eq!(raw[name].as_array().unwrap().len(), 0xdc);
        if let Some(id) = actual.players.get(name) {
            let player = actual.arena.world_player(*id).unwrap();
            assert_eq!(
                dword(&field(&raw[name], 0xd4, 4)),
                Some(player.handle_d4.raw())
            );
            assert_eq!(
                dword(&field(&raw[name], 0xd8, 4)),
                Some(player.handle_d8.raw())
            );
            let index = actual
                .arena
                .world_players()
                .iter()
                .position(|p| p.id == *id)
                .unwrap();
            assert_eq!(
                dword(&field(&raw[name], 0xc8, 4)),
                Some(nodes.get(index + 1).copied().unwrap_or(tail))
            );
            assert_eq!(
                dword(&field(&raw[name], 0xcc, 4)),
                Some(index.checked_sub(1).map(|i| nodes[i]).unwrap_or(head))
            );
            assert_eq!(
                dword(&field(&raw[name], 0xd0, 4)),
                Some(native_player_pointer(name))
            );
            assert!(field(&raw[name], 0, 0xc8).iter().all(Option::is_none));
        } else {
            assert!(bytes(&raw[name]).iter().all(Option::is_none));
        }
    }
    assert!(actual.arena.is_empty());
    assert!(actual.arena.actor_list().is_empty());
}

fn native_player_pointer(name: &str) -> u32 {
    NATIVE_PLAYERS[PLAYER_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap()]
}

fn compare_invocation(
    actual: &Fixture,
    step: &Value,
    native: &Value,
    result: Result<Option<PlayerId>, PlayerActorLookupError>,
) -> bool {
    let mut expected_events = step["patches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|patch| {
            json!({
                "kind":"supplied_effect", "region":patch["region"],
                "offset":patch["offset"], "bytes":patch["bytes"],
            })
        })
        .collect::<Vec<_>>();
    expected_events.push(json!({"kind":"original_body_entry","address":0x0046_b090}));
    expected_events.extend(actual.native_read_order(number(&step["handle"])));
    let stopped = match result {
        Ok(player) => {
            let identity = player.map(|id| actual.player_name(id));
            assert_eq!(native["result"], json!(identity));
            assert_eq!(
                native["native_pointer"],
                identity.map(native_player_pointer).unwrap_or(0)
            );
            assert_eq!(
                native["outcome"],
                json!({"kind":"original_body","stop":null})
            );
            expected_events
                .push(json!({"kind":"original_return","ip":0x0046_b0df,"player":identity}));
            false
        }
        Err(PlayerActorLookupError::MissingProcessNullHandle) => {
            assert!(native["result"].is_null());
            assert!(native["native_pointer"].is_null());
            assert_eq!(
                native["outcome"],
                json!({"kind":"semantic_prefix","stop":"missing_process_null"})
            );
            // The oracle deliberately stops at this reached required read;
            // the typed error does not claim an unmapped native fault or IP.
            expected_events.push(json!({
                "kind":"semantic_boundary", "operation":"process_null_read",
                "reason":"missing_process_null", "owner_present":actual.process_owner_present,
                "ip":0x0046_b096,
            }));
            true
        }
    };
    assert_eq!(native["events"], json!(expected_events));
    stopped
}

fn authenticate_fixture(fixture: &Value) {
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(fixture["floating_point_control_word"], 0x027f);
    assert_eq!(fixture["entry"], 0x0046_b090);
    assert_eq!(fixture["setup"], "supplied manager/player-node backing and complete D4/D8 inputs; no player/actor constructor execution");
    assert_eq!(fixture["registry_scope"], "explicit portable construction inputs only; no physical registry reads, cache updates or required actor allocations");
    assert_eq!(fixture["player_list_scope"], "physical supplied G+60 backing and exact node identities; disconnected managerNULL/raw-link backing in diagnostics is not admitted as a live typed list");
    assert_eq!(fixture["return_convention"], "fullDWORD EAX player pointer or0; normalized to supplied player identity, never Boolean AL");
    assert_eq!(
        fixture["body_guard"],
        json!({"size":82,"sha256":"18dfb4b22c6657b39616026564af78c321ee059ad2c8a25da2b6c0c2ac975ee8"})
    );
    assert_eq!(
        fixture["native_bindings"],
        json!({
            "players":{"first":0x748800,"second":0x748a00,"third":0x748c00},
            "game":0x749000,"player_head":0x749060,"player_tail":0x74906c,"node_offset":0xc8,
            "game_pointer":0x55f160,"process_null":0x54e894,
        })
    );
    for (key, expected) in [
        ("history_count", 20),
        ("invocation_count", 31),
        ("original_body_entry_count", 31),
        ("original_return_count", 25),
        ("native_fault_count", 0),
        ("original_history_count", 13),
        ("semantic_prefix_history_count", 3),
        ("diagnostic_history_count", 4),
        ("admitted_history_count", 16),
        ("admitted_invocation_count", 24),
    ] {
        assert_eq!(fixture[key], expected, "provenance count {key}");
    }
    assert_eq!(
        fixture["lower_entry_counts"],
        json!({"0047AD90":0,"00462190":0,"00488AA0":0})
    );
    assert_eq!(
        fixture["event_counts"],
        json!({
            "diagnostic_boundary":1,"original_body_entry":31,"original_return":25,
            "read":185,"semantic_boundary":3,"supplied_effect":9,
        })
    );
    assert_eq!(
        fixture["invocation_outcomes"],
        json!({"original_body":21,"semantic_prefix":3,"source_diagnostic":7})
    );
    assert_eq!(
        fixture["stop_counts"],
        json!({
            "missing_process_null":3,"raw_null_player_head":1,"unknown_first_d4":1,"unknown_first_d8":1,
        })
    );
    assert_eq!(fixture["named_semantic_prefixes"], json!(PREFIXES));
    assert_eq!(fixture["named_diagnostics"], json!(DIAGNOSTICS));
}

#[test]
fn player_actor_lookup_matches_independent_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/player_actor_lookup_native.json"
    ))
    .unwrap();
    authenticate_fixture(&fixture);
    let mut histories = 0;
    let mut invocations = 0;
    let mut prefixes = Vec::new();
    let mut skipped = Vec::new();
    let mut skipped_calls = 0;
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    assert_eq!(
        cases
            .iter()
            .map(|case| case["steps"].as_array().unwrap().len())
            .sum::<usize>(),
        31
    );
    for case in cases {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        let evidence = case["evidence"].as_str().unwrap();
        assert_eq!(
            input["steps"].as_array().unwrap().len(),
            case["steps"].as_array().unwrap().len()
        );
        if evidence == "source_diagnostic" {
            assert!(DIAGNOSTICS.contains(&name), "unadmitted diagnostic {name}");
            skipped.push(name);
            skipped_calls += case["steps"].as_array().unwrap().len();
            continue;
        }
        if evidence == "semantic_prefix" {
            assert!(
                PREFIXES.contains(&name),
                "unadmitted required prefix {name}"
            );
        } else {
            assert_eq!(evidence, "original_body");
            assert!(!DIAGNOSTICS.contains(&name) && !PREFIXES.contains(&name));
        }
        let mut actual = Fixture::new(input);
        let mut expected_native = case["initial_state"].clone();
        compare_state(&actual, &case["initial_state"], &expected_native);
        for (step, native) in input["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["steps"].as_array().unwrap())
        {
            for patch in step["patches"].as_array().unwrap() {
                actual.apply(patch);
                patch_native_state(&mut expected_native, patch);
            }
            let before = actual.unchanged_snapshot();
            let result = actual.lookup(number(&step["handle"]));
            assert_eq!(actual.unchanged_snapshot(), before, "pure lookup {name}");
            if compare_invocation(&actual, step, native, result) {
                assert_eq!(evidence, "semantic_prefix");
                prefixes.push(name);
            } else {
                assert_eq!(evidence, "original_body");
            }
            compare_state(&actual, &native["state"], &expected_native);
            invocations += 1;
        }
        histories += 1;
    }
    prefixes.sort_unstable();
    skipped.sort_unstable();
    assert_eq!(prefixes, PREFIXES);
    assert_eq!(skipped, DIAGNOSTICS);
    assert_eq!(skipped_calls, 7);
    assert_eq!((histories, invocations), (16, 24));
}
