mod extended;

use std::path::Path;

use inf_formats::{parse_grant_table, FrontendExecutable, GrantTable};
use sha2::{Digest, Sha256};

use super::*;
use crate::object_runtime::{
    ActorConstructionError, ActorConstructionRequest, ActorExtension, ActorHandle,
    ActorHandleError, ActorRegistryConfig, ActorTerrainError, ActorTerrainInputs,
    ActorTerrainSurface, ActorWaterPlane, DerivedActorClass, DerivedActorConstructionRequest,
    Family428650Class, FamilyUpdateEffect, FamilyUpdateHost, ObjectClassKind, ObjectId,
    Selector12GrantContext,
};

// H12 is genuinely absent in these supplied-constructor fixtures. Its native
// NULL branch leaves the sine table, XYZ and H14 flags unread.
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn request() -> DerivedActorConstructionRequest<'static> {
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
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

fn executable() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe")).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Constructor {
    Shared,
    Derived,
    Family428650,
    Family429e20,
    Family4236d0,
    Selector10,
    Selector11,
    Selector12,
    Selector34,
    Selector35,
    Selector36,
    Selector77,
    Selector30,
}

fn construct(
    arena: &mut ObjectArena,
    class: Constructor,
    rng: &mut u32,
    grants: &GrantTable,
) -> ObjectId {
    let request = request();
    match class {
        Constructor::Shared => arena.allocate_actor(request.actor),
        Constructor::Derived => arena.allocate_derived_actor(request, rng),
        Constructor::Family428650 => arena.allocate_family428650_actor(request, rng),
        Constructor::Family429e20 => arena.allocate_family429e20_actor(request, rng),
        Constructor::Family4236d0 => arena.allocate_family4236d0_actor(request, rng),
        Constructor::Selector10 => arena.allocate_selector10_actor(request, rng),
        Constructor::Selector11 => arena.allocate_selector11_actor(request, rng),
        Constructor::Selector12 => arena.allocate_selector12_actor(
            request,
            rng,
            Selector12GrantContext {
                table: grants,
                first_player_grants: None,
            },
        ),
        Constructor::Selector34 => arena.allocate_selector34_actor(request, rng),
        Constructor::Selector35 => arena.allocate_selector35_actor(request, rng),
        Constructor::Selector36 => arena.allocate_selector36_actor(request, rng),
        Constructor::Selector77 => arena.allocate_selector77_actor(request, rng),
        Constructor::Selector30 => arena.allocate_selector30_actor(request.actor),
    }
    .unwrap()
}

// Normalize only cross-arena identity; compare the complete real registry
// configuration, packed generations/free chains and lookup cache otherwise.
fn registry_shape(arena: &ObjectArena) -> String {
    let registry = arena.actor_handles().unwrap();
    let cache = registry.cache();
    let owners: Vec<_> = registry
        .owners()
        .iter()
        .map(|owner| {
            (
                owner.word_00,
                owner.word_04,
                owner.free_head,
                owner.free_tail,
                owner
                    .slots()
                    .iter()
                    .map(|slot| {
                        (
                            slot.handle,
                            slot.object.map(|object| (object.slot, object.generation)),
                            slot.next_free,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    format!(
        "{:?}",
        (
            registry.initial_capacity(),
            registry.null_handle(),
            owners,
            cache.handle,
            cache.object.map(|object| (object.slot, object.generation))
        )
    )
}

// Change only an expected constructor snapshot, through the actual typed
// hierarchy. Complete ObjectState equality below protects every other scalar,
// unknown field, target, behavior and constructor store.
fn normalize_expected_carries(arena: &mut ObjectArena, object: ObjectId, class: Constructor) {
    match class {
        Constructor::Family428650 | Constructor::Selector34 | Constructor::Selector35 => {
            arena.family428650_actor_mut(object).unwrap().word_220 = Some(0);
        }
        Constructor::Family429e20
        | Constructor::Family4236d0
        | Constructor::Selector10
        | Constructor::Selector12
        | Constructor::Selector36 => {
            let state = arena.family428650_actor_mut(object).unwrap();
            state.word_220 = Some(0);
            let Family428650Class::Family429e20(state) = &mut state.class else {
                panic!("real inherited constructor")
            };
            state.word_230 = Some(0);
        }
        Constructor::Selector77 => {
            let DerivedActorClass::Selector77(state) =
                &mut arena.derived_actor_mut(object).unwrap().class
            else {
                panic!("real selector77 constructor")
            };
            state.word_19c = Some(0);
            state.word_1a0 = Some(0);
        }
        Constructor::Shared
        | Constructor::Derived
        | Constructor::Selector11
        | Constructor::Selector30 => {}
    }
}

#[test]
fn explicit_policy_changes_only_the_four_inherited_constructor_carries() {
    let raw = executable();
    let grants = parse_grant_table(FrontendExecutable::parse(&raw).unwrap()).unwrap();
    for class in [
        Constructor::Shared,
        Constructor::Derived,
        Constructor::Family428650,
        Constructor::Family429e20,
        Constructor::Family4236d0,
        Constructor::Selector10,
        Constructor::Selector11,
        Constructor::Selector12,
        Constructor::Selector34,
        Constructor::Selector35,
        Constructor::Selector36,
        Constructor::Selector77,
        Constructor::Selector30,
    ] {
        let mut strict = arena();
        let mut approximated = arena();
        approximated
            .select_actor_allocation_policy(ActorAllocationPolicy::FreshHeapZeroCarryApproximation)
            .unwrap();
        let mut strict_rng = 0x1234_5678;
        let mut approximated_rng = strict_rng;
        let expected = construct(&mut strict, class, &mut strict_rng, &grants);
        let actual = construct(&mut approximated, class, &mut approximated_rng, &grants);
        assert_eq!(
            strict.actor_allocation,
            ActorAllocationService::Started(ActorAllocationPolicy::Unowned)
        );
        assert_eq!(
            approximated.actor_allocation,
            ActorAllocationService::Started(ActorAllocationPolicy::FreshHeapZeroCarryApproximation)
        );
        assert_eq!(strict_rng, approximated_rng, "{class:?}");
        normalize_expected_carries(&mut strict, expected, class);
        assert_eq!(
            strict.object(expected).unwrap(),
            approximated.object(actual).unwrap(),
            "{class:?}"
        );
        assert_eq!(strict.len(), approximated.len());
        assert_eq!(strict.actor_list().len(), approximated.actor_list().len());
        assert_eq!(
            registry_shape(&strict),
            registry_shape(&approximated),
            "{class:?}"
        );
        assert_eq!(strict.list_owners, approximated.list_owners);
        let lists = |arena: &ObjectArena| {
            arena
                .lists
                .iter()
                .map(|list| {
                    list.iter()
                        .map(|node| (node.object.slot, node.object.generation, node.kind))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(lists(&strict), lists(&approximated), "{class:?}");
        // Constructor-owned DWORD +230 is not the water-timer WORD.
        if matches!(class, Constructor::Selector34) {
            assert_eq!(approximated.selector34_actor(actual).unwrap().word_230, 500);
        }
        if matches!(class, Constructor::Selector35) {
            assert_eq!(approximated.selector35_actor(actual).unwrap().word_230, 500);
        }
    }
}

#[test]
fn strict_default_and_explicit_unowned_selection_keep_eye_holes_unknown() {
    for policy in [None, Some(ActorAllocationPolicy::Unowned)] {
        let mut arena = arena();
        if let Some(policy) = policy {
            arena.select_actor_allocation_policy(policy).unwrap();
        }
        let object = arena.allocate_selector77_actor(request(), &mut 0).unwrap();
        let state = arena.selector77_actor(object).unwrap();
        assert_eq!(state.words_184_190, [0; 7]);
        assert_eq!((state.word_19c, state.word_19e), (None, None));
        assert_eq!((state.word_1a0, state.word_1ac), (None, None));
        assert_eq!(state.position_192, None);
        assert_eq!(
            arena.actor_allocation,
            ActorAllocationService::Started(ActorAllocationPolicy::Unowned)
        );
    }
}

#[test]
fn selection_is_once_and_survives_an_empty_arena_after_actual_actor_retirement() {
    let mut arena = arena();
    let nonactor = arena.allocate(ObjectClassKind::Base);
    let policy = ActorAllocationPolicy::FreshHeapZeroCarryApproximation;
    arena.select_actor_allocation_policy(policy).unwrap();
    let before = format!("{arena:?}");
    assert_eq!(
        arena.select_actor_allocation_policy(ActorAllocationPolicy::Unowned),
        Err(ActorAllocationPolicyError::AlreadySelected { policy })
    );
    assert_eq!(format!("{arena:?}"), before);
    let object = arena.allocate_actor(request().actor).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: nonactor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.is_empty());
    let before = format!("{arena:?}");
    assert_eq!(
        arena.select_actor_allocation_policy(policy),
        Err(ActorAllocationPolicyError::AlreadyStarted { policy })
    );
    assert_eq!(format!("{arena:?}"), before);
    let later = arena
        .allocate_family429e20_actor(request(), &mut 0)
        .unwrap();
    assert_eq!(arena.family428650_actor(later).unwrap().word_220, Some(0));
    assert_eq!(arena.family429e20_actor(later).unwrap().word_230, Some(0));
}

#[test]
fn preflight_failure_does_not_start_policy_but_failed_actual_handle_allocation_does() {
    let mut missing_registry = ObjectArena::new();
    assert_eq!(
        missing_registry.allocate_actor(request().actor),
        Err(ActorConstructionError::MissingRegistry)
    );
    assert_eq!(
        missing_registry.actor_allocation,
        ActorAllocationService::Unselected
    );
    missing_registry
        .select_actor_allocation_policy(ActorAllocationPolicy::FreshHeapZeroCarryApproximation)
        .unwrap();

    // A valid supplied H12 still fails at unknown pre-context XYZ, before
    // any cell/sine/H14 read or allocation. This is not an original terrain.
    let bytes = vec![0; inf_formats::TERRAIN_ENTRY_FIXED_BYTES];
    let span = inf_formats::ByteSpan::new(0, bytes.len());
    let terrain_bank = inf_formats::parse_terrain_bank(
        &bytes,
        &inf_formats::HandlerRegion {
            handler: inf_formats::HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: inf_formats::HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    let terrain = crate::terrain_runtime::TerrainRuntime::new(
        &terrain_bank.entries[0],
        crate::terrain_runtime::TerrainSampling {
            spacing: crate::terrain_runtime::TerrainSpacing::Units128,
            height_scale: crate::terrain_runtime::TerrainHeightScale::Units32,
        },
    );
    let mut terrain_preflight = arena();
    let bad_position = ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: Some(ActorTerrainSurface {
                terrain: &terrain,
                plane_height_fixed: 0,
            }),
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    };
    assert_eq!(
        terrain_preflight.allocate_actor(bad_position),
        Err(ActorConstructionError::Terrain(
            ActorTerrainError::UnknownAllocationPosition
        ))
    );
    assert!(terrain_preflight.is_empty());
    assert_eq!(
        terrain_preflight.actor_allocation,
        ActorAllocationService::Unselected
    );
    terrain_preflight
        .select_actor_allocation_policy(ActorAllocationPolicy::FreshHeapZeroCarryApproximation)
        .unwrap();

    let mut arena = arena();
    let mut invalid_owner = request().actor;
    invalid_owner.owner = 0;
    assert_eq!(
        arena.allocate_actor(invalid_owner),
        Err(ActorConstructionError::Handles(ActorHandleError::Owner(0)))
    );
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
    assert_eq!(
        arena.actor_allocation,
        ActorAllocationService::Started(ActorAllocationPolicy::Unowned)
    );
    assert_eq!(
        arena
            .select_actor_allocation_policy(ActorAllocationPolicy::FreshHeapZeroCarryApproximation),
        Err(ActorAllocationPolicyError::AlreadyStarted {
            policy: ActorAllocationPolicy::Unowned
        })
    );
}

#[test]
fn later_allocations_do_not_reset_existing_carries_or_fill_other_holes() {
    let mut arena = arena();
    arena
        .select_actor_allocation_policy(ActorAllocationPolicy::FreshHeapZeroCarryApproximation)
        .unwrap();
    let family = arena
        .allocate_family429e20_actor(request(), &mut 0)
        .unwrap();
    let eye = arena.allocate_selector77_actor(request(), &mut 0).unwrap();
    // Explicit supplied post-construction history. These stores do not stand
    // in for an unimplemented Eye update or captured original heap values.
    let state = arena.family428650_actor_mut(family).unwrap();
    state.word_220 = Some(0x1234);
    let Family428650Class::Family429e20(state) = &mut state.class else {
        unreachable!()
    };
    state.word_230 = Some(0x5678);
    let DerivedActorClass::Selector77(state) = &mut arena.derived_actor_mut(eye).unwrap().class
    else {
        unreachable!()
    };
    state.word_19c = Some(0x8001);
    state.word_1a0 = Some(0xffff_ff00);
    let before_family = arena.object(family).unwrap().clone();
    let before_eye = arena.object(eye).unwrap().clone();
    let later_family = arena
        .allocate_family429e20_actor(request(), &mut 0)
        .unwrap();
    let later_eye = arena.allocate_selector77_actor(request(), &mut 0).unwrap();
    assert_eq!(arena.object(family).unwrap(), &before_family);
    assert_eq!(arena.object(eye).unwrap(), &before_eye);
    assert_eq!(
        arena.family428650_actor(later_family).unwrap().word_220,
        Some(0)
    );
    assert_eq!(
        arena.family429e20_actor(later_family).unwrap().word_230,
        Some(0)
    );
    let later = arena.selector77_actor(later_eye).unwrap();
    assert_eq!((later.word_19c, later.word_1a0), (Some(0), Some(0)));
    assert_eq!(
        (later.word_19e, later.word_1ac, later.position_192),
        (None, None, None)
    );
    assert_eq!(
        arena.family429e20_actor(later_family).unwrap().word_23a,
        None
    );
    assert_eq!(
        arena.family428650_actor(later_family).unwrap().position_222,
        None
    );
}

struct UnreachedEffects;
impl FamilyUpdateHost for UnreachedEffects {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        panic!("this allocation prefix must not read a damage source")
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        Err("unexpected RNG")
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        Err("unexpected water effect")
    }
}

#[test]
fn the_selected_carry_is_read_and_mutated_by_the_real_water_consumer() {
    let mut arena = arena();
    arena
        .select_actor_allocation_policy(ActorAllocationPolicy::FreshHeapZeroCarryApproximation)
        .unwrap();
    let mut actual_request = request();
    actual_request.actor.allocation_position = Some([10, 0xffff, 30]);
    let actor = arena
        .allocate_family429e20_actor(actual_request, &mut 0)
        .unwrap();
    let mut host = UnreachedEffects;
    arena
        .tick_actor_water(
            actor,
            2048,
            ActorWaterPlane {
                height_fixed: 0,
                coordinate_shift: 8,
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, Some(2));
    arena
        .allocate_family429e20_actor(request(), &mut 0)
        .unwrap();
    assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, Some(2));
    // No damage or random callback was supplied successfully. The actual
    // previous<=4000 submerged branch only writes its low-WORD timer.
}

#[test]
fn production_world_owner_selects_before_its_first_actor_and_retains_the_same_arena_policy() {
    let mut owner = crate::world_backend::WorldLoadOwner::unprepared().unwrap();
    assert_eq!(
        owner.arena.actor_allocation,
        ActorAllocationService::Selected(
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation
        )
    );
    // Move the actual arena, not a save projection or simulated transition.
    let mut actual_arena = owner.arena;
    let object = actual_arena
        .allocate_selector77_actor(request(), &mut 0)
        .unwrap();
    assert_eq!(
        actual_arena.selector77_actor(object).unwrap().word_19c,
        Some(0)
    );
    assert_eq!(
        actual_arena.selector77_actor(object).unwrap().word_1a0,
        Some(0)
    );
    owner.arena = actual_arena;
    assert_eq!(
        owner.arena.actor_allocation,
        ActorAllocationService::Started(
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation
        )
    );
}

#[test]
fn complete_native_constructors_preserve_the_four_unwritten_carry_boundaries() {
    let raw = executable();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    FrontendExecutable::parse(&raw).unwrap();
    for (start, end, digest) in [
        (
            0x0042_8650,
            0x0042_8729,
            "42f4992c11ffd8b87ab75bee34edd89b920c3182a2c8ba1e936abe574351b1cf",
        ),
        (
            0x0042_9e20,
            0x0042_9e41,
            "7029b147d82d514c0f32d97171127f1c7aca1f78e9a12ee238034eb5ec77b72f",
        ),
        (
            0x0040_7410,
            0x0040_7489,
            "de7cb9f1b1d315c9b03dbd4c9ebe61dca2f3fc2379d5f2261099479b707342b2",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&raw[start - 0x0040_0000..end - 0x0040_0000])
            ),
            digest
        );
    }
    // Static guards authenticate complete original bodies. The approximation
    // remains separate from native stores and allocator/execution acceptance.
}
