//! Selector 35's landing descriptor (index 7): score 0043FC60, entry
//! 0043FCA0 (0043E660) and update 0043FCC0 (0043E390). A ship picks a
//! landing site at random, flies to a point behind it ("Traveling"),
//! settles there ("Landing"), waits ("Waiting") and gives the descriptor up.
//! The sites are the actors whose +54 answers with byte +138 set: the
//! 0040D800 family but the course targets, which clear it.

use inf_formats::native_basis::isqrt_48efc0;

use super::{required, Selector35DescriptorContext, Selector35DescriptorError};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{native_trunc_low32, ActorTerrainHost, ObjectArena, ObjectId},
};

/// The status texts +228 names: "Traveling", "Landing" and "Waiting".
const TRAVELING: u32 = 0x0053_3db8;
const LANDING: u32 = 0x0053_3dc4;
const WAITING: u32 = 0x0053_3dcc;
/// 0043E305: a site nearer than 1000 (squared) is passed over.
const SITE_SQUARED: u32 = 1_000_000;
/// 0043E5ED: below this speed a ship over its point starts landing.
const LANDING_SPEED: i16 = 0x32;

impl ObjectArena {
    /// `0043E660`: entering, the ship's current target `+148` becomes a fresh
    /// kind-8 point at its own position (the old one is not released), and
    /// `+18C`, `+190`, `+22C` and `+208` clear. Leaving does nothing.
    pub(super) fn selector35_change_landing<E>(
        &mut self,
        actor: ObjectId,
        entering: bool,
        context: &mut Selector35DescriptorContext<'_>,
    ) -> Result<(), Selector35DescriptorError<E>> {
        if !entering {
            return Ok(());
        }
        // 0043E67C: +04 is the process NULL 0054E894, +08 the position.
        self.selector35_point_at_self(actor, context)?;
        let family = self.family428650_actor_mut(actor)?;
        family.words_188_19c[1] = Some(0);
        family.words_188_19c[2] = Some(0);
        family.byte_22c = 0;
        family.words_204_208[1] = Some(0);
        Ok(())
    }

    /// `0043E390`, one landing update by `delta` microseconds, over the
    /// stage `+190`: 0 picks a site, 1 travels, 2 lands and 3 waits.
    pub(super) fn selector35_update_landing<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        context: &mut Selector35DescriptorContext<'_>,
        terrain: &mut H,
    ) -> Result<(), Selector35DescriptorError<H::Error>> {
        // 0043E396: no site anywhere, or no current target, gives it up.
        if self.selector35_actor(actor)?.byte_26c != 0
            || self.derived_actor(actor)?.target_148.is_none()
        {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        }
        let stage = self.selector35_family_word(actor, 2, 0x190, 0x0043_e3b3)?;
        let delta = delta as i32;
        match stage {
            1 => {
                self.family428650_actor_mut(actor)?.word_228 = TRAVELING;
                let size = u32::from(self.object(actor)?.base.word_28);
                let height = self.selector35_height(actor, 0x0043_e5b1)?;
                let mut next = 1;
                if height < (size >> 2) as i32 {
                    let [vx, _, vz] = self.actor(actor)?.words_8e_92.map(|w| i32::from(w as i16));
                    let speed = isqrt_48efc0(vx.wrapping_mul(vx).wrapping_add(vz.wrapping_mul(vz)));
                    if (speed as u16 as i16) < LANDING_SPEED {
                        next = 2;
                        let descent = self.selector35_actor(actor)?.word_234.wrapping_shl(12);
                        self.family428650_actor_mut(actor)?.words_188_19c[3] = Some(descent);
                        self.selector35_actor_mut(actor)?.word_230 = 0;
                    }
                } else {
                    let state = self.selector35_actor_mut(actor)?;
                    state.word_230 = 0xfa;
                    state.word_234 = size + 500;
                }
                // 0043E628: a ship with a helper target `+154` stops here.
                if self.derived_actor(actor)?.target_154.is_some() {
                    next = 0;
                }
                self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(next);
            }
            2 => {
                self.family428650_actor_mut(actor)?.word_228 = LANDING;
                let size = u32::from(self.object(actor)?.base.word_28);
                if self.selector35_height(actor, 0x0043_e527)? > (size >> 1) as i32 {
                    self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(1);
                    return Ok(());
                }
                let timer = self.selector35_family_word(actor, 3, 0x194, 0x0043_e557)? as i32;
                if timer > delta {
                    let left = timer.wrapping_sub(delta);
                    let family = self.family428650_actor_mut(actor)?;
                    family.words_188_19c[2] = Some(2);
                    family.words_188_19c[3] = Some(left as u32);
                    self.selector35_actor_mut(actor)?.word_234 = (left >> 12) as u32;
                    return Ok(());
                }
                let random = name_entry_rng_next(context.rng) & 0xffff;
                let family = self.family428650_actor_mut(actor)?;
                family.words_188_19c[2] = Some(3);
                family.words_188_19c[3] = Some((random + 0x2625a) << 5);
            }
            3 => {
                self.family428650_actor_mut(actor)?.word_228 = WAITING;
                self.selector35_actor_mut(actor)?.word_234 = 0;
                let timer = self.selector35_family_word(actor, 3, 0x194, 0x0043_e4cd)? as i32;
                if timer > delta {
                    let left = timer.wrapping_sub(delta);
                    let family = self.family428650_actor_mut(actor)?;
                    family.words_188_19c[2] = Some(3);
                    family.words_188_19c[3] = Some(left as u32);
                    self.selector35_actor_mut(actor)?.word_234 = (left >> 20) as u32;
                    return Ok(());
                }
                let family = self.family428650_actor_mut(actor)?;
                family.word_22e |= 1;
                family.words_188_19c[2] = Some(0);
            }
            _ => {
                if self.selector35_find_site(actor, context.rng)? {
                    let handle = self.selector35_actor(actor)?.handle_268;
                    if let Some(site) = self.resolve_actor_handle(handle)? {
                        let point = self.selector35_landing_point(actor, site, terrain)?;
                        let reference = self.selector35_current(actor, 0x0043_e4a0)?;
                        let mut target = self.native_target(reference)?;
                        if target.kind_word() == 8 {
                            target.set_focus_08_words(point);
                            self.write_native_target(reference, target)?;
                        }
                        self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(1);
                        return Ok(());
                    }
                }
                let family = self.family428650_actor_mut(actor)?;
                family.word_22e |= 1;
                family.words_188_19c[2] = Some(stage);
            }
        }
        Ok(())
    }

    /// `0043E270`: over the actor list, every site (flags `0x28` without
    /// `0x80`, a `+54` that answers, its byte `+138` set) clears `+26C`, and
    /// one farther than 1000 draws the RNG: the highest draw's handle goes
    /// to `+268`. Answers whether that draw, signed, is above zero.
    fn selector35_find_site<E>(
        &mut self,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<bool, Selector35DescriptorError<E>> {
        let mut best = 0x8000_0001_u32 as i32;
        self.selector35_actor_mut(actor)?.byte_26c = 1;
        for candidate in self.actor_list().to_vec() {
            if self.object(candidate)?.base.flags & 0xa8 != 0x28
                || !self.actor_completion_kept(candidate)?
                || self.family40d800(candidate)?.byte_138 == 0
            {
                continue;
            }
            self.selector35_actor_mut(actor)?.byte_26c = 0;
            let site = self.landing_position(candidate, 0x0043_e2d1)?;
            let own = self.landing_position(actor, 0x0043_e2d5)?;
            let squared = [2, 0, 1].into_iter().fold(0_i32, |sum, axis| {
                let delta = i32::from(site[axis].wrapping_sub(own[axis]) as i16);
                sum.wrapping_add(delta.wrapping_mul(delta))
            }) as u32;
            if squared <= SITE_SQUARED {
                continue;
            }
            let draw = (name_entry_rng_next(rng) & 0xffff) as i32;
            if draw > best {
                best = draw;
                self.selector35_actor_mut(actor)?.handle_268 = self.actor(candidate)?.handle;
            }
        }
        Ok(best > 0)
    }

    /// `0043E402..0043E49B`: the point behind the site's face by the ship's
    /// size and a quarter plus the site's, at the ground's height there.
    fn selector35_landing_point<H: ActorTerrainHost>(
        &mut self,
        actor: ObjectId,
        site: ObjectId,
        terrain: &mut H,
    ) -> Result<[u16; 3], Selector35DescriptorError<H::Error>> {
        let size = i32::from(self.object(actor)?.base.word_28);
        let reach = ((size * 5) >> 2) + i32::from(self.object(site)?.base.word_28);
        // The site's +48 is 004026B0, its own basis: forward X and Z.
        let basis = self.actor(site)?.rotating.basis;
        let along = |word: u32| {
            native_trunc_low32(f64::from(reach) * f64::from(f32::from_bits(word))) as u16
        };
        let position = self.landing_position(site, 0x0043_e40e)?;
        let x = position[0].wrapping_sub(along(basis.words[7]));
        let z = position[2].wrapping_sub(along(basis.words[9]));
        let height = terrain
            .terrain(self, actor)
            .map_err(Selector35DescriptorError::Terrain)?
            .height_at([x, z]);
        Ok([x, height as u16, z])
    }

    fn landing_position<E>(
        &self,
        object: ObjectId,
        address: u32,
    ) -> Result<[u16; 3], Selector35DescriptorError<E>> {
        self.object(object)?
            .base
            .position_words
            .ok_or(required(object, 0x32, address))
    }

    fn selector35_height<E>(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<i32, Selector35DescriptorError<E>> {
        self.selector35_actor(actor)?
            .word_260
            .map(|height| height as i32)
            .ok_or(required(actor, 0x260, address))
    }
}
