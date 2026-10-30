//! Constructor-guard behavior storage, 00426C80/0048CC70, and the initial
//! selection performed by 00428390, plus shared damage predicate 0048D820.
//! model_custom_vertex owns retained group capture 0048CF70 on this same storage.
//! Later descriptor simulation and audio effects retain separate owners.

mod position;
pub use position::{ActorBehaviorPositionError, ActorBehaviorPositionRequest};

mod tick;
pub use tick::ActorBehaviorTickError;

mod firing;
pub(in crate::object_runtime::actor) use firing::normalize_417da0;
pub use firing::{
    ActorWeaponFireContext, ActorWeaponFireError, ActorWeaponFireMode, ActorWeaponFireRequest,
};
mod emission;
pub use emission::*;

mod damage_cache;
pub(in crate::object_runtime::actor) use damage_cache::storage::{
    read_row_word, read_vehicle_row_dword,
};
mod family_gate;
pub use damage_cache::{
    BehaviorActorResourceRequest, BehaviorCacheStorageError, BehaviorComponentPacketSourceRequest,
    BehaviorComponentQuery, BehaviorCreationRequest, BehaviorPacketCopy, BehaviorPacketSlot,
    BehaviorPacketSourceRequest, BehaviorParticleAdapter, BehaviorParticleAdapterError,
    BehaviorParticleCreationSources, BehaviorParticleEffectRequest, BehaviorParticleServiceContext,
    BehaviorParticleServices, BehaviorPredicateContext, BehaviorPredicateError,
    BehaviorPredicateHost, BehaviorPredicateRequest, BehaviorRowSelection, BehaviorTemporary,
    ProcessActorBehaviorTables,
};
pub use family_gate::{FamilyCandidateGateError, FamilyCandidateGateRequest};

/// Native descriptor identities installed at behavior +14. These refer to
/// executable data, not H13 resources or actor constructor selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ActorBehaviorTable {
    Mode01 = 0x0053_0e60,
    Mode02 = 0x0053_0f10,
    Mode03 = 0x0053_0f68,
    Mode04 = 0x0053_0f98,
    Mode05 = 0x0053_0ff0,
    Mode06 = 0x0053_1048,
    Mode07 = 0x0053_10a0,
    Mode08 = 0x0053_10d0,
    Mode09 = 0x0053_1100,
    Mode10 = 0x0053_1130,
    Mode11 = 0x0053_1160,
    Mode12 = 0x0053_1190,
    Mode13 = 0x0053_0eb8,
    /// Selector 2 points +14 at its own +26C copy of these 00530158 bytes.
    Selector2Copy = 0x0053_0158,
    /// The Pursuit Ship's (selector 61) 0048CC70(2, 1, 0, 0052F748).
    Selector61 = 0x0052_f748,
}

/// One 16-byte record in the allocation retained at behavior +1C.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorBehaviorGroup {
    /// The six WORDs at +00/+02/+04/+06/+08/+0A have independent custody.
    /// 0048CC70 leaves them unwritten; 0048CF70 first captures world +06..0A,
    /// then conditionally writes relative +00..04. No unknown half is zeroed.
    pub coordinates_00_0a: [Option<u16>; 6],
    pub word_0c: u32,
}

impl ActorBehaviorGroup {
    /// Inspect the original three DWORDs without discarding partial WORD
    /// knowledge. A DWORD is known only when both of its halves are known.
    pub fn dwords_00_08(&self) -> [Option<u32>; 3] {
        std::array::from_fn(|index| {
            Some(
                u32::from(self.coordinates_00_0a[index * 2]?)
                    | (u32::from(self.coordinates_00_0a[index * 2 + 1]?) << 16),
            )
        })
    }
}

/// A completed 00426C80 guard's owned +140 object. Linked fire allocations
/// live in the arena's actor-service heap and are explicitly freed by 0048CD60.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorBehaviorState {
    pub fire_head_00: Option<crate::object_runtime::NativeHeapRef>,
    /// Portable fail-closed custody after a partially executed drain. The
    /// reached node identifies the failure; the actor's whole chain stops.
    pub firing_stop_00: Option<crate::object_runtime::NativeHeapRef>,
    pub word_04: u32,
    pub word_08: u32,
    pub byte_0c: u8,
    pub byte_0d: u8,
    pub byte_0e: u8,
    pub byte_0f: u8,
    pub byte_10: u8,
    pub byte_11: u8,
    pub table_14: ActorBehaviorTable,
    /// +18 contains byte_0e + byte_0f records, each exactly 32 bytes. All
    /// eight DWORDs are initialized: [-1, 0, 0, 0, 0, 0, 0, 0].
    pub entries_18: Vec<[u32; 8]>,
    pub groups_1c: Vec<ActorBehaviorGroup>,
    pub word_20: u32,
    /// +24 allocates one DWORD per group without writing its contents.
    pub words_24: Vec<Option<u32>>,
    pub sound_28: Option<crate::process_sound::ProcessSoundRef>,
    pub word_2c: u32,
    pub word_30: Option<u32>,
    pub word_34: Option<u32>,
    pub word_38: u32,
}

impl ActorBehaviorState {
    /// 0048CD60 publishes each successor before its free, then releases +18,
    /// +1C and +24 before reaching nonnull +28 audio. A service error retains
    /// the already reached prefix; callers release the behavior only on return.
    pub(in crate::object_runtime) fn teardown(
        &mut self,
        heap: &mut crate::object_runtime::ActorServiceHeap,
        sound: &mut crate::object_runtime::SoundTeardownService<'_>,
    ) -> Result<(), crate::object_runtime::ObjectError> {
        while let Some(head) = self.fire_head_00 {
            self.fire_head_00 = heap.weapon_fire_node(head)?.next;
            heap.release(head)?;
        }
        self.firing_stop_00 = None;
        self.entries_18 = Vec::new();
        self.groups_1c = Vec::new();
        self.words_24 = Vec::new();
        if let Some(reference) = self.sound_28 {
            sound.delete(reference)?;
        }
        Ok(())
    }
}

impl crate::object_runtime::ObjectArena {
    /// The weapon behavior an actor owns: a derived actor's +140, or the
    /// Pursuit Ship's +138 (its +7C, 00413130).
    pub(in crate::object_runtime) fn actor_behavior_storage(
        &self,
        actor: crate::object_runtime::ObjectId,
    ) -> Result<&Option<ActorBehaviorState>, crate::object_runtime::ObjectError> {
        if let super::ActorExtension::Selector61(ship) = &self.actor(actor)?.extension {
            return Ok(&ship.behavior_138);
        }
        Ok(&self.derived_actor(actor)?.behavior_140)
    }

    pub(in crate::object_runtime) fn actor_behavior_storage_mut(
        &mut self,
        actor: crate::object_runtime::ObjectId,
    ) -> Result<&mut Option<ActorBehaviorState>, crate::object_runtime::ObjectError> {
        if self.selector61(actor).is_ok() {
            return Ok(&mut self.selector61_mut(actor)?.behavior_138);
        }
        Ok(&mut self.derived_actor_mut(actor)?.behavior_140)
    }

    pub fn teardown_actor_behavior(
        &mut self,
        actor: crate::object_runtime::ObjectId,
        sound: &mut crate::object_runtime::SoundTeardownService<'_>,
    ) -> Result<(), crate::object_runtime::ObjectError> {
        if let Some(mut behavior) = self.actor_behavior_storage_mut(actor)?.take() {
            if let Err(error) = behavior.teardown(&mut self.actor_services, sound) {
                *self.actor_behavior_storage_mut(actor)? = Some(behavior);
                return Err(error);
            }
        }
        Ok(())
    }
}

/// Build the replacement selected by 00426C80. None is the retail guard's
/// successful no-change branch; callers must preserve any previous +140.
/// Mode byte bit 7 changes group count independently of the low-seven-bit
/// table selection. The returned object includes the subsequent 00428390.
pub fn build_actor_behavior(
    mode_byte: u8,
    flags: u32,
    selector_b4: u32,
) -> Option<ActorBehaviorState> {
    use ActorBehaviorTable::*;

    let (table_14, entry_count, word_04, word_08) = match mode_byte & 0x7f {
        1 => (Mode01, 2, 8_000, 150_000),
        2 => (Mode02, 2, 3_000, 150_000),
        3 => (Mode03, 1, 4_000, 150_000),
        4 => (Mode04, 2, 2_500, 0),
        5 => (Mode05, 2, 1_500, 0),
        6 => (Mode06, 2, 500, 0),
        7 => (Mode07, 1, 0, 2_000_000),
        8 => (Mode08, 1, 0, 2_000_000),
        9 => (Mode09, 1, 0, 2_000_000),
        // 00426F6D writes this numeric DWORD; the decompiler incorrectly
        // renders it as an address because it falls inside the image.
        10 => (Mode10, 1, 2_560, 5_000_000),
        11 => (Mode11, 1, 8_000, 500_000),
        12 => (Mode12, 1, 8_000, 500_000),
        13 => (Mode13, 2, 4_000, 250_000),
        _ => return None,
    };
    let group_count = 1 + u8::from(mode_byte & 0x80 != 0);
    let mut state = ActorBehaviorState {
        fire_head_00: None,
        firing_stop_00: None,
        word_04,
        word_08,
        byte_0c: 1,
        byte_0d: group_count,
        byte_0e: entry_count,
        byte_0f: 0,
        byte_10: 0,
        byte_11: entry_count,
        table_14,
        entries_18: vec![[u32::MAX, 0, 0, 0, 0, 0, 0, 0]; entry_count as usize],
        groups_1c: vec![
            ActorBehaviorGroup {
                coordinates_00_0a: [None; 6],
                word_0c: 4,
            };
            group_count as usize
        ],
        word_20: 0,
        words_24: vec![None; group_count as usize],
        sound_28: None,
        word_2c: 0,
        word_30: None,
        word_34: None,
        word_38: 0,
    };
    select_actor_behavior(&mut state, flags, selector_b4);
    Some(state)
}

/// 00428390 is also called by derived postinitializers after instance commands
/// may have changed actor +B4. Flag 2000 preserves the prior selection.
pub fn select_actor_behavior(state: &mut ActorBehaviorState, flags: u32, selector_b4: u32) {
    if flags & 0x2000 != 0 {
        return;
    }
    let desired = u8::from(!matches!(selector_b4, 18 | 3));
    if state.byte_10 != desired {
        // The retail consumer sign-extends +0E and uses signed IDIV. All
        // guard profiles initialize it to 1 or 2, so this divisor is nonzero.
        state.byte_10 = ((desired as i32) % (state.byte_0e as i8 as i32)) as u8;
    }
}

#[cfg(test)]
#[path = "behavior_tests.rs"]
mod tests;
