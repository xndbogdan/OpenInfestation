//! Final10/36+FC424F50 and final12/35+FC427320. All target queries use
//! the actual arena heap; no target clone, hidden RNG or position default.

use super::{FamilyDescriptorScoreError as Error, FamilyDescriptorScoreRng};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use inf_formats::native_basis::{normalize_words, NativeLengthPrecision};

impl ObjectArena {
    pub(super) fn descriptor_fc_source(
        &mut self,
        actor: ObjectId,
        current: bool,
        rng: &mut FamilyDescriptorScoreRng<'_>,
    ) -> Result<i32, Error> {
        let common = self.family4236d0_actor(actor).is_ok();
        if !common && self.selector12_actor(actor).is_err() && self.selector35_actor(actor).is_err()
        {
            return Err(Error::RequiredActorVirtual {
                actor,
                callback: 0x0043_fa40,
                offset: 0xfc,
            });
        }
        //424F5E captures this result before any byte260 or RNG read.
        let source = self.descriptor_shared_fc(actor, current)?;
        if !common || !current {
            return Ok(source);
        }
        if self.family4236d0_actor(actor)?.byte_260 == 0 {
            let state = match rng {
                FamilyDescriptorScoreRng::Available(state) => &mut **state,
                FamilyDescriptorScoreRng::RequiredAtReach => {
                    return Err(Error::RequiredRandom {
                        actor,
                        address: 0x0042_4f77,
                    });
                }
            };
            let draw = crate::name_entry::name_entry_rng_next(state) as u16;
            if draw >= 0x4000 {
                return Ok(source);
            }
        }
        //Fresh148 at424F86; a successful virtual14 precedes byte260 publication.
        let reference =
            self.derived_actor(actor)?
                .target_148
                .ok_or(Error::MissingCurrentTarget {
                    actor,
                    address: 0x0042_4f86,
                })?;
        let object = self.native_target(reference)?.object_referent();
        self.family4236d0_actor_mut(actor)?.byte_260 = 1;
        let Some(object) = object else {
            return Ok(source);
        };
        let Some(previous) = self.route_previous(object)? else {
            return Ok(source);
        };
        //Native individual WORD reads are preserved within the current wholeXYZ
        //backing model: no partial axis is manufactured on a missing position.
        let target = self
            .object(object)?
            .base
            .position_words
            .ok_or(ObjectError::RoutePositionUnknown(object))?;
        let prior = self
            .object(previous)?
            .base
            .position_words
            .ok_or(ObjectError::RoutePositionUnknown(previous))?;
        let direction = normalize_words(
            std::array::from_fn(|i| target[i].wrapping_sub(prior[i]) as i16),
            NativeLengthPrecision::Retained,
        )
        .map(f64::from);
        let position = self
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        //Literal424FF7 uses CURRENT targetX; Y/Z use PREVIOUS node. This is
        //deliberately not the usual actor-minus-previous point-line vector.
        let x = f64::from(position[0] as i16) - f64::from(target[0] as i16);
        let y = f64::from(position[1] as i16) - f64::from(prior[1] as i16);
        let z = f64::from(position[2] as i16) - f64::from(prior[2] as i16);
        let spilled_x = f64::from(x as f32);
        let spilled_z = f64::from(z as f32);
        let truncated = [
            y * direction[0] - spilled_x * direction[1],
            z * direction[1] - y * direction[2],
            spilled_x * direction[2] - spilled_z * direction[0],
        ]
        .map(|v| super::super::kinematics::ftol(v) as i16 as i32);
        let squared = truncated
            .into_iter()
            .fold(0u32, |sum, v| sum.wrapping_add(v.wrapping_mul(v) as u32));
        if squared >= 0x1_0000 {
            return Ok(source);
        }
        //Final10/36+48=4026B0 returns actual embedded44. Z+Y then X, retained
        //53-bit accumulation and strict fcomp; equality/NaN keep byte260=1.
        let basis = self.actor(actor)?.rotating.basis;
        let forward = [7, 8, 9].map(|i| f64::from(f32::from_bits(basis.words[i])));
        let dot =
            (direction[2] * forward[2] + direction[1] * forward[1]) + direction[0] * forward[0];
        if dot > f64::from(f32::from_bits(0x3f66_6666)) {
            self.family4236d0_actor_mut(actor)?.byte_260 = 0;
        }
        Ok(source)
    }

    ///427320: signed14C precedes target dereference. NULL148 or virtual14 NULL
    ///returns the actual dispatcher-supplied fullDWORD1, not an invented score.
    fn descriptor_shared_fc(&self, actor: ObjectId, current: bool) -> Result<i32, Error> {
        if !current {
            return Ok(self.family428650_actor(actor)?.words_1d8_200[5] as i32);
        }
        let Some(reference) = self.derived_actor(actor)?.target_148 else {
            return Ok(1);
        };
        let distance = self
            .derived_actor(actor)?
            .word_14c
            .ok_or(Error::Uninitialized {
                actor,
                offset: 0x14c,
            })?;
        if distance as i32 > 256 {
            return Ok(self.family_radius_score(actor, 0x1_0000)? as i32);
        }
        let Some(object) = self.native_target(reference)?.object_referent() else {
            return Ok(1);
        };
        //Only the authenticated RouteNode layout owns rawDWORD44 here. Neither
        //actor presence nor another class's same-offset field substitutes for it.
        let raw_44 = match self.route_node(object) {
            Ok(route) => route.distance_squared,
            Err(ObjectError::NotRouteNode(_)) => {
                return Err(Error::RequiredReferent44 {
                    object,
                    address: 0x0042_735a,
                });
            }
            Err(error) => return Err(error.into()),
        };
        Ok(if raw_44 == 0 {
            1
        } else {
            self.family_radius_score(actor, 0x1_0000)? as i32
        })
    }
}
