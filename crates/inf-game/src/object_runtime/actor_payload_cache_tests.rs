use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use inf_formats::{parse_constructor_asset_bank, HANDLER_COUNT};

use super::*;
use crate::{
    object_runtime::{
        construct_world_object, ActorDamageTable, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, BaseContext, CreationListener, CreationListenerError,
    },
    Installation, ResidencyRequest, ResourceSelection,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
const UNUSED_SINE: [u16; 4096] = [0; 4096];

/// Canonical synthetic H13 registrations: slots 0 and 1 alias one record,
/// slot 2 names another. Difficulty replacement changes the actual allocation.
struct Fixture {
    root: PathBuf,
    runtime: ResidentRuntime,
}

impl Fixture {
    fn new() -> Self {
        Self::with_selector(88)
    }

    fn with_selector(selector: u16) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-actor-payload-alias-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            preload.extend_from_slice(&(if handler == 13 { 3_u16 } else { 0 }).to_le_bytes());
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for table in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let populated = table == 1 && handler == 13;
                preload.extend_from_slice(&(if populated { 3_i16 } else { 0 }).to_le_bytes());
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        for (variant, seeds) in [(0, [100_u16, 200_u16]), (1, [900_u16, 1000_u16])] {
            let mut data = vec![];
            for seed in seeds {
                data.extend_from_slice(&(selector - 1).to_le_bytes());
                data.extend_from_slice(&u16::MAX.to_le_bytes()); // Disabled H8.
                let mut payload = vec![0_u8; if selector == 88 { 0x42 } else { 0x58 }];
                payload[..4].copy_from_slice(&3_u32.to_le_bytes());
                payload[4..6].copy_from_slice(&seed.to_le_bytes());
                for channel in 0..10 {
                    let start = 0x10 + channel * 4;
                    payload[start..start + 2]
                        .copy_from_slice(&(seed + channel as u16).to_le_bytes());
                    payload[start + 2..start + 4].copy_from_slice(&256_u16.to_le_bytes());
                }
                if selector != 88 {
                    payload[0x44] = 0x81; // Two groups, mode 1 allocation guard.
                    payload[0x4a..0x4c].copy_from_slice(&(-1_i16).to_le_bytes());
                }
                data.extend_from_slice(&payload);
            }
            let mut overlay = (data.len() as u32).to_le_bytes().to_vec();
            overlay.extend_from_slice(&data);
            overlay.extend_from_slice(&0_u32.to_le_bytes()); // No relocations.
            for offset in [0_u32, 0, if selector == 88 { 0x46 } else { 0x5c }] {
                overlay.extend_from_slice(&offset.to_le_bytes());
            }
            fs::write(root.join(format!("Overlay/0d{variant}.ovl")), overlay).unwrap();
        }
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        Self { root, runtime }
    }

    fn select(&mut self, difficulty: u8) {
        self.runtime
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty,
                    language: 0,
                },
            })
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("inf-actor-payload-alias-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

struct NoBindings;

impl CreationListenerEffects for NoBindings {
    fn invoke(
        &mut self,
        _listener: CreationListener,
        _actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        panic!("the fixture has no registered listener")
    }
}

fn construct(arena: &mut ObjectArena, runtime: &ResidentRuntime, slot: u32) -> ObjectId {
    construct_with_derived(arena, runtime, slot, None, None)
}

fn construct_with_derived(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    slot: u32,
    derived: Option<DerivedActorFactoryInputs<'_>>,
    suffix: Option<crate::object_runtime::ObjectInstanceCommands<'_>>,
) -> ObjectId {
    let mut listeners = CreationListeners::new();
    // Neither terrain nor rotation is present, so this table is never read.
    let math = NativeBasisMath::new(UNUSED_SINE);
    let outcome = construct_world_object(
        arena,
        runtime,
        ObjectConstructionRequest {
            resource_argument: slot,
            context: BaseContext {
                position_words: Some([11, 22, 33]),
                placement_index: slot,
            },
            suffix,
            command_allocation: None,
            network_active: false,
            actor: Some(ActorFactoryInputs {
                terrain: None,
                world_start: None,
                derived,
                allocation: ActorConstructionRequest {
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
                rotation_words: None,
                local_owner: 1,
                first_player_grants: None,
                grant_table: None,
                camera_settings: None,
                spawn_mode_1: false,
                network_handle: None,
                vehicle_table: None,
                math: &math,
                listeners: &mut listeners,
                listener_effects: &mut NoBindings,
                network_world_active: false,
            }),
        },
    )
    .unwrap();
    let ObjectConstructionOutcome::Constructed(object) = outcome else {
        panic!("expected supported actor construction")
    };
    object
}

#[test]
fn derived_factory_runs_allocating_guard_even_without_suffix_and_owns_each_result() {
    use crate::{name_entry::name_entry_rng_next, object_runtime::INITIALIZED_FLAG};
    for selector in [7, 33, 77] {
        let fixture = Fixture::with_selector(selector);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 16,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let mut rng = 0x1234_5678;
        let mut expected_rng = rng;
        for slot in [0, 1] {
            name_entry_rng_next(&mut expected_rng);
            if selector == 77 {
                name_entry_rng_next(&mut expected_rng);
            }
            let object = construct_with_derived(
                &mut arena,
                &fixture.runtime,
                slot,
                Some(DerivedActorFactoryInputs {
                    world_mask_limits: [4, 4],
                    rng: &mut rng,
                }),
                None,
            );
            assert_eq!(rng, expected_rng);
            let state = arena.object(object).unwrap();
            assert_eq!(state.base.resource_word, slot as u16);
            assert_ne!(state.base.flags & INITIALIZED_FLAG, 0);
            assert_eq!(state.base.dirty_word, 0);
            let derived = arena.derived_actor(object).unwrap();
            assert_eq!(derived.word_164, -1);
            assert_eq!(derived.word_178, 15);
            let behavior = derived
                .behavior_140
                .as_ref()
                .expect("null suffix still runs guard");
            assert_eq!(behavior.groups_1c.len(), 2);
            assert_eq!(behavior.entries_18.len(), 2);
            if selector == 77 {
                assert_eq!(
                    arena.selector77_actor(object).unwrap().position_192,
                    Some([11, 22, 33])
                );
            }
            arena
                .initialize_after_load(object, Default::default())
                .unwrap();
        }
        let objects = arena.actor_list().to_vec();
        assert!(Arc::ptr_eq(
            retained(&arena, objects[0]),
            retained(&arena, objects[1])
        ));
        let first = arena
            .derived_actor(objects[0])
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        let second = arena
            .derived_actor(objects[1])
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        assert_ne!(first.entries_18.as_ptr(), second.entries_18.as_ptr());
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: objects[0],
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert!(arena
            .derived_actor(objects[1])
            .unwrap()
            .behavior_140
            .is_some());
    }
}

fn retained(arena: &ObjectArena, object: ObjectId) -> &Arc<ActorConstructorPayload> {
    let ActorDamageTable::Payload(payload) = &arena.actor(object).unwrap().damage_table else {
        panic!("factory must initialize the retained H13 table")
    };
    payload
}

#[test]
fn central_guard_precedes_commands_and_only_selector77_postinit_reselects_behavior() {
    let command = super::super::tests::Commands::new(1, &[(6, &2_i32.to_le_bytes())]);
    for selector in [7, 33, 77] {
        let fixture = Fixture::with_selector(selector);
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 16,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let mut rng = 1;
        let object = construct_with_derived(
            &mut arena,
            &fixture.runtime,
            0,
            Some(DerivedActorFactoryInputs {
                world_mask_limits: [0, 0],
                rng: &mut rng,
            }),
            Some(command.request()),
        );
        assert_eq!(arena.actor(object).unwrap().word_b4, 18);
        let behavior = arena
            .derived_actor(object)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        // Payload +B4=0 selected entry 1 before command 6. Only the final
        // selector-77 +1C revisits that choice after +B4 changes to 18.
        assert_eq!(behavior.byte_10, if selector == 77 { 0 } else { 1 });
    }
}

#[test]
fn aliased_resource_factories_share_payload_but_new_allocations_do_not() {
    let mut fixture = Fixture::new();
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    // Establish aliases from the canonical parser, not a test-only cache key.
    let resource = fixture.runtime.resource(13, 0).unwrap();
    let region = &resource.overlay.layout().regions[resource.registration.region_index];
    let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
    assert_eq!(
        bank.prefixes.entries[0].payload_offset,
        bank.prefixes.entries[1].payload_offset
    );
    assert_ne!(
        bank.prefixes.entries[0].payload_offset,
        bank.prefixes.entries[2].payload_offset
    );
    let old_source = Arc::downgrade(resource.overlay);

    let first = construct(&mut arena, &fixture.runtime, 0);
    let alias = construct(&mut arena, &fixture.runtime, 1);
    let distinct = construct(&mut arena, &fixture.runtime, 2);
    assert!(Arc::ptr_eq(
        retained(&arena, first),
        retained(&arena, alias)
    ));
    assert!(!Arc::ptr_eq(
        retained(&arena, first),
        retained(&arena, distinct)
    ));
    assert_eq!(arena.actor(first).unwrap().health, 100);
    assert_eq!(arena.actor(distinct).unwrap().health, 200);
    let original_payload = Arc::downgrade(retained(&arena, first));

    fixture.select(1);
    assert!(
        old_source.upgrade().is_none(),
        "source residency really ended"
    );
    let replacement = construct(&mut arena, &fixture.runtime, 0);
    let replacement_alias = construct(&mut arena, &fixture.runtime, 1);
    assert!(Arc::ptr_eq(
        retained(&arena, replacement),
        retained(&arena, replacement_alias)
    ));
    assert!(!Arc::ptr_eq(
        retained(&arena, first),
        retained(&arena, replacement)
    ));
    assert_eq!(arena.actor(replacement).unwrap().health, 900);
    assert_eq!(
        arena.actor(first).unwrap().damage_table.responses()[0],
        (100, 256)
    );

    // Reloading identical original bytes is still a new native allocation.
    fixture.select(0);
    let reloaded = construct(&mut arena, &fixture.runtime, 0);
    assert_eq!(
        retained(&arena, first).as_ref(),
        retained(&arena, reloaded).as_ref()
    );
    assert!(!Arc::ptr_eq(
        retained(&arena, first),
        retained(&arena, reloaded)
    ));
    assert_eq!(
        arena.actor(replacement).unwrap().damage_table.responses()[0],
        (900, 256)
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(
        original_payload.upgrade().is_some(),
        "alias actor retains custody"
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: alias,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(
        original_payload.upgrade().is_none(),
        "cache must not own actors' tables"
    );
}
