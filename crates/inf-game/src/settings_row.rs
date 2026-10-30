//! Settings value rows: draw 00453AE0 and adjust 00453B80.
//!
//! A value row shows a fixed H0 label on the left and the current setting's
//! string on the right. The row carries an H0 base ID and a DWORD-cell
//! address; the low WORD of that cell selects the right string as
//! `base + value + 1` with native 16-bit wrapping. Adjust commands mutate the
//! cell: 2 decrements with a zero floor, 3 increments clamped to the row's
//! maximum, and 4 increments wrapping to zero. Other commands are a no-op.
//! Cell addresses resolve to live settings words through the frontend host;
//! this owner only does arithmetic and text composition.
//!
//! Identity rows 004639D0/00463A30 share the left/right composition with a
//! fixed label ID and caller-resolved text (the net-name and initials caches)
//! instead of a pool-selected value.
//!
//! Sound slider rows 00453D50 share it too: a zero cell selects the H0 string
//! after the label, while any other value draws the generated 00453C90 glyph
//! slider. Their actions 00463AC0/00463B00 adjust through 00453B80 and then
//! run a fixed audio tail named by [`VolumeChannel`].

use thiserror::Error;

use crate::frontend_settings::{setting_word_index, SETTINGS_WORDS};
use crate::menu::{MenuCommand, MenuRowDrawRequest};
use crate::menu_text::{
    draw_row_text, MenuRowTextRequest, MenuTextError, MenuTextHost, MenuTextLayoutError,
    MenuTextPlacement, MenuTextResources,
};

/// H0 string pool bound behind a value row. The native count at 0055EE58 is
/// the pool's +08 allocation count; indices use the sign-extended low word.
pub trait SettingsRowResources: MenuTextResources {
    fn string_count(&self) -> Result<usize, Self::Error>;
}

#[derive(Debug, Error)]
pub enum SettingsRowError<R, H> {
    #[error("settings row text resource: {0}")]
    Resource(R),
    #[error("settings row text backend: {0}")]
    Host(H),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error(transparent)]
    Side(#[from] RowSideError),
}

/// Native reads the port replaces with explicit errors while resolving the
/// two sides of a row's single 00453810 call.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum RowSideError {
    #[error("settings row string index {index} addresses one past the H0 pool")]
    PoolBoundary { index: u32 },
    #[error("slider width {width} does not fit 00453C90's 64-byte stack buffer")]
    SliderWidth { width: i32 },
    #[error("slider fill {value} overwrites the terminator of a {width}-glyph slider")]
    SliderOverrun { value: u32, width: usize },
}

/// Right side of one row: a pool-selected H0 string or generated glyphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowValueText {
    /// `None` is the native null pointer, which draws nothing.
    Pool(Option<i32>),
    /// 00453C90 slider glyph bytes, without the terminator.
    Slider(Vec<u8>),
}

/// Both sides of a row, resolved before its single native text call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowSides {
    pub label: Option<i32>,
    pub value: RowValueText,
}

/// The audio consumer a Sound row runs after its 00453B80 adjust. Each tail
/// re-reads its own fixed settings word, never the row's cell argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeChannel {
    /// 00463AC0: word +00, nibble-expanded into the 004F4780 FX master, which
    /// re-attenuates every live voice.
    Effects,
    /// 00463B00: word +04 through the 00487370 music resume, then 004F7D80
    /// AUX volume at the linear level computed before that resume.
    Music,
}

/// 00453C90 builds the slider in a 64-byte stack buffer.
const SLIDER_BUFFER_BYTES: usize = 0x40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdjustOutcome {
    Adjusted,
    /// Commands other than 2/3/4 fall through every native branch.
    Ignored,
}

/// 00453AE0 right-string selection: the cell's low WORD plus the H0 base with
/// 16-bit wrapping, then plus one. Upper DWORD bits never participate: every
/// native use is a 16-bit read of the wrapped sum.
pub fn resolve_value_id(base: i32, value: u32) -> u32 {
    (u32::from((value as u16).wrapping_add(base as u16))).wrapping_add(1) & 0xFFFF
}

/// One H0 side lookup. Every native row consumer tests the argument's signed
/// low WORD (`test si,si`) and indexes with its sign extension, so upper
/// DWORD bits never select a string. Negative or above-count indices render
/// null; addressing exactly one past the pool is an explicit error instead of
/// the native overread.
pub fn pool_side(word: u32, count: usize) -> Result<Option<i32>, RowSideError> {
    let index = i32::from(word as u16 as i16);
    if index < 0 || index as usize > count {
        return Ok(None);
    }
    if index as usize == count {
        return Err(RowSideError::PoolBoundary {
            index: index as u32,
        });
    }
    Ok(Some(index))
}

/// 00453AE0 sides: the value string resolves first (EDI), then the label.
pub fn settings_row_sides(base: u32, value: u32, count: usize) -> Result<RowSides, RowSideError> {
    let value = RowValueText::Pool(pool_side(resolve_value_id(base as i32, value), count)?);
    let label = pool_side(base, count)?;
    Ok(RowSides { label, value })
}

/// 00453D50 sides. Only a zero cell DWORD selects `H0[low16(argument + 1)]`;
/// every other value, negative included, draws the slider. The label is the
/// argument's low word, whose high word carries the slider width.
pub fn volume_row_sides(argument: u32, value: u32, count: usize) -> Result<RowSides, RowSideError> {
    let value = if value == 0 {
        RowValueText::Pool(pool_side(argument.wrapping_add(1), count)?)
    } else {
        RowValueText::Slider(volume_slider(argument, value)?)
    };
    let label = pool_side(argument, count)?;
    Ok(RowSides { label, value })
}

/// 00453C90 slider glyphs. Width is the argument's arithmetic high word:
/// above one it spans `1B`, `1C` fill and `1D`; otherwise it is a lone `1D`.
/// A positive cell DWORD then overwrites that many leading bytes with `1E`,
/// without a width check. A template whose terminator leaves the buffer, or
/// a fill that overwrites the terminator, is an explicit error instead of the
/// native stack overrun or overread.
pub fn volume_slider(argument: u32, value: u32) -> Result<Vec<u8>, RowSideError> {
    let high = (argument as i32) >> 16;
    let width = if high > 1 { high as usize } else { 1 };
    if width >= SLIDER_BUFFER_BYTES {
        return Err(RowSideError::SliderWidth { width: high });
    }
    let mut glyphs = vec![0x1C; width];
    glyphs[0] = 0x1B;
    glyphs[width - 1] = 0x1D;
    if (value as i32) > 0 {
        let fill = value as usize;
        if fill > width {
            return Err(RowSideError::SliderOverrun { value, width });
        }
        glyphs[..fill].fill(0x1E);
    }
    Ok(glyphs)
}

/// The highest value the row at settings cell `address` steps to, where the
/// row's own `max` (retail's) is: Camera Style (`+6C`) goes on to the port's
/// Remastered.
pub fn row_most(address: u32, max: u32) -> u32 {
    if address
        == crate::frontend_settings::SETTINGS_STATE_ADDRESS
            + 4 * crate::camera_clearance::CAMERA_STYLE_CELL as u32
    {
        max.max(crate::camera_clearance::CAMERA_STYLE_REMASTERED)
    } else {
        max
    }
}

/// 00453B80 cell arithmetic over native DWORDs with signed comparisons.
/// 2 decrements with a zero floor, 3 increments clamped to `max`, and 4
/// increments wrapping to zero once past `max`.
pub fn adjust_value(command: MenuCommand, max: u32, value: &mut u32) -> AdjustOutcome {
    match command {
        MenuCommand::AdjustPrevious => {
            let next = value.wrapping_sub(1);
            *value = if (next as i32) < 0 { 0 } else { next };
            AdjustOutcome::Adjusted
        }
        MenuCommand::AdjustNext => {
            let next = value.wrapping_add(1);
            *value = next;
            if (next as i32) > (max as i32) {
                *value = max;
            }
            AdjustOutcome::Adjusted
        }
        MenuCommand::Activate => {
            let next = value.wrapping_add(1);
            *value = next;
            if (next as i32) > (max as i32) {
                *value = 0;
            }
            AdjustOutcome::Adjusted
        }
        _ => AdjustOutcome::Ignored,
    }
}

/// 00453AE0 row composition: the fixed H0 label draws left, then the resolved
/// value string draws right through the shared descriptor, matching the
/// native single 00453810 call's sequencing. The caller resolves the cell
/// address to its live value; settings layout stays with the frontend host.
/// `00453AE0`, a settings value row.
pub const VALUE_ROW: u32 = 0x0045_3ae0;
/// `00453D50`, a Sound slider.
pub const VOLUME_ROW: u32 = 0x0045_3d50;

/// What a value row or slider draws from the live settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsRowContent {
    Value(u32),
    Volume(u32),
    /// Port text for the Resolution (`+0C`) and Display (`+40`) rows, whose
    /// exact size or Borderless has no authored string: the H0 label and
    /// the text.
    Text(i32, Vec<u8>),
    /// A row the port relabels as well: Filtering (`+3C`, retail's Bilinear
    /// Filtering), its label and value in the settings' language.
    Port(Vec<u8>, Vec<u8>),
}

/// A value row's or slider's content: its cell's live settings word, or the
/// port's display text. None for other callbacks; `Err` names a cell
/// outside the settings state.
pub fn settings_row_content(
    settings: &crate::frontend_settings::FrontendSettings,
    callback: u32,
    arguments: [u32; 2],
) -> Option<Result<SettingsRowContent, u32>> {
    let cell = |address: u32| settings.setting_word(address).ok_or(address);
    Some(match callback {
        VALUE_ROW => {
            let display = settings.display_selection();
            let language = settings.residency_language();
            let offset =
                arguments[1].wrapping_sub(crate::frontend_settings::SETTINGS_STATE_ADDRESS);
            if let Some(row) = crate::frontend_video::ModernRow::from_address(arguments[1]) {
                let modern = settings.modern();
                return Some(Ok(SettingsRowContent::Port(
                    row.label(language).to_vec(),
                    row.value_text(&modern, settings.modern_support(), language),
                )));
            }
            if offset == 0x3c {
                // The Filtering this renderer draws, a saved Vulkan choice
                // under OpenGL showing as Bilinear.
                return Some(cell(arguments[1]).map(|level| {
                    SettingsRowContent::Port(
                        crate::port_text::PortText::Filtering
                            .text(language)
                            .to_vec(),
                        crate::frontend_video::filtering_text(
                            level.min(settings.filtering_most()),
                            language,
                        ),
                    )
                }));
            }
            let port_text = match offset {
                0x0c => display.resolution_text(settings.desktop()),
                0x40 => display.mode_text(language).map(<[u8]>::to_vec),
                // Camera Style's Remastered, past retail's Normal and
                // Cinematic.
                0x6c if settings.camera_style()
                    == crate::camera_clearance::CameraStyle::Remastered =>
                {
                    Some(
                        crate::port_text::PortText::Remastered
                            .text(language)
                            .to_vec(),
                    )
                }
                _ => None,
            };
            match port_text {
                Some(text) => Ok(SettingsRowContent::Text(arguments[0] as i32, text)),
                None => cell(arguments[1]).map(SettingsRowContent::Value),
            }
        }
        VOLUME_ROW if arguments[1] == crate::frontend_sound::AMBIENCE_CELL => {
            let (label, value) = crate::frontend_sound::ambience_texts(
                arguments[0],
                settings.modern().ambience,
                settings.ambience_sounds(),
                settings.residency_language(),
            );
            Ok(SettingsRowContent::Port(label, value))
        }
        VOLUME_ROW if arguments[1] == crate::frontend_sound::HEADPHONES_CELL => {
            let (label, value) = crate::frontend_sound::headphones_texts(
                settings.modern().headphones,
                settings.ambience_sounds(),
                settings.residency_language(),
            );
            Ok(SettingsRowContent::Port(label, value))
        }
        VOLUME_ROW => cell(arguments[1]).map(SettingsRowContent::Volume),
        _ => return None,
    })
}

pub fn draw_settings_row<R: SettingsRowResources, H: MenuTextHost>(
    request: &mut MenuRowDrawRequest,
    value: u32,
    resources: &R,
    host: &mut H,
) -> Result<(), SettingsRowError<R::Error, H::Error>> {
    let count = resources
        .string_count()
        .map_err(SettingsRowError::Resource)?;
    let sides = settings_row_sides(request.arguments[0], value, count)?;
    draw_row_sides(request, &sides, resources, host)
}

/// 00453D50 slider row: the H0 label draws left and, through the same
/// descriptor, either the zero string or the generated slider draws right.
/// The caller resolves the cell address to its live settings DWORD.
pub fn draw_volume_row<R: SettingsRowResources, H: MenuTextHost>(
    request: &mut MenuRowDrawRequest,
    value: u32,
    resources: &R,
    host: &mut H,
) -> Result<(), SettingsRowError<R::Error, H::Error>> {
    let count = resources
        .string_count()
        .map_err(SettingsRowError::Resource)?;
    let sides = volume_row_sides(request.arguments[0], value, count)?;
    draw_row_sides(request, &sides, resources, host)
}

/// Identity rows 004639D0/00463A30: a fixed H0 label on the left and a
/// caller-resolved byte string on the right. Unlike value rows the right
/// side is literal text rather than a pool selection, so it always draws
/// (possibly empty); the label keeps the pool bound rule with its one-past
/// explicit error. Both resolutions precede either text call.
pub fn draw_identity_row<R: SettingsRowResources, H: MenuTextHost>(
    request: &mut MenuRowDrawRequest,
    label: i32,
    text: &[u8],
    resources: &R,
    host: &mut H,
) -> Result<(), SettingsRowError<R::Error, H::Error>> {
    let count = resources
        .string_count()
        .map_err(SettingsRowError::Resource)?;
    let label = pool_side(label as u32, count)?
        .map(|id| resources.string(id))
        .transpose()
        .map_err(SettingsRowError::Resource)?;
    draw_text_pair(request, label, Some(text), resources, host)
}

/// A row whose label and value are both the port's text, drawn as a value
/// row draws its two sides.
pub fn draw_port_row<R: SettingsRowResources, H: MenuTextHost>(
    request: &mut MenuRowDrawRequest,
    label: &[u8],
    value: &[u8],
    resources: &R,
    host: &mut H,
) -> Result<(), SettingsRowError<R::Error, H::Error>> {
    draw_text_pair(request, Some(label), Some(value), resources, host)
}

/// Resolve both texts before either text call, matching the single native
/// 00453810 invocation: a later failure submits no glyphs. The value side
/// (EDI) resolves before the label (EAX), as every row consumer loads them.
fn draw_row_sides<R: SettingsRowResources, H: MenuTextHost>(
    request: &mut MenuRowDrawRequest,
    sides: &RowSides,
    resources: &R,
    host: &mut H,
) -> Result<(), SettingsRowError<R::Error, H::Error>> {
    let value = match &sides.value {
        RowValueText::Pool(id) => id
            .map(|id| resources.string(id))
            .transpose()
            .map_err(SettingsRowError::Resource)?,
        RowValueText::Slider(glyphs) => Some(glyphs.as_slice()),
    };
    let label = sides
        .label
        .map(|id| resources.string(id))
        .transpose()
        .map_err(SettingsRowError::Resource)?;
    draw_text_pair(request, label, value, resources, host)
}

/// The left then right halves of one 00453810 call through the shared
/// descriptor. A null side is skipped; two null sides draw nothing.
fn draw_text_pair<R: SettingsRowResources, H: MenuTextHost>(
    request: &mut MenuRowDrawRequest,
    left: Option<&[u8]>,
    right: Option<&[u8]>,
    resources: &R,
    host: &mut H,
) -> Result<(), SettingsRowError<R::Error, H::Error>> {
    for (text, placement) in [
        (left, MenuTextPlacement::Left),
        (right, MenuTextPlacement::Right),
    ] {
        let Some(text) = text else { continue };
        draw_row_text(
            MenuRowTextRequest {
                descriptor: &mut request.descriptor,
                text,
                placement,
                context_origin_pixels: request.context_origin_pixels,
            },
            resources,
            host,
        )
        .map_err(map_text_error)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayAdjustOutcome {
    /// The cell value survived the adjust unchanged: no variant work follows,
    /// matching the native early return.
    Unchanged,
    /// Changed, but the resolved variant needs no backend reset.
    AdjustedNoReinit,
    /// Changed with a backend reset for the resolved variant.
    AdjustedReinitialized,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DisplayAdjustError<E> {
    #[error("display adjust cell {address:#010x} is outside the 34-word settings state")]
    Cell { address: u32 },
    #[error("display backend reset failed")]
    Host(E),
}

/// 00462F40 variant handling after a changed adjust, over signed DWORD
/// comparisons: a nonpositive variant forces word `+0x0C` to 1 without any
/// reset; above 4 it clamps to 4; otherwise the backend reinitializes with
/// the variant. Returns the stored variant word and the reset target.
pub fn resolve_display_reinit(variant: u32) -> (u32, Option<u32>) {
    let signed = variant as i32;
    if signed <= 0 {
        (1, None)
    } else if signed >= 5 {
        (4, Some(4))
    } else {
        (variant, Some(variant))
    }
}

/// 00462F40 display-aware adjust: a plain 00453B80 adjust first, then variant
/// handling when the cell changed. A failed backend reset restores the row
/// cell like the native setjmp handler; the variant-word stores persist.
pub fn adjust_display_setting<E>(
    words: &mut [u32; SETTINGS_WORDS],
    address: u32,
    command: MenuCommand,
    max: u32,
    mut reinit: impl FnMut(u32, &[u32; SETTINGS_WORDS]) -> Result<(), E>,
) -> Result<DisplayAdjustOutcome, DisplayAdjustError<E>> {
    let index = setting_word_index(address).ok_or(DisplayAdjustError::Cell { address })?;
    let old = words[index];
    adjust_value(command, max, &mut words[index]);
    if words[index] == old {
        return Ok(DisplayAdjustOutcome::Unchanged);
    }
    let (stored, target) = resolve_display_reinit(words[3]);
    words[3] = stored;
    match target {
        None => Ok(DisplayAdjustOutcome::AdjustedNoReinit),
        Some(variant) => reinit(variant, words)
            .map(|_| DisplayAdjustOutcome::AdjustedReinitialized)
            .map_err(|error| {
                words[index] = old;
                DisplayAdjustError::Host(error)
            }),
    }
}

fn map_text_error<R, H>(error: MenuTextError<R, H>) -> SettingsRowError<R, H> {
    match error {
        MenuTextError::Resource(error) => SettingsRowError::Resource(error),
        MenuTextError::Backend(error) => SettingsRowError::Host(error),
        MenuTextError::Layout(error) => SettingsRowError::Layout(error),
        MenuTextError::UnsupportedCallback(_) => {
            unreachable!("settings rows submit direct text, never a callback")
        }
    }
}

#[cfg(test)]
#[path = "settings_row_tests.rs"]
mod tests;
