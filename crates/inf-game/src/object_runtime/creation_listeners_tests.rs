use super::*;
use crate::object_runtime::ObjectClassKind;

// These objects supply stable identities for list-protocol tests. Test effects
// deliberately do not claim to implement selector 18 or 78 actor behavior.
fn identities(arena: &mut ObjectArena, count: usize) -> Vec<ObjectId> {
    (0..count)
        .map(|_| arena.allocate(ObjectClassKind::Base))
        .collect()
}

fn append(listeners: &mut CreationListeners, owner: ObjectId) -> CreationListenerId {
    listeners.register(owner, CreationListenerInterface::Selector18)
}

fn assert_finished(listeners: &CreationListeners) {
    let mut previous = HEAD;
    let mut next = listeners.node(HEAD).next;
    let mut visited = vec![HEAD];
    while next != TAIL {
        assert!(!visited.contains(&next), "linked cycle");
        let node = listeners.node(next);
        assert_eq!(node.previous, previous);
        assert!(node.listener.is_some(), "notification cursor leaked");
        visited.push(next);
        previous = next;
        next = node.next;
    }
    assert_eq!(listeners.node(TAIL).previous, previous);
    visited.push(TAIL);
    assert_eq!(visited.len(), listeners.len() + 2);
    for (index, slot) in listeners.slots.iter().enumerate() {
        assert_eq!(slot.node.is_some(), visited.contains(&index));
        assert_eq!(slot.node.is_none(), listeners.free.contains(&index));
    }
}

#[derive(Default)]
struct Recording {
    calls: Vec<(ObjectId, CreationListenerId)>,
}

impl CreationListenerEffects for Recording {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls.push((actor, listener.id));
        Ok(())
    }
}

#[test]
fn stopped_construction_listener_keeps_binding_and_continues_to_healthy_owner() {
    let mut arena = ObjectArena::new();
    let objects = identities(&mut arena, 3);
    let mut listeners = CreationListeners::new();
    let stopped = append(&mut listeners, objects[0]);
    let healthy = append(&mut listeners, objects[1]);
    arena.stop_object_construction(objects[0]).unwrap();
    let mut effects = Recording::default();

    listeners
        .notify(objects[2], &mut arena, &mut effects)
        .unwrap();

    assert_eq!(effects.calls, [(objects[2], healthy)]);
    assert_eq!(listeners.get(stopped).unwrap().owner, objects[0]);
    assert_eq!(listeners.len(), 2);
    assert!(arena.construction_is_stopped(objects[0]).unwrap());
    assert_finished(&listeners);
}

#[test]
fn stopped_disposal_listener_preserves_binding_and_cursor_without_callback() {
    let mut arena = ObjectArena::new();
    let objects = identities(&mut arena, 3);
    let mut listeners = CreationListeners::new();
    let stopped = append(&mut listeners, objects[0]);
    let healthy = append(&mut listeners, objects[1]);
    arena.dispose(objects[0]).unwrap();
    // Supplied retained-destructor boundary; disposal tests separately prove
    // that real failures install this allocation-owned custody.
    arena.owned_mut(objects[0]).unwrap().disposal_stop =
        Some(crate::object_runtime::ObjectDisposalStop {
            object: objects[0],
            phase: crate::object_runtime::ObjectDisposalPhase::DeletingDestructor,
            reason: "supplied destructor stop".into(),
        });
    let before = format!("{arena:?}");
    let mut effects = Recording::default();
    listeners
        .notify(objects[2], &mut arena, &mut effects)
        .unwrap();
    assert_eq!(effects.calls, [(objects[2], healthy)]);
    assert_eq!(listeners.get(stopped).unwrap().owner, objects[0]);
    assert_eq!(listeners.len(), 2);
    assert_eq!(format!("{arena:?}"), before);
    assert_finished(&listeners);
}

#[test]
fn binding_ids_and_owner_cleanup_do_not_destroy_objects() {
    let mut arena = ObjectArena::new();
    let objects = identities(&mut arena, 2);
    let mut listeners = CreationListeners::new();
    let first = append(&mut listeners, objects[0]);
    let middle = append(&mut listeners, objects[1]);
    let last = listeners.register(objects[0], CreationListenerInterface::Selector78);
    assert_eq!(listeners.unregister_owner(objects[0]), 2);
    assert_eq!(
        listeners.registrations().map(|v| v.id).collect::<Vec<_>>(),
        [middle]
    );
    assert!(matches!(
        listeners.get(first),
        Err(CreationListenerError::UnknownListener(_))
    ));
    assert!(matches!(
        listeners.unregister(last),
        Err(CreationListenerError::UnknownListener(_))
    ));
    let reused = append(&mut listeners, objects[0]);
    assert_ne!(reused, first);
    assert_ne!(reused, last);
    let mut other = CreationListeners::new();
    let foreign = append(&mut other, objects[1]);
    assert!(matches!(
        listeners.unregister(foreign),
        Err(CreationListenerError::UnknownListener(_))
    ));
    listeners.clear();
    assert!(listeners.is_empty());
    assert!(arena.object(objects[0]).is_ok());
    assert!(arena.object(objects[1]).is_ok());
    assert_finished(&listeners);
    assert_finished(&other);
}

struct Reject {
    calls: usize,
}

impl CreationListenerEffects for Reject {
    fn invoke(
        &mut self,
        listener: CreationListener,
        _actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls += 1;
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

#[test]
fn empty_walk_needs_no_callback_but_a_reached_binding_requires_dispatch() {
    let mut arena = ObjectArena::new();
    let object = identities(&mut arena, 1)[0];
    let mut listeners = CreationListeners::new();
    let mut effects = Reject { calls: 0 };
    listeners.notify(object, &mut arena, &mut effects).unwrap();
    assert_eq!(effects.calls, 0);
    assert_finished(&listeners);
    let binding = append(&mut listeners, object);
    assert_eq!(
        listeners.notify(object, &mut arena, &mut effects),
        Err(CreationListenerError::UnsupportedInterface(
            CreationListenerInterface::Selector18
        ))
    );
    assert_eq!(effects.calls, 1);
    assert!(listeners.get(binding).is_ok());
    assert_finished(&listeners);
    let mut recording = Recording::default();
    listeners
        .notify(object, &mut arena, &mut recording)
        .unwrap();
    assert_eq!(recording.calls, [(object, binding)]);
    assert_finished(&listeners);
}

enum Mutation {
    RemoveCurrentAndNext(CreationListenerId),
    Clear,
    RemoveCurrent,
    None,
}

struct Mutating {
    calls: Vec<CreationListenerId>,
    first_action: Option<Mutation>,
    append_owner: ObjectId,
    appended: Option<CreationListenerId>,
}

impl CreationListenerEffects for Mutating {
    fn invoke(
        &mut self,
        listener: CreationListener,
        _actor: ObjectId,
        listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls.push(listener.id);
        if let Some(action) = self.first_action.take() {
            match action {
                Mutation::RemoveCurrentAndNext(next) => {
                    listeners.unregister(listener.id)?;
                    listeners.unregister(next)?;
                }
                Mutation::Clear => listeners.clear(),
                Mutation::RemoveCurrent => {
                    listeners.unregister(listener.id)?;
                }
                Mutation::None => {}
            }
            self.appended = Some(append(listeners, self.append_owner));
        }
        Ok(())
    }
}

#[test]
fn removing_current_and_next_skips_deleted_binding_and_visits_append() {
    let mut arena = ObjectArena::new();
    let objects = identities(&mut arena, 4);
    let mut listeners = CreationListeners::new();
    let a = append(&mut listeners, objects[0]);
    let b = append(&mut listeners, objects[1]);
    let c = append(&mut listeners, objects[2]);
    let mut effects = Mutating {
        calls: vec![],
        first_action: Some(Mutation::RemoveCurrentAndNext(b)),
        append_owner: objects[3],
        appended: None,
    };
    listeners
        .notify(objects[0], &mut arena, &mut effects)
        .unwrap();
    assert_eq!(effects.calls, [a, c, effects.appended.unwrap()]);
    assert_eq!(listeners.len(), 2);
    assert_finished(&listeners);
}

#[test]
fn appends_after_last_callback_and_clear_during_walk_preserve_cursor() {
    for mutation in [Mutation::None, Mutation::Clear, Mutation::RemoveCurrent] {
        let mut arena = ObjectArena::new();
        let object = identities(&mut arena, 1)[0];
        let mut listeners = CreationListeners::new();
        let a = append(&mut listeners, object);
        let mut effects = Mutating {
            calls: vec![],
            first_action: Some(mutation),
            append_owner: object,
            appended: None,
        };
        listeners.notify(object, &mut arena, &mut effects).unwrap();
        assert_eq!(effects.calls, [a, effects.appended.unwrap()]);
        assert_finished(&listeners);
    }
}

struct Nested {
    calls: Vec<(ObjectId, CreationListenerId)>,
    nested_actor: ObjectId,
    started: bool,
    mutate_inner: bool,
    appended: Option<CreationListenerId>,
}

impl CreationListenerEffects for Nested {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls.push((actor, listener.id));
        if !self.started {
            self.started = true;
            listeners.notify(self.nested_actor, arena, self)?;
        } else if actor == self.nested_actor && self.mutate_inner && self.appended.is_none() {
            // Both walks are at this binding. Unlinking it must update both
            // cursors; each walk must later see the newly appended binding.
            listeners.unregister(listener.id)?;
            self.appended = Some(append(listeners, listener.owner));
        }
        Ok(())
    }
}

#[test]
fn nested_walks_skip_other_cursors_and_share_live_mutations() {
    for mutate_inner in [false, true] {
        let mut arena = ObjectArena::new();
        let objects = identities(&mut arena, 2);
        let mut listeners = CreationListeners::new();
        let a = append(&mut listeners, objects[0]);
        let b = append(&mut listeners, objects[1]);
        let mut effects = Nested {
            calls: vec![],
            nested_actor: objects[1],
            started: false,
            mutate_inner,
            appended: None,
        };
        listeners
            .notify(objects[0], &mut arena, &mut effects)
            .unwrap();
        let mut expected = vec![(objects[0], a), (objects[1], a), (objects[1], b)];
        if let Some(appended) = effects.appended {
            expected.push((objects[1], appended));
        }
        expected.push((objects[0], b));
        if let Some(appended) = effects.appended {
            expected.push((objects[0], appended));
        }
        assert_eq!(effects.calls, expected);
        assert_finished(&listeners);
    }
}

struct FailureAfterMutation;

impl CreationListenerEffects for FailureAfterMutation {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        listeners.unregister_owner(listener.owner);
        arena.apply_payload_flags(actor, 0x1234)?;
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

#[test]
fn callback_failure_cleans_only_cursor_and_preserves_prior_effects() {
    let mut arena = ObjectArena::new();
    let objects = identities(&mut arena, 2);
    let mut listeners = CreationListeners::new();
    let removed = append(&mut listeners, objects[0]);
    let remaining = append(&mut listeners, objects[1]);
    assert!(listeners
        .notify(objects[0], &mut arena, &mut FailureAfterMutation)
        .is_err());
    assert!(listeners.get(removed).is_err());
    assert_eq!(arena.object(objects[0]).unwrap().base.flags, 0x0800_1234);
    assert_finished(&listeners);
    let mut recording = Recording::default();
    listeners
        .notify(objects[0], &mut arena, &mut recording)
        .unwrap();
    assert_eq!(recording.calls, [(objects[0], remaining)]);
    assert_finished(&listeners);
}

#[test]
fn completed_walks_reuse_cursor_storage() {
    let mut arena = ObjectArena::new();
    let object = identities(&mut arena, 1)[0];
    let mut listeners = CreationListeners::new();
    let binding = append(&mut listeners, object);
    let mut recording = Recording::default();
    for _ in 0..100 {
        listeners
            .notify(object, &mut arena, &mut recording)
            .unwrap();
    }
    assert_eq!(recording.calls, vec![(object, binding); 100]);
    assert_eq!(listeners.slots.len(), 4); // Two sentinels, binding, cursor.
    assert_finished(&listeners);
}
