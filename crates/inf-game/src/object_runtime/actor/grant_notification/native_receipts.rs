//! Actual canonical event22 and zero-radius broadcast result/state parity.
//!
//! Genuine constructors and live arena/player/process owners receive only declared
//! inputs. Native expected writes enter cloned assertion data after the API call.
//! Original query/lookup/callback cadence is audited in the sibling evidence
//! phase; no Rust cadence observer or detached grant scanner is installed.

use inf_formats::{
    parse_frontend_sine_table, parse_grant_table, parse_terrain_bank, ByteSpan, FrontendExecutable,
    HandlerFraming, HandlerId, HandlerRegion, FRONTEND_SINE_TABLE_ADDRESS,
    TERRAIN_ENTRY_FIXED_BYTES,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::native_evidence::{
    bytes, known_word, mask, number, table, NativeEvidence, DIAGNOSTICS, PREFIXES,
};
use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, CameraSettings, DerivedActorConstructionRequest, ObjectClass,
        ObjectState, PlayerId, Selector12GrantContext, Selector12State,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};

/// Complete genuine constructor preparation, followed by declared fixture
/// scalars elsewhere. This phase does not execute a native constructor or
/// substitute a table patch for an unowned class.
fn construct_actor(
    arena: &mut ObjectArena,
    binding: &str,
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
    // Constructor-only fixture seed; none of the admitted event22 bodies
    // receives this value or draws from it.
    let mut constructor_rng = 0;
    match binding {
        "derived" => arena.allocate_derived_actor(request, &mut constructor_rng),
        "family" => arena.allocate_family428650_actor(request, &mut constructor_rng),
        "probe_base" => arena.allocate_family429e20_actor(request, &mut constructor_rng),
        "family4236" => arena.allocate_family4236d0_actor(request, &mut constructor_rng),
        "selector10" => arena.allocate_selector10_actor(request, &mut constructor_rng),
        "selector34" => arena.allocate_selector34_actor(request, &mut constructor_rng),
        "selector35" => arena.allocate_selector35_actor(request, &mut constructor_rng),
        "selector36" => arena.allocate_selector36_actor(request, &mut constructor_rng),
        "selector11" => arena.allocate_selector11_actor(request, &mut constructor_rng),
        "selector2" => {
            // Selector2 really requires supplied H12/origin. This complete
            // construction carrier is not a broadcast XYZ/default input.
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
        "selector60" => arena.allocate_selector60_actor(request, &mut constructor_rng),
        "selector77" => arena.allocate_selector77_actor(request, &mut constructor_rng),
        "selector12" => {
            assert!(arena.first_player().is_none());
            let grants = parse_grant_table(executable).unwrap();
            arena.allocate_selector12_actor(
                request,
                &mut constructor_rng,
                Selector12GrantContext {
                    table: &grants,
                    first_player_grants: None,
                },
            )
        }
        "shared" => arena.allocate_actor(actor),
        "selector16" => arena.allocate_selector16(
            actor,
            CameraSettings {
                first_person: 0,
                camera_style: 0,
            },
        ),
        "selector30" => arena.allocate_selector30_actor(actor),
        "selector88" => arena.allocate_selector88(actor),
        "selector90" => arena.allocate_selector90(actor),
        other => panic!("unadmitted constructor {other}"),
    }
    .unwrap()
}

fn selector_in_state(state: &mut ObjectState) -> &mut Selector12State {
    let ObjectClass::Actor(actor) = &mut state.class else {
        panic!("actor")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("derived")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("family")
    };
    let Family428650Class::Family429e20(probe) = &mut family.class else {
        panic!("probe")
    };
    let Family429e20Class::Selector12(selector) = &mut probe.class else {
        panic!("selector12")
    };
    selector
}

/// Complete arena custody outside object payloads. Full live payloads are
/// separately compared; this includes registry/cache/free chain, player IDs,
/// allocation generations and memberships, including parked/disposed nodes.
fn custody_snapshot(arena: &ObjectArena) -> String {
    let allocations: Vec<_> = arena
        .slots
        .iter()
        .map(|slot| {
            (
                slot.generation,
                slot.value.as_ref().map(|owned| owned.memberships),
            )
        })
        .collect();
    format!(
        "{:?}",
        (
            (
                &arena.identity,
                &arena.free,
                &arena.live_count,
                &arena.lists,
                &arena.list_owners,
                &arena.routes,
                &arena.actors,
                &arena.map_annotations,
                &arena.actor_handles,
            ),
            (
                &arena.actor_payloads,
                &arena.constructor_allocations,
                &arena.selector60_rows,
                &arena.players,
                &arena.world_load,
                allocations,
            )
        )
    )
}

struct Actual {
    arena: ObjectArena,
    actors: BTreeMap<String, ObjectId>,
    bindings: BTreeMap<String, String>,
    players: BTreeMap<String, PlayerId>,
    process: ParticleFactoryProcessState,
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
    process_owner_present: bool,
}

impl Actual {
    fn new(input: &Value, executable: FrontendExecutable<'_>, sine: &[u16; 4096]) -> Self {
        let mut actual = Self {
            arena: ObjectArena::with_actor_registry(ActorRegistryConfig {
                owner_count: 2,
                initial_capacity: 8,
                null_handle: ActorHandle::from_raw(number(&input["registry_null"])),
            })
            .unwrap(),
            actors: BTreeMap::new(),
            bindings: BTreeMap::new(),
            players: BTreeMap::new(),
            // These unused values are explicit test sentinels, not required
            // native inputs or a duplicate session/network owner.
            phase:
                crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(
                    None,
                ),
            process: ParticleFactoryProcessState {
                network_flag: 0xa5a5_0100,
                local_slot: 1,
                host_flag: 0,
                settings: [0; 20],
                process_null_handle: input["process_null"]
                    .as_u64()
                    .map(|value| ActorHandle::from_raw(value.try_into().unwrap())),
            },
            process_owner_present: input["process_owner_present"].as_bool().unwrap(),
        };
        let declared = input["actors"].as_array().unwrap();
        for name in input["actor_order"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            let data = declared
                .iter()
                .find(|actor| actor["identity"] == name)
                .unwrap();
            assert_eq!(data["live"], true);
            let binding = data["binding"].as_str().unwrap();
            let actor = construct_actor(&mut actual.arena, binding, executable, sine);
            assert!(actual.actors.insert(name.to_owned(), actor).is_none());
            actual.bindings.insert(name.to_owned(), binding.to_owned());
            // Membership seed uses the real bit owners. All other supplied
            // bits/classification/lifecycle scalars remain literal fields,
            // not implicit list-routing or virtual-binding selectors.
            let flags = number(&data["flags2c"]);
            actual.arena.set_flag0(actor, flags & 1 != 0).unwrap();
            actual.arena.set_flag1(actor, flags & 2 != 0).unwrap();
            {
                let state = &mut actual.arena.owned_mut(actor).unwrap().state.base;
                state.flags = flags;
                state.dirty_word = number(&data["dirty38"]).try_into().unwrap();
                let position = bytes(&data["position6"]);
                assert_eq!(position.len(), 6);
                assert!(
                    position.iter().all(Option::is_some) || position.iter().all(Option::is_none)
                );
                state.position_words =
                    position
                        .iter()
                        .copied()
                        .collect::<Option<Vec<_>>>()
                        .map(|bytes| {
                            std::array::from_fn(|index| {
                                u16::from_le_bytes(
                                    bytes[2 * index..2 * index + 2].try_into().unwrap(),
                                )
                            })
                        });
            }
            let state = actual.arena.actor_mut(actor).unwrap();
            state.handle = ActorHandle::from_raw(number(&data["handle84"]));
            state.word_b4 = number(&data["classification_b4"]);
            state.word_c4 = number(&data["lifecycle_c4"]);
            if binding == "selector12" {
                let selector = selector_in_state(&mut actual.arena.owned_mut(actor).unwrap().state);
                selector.grants_24c.mask_04 = mask(&data["grants6"]);
                selector.byte_29d = number(&data["byte29d"]).try_into().unwrap();
            }
        }
        assert_eq!(actual.actors.len(), declared.len());
        for player in input["players"].as_array().unwrap() {
            actual.append_player(player);
        }
        // A declared unavailable registry is a reached owner absence after
        // genuine construction; no registry default or fake constructor is
        // used. Any subsequent actual destroy must have its owner available.
        if !input["registry_available"].as_bool().unwrap() {
            actual.arena.actor_handles = None;
        }
        for patch in input["patches"].as_array().unwrap() {
            actual.patch(patch);
        }
        actual
    }

    fn append_player(&mut self, input: &Value) {
        let name = input["identity"].as_str().unwrap();
        assert!(matches!(name, "first" | "second"));
        let player = self.arena.add_world_player(mask(&input["grants6"]));
        assert!(self.players.insert(name.to_owned(), player).is_none());
        let state = self
            .arena
            .players
            .list
            .iter_mut()
            .find(|entry| entry.id == player)
            .unwrap();
        state.handle_d4 = ActorHandle::from_raw(number(&input["handle_d4"]));
        state.handle_d8 = ActorHandle::from_raw(number(&input["handle_d8"]));
    }

    fn patch(&mut self, patch: &Value) {
        let region = patch["region"].as_str().unwrap();
        let offset = number(&patch["offset"]) as usize;
        let data = bytes(&patch["bytes"]);
        if region == "process_null" {
            assert_eq!((offset, data.len()), (0, 4));
            assert!(data.iter().all(Option::is_some) || data.iter().all(Option::is_none));
            self.process.process_null_handle = known_word(&data).map(ActorHandle::from_raw);
            return;
        }
        if let Some(&actor) = self.actors.get(region) {
            match (offset, data.len()) {
                (0x32, 6) => {
                    assert!(data.iter().all(Option::is_some) || data.iter().all(Option::is_none));
                    self.arena
                        .owned_mut(actor)
                        .unwrap()
                        .state
                        .base
                        .position_words = data.into_iter().collect::<Option<Vec<_>>>().map(|raw| {
                        std::array::from_fn(|index| {
                            u16::from_le_bytes(raw[2 * index..2 * index + 2].try_into().unwrap())
                        })
                    });
                }
                (0x38, 2) => {
                    let raw: Vec<_> = data.into_iter().map(Option::unwrap).collect();
                    self.arena.owned_mut(actor).unwrap().state.base.dirty_word =
                        u16::from_le_bytes(raw.try_into().unwrap());
                }
                (0x2c | 0x84 | 0xb4 | 0xc4, 4) => {
                    let word = known_word(&data).unwrap();
                    match offset {
                        0x2c => self.arena.owned_mut(actor).unwrap().state.base.flags = word,
                        0x84 => {
                            self.arena.actor_mut(actor).unwrap().handle =
                                ActorHandle::from_raw(word)
                        }
                        0xb4 => self.arena.actor_mut(actor).unwrap().word_b4 = word,
                        0xc4 => self.arena.actor_mut(actor).unwrap().word_c4 = word,
                        _ => unreachable!(),
                    }
                }
                (0x250, 24) => {
                    let selector =
                        selector_in_state(&mut self.arena.owned_mut(actor).unwrap().state);
                    selector.grants_24c.mask_04 = std::array::from_fn(|index| {
                        known_word(&data[4 * index..4 * index + 4]).unwrap()
                    });
                }
                (0x29d, 1) => {
                    selector_in_state(&mut self.arena.owned_mut(actor).unwrap().state).byte_29d =
                        data[0].unwrap();
                }
                _ => panic!(
                    "unadmitted actor patch {region}+{offset:x}, {} bytes",
                    data.len()
                ),
            }
            return;
        }
        let id = self.players[region];
        let player = self
            .arena
            .players
            .list
            .iter_mut()
            .find(|player| player.id == id)
            .unwrap();
        match (offset, data.len()) {
            (0xa0, 24) => {
                player.grants_9c = std::array::from_fn(|index| {
                    known_word(&data[4 * index..4 * index + 4]).unwrap()
                });
            }
            (0xd4, 4) => player.handle_d4 = ActorHandle::from_raw(known_word(&data).unwrap()),
            (0xd8, 4) => player.handle_d8 = ActorHandle::from_raw(known_word(&data).unwrap()),
            _ => panic!("unadmitted player patch {region}+{offset:x}"),
        }
    }

    fn effects(&mut self, step: &Value) {
        for effect in step["setup_effects"].as_array().unwrap() {
            match effect["operation"].as_str() {
                Some("append_player") => self.append_player(&effect["player"]),
                Some("set_flag0") => self
                    .arena
                    .set_flag0(
                        self.actors[effect["actor"].as_str().unwrap()],
                        effect["enabled"].as_bool().unwrap(),
                    )
                    .unwrap(),
                Some("set_flag1") => self
                    .arena
                    .set_flag1(
                        self.actors[effect["actor"].as_str().unwrap()],
                        effect["enabled"].as_bool().unwrap(),
                    )
                    .unwrap(),
                Some("dispose") => self
                    .arena
                    .dispose(self.actors[effect["actor"].as_str().unwrap()])
                    .unwrap(),
                Some("destroy_shared") => {
                    let name = effect["actor"].as_str().unwrap();
                    assert_eq!(self.bindings[name], "shared");
                    let id = self.actors[name];
                    let handle = self.arena.actor(id).unwrap().handle;
                    let owner = self
                        .arena
                        .actor_handles()
                        .unwrap()
                        .owner(handle.owner())
                        .unwrap();
                    assert_eq!(owner.slots()[usize::from(handle.slot())].object, Some(id));
                    self.arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: id,
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                    assert_eq!(
                        self.arena.object(id).unwrap_err(),
                        ObjectError::UnknownObject(id)
                    );
                    assert!(!self.arena.actor_list().contains(&id));
                    assert_eq!(
                        self.arena
                            .actor_handles()
                            .unwrap()
                            .owner(handle.owner())
                            .unwrap()
                            .slots()[usize::from(handle.slot())]
                        .object,
                        None
                    );
                }
                None => self.patch(effect),
                other => panic!("unadmitted actual effect {other:?}"),
            }
        }
    }

    fn compare(&self, state: &Value) {
        let names: Vec<_> = self
            .arena
            .actor_list()
            .iter()
            .map(|id| {
                self.actors
                    .iter()
                    .find(|(_, stored)| *stored == id)
                    .unwrap()
                    .0
                    .as_str()
            })
            .collect();
        assert_eq!(state["actor_order"], json!(names));
        for (name, &id) in &self.actors {
            let live = self.arena.object(id).is_ok();
            assert_eq!(state["actor_live"][name], live);
            if !live {
                continue;
            } // Retired physical bytes are not a live allocation.
            let raw = bytes(&state["storage"][name]);
            let object = self.arena.object(id).unwrap();
            let actor = self.arena.actor(id).unwrap();
            assert_eq!(known_word(&raw[..4]), Some(table(&self.bindings[name]).0));
            for (offset, word) in [
                (0x2c, object.base.flags),
                (0x84, actor.handle.raw()),
                (0xb4, actor.word_b4),
                (0xc4, actor.word_c4),
            ] {
                assert_eq!(known_word(&raw[offset..offset + 4]), Some(word));
            }
            assert_eq!(
                &raw[0x38..0x3a],
                &object.base.dirty_word.to_le_bytes().map(Some)
            );
            let position: Vec<_> = object
                .base
                .position_words
                .map(|xyz| {
                    xyz.into_iter()
                        .flat_map(|word| word.to_le_bytes().map(Some))
                        .collect()
                })
                .unwrap_or_else(|| vec![None; 6]);
            assert_eq!(raw[0x32..0x38], position);
            if self.bindings[name] == "selector12" {
                let mut object = object.clone();
                let selector = selector_in_state(&mut object);
                assert_eq!(raw[0x29d], Some(selector.byte_29d));
                for (i, &word) in selector.grants_24c.mask_04.iter().enumerate() {
                    assert_eq!(known_word(&raw[0x250 + 4 * i..0x254 + 4 * i]), Some(word));
                }
            } else {
                assert!(raw[0x250..0x268].iter().all(Option::is_none));
                assert_eq!(raw[0x29d], None);
            }
        }
        let player_names: Vec<_> = self
            .arena
            .world_players()
            .iter()
            .map(|player| {
                self.players
                    .iter()
                    .find(|(_, id)| **id == player.id)
                    .unwrap()
                    .0
                    .as_str()
            })
            .collect();
        assert_eq!(state["player_order"], json!(player_names));
        assert_eq!(
            self.arena.first_player(),
            self.arena.world_players().first().map(|player| player.id)
        );
        for (name, &id) in &self.players {
            let player = self.arena.world_player(id).unwrap();
            let raw = bytes(&state["storage"][name]);
            for (i, &word) in player.grants_9c.iter().enumerate() {
                assert_eq!(known_word(&raw[0xa0 + 4 * i..0xa4 + 4 * i]), Some(word));
            }
            assert_eq!(known_word(&raw[0xd4..0xd8]), Some(player.handle_d4.raw()));
            assert_eq!(known_word(&raw[0xd8..0xdc]), Some(player.handle_d8.raw()));
        }
        assert_eq!(state["process_owner_present"], self.process_owner_present);
        assert_eq!(
            known_word(&bytes(&state["storage"]["process_null"])),
            self.process.process_null_handle.map(ActorHandle::raw)
        );
        assert_eq!(self.process.network_flag, 0xa5a5_0100);
        assert_eq!(self.phase.snapshot(), None);
        assert_eq!(
            state["registry"]["available"],
            self.arena.actor_handles().is_some()
        );
        if let Some(registry) = self.arena.actor_handles() {
            assert_eq!(state["registry"]["null"], registry.null_handle().raw());
        }
    }

    fn invoke(&mut self, input: &Value, receipt: &Value) {
        self.effects(input);
        let before = custody_snapshot(&self.arena);
        let before_process = self.process;
        let mut expected: Vec<_> = self
            .arena
            .actor_list()
            .iter()
            .map(|&id| (id, self.arena.object(id).unwrap().clone()))
            .collect();
        let delivery = match input["delivery"].as_str().unwrap() {
            "actor" => {
                ActorGrantDifferenceDelivery::Actor(self.actors[input["actor"].as_str().unwrap()])
            }
            "actor_list" => ActorGrantDifferenceDelivery::ActorList,
            other => panic!("delivery {other}"),
        };
        let result = self.arena.notify_actor_grant_difference(
            ActorGrantDifferenceNotificationRequest { delivery },
            PlayerActorLookupContext {
                process: self.process_owner_present.then_some(&self.process),
            },
        );
        if receipt["outcome"]["stop"].is_null() {
            match result {
                Ok(value) => assert_eq!(receipt["result"], value),
                Err(error) => panic!("admitted receiver stopped: {error}"),
            }
        } else {
            assert_eq!(receipt["outcome"]["stop"], "missing_process_null");
            assert!(matches!(
                result,
                Err(ActorGrantDifferenceNotificationError::PlayerLookup(
                    PlayerActorLookupError::MissingProcessNullHandle
                ))
            ));
            assert!(receipt["result"].is_null());
        }
        // Native writes enter only cloned post-call assertion data. No grant
        // scan, expected result or physical pointer is fed to actual owners.
        for (id, object) in &mut expected {
            let name = self
                .actors
                .iter()
                .find(|(_, stored)| **stored == *id)
                .unwrap()
                .0;
            if self.bindings[name] == "selector12" {
                selector_in_state(object).byte_29d =
                    bytes(&receipt["state"]["storage"][name])[0x29d].unwrap();
            }
            assert_eq!(self.arena.object(*id).unwrap(), &*object);
        }
        assert_eq!(custody_snapshot(&self.arena), before);
        assert_eq!(self.process, before_process);
        self.compare(&receipt["state"]);
    }
}

#[test]
fn actor_grant_notification_matches_native_event22_broadcast_histories() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/actor_grant_notification_native.json"
    ))
    .unwrap();
    let Ok(raw) = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    ) else {
        eprintln!("retail Infest.exe missing; copy the game into retail/ to run this test");
        return;
    };
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut evidence = NativeEvidence::new(&fixture["metadata"], &raw, executable);
    let sine = parse_frontend_sine_table(executable.data(), FRONTEND_SINE_TABLE_ADDRESS)
        .unwrap()
        .values;
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 34);
    let mut skipped = BTreeSet::new();
    let (mut admitted, mut compared, mut prefixes, mut diagnostics) = (0, 0, 0, 0);
    for case in cases {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        assert_eq!(case["instruction_bytes_unchanged"], true);
        let evidence_kind = case["evidence"].as_str().unwrap();
        let diagnostic = DIAGNOSTICS.contains(&name);
        if diagnostic {
            assert_eq!(evidence_kind, "source_diagnostic");
            assert!(skipped.insert(name));
        } else {
            assert!(matches!(evidence_kind, "original_body" | "semantic_prefix"));
            assert_eq!(evidence_kind == "semantic_prefix", PREFIXES.contains(&name));
            admitted += 1;
        }
        evidence.audit_history(case);
        // Exact named diagnostic exclusion occurs before any typed constructor,
        // scalar admission or process/list preparation for that history.
        let mut actual = (!diagnostic).then(|| Actual::new(input, executable, &sine));
        if let Some(actual) = &actual {
            actual.compare(&case["initial_state"]);
        }
        let requests = input["steps"].as_array().unwrap();
        let receipts = case["steps"].as_array().unwrap();
        assert_eq!(requests.len(), receipts.len());
        for (request, receipt) in requests.iter().zip(receipts) {
            if let Some(actual) = &mut actual {
                actual.invoke(request, receipt);
                compared += 1;
                if !receipt["outcome"]["stop"].is_null() {
                    prefixes += 1;
                }
            } else {
                diagnostics += 1;
            }
        }
    }
    assert_eq!((admitted, compared, prefixes, diagnostics), (27, 56, 3, 10));
    assert_eq!(skipped, DIAGNOSTICS.into_iter().collect());
    evidence.finish();
}
