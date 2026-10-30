use std::sync::Arc;

use inf_formats::ByteSpan;
use serde_json::Value;

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyBehaviorProfile, RuntimeAllocationError,
    Selector12GrantContext,
};

const SINE: [u16; 4096] = [0; 4096];

fn request() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([1, 2, 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3, 3],
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap()
}

fn allocation(bytes: Vec<u8>) -> Arc<RuntimeAllocation> {
    let main = ByteSpan::new(0, bytes.len());
    Arc::new(RuntimeAllocation::new(bytes.into(), main).unwrap())
}

fn fixture(profile: FamilyBehaviorProfile) -> (ObjectArena, ObjectId) {
    let mut arena = arena();
    let actor = match profile {
        FamilyBehaviorProfile::Family4236d0 { mode } => {
            let actor = arena
                .allocate_family4236d0_actor(request(), &mut 0)
                .unwrap();
            arena.family4236d0_actor_mut(actor).unwrap().word_266 = mode;
            actor
        }
        FamilyBehaviorProfile::Selector12 { .. } => arena
            .allocate_selector12_actor(
                request(),
                &mut 0,
                Selector12GrantContext {
                    table: &inf_formats::GrantTable {
                        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                        records: vec![],
                    },
                    first_player_grants: None,
                },
            )
            .unwrap(),
        FamilyBehaviorProfile::Selector35 => {
            arena.allocate_selector35_actor(request(), &mut 0).unwrap()
        }
        FamilyBehaviorProfile::Selector34 => {
            arena.allocate_selector34_actor(request(), &mut 0).unwrap()
        }
    };
    (arena, actor)
}

fn retain(
    arena: &mut ObjectArena,
    actor: ObjectId,
    profile: FamilyBehaviorProfile,
    owner: &Arc<RuntimeAllocation>,
    offset: u32,
) {
    match profile {
        FamilyBehaviorProfile::Family4236d0 { .. } => arena
            .set_family4236d0_retained(actor, owner, offset)
            .unwrap(),
        FamilyBehaviorProfile::Selector12 { .. } => {
            arena.set_selector12_retained(actor, owner, offset).unwrap()
        }
        FamilyBehaviorProfile::Selector35 => {
            arena.set_selector35_retained(actor, owner, offset).unwrap()
        }
        FamilyBehaviorProfile::Selector34 => {
            arena.set_selector34_retained(actor, owner, offset).unwrap()
        }
    }
}

fn profile(name: &str) -> FamilyBehaviorProfile {
    match name {
        "family-mode1" => FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        "family-mode2" => FamilyBehaviorProfile::Family4236d0 { mode: 2 },
        "selector35" => FamilyBehaviorProfile::Selector35,
        "selector12-absent" => FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        "selector12-present" => FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
        _ => panic!("unsupported score receipt profile"),
    }
}

fn word(value: &Value) -> u32 {
    value.as_u64().unwrap().try_into().unwrap()
}

#[test]
fn descriptor_motion_scores_match_312_original_wrapper_receipts_without_writes() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family_descriptor_motion_native.json"
    ))
    .unwrap();
    assert_eq!(
        receipt["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let cases = receipt["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 312);
    for case in cases {
        let profile = profile(case["profile"].as_str().unwrap());
        let descriptor = profile
            .descriptor(case["index"].as_i64().unwrap() as i32)
            .unwrap();
        let request = FamilyDescriptorScoreRequest::from_descriptor(
            descriptor,
            false, //These original scalar wrappers ignore current.
            case["bias"].as_i64().unwrap() as i32,
        )
        .unwrap();
        assert_eq!(request.kind.address(), word(&case["callback"]));
        let (mut arena, actor) = fixture(profile);
        let owner = allocation(vec![0xa5; 32]);
        // A nonzero retained base distinguishes pointer retention from an
        // accidental read of the beginning of the owner allocation.
        let retained_base = 4;
        owner
            .write_word(
                retained_base + request.kind.coefficient_offset().unwrap(),
                word(&case["coefficient"]) as u16,
            )
            .unwrap();
        retain(&mut arena, actor, profile, &owner, retained_base);
        let scores = &mut arena.family428650_actor_mut(actor).unwrap().words_1d8_200;
        scores[1] = word(&case["first"]);
        scores[3] = word(&case["second"]);
        scores[2] = word(&case["third"]);
        arena.derived_actor_mut(actor).unwrap().word_164 = case["radius"].as_i64().unwrap() as i32;
        let before = arena.derived_actor(actor).unwrap().clone();
        assert_eq!(
            arena
                .score_family_descriptor(actor, request, FamilyDescriptorScoreRng::RequiredAtReach)
                .unwrap() as u32,
            word(&case["result"]),
            "{case}"
        );
        assert_eq!(arena.derived_actor(actor).unwrap(), &before, "{case}");
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
        if !matches!(profile, FamilyBehaviorProfile::Selector35) {
            assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        }
    }
}

#[test]
fn descriptor_motion_zero_is_lazy_and_repeated_reads_use_shared_live_bytes() {
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let (mut arena, actor) = fixture(profile);
    let owner = allocation(vec![0; 14]);
    retain(&mut arena, actor, profile, &owner, 0);
    arena.derived_actor_mut(actor).unwrap().word_164 = -3;
    let state = arena.family428650_actor_mut(actor).unwrap();
    state.words_1d8_200[1] = 0x8000_0000;
    state.words_1d8_200[2] = 0x8000_0001;
    state.words_1d8_200[3] = 0x7fff_ffff;
    // These callbacks use scalar words without dereferencing source targets.
    let released = arena.import_fixture_target(crate::object_runtime::NativeTarget::FixedPosition(
        crate::object_runtime::FixedPositionTarget {
            handle_04: crate::object_runtime::ActorHandle::from_raw(0),
            word_10: 8,
            focus_08: None,
        },
    ));
    arena.release_actor_service(released).unwrap();
    arena
        .family428650_actor_mut(actor)
        .unwrap()
        .targets_1d4_1fc
        .fill(Some(released));
    for (kind, expected) in [
        (FamilyDescriptorScoreKind::Max1e0And1f0, i32::MIN),
        (FamilyDescriptorScoreKind::Score1e8, i32::MIN + 1),
        (FamilyDescriptorScoreKind::HalfRadius164Common, -1),
        (FamilyDescriptorScoreKind::HalfRadius164Selector12, -1),
    ] {
        let request = FamilyDescriptorScoreRequest {
            kind,
            current: false,
            bias: 0,
        };
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                request,
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0)
        );
        owner
            .write_word(kind.coefficient_offset().unwrap(), 1)
            .unwrap();
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                request,
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(expected)
        );
        owner
            .write_word(kind.coefficient_offset().unwrap(), 0)
            .unwrap();
    }
    assert!(arena
        .family428650_actor(actor)
        .unwrap()
        .targets_1d4_1fc
        .iter()
        .all(|&reference| reference == Some(released)));
    assert_eq!(
        arena.actor_service_heap().state(released).unwrap(),
        crate::object_runtime::ActorServiceResourceState::Released
    );

    // Zero coefficient still requires its own valid reached retained WORD;
    // one-past retention is valid, but a later read remains an explicit error.
    retain(&mut arena, actor, profile, &owner, 14);
    assert!(matches!(
        arena.score_family_descriptor(
            actor,
            FamilyDescriptorScoreRequest {
                kind: FamilyDescriptorScoreKind::Max1e0And1f0,
                current: false,
                bias: 0,
            },
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Derived(
            DerivedActorError::Allocation(RuntimeAllocationError::Range { offset: 16, .. })
        ))
    ));
}

#[test]
fn descriptor_motion_preserves_missing_owners_and_class_admission_boundaries() {
    let request = FamilyDescriptorScoreRequest {
        kind: FamilyDescriptorScoreKind::HalfRadius164Common,
        current: false,
        bias: 0,
    };
    for (profile, missing) in [
        (
            FamilyBehaviorProfile::Family4236d0 { mode: 1 },
            DerivedActorError::MissingFamily4236d0Payload,
        ),
        (
            FamilyBehaviorProfile::Selector12 {
                behavior_present: false,
            },
            DerivedActorError::MissingSelector12Payload,
        ),
        (
            FamilyBehaviorProfile::Selector35,
            DerivedActorError::MissingSelector35Payload,
        ),
    ] {
        let (mut arena, actor) = fixture(profile);
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                request,
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Err(FamilyDescriptorScoreError::Derived(missing))
        );
    }
    let mut arena = arena();
    let base = arena
        .allocate_family428650_actor(self::request(), &mut 0)
        .unwrap();
    assert_eq!(
        arena.score_family_descriptor(base, request, FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::UnsupportedRetainedBinding(base))
    );
    let actor = arena
        .allocate_selector34_actor(self::request(), &mut 0)
        .unwrap();
    assert_eq!(
        arena.score_family_descriptor(actor, request, FamilyDescriptorScoreRng::RequiredAtReach),
        Err(FamilyDescriptorScoreError::Derived(
            DerivedActorError::MissingSelector34Payload
        ))
    );
    let owner = allocation(vec![0; 10]);
    arena.set_selector34_retained(actor, &owner, 0).unwrap();
    assert_eq!(
        arena.score_family_descriptor(actor, request, FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(0)
    );
    owner.write_word(8, 1).unwrap();
    arena.derived_actor_mut(actor).unwrap().word_164 = -5;
    assert_eq!(
        arena.score_family_descriptor(actor, request, FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(-2)
    );
}

#[path = "wrappers_tests.rs"]
mod wrappers_tests;

#[path = "escort_native_tests.rs"]
mod escort_native_tests;
