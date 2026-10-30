//! Profile callbacks 004483B0/00448990/00448C60/00448D10/00448E60/00449320.
//! Byte strings retain their H0/save custody. Camera selection belongs to the
//! frontend adapter; all text, clip and draw-time menu mutations occur here.

use thiserror::Error;

use crate::{
    menu::{MenuColor, MenuRowDrawRequest, MenuState, MenuStateError},
    menu_text::{
        measure_text_width, MenuRowTextRequest, MenuTextEmission, MenuTextLayoutError,
        MenuTextOutcome, MenuTextPlacement, MenuTextResources, MenuTextReveal, RawMenuTextRequest,
        INITIAL_GLYPH_BASE_WORD,
    },
    profile_menu::{ProfileMenuState, ProfileMode, ProfileTimer, ProfileTimerKind},
    profile_store::{ProfileStore, ProfileStoreError},
};

/// The sink executes existing native text consumers; it does not manufacture
/// temporary string-resource slots or turn raw text into animated row text.
pub trait ProfileTextHost {
    type Error;
    fn row_text(&mut self, request: MenuRowTextRequest<'_>)
        -> Result<MenuTextOutcome, Self::Error>;
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error>;
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error>;
    fn published_origin_pixels(&self) -> [i32; 2];
}

#[derive(Debug, Error)]
pub enum ProfileDrawError<R, H> {
    #[error("profile text resource: {0}")]
    Resource(R),
    #[error("profile text backend: {0}")]
    Host(H),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error(transparent)]
    Store(#[from] ProfileStoreError),
    #[error(transparent)]
    State(#[from] MenuStateError),
    #[error("unsupported profile row callback {0:#010x}")]
    UnsupportedCallback(u32),
}

pub struct ProfileRowDraw<'a> {
    pub request: &'a mut MenuRowDrawRequest,
    pub base: &'a mut MenuState,
    pub state: &'a mut ProfileMenuState,
    pub store: &'a mut ProfileStore,
    pub owner_flag: bool,
    pub surface_pixels: [i32; 2],
}

pub fn draw_profile_row<R: MenuTextResources, H: ProfileTextHost>(
    draw: ProfileRowDraw<'_>,
    resources: &R,
    host: &mut H,
) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
    let mut context = ProfileDrawContext { resources, host };
    match draw.request.callback_address {
        0x0044_8e60 => context.header(draw),
        0x0044_9320 => context.slot(draw),
        0x0044_8c60 => context.exceptional(draw),
        0x0044_8d10 => context.format(draw),
        other => Err(ProfileDrawError::UnsupportedCallback(other)),
    }
}

/// 004483B0 resolves font/style even on a suppressed row frame. Call this
/// before the derived draw gate so a failure cannot acknowledge its handshake.
pub fn prepare_profile_draw<R: MenuTextResources>(resources: &R) -> Result<(), R::Error> {
    resources.font(0)?;
    resources.style(3)?;
    Ok(())
}

pub fn draw_profile_status<R: MenuTextResources, H: ProfileTextHost>(
    timer: ProfileTimer,
    surface: [i32; 2],
    resources: &R,
    host: &mut H,
) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
    let font = resources.font(0).map_err(ProfileDrawError::Resource)?;
    let start = resources
        .style(3)
        .map_err(ProfileDrawError::Resource)?
        .to_le_bytes();
    let end = resources
        .style(2)
        .map_err(ProfileDrawError::Resource)?
        .to_le_bytes();
    let phase = timer.millis % 400;
    let color = MenuColor::from_rgb(std::array::from_fn(|i| {
        (i32::from(start[i]) + (i32::from(end[i]) - i32::from(start[i])) * phase / 400) as u8
    }));
    let label = match timer.kind {
        ProfileTimerKind::LoadFailure => 0xc6,
        ProfileTimerKind::CopyFailure => 0xc8,
        ProfileTimerKind::InitialWarning => 0xc7,
        ProfileTimerKind::DeleteFailure => 0xc9,
    };
    let text = resources
        .string(label)
        .map_err(ProfileDrawError::Resource)?;
    let measured = measure_text_width(font, text)? as i16;
    host.raw_text(&mut RawMenuTextRequest {
        text,
        font,
        position_pixels: [
            surface[0].wrapping_sub(i32::from(measured)) / 2,
            surface[1].wrapping_mul(69) / 100,
        ],
        width_pixels: surface[0],
        color,
        base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
        context_origin_pixels: host.published_origin_pixels(),
        emission: MenuTextEmission::Draw,
        reveal: MenuTextReveal::Complete,
    })
    .map_err(ProfileDrawError::Host)
}

struct ProfileDrawContext<'a, R, H> {
    resources: &'a R,
    host: &'a mut H,
}

impl<R: MenuTextResources, H: ProfileTextHost> ProfileDrawContext<'_, R, H> {
    fn color(&self, style: i32) -> Result<MenuColor, ProfileDrawError<R::Error, H::Error>> {
        self.resources
            .style(style)
            .map(MenuColor::from_word)
            .map_err(ProfileDrawError::Resource)
    }

    fn text(
        &mut self,
        row: &mut MenuRowDrawRequest,
        text: &[u8],
    ) -> Result<i32, ProfileDrawError<R::Error, H::Error>> {
        Ok(self
            .host
            .row_text(MenuRowTextRequest {
                descriptor: &mut row.descriptor,
                text,
                placement: MenuTextPlacement::Left,
                context_origin_pixels: row.context_origin_pixels,
            })
            .map_err(ProfileDrawError::Host)?
            .advance_from_origin)
    }

    fn label(
        &mut self,
        row: &mut MenuRowDrawRequest,
        label: i32,
    ) -> Result<i32, ProfileDrawError<R::Error, H::Error>> {
        let text = self
            .resources
            .string(label)
            .map_err(ProfileDrawError::Resource)?;
        self.text(row, text)
    }

    fn confirmation(
        &mut self,
        draw: &mut ProfileRowDraw<'_>,
        selected_x: i16,
        prompt: i32,
    ) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
        let row = &mut *draw.request;
        row.descriptor.position_pixels[0] = selected_x;
        row.descriptor.position_pixels[1] = row.descriptor.position_pixels[1]
            .wrapping_add((i32::from(row.descriptor.extent_pixels[1]).wrapping_mul(7) >> 5) as i16);
        row.descriptor.color = self.color(3)?;
        let advance = self.label(row, prompt)?;
        advance_x(row, advance);
        let yes = self
            .resources
            .string(0x10a)
            .map_err(ProfileDrawError::Resource)?
            .first()
            .copied()
            .unwrap_or(0);
        let no = self
            .resources
            .string(0x10b)
            .map_err(ProfileDrawError::Resource)?
            .first()
            .copied()
            .unwrap_or(0);
        let segments: [(&[u8], i32); 5] = [
            (b" (", 2),
            (
                std::slice::from_ref(&yes),
                if draw.state.choice() { 3 } else { 2 },
            ),
            (b"/", 2),
            (
                std::slice::from_ref(&no),
                if draw.state.choice() { 2 } else { 3 },
            ),
            (b")", 2),
        ];
        for (i, (text, style)) in segments.into_iter().enumerate() {
            row.descriptor.color = self.color(style)?;
            let advance = self.text(row, text)?;
            if i != 4 {
                advance_x(row, advance);
            }
        }
        Ok(())
    }

    fn header(
        &mut self,
        mut draw: ProfileRowDraw<'_>,
    ) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
        self.resources.font(0).map_err(ProfileDrawError::Resource)?;
        let selected_color = self.color(3)?;
        let status = draw.store.query(false)?.status as u32;
        let mut selected_x = draw.request.descriptor.position_pixels[0];
        if status != 0 {
            if matches!(status, 2 | 4) {
                draw.request.descriptor.color = selected_color;
                self.label(draw.request, draw.request.arguments[0] as i32)?;
            } else if draw.owner_flag {
                draw.request.descriptor.color = selected_color;
                self.label(draw.request, 0x90)?;
            } else {
                for (mode, label) in [0xcd, 0xd0, 0xcf].into_iter().enumerate() {
                    let selected = draw.state.mode().native() == mode as i32;
                    draw.request.descriptor.color = if selected {
                        selected_color
                    } else {
                        self.color(2)?
                    };
                    if selected {
                        selected_x = draw.request.descriptor.position_pixels[0];
                    }
                    self.label(draw.request, label)?;
                    if mode < 2 {
                        let extent = i32::from(draw.request.descriptor.extent_pixels[0]);
                        advance_x(
                            draw.request,
                            if mode == 0 {
                                extent / 3
                            } else {
                                extent.wrapping_mul(9) / 30
                            },
                        );
                        draw.request.descriptor.age_millis =
                            draw.request.descriptor.age_millis.wrapping_sub(300);
                    }
                }
            }
        }
        if draw.base.navigation_locked() {
            let prompt = if draw.owner_flag { 0xca } else { 0xce };
            self.confirmation(&mut draw, selected_x, prompt)?;
        }
        if draw.owner_flag {
            let [width, height] = draw.surface_pixels;
            self.host
                .set_clip_rect([0, 0, width as i16, height as i16])
                .map_err(ProfileDrawError::Host)?;
            self.host
                .raw_text(&mut RawMenuTextRequest {
                    text: self
                        .resources
                        .string(0x28)
                        .map_err(ProfileDrawError::Resource)?,
                    font: self.resources.font(0).map_err(ProfileDrawError::Resource)?,
                    // 00449260 divides width directly by ten; 00449283 wraps
                    // width*8 before dividing by ten. Equivalent percentages
                    // with different intermediate products do not preserve i32 overflow.
                    position_pixels: [width / 10, height.wrapping_mul(7) / 8],
                    width_pixels: width.wrapping_mul(8) / 10,
                    color: selected_color,
                    base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                    context_origin_pixels: self.host.published_origin_pixels(),
                    emission: MenuTextEmission::Draw,
                    reveal: MenuTextReveal::Complete,
                })
                .map_err(ProfileDrawError::Host)?;
        }
        Ok(())
    }

    fn slot(
        &mut self,
        draw: ProfileRowDraw<'_>,
    ) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
        if draw.store.query(false)?.status as u32 == 0 {
            return Ok(());
        }
        let slot = draw.request.arguments[0] as usize;
        let mut text = format!("{}. ", slot.wrapping_add(1)).into_bytes();
        if let Some(entry) = draw.store.entry(slot)? {
            // 0044A480(buffer, slot, false): "%d. %s - %s %02d:%02d".
            text.extend_from_slice(terminated(&entry.name));
            text.extend_from_slice(b" - ");
            let label = i32::from(entry.mission_selection.wrapping_add(0xe0) as i16);
            if entry.pelagia {
                // Port option: a game saved in Pelagia, not "Completed".
                text.extend_from_slice(crate::pelagia::NAME);
            } else {
                text.extend_from_slice(terminated(
                    self.resources
                        .string(label)
                        .map_err(ProfileDrawError::Resource)?,
                ));
            }
            text.extend_from_slice(
                format!(
                    " {:02}:{:02}",
                    entry.elapsed_seconds / 3600,
                    (entry.elapsed_seconds / 60) % 60
                )
                .as_bytes(),
            );
        } else {
            if slot as i32 == draw.base.selection().wrapping_sub(1)
                && draw.state.mode() != ProfileMode::Load
            {
                draw.state.change_mode(
                    draw.base,
                    -draw.state.mode().native(),
                    draw.owner_flag,
                    draw.store.cached_count(),
                )?;
            }
            text.extend_from_slice(terminated(
                self.resources
                    .string(0xd1)
                    .map_err(ProfileDrawError::Resource)?,
            ));
        }
        self.text(draw.request, &text)?;
        Ok(())
    }

    fn exceptional(
        &mut self,
        draw: ProfileRowDraw<'_>,
    ) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
        let original_x = draw.request.descriptor.position_pixels[0];
        self.label(draw.request, draw.request.arguments[0] as i32)?;
        draw.request.descriptor.position_pixels[0] = original_x;
        draw.request.descriptor.position_pixels[1] = draw.request.descriptor.position_pixels[1]
            .wrapping_add((draw.surface_pixels[1].wrapping_mul(5) / 100) as i16);
        self.label(draw.request, if draw.owner_flag { 0x26 } else { 0x28 })?;
        Ok(())
    }

    fn format(
        &mut self,
        mut draw: ProfileRowDraw<'_>,
    ) -> Result<(), ProfileDrawError<R::Error, H::Error>> {
        let initial = draw.state.format_state();
        let label = match initial {
            0 if draw.base.navigation_locked() => 0x93,
            0 => {
                draw.state.set_format_state(3);
                0x9d
            }
            1 => 0x9a,
            2 => 0x98,
            3 => 0x9d,
            _ => draw.request.arguments[0] as i32,
        };
        let original_x = draw.request.descriptor.position_pixels[0];
        draw.request.descriptor.color = self.color(3)?;
        self.label(draw.request, label)?;
        if draw.base.navigation_locked() {
            self.confirmation(&mut draw, original_x, 0xce)?;
        }
        // Initial state0 writes3, but does not enter this help branch until the next draw.
        if matches!(initial, 1..=3) {
            draw.request.descriptor.position_pixels[0] = original_x;
            draw.request.descriptor.position_pixels[1] = draw.request.descriptor.position_pixels[1]
                .wrapping_add((draw.surface_pixels[1].wrapping_mul(5) / 100) as i16);
            self.label(draw.request, 0x28)?;
        }
        Ok(())
    }
}

fn advance_x(row: &mut MenuRowDrawRequest, advance: i32) {
    row.descriptor.position_pixels[0] =
        row.descriptor.position_pixels[0].wrapping_add(advance as i16);
}

fn terminated(text: &[u8]) -> &[u8] {
    &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())]
}

#[cfg(test)]
#[path = "profile_draw_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "profile_draw_native_tests.rs"]
mod native_tests;
