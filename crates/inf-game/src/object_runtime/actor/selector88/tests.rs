use inf_formats::{decode_instance_command, InstanceCommandRequest, InstanceField};

use super::*;
use crate::object_runtime::{
    native_model_fixture::{Fixture, Models},
    ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0xa5a5, 1, 0xffff]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn fixture() -> Fixture {
    let models: Models = serde_json::from_str(r#"{"capacity":1,"entries":[]}"#).unwrap();
    Fixture::new(&models)
}

fn allocation(code: i32, operands: &[u8]) -> Arc<InstanceCommandAllocation> {
    let mut bytes = vec![0xcc; 3];
    bytes.extend_from_slice(&code.to_le_bytes());
    bytes.extend_from_slice(operands);
    let main_payload = ByteSpan::new(3, bytes.len());
    // Bytes beyond the main allocation must not satisfy operand reads.
    bytes.extend_from_slice(&[0xee; 8]);
    Arc::new(InstanceCommandAllocation::new(bytes.into(), main_payload, Arc::from([])).unwrap())
}

fn command(allocation: &InstanceCommandAllocation) -> InstanceCommandView {
    decode_instance_command(InstanceCommandRequest {
        read_scope: inf_formats::InstanceCommandReadScope::Inspection,
        source: allocation.source(),
        main_payload: allocation.main_payload(),
        relocations: allocation.relocations(),
        command_offset: 0,
        target: InstanceCommandTarget::Constructor(88),
    })
    .unwrap()
}

fn empty_placement() -> PlacementInstanceAssets {
    PlacementInstanceAssets {
        placement_index: 117,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(0, 4),
            value: 0,
        },
        command_pointer_span: ByteSpan::new(4, 4),
        commands: vec![],
    }
}

#[test]
fn constructor_preserves_unknown_fields_and_later_callbacks_remain_gated() {
    let mut arena = arena();
    let object = arena.allocate_selector88(request()).unwrap();
    let state = arena.selector88(object).unwrap();
    assert_eq!(state.word_138, None);
    assert_eq!(state.retained_13c, None);
    assert_eq!(state.word_140, None);
    assert_eq!([state.word_144, state.word_148, state.word_14c], [0; 3]);
    assert_eq!(state.word_150, None);
    assert_eq!(state.actor_154, None);
    assert_eq!(state.actor_158, None);
    assert_eq!(arena.actor_list(), &[object]);
    let handle = arena.actor(object).unwrap().handle;
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(object));
    let before = arena.object(object).unwrap().clone();
    assert_eq!(
        arena.initialize_after_load(object, Default::default()),
        Err(ObjectError::ActorAfterLoadBoundary(object))
    );
    assert_eq!(
        arena.update(object),
        Err(ObjectError::ActorUpdateBoundary(object))
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.actor_list().is_empty());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}

#[test]
fn one_past_retained_address_shares_allocation_and_replacement_changes_only_one_actor() {
    let fixture = fixture();
    let context = ActorCommandContext {
        terrain: None,
        runtime: &fixture.runtime,
        first_player_grants: None,
    };
    let mut arena = arena();
    let first = arena.allocate_selector88(request()).unwrap();
    let second = arena.allocate_selector88(request()).unwrap();
    let owner = allocation(16, &[]);
    let old_owner = Arc::downgrade(&owner);
    for object in [first, second] {
        assert_eq!(
            arena.apply_selector88_command(object, &command(&owner), context, Some(&owner)),
            Ok(true)
        );
        let retained = arena
            .selector88(object)
            .unwrap()
            .retained_13c
            .as_ref()
            .unwrap();
        assert_eq!(retained.allocation_offset, 4);
        assert!(Arc::ptr_eq(retained.allocation(), &owner));
        assert_eq!(retained.allocation().main_payload(), ByteSpan::new(3, 7));
    }
    let second_before = arena.object(second).unwrap().clone();
    let replacement = allocation(16, &[]);
    let replacement_lifetime = Arc::downgrade(&replacement);
    assert!(!Arc::ptr_eq(&replacement, &owner));
    arena
        .apply_selector88_command(first, &command(&replacement), context, Some(&replacement))
        .unwrap();
    assert_eq!(arena.object(second).unwrap(), &second_before);
    drop(second_before);
    drop(owner);
    drop(replacement);
    assert!(old_owner.upgrade().is_some());
    assert!(replacement_lifetime.upgrade().is_some());
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(replacement_lifetime.upgrade().is_none());
    assert!(old_owner.upgrade().is_some());
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(old_owner.upgrade().is_none());
}

#[test]
fn shared_delegation_and_rejection_do_not_require_a_retained_owner() {
    let fixture = fixture();
    let context = ActorCommandContext {
        terrain: None,
        runtime: &fixture.runtime,
        first_player_grants: None,
    };
    let mut arena = arena();
    let object = arena.allocate_selector88(request()).unwrap();
    for code in [0, 1, 3, 13, 14] {
        assert_eq!(
            arena.apply_selector88_command(object, &command(&allocation(code, &[])), context, None),
            Ok(true)
        );
    }
    assert_eq!(
        arena.apply_selector88_command(
            object,
            &command(&allocation(8, &0xabcd_u16.to_le_bytes())),
            context,
            None,
        ),
        Ok(true)
    );
    assert_eq!(arena.actor(object).unwrap().health, 0xabcd);
    for code in [i32::MIN, -1, 17, i32::MAX] {
        assert_eq!(
            arena.apply_selector88_command(object, &command(&allocation(code, &[])), context, None),
            Ok(false)
        );
    }
    let before = arena.object(object).unwrap().clone();
    assert_eq!(
        arena.apply_selector88_command(object, &command(&allocation(16, &[])), context, None),
        Err(ActorCommandError::Allocation(
            InstanceCommandAllocationError::MissingOwner
        ))
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    let shared = arena.allocate_actor(request()).unwrap();
    assert_eq!(
        arena.apply_actor_command(shared, &command(&allocation(3, &[])), context),
        Err(ActorCommandError::WrongTarget)
    );
}

#[test]
fn allocation_custody_requires_identity_and_preserves_main_span() {
    let owner = allocation(16, &[]);
    let placement = empty_placement();
    let commands = owner.commands(&placement);
    assert_eq!(owner.validate_commands(commands), Ok(()));
    let equal_bytes = owner.source().to_vec();
    assert_eq!(
        owner.validate_commands(ObjectInstanceCommands {
            source: &equal_bytes,
            ..commands
        }),
        Err(InstanceCommandAllocationError::SourceIdentity)
    );
    assert_eq!(
        owner.validate_commands(ObjectInstanceCommands {
            main_payload: ByteSpan::new(3, owner.source().len()),
            ..commands
        }),
        Err(InstanceCommandAllocationError::SourceIdentity)
    );
    assert!(matches!(
        InstanceCommandAllocation::new(Arc::from([0_u8; 4]), ByteSpan::new(0, 5), Arc::from([])),
        Err(InstanceCommandAllocationError::SourceRange)
    ));
}
