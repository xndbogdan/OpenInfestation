//! Selector35 +124=0043DB90. Unlike selector12's similarly shaped point
//! update, this uses unsigned DIV and writes fixed Y=-5000 without190.
use super::*;

impl ObjectArena {
    pub(super) fn selector35_update_evade<E>(
        &mut self,
        actor: ObjectId,
        delta: u32,
        context: &mut Selector35DescriptorContext<'_>,
    ) -> Result<(), Selector35DescriptorError<E>> {
        let timer = self.selector35_family_word(actor, 1, 0x18c, 0x0043_db9a)?;
        if (timer as i32) > delta as i32
            && self
                .derived_actor(actor)?
                .word_150
                .ok_or(required(actor, 0x150, 0x0043_dba6))? as i32
                > 2
        {
            self.family428650_actor_mut(actor)?.words_188_19c[2] = Some(20_000);
            self.family428650_actor_mut(actor)?.words_188_19c[1] = Some(timer.wrapping_sub(delta));
            return Ok(());
        }
        self.selector35_reset_timer(actor, 2_000_000, context.rng)?;
        if self.family428650_actor(actor)?.targets_1d4_1fc[3].is_none()
            && self.family428650_actor(actor)?.targets_1d4_1fc[1].is_none()
        {
            self.family428650_actor_mut(actor)?.word_22e |= 1;
            return Ok(());
        }
        self.selector35_reset_timer(actor, 1_000_000, context.rng)?;
        let selected = self.selector35_weighted(actor, 1, 3, 0x0043_dc62)?;
        let mut target = self.native_target(selected)?;
        let refreshed = self.refresh_native_target(&mut target, context.settings_7c);
        self.write_native_target(selected, target)?;
        refreshed?;
        let position = self.selector35_position(actor)?;
        let focus = self.selector35_focus(selected, 0x0043_dc87)?;
        let difference: [i16; 3] =
            std::array::from_fn(|i| position[i].wrapping_sub(focus[i]) as i16);
        let distance = integer_distance(
            difference
                .into_iter()
                .fold(0u32, |s, v| s.wrapping_add(squared_word(v))),
        );
        let (x, z) = if distance > 256 {
            let shift = 14 / distance;
            let mut x = word_shift(difference[0] as u16, shift);
            let mut z = word_shift(difference[2] as u16, shift);
            let rx = u32::from(name_entry_rng_next(context.rng) as u16);
            x = x.wrapping_add(((rx.wrapping_mul(x as i16 as i32 as u32) as i32) >> 16) as u16);
            let rz = u32::from(name_entry_rng_next(context.rng) as u16);
            z = z.wrapping_add(((rz.wrapping_mul(z as i16 as i32 as u32) as i32) >> 16) as u16);
            (x, z)
        } else {
            let rx = (name_entry_rng_next(context.rng) as u16) >> 3;
            let x = if distance & 1 != 0 {
                rx.wrapping_add(128)
            } else {
                0xff80u16.wrapping_sub(rx)
            };
            let rz = (name_entry_rng_next(context.rng) as u16) >> 3;
            let z = if distance & 1 != 0 {
                rz.wrapping_add(128)
            } else {
                0xff80u16.wrapping_sub(rz)
            };
            let distance = integer_distance(
                squared_word(difference[1])
                    .wrapping_add(squared_word(z as i16))
                    .wrapping_add(squared_word(x as i16)),
            );
            //Both new XZ magnitudes are in128..8319, so the native unsigned
            //DIV denominator cannot be zero on this closed RNG domain.
            let shift = 14 / distance;
            (word_shift(x, shift), word_shift(z, shift))
        };
        let position = self.selector35_position(actor)?;
        let current = self.selector35_current(actor, 0x0043_ddcc)?;
        let mut target = self.native_target(current)?;
        if target.kind_word() == 8 {
            target.set_focus_08_words([
                position[0].wrapping_add(x),
                0xec78,
                position[2].wrapping_add(z),
            ]);
            self.write_native_target(current, target)?;
        }
        Ok(())
    }
}

fn word_shift(word: u16, count: u32) -> u16 {
    //A WORD SHL still masks the x86 count to five bits.
    u32::from(word).wrapping_shl(count & 31) as u16
}
