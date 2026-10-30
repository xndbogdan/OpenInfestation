//! Per-placement attract suffix 0048DF5B..0048DFB8.
//!
//! The enclosing loader owns factory/network/registration order. This phase
//! reads the actual current BYTE only when that prefix has completed. Canonical
//! final vtables return Self/NULL for +18 and +58; arbitrary overwritten vtables
//! are outside the typed ObjectClass/ActorExtension domain.

use std::cell::Cell;

use super::{ActorExtension, ObjectArena, ObjectClass, ObjectError, ObjectId};

/// Actual current 0055F14C and the successful placement allocation.
pub struct WorldLoadAttractRequest<'a> {
    pub object: ObjectId,
    pub current_attract: &'a Cell<u8>,
}

impl ObjectArena {
    /// Canonical object virtual +18: 00404610 returns the actor itself;
    /// 00401300 returns NULL for every represented nonactor final class.
    /// Unknown/deleted/foreign allocations are errors, not the NULL policy.
    pub fn object_actor_component(
        &self,
        object: ObjectId,
    ) -> Result<Option<ObjectId>, ObjectError> {
        Ok(match &self.object(object)?.class {
            ObjectClass::Actor(_) => Some(object),
            ObjectClass::Base
            | ObjectClass::Selector81(_)
            | ObjectClass::Selector94(_)
            | ObjectClass::Selector75(_)
            | ObjectClass::Selector86(_)
            | ObjectClass::Selector67(_)
            | ObjectClass::MapAnnotation(_)
            | ObjectClass::Visual(_)
            | ObjectClass::Selector22(_)
            | ObjectClass::Selector65(_)
            | ObjectClass::Selector50(_)
            | ObjectClass::Selector48(_)
            | ObjectClass::Selector43(_)
            | ObjectClass::Selector42(_)
            | ObjectClass::Selector64(_)
            | ObjectClass::TimedPickup(_)
            | ObjectClass::RotatingVisual(_)
            | ObjectClass::Selector45(_)
            | ObjectClass::Selector46(_)
            | ObjectClass::Race(_)
            | ObjectClass::Selector58(_)
            | ObjectClass::Selector83(_)
            | ObjectClass::Selector76(_)
            | ObjectClass::TerrainLight { .. }
            | ObjectClass::RotatingTerrainLight { .. }
            | ObjectClass::RouteNode { .. } => None,
        })
    }

    /// Fresh BYTE -> +18 -> +58 -> signed DWORD144 -> flags -> dirty.
    /// No class-specific lifecycle, behavior, position or registry read belongs
    /// to these pure canonical queries. Native's mask leaves low9 unchanged;
    /// the literal dirty predicate is retained rather than adding dirty2.
    pub fn apply_world_load_attract(
        &mut self,
        request: WorldLoadAttractRequest<'_>,
    ) -> Result<(), ObjectError> {
        if request.current_attract.get() == 0 {
            return Ok(());
        }
        let Some(actor) = self.object_actor_component(request.object)? else {
            return Ok(());
        };
        let Some(component) = self.actor_derived_component(actor)? else {
            return Ok(());
        };
        let ActorExtension::Derived(derived) = &mut self.actor_mut(component)?.extension else {
            return Err(ObjectError::NotDerivedActor(component));
        };
        if derived.word_144 < 0 {
            derived.word_144 = derived.word_144.wrapping_mul(-1000);
        }
        let base = &mut self.owned_mut(component)?.state.base;
        let old_flags = base.flags;
        base.flags &= 0xFB9F_FFFF;
        if (base.flags ^ old_flags) & 0x1FF != 0 {
            base.dirty_word |= 2;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
