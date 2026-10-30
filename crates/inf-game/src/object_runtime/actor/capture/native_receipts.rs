//! Direct 0047D7C0 receipts with independent saved storage and live owners.
//!
//! Native list/registry/player backing is supplied by the fixture, rather than
//! produced by its actor/player constructors. The portable side uses genuine
//! constructors followed by those declared scalar inputs. Only 0046C760 is a
//! supplied mutable service; this does not establish any outer virtual+8C path.

use std::collections::BTreeMap;

use serde_json::{json, Value};

// This physical file is registered as a test-only child of actor_handles. That
// permits the declared cache backing to be seeded directly without widening a
// production API or executing an unrelated mark-mode operation to clear it.
use super::ActorHandleCache;
use crate::{
    object_runtime::{
        ActorCaptureContext, ActorCaptureError, ActorCaptureHost, ActorCapturePlayerCommand,
        ActorCaptureRequest, ActorCaptureSavedFlags, ActorConstructionError,
        ActorConstructionRequest, ActorExtension, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, ObjectArena, ObjectId, ObjectList, ObjectNodeKind, ObjectState,
        PlayerId,
    },
    process_particles::factories::ParticleFactoryProcessState,
};

const ACTOR_NAMES: [&str; 3] = ["actor", "source", "other"];
const LISTS: [(ObjectList, &str); 6] = [
    (ObjectList::Flag0Primary, "flag0_primary"),
    (ObjectList::Flag0Secondary, "flag0_secondary"),
    (ObjectList::Flag1Primary, "flag1_primary"),
    (ObjectList::Flag1Secondary, "flag1_secondary"),
    (ObjectList::Parking, "parking"),
    (ObjectList::Disposal, "disposal"),
];

// Explicit unconsumed test carriers, not inferred process defaults. The direct
// body has no phase/network consumer; every comparison checks these unchanged.
const UNUSED_NETWORK: u32 = 0x1973_2605;
const UNUSED_PHASE: u32 = 0x83ab_19d0;
const NATIVE_ACTORS: [u32; 3] = [0x0074_0000, 0x0074_1000, 0x0074_2000];
const NATIVE_GROUPS: u32 = 0x0074_8000;

const DIAGNOSTICS: [&str; 5] = [
    "diagnostic-cached-source-bypasses-unknown-group-backing",
    "diagnostic-saved-aliases-actor-flags",
    "diagnostic-unknown-actor-6c",
    "diagnostic-unknown-actor-flags",
    "diagnostic-unknown-source-6c",
];
const PREFIXES: [&str; 8] = [
    "capture-unavailable-registry-owner",
    "capture-unavailable-saved-storage",
    "mismatch-command-error-preserves-effects",
    "mismatch-requires-command",
    "release-known-low-byte-unknown-dword",
    "release-missing-current-null-after-merge",
    "release-unavailable-saved-storage",
    "release-unknown-low-byte",
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

fn known_dword(value: &[Option<u8>]) -> Option<u32> {
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

fn scalar(value: &Value, offset: usize) -> u32 {
    known_dword(&field(value, offset, 4)).expect("admitted complete native DWORD")
}

fn known_word(value: &[Option<u8>]) -> u16 {
    assert_eq!(value.len(), 2);
    u16::from_le_bytes(
        value
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()
            .unwrap()
            .try_into()
            .unwrap(),
    )
}

fn optional_field(value: &Value, offset: usize) -> Option<u32> {
    let supplied = field(value, offset, 4);
    assert!(supplied.iter().all(Option::is_some) || supplied.iter().all(Option::is_none));
    known_dword(&supplied)
}

fn saved(value: &Value) -> ActorCaptureSavedFlags {
    ActorCaptureSavedFlags {
        bytes: (!value.is_null()).then(|| bytes(value).try_into().unwrap()),
    }
}

fn allocation() -> ActorConstructionRequest<'static> {
    // These explicit construction carriers are outside the original-body
    // receipt. No position/terrain/RNG/service fallback enters the tested API.
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &[0; 4096],
        },
    }
}

struct Fixture {
    arena: ObjectArena,
    actors: BTreeMap<String, ObjectId>,
    players: BTreeMap<String, PlayerId>,
    process: ParticleFactoryProcessState,
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    saved: ActorCaptureSavedFlags,
}

impl Fixture {
    fn new(input: &Value) -> Self {
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 3,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(number(&input["registry_null"])),
        })
        .unwrap();
        let mut actors = BTreeMap::new();
        // Native supplied backing mirrors this per-actor order: constructor
        // flag1/flag0/lifecycle membership, then initialization0 followed by1.
        // Constructing every actor before every initializer changes Parking.
        for name in ACTOR_NAMES {
            let supplied = &input["actors"][name];
            assert_eq!(number(&supplied["vtable"]), 0x0052_7c00);
            let id = arena.allocate_actor(allocation()).unwrap();
            assert!(matches!(
                arena.actor(id).unwrap().extension,
                ActorExtension::Shared
            ));
            let mut handle = arena.actor(id).unwrap().handle;
            arena
                .actor_handles
                .as_mut()
                .unwrap()
                .reassign(
                    id,
                    &mut handle,
                    ActorHandle::from_raw(number(&supplied["handle"])),
                )
                .unwrap();
            arena.actor_mut(id).unwrap().handle = handle;
            let flags = number(&supplied["flags"]);
            arena.set_flag0(id, flags & 1 != 0).unwrap();
            arena.set_flag1(id, flags & 2 != 0).unwrap();
            let base = &mut arena.owned_mut(id).unwrap().state.base;
            base.flags = flags;
            base.dirty_word = number(&supplied["dirty_word"]).try_into().unwrap();
            let actor = arena.actor_mut(id).unwrap();
            actor.word_6c = number(&supplied["word_6c"]);
            actor.capture_handle_70 =
                optional_number(&supplied["capture_handle_70"]).map(ActorHandle::from_raw);
            actor.capture_flags_d4 = optional_number(&supplied["capture_flags_d4"]);
            actors.insert(name.to_owned(), id);
        }
        let mut players = BTreeMap::new();
        for supplied in input["players"].as_array().unwrap() {
            let identity = supplied["identity"].as_str().unwrap();
            assert!(matches!(identity, "first" | "second"));
            let id = arena.add_world_player([0; 6]);
            let player = arena
                .players
                .list
                .iter_mut()
                .find(|player| player.id == id)
                .unwrap();
            player.handle_d4 = ActorHandle::from_raw(number(&supplied["handle_d4"]));
            player.handle_d8 = ActorHandle::from_raw(number(&supplied["handle_d8"]));
            assert!(players.insert(identity.to_owned(), id).is_none());
        }
        arena.actor_handles.as_mut().unwrap().cache = ActorHandleCache {
            handle: ActorHandle::from_raw(number(&input["cache"]["handle"])),
            object: input["cache"]["actor"].as_str().map(|name| actors[name]),
        };
        if !input["registry_available"].as_bool().unwrap() {
            // Required backing is removed only after genuine construction.
            // Ineligible capture/release must still bypass its admission.
            arena.actor_handles = None;
        }
        let mut fixture = Self {
            arena,
            actors,
            players,
            phase:
                crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(
                    Some(UNUSED_PHASE),
                ),
            process: ParticleFactoryProcessState {
                network_flag: UNUSED_NETWORK,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: optional_number(&input["process_null"])
                    .map(ActorHandle::from_raw),
            },
            saved: saved(&input["saved_flags"]),
        };
        for patch in input["patches"].as_array().unwrap() {
            apply_patch(
                &mut fixture.arena,
                &mut ActorCaptureContext {
                    process: &mut fixture.process,
                    saved_flags: &mut fixture.saved,
                },
                &fixture.actors,
                patch,
            );
        }
        fixture
    }

    fn actor_name(&self, id: ObjectId) -> &str {
        self.actors
            .iter()
            .find_map(|(name, &actor)| (id == actor).then_some(name.as_str()))
            .unwrap()
    }

    fn player_name(&self, id: PlayerId) -> &str {
        self.players
            .iter()
            .find_map(|(name, &player)| (id == player).then_some(name.as_str()))
            .unwrap()
    }

    fn typed_actors(&self) -> BTreeMap<String, ObjectState> {
        self.actors
            .iter()
            .filter_map(|(name, &id)| {
                self.arena
                    .object(id)
                    .ok()
                    .map(|actor| (name.clone(), actor.clone()))
            })
            .collect()
    }
}

/// Input-backed scalar patches are separate from membership/lifetime actions.
/// A raw flags write does not silently move either of the native flag nodes.
fn apply_patch(
    arena: &mut ObjectArena,
    context: &mut ActorCaptureContext<'_>,
    actors: &BTreeMap<String, ObjectId>,
    patch: &Value,
) {
    let region = patch["region"].as_str().unwrap();
    let offset = number(&patch["offset"]) as usize;
    let data = bytes(&patch["bytes"]);
    match region {
        "saved_flags" => {
            let storage = context
                .saved_flags
                .bytes
                .as_mut()
                .expect("supplied saved backing");
            storage[offset..offset + data.len()].copy_from_slice(&data);
        }
        "process_null" => {
            assert_eq!(offset, 0);
            assert_eq!(data.len(), 4);
            assert!(data.iter().all(Option::is_some) || data.iter().all(Option::is_none));
            context.process.process_null_handle = known_dword(&data).map(ActorHandle::from_raw);
        }
        name if ACTOR_NAMES.contains(&name) => {
            let id = actors[name];
            match offset {
                0x2c => arena.owned_mut(id).unwrap().state.base.flags = known_dword(&data).unwrap(),
                0x38 => {
                    assert_eq!(data.len(), 2);
                    arena.owned_mut(id).unwrap().state.base.dirty_word = u16::from_le_bytes(
                        data.iter()
                            .copied()
                            .collect::<Option<Vec<_>>>()
                            .unwrap()
                            .try_into()
                            .unwrap(),
                    );
                }
                0x6c => arena.actor_mut(id).unwrap().word_6c = known_dword(&data).unwrap(),
                0x70 => {
                    assert!(data.iter().all(Option::is_some) || data.iter().all(Option::is_none));
                    arena.actor_mut(id).unwrap().capture_handle_70 =
                        known_dword(&data).map(ActorHandle::from_raw);
                }
                0xd4 => {
                    assert!(data.iter().all(Option::is_some) || data.iter().all(Option::is_none));
                    arena.actor_mut(id).unwrap().capture_flags_d4 = known_dword(&data);
                }
                other => panic!("unadmitted complete actor scalar patch {name}+{other:X}"),
            }
        }
        other => panic!("unadmitted patch region {other}"),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CommandError {
    Required,
    Stopped(String),
}

struct Services<'a> {
    actors: &'a BTreeMap<String, ObjectId>,
    players: &'a BTreeMap<String, PlayerId>,
    plans: &'a [Value],
    consumed: usize,
    commands: Vec<Value>,
}

impl ActorCaptureHost for Services<'_> {
    type Error = CommandError;

    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut ActorCaptureContext<'_>,
        request: ActorCapturePlayerCommand,
    ) -> Result<(), Self::Error> {
        assert_eq!((request.code, request.value), (0x42, 0));
        assert_eq!(Some(request.player), arena.first_player());
        let identity = self
            .players
            .iter()
            .find_map(|(name, &id)| (id == request.player).then_some(name))
            .unwrap();
        self.commands.push(json!({
            "player":identity,
            "code":request.code,
            "value":request.value,
        }));
        let Some(plan) = self.plans.get(self.consumed) else {
            return Err(CommandError::Required);
        };
        self.consumed += 1;
        for effect in plan["effects"].as_array().unwrap() {
            apply_patch(arena, context, self.actors, effect);
        }
        match plan["error"].as_str() {
            Some(error) => Err(CommandError::Stopped(error.to_owned())),
            None => {
                assert!(plan["error"].is_null());
                Ok(())
            }
        }
    }
}

fn compare_unconsumed_state(
    fixture: &Fixture,
    before: &BTreeMap<String, ObjectState>,
    players_before: &[crate::object_runtime::WorldPlayer],
) {
    for (name, before) in before {
        let Ok(after) = fixture.arena.object(fixture.actors[name]) else {
            // Retirement is separately compared as live arena/list/registry
            // custody; retired raw bytes are not a live portable allocation.
            continue;
        };
        let mut after = after.clone();
        after.base.flags = before.base.flags;
        after.base.dirty_word = before.base.dirty_word;
        let crate::object_runtime::ObjectClass::Actor(after_actor) = &mut after.class else {
            panic!("shared actor class must survive direct body and admitted effects");
        };
        let crate::object_runtime::ObjectClass::Actor(before_actor) = &before.class else {
            unreachable!();
        };
        after_actor.word_6c = before_actor.word_6c;
        after_actor.capture_handle_70 = before_actor.capture_handle_70;
        after_actor.capture_flags_d4 = before_actor.capture_flags_d4;
        assert_eq!(&after, before, "unconsumed typed actor fields {name}");
    }
    assert_eq!(fixture.arena.world_players(), players_before);
    assert_eq!(fixture.process.network_flag, UNUSED_NETWORK);
    assert_eq!(fixture.phase.snapshot(), Some(UNUSED_PHASE));
}

fn native_actor(pointer: u32) -> Option<&'static str> {
    if pointer == 0 {
        return None;
    }
    Some(
        ACTOR_NAMES[NATIVE_ACTORS
            .iter()
            .position(|&actor| actor == pointer)
            .unwrap()],
    )
}

fn compare_state(fixture: &Fixture, state: &Value, initial_native: &Value) {
    for name in ACTOR_NAMES {
        let id = fixture.actors[name];
        let actor = fixture.arena.actor(id).unwrap();
        let base = &fixture.arena.object(id).unwrap().base;
        let backing = &state["actors"][name];
        assert_eq!(backing.as_array().unwrap().len(), 0xe8);
        assert!(matches!(actor.extension, ActorExtension::Shared));
        assert_eq!(scalar(backing, 0), 0x0052_7c00);
        assert_eq!(base.flags, scalar(backing, 0x2c), "flags {name}");
        assert_eq!(
            base.dirty_word,
            known_word(&field(backing, 0x38, 2)),
            "dirty {name}"
        );
        assert_eq!(actor.word_6c, scalar(backing, 0x6c), "identity {name}");
        assert_eq!(
            actor.handle.raw(),
            scalar(backing, 0x84),
            "registration {name}"
        );
        assert_eq!(
            actor.capture_handle_70.map(ActorHandle::raw),
            optional_field(backing, 0x70)
        );
        assert_eq!(actor.capture_flags_d4, optional_field(backing, 0xd4));
        // Known native node-link bytes are compared through the actual ordered
        // semantic list owners below, rather than guessed portable addresses.
        // Every other native actor byte remains explicitly uninitialized.
        let raw = bytes(backing);
        for (offset, byte) in raw.iter().enumerate() {
            let represented = [
                (0, 4),
                (4, 36),
                (0x2c, 4),
                (0x38, 2),
                (0x6c, 4),
                (0x70, 4),
                (0x84, 4),
                (0xa8, 12),
                (0xd4, 4),
            ]
            .iter()
            .any(|&(start, size)| (start..start + size).contains(&offset));
            assert!(
                represented || byte.is_none(),
                "unadmitted native byte {name}+{offset:X}"
            );
        }
    }
    assert_eq!(
        serde_json::to_value(fixture.saved.bytes).unwrap(),
        state["saved_flags"]
    );
    assert_eq!(
        fixture.process.process_null_handle.map(ActorHandle::raw),
        optional_field(&state["process_null"], 0)
    );
    assert_eq!(
        fixture.arena.actor_handles().is_some(),
        state["registry_available"].as_bool().unwrap()
    );
    if let Some(registry) = fixture.arena.actor_handles() {
        assert_eq!(registry.owners().len(), 3);
        assert_eq!(registry.initial_capacity(), 8);
        let owner = registry.owner(1).unwrap();
        assert_eq!(owner.capacity(), scalar(&state["groups"], 24 + 16) as usize);
        assert_eq!(owner.word_00 as u32, scalar(&state["groups"], 24));
        assert_eq!(owner.word_04 as u32, scalar(&state["groups"], 24 + 4));
        for slot in 1..=3 {
            let raw = &state["entries"];
            let entry = owner.slots()[slot];
            assert_eq!(entry.handle.raw(), scalar(raw, slot * 12));
            let expected = native_actor(scalar(raw, slot * 12 + 4));
            assert_eq!(entry.object.map(|id| fixture.actor_name(id)), expected);
        }
        let cache = registry.cache();
        assert_eq!(cache.handle.raw(), scalar(&state["resolver_cache"], 4));
        assert_eq!(
            cache.object.map(|id| fixture.actor_name(id)),
            native_actor(scalar(&state["resolver_cache"], 0))
        );
        assert_eq!(scalar(&state["resolver_cache"], 8), NATIVE_GROUPS);
        assert_eq!(
            scalar(&state["resolver_cache"], 12),
            registry.owners().len() as u32
        );
    }
    // Lookup writes only the cache. Raw native unused-slot/free-chain backing
    // is supplied support and remains unchanged; there is no constructor/free
    // body parity claim or reason to seed its words into the portable owner.
    assert_eq!(state["groups"], initial_native["groups"]);
    assert_eq!(state["entries"], initial_native["entries"]);
    for (role, name) in LISTS {
        let nodes: Vec<_> = fixture
            .arena
            .list(role)
            .iter()
            .map(|node| {
                json!({
                    "actor":fixture.actor_name(node.object),
                    "kind":match node.kind {
                        ObjectNodeKind::Flag0 => "flag0",
                        ObjectNodeKind::Flag1 => "flag1",
                        ObjectNodeKind::Lifecycle => "lifecycle",
                    },
                })
            })
            .collect();
        assert_eq!(
            json!(nodes),
            state["membership_lists"][name],
            "ordered {name}"
        );
    }
    let actors: Vec<_> = fixture
        .arena
        .actor_list()
        .iter()
        .map(|&id| {
            json!({
                "actor":fixture.actor_name(id),"kind":"actor",
            })
        })
        .collect();
    assert_eq!(json!(actors), state["actor_list"]);
    let players: Vec<_> = fixture
        .arena
        .world_players()
        .iter()
        .map(|player| {
            json!({
                "player":fixture.player_name(player.id),
            })
        })
        .collect();
    assert_eq!(json!(players), state["players"]);
    for name in ["first", "second"] {
        let raw = &state["player_storage"][name];
        assert_eq!(raw.as_array().unwrap().len(), 0xdc);
        if let Some(&id) = fixture.players.get(name) {
            let player = fixture
                .arena
                .world_players()
                .iter()
                .find(|player| player.id == id)
                .unwrap();
            assert_eq!(player.handle_d4.raw(), scalar(raw, 0xd4));
            assert_eq!(player.handle_d8.raw(), scalar(raw, 0xd8));
            for (offset, byte) in bytes(raw).iter().enumerate() {
                assert!(
                    (0xc8..0xdc).contains(&offset) || byte.is_none(),
                    "unadmitted native player byte {name}+{offset:X}"
                );
            }
        } else {
            assert!(bytes(raw).iter().all(Option::is_none));
        }
    }
}

fn compare_result(result: &Result<bool, ActorCaptureError<CommandError>>, native: &Value) {
    match result {
        Ok(value) => {
            assert_eq!(*value, native["result"].as_bool().unwrap());
            assert_eq!(*value, number(&native["raw_eax"]) as u8 != 0);
            assert_eq!(
                native["outcome"],
                json!({"kind":"original_body","stop":null})
            );
        }
        Err(error) => {
            assert!(native["result"].is_null());
            assert_eq!(native["outcome"]["kind"], "semantic_prefix");
            let expected = match error {
                ActorCaptureError::Construction(ActorConstructionError::MissingRegistry) => {
                    "missing_actor_registry".to_owned()
                }
                ActorCaptureError::MissingSavedFlagsStorage => {
                    "missing_saved_flags_storage".to_owned()
                }
                ActorCaptureError::UninitializedSavedFlags { offset } => {
                    format!("uninitialized_saved_flags_{offset}")
                }
                ActorCaptureError::MissingProcessNullHandle => "missing_process_null".to_owned(),
                ActorCaptureError::Host { address, error } => {
                    assert_eq!(*address, 0x0046_c760);
                    match error {
                        CommandError::Required => "required_player_command".to_owned(),
                        CommandError::Stopped(error) => format!("player_command_error_{error}"),
                    }
                }
                other => panic!("unadmitted typed capture error {other:?}"),
            };
            assert_eq!(native["outcome"]["stop"], expected);
        }
    }
}

fn authenticate_fixture(fixture: &Value) {
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 28);
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(
        fixture["text_sha256"],
        "403045b1d532f4dc9204801635a83356126bc5b9cd84d2cf771b834c671cb2a2"
    );
    assert_eq!(fixture["floating_point_control_word"], 0x027f);
    assert_eq!(fixture["entry"], 0x0047_d7c0);
    assert_eq!(fixture["direct_shared_body_only"], true);
    assert_eq!(fixture["return_convention"],
        "Boolean low BYTE AL; raw_eax is incidental native evidence, not a portable full-DWORD result");
    assert_eq!(
        fixture["body_guards"],
        json!({
            "0047D7C0":{"size":327,"sha256":"b7673938253cd92c74cc0f9be14b0b2b71ecdb35c03ac5d474eca1aefd72c86a"},
            "0047AD90":{"size":13,"sha256":"5bc4fc44fba4c6b1945c38f61570c479687c1b23e23f691c7e4d89c7db860271"},
            "00462190":{"size":127,"sha256":"2b2aca8473232d0d8f7ca4c7d9f080a1cbadddda0c26b210f4a397e37cbcc207"},
            "00479E90":{"size":129,"sha256":"ea94cbf02622d327d1353054162f78560f1766ab1471baa5df3323d1e1395fa3"},
            "00479F20":{"size":128,"sha256":"edd0f1932a68a99bed1f5ca7f6c3018d3d9162e4c4e6a991946cf07b865478ba"},
            "00488AA0":{"size":24,"sha256":"9b594118f5712139882504b45ab023d976170685230e7d3358ba064a2981f1a5"},
        })
    );
    assert_eq!(
        fixture["constructor_table_guard"],
        json!({
            "address":0x0052_7c00,"words":[0x0047_aa40,0x0047_ada0,0x0047_a610,0x0047_ad70],
        })
    );
    assert_eq!(
        fixture["native_bindings"],
        json!({
            "actors":{"actor":0x740000,"source":0x741000,"other":0x742000},
            "handles":{"actor":0x08010001,"source":0x08020001,"other":0x08030001},
            "players":{"first":0x748800,"second":0x748a00},
            "groups":0x748000,"entries":0x748200,"saved_flags":0x748400,
            "game":0x749000,"headers":0x749200,
            "lists":{"flag0_primary":0x749230,"flag0_secondary":0x749248,
                     "flag1_primary":0x749200,"flag1_secondary":0x749218,
                     "parking":0x749030,"disposal":0x749018},
            "node_offsets":{"flag0":4,"flag1":16,"lifecycle":28,"actor":168},
            "resolver_cache":0x54e880,"process_null":0x54e894,"game_pointer":0x55f160,
        })
    );
    // Static final-vtable bindings guard the direct/outer distinction. They
    // do not turn this Shared constructor corpus into a virtual-dispatch test.
    let tables = [
        ("derived", 0x00520e58, 0x0047d7c0),
        ("family", 0x00520f34, 0x0047d7c0),
        ("scalar_base", 0x0052104c, 0x0047d7c0),
        ("probe_base", 0x00521164, 0x0047d7c0),
        ("family4236", 0x00520b84, 0x0047d7c0),
        ("selector10", 0x00520cc8, 0x0047d7c0),
        ("selector34", 0x00521eac, 0x0047d7c0),
        ("selector35", 0x0052273c, 0x0047d7c0),
        ("selector36", 0x00527ee0, 0x0047d7c0),
        ("selector11", 0x0051f080, 0x0047d7c0),
        ("selector2", 0x00520860, 0x0047d7c0),
        ("selector60", 0x00527de8, 0x0047d7c0),
        ("selector77", 0x0051fbb0, 0x0047d7c0),
        ("selector12", 0x00527058, 0x00468220),
        ("selector13", 0x005269c4, 0x00468b30),
        ("shared", 0x00527c00, 0x0047d7c0),
        ("selector16", 0x005204d0, 0x0047d7c0),
        ("selector30", 0x005289c0, 0x0047d7c0),
        ("selector88", 0x00524ac0, 0x0047d7c0),
        ("selector90", 0x00526b50, 0x0047d7c0),
    ];
    let matrix: BTreeMap<_, _> = tables
        .into_iter()
        .map(|(name, table, entry)| (name, json!({"table":table,"offset":0x8c,"entry":entry})))
        .collect();
    assert_eq!(fixture["canonical_capture_slot_matrix"], json!(matrix));
    for (key, count) in [
        ("history_count", 28),
        ("invocation_count", 37),
        ("original_body_entry_count", 106),
        ("original_history_count", 15),
        ("semantic_prefix_history_count", 8),
        ("diagnostic_history_count", 5),
        ("admitted_history_count", 23),
        ("admitted_invocation_count", 32),
        ("completed_invocation_count", 26),
        ("stopped_invocation_count", 11),
        ("native_fault_count", 0),
    ] {
        assert_eq!(fixture[key], count, "provenance count {key}");
    }
    assert_eq!(
        fixture["original_body_entry_counts"],
        json!({
            "00462190":16,"00479E90":14,"00479F20":14,"0047AD90":16,"0047D7C0":37,"00488AA0":9,
        })
    );
    assert_eq!(
        fixture["event_counts"],
        json!({
            "first_player_return":9,"membership":28,"original_body_entry":106,
            "original_return":95,"read":583,"required_service":1,"resolve":16,
            "resolved":16,"semantic_boundary":6,"supplied_effect":24,"supplied_service":2,"write":318,
        })
    );
    assert_eq!(
        fixture["invocation_outcomes"],
        json!({"original_body":24,"semantic_prefix":8,"source_diagnostic":5})
    );
    assert_eq!(
        fixture["stop_counts"],
        json!({
            "missing_actor_registry":1,"missing_process_null":1,"missing_saved_flags_storage":2,
            "player_command_error_stopped":1,"required_player_command":1,
            "uninitialized_saved_flags_0":1,"uninitialized_saved_flags_1":1,
            "unknown_actor_2c":1,"unknown_actor_6c":1,"unknown_source_6c":1,
        })
    );
    assert_eq!(
        fixture["command_entry_provenance"],
        json!({"original_body":0,"required_service":1,"supplied_service":2})
    );
    assert_eq!(fixture["named_diagnostics"], json!(DIAGNOSTICS));
    assert_eq!(fixture["named_semantic_prefixes"], json!(PREFIXES));
}

fn compare_native_cadence(native: &Value, commands: &[Value]) {
    let mut native_commands = Vec::new();
    let mut players = Vec::new();
    let mut saved_reads = Vec::new();
    let mut members = Vec::new();
    let events = native["events"].as_array().unwrap();
    assert!(
        matches!(events.iter().find(|event|event["kind"] == "original_body_entry"),
        Some(event) if event["address"] == 0x0047_d7c0)
    );
    for event in events {
        match event["kind"].as_str().unwrap() {
            "original_body_entry" => assert!(matches!(
                number(&event["address"]),
                0x0047_d7c0 | 0x0047_ad90 | 0x0046_2190 | 0x0047_9e90 | 0x0047_9f20 | 0x0048_8aa0
            )),
            "original_return" => assert!(matches!(
                number(&event["ip"]),
                0x0047_d863
                    | 0x0047_d883
                    | 0x0047_d904
                    | 0x0047_ad9a
                    | 0x0046_21a4
                    | 0x0046_2206
                    | 0x0046_220c
                    | 0x0047_9ee4
                    | 0x0047_9f0e
                    | 0x0047_9f73
                    | 0x0047_9f9d
                    | 0x0048_8ab3
                    | 0x0048_8ab7
            )),
            "required_service" | "supplied_service" => {
                assert_eq!(event["operation"], "player_command_46c760");
                native_commands.push(event["request"].clone());
            }
            "first_player_return" => players.push(event["player"].clone()),
            "membership" => {
                assert_eq!(event["actor"], "actor");
                members.push(event["node"].as_str().unwrap());
            }
            "read" if event["region"] == "saved_flags" => {
                saved_reads.push((number(&event["ip"]), number(&event["bytes"])));
            }
            _ => {}
        }
    }
    assert_eq!(native_commands, commands);
    if !commands.is_empty() {
        assert_eq!(
            players,
            json!(["first", "first"]).as_array().unwrap().clone()
        );
    }
    assert!(matches!(
        members.as_slice(),
        [] | ["flag0"] | ["flag1"] | ["flag0", "flag1"]
    ));
    assert!(matches!(
        saved_reads.as_slice(),
        [] | [(0x47d899, 1)] | [(0x47d899, 1), (0x47d8a7, 1), (0x47d8d6, 4)]
    ));
}

#[test]
fn shared_actor_capture_matches_independent_native_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/actor_capture_native.json"
    ))
    .unwrap();
    authenticate_fixture(&fixture);
    let mut histories = 0;
    let mut invocations = 0;
    let mut prefixes = 0;
    let mut skipped = Vec::new();
    let mut seen_prefixes = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        let evidence = case["evidence"].as_str().unwrap();
        if evidence == "source_diagnostic" {
            assert!(DIAGNOSTICS.contains(&name), "unadmitted diagnostic {name}");
            skipped.push(name);
            assert_eq!(case["steps"].as_array().unwrap().len(), 1);
            continue;
        }
        if evidence == "semantic_prefix" {
            assert!(PREFIXES.contains(&name), "unadmitted prefix {name}");
            seen_prefixes.push(name);
        } else {
            assert_eq!(evidence, "original_body");
            assert!(!DIAGNOSTICS.contains(&name) && !PREFIXES.contains(&name));
        }
        let mut actual = Fixture::new(input);
        let initial_native = &case["initial_state"];
        compare_state(&actual, initial_native, initial_native);
        assert_eq!(
            input["steps"].as_array().unwrap().len(),
            case["steps"].as_array().unwrap().len()
        );
        for (step, native) in input["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["steps"].as_array().unwrap())
        {
            assert_eq!(step["saved_pointer"], "independent");
            let mut context = ActorCaptureContext {
                process: &mut actual.process,
                saved_flags: &mut actual.saved,
            };
            for patch in step["patches"].as_array().unwrap() {
                apply_patch(&mut actual.arena, &mut context, &actual.actors, patch);
            }
            let before = actual.typed_actors();
            let players_before = actual.arena.world_players().to_vec();
            let registry_before = actual
                .arena
                .actor_handles()
                .map(|registry| registry.owners().to_vec());
            let mut services = Services {
                actors: &actual.actors,
                players: &actual.players,
                plans: step["command_services"].as_array().unwrap(),
                consumed: 0,
                commands: Vec::new(),
            };
            let result = actual.arena.apply_shared_actor_capture(
                ActorCaptureRequest {
                    actor: actual.actors["actor"],
                    operation: number(&step["operation"]),
                    source: ActorHandle::from_raw(number(&step["source_handle"])),
                },
                &mut ActorCaptureContext {
                    process: &mut actual.process,
                    saved_flags: &mut actual.saved,
                },
                &mut services,
            );
            assert_eq!(
                services.consumed,
                services.plans.len(),
                "unused supplied service {name}"
            );
            compare_result(&result, native);
            compare_native_cadence(native, &services.commands);
            compare_unconsumed_state(&actual, &before, &players_before);
            assert_eq!(
                actual
                    .arena
                    .actor_handles()
                    .map(|registry| registry.owners().to_vec()),
                registry_before,
                "lookup must not change portable slot/free-chain owner {name}"
            );
            compare_state(&actual, &native["state"], initial_native);
            prefixes += usize::from(result.is_err());
            invocations += 1;
        }
        histories += 1;
    }
    skipped.sort_unstable();
    seen_prefixes.sort_unstable();
    assert_eq!(skipped, DIAGNOSTICS);
    assert_eq!(seen_prefixes, PREFIXES);
    assert_eq!((histories, invocations, prefixes), (23, 32, 8));
}
