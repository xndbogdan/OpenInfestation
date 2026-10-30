use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        actor::ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
        ObjectError, ObjectList,
    },
};
use inf_formats::{AssetRelocation, ByteSpan};

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

fn allocation(bytes: Vec<u8>, relocations: &[(u32, u32)]) -> Arc<InstanceCommandAllocation> {
    let main = ByteSpan::new(5, 5 + bytes.len());
    let mut source = vec![0x5a; 5];
    source.extend(bytes);
    Arc::new(
        InstanceCommandAllocation::new(
            source.into(),
            main,
            relocations
                .iter()
                .map(|&(field, target)| AssetRelocation {
                    table_span: ByteSpan::new(0, 0),
                    field_offset: field,
                    field_span: ByteSpan::new(
                        main.start + field as usize,
                        main.start + field as usize + 4,
                    ),
                    target_offset: target,
                })
                .collect::<Vec<_>>()
                .into(),
        )
        .unwrap(),
    )
}

fn list(rows: &[(i32, i16, i16, u8)]) -> Arc<InstanceCommandAllocation> {
    let mut bytes = vec![0; 32 + rows.len() * 24];
    bytes[4..8].copy_from_slice(&(rows.len() as i32).to_le_bytes());
    bytes[8..12].copy_from_slice(&32_u32.to_le_bytes());
    for (index, &(reference, capacity, timer, flags)) in rows.iter().enumerate() {
        let at = 32 + index * 24;
        bytes[at..at + 4].copy_from_slice(&reference.to_le_bytes());
        bytes[at + 6..at + 8].copy_from_slice(&capacity.to_le_bytes());
        bytes[at + 10..at + 12].copy_from_slice(&timer.to_le_bytes());
        bytes[at + 16] = flags;
    }
    allocation(bytes, &[(8, 32)])
}

fn row_id(owner: &InstanceCommandAllocation, offset: u32) -> Selector60RowId {
    match owner
        .runtime_allocation()
        .runtime_pointer(offset)
        .unwrap()
        .unwrap()
    {
        RuntimePointer::Selector60Row(id) => id,
    }
}

#[test]
fn constructor_adds_no_rng_step_and_keeps_unwritten_position_unknown() {
    let mut arena = arena();
    let mut rng = 0x1234_5678;
    let mut expected = rng;
    name_entry_rng_next(&mut expected);
    let actor = arena
        .allocate_selector60_actor(request(), &mut rng)
        .unwrap();
    assert_eq!(rng, expected);
    let state = arena.selector60_actor(actor).unwrap();
    assert_eq!(
        (
            state.byte_184,
            state.byte_188,
            state.byte_189,
            state.byte_1a8
        ),
        (0, 0, 0, 0)
    );
    assert_eq!((state.word_18a, state.word_18c), (1, 1000));
    assert_eq!(
        (
            state.word_194,
            state.word_19e,
            state.word_1a0,
            state.word_1a4
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(state.position_198, None);
    assert!(state.retained_190.is_none());
    let handle = arena.actor(actor).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}

#[test]
fn command_retains_before_first_read_and_nonpositive_count_never_reads_pointer_or_mode() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    for count in [0_i32, -1, i32::MIN] {
        let mut bytes = vec![0xee; 8];
        bytes[4..8].copy_from_slice(&count.to_le_bytes());
        let owner = allocation(bytes, &[]);
        // Poisoned mode is opaque; there is no +8 pointer at all.
        arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
        let retained = arena
            .selector60_actor(actor)
            .unwrap()
            .retained_190
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(retained.allocation(), &owner));
        assert_eq!(retained.allocation_offset, 0);
        arena
            .initialize_after_load(actor, Default::default())
            .unwrap();
        assert_eq!(
            arena.object(actor).unwrap().base.flags & 0x0180_0000,
            0x0180_0000
        );
    }
    let truncated = allocation(vec![0; 7], &[]);
    assert!(matches!(
        arena.set_selector60_spawn_list(actor, &truncated, 0),
        Err(Selector60Error::Allocation(RuntimeAllocationError::Range {
            offset: 4,
            ..
        }))
    ));
    assert!(Arc::ptr_eq(
        arena
            .selector60_actor(actor)
            .unwrap()
            .retained_190
            .as_ref()
            .unwrap()
            .allocation(),
        &truncated
    ));
}

#[test]
fn command_normalizes_only_zero_capacity_and_rereads_aliased_count() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let owner = list(&[(-1, 0, 0, 0), (-1, -1, 0, 0), (-1, 3, 0, 0)]);
    for pointer in [52, 76, 100] {
        owner
            .runtime_allocation()
            .write_dword(pointer, 0xdeadbeef)
            .unwrap();
    }
    arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
    assert_eq!(
        [38, 62, 86].map(|at| owner.runtime_allocation().word(at).unwrap()),
        [1, 0xffff, 3]
    );
    assert_eq!(
        [52, 76, 100].map(|at| owner.runtime_allocation().dword(at).unwrap()),
        [0; 3]
    );
    // The first row's +14 aliases the header count. An eager count snapshot
    // would reach row 1, whose normalization word is deliberately absent.
    let mut bytes = vec![0; 28];
    bytes[20..24].copy_from_slice(&99_i32.to_le_bytes());
    let overlap = allocation(bytes, &[(24, 0)]);
    arena
        .set_selector60_spawn_list(actor, &overlap, 16)
        .unwrap();
    assert_eq!(overlap.runtime_allocation().word(6).unwrap(), 1);
    assert_eq!(overlap.runtime_allocation().dword(20).unwrap(), 0);
}

#[test]
fn command_rejects_relocated_overlap_after_preserving_prior_normalization() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let mut bytes = vec![0; 56];
    bytes[4..8].copy_from_slice(&1_i32.to_le_bytes());
    bytes[8..12].copy_from_slice(&32_u32.to_le_bytes());
    let owner = allocation(bytes, &[(8, 32), (52, 0)]);
    assert_eq!(
        arena.set_selector60_spawn_list(actor, &owner, 0),
        Err(Selector60Error::Allocation(
            RuntimeAllocationError::RelocatedWrite {
                offset: 52,
                field_offset: 52
            }
        ))
    );
    assert_eq!(owner.runtime_allocation().word(38).unwrap(), 1);
    assert!(arena
        .selector60_actor(actor)
        .unwrap()
        .retained_190
        .is_some());
}

#[test]
fn null_after_load_queues_real_disposal_then_wrapper_sets_only_its_high_flag() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    arena
        .initialize_after_load(actor, Default::default())
        .unwrap();
    assert_eq!(
        arena.object(actor).unwrap().base.flags & 0x0180_0000,
        0x0080_0000
    );
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == actor));
    assert_eq!(arena.selector60_live_rows(), 0);
}

#[test]
fn row_construction_retains_or_disposes_real_placement_target_and_fills_null_handles() {
    for (flags, expected_live, disposed) in [(2_u8, 1_u32, false), (0, 0, true), (0xfe, 1, false)] {
        let mut arena = arena();
        let target = arena.allocate_actor(request().actor).unwrap();
        arena
            .initialize_context(
                target,
                BaseContext {
                    position_words: None,
                    placement_index: 0xffff,
                },
            )
            .unwrap();
        let handle = arena.actor(target).unwrap().handle;
        let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
        let owner = list(&[(0, 3, -32768, flags), (-1, 1, 32767, 2)]);
        arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
        arena
            .initialize_after_load(actor, Default::default())
            .unwrap();
        let row = arena.selector60_row(row_id(&owner, 52)).unwrap();
        assert_eq!(
            (
                row.byte_00,
                row.byte_01,
                row.word_04,
                row.word_0c,
                row.word_10
            ),
            (0, 0, 0xffff8000, expected_live, 0)
        );
        assert_eq!(
            row.handles_08,
            if expected_live == 1 {
                vec![
                    handle,
                    ActorHandle::from_raw(0x1234),
                    ActorHandle::from_raw(0x1234),
                ]
            } else {
                vec![ActorHandle::from_raw(0x1234); 3]
            }
        );
        let missing = arena.selector60_row(row_id(&owner, 76)).unwrap();
        assert_eq!(missing.word_04, 32767);
        assert_eq!(missing.handles_08, vec![ActorHandle::from_raw(0x1234)]);
        assert_eq!(missing.word_0c, 0);
        assert_eq!(
            arena
                .list(ObjectList::Disposal)
                .iter()
                .any(|node| node.object == target),
            disposed
        );
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: actor,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert_eq!(arena.selector60_live_rows(), 0);
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(target));
    }
}

#[test]
fn alias_replacement_leaks_old_rows_and_destructor_leaves_a_checked_dangling_pointer() {
    let mut arena = arena();
    let first = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let second = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let owner = list(&[(-1, 1, -1, 2)]);
    arena.set_selector60_spawn_list(first, &owner, 0).unwrap();
    arena.set_selector60_spawn_list(second, &owner, 0).unwrap();
    arena
        .initialize_after_load(first, Default::default())
        .unwrap();
    let orphan = row_id(&owner, 52);
    arena
        .initialize_after_load(second, Default::default())
        .unwrap();
    let current = row_id(&owner, 52);
    assert_ne!(orphan, current);
    assert_eq!(arena.selector60_live_rows(), 2);
    // The first actor destroys the SECOND actor's current shared pointer.
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.selector60_row(orphan).is_ok());
    assert_eq!(
        arena.selector60_row(current),
        Err(Selector60Error::UnknownRow(current))
    );
    assert_eq!(row_id(&owner, 52), current);
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::Selector60(Selector60Error::UnknownRow(
            current
        )))
    );
    assert!(arena.object(second).is_ok());
    // Reissuing command 19 clears dangling pointer custody without freeing
    // the still-orphaned first allocation; a fresh row gets a fresh ID.
    arena.set_selector60_spawn_list(second, &owner, 0).unwrap();
    assert!(owner.runtime_allocation().runtime_pointers().is_empty());
    assert_eq!(arena.selector60_live_rows(), 1);
    arena
        .initialize_after_load(second, Default::default())
        .unwrap();
    assert_ne!(row_id(&owner, 52), current);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.selector60_live_rows(), 1);
}

#[test]
fn command_clearing_a_live_row_preserves_orphan_and_typed_pointer_reads_are_opaque() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let owner = list(&[(-1, 1, 0, 0)]);
    arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
    arena
        .initialize_after_load(actor, Default::default())
        .unwrap();
    let id = row_id(&owner, 52);
    let image = owner.runtime_allocation();
    for (offset, length) in [(52, 4), (51, 2), (55, 1)] {
        assert_eq!(
            image.validate_scalar_read(offset, length),
            Err(RuntimeAllocationError::RuntimePointerScalar {
                offset,
                field_offset: 52
            })
        );
    }
    assert_eq!(
        image.write_word(54, 0),
        Err(RuntimeAllocationError::RuntimePointerScalar {
            offset: 54,
            field_offset: 52
        })
    );
    assert_eq!(&image.bytes()[52..56], &[0; 4]);
    assert_eq!(
        image.runtime_pointers(),
        vec![(52, RuntimePointer::Selector60Row(id))]
    );
    let foreign = ObjectArena::new();
    assert_eq!(
        foreign.selector60_row(id),
        Err(Selector60Error::UnknownRow(id))
    );
    arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
    assert_eq!(image.dword(52).unwrap(), 0);
    assert!(arena.selector60_row(id).is_ok());
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.selector60_row(id).is_ok());
}

#[test]
fn after_load_rejects_negative_capacity_and_opaque_pointer_count_before_wrapper_flag() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let owner = list(&[(-1, -1, 0, 0)]);
    arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
    assert_eq!(
        arena.initialize_after_load(actor, Default::default()),
        Err(ObjectError::Selector60(Selector60Error::NegativeCapacity(
            -1
        )))
    );
    assert_eq!(arena.object(actor).unwrap().base.flags & 0x0180_0000, 0);
    assert_eq!(arena.selector60_live_rows(), 0);
    // A runtime pointer landing over an aliased count is opaque. Native
    // addresses would decide its iteration bound; the port must reject.
    let mut bytes = vec![0; 28];
    bytes[6..8].copy_from_slice(&1_i16.to_le_bytes());
    bytes[20..24].copy_from_slice(&1_i32.to_le_bytes());
    let owner = allocation(bytes, &[(24, 0)]);
    let retained = RetainedSpawnList {
        allocation: Arc::clone(&owner),
        allocation_offset: 16,
    };
    arena.selector60_actor_mut(actor).unwrap().retained_190 = Some(retained);
    assert_eq!(
        arena.initialize_after_load(actor, Default::default()),
        Err(ObjectError::Selector60(Selector60Error::Allocation(
            RuntimeAllocationError::RuntimePointerScalar {
                offset: 20,
                field_offset: 20
            }
        )))
    );
    assert_eq!(arena.selector60_live_rows(), 1);
    assert_eq!(arena.object(actor).unwrap().base.flags & 0x0180_0000, 0);
}

#[test]
fn zero_capacity_seeded_row_cannot_store_a_resolving_handle_and_raw_pointers_cannot_be_freed() {
    let mut arena = arena();
    let target = arena.allocate_actor(request().actor).unwrap();
    arena
        .initialize_context(
            target,
            BaseContext {
                position_words: None,
                placement_index: 41,
            },
        )
        .unwrap();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    let owner = list(&[(42, 0, -1, 2)]);
    let retained = RetainedSpawnList {
        allocation: Arc::clone(&owner),
        allocation_offset: 0,
    };
    // A caller that bypasses command normalization reaches native's unsafe
    // zero-length initial store, rather than silently growing the array.
    assert_eq!(
        arena.construct_selector60_row(&retained, 32),
        Err(Selector60Error::InitialHandleOutsideCapacity)
    );
    assert_eq!(arena.selector60_live_rows(), 0);
    arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
    arena
        .initialize_after_load(actor, Default::default())
        .unwrap();
    assert_eq!(
        arena.selector60_row(row_id(&owner, 52)).unwrap().handles_08,
        vec![arena.actor(target).unwrap().handle]
    );
    // A scalar DWORD has no heap custody even when its bits look plausible.
    arena.set_selector60_spawn_list(actor, &owner, 0).unwrap();
    owner.runtime_allocation().write_dword(52, 0x1234).unwrap();
    assert_eq!(
        arena.destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach
        }),
        Err(ObjectError::Selector60(Selector60Error::Allocation(
            RuntimeAllocationError::UnownedRuntimePointer {
                offset: 52,
                value: 0x1234,
            }
        )))
    );
    assert!(arena.actor(actor).is_ok());
}

/// 00480F40: message 7 counts DWORD +1A4, marks byte +1A8 and locks from
/// state 2; the last message 8 unlocks a marked actor from state 3.
#[test]
fn link_messages_count_in_1a4_and_unlock_only_once_linked() {
    let mut arena = arena();
    let mut rng = 1;
    let object = arena
        .allocate_selector60_actor(request(), &mut rng)
        .unwrap();
    let source = arena.actor(object).unwrap().handle;
    let change = |target_state| crate::object_runtime::ActorStateChangeRequest {
        target_state,
        argument: 0,
        source,
    };
    arena.actor_mut(object).unwrap().word_c4 = 3;
    // Never linked: +1A8 is clear, so even a nonpositive count keeps state 3.
    assert_eq!(
        arena.selector60_link_message(object, 8).unwrap(),
        crate::object_runtime::FamilyLinkChange::None
    );
    assert_eq!(arena.selector60_actor(object).unwrap().word_1a4, u32::MAX);
    assert_eq!(
        arena.selector60_link_message(object, 7).unwrap(),
        crate::object_runtime::FamilyLinkChange::None
    );
    assert_eq!(
        arena.selector60_link_message(object, 7).unwrap(),
        crate::object_runtime::FamilyLinkChange::None
    );
    let state = arena.selector60_actor(object).unwrap();
    assert_eq!((state.word_1a4, state.byte_1a8), (1, 1));
    assert_eq!(
        arena.selector60_link_message(object, 8).unwrap(),
        crate::object_runtime::FamilyLinkChange::Unlock(change(2))
    );
    arena.actor_mut(object).unwrap().word_c4 = 2;
    assert_eq!(
        arena.selector60_link_message(object, 7).unwrap(),
        crate::object_runtime::FamilyLinkChange::Lock(change(3))
    );
    assert_eq!(
        arena.selector60_link_message(object, 0xd),
        Err(ObjectError::LinkMessageBoundary(object))
    );
}

/// 00480C80, selector 60's +B4: without a spawn list it reacts and leaves
/// +144; with one it clears +144 and the list's first WORD decides: 0
/// never, 2 with byte +188, 3 with a zero WORD +18A, 4 with byte +189, and
/// 1, 5 or a negative WORD always.
#[test]
fn damage_gate_follows_the_spawn_list_mode() {
    let mut arena = arena();
    let actor = arena.allocate_selector60_actor(request(), &mut 0).unwrap();
    arena.derived_actor_mut(actor).unwrap().word_144 = 77;
    assert!(arena.selector60_reacts_to_damage(actor).unwrap());
    assert_eq!(arena.derived_actor(actor).unwrap().word_144, 77);
    for (mode, byte_188, byte_189, word_18a, reacts) in [
        (0_i16, 1, 1, 0, false),
        (1, 0, 0, 1, true),
        (2, 0, 1, 0, false),
        (2, 1, 0, 1, true),
        (3, 1, 1, 1, false),
        (3, 0, 0, 0, true),
        (4, 1, 0, 0, false),
        (4, 0, 1, 1, true),
        (5, 0, 0, 1, true),
        (-1, 0, 0, 1, true),
    ] {
        let mut bytes = vec![0; 8];
        bytes[0..2].copy_from_slice(&mode.to_le_bytes());
        arena
            .set_selector60_spawn_list(actor, &allocation(bytes, &[]), 0)
            .unwrap();
        let state = arena.selector60_actor_mut(actor).unwrap();
        state.byte_188 = byte_188;
        state.byte_189 = byte_189;
        state.word_18a = word_18a;
        arena.derived_actor_mut(actor).unwrap().word_144 = 77;
        assert_eq!(
            arena.selector60_reacts_to_damage(actor).unwrap(),
            reacts,
            "{mode}"
        );
        assert_eq!(arena.derived_actor(actor).unwrap().word_144, 0);
    }
}
