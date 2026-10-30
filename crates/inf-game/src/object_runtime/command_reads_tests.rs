use std::sync::Arc;

use inf_formats::{
    AssetRelocation, ByteSpan, InstanceCommand, InstanceField, PlacementInstanceAssets,
};

use super::super::run_commands;
use super::*;
use crate::object_runtime::{
    native_model_fixture::{Fixture, Models},
    ActorConstructionRequest, ActorExtension, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, InstanceCommandAllocation, ObjectClass,
    RuntimeAllocationError, RuntimePointer, Selector60RowId,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

#[test]
fn new_actor_scalar_views_reject_opaque_heap_pointer_slots_at_their_actual_widths() {
    use inf_formats::{
        decode_instance_command, InstanceCommandReadScope, InstanceCommandRequest,
        InstanceCommandTarget,
    };
    let (arena, object) = actor(0);
    for (selector, codes) in [
        (30, (16..=32).collect::<Vec<_>>()),
        (35, vec![20, 21, 22, 24]),
    ] {
        for code in codes {
            let (owner, _) = command(code);
            owner
                .runtime_allocation()
                .set_runtime_pointer(20, pointer())
                .unwrap();
            let snapshot = owner.runtime_allocation().source_snapshot();
            let view = decode_instance_command(InstanceCommandRequest {
                source: &snapshot,
                main_payload: owner.main_payload(),
                relocations: owner.relocations(),
                command_offset: 16,
                target: InstanceCommandTarget::Constructor(selector),
                read_scope: InstanceCommandReadScope::Dispatch,
            })
            .unwrap();
            assert!(
                matches!(
                    validate(
                        &arena,
                        object,
                        CommandPolicy::Constructor(1),
                        owner.runtime_allocation(),
                        owner.main_payload(),
                        &view
                    ),
                    Err(ObjectConstructionError::RuntimeAllocation(
                        RuntimeAllocationError::RuntimePointerScalar {
                            offset: 20,
                            field_offset: 20
                        }
                    ))
                ),
                "selector {selector}, code {code}"
            );
        }
    }
    // Retention itself does not read the aliased operand; its later ratio
    // read belongs to the selector35 retained-data consumer.
    let (owner, _) = command(23);
    owner
        .runtime_allocation()
        .set_runtime_pointer(20, pointer())
        .unwrap();
    let snapshot = owner.runtime_allocation().source_snapshot();
    let view = decode_instance_command(InstanceCommandRequest {
        source: &snapshot,
        main_payload: owner.main_payload(),
        relocations: owner.relocations(),
        command_offset: 16,
        target: InstanceCommandTarget::Constructor(35),
        read_scope: InstanceCommandReadScope::Dispatch,
    })
    .unwrap();
    validate(
        &arena,
        object,
        CommandPolicy::Constructor(1),
        owner.runtime_allocation(),
        owner.main_payload(),
        &view,
    )
    .unwrap();
}

fn actor(mode: u8) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let object = arena
        .allocate_selector11_actor(
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
                world_mask_limits: [1, 1],
            },
            &mut 0,
        )
        .unwrap();
    let ObjectClass::Actor(state) = &mut arena.owned_mut(object).unwrap().state.class else {
        unreachable!()
    };
    let ActorExtension::Derived(state) = &mut state.extension else {
        unreachable!()
    };
    state.byte_13d = Some(mode);
    (arena, object)
}

fn resident() -> Fixture {
    Fixture::new(
        &serde_json::from_value::<Models>(serde_json::json!({"capacity":0,"entries":[]})).unwrap(),
    )
}

fn command(code: i32) -> (Arc<InstanceCommandAllocation>, PlacementInstanceAssets) {
    // Nonzero file prefix ensures validation converts spans to allocation offsets.
    let main = ByteSpan::new(5, 37);
    let mut source = vec![0xa5; main.end];
    source[5..9].copy_from_slice(&1_i32.to_le_bytes());
    source[9..13].copy_from_slice(&16_u32.to_le_bytes());
    source[21..25].copy_from_slice(&code.to_le_bytes());
    source[25..29].copy_from_slice(&0_u32.to_le_bytes());
    let pointer_span = ByteSpan::new(9, 13);
    let owner = Arc::new(
        InstanceCommandAllocation::new(
            source.into(),
            main,
            vec![AssetRelocation {
                table_span: ByteSpan::new(0, 0),
                field_offset: 4,
                field_span: pointer_span,
                target_offset: 16,
            }]
            .into(),
        )
        .unwrap(),
    );
    let placement = PlacementInstanceAssets {
        placement_index: 0,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(5, 9),
            value: 1,
        },
        command_pointer_span: pointer_span,
        commands: vec![InstanceCommand {
            index: 0,
            pointer_span,
            allocation_offset: 16,
            code: InstanceField {
                span: ByteSpan::new(21, 25),
                value: code,
            },
            payload_offset: 20,
        }],
    };
    (owner, placement)
}

fn pointer() -> RuntimePointer {
    // This opaque identity is never dereferenced by a command scalar read.
    RuntimePointer::Selector60Row(Selector60RowId { arena: 0, index: 0 })
}

fn assert_scalar_error(
    result: Result<Option<crate::object_runtime::ObjectCommandRejection>, ObjectConstructionError>,
    offset: u32,
) {
    assert!(
        matches!(result, Err(ObjectConstructionError::RuntimeAllocation(
        RuntimeAllocationError::RuntimePointerScalar { offset: actual, field_offset }
    )) if actual == offset && field_offset == offset),
        "{result:?}"
    );
}

#[test]
fn owned_walker_rejects_typed_pointers_in_count_code_and_scalar_operand() {
    let resident = resident();
    for offset in [0, 16, 20] {
        let (mut arena, object) = actor(0);
        let (owner, placement) = command(19);
        owner
            .runtime_allocation()
            .set_runtime_pointer(offset, pointer())
            .unwrap();
        let before = arena.object(object).unwrap().clone();
        let result = run_commands(
            &mut arena,
            &resident.runtime,
            object,
            CommandPolicy::DerivedActor {
                vehicle_table: None,
                terrain: None,
                selector: 11,
                first_player_grants: None,
                allocation: Some(&owner),
            },
            0,
            Some(owner.commands(&placement)),
        );
        assert_scalar_error(result, offset);
        assert_eq!(arena.object(object).unwrap(), &before);
    }
}

#[test]
fn relocated_command_pointer_cannot_be_replaced_by_a_typed_heap_pointer() {
    let resident = resident();
    let (mut arena, object) = actor(0);
    let (owner, placement) = command(16);
    let before = owner.runtime_allocation().bytes();
    assert_eq!(
        owner.runtime_allocation().set_runtime_pointer(4, pointer()),
        Err(RuntimeAllocationError::RelocatedWrite {
            offset: 4,
            field_offset: 4
        })
    );
    assert_eq!(owner.runtime_allocation().bytes(), before);
    assert!(owner.runtime_allocation().runtime_pointers().is_empty());
    assert!(run_commands(
        &mut arena,
        &resident.runtime,
        object,
        CommandPolicy::DerivedActor {
            vehicle_table: None,
            terrain: None,
            selector: 11,
            first_player_grants: None,
            allocation: Some(&owner)
        },
        0,
        Some(owner.commands(&placement)),
    )
    .unwrap()
    .is_none());
}

#[test]
fn shared_conditional_commands_skip_pointer_aliased_operands_without_a_player() {
    let resident = resident();
    let grants = [0; inf_formats::GRANT_MASK_WORDS];
    for code in [13, 14] {
        for present in [false, true] {
            let (mut arena, object) = actor(0);
            let (owner, placement) = command(code);
            owner
                .runtime_allocation()
                .set_runtime_pointer(20, pointer())
                .unwrap();
            let before = arena.object(object).unwrap().clone();
            let result = run_commands(
                &mut arena,
                &resident.runtime,
                object,
                CommandPolicy::DerivedActor {
                    vehicle_table: None,
                    terrain: None,
                    selector: 11,
                    first_player_grants: present.then_some(&grants),
                    allocation: Some(&owner),
                },
                0,
                Some(owner.commands(&placement)),
            );
            if present {
                assert_scalar_error(result, 20);
            } else {
                assert!(result.unwrap().is_none());
            }
            assert_eq!(arena.object(object).unwrap(), &before);
        }
    }
}

#[test]
fn selector11_conditional_command_skips_pointer_alias_until_guard_creates_behavior() {
    let resident = resident();
    for mode in [0, 1] {
        let (mut arena, object) = actor(mode);
        let (owner, placement) = command(21);
        owner
            .runtime_allocation()
            .set_runtime_pointer(20, pointer())
            .unwrap();
        // Establish the exact state the walker guard will reconstruct, so a
        // failed operand read must leave every command-owned field unchanged.
        arena.admit_derived_actor(object).unwrap();
        let before = arena.object(object).unwrap().clone();
        let result = run_commands(
            &mut arena,
            &resident.runtime,
            object,
            CommandPolicy::DerivedActor {
                vehicle_table: None,
                terrain: None,
                selector: 11,
                first_player_grants: None,
                allocation: Some(&owner),
            },
            0,
            Some(owner.commands(&placement)),
        );
        if mode == 0 {
            assert!(result.unwrap().is_none());
        } else {
            assert_scalar_error(result, 20);
        }
        assert_eq!(arena.object(object).unwrap(), &before);
    }
}
