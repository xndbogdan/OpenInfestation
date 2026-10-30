//! Selector 35's +A0 `0043F790` in the pair pass: a transport on a rescue
//! takes a passenger aboard.

use thiserror::Error;

use crate::object_runtime::{
    DerivedActorError, FactoryStaffHost, ObjectArena, ObjectError, ObjectId,
};

#[derive(Debug, Error)]
pub enum Selector35PickupError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error("selector35 {actor:?} requires unwritten +{offset:03X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("selector35 pickup service at {address:08X}: {error}")]
    Host { address: u32, error: E },
}

impl ObjectArena {
    /// `0043F790`: a live transport (`+C4` at least 3) with a rescue count
    /// `+242` takes a partner whose `+5C` answers, which only Selector12's
    /// does (`00404610`, itself). The partner is disposed (`00479FA0`) and
    /// counted aboard in `+243`. Once all are aboard, the placement's record
    /// (`0048EAD0(+78, 0)`) and `00427BE0`'s settle follow. The slot flash
    /// (`00489A90(2)`) at the partner ends it. Answers whether it took the
    /// partner.
    pub fn selector35_picks_up<H: FactoryStaffHost>(
        &mut self,
        ship: ObjectId,
        partner: ObjectId,
        host: &mut H,
    ) -> Result<bool, Selector35PickupError<H::Error>> {
        let required = |offset, address| Selector35PickupError::RequiredField {
            actor: ship,
            offset,
            address,
        };
        if (self.actor(ship)?.word_c4 as i32) < 3 {
            return Ok(false);
        }
        let count =
            self.selector35_actor(ship)?.bytes_240[2].ok_or(required(0x242, 0x0043_f7a1))?;
        if count == 0 || self.selector12_actor(partner).is_err() {
            return Ok(false);
        }
        self.dispose(partner)?;
        let state = self.selector35_actor_mut(ship)?;
        let aboard = state.bytes_240[3]
            .ok_or(required(0x243, 0x0043_f7c9))?
            .wrapping_add(1);
        state.bytes_240[3] = Some(aboard);
        if aboard >= count {
            let index = self.actor(ship)?.byte_78 as i8;
            host.complete_placement(index)
                .map_err(|error| Selector35PickupError::Host {
                    address: 0x0043_f7f6,
                    error,
                })?;
            // 0043F7FB..33 inline 00427BE0.
            self.settle_pair_contact(ship)?;
        }
        let position = self
            .object(partner)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(partner))?;
        host.flash(self, position)
            .map_err(|error| Selector35PickupError::Host {
                address: 0x0043_f841,
                error,
            })?;
        Ok(true)
    }
}
