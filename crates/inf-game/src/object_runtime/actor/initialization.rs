//! Shared payload 0047AC10 and virtual-context wrapper 0047AD70.

use std::sync::Arc;

use inf_formats::{native_basis::NativeBasisMath, ActorConstructorPayload};

use super::super::{ObjectArena, ObjectError, ObjectId, RotatingVisualContext};
use crate::ResidentRuntime;

/// The native pointer at +A4. The audited actor paths only read this table;
/// retaining shared decoded data preserves allocation identity across slot
/// replacement without granting mutation of the original H13 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorDamageTable {
    ConstructorDefault,
    Payload(Arc<ActorConstructorPayload>),
}

impl ActorDamageTable {
    pub fn responses(&self) -> [(u16, u16); 10] {
        match self {
            Self::ConstructorDefault => {
                let mut pairs = [(2000, 256); 10];
                pairs[0] = (0, 0);
                pairs
            }
            Self::Payload(payload) => std::array::from_fn(|index| {
                let response = payload.damage_responses[index];
                (response.threshold.value, response.multiplier_q8.value)
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ActorInitializationRequest<'a> {
    pub context: RotatingVisualContext,
    /// One prepared canonical payload may be shared by multiple actors.
    pub payload: &'a Arc<ActorConstructorPayload>,
    /// Process byte 0055F2F4 at initialization, not necessarily the byte
    /// that selected the actor's registry owner during construction.
    pub local_owner: u8,
}

impl ObjectArena {
    /// 0047AD70 invokes the exact shared actor's +08 (0047A610), then reads
    /// the separately supplied payload. A later payload flag write can thus
    /// restore flag 4 after visual context cleared it for a disabled model.
    pub fn initialize_actor(
        &mut self,
        object: ObjectId,
        request: ActorInitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        self.actor(object)?;
        self.initialize_rotating_visual_context(object, request.context, runtime, math)?;
        self.initialize_actor_payload(object, request.payload, request.local_owner)
    }

    /// 0047AC10. The retained response table comes from the same shared
    /// payload as the scalar fields; no current H13 slot is retained instead.
    pub fn initialize_actor_payload(
        &mut self,
        object: ObjectId,
        payload: &Arc<ActorConstructorPayload>,
        local_owner: u8,
    ) -> Result<(), ObjectError> {
        let remote_owner = self.actor(object)?.handle.owner() != local_owner;
        self.apply_payload_flags(object, payload.flags.value)?;
        let actor = self.actor_mut(object)?;
        actor.health = payload.initial_health.value;
        actor.maximum_health = Some(payload.initial_health.value);
        actor.damage_table = ActorDamageTable::Payload(Arc::clone(payload));
        actor.word_8c = payload.word_08.value;
        actor.word_b4 = map_actor_selector(payload.selector_0c.value);
        actor.word_7a = if payload.word_38.value < 175 {
            payload.word_38.value
        } else {
            0
        };
        actor.word_76 = Some(payload.word_40.value);
        let base = &mut self.owned_mut(object)?.state.base;
        // This OR never changes the dirty mask's low nine bits, and a local
        // owner does not clear an earlier 80000000 bit.
        if remote_owner {
            base.flags |= 0x8000_0000;
        }
        // Test the full DWORD before truncating. 00010000 overrides to zero.
        if payload.word_3c.value != 0 {
            base.word_28 = payload.word_3c.value as u16;
        }
        Ok(())
    }

    /// Exact shared +1C = 0047AAD0. Completion's later dirty clear and
    /// initialized flag are owned by the enclosing factory, not this method.
    pub fn finish_actor_initialization(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let extent = self.object(object)?.base.word_28;
        self.actor_mut(object)?.word_74 = extent >> 3;
        Ok(())
    }
}

/// Shared by payload +0C and command 6. The default path's subtraction wraps
/// as a DWORD and its upper-bound comparison is signed, including negatives.
pub(super) fn map_actor_selector(value: i32) -> u32 {
    match value {
        0 => 0,
        1 => 1,
        2 => 18,
        3 => 2,
        value => {
            let mapped = value.wrapping_sub(2);
            if mapped < 19 {
                mapped as u32
            } else {
                2
            }
        }
    }
}
