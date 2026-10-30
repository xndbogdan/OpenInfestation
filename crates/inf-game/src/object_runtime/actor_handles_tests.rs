use super::*;
use crate::object_runtime::{ObjectArena, ObjectClassKind};

fn registry() -> ActorHandles {
    ActorHandles::new(3, 8, ActorHandle::from_raw(0)).unwrap()
}

fn objects<const N: usize>() -> [ObjectId; N] {
    let mut arena = ObjectArena::new();
    // The registry stores opaque IDs and never dereferences or destroys them.
    std::array::from_fn(|_| arena.allocate(ObjectClassKind::Base))
}

fn handle(owner: u8, slot: u16, generation: u16, marked: bool) -> ActorHandle {
    ActorHandle::from_raw(
        (u32::from(owner) << 27)
            | (u32::from(slot) << 16)
            | u32::from(generation)
            | if marked { MARK } else { 0 },
    )
}

fn free_order(registry: &ActorHandles, owner: u8) -> Vec<usize> {
    let table = registry.owner(owner).unwrap();
    let mut result = Vec::new();
    let mut slot = table.free_head;
    while slot > 0 {
        assert!(!result.contains(&(slot as usize)), "unexpected free cycle");
        result.push(slot as usize);
        slot = table.slots[slot as usize].next_free;
    }
    result
}

#[test]
fn packed_identity_and_configured_domain_are_explicit() {
    let value = handle(31, 1023, 0xabcd, true);
    assert_eq!(value.raw(), 0xffff_abcd);
    assert_eq!(
        (value.owner(), value.slot(), value.generation()),
        (31, 1023, 0xabcd)
    );
    assert!(value.marked());
    for count in [0, 1, 33] {
        assert!(matches!(
            ActorHandles::new(count, 8, value),
            Err(ActorHandleError::OwnerCount(_))
        ));
    }
    for capacity in [0, 7, 1025] {
        assert!(matches!(
            ActorHandles::new(2, capacity, value),
            Err(ActorHandleError::InitialCapacity(_))
        ));
    }
    let mut registry = ActorHandles::new(32, 1024, value).unwrap();
    assert_eq!(registry.cache().handle, value);
    assert_eq!(registry.resolve(value), None);
    assert_eq!(registry.owner(0).unwrap().capacity(), 0);
    assert_eq!(registry.owner(31).unwrap().slots[0].next_free, -1);
}

#[test]
fn allocation_and_release_preserve_native_queue_and_stale_fields() {
    let mut registry = registry();
    let [a, b, c] = objects();
    let a_handle = registry.allocate(1, a).unwrap();
    let b_handle = registry.allocate(1, b).unwrap();
    assert_eq!(a_handle, handle(1, 7, 1, false));
    assert_eq!(b_handle, handle(1, 6, 1, false));
    assert_eq!(registry.owner(1).unwrap().slots[7].next_free, 6);
    assert_eq!(registry.cache().handle.raw(), 0);
    assert!(registry.release(a_handle));
    assert!(registry.release(b_handle));
    assert!(!registry.release(a_handle));
    assert_eq!(free_order(&registry, 1), [5, 4, 3, 2, 1, 7, 6]);
    let c_handle = registry.allocate(1, c).unwrap();
    assert_eq!(c_handle.slot(), 5);
    assert_eq!(registry.owner(1).unwrap().free_tail, 6);
    assert_eq!(registry.owner(1).unwrap().slots[7].handle, a_handle);
}

#[test]
fn exhausted_release_keeps_retails_stale_tail_self_link() {
    let mut registry = registry();
    let ids = objects::<8>();
    let handles: Vec<_> = ids[..7]
        .iter()
        .map(|id| registry.allocate(1, *id).unwrap())
        .collect();
    assert_eq!(registry.owner(1).unwrap().free_head, 0);
    assert_eq!(registry.owner(1).unwrap().free_tail, 1);
    assert!(registry.release(handles[6]));
    let table = registry.owner(1).unwrap();
    assert_eq!(
        (table.free_head, table.free_tail, table.slots[1].next_free),
        (1, 1, 1)
    );
    let replacement = registry.allocate(1, ids[7]).unwrap();
    assert_eq!(replacement, handle(1, 1, 2, false));
    assert_eq!(registry.owner(1).unwrap().free_head, 1);
    let rebound = handle(1, 1, 3, false);
    registry.bind(rebound, ids[0]).unwrap();
    assert_eq!(registry.resolve(replacement), None);
    assert_eq!(registry.resolve(rebound), Some(ids[0]));
    assert_eq!(registry.owner(1).unwrap().free_head, 1);
    assert_eq!(registry.owner(1).unwrap().free_tail, 0);
}

#[test]
fn lookup_caches_empty_matches_but_not_invalid_or_stale_identities() {
    let mut registry = registry();
    let [object] = objects();
    let occupied = registry.allocate(1, object).unwrap();
    assert_eq!(registry.resolve(occupied), Some(object));
    let cache = registry.cache();
    assert_eq!(registry.resolve(handle(1, 7, 2, false)), None);
    assert_eq!(registry.resolve(handle(0, 7, 1, false)), None);
    assert_eq!(registry.cache(), cache);
    let empty = handle(1, 6, 0, false);
    assert_eq!(registry.resolve(empty), None);
    assert_eq!(
        registry.cache(),
        ActorHandleCache {
            handle: empty,
            object: None
        }
    );
    assert!(!registry.release(empty));
    assert_eq!(registry.cache().handle.raw(), 0);
    assert_eq!(registry.resolve(occupied), Some(object));
    assert!(registry.release(occupied));
    assert_eq!(registry.cache().handle.raw(), 0);
    assert_eq!(registry.resolve(occupied), None);
    assert_eq!(registry.cache().handle, occupied);
}

#[test]
fn binding_unlinks_head_middle_and_tail_and_preserves_supplied_bits() {
    let mut registry = registry();
    let [a, b, c] = objects();
    for (value, object) in [
        (handle(1, 7, 0, true), a),
        (handle(1, 4, 0xffff, false), b),
        (handle(1, 1, 13, true), c),
    ] {
        registry.bind(value, object).unwrap();
        assert_eq!(registry.resolve(value), Some(object));
    }
    assert_eq!(free_order(&registry, 1), [6, 5, 3, 2]);
    assert_eq!(registry.owner(1).unwrap().free_tail, 2);
    assert_eq!(registry.owner(1).unwrap().slots[4].next_free, 3);
    assert!(matches!(
        registry.bind(handle(1, 4, 0xffff, false), c),
        Err(ActorHandleError::AlreadyBound(_))
    ));
    assert_eq!(
        registry.bind(handle(1, 4, 123, false), c),
        Err(ActorHandleError::FreeChain { owner: 1, slot: 4 })
    );
    assert_eq!(registry.resolve(handle(1, 4, 0xffff, false)), Some(b));
    assert!(matches!(
        registry.bind(handle(1, 0, 1, false), a),
        Err(ActorHandleError::ReservedSlot(_))
    ));
    assert_eq!(registry.allocate(0, a), Err(ActorHandleError::Owner(0)));
}

#[test]
fn mode1_matches_full_identity_even_when_empty_and_survives_reuse() {
    let mut registry = registry();
    let [object] = objects();
    let mut value = handle(1, 7, 0, false);
    assert!(registry.mark_mode1(&mut value));
    assert!(value.marked());
    assert_eq!(registry.cache().handle.raw(), 0);
    let mut stale = handle(1, 7, 0, false);
    assert!(!registry.mark_mode1(&mut stale));
    registry.owners[1].slots[7].handle = handle(1, 7, u16::MAX, true);
    let allocated = registry.allocate(1, object).unwrap();
    assert_eq!(allocated, handle(1, 7, 1, true));
    assert_eq!(registry.resolve(allocated), Some(object));
    let mut marked = allocated;
    assert!(registry.mark_mode1(&mut marked));
    assert_eq!(registry.cache().handle.raw(), 0);
}

#[test]
fn mode2_checks_empty_generation_and_can_admit_reserved_zero() {
    let mut registry = registry();
    let [object] = objects();
    assert!(!registry.mode2_admissible(handle(0, 1, 1, false)).unwrap());
    assert!(!registry.mode2_admissible(handle(1, 7, 0, true)).unwrap());
    assert!(registry.mode2_admissible(handle(1, 7, 1, true)).unwrap());
    assert!(registry.mode2_admissible(handle(1, 0, 1, false)).unwrap());
    let allocated = registry.allocate(1, object).unwrap();
    assert!(!registry.mode2_admissible(handle(1, 7, 9, false)).unwrap());
    registry.release(allocated);
    assert!(!registry.mode2_admissible(handle(1, 7, 1, true)).unwrap());
    assert!(registry.mode2_admissible(handle(1, 7, 2, false)).unwrap());
    assert!(registry.mode2_admissible(handle(1, 19, 1, false)).unwrap());
    assert_eq!(registry.owner(1).unwrap().capacity(), 32);
}

#[test]
fn growth_rebuilds_free_order_and_rebinds_occupied_slots_in_index_order() {
    let mut registry = registry();
    let [a, b, c] = objects();
    let a_handle = registry.allocate(1, a).unwrap();
    let b_handle = registry.allocate(1, b).unwrap();
    registry.release(a_handle);
    let mut marked_free = a_handle;
    registry.mark_mode1(&mut marked_free);
    assert_eq!(free_order(&registry, 1), [5, 4, 3, 2, 1, 7]);
    let before = registry.owner(2).unwrap().clone();
    assert!(registry.mode2_admissible(handle(1, 10, 1, false)).unwrap());
    assert_eq!(
        registry.cache(),
        ActorHandleCache {
            handle: b_handle,
            object: Some(b)
        }
    );
    assert_eq!(registry.owner(1).unwrap().slots[7].handle, marked_free);
    assert_eq!(
        free_order(&registry, 1),
        [15, 14, 13, 12, 11, 10, 9, 8, 7, 5, 4, 3, 2, 1]
    );
    assert_eq!(registry.owner(2).unwrap(), &before);
    registry.bind(handle(1, 20, 7, true), c).unwrap();
    assert_eq!(registry.owner(1).unwrap().capacity(), 32);
    assert_eq!(registry.resolve(b_handle), Some(b));
    assert_eq!(registry.initial_capacity(), 8);
}

#[test]
fn capacity_growth_uses_doubling_and_exhaustion_is_bounded() {
    let [object] = objects();
    let mut registry = ActorHandles::new(2, 9, ActorHandle::from_raw(0)).unwrap();
    registry.bind(handle(1, 18, 42, false), object).unwrap();
    assert_eq!(registry.owner(1).unwrap().capacity(), 36);
    registry.bind(handle(1, 1023, 42, false), object).unwrap();
    assert_eq!(registry.owner(1).unwrap().capacity(), 1024);
    for _ in 0..1021 {
        registry.allocate(1, object).unwrap();
    }
    assert_eq!(
        registry.allocate(1, object),
        Err(ActorHandleError::Exhausted(1))
    );
}

#[test]
fn reassignment_publishes_before_bind_and_does_not_roll_back_failure() {
    let mut registry = registry();
    let [object, other] = objects();
    let mut current = registry.allocate(1, object).unwrap();
    let old = current;
    let same = current;
    registry.reassign(object, &mut current, same).unwrap();
    assert_eq!(current, old);
    assert_eq!(registry.resolve(current), Some(object));
    let occupied = registry.allocate(1, other).unwrap();
    assert_eq!(
        registry.reassign(object, &mut current, occupied),
        Err(ActorHandleError::AlreadyBound(occupied))
    );
    assert_eq!(current, occupied);
    assert_eq!(registry.resolve(old), None);
    assert_eq!(registry.resolve(occupied), Some(other));
}

#[test]
fn repeated_configuration_preserves_grown_state_and_clear_keeps_configured_size() {
    let mut registry = registry();
    let [object] = objects();
    let value = handle(1, 18, 1, false);
    registry.bind(value, object).unwrap();
    let owners = registry.owners.clone();
    let cache = registry.cache();
    registry.configure(3, 8).unwrap();
    assert_eq!(registry.owners, owners);
    assert_eq!(registry.cache(), cache);
    registry.configure(2, 16).unwrap();
    assert_eq!(registry.owners().len(), 2);
    assert_eq!(registry.resolve(value), None);
    registry.clear();
    assert!(registry.owners().is_empty());
    assert_eq!(registry.initial_capacity(), 16);
    assert_eq!(
        registry.allocate(1, object),
        Err(ActorHandleError::Owner(1))
    );
    registry.configure(2, 16).unwrap();
    assert_eq!(registry.allocate(1, object).unwrap().slot(), 15);
}

/// 00462510 and 00462560 (message 7's slot routines): the slot's object
/// whatever the generation, and an empty slot forgetting the generation it
/// still holds.
#[test]
fn a_given_up_handle_frees_its_empty_slot() {
    let mut registry = registry();
    let [object] = objects::<1>();
    let held = registry.allocate(2, object).unwrap();
    let other = handle(2, held.slot(), held.generation().wrapping_add(1), false);
    assert_eq!(registry.slot_object(held), Some(object));
    assert_eq!(registry.slot_object(other), Some(object));
    assert!(!registry.forget_slot(held));
    assert!(registry.release(held));
    assert_eq!(registry.slot_object(held), None);
    assert!(!registry.forget_slot(other));
    assert!(registry.forget_slot(held));
    let stored = registry.owner(2).unwrap().slots[usize::from(held.slot())].handle;
    assert_eq!(stored.generation(), 0);
    assert_eq!(stored.raw() >> 16, held.raw() >> 16);
}
