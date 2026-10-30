use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorExtension, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorClass, DerivedActorConstructionRequest, Family428650Class, Family429e20Class,
    ObjectClass, ObjectError, ObjectState, Selector12GrantContext, Selector12State,
};
use inf_formats::{GrantTable, GRANT_MASK_WORDS, GRANT_TABLE_ADDRESS};

// This constructor arm does not read terrain or sine data.
const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn actor_request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
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
    }
}

fn fixture(source: &[u16], byte_29d: u8) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x0801_0000),
    })
    .unwrap();
    // No first-player source means the constructor does not consume this table.
    let grants = GrantTable {
        virtual_address: GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let mut rng = 0;
    let actor = arena
        .allocate_selector12_actor(
            DerivedActorConstructionRequest {
                actor: actor_request(),
                world_mask_limits: [3, 3],
            },
            &mut rng,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    let selector = arena.selector12_actor_mut(actor).unwrap();
    selector.grants_24c.mask_04 = mask(source);
    selector.byte_29d = byte_29d;
    (arena, actor)
}

fn mask(ids: &[u16]) -> [u32; GRANT_MASK_WORDS] {
    let mut mask = [0; GRANT_MASK_WORDS];
    for &id in ids {
        mask[usize::from(id >> 5)] |= 1_u32 << (id & 31);
    }
    mask
}

fn selector_in_state(state: &mut ObjectState) -> &mut Selector12State {
    let ObjectClass::Actor(actor) = &mut state.class else {
        panic!("actual actor")
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        panic!("actual derived actor")
    };
    let DerivedActorClass::Family428650(family) = &mut derived.class else {
        panic!("actual 00428650 family")
    };
    let Family428650Class::Family429e20(probe_family) = &mut family.class else {
        panic!("actual probe family")
    };
    let Family429e20Class::Selector12(selector) = &mut probe_family.class else {
        panic!("actual selector12")
    };
    selector
}

/// Compare the entire actor, player records, registry and ordered arena nodes.
/// The one admitted event write is a literal +29D byte; it has no dirty bit.
fn notify(arena: &mut ObjectArena, actor: ObjectId, byte_29d: u8) {
    let mut expected = arena.object(actor).unwrap().clone();
    selector_in_state(&mut expected).byte_29d = byte_29d;
    let players = arena.world_players().to_vec();
    let lists = arena.lists.clone();
    let actors = arena.actors.clone();
    let handles = arena.actor_handles().unwrap();
    let owners = handles.owners().to_vec();
    let cache = handles.cache();
    assert_eq!(
        arena
            .notify_selector12_grant_difference(Selector12GrantDifferenceRequest { actor })
            .unwrap(),
        0,
    );
    assert_eq!(arena.object(actor).unwrap(), &expected);
    assert_eq!(arena.world_players(), players);
    assert_eq!(arena.lists, lists);
    assert_eq!(arena.actors, actors);
    assert_eq!(arena.actor_handles().unwrap().owners(), owners);
    assert_eq!(arena.actor_handles().unwrap().cache(), cache);
}

#[test]
fn no_player_preserves_byte_and_each_call_selects_the_actual_first_player() {
    let (mut arena, actor) = fixture(&[1, 174], 0xa5);
    notify(&mut arena, actor, 0xa5);

    let first = arena.add_world_player(mask(&[1]));
    notify(&mut arena, actor, 1);
    // A later real append cannot substitute its complete mask for the first.
    let second = arena.add_world_player(mask(&[1, 174]));
    assert_ne!(first, second);
    assert_eq!(arena.first_player(), Some(first));
    notify(&mut arena, actor, 1);

    // Fresh actual owner data is read on the next request, rather than cached.
    arena.players.list[0].grants_9c = mask(&[1, 174]);
    notify(&mut arena, actor, 0);
    arena.players.list[1].grants_9c = mask(&[]);
    notify(&mut arena, actor, 0);
}

#[test]
fn bit_zero_and_empty_source_do_not_create_a_difference() {
    for (source, target) in [(&[][..], &[0][..]), (&[0][..], &[][..])] {
        let (mut arena, actor) = fixture(source, 0xfe);
        arena.add_world_player(mask(target));
        notify(&mut arena, actor, 0);
    }
}

#[test]
fn source_only_ids_one_and_174_write_literal_one_and_high_ids_are_ignored() {
    for id in [1, 174] {
        let (mut arena, actor) = fixture(&[id, 175, 191], 0xfe);
        arena.add_world_player(mask(&[175, 190]));
        notify(&mut arena, actor, 1);
    }
    let (mut arena, actor) = fixture(&[175, 191], 0xfe);
    arena.add_world_player(mask(&[190]));
    notify(&mut arena, actor, 0);
}

#[test]
fn actor_source_extra_and_player_target_extra_are_intentionally_asymmetric() {
    let (mut arena, actor) = fixture(&[5, 96], 0xfe);
    arena.add_world_player(mask(&[5]));
    notify(&mut arena, actor, 1);
    arena
        .selector12_actor_mut(actor)
        .unwrap()
        .grants_24c
        .mask_04 = mask(&[5]);
    arena.players.list[0].grants_9c = mask(&[5, 96]);
    notify(&mut arena, actor, 0);
}

#[test]
fn represented_selector12_admission_does_not_dispatch_on_mutable_b4() {
    let (mut arena, actor) = fixture(&[32], 0xfe);
    arena.add_world_player(mask(&[]));
    for class in [0, 12, 13, u32::MAX] {
        arena.actor_mut(actor).unwrap().word_b4 = class;
        notify(&mut arena, actor, 1);
    }

    let shared = arena.allocate_actor(actor_request()).unwrap();
    arena.actor_mut(shared).unwrap().word_b4 = 12;
    let before = arena.object(shared).unwrap().clone();
    let players = arena.world_players().to_vec();
    assert!(matches!(
        arena.notify_selector12_grant_difference(Selector12GrantDifferenceRequest { actor: shared }),
        Err(Selector12GrantDifferenceError::Derived(
            DerivedActorError::Object(ObjectError::NotDerivedActor(id))
        )) if id == shared
    ));
    assert_eq!(arena.object(shared).unwrap(), &before);
    assert_eq!(arena.world_players(), players);

    // Retire the actually owned Shared class, without inventing a selector12
    // destructor or relying on the raw callback's stale-this/no-player bypass.
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: shared,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(matches!(
        arena.notify_selector12_grant_difference(Selector12GrantDifferenceRequest { actor: shared }),
        Err(Selector12GrantDifferenceError::Derived(
            DerivedActorError::Object(ObjectError::UnknownObject(id))
        )) if id == shared
    ));
}

#[test]
fn id_zero_bypasses_invalid_owners_and_source_failure_precedes_target() {
    let (mut arena, actor) = fixture(&[], 0xfe);
    let shared = arena.allocate_actor(actor_request()).unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: shared,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let mut foreign = ObjectArena::new();
    let foreign_player = foreign.add_world_player(mask(&[1]));
    let source = GrantOwner::Selector12Actor24c(shared);
    let target = GrantOwner::Player9c(foreign_player);
    assert!(grant_owner_has(&arena, source, 0).unwrap());
    assert!(grant_owner_has(&arena, target, 0).unwrap());
    assert!(matches!(
        grant_difference(&arena, source, target),
        Err(Selector12GrantDifferenceError::Derived(
            DerivedActorError::Object(ObjectError::UnknownObject(id))
        )) if id == shared
    ));
    // The invalid target is an actual PlayerId from another arena, with no
    // corresponding live player here. Empty source skips its nonzero queries.
    assert!(!grant_difference(&arena, GrantOwner::Selector12Actor24c(actor), target).unwrap());
    arena
        .selector12_actor_mut(actor)
        .unwrap()
        .grants_24c
        .mask_04 = mask(&[1]);
    assert!(matches!(
        grant_difference(&arena, GrantOwner::Selector12Actor24c(actor), target),
        Err(Selector12GrantDifferenceError::Player(PlayerAttachError::UnknownPlayer(id)))
            if id == foreign_player
    ));
}

#[test]
fn captured_equal_owner_identities_are_valid_without_mutable_mask_borrows() {
    let (mut arena, actor) = fixture(&[1, 31, 32, 96, 174, 191], 0xfe);
    let actor_owner = GrantOwner::Selector12Actor24c(actor);
    assert!(!grant_difference(&arena, actor_owner, actor_owner).unwrap());
    let player = arena.add_world_player(mask(&[1, 31, 32, 96, 174, 191]));
    let player_owner = GrantOwner::Player9c(player);
    assert!(!grant_difference(&arena, player_owner, player_owner).unwrap());
    notify(&mut arena, actor, 0);
}

#[test]
fn ignored_owner_headers_and_unavailable_actor_inputs_are_not_consumed() {
    let (mut arena, actor) = fixture(&[65, 174, 191], 0xfe);
    let selector = arena.selector12_actor_mut(actor).unwrap();
    selector.grants_24c.vtable_00 = 0xdead_beef;
    selector.grants_24c.notification_owner_1c = u32::MAX;
    arena.actor_mut(actor).unwrap().handle = ActorHandle::from_raw(0x1234_5678);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    let player = arena.add_world_player(mask(&[65, 174, 190]));
    assert_eq!(arena.first_player(), Some(player));
    // Unresolved player handles are ignored: this event reads only +9C masks.
    arena.players.list[0].handle_d4 = ActorHandle::from_raw(0xffff_ffff);
    arena.players.list[0].handle_d8 = ActorHandle::from_raw(0x1234_5678);
    notify(&mut arena, actor, 0);
}
