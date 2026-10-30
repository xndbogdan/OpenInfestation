//! A failed retained-table read can also stop the real class destructor.
//! The factory reports that live identity instead of claiming cleanup succeeded.

use super::{
    actor_factory_tests::{arena, inputs, math},
    tests::{request, resource_with_selector, retail_runtime},
    *,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorHandle, CreationListener, CreationListenerEffects, CreationListeners, INITIALIZED_FLAG,
    },
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
};
use inf_formats::{InstanceCommand, InstanceField};

struct UnreachedListeners;

impl CreationListenerEffects for UnreachedListeners {
    fn invoke(
        &mut self,
        _: CreationListener,
        _: ObjectId,
        _: &mut CreationListeners,
        _: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        panic!("a retained command failure precedes listener notification")
    }
}

#[test]
fn cleanup_error_identifies_published_actor_and_retains_partial_command_state() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 60);
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    // Real H14-style count/pointer-table ownership; code 17 succeeds before
    // code 19 retains an operand whose mandatory +4 count is outside storage.
    let mut bytes = vec![0; 28];
    bytes[0..4].copy_from_slice(&2_i32.to_le_bytes());
    bytes[4..8].copy_from_slice(&12_u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&20_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&17_i32.to_le_bytes());
    bytes[16..18].copy_from_slice(&0xfb2e_u16.to_le_bytes());
    bytes[20..24].copy_from_slice(&19_i32.to_le_bytes());
    let main = ByteSpan::new(0, bytes.len());
    let allocation = Arc::new(
        InstanceCommandAllocation::new(
            bytes.into(),
            main,
            [(4_u32, 12_u32), (8, 20)]
                .map(|(field, target)| AssetRelocation {
                    table_span: ByteSpan::new(0, 0),
                    field_offset: field,
                    field_span: ByteSpan::new(field as usize, field as usize + 4),
                    target_offset: target,
                })
                .into(),
        )
        .unwrap(),
    );
    let suffix = PlacementInstanceAssets {
        placement_index: 0x8000,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(0, 4),
            value: 2,
        },
        command_pointer_span: ByteSpan::new(4, 12),
        commands: [(12_u32, 17_i32), (20, 19)]
            .into_iter()
            .enumerate()
            .map(|(index, (offset, code))| InstanceCommand {
                index: index as u32,
                pointer_span: ByteSpan::new(4 + index * 4, 8 + index * 4),
                allocation_offset: offset,
                code: InstanceField {
                    span: ByteSpan::new(offset as usize, offset as usize + 4),
                    value: code,
                },
                payload_offset: offset + 4,
            })
            .collect(),
    };
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = UnreachedListeners;
    let mut rng = 0x1234_5678;
    let mut actor_inputs = inputs(&math, &mut listeners, &mut effects);
    actor_inputs.derived = Some(DerivedActorFactoryInputs {
        world_mask_limits: placements.world().actor_mask_limits(),
        rng: &mut rng,
    });
    let result = construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            suffix: Some(allocation.commands(&suffix)),
            command_allocation: Some(&allocation),
            actor: Some(actor_inputs),
            ..request(resource)
        },
    );
    let ObjectConstructionError::Cleanup { object, source } = result.unwrap_err() else {
        panic!("expected cleanup boundary to identify its live object")
    };
    assert_eq!(
        source,
        ObjectError::Selector60(Selector60Error::Allocation(RuntimeAllocationError::Range {
            offset: 28,
            length: 4,
            allocation_bytes: 28,
        }))
    );
    assert_eq!(arena.len(), 1);
    assert_eq!(arena.actor_list(), &[object]);
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.position_words, Some([0xfffe, 0x8000, 1]));
    assert_eq!(state.base.placement_word, 0x8000);
    assert_eq!(state.base.resource_word, resource as u16);
    assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
    assert_eq!(arena.derived_actor(object).unwrap().word_164, 0xfb2e);
    let retained = arena
        .selector60_actor(object)
        .unwrap()
        .retained_190
        .as_ref()
        .unwrap();
    assert_eq!(retained.allocation_offset, 24);
    assert!(Arc::ptr_eq(retained.allocation(), &allocation));
    let handle = arena.actor(object).unwrap().handle;
    assert_eq!(handle, ActorHandle::from_raw(0x08ff_0001));
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(object));
    assert_eq!(arena.selector60_live_rows(), 0);
    assert!(listeners.is_empty());
    let mut expected_rng = 0x1234_5678;
    name_entry_rng_next(&mut expected_rng);
    assert_eq!(rng, expected_rng);

    // The reported identity permits deliberate recovery: a valid empty list
    // replaces the faulty retained input, then real destruction can finish.
    let empty = Arc::new(
        InstanceCommandAllocation::new(Arc::from([0; 8]), ByteSpan::new(0, 8), Arc::from([]))
            .unwrap(),
    );
    arena.set_selector60_spawn_list(object, &empty, 0).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}
