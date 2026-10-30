//! Packed actor identities and slot ownership, 00461C80..004625D0.
//!
//! This registry does not own or reference-count objects. Its ObjectId payloads
//! replace native pointers; callers still validate their lifetime in the arena.
//! Actor construction, network cursor processing and factory admission remain
//! separate. Fatal native arguments and an unfindable free slot become errors.

use thiserror::Error;

use super::ObjectId;

const MAX_OWNERS: usize = 32;
const MAX_SLOTS: usize = 1024;
const MARK: u32 = 1 << 26;

/// Native DWORD: generation 0..15, slot 16..25, mode-1 mark 26, owner 27..31.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActorHandle(u32);

impl ActorHandle {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn owner(self) -> u8 {
        (self.0 >> 27) as u8
    }

    pub const fn slot(self) -> u16 {
        ((self.0 >> 16) & 0x3ff) as u16
    }

    pub const fn generation(self) -> u16 {
        self.0 as u16
    }

    pub const fn marked(self) -> bool {
        self.0 & MARK != 0
    }
}

/// Logical native 12-byte entry. `next_free` remains stale while occupied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorHandleSlot {
    pub handle: ActorHandle,
    pub object: Option<ObjectId>,
    pub next_free: i32,
}

/// Logical owner header; the two leading words are reset to 0/1 on growth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorHandleOwner {
    pub word_00: i32,
    pub word_04: i32,
    pub free_head: i32,
    pub free_tail: i32,
    slots: Vec<ActorHandleSlot>,
}

impl ActorHandleOwner {
    pub fn slots(&self) -> &[ActorHandleSlot] {
        &self.slots
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// 00461DE0: generation and pointer zero in every slot, the owner and
    /// index in its handle, next-free one below it; the free head is the
    /// last slot and the tail 1, so fresh allocations descend.
    fn new(owner: u8, capacity: usize) -> Self {
        Self {
            word_00: 0,
            word_04: 1,
            free_head: capacity as i32 - 1,
            free_tail: 1,
            slots: (0..capacity)
                .map(|index| ActorHandleSlot {
                    handle: ActorHandle(((owner as u32) << 27) | ((index as u32) << 16)),
                    object: None,
                    next_free: index as i32 - 1,
                })
                .collect(),
        }
    }

    fn reserved() -> Self {
        Self {
            word_00: 0,
            word_04: 0,
            free_head: 0,
            free_tail: 0,
            slots: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorHandleCache {
    pub handle: ActorHandle,
    pub object: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ActorHandleError {
    #[error("actor owner count {0} is outside the supported packed domain 2..=32")]
    OwnerCount(usize),
    #[error("actor initial capacity {0} is outside the supported packed domain 8..=1024")]
    InitialCapacity(usize),
    #[error("actor owner {0} is not an initialized nonzero owner")]
    Owner(u8),
    #[error("actor handle {0:?} uses the reserved slot zero")]
    ReservedSlot(ActorHandle),
    #[error("actor owner {0} has no free slot after growth")]
    Exhausted(u8),
    #[error("actor handle {0:?} already resolves to an object")]
    AlreadyBound(ActorHandle),
    #[error("actor slot {slot} cannot be found in owner {owner}'s free chain")]
    FreeChain { owner: u8, slot: u16 },
}

/// Standalone registry state. Owner zero and each owner's slot zero are reserved.
#[derive(Clone, Debug)]
pub struct ActorHandles {
    owners: Vec<ActorHandleOwner>,
    initial_capacity: usize,
    null_handle: ActorHandle,
    cache: ActorHandleCache,
}

impl ActorHandles {
    /// `null_handle` is the explicit value of native global 0054E894.
    /// The upper bounds contain the native packed representation; native only
    /// diagnoses the lower bounds and cannot safely represent larger tables.
    pub fn new(
        owner_count: usize,
        initial_capacity: usize,
        null_handle: ActorHandle,
    ) -> Result<Self, ActorHandleError> {
        let mut registry = Self {
            owners: Vec::new(),
            initial_capacity: 0,
            null_handle,
            cache: ActorHandleCache {
                handle: null_handle,
                object: None,
            },
        };
        registry.configure(owner_count, initial_capacity)?;
        Ok(registry)
    }

    pub fn owners(&self) -> &[ActorHandleOwner] {
        &self.owners
    }

    /// Like 00462460 this exposes the reserved owner-zero header as well.
    pub fn owner(&self, owner: u8) -> Option<&ActorHandleOwner> {
        self.owners.get(owner as usize)
    }

    pub fn initial_capacity(&self) -> usize {
        self.initial_capacity
    }

    pub fn null_handle(&self) -> ActorHandle {
        self.null_handle
    }

    pub fn cache(&self) -> ActorHandleCache {
        self.cache
    }

    /// 004622F0 compares the configured size, not any owner's grown capacity.
    pub fn configure(
        &mut self,
        owner_count: usize,
        initial_capacity: usize,
    ) -> Result<(), ActorHandleError> {
        if !(2..=MAX_OWNERS).contains(&owner_count) {
            return Err(ActorHandleError::OwnerCount(owner_count));
        }
        if !(8..=MAX_SLOTS).contains(&initial_capacity) {
            return Err(ActorHandleError::InitialCapacity(initial_capacity));
        }
        if self.owners.len() == owner_count && self.initial_capacity == initial_capacity {
            return Ok(());
        }
        self.clear();
        self.initial_capacity = initial_capacity;
        self.owners.push(ActorHandleOwner::reserved());
        self.owners.extend(
            (1..owner_count).map(|owner| ActorHandleOwner::new(owner as u8, initial_capacity)),
        );
        Ok(())
    }

    /// 004623F0 removes registrations without destroying their objects. The
    /// configured initial capacity survives teardown, as it does in retail.
    pub fn clear(&mut self) {
        self.owners.clear();
        self.invalidate_cache();
    }

    /// 00461EA0 pops the free head, increments generation (skipping zero), and
    /// preserves the mark bit. It leaves both tail and lookup cache unchanged.
    pub fn allocate(
        &mut self,
        owner: u8,
        object: ObjectId,
    ) -> Result<ActorHandle, ActorHandleError> {
        let index = self.checked_owner(owner)?;
        if !self.has_free_head(index) {
            self.grow(owner, self.owners[index].capacity() + 1)?;
        }
        if !self.has_free_head(index) {
            return Err(ActorHandleError::Exhausted(owner));
        }
        let table = &mut self.owners[index];
        let slot_index = table.free_head as usize;
        let slot = &mut table.slots[slot_index];
        table.free_head = slot.next_free;
        slot.object = Some(object);
        let generation = slot.handle.generation().wrapping_add(1).max(1);
        slot.handle = ActorHandle(
            ((owner as u32) << 27)
                | ((slot_index as u32) << 16)
                | (slot.handle.raw() & MARK)
                | u32::from(generation),
        );
        Ok(slot.handle)
    }

    /// 00461FE0 binds a supplied identity; it is not a reference-count retain.
    /// A slot absent from the free chain reaches an unbounded native walk.
    /// The portable error preserves preceding growth/cache effects. Retail's
    /// stale-tail self-link can put an occupied slot on that chain; binding
    /// still overwrites it when a different full identity misses lookup.
    pub fn bind(&mut self, handle: ActorHandle, object: ObjectId) -> Result<(), ActorHandleError> {
        let owner = handle.owner();
        let index = self.checked_owner(owner)?;
        let slot_index = usize::from(handle.slot());
        if slot_index >= self.owners[index].capacity() {
            self.grow(owner, slot_index + 1)?;
        }
        if slot_index == 0 {
            return Err(ActorHandleError::ReservedSlot(handle));
        }
        if self.resolve(handle).is_some() {
            return Err(ActorHandleError::AlreadyBound(handle));
        }
        let table = &mut self.owners[index];
        let predecessor = if table.free_head == slot_index as i32 {
            table.free_head = table.slots[slot_index].next_free;
            0
        } else {
            let mut cursor = table.free_head;
            let mut predecessor = None;
            for _ in 0..table.capacity() {
                if cursor <= 0 || cursor as usize >= table.capacity() {
                    break;
                }
                let next = table.slots[cursor as usize].next_free;
                if next == slot_index as i32 {
                    predecessor = Some(cursor);
                    break;
                }
                cursor = next;
            }
            let predecessor = predecessor.ok_or(ActorHandleError::FreeChain {
                owner,
                slot: handle.slot(),
            })?;
            table.slots[predecessor as usize].next_free = table.slots[slot_index].next_free;
            predecessor
        };
        if table.free_tail == slot_index as i32 {
            table.free_tail = predecessor;
        }
        table.slots[slot_index].object = Some(object);
        table.slots[slot_index].handle = handle;
        self.cache = ActorHandleCache {
            handle,
            object: Some(object),
        };
        Ok(())
    }

    /// 00462190 checks the cache before validating the full packed identity.
    /// An exact empty-slot match also becomes a cached null result.
    pub fn resolve(&mut self, handle: ActorHandle) -> Option<ObjectId> {
        if self.cache.handle == handle {
            return self.cache.object;
        }
        let (owner, slot) = self.valid_slot(handle)?;
        let entry = self.owners[owner].slots[slot];
        if entry.handle != handle {
            return None;
        }
        self.cache = ActorHandleCache {
            handle,
            object: entry.object,
        };
        entry.object
    }

    /// [`Self::resolve`] without its lookup cache, for a reader that leaves
    /// the game's state alone (the tracer's trails).
    pub fn peek(&self, handle: ActorHandle) -> Option<ObjectId> {
        let (owner, slot) = self.valid_slot(handle)?;
        let entry = self.owners[owner].slots[slot];
        (entry.handle == handle).then_some(entry.object).flatten()
    }

    /// 00462210 unregisters a matching object and appends the slot to the free
    /// chain, preserving the packed identity and the native stale-tail writes.
    pub fn release(&mut self, handle: ActorHandle) -> bool {
        if self.cache.handle == handle {
            self.invalidate_cache();
        }
        let Some((owner, slot)) = self.valid_slot(handle) else {
            return false;
        };
        let table = &mut self.owners[owner];
        if table.slots[slot].handle != handle || table.slots[slot].object.is_none() {
            return false;
        }
        table.slots[slot].object = None;
        table.slots[slot].next_free = 0;
        if table.free_head >= 0 {
            if table.free_tail != 0 {
                table.slots[table.free_tail as usize].next_free = slot as i32;
            }
            table.free_tail = slot as i32;
            if table.free_head == 0 {
                table.free_head = slot as i32;
            }
        }
        true
    }

    /// 00462510: the object in the handle's slot, whatever its generation.
    pub fn slot_object(&self, handle: ActorHandle) -> Option<ObjectId> {
        let owner = self.checked_owner(handle.owner()).ok()?;
        self.owners[owner]
            .slots
            .get(usize::from(handle.slot()))?
            .object
    }

    /// 00462560: an empty slot still holding the handle's generation forgets
    /// it (its stored WORD becomes 0). The lookup cache stays as it is.
    pub fn forget_slot(&mut self, handle: ActorHandle) -> bool {
        let Ok(owner) = self.checked_owner(handle.owner()) else {
            return false;
        };
        let Some(entry) = self.owners[owner].slots.get_mut(usize::from(handle.slot())) else {
            return false;
        };
        if entry.object.is_some() || entry.handle.generation() != handle.generation() {
            return false;
        }
        entry.handle = ActorHandle(entry.handle.0 & !0xffff);
        true
    }

    /// 00462480 deliberately permits slot zero and compares only generation.
    /// This predicate alone does not establish that subsequent binding succeeds.
    pub fn mode2_admissible(&mut self, handle: ActorHandle) -> Result<bool, ActorHandleError> {
        let Ok(owner) = self.checked_owner(handle.owner()) else {
            return Ok(false);
        };
        let slot = usize::from(handle.slot());
        if slot >= self.owners[owner].capacity() {
            self.grow(handle.owner(), slot + 1)?;
        }
        let entry = self.owners[owner].slots[slot];
        Ok(entry.object.is_none() && entry.handle.generation() != handle.generation())
    }

    /// 004625D0 updates the caller's packed word and matching table entry even
    /// when that entry is empty, then invalidates the single lookup cache.
    pub fn mark_mode1(&mut self, handle: &mut ActorHandle) -> bool {
        let Some((owner, slot)) = self.valid_slot(*handle) else {
            return false;
        };
        if self.owners[owner].slots[slot].handle != *handle {
            return false;
        }
        handle.0 |= MARK;
        self.owners[owner].slots[slot].handle = *handle;
        self.invalidate_cache();
        true
    }

    /// 0047E2B0 releases the old registration, publishes the new actor word,
    /// then binds. A bind error intentionally does not roll those effects back.
    pub fn reassign(
        &mut self,
        object: ObjectId,
        current: &mut ActorHandle,
        new: ActorHandle,
    ) -> Result<(), ActorHandleError> {
        self.release(*current);
        *current = new;
        self.bind(new, object)
    }

    fn invalidate_cache(&mut self) {
        self.cache = ActorHandleCache {
            handle: self.null_handle,
            object: None,
        };
    }

    fn checked_owner(&self, owner: u8) -> Result<usize, ActorHandleError> {
        let index = usize::from(owner);
        if index == 0 || index >= self.owners.len() {
            Err(ActorHandleError::Owner(owner))
        } else {
            Ok(index)
        }
    }

    fn valid_slot(&self, handle: ActorHandle) -> Option<(usize, usize)> {
        let owner = self.checked_owner(handle.owner()).ok()?;
        let slot = usize::from(handle.slot());
        (slot > 0 && slot < self.owners[owner].capacity()).then_some((owner, slot))
    }

    fn has_free_head(&self, owner: usize) -> bool {
        let table = &self.owners[owner];
        table.free_head > 0 && (table.free_head as usize) < table.capacity()
    }

    /// 00461CE0 rebuilds descending free order, then rebinds occupied old
    /// entries in increasing index order. Empty entries copy only the handle.
    fn grow(&mut self, owner: u8, requested: usize) -> Result<(), ActorHandleError> {
        let index = self.checked_owner(owner)?;
        let old_capacity = self.owners[index].capacity();
        let mut capacity = old_capacity;
        while capacity < requested {
            capacity *= 2;
        }
        capacity = capacity.min(MAX_SLOTS);
        if capacity <= old_capacity {
            return Ok(());
        }
        let old = std::mem::replace(
            &mut self.owners[index],
            ActorHandleOwner::new(owner, capacity),
        );
        self.invalidate_cache();
        for (slot, entry) in old.slots.into_iter().enumerate() {
            if let Some(object) = entry.object {
                self.bind(entry.handle, object)?;
            } else {
                self.owners[index].slots[slot].handle = entry.handle;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "actor_handles_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "actor_handles_native_tests.rs"]
mod native_tests;

#[cfg(test)]
#[path = "actor/capture/native_receipts.rs"]
mod capture_native_receipts;
