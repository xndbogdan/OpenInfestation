//! Creation-listener bindings and moving cursors, 00460120/00461BE0/00461C00.
//!
//! Native bindings belong to both the global 0054E868 list and the interface
//! owner. The owner must unregister its bindings before destroying its callback
//! interface. Clearing the global list releases bindings, not their objects.
//! Selector-specific callbacks remain required effects, not successful defaults.

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use super::{ObjectArena, ObjectError, ObjectId};

static NEXT_LIST: AtomicU64 = AtomicU64::new(1);
const HEAD: usize = 0;
const TAIL: usize = 1;

/// Stable binding identity, distinct across registries and reused slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CreationListenerId {
    list: u64,
    slot: usize,
    generation: u64,
}

/// The two concrete actor +138 interfaces that register with 0054E868.
/// Naming by selector avoids assigning unproven gameplay meaning to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationListenerInterface {
    /// 00476750 registers vtable 005279DC; first method is 00477350.
    Selector18,
    /// 00406640 registers vtable 0051FA98; first method is 00407350.
    Selector78,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationListener {
    pub id: CreationListenerId,
    pub owner: ObjectId,
    pub interface: CreationListenerInterface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum CreationListenerError {
    #[error("unknown or expired creation listener {0:?}")]
    UnknownListener(CreationListenerId),
    #[error("creation-listener callback {0:?} is not implemented")]
    UnsupportedInterface(CreationListenerInterface),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

/// Concrete callback execution is mandatory when a binding is reached. Effects
/// may unregister/register listeners, mutate objects, or notify recursively.
/// No default implementation stands in for either retail interface method.
pub trait CreationListenerEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError>;
}

#[derive(Clone, Debug)]
struct Node {
    previous: usize,
    next: usize,
    /// None is a sentinel or active traversal cursor, never a callback.
    listener: Option<CreationListener>,
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u64,
    node: Option<Node>,
}

/// Binding ownership for the global creation-listener list. This does not own
/// actor allocations: destruction must call `unregister_owner` first. Multiple
/// bindings per interface are retained independently, as in the two native
/// embedded-list nodes of each 0x1C-byte binding.
#[derive(Clone, Debug)]
pub struct CreationListeners {
    identity: u64,
    slots: Vec<Slot>,
    free: Vec<usize>,
    count: usize,
}

impl Default for CreationListeners {
    fn default() -> Self {
        Self::new()
    }
}

impl CreationListeners {
    /// 00460010, a CRT initializer, empties the list 0054E868: its head and
    /// tail sentinels link to each other.
    pub fn new() -> Self {
        Self {
            identity: NEXT_LIST.fetch_add(1, Ordering::Relaxed),
            slots: vec![
                Slot {
                    generation: 0,
                    node: Some(Node {
                        previous: HEAD,
                        next: TAIL,
                        listener: None,
                    }),
                },
                Slot {
                    generation: 0,
                    node: Some(Node {
                        previous: HEAD,
                        next: TAIL,
                        listener: None,
                    }),
                },
            ],
            free: Vec::new(),
            count: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Append a binding at the global and owner-local tails. The caller owns
    /// the interface's lifetime and supplies its actual concrete selector.
    pub fn register(
        &mut self,
        owner: ObjectId,
        interface: CreationListenerInterface,
    ) -> CreationListenerId {
        let slot = self.allocate_node();
        let id = CreationListenerId {
            list: self.identity,
            slot,
            generation: self.slots[slot].generation,
        };
        self.node_mut(slot).listener = Some(CreationListener {
            id,
            owner,
            interface,
        });
        self.insert_before(slot, TAIL);
        self.count += 1;
        id
    }

    pub fn get(&self, id: CreationListenerId) -> Result<CreationListener, CreationListenerError> {
        self.slots
            .get(id.slot)
            .and_then(|slot| slot.node.as_ref())
            .and_then(|node| node.listener)
            .filter(|listener| listener.id == id)
            .ok_or(CreationListenerError::UnknownListener(id))
    }

    /// Read global binding order, excluding all nested notification cursors.
    pub fn registrations(&self) -> impl Iterator<Item = CreationListener> + '_ {
        let mut next = self.node(HEAD).next;
        std::iter::from_fn(move || {
            while next != TAIL {
                let node = self.node(next);
                next = node.next;
                if let Some(listener) = node.listener {
                    return Some(listener);
                }
            }
            None
        })
    }

    pub fn unregister(
        &mut self,
        id: CreationListenerId,
    ) -> Result<CreationListener, CreationListenerError> {
        let listener = self.get(id)?;
        self.release_node(id.slot);
        self.count -= 1;
        Ok(listener)
    }

    /// 00406750/004768D0's binding cleanup. This neither destroys the owner
    /// nor runs its other destructor policies (including selector 18 masks).
    pub fn unregister_owner(&mut self, owner: ObjectId) -> usize {
        let ids: Vec<_> = self
            .registrations()
            .filter(|listener| listener.owner == owner)
            .map(|listener| listener.id)
            .collect();
        for id in &ids {
            self.unregister(*id).expect("collected live binding");
        }
        ids.len()
    }

    /// 00460050's global binding cleanup. Active cursors stay linked until
    /// their walks end, and the callback objects remain owned by the arena.
    pub fn clear(&mut self) {
        let ids: Vec<_> = self.registrations().map(|listener| listener.id).collect();
        for id in ids {
            self.unregister(id).expect("collected live binding");
        }
    }

    /// Walk with the native null-payload cursor: move it immediately after a
    /// binding before invoking that interface. Removal of current/future nodes,
    /// appends, and nested walks therefore change subsequent traversal exactly
    /// where the native links change; this is deliberately not a snapshot.
    ///
    /// A host callback error removes this cursor but retains earlier callback
    /// effects. That error cleanup is a portable boundary, not native rollback.
    pub fn notify(
        &mut self,
        actor: ObjectId,
        arena: &mut ObjectArena,
        effects: &mut dyn CreationListenerEffects,
    ) -> Result<(), CreationListenerError> {
        let cursor = self.allocate_node();
        self.insert_before(cursor, self.node(HEAD).next);
        let result = self.walk(cursor, actor, arena, effects);
        self.release_node(cursor);
        result
    }

    fn walk(
        &mut self,
        cursor: usize,
        actor: ObjectId,
        arena: &mut ObjectArena,
        effects: &mut dyn CreationListenerEffects,
    ) -> Result<(), CreationListenerError> {
        loop {
            let mut next = self.node(cursor).next;
            let listener = loop {
                if next == TAIL {
                    return Ok(());
                }
                let node = self.node(next);
                if let Some(listener) = node.listener {
                    break listener;
                }
                next = node.next;
            };
            self.unlink(cursor);
            self.insert_before(cursor, self.node(next).next);
            // A retained construction/disposal prefix keeps its binding for
            // inspection, but is not an admitted callback receiver. External
            // callback owners remain under the supplied effect's custody.
            if listener.owner.arena == arena.identity
                && (arena.construction_is_stopped(listener.owner)?
                    || arena.disposal_is_stopped(listener.owner)?)
            {
                continue;
            }
            effects.invoke(listener, actor, self, arena)?;
        }
    }

    fn node(&self, slot: usize) -> &Node {
        self.slots[slot].node.as_ref().expect("linked live node")
    }

    fn node_mut(&mut self, slot: usize) -> &mut Node {
        self.slots[slot].node.as_mut().expect("linked live node")
    }

    fn allocate_node(&mut self) -> usize {
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot].generation = self.slots[slot]
                .generation
                .checked_add(1)
                .expect("creation-listener generation exhausted");
            slot
        } else {
            self.slots.push(Slot {
                generation: 1,
                node: None,
            });
            self.slots.len() - 1
        };
        self.slots[slot].node = Some(Node {
            previous: slot,
            next: slot,
            listener: None,
        });
        slot
    }

    fn insert_before(&mut self, slot: usize, next: usize) {
        let previous = self.node(next).previous;
        self.node_mut(slot).previous = previous;
        self.node_mut(slot).next = next;
        self.node_mut(previous).next = slot;
        self.node_mut(next).previous = slot;
    }

    fn unlink(&mut self, slot: usize) {
        let previous = self.node(slot).previous;
        let next = self.node(slot).next;
        self.node_mut(previous).next = next;
        self.node_mut(next).previous = previous;
    }

    fn release_node(&mut self, slot: usize) {
        self.unlink(slot);
        self.slots[slot].node = None;
        self.free.push(slot);
    }
}

#[cfg(test)]
#[path = "creation_listeners_tests.rs"]
mod tests;
