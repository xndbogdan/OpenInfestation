use std::sync::Arc;

use inf_formats::{native_basis::NativeBasis, ByteSpan};

use super::*;
use crate::object_runtime::{
    ActorChildFactoryCall, ActorConstructionRequest, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, RuntimeAllocation,
};

fn fixture() -> (ObjectArena, ObjectId, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    // Synthetic ownership/error test: no terrain or angle lookup is reached.
    let construction = ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([1, 2, 3]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &[0; 4096],
        },
    };
    let parent = arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
                actor: construction,
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    let child = arena.allocate_actor(construction).unwrap();
    arena.actor_mut(parent).unwrap().word_b4 = 7;
    arena.actor_mut(parent).unwrap().word_74 = 99;
    arena.owned_mut(parent).unwrap().state.base.word_28 = 101;
    let mut bytes = vec![0; 0x7a];
    bytes[0x76..0x78].copy_from_slice(&0xffff_u16.to_le_bytes());
    bytes[0x78..0x7a].copy_from_slice(&0x3a_u16.to_le_bytes());
    let allocation = Arc::new(
        RuntimeAllocation::new(
            Arc::from(bytes),
            ByteSpan {
                start: 0,
                end: 0x7a,
            },
        )
        .unwrap(),
    );
    let Family4236d0Class::Selector36(state) =
        &mut arena.family4236d0_actor_mut(parent).unwrap().class
    else {
        unreachable!()
    };
    state.payload_274 = Some(RetainedActorAllocation::new(&allocation, 0).unwrap());
    state.word_27e = 23;
    (arena, parent, child)
}

struct Host {
    child: ObjectId,
    stop_basis: bool,
    stop_refresh: bool,
    creates: usize,
}

impl ActorChildHost for Host {
    type Error = &'static str;

    fn create_child_object(
        &mut self,
        _: &mut ObjectArena,
        _: ActorChildFactoryCall,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.creates += 1;
        // Deliberate supplied lower-service mutation, not a factory model.
        *rng = rng.wrapping_add(1);
        Ok(Some(self.child))
    }

    fn resolve_child_actor(
        &mut self,
        _: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Ok(Some(raw))
    }

    fn child_source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<NativeBasis>, Self::Error> {
        assert_eq!(arena.actor(self.child).unwrap().word_b4, 7);
        assert_eq!(
            arena.actor(self.child).unwrap().owner_handle,
            arena.actor(parent).unwrap().handle
        );
        if self.stop_basis {
            Err("basis")
        } else {
            Ok(Some(arena.actor(parent).unwrap().rotating.basis))
        }
    }

    fn create_registry_object(
        &mut self,
        _: &mut ObjectArena,
        _: u16,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Err("the selector-36 child builds no registry object")
    }
    fn take_shot(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        _: crate::object_runtime::ActorHandle,
    ) -> Result<bool, Self::Error> {
        Err("the selector-36 child gives no player a shot")
    }

    fn refresh_child_parent_model(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<(), Self::Error> {
        assert_eq!(
            arena.actor(parent).unwrap().rotating.visual.model_word,
            0xffff
        );
        assert_ne!(arena.object(parent).unwrap().base.dirty_word & 0x100, 0);
        assert_ne!(
            arena.family4236d0_actor(parent).unwrap().word_264 & 0x1000,
            0
        );
        assert_eq!(arena.actor(parent).unwrap().word_74, 99);
        assert_eq!(arena.selector36_actor(parent).unwrap().word_27e, 23);
        if self.stop_refresh {
            Err("refresh")
        } else {
            Ok(())
        }
    }
}

#[test]
fn actor_child_basis_error_keeps_published_out_slot_and_factory_rng() {
    let (mut arena, parent, child) = fixture();
    let mut host = Host {
        child,
        stop_basis: true,
        stop_refresh: false,
        creates: 0,
    };
    let original_position = arena.object(child).unwrap().base.position_words;
    let mut out = Some(parent);
    let mut rng = 20;
    assert_eq!(
        arena.create_actor_child(
            parent,
            ActorChildRequest {
                resource: ActorChildResource::Full(0x8765_003a),
                position: Some([4, 5, 6]),
            },
            Some(&mut out),
            &mut host,
            &mut rng,
        ),
        Err(ActorChildError::Service("basis"))
    );
    assert_eq!(out, Some(child));
    assert_eq!(rng, 21);
    assert_eq!(
        arena.object(child).unwrap().base.position_words,
        original_position
    );
    assert_eq!(arena.object(child).unwrap().base.dirty_word & 6, 6);
    assert_eq!(arena.actor(child).unwrap().words_8e_92, [0; 3]);
}

#[test]
fn selector36_child_refresh_error_preserves_child_and_published_transition() {
    let (mut arena, parent, child) = fixture();
    let mut host = Host {
        child,
        stop_basis: false,
        stop_refresh: true,
        creates: 0,
    };
    let mut rng = 20;
    assert_eq!(
        arena.spawn_selector36_child(parent, &mut host, &mut rng),
        Err(ActorChildError::Service("refresh"))
    );
    assert_eq!(
        arena.object(child).unwrap().base.position_words,
        Some([1, 103, 3])
    );
    assert_eq!(arena.actor(child).unwrap().words_8e_92, [0, 50, 0]);
    assert_eq!(arena.actor(parent).unwrap().word_74, 99);
    assert_eq!(arena.selector36_actor(parent).unwrap().word_27e, 23);
    // The native1000 publication prevents another child even if an outer
    // caller later revisits this owner. It does not complete failed refresh.
    arena
        .spawn_selector36_child(parent, &mut host, &mut rng)
        .unwrap();
    assert_eq!(host.creates, 1);
    assert_eq!(rng, 21);
}
