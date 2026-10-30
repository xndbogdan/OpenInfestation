use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyAttackUpdateRequest, FixedPositionTarget, NativeTarget,
    TargetNullHandleSource,
};

const SINE: [u16; 4096] = [0; 4096];

pub(super) fn construction() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([100, 200, 300]),
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

fn arena() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234_5678),
    })
    .unwrap();
    let actor = arena
        .allocate_family428650_actor(construction(), &mut 0)
        .unwrap();
    arena.family428650_actor_mut(actor).unwrap().helper_218 =
        Some(Box::new(FamilyHelper482ff0::new(None)));
    (arena, actor)
}

const MISSING: FamilyHelperHistoryInputs = FamilyHelperHistoryInputs {
    process_null_handle: None,
};
const INPUTS: FamilyHelperHistoryInputs = FamilyHelperHistoryInputs {
    process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
};

#[test]
fn signed_negative_record_count_replaces_zero_without_registry_or_count_publication() {
    let (mut arena, actor) = arena();
    let helper = arena.family_helper_history_mut(actor).unwrap();
    helper.word_08 = 0x8000_0000;
    helper.records[0].padding = Some(0xface);
    let before = helper.clone();
    arena.actor_handles = None;
    arena
        .fade_family_helper_history(actor, u32::MAX, MISSING)
        .unwrap();
    assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
    arena
        .record_family_helper_history(
            actor,
            FamilyHelperRecordRequest {
                handle: ActorHandle::from_raw(7),
                displacement_words: [0; 3],
                native_weight: 0x0001_0001,
            },
            MISSING,
        )
        .unwrap();
    let helper = arena.family_helper_history(actor).unwrap();
    assert_eq!(helper.word_08, 0x8000_0000);
    assert_eq!(
        helper.records[0],
        FamilyHelperHistoryRecord {
            handle: Some(ActorHandle::from_raw(7)),
            normal_bits: [Some(1.0f32.to_bits()), Some(0), Some(0)],
            weight: Some(2048),
            padding: Some(0xface),
        }
    );
    assert_eq!(helper.records[1..], before.records[1..]);
    let helper = arena.family_helper_history_mut(actor).unwrap();
    helper.word_08 = 5;
    let before = helper.clone();
    assert_eq!(
        arena.record_family_helper_history(
            actor,
            FamilyHelperRecordRequest {
                handle: ActorHandle::from_raw(8),
                displacement_words: [1, 2, 3],
                native_weight: 90,
            },
            INPUTS
        ),
        Err(FamilyHelperHistoryError::CountOutsideAllocation { actor, count: 5 })
    );
    assert_eq!(arena.family_helper_history(actor).unwrap(), &before);
}

#[test]
fn missing_tail_scalar_retains_weight_and_compaction_prefixes() {
    let (mut arena, actor) = arena();
    let helper = arena.family_helper_history_mut(actor).unwrap();
    helper.word_08 = 2;
    helper.words_00_04 = [Some(7), None];
    helper.records[0].weight = Some(0);
    helper.records[0].padding = Some(0xabcd);
    helper.records[1] = FamilyHelperHistoryRecord {
        handle: None,
        normal_bits: [None, Some(0x8000_0000), None],
        weight: Some(50),
        padding: Some(0xeeee),
    };
    assert_eq!(
        arena.fade_family_helper_history(actor, 0, MISSING),
        Err(FamilyHelperHistoryError::MissingProcessNullHandle)
    );
    let helper = arena.family_helper_history(actor).unwrap();
    assert_eq!(helper.word_08, 2);
    assert_eq!(helper.records[0].handle, None);
    assert_eq!(
        helper.records[0].normal_bits,
        [None, Some(0x8000_0000), None]
    );
    assert_eq!(helper.records[0].weight, Some(50));
    assert_eq!(helper.records[0].padding, Some(0xabcd));
    assert_eq!(helper.words_00_04, [Some(7), None]);
    assert_eq!(
        arena.clear_family_helper_history(actor, MISSING),
        Err(FamilyHelperHistoryError::MissingProcessNullHandle)
    );
    let helper = arena.family_helper_history(actor).unwrap();
    assert_eq!(helper.word_08, 2);
    assert_eq!(helper.records[0].weight, Some(0));
    assert_eq!(helper.records[1].weight, Some(50));
    arena.clear_family_helper_history(actor, INPUTS).unwrap();
    let helper = arena.family_helper_history(actor).unwrap();
    assert_eq!(helper.word_08, 0);
    assert_eq!(
        helper.records[0].normal_bits,
        [None, Some(0x8000_0000), None]
    );
    assert_eq!(helper.records[0].padding, Some(0xabcd));
    assert!(helper
        .records
        .iter()
        .all(|r| r.weight == Some(0) && r.handle == INPUTS.process_null_handle));
}

struct FixtureProcess;
impl TargetNullHandleSource for FixtureProcess {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        INPUTS.process_null_handle
    }
}

#[test]
fn concrete_attack_services_append_to_existing_helper_and_publish_flags_after_success() {
    let (mut arena, actor) = arena();
    let handle = ActorHandle::from_raw(0x0801_0001);
    let current = arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: handle,
        word_10: 8,
        focus_08: Some([99, 198, 297]),
    }));
    let state = arena.derived_actor_mut(actor).unwrap();
    state.target_148 = Some(current);
    state.word_14c = Some(1);
    state.word_150 = Some(3);
    let family = arena.family428650_actor_mut(actor).unwrap();
    family.words_188_19c[1] = Some(100);
    family.word_22e = 0x10;
    let helper = family.helper_218.as_mut().unwrap();
    helper.words_00_04 = [None, Some(8_000_000)];
    helper.records[0].padding = Some(0xbeef);
    let process = FixtureProcess;
    let mut host = FamilyAttackUpdateServices {
        process: &process,
        helper_inputs: MISSING,
    };
    let mut rng = 123;
    arena
        .update_family_attack(
            actor,
            FamilyAttackUpdateRequest {
                microseconds: 1,
                settings_7c: 0,
            },
            &mut rng,
            &mut host,
        )
        .unwrap();
    assert_eq!(rng, 123);
    let family = arena.family428650_actor(actor).unwrap();
    assert_eq!(family.word_22e, 0x1c);
    let helper = family.helper_218.as_ref().unwrap();
    assert_eq!(helper.words_00_04, [None, Some(8_000_001)]);
    assert_eq!(helper.word_08, 1);
    assert_eq!(
        helper.records[0],
        FamilyHelperHistoryRecord {
            handle: Some(handle),
            normal_bits: [Some(0x3e88_d677), Some(0x3f08_d677), Some(0x3f4d_41b2)],
            weight: Some(65535),
            padding: Some(0xbeef),
        }
    );
    // A reached helper error keeps the timer prefix and prevents flag0C.
    let helper = arena.family_helper_history_mut(actor).unwrap();
    helper.word_08 = 4;
    helper.records[0].handle = None;
    arena.family428650_actor_mut(actor).unwrap().word_22e = 0x10;
    let result = arena.update_family_attack(
        actor,
        FamilyAttackUpdateRequest {
            microseconds: 1,
            settings_7c: 0,
        },
        &mut rng,
        &mut host,
    );
    assert!(matches!(
        result,
        Err(super::super::FamilyAttackUpdateError::Helper(
            FamilyHelperHistoryError::Uninitialized { offset: 0x0c, .. }
        ))
    ));
    assert_eq!(arena.family428650_actor(actor).unwrap().word_22e, 0x10);
    assert_eq!(
        arena.family_helper_history(actor).unwrap().words_00_04[1],
        Some(8_000_002)
    );
}
