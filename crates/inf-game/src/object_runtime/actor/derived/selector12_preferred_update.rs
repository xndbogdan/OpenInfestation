//! Complete Selector12 preferred-target update469310/+13C=467660 and467720.
//! Actual target/terrain/grant owners retain every reached prefix on error.
//! First nonzero grant may require constructor-unwritten14C; no seeding occurs.
use crate::object_runtime::{
    terrain_furniture_grant, DerivedActorError, FamilyGrantGridError, FamilyGrantGridHost,
    FamilyTerrainGrantRequest, GrantOwner, LiveGrantContext, LiveGrantError, LiveGrantOperation,
    LiveGrantRequest, NativeHeapRef, ObjectArena, ObjectError, ObjectId,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug)]
pub struct Selector12PreferredUpdateRequest<'context, 'source> {
    pub microseconds: u32,
    pub live_grants: &'context LiveGrantContext<'source>,
}

#[derive(Debug, Error)]
pub enum Selector12PreferredUpdateError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Class(#[from] DerivedActorError),
    #[error("selector12 {actor:?} requires retained field+{offset:X}")]
    RequiredField { actor: ObjectId, offset: u16 },
    #[error("target {target:?} requires retained WORD+{offset:X}")]
    RequiredTargetField { target: NativeHeapRef, offset: u16 },
    #[error(transparent)]
    Terrain(#[from] FamilyGrantGridError<E>),
    #[error(transparent)]
    Grant(#[from] LiveGrantError),
}

impl ObjectArena {
    /// The supplied owner is the actual class12 virtual. No profile, actor slot,
    /// mission or model selects behavior. This body reads148 once, never refreshes
    /// target+18 or computes a replacement for retained14C.
    pub fn update_selector12_preferred_target<H: FamilyGrantGridHost>(
        &mut self,
        actor: ObjectId,
        request: Selector12PreferredUpdateRequest<'_, '_>,
        host: &mut H,
    ) -> Result<(), Selector12PreferredUpdateError<H::Error>> {
        //Authenticate the represented complete class12 receiver domain.
        self.selector12_actor(actor)?;
        let reference = self.derived_actor(actor)?.target_148.ok_or(
            Selector12PreferredUpdateError::RequiredField {
                actor,
                offset: 0x148,
            },
        )?;
        let target = self.native_target(reference)?;
        let grant = if target.kind_word() == 2 {
            //467677 uses the actual allocation's virtual10, not kind-based
            //reconstruction of a new follow target.
            match self.resolve_target_actor(&target)? {
                Some(referent) => self.actor(referent)?.word_7a,
                None => 0,
            }
        } else {
            //467684 reads Z before X. Whole-point backing preserves unknown
            //words as one existing Option; no virtual1C or referent substitution.
            let focus = target.raw_focus_08_words().ok_or(
                Selector12PreferredUpdateError::RequiredTargetField {
                    target: reference,
                    offset: 0x0c,
                },
            )?;
            let z = focus[2];
            let x = focus[0];
            terrain_furniture_grant(self, FamilyTerrainGrantRequest { actor, xz: [x, z] }, host)?
        };
        if grant == 0 {
            //467698 ORs lowBYTE only; the retained highBYTE is untouched.
            self.family428650_actor_mut(actor)?.word_22e |= 0x000c;
            return Ok(());
        }
        //4676A5 precedes198, which precedes190. The first entered update is
        //before the movement tail, so authentic constructorNone must survive.
        let distance = self.derived_actor(actor)?.word_14c.ok_or(
            Selector12PreferredUpdateError::RequiredField {
                actor,
                offset: 0x14c,
            },
        )?;
        let threshold = self.family428650_actor(actor)?.words_188_19c[4].ok_or(
            Selector12PreferredUpdateError::RequiredField {
                actor,
                offset: 0x198,
            },
        )?;
        let mut delta = request.microseconds;
        if (distance as i32) > (threshold as i32) {
            let difference = distance.wrapping_sub(threshold);
            let difference = if (difference as i32) > 920 {
                920
            } else {
                difference
            };
            let scaled = ((difference.wrapping_mul(delta) as i32) >> 10) as u32;
            delta = delta.wrapping_sub(scaled);
        }
        let remaining = self.family428650_actor(actor)?.words_188_19c[2].ok_or(
            Selector12PreferredUpdateError::RequiredField {
                actor,
                offset: 0x190,
            },
        )?;
        if (remaining as i32) > (delta as i32) {
            self.family428650_actor_mut(actor)?.words_188_19c[2] =
                Some(remaining.wrapping_sub(delta));
            return Ok(());
        }
        //4676E3 flags first, then190 and194 before actual embedded24C setter.
        self.family428650_actor_mut(actor)?.word_22e |= 0x000c;
        self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(0);
        self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(1);
        //MOV BX preserves native highEBX, but every represented setter/record/
        //query consumer uses LOWWORD. No unowned upper-register value is invented.
        self.mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Selector12Actor24c(actor),
                operation: LiveGrantOperation::SetRaw {
                    id: u32::from(grant),
                },
            },
            request.live_grants,
        )?;
        self.notify_selector12_single_grant(actor, u32::from(grant))?;
        Ok(())
    }

    /// Complete467720, separate from the whole-grant event22 difference owner.
    /// Its first-player NULL branch precedes all receiver/mask authentication.
    fn notify_selector12_single_grant(
        &mut self,
        actor: ObjectId,
        raw_grant: u32,
    ) -> Result<(), LiveGrantError> {
        let Some(player) = self.first_player() else {
            return Ok(());
        };
        let owned = self.live_grant_has(GrantOwner::Player9c(player), raw_grant as u16)?;
        self.selector12_actor_mut(actor)?.byte_29d = u8::from(!owned);
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector12_preferred_update/native_guards.rs"]
mod native_guards;
#[cfg(test)]
#[path = "selector12_preferred_update/tests.rs"]
mod tests;
