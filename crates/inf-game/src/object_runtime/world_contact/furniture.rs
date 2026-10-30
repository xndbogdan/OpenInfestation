//! 0047BFD0, the shared furniture contact response. Selector34/35 tables
//! hold it at virtual +A8; the 00429E20 family reaches it through guard
//! 0042A760. 0047C1D0 passes the contact, the H11 entry behind the cell's
//! placement lookup and its stack cell position. It always returns AL 0.

use thiserror::Error;

use super::{
    super::{
        native_trunc_low32 as ftol, ActorDamagePacket, ActorDamageRequest, ActorHandle,
        CollisionContact, ObjectArena, ObjectError, ObjectId,
    },
    NetworkImpactGate,
};

/// Services 0047BFD0 reaches after its push and rebound.
pub trait FurnitureResponseHost {
    type Error;
    /// 0047C15B..6E, read only past the 0x32 threshold of other kinds.
    fn network_impact_gate(&self) -> NetworkImpactGate;
    /// Current 0054E894, the source of kind 0E's fixed damage.
    fn process_null_handle(&self) -> Option<ActorHandle>;
    /// 0047C6E0.
    fn actor_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error>;
    /// 00436020 over the cell's low WORDs. Its result is discarded.
    fn furniture_damage(
        &mut self,
        xz: [u16; 2],
        packet: ActorDamagePacket,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureContactResponse {
    pub contact: CollisionContact,
    /// Byte +2 of the H11 entry behind the contact's placement lookup.
    pub kind: u8,
    /// Aligned cell X/Z WORDs of 0047C1D0's stack position.
    pub cell_xz: [u16; 2],
}

#[derive(Debug, Error)]
pub enum FurnitureResponseError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("furniture contact for {actor:?} left DWORD +{offset:02X} unwritten at 0047BFD0")]
    RequiredContact { actor: ObjectId, offset: u8 },
    #[error("0047BFD0 network gate 00463AB0 for {actor:?} is not owned")]
    RequiredNetworkGate { actor: ObjectId },
    #[error("0047BFD0 kind-0E damage for {actor:?} requires process NULL 0054E894")]
    RequiredProcessNull { actor: ObjectId },
    #[error("0047BFD0 damage service failed for {actor:?}: {error}")]
    Host { actor: ObjectId, error: E },
}

/// Kind 0E rebounds at a fixed -1500 and takes fixed damage 3000, channel 7.
const KIND_FIXED: u8 = 0x0e;

impl ObjectArena {
    /// 0047BFD0. Position WORDs and velocity WORDs change without dirty bits;
    /// the velocity is written from its saved copy, not through 0047B510.
    pub fn respond_furniture_contact<H: FurnitureResponseHost>(
        &mut self,
        actor: ObjectId,
        response: FurnitureContactResponse,
        host: &mut H,
    ) -> Result<u8, FurnitureResponseError<H::Error>> {
        let FurnitureContactResponse {
            contact,
            kind,
            cell_xz,
        } = response;
        let missing = |offset| FurnitureResponseError::RequiredContact { actor, offset };
        let mut normal = [0.0; 3];
        for (component, bits) in contact.normal_bits().into_iter().enumerate() {
            let bits = bits.ok_or(missing(4 + 4 * component as u8))?;
            normal[component] = f64::from(f32::from_bits(bits));
        }
        let saved = self.actor(actor)?.words_8e_92;
        let velocity = saved.map(|word| f64::from(word as i16));
        // 0047C011..3F sums Y and Z before X, then 0047C052 decrements.
        let mut along =
            ftol((velocity[1] * normal[1] + velocity[2] * normal[2]) + velocity[0] * normal[0])
                .wrapping_sub(1);
        let displacement = contact.displacement().ok_or(missing(0x10))?;
        let push = f64::from(displacement as i16);
        let offset = normal.map(|n| ftol(push * n) as u16);
        let base = &mut self.owned_mut(actor)?.state.base;
        let mut position = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        for (word, offset) in position.iter_mut().zip(offset) {
            *word = word.wrapping_add(offset);
        }
        base.position_words = Some(position);
        if along < 0 {
            along = if kind == KIND_FIXED {
                -1500
            } else {
                along.wrapping_add(along >> 3)
            };
            let rebound = f64::from(along as i16);
            let impulse = normal.map(|n| ftol(rebound * n) as u16);
            let words = &mut self.actor_mut(actor)?.words_8e_92;
            for axis in 0..3 {
                words[axis] = saved[axis].wrapping_sub(impulse[axis]);
            }
        }
        if self.object(actor)?.base.flags & 0x8000_0000 != 0 || along >= -0x32 {
            return Ok(0);
        }
        if kind == KIND_FIXED {
            // Byte 3 is the top byte of the H11 entry pointer whose slot
            // holds the packet: an allocation address, left unknown here.
            let source = host
                .process_null_handle()
                .ok_or(FurnitureResponseError::RequiredProcessNull { actor })?;
            let packet = ActorDamagePacket {
                strength: 3000,
                channel: 7,
                byte_03: None,
            };
            host.actor_damage(self, actor, ActorDamageRequest { packet, source })
                .map_err(|error| FurnitureResponseError::Host { actor, error })?;
            return Ok(0);
        }
        match host.network_impact_gate() {
            NetworkImpactGate::Offline => {}
            NetworkImpactGate::Session(0) => return Ok(0),
            NetworkImpactGate::Session(_) => {}
            NetworkImpactGate::Unowned => {
                return Err(FurnitureResponseError::RequiredNetworkGate { actor })
            }
        }
        if along >= -0x96 {
            return Ok(0);
        }
        let packet = ActorDamagePacket {
            strength: self.velocity_change_strength(actor, saved)?,
            channel: 1,
            byte_03: None,
        };
        let source = self.actor(actor)?.handle;
        host.actor_damage(self, actor, ActorDamageRequest { packet, source })
            .map_err(|error| FurnitureResponseError::Host { actor, error })?;
        host.furniture_damage(cell_xz, packet)
            .map_err(|error| FurnitureResponseError::Host { actor, error })?;
        Ok(0)
    }
}
