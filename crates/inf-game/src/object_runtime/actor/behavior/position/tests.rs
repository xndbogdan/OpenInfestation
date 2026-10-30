use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};

fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut rng = 0;
    let actor = arena
        .allocate_selector36_actor(
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
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut rng,
        )
        .unwrap();
    let mut state = super::super::build_actor_behavior(0x81, 0, 1).unwrap();
    state.byte_10 = 0;
    state.entries_18[0][6] = 0;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(state);
    (arena, actor)
}
fn group(arena: &mut ObjectArena, actor: ObjectId) -> &mut super::super::ActorBehaviorGroup {
    &mut arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .groups_1c[0]
}
#[test]
fn actor_position_mode_bypasses_even_missing_behavior() {
    let (mut arena, actor) = fixture();
    arena.derived_actor_mut(actor).unwrap().behavior_140 = None;
    assert_eq!(
        arena
            .actor_behavior_position(actor, ActorBehaviorPositionRequest::ActorPosition)
            .unwrap(),
        [100, 200, 300]
    );
    assert!(
        matches!(arena.actor_behavior_position(actor,ActorBehaviorPositionRequest::SelectedWeaponGroup),Err(ActorBehaviorPositionError::RequiredBehavior(actual)) if actual==actor)
    );
}
#[test]
fn cached_group_bypasses_relative_actor_xyz_and_basis_reads_but_requires_actual_world() {
    let (mut arena, actor) = fixture();
    let g = group(&mut arena, actor);
    g.word_0c = 3;
    g.coordinates_00_0a = [None, None, None, Some(0xfffe), Some(7), Some(8)];
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    assert_eq!(
        arena
            .actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup)
            .unwrap(),
        [0xfffe, 7, 8]
    );
    group(&mut arena, actor).coordinates_00_0a[4] = None;
    let before = arena.derived_actor(actor).unwrap().behavior_140.clone();
    assert!(matches!(
        arena.actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup),
        Err(ActorBehaviorPositionError::Coordinate {
            offset: 8,
            index: 0,
            ..
        })
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().behavior_140, before);
}
#[test]
fn all_relative_inputs_are_captured_before_any_transformed_world_store() {
    let (mut arena, actor) = fixture();
    let g = group(&mut arena, actor);
    g.word_0c = 1;
    g.coordinates_00_0a = [Some(1), None, Some(3), Some(10), Some(20), Some(30)];
    let before = arena.derived_actor(actor).unwrap().behavior_140.clone();
    assert!(matches!(
        arena.actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup),
        Err(ActorBehaviorPositionError::Coordinate { offset: 2, .. })
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().behavior_140, before);
}
#[test]
fn missing_actor_position_retains_actual_complete_transform_but_not_cache_bit() {
    let (mut arena, actor) = fixture();
    let g = group(&mut arena, actor);
    g.word_0c = 1;
    g.coordinates_00_0a = [Some(0xfffd), Some(2), Some(5), None, None, None];
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    assert!(
        matches!(arena.actor_behavior_position(actor,ActorBehaviorPositionRequest::SelectedWeaponGroup),
        Err(ActorBehaviorPositionError::Object(ObjectError::ActorPositionUnknown(actual))) if actual==actor)
    );
    let g = group(&mut arena, actor);
    assert_eq!(
        g.coordinates_00_0a,
        [
            Some(0xfffd),
            Some(2),
            Some(5),
            Some(0xfffd),
            Some(2),
            Some(5)
        ]
    );
    assert_eq!(g.word_0c, 1);
}
#[test]
fn relative_wrap_then_actual_weapon_tick_invalidates_same_retained_world_cache() {
    let (mut arena, actor) = fixture();
    let g = group(&mut arena, actor);
    g.word_0c = 1;
    g.coordinates_00_0a = [Some(0xffff), Some(2), Some(3), None, None, None];
    assert_eq!(
        arena
            .actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup)
            .unwrap(),
        [99, 202, 303]
    );
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([400, 500, 600]);
    assert_eq!(
        arena
            .actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup)
            .unwrap(),
        [99, 202, 303]
    );
    arena.tick_actor_behavior(actor, 0).unwrap();
    assert_eq!(
        arena
            .actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup)
            .unwrap(),
        [399, 502, 603]
    );
    assert_eq!(group(&mut arena, actor).word_0c & 2, 2);
}
#[test]
fn absolute_world_publication_bypasses_unknown_relative_words_and_sets_bit_only_normally() {
    let (mut arena, actor) = fixture();
    let g = group(&mut arena, actor);
    g.word_0c = 0;
    g.coordinates_00_0a = [None; 6];
    assert_eq!(
        arena
            .actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup)
            .unwrap(),
        [100, 200, 300]
    );
    assert_eq!(
        group(&mut arena, actor).coordinates_00_0a,
        [None, None, None, Some(100), Some(200), Some(300)]
    );
    assert_eq!(group(&mut arena, actor).word_0c, 2);
}
#[test]
fn signed_selection_and_unsigned_group_index_stop_without_clamping_or_writes() {
    let (mut arena, actor) = fixture();
    arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .byte_10 = 0xff;
    assert!(matches!(
        arena.actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup),
        Err(ActorBehaviorPositionError::Entry { index: -1, .. })
    ));
    let b = arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    b.byte_10 = 0;
    b.entries_18[0][6] = u32::MAX;
    assert!(matches!(
        arena.actor_behavior_position(actor, ActorBehaviorPositionRequest::SelectedWeaponGroup),
        Err(ActorBehaviorPositionError::Group {
            index: u32::MAX,
            ..
        })
    ));
}
#[test]
fn native_word_transform_has_xyz_precision_order_distinct_from_dword_point_transform() {
    let mut basis = inf_formats::native_basis::NativeBasis::INITIAL;
    basis.words[1] = 1e30_f32.to_bits();
    basis.words[4] = (-1e30_f32).to_bits();
    basis.words[7] = 1_f32.to_bits();
    assert_eq!(basis.transform_words([32767; 3])[0], 32767);
    assert_eq!(basis.transform_point([32767; 3])[0], 0);
    let mut basis = inf_formats::native_basis::NativeBasis::INITIAL;
    basis.words[1] = 0.5_f32.to_bits();
    assert_eq!(basis.transform_words([1, 0, 0])[0], 0);
    assert_eq!(basis.transform_words([3, 0, 0])[0], 2);
}
