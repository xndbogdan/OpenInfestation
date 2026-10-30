//! The local star generator and 78-cell ring overlay of 00459310.

use inf_formats::native_basis::sine_q15;

use crate::{
    frontend_scene::{FooterQuad, StarQuad},
    menu::MenuColor,
    menu_text::MenuTextResources,
    name_entry::{CONFIRM_CELL, DELETE_CELL, NAME_GRID},
};

use super::{divide, DrawContext, NameEntryDrawError, NameEntryDrawHost};

impl<R: MenuTextResources, H: NameEntryDrawHost> DrawContext<'_, R, H> {
    pub(super) fn stars(&mut self) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        let [width, height] = self.frame.surface_pixels;
        let age = self.frame.age_millis;
        // 00459410: this is a fresh local generator, not the process RNG.
        let mut seed = 0_i32;
        for _ in 0..1000 {
            seed = seed.wrapping_mul(0x13_d56d).wrapping_add(0x13_d573);
            let x = seed.wrapping_sub(divide(seed, width)?.wrapping_mul(width));
            seed = seed.wrapping_mul(0x13_d56d).wrapping_add(0x13_d573);
            let y = seed.wrapping_sub(divide(seed, height)?.wrapping_mul(height));
            let angle = seed.wrapping_add(age.wrapping_mul(50)) & 0x7fff;
            let twinkle =
                (sine_q15(angle, self.frame.sine_quarter).wrapping_mul(5) >> 15) as i16 as i32;
            let mut material = (37 + seed.wrapping_abs() % 7 + twinkle).clamp(37, 45);
            if age < 2000 {
                material = material
                    .wrapping_sub(2000_i32.wrapping_sub(age) / 100)
                    .max(37);
            }
            self.host
                .star(StarQuad {
                    corners: [
                        [x as i16, y as i16],
                        [x.wrapping_add(1) as i16, y as i16],
                        [x.wrapping_add(1) as i16, y.wrapping_add(1) as i16],
                        [x as i16, y.wrapping_add(1) as i16],
                    ],
                    color_argument: material,
                })
                .map_err(NameEntryDrawError::Host)?;
        }
        Ok(())
    }

    pub(super) fn rings(&mut self) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        // 00459702..0045970D: only the nonzero unsigned value <=1 skips.
        if self.frame.state.completion_delay == 1 {
            return Ok(());
        }
        let resources = self.resources;
        for index in 0..78_i32 {
            let byte = [NAME_GRID[index as usize]];
            let text = match index {
                DELETE_CELL => resources
                    .string(279)
                    .map_err(NameEntryDrawError::Resource)?,
                CONFIRM_CELL => resources
                    .string(280)
                    .map_err(NameEntryDrawError::Resource)?,
                _ => &byte,
            };
            let radius = self.ring_radius(index)?;
            let phase = index.wrapping_sub(self.frame.state.selection) % 26;
            let angle = phase.wrapping_shl(16) / 26;
            let position_angle = angle.wrapping_add(self.frame.state.row_anim);
            let dx = (sine_q15(position_angle, self.frame.sine_quarter).wrapping_mul(radius[0])
                >> 15) as i16 as i32;
            let dy = (sine_q15(position_angle.wrapping_add(0x4000), self.frame.sine_quarter)
                .wrapping_mul(radius[1])
                >> 15) as i16 as i32;
            let [width, height] = self.frame.surface_pixels;
            let mut position = [
                (width / 2).wrapping_add(dx.wrapping_mul(3) / 4),
                (height / 2).wrapping_sub(dy.wrapping_mul(3) / 4),
            ];
            let selected = index == self.frame.state.selection && self.frame.age_millis > 2000;
            if selected {
                self.ring_highlight(position)?;
            }
            position[0] = position[0].wrapping_sub(self.width(text)? / 2);
            let shadow = self.color(7)?;
            self.line(text, position.map(|value| value.wrapping_add(1)), shadow)?;
            let color = if selected {
                self.color(3)?
            } else {
                self.ring_color(angle, radius[1])?
            };
            // SI/BP save the low words before the shadow walker mutates X/Y.
            self.line(text, position.map(|value| value as i16 as i32), color)?;
        }
        Ok(())
    }

    fn ring_radius(&self, index: i32) -> Result<[i32; 2], NameEntryDrawError<R::Error, H::Error>> {
        let x = self.frame.surface_pixels[0].wrapping_mul(7) / 20;
        let y = divide(x.wrapping_mul(self.frame.focal[1]), self.frame.focal[0])?;
        let delay = self.frame.state.completion_delay;
        let mut radius = [x, y].map(|value| {
            if delay != 0 {
                // 00459812..00459837 uses MUL and SHR: unsigned /1000 after
                // the wrapping doubled product, unlike the age branch.
                (delay.wrapping_sub(500).wrapping_mul(value).wrapping_mul(2) as u32 / 1000) as i32
            } else if self.frame.age_millis < 2000 {
                value.wrapping_mul(self.frame.age_millis) / 2000
            } else {
                value
            }
        });
        if index >= 52 {
            radius = radius.map(|value| value.wrapping_mul(14) / 20);
        } else if index >= 26 {
            radius = radius.map(|value| value.wrapping_mul(17) / 20);
        }
        Ok(radius)
    }

    fn ring_highlight(
        &mut self,
        [x, y]: [i32; 2],
    ) -> Result<(), NameEntryDrawError<R::Error, H::Error>> {
        // The authored constant is "EN", not the selected label.
        let half_width = self.width(b"EN")?;
        let half_height = divide(
            half_width.wrapping_mul(self.frame.focal[1]),
            self.frame.focal[0],
        )?;
        let style_word = self
            .resources
            .style(3)
            .map_err(NameEntryDrawError::Resource)?;
        let center_y = y.wrapping_sub(half_height / 4);
        let left = x.wrapping_sub(half_width) as i16;
        let right = x.wrapping_add(half_width) as i16;
        let top = center_y.wrapping_sub(half_height) as i16;
        let bottom = center_y.wrapping_add(half_height) as i16;
        self.host
            .highlight(FooterQuad {
                sprite_slot: 7,
                corners: [[left, top], [right, top], [right, bottom], [left, bottom]],
                style_word,
                vertex_attribute: 0x4000,
            })
            .map_err(NameEntryDrawError::Host)
    }

    fn ring_color(
        &self,
        angle: i32,
        radius_y: i32,
    ) -> Result<MenuColor, NameEntryDrawError<R::Error, H::Error>> {
        let age = self.frame.age_millis;
        let shade_angle = angle.wrapping_sub(age.min(0x3000)).wrapping_add(0x4000);
        let vertical = (sine_q15(shade_angle, self.frame.sine_quarter).wrapping_mul(radius_y) >> 15)
            as i16 as i32;
        let mut color = self.color(if age < 1000 { 7 } else { 2 })?;
        let since = age.wrapping_sub(1000);
        if since < 8000 {
            // 00459D80 resolves the second endpoint before the divisions.
            let target = if since >= 3000 {
                Some(self.color(2)?)
            } else {
                None
            };
            // The byte stores between the two arithmetic stages are observable
            // for negative ages and wrapped radii; do not fuse the fractions.
            let extra = radius_y.wrapping_mul(9) / 8;
            let numerator = extra.wrapping_add(vertical);
            let denominator = extra.wrapping_add(radius_y).wrapping_add(1);
            for value in &mut color.rgb {
                *value = divide(i32::from(*value).wrapping_mul(numerator), denominator)? as u8;
            }
            if since < 3000 {
                for value in &mut color.rgb {
                    *value = (i32::from(*value).wrapping_mul(since) / 3000) as u8;
                }
            } else if let Some(target) = target {
                for (value, target) in color.rgb.iter_mut().zip(target.rgb) {
                    let delta = i32::from(target).wrapping_sub(i32::from(*value));
                    *value = value
                        .wrapping_add((delta.wrapping_mul(since.wrapping_sub(3000)) / 5000) as u8);
                }
            }
        }
        Ok(color)
    }
}
