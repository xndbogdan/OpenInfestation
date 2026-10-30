//! Selector 12, 00467070/00468330/004672C0/00468040, vtable 00527058.
//! The 00429E20 prefix owns the terrain probe. This class embeds a grant
//! owner, whose null notification owner makes its grant operations bit-only.

mod destruction;
mod dying;
mod grant_difference;

pub use dying::{Selector12DyingError, Selector12DyingHost, Selector12DyingRequest};
pub use grant_difference::{Selector12GrantDifferenceError, Selector12GrantDifferenceRequest};

use std::sync::Arc;

use inf_formats::{
    grant_into, grant_is_owned, merge_grants, native_basis::NativeBasisMath, GrantTable,
    InstanceCommandTarget, InstanceCommandView, GRANT_MASK_WORDS,
};

use super::super::allocation::ActorAllocationDwordCarry;
use super::{
    DerivedActorConstructionRequest, DerivedActorError, Family429e20Class, FamilyHelper482ff0,
    RetainedActorAllocation, TerrainProbeAllocation, TerrainProbeCache,
};
use crate::{
    object_runtime::{
        ActorCommandContext, ActorCommandError, ActorConstructionError, ActorInitializationRequest,
        NativeHeapRef, ObjectArena, ObjectId, RuntimeAllocation,
    },
    terrain_runtime::TerrainRuntime,
    ResidentRuntime,
};

/// The 00475020(0) embedded object at actor +24C. Its +1C stays null in
/// these supported methods, so 004750C0 never enters notification callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector12GrantOwner {
    /// Static executable identity, not a host pointer.
    pub vtable_00: u32,
    pub mask_04: [u32; GRANT_MASK_WORDS],
    pub notification_owner_1c: u32,
}

/// 004671E0 releases the four class targets in reverse field order before
/// the probe/common/behavior chain, after the active descriptor's exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector12State {
    pub retained_23c: Option<RetainedActorAllocation>,
    pub word_240: Option<u32>,
    pub word_244: u32,
    pub word_248: Option<u32>,
    pub grants_24c: Selector12GrantOwner,
    pub targets_26c_284: [Option<NativeHeapRef>; 4],

    pub word_270: u32,
    pub word_278: u32,
    pub word_280: u32,
    pub word_288: u32,
    /// Static executable identities, not dereferenceable host pointers.
    pub table_28c: u32,
    pub table_290: u32,
    pub table_294: u32,
    pub table_298: u32,
    pub byte_29c: u8,
    pub byte_29d: u8,
    pub word_29e: Option<u16>,
    /// Port state, not retail's: the line of Pelagia's story this colonist
    /// tells as it boards ([`crate::pelagia::colonist_line`]), once.
    pub pelagia_story: Option<u8>,
}

/// The first player is queried separately by construction and postinit. An
/// absent list entry differs from a present player with an empty grant mask.
#[derive(Clone, Copy, Debug)]
pub struct Selector12GrantContext<'a> {
    pub table: &'a GrantTable,
    pub first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
}

#[derive(Clone, Copy, Debug)]
pub struct Selector12InitializationRequest<'a> {
    pub actor: ActorInitializationRequest<'a>,
    pub allocation: &'a Arc<RuntimeAllocation>,
    pub payload_offset: u32,
}

impl ObjectArena {
    /// Allocation at 00467070: validate the grant closure before publication/RNG, like the
    /// other portable construction preflights. Valid native inputs then follow
    /// the original writes and acquisition order without an error rollback.
    pub fn allocate_selector12_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
        grants: Selector12GrantContext<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let mut merged_mask = [0; GRANT_MASK_WORDS];
        if let Some(source) = grants.first_player_grants {
            merge_grants(&mut merged_mask, grants.table, source)?;
        }
        let object = self.allocate_family429e20_actor(request, rng)?;
        self.family429e20_actor_mut(object)?.class =
            Family429e20Class::Selector12(Box::new(Selector12State {
                retained_23c: None,
                word_240: None,
                word_244: 0,
                word_248: None,
                grants_24c: Selector12GrantOwner {
                    vtable_00: 0x0052_7818,
                    mask_04: [0; GRANT_MASK_WORDS],
                    notification_owner_1c: 0,
                },
                targets_26c_284: [None; 4],

                word_270: 0,
                word_278: 0,
                word_280: 0,
                word_288: 0,
                table_28c: 0x0052_fe40,
                table_290: 0x0053_a080,
                table_294: 0x0053_a090,
                table_298: 0x0052_fe58,
                byte_29c: 0,
                byte_29d: 0,
                word_29e: None,
                pelagia_story: None,
            }));
        let base = &mut self.owned_mut(object)?.state.base;
        let before = base.flags;
        base.flags |= 0x40;
        if (before ^ base.flags) & 0x1ff != 0 {
            base.dirty_word |= 2;
        }
        let before = base.flags;
        base.flags &= !0x100;
        if (before ^ base.flags) & 0x1ff != 0 {
            base.dirty_word |= 2;
        }
        let Family429e20Class::Selector12(state) = &mut self.family429e20_actor_mut(object)?.class
        else {
            unreachable!("class just installed")
        };
        state.grants_24c.mask_04 = merged_mask;
        self.family428650_actor_mut(object)?.word_22e |= 0x0c;
        self.family429e20_actor_mut(object)?.probe_234 = Some(Box::new(
            TerrainProbeAllocation::new(TerrainProbeCache::new(8)),
        ));
        self.family428650_actor_mut(object)?.words_188_19c[0] = Some(0);
        Ok(object)
    }

    pub fn selector12_actor(
        &self,
        object: ObjectId,
    ) -> Result<&Selector12State, DerivedActorError> {
        match &self.family429e20_actor(object)?.class {
            Family429e20Class::Selector12(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector12(object)),
        }
    }

    pub(in crate::object_runtime) fn selector12_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector12State, DerivedActorError> {
        match &mut self.family429e20_actor_mut(object)?.class {
            Family429e20Class::Selector12(state) => Ok(state),
            _ => Err(DerivedActorError::NotSelector12(object)),
        }
    }

    /// The Pelagia reporter's bulb ([`crate::pelagia`]): it has something to
    /// tell the player, as a scientist carrying an unowned grant does. Only
    /// +29D is set; its grants stay retail's, and the next grant broadcast
    /// recomputes it natively.
    pub(crate) fn mark_pelagia_reporter(
        &mut self,
        scientist: ObjectId,
    ) -> Result<(), crate::object_runtime::ObjectError> {
        self.selector12_actor_mut(scientist)
            .map_err(|_| crate::object_runtime::ObjectError::NotSelector12(scientist))?
            .byte_29d = 1;
        Ok(())
    }

    /// One of Pelagia's colonists ([`crate::pelagia`]): it tells line
    /// `line` of the colony's story as it boards the player's craft.
    pub(crate) fn mark_pelagia_colonist(
        &mut self,
        scientist: ObjectId,
        line: u8,
    ) -> Result<(), crate::object_runtime::ObjectError> {
        self.selector12_actor_mut(scientist)
            .map_err(|_| crate::object_runtime::ObjectError::NotSelector12(scientist))?
            .pelagia_story = Some(line);
        Ok(())
    }

    /// The story line a boarding colonist tells, once: None for every other
    /// scientist and for one that has told its line.
    pub(crate) fn take_pelagia_story(&mut self, scientist: ObjectId) -> Option<u8> {
        self.selector12_actor_mut(scientist)
            .ok()
            .and_then(|state| state.pelagia_story.take())
    }

    /// 00468310 invokes virtual +08 before 00468330 delegates to 00426C10
    /// and retains payload +58. No specialized scalar is read at this point.
    pub fn initialize_selector12_actor(
        &mut self,
        object: ObjectId,
        request: Selector12InitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.selector12_actor(object)?;
        let payload = request
            .actor
            .payload
            .family467070
            .as_ref()
            .ok_or(DerivedActorError::MissingSelector12Payload)?;
        if request.payload_offset.checked_add(0x58) != Some(payload.retained_58_offset) {
            return Err(DerivedActorError::MissingSelector12Payload);
        }
        self.initialize_derived_actor(object, request.actor, runtime, math)?;
        self.set_selector12_retained(object, request.allocation, payload.retained_58_offset)
    }

    /// Code 23 replaces the retained address without reading its contents.
    pub fn set_selector12_retained(
        &mut self,
        object: ObjectId,
        allocation: &Arc<RuntimeAllocation>,
        allocation_offset: u32,
    ) -> Result<(), DerivedActorError> {
        self.selector12_actor(object)?;
        let retained = RetainedActorAllocation::new(allocation, allocation_offset)?;
        self.selector12_actor_mut(object)?.retained_23c = Some(retained);
        Ok(())
    }

    pub fn apply_selector12_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(12) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector12_actor(object)?;
        match command.code.value {
            ..=22 => self.apply_family428650_command(object, command, context),
            // The enclosing command walk supplies the retained allocation.
            23 => Err(ActorCommandError::Payload(23)),
            _ => Ok(false),
        }
    }

    /// 00468040 clamps in native read/write order, snaps Y, grants +7A, queries
    /// the first player's mask, then calls shared postinit and acquires +218.
    /// It does not reselect +140 behavior or refresh the admission home position.
    pub fn finish_selector12_initialization(
        &mut self,
        object: ObjectId,
        terrain: Option<&TerrainRuntime>,
        grants: Selector12GrantContext<'_>,
    ) -> Result<(), DerivedActorError> {
        let retained = self
            .selector12_actor(object)?
            .retained_23c
            .clone()
            .ok_or(DerivedActorError::MissingSelector12Payload)?;
        // +0E is deliberately untouched. Each access uses the shared mutable
        // allocation, retaining aliases and earlier writes on a later failure.
        for relative in [0x10, 0, 2, 4, 6, 8, 0x0a, 0x0c] {
            let at = retained
                .allocation_offset
                .checked_add(relative)
                .ok_or(DerivedActorError::MissingSelector12Payload)?;
            if retained.allocation().word(at)? > 1000 {
                retained.allocation().write_word(at, 1000)?;
            }
        }
        let terrain = terrain.ok_or(DerivedActorError::MissingSelector12Terrain)?;
        let [x, _, z] = self
            .object(object)?
            .base
            .position_words
            .ok_or(DerivedActorError::MissingPosition(object))?;
        // 00479DD0 directly stores AX. No radius, terrain flag guard, position
        // setter or dirty-word update participates in this operation.
        let y = terrain.height_at([x, z]) as u16;
        self.owned_mut(object)?.state.base.position_words = Some([x, y, z]);
        let id = u32::from(self.actor(object)?.word_7a);
        grant_into(
            &mut self.selector12_actor_mut(object)?.grants_24c.mask_04,
            grants.table,
            id,
        )?;
        // 00467720 performs a new first-player query, independent of the
        // embedded mask just granted. An absent player preserves +29D.
        if let Some(source) = grants.first_player_grants {
            self.selector12_actor_mut(object)?.byte_29d = u8::from(!grant_is_owned(source, id)?);
        }
        self.finish_actor_initialization(object)?;
        if self.family428650_actor(object)?.helper_218.is_none()
            && self.derived_actor(object)?.behavior_140.is_some()
        {
            // This vtable's +7C is 004027A0: return actor +140.
            let initial_handle = self
                .actor_allocation
                .initial_dword(ActorAllocationDwordCarry::FamilyHelper00);
            self.family428650_actor_mut(object)?.helper_218 =
                Some(Box::new(FamilyHelper482ff0::new(initial_handle)));
        }
        Ok(())
    }
}
