//! The world tint's schedule, `0048E2E0`'s `0048E521..0048E6BA`. H14 `+1C`
//! is a period in seconds, `+30` a count and `+34` a list of that many H6
//! styles. Each tick the tint (`0053DED0`, published to `005474E0` by
//! `0043A370`) blends from the style the mission's timer `+2C` has reached
//! toward the next, over one period, and holds the last once the list is
//! spent. The timer gains each tick's whole milliseconds only while mission
//! `+13` is set: the world's constructor sets it, the load clears it for H14
//! flag `0x1000`, and a target with `+162` sets it as it dies, as Artoria's
//! and world 30's generators do. Infrared vision (world `+5C3`) replaces
//! the colour with `0055F15C`, `0053C678` and `0055F15D`: two bytes no
//! instruction writes (zero) and 127, a green.

use inf_formats::WorldBank;
use thiserror::Error;

/// `0048E693..0048E6AF`: infrared vision's colour.
pub const INFRARED_RGB: [u8; 3] = [0, 127, 0];
/// H14 header offsets of the period, the count and the list pointer.
const PERIOD_WORD: usize = 0x1c / 4;
const COUNT_WORD: usize = 0x30 / 4;
const LIST_FIELD: u32 = 0x34;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldTintSchedule {
    /// `+1C`, seconds a style lasts.
    pub period: i32,
    /// The `+30` H6 style WORDs `+34` points at.
    pub styles: Vec<i16>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorldTintError {
    #[error("H14 +34's style list is not relocated")]
    MissingList,
    #[error("H14 +30's {0} styles leave the list outside its payload")]
    List(i32),
    /// `0048E547`/`0048E612`/`0048E61D` divide by `+1C * 1000` and by it
    /// shifted right 8.
    #[error("H14 +1C {0} makes the tint divide by zero (0048E547)")]
    Period(i32),
    /// Mission `+2C`, which the world's constructor writes, unwritten.
    #[error("the world tint reads mission +2C, which is unwritten")]
    Timer,
    /// `0048E556`: the list read before its start or past its end.
    #[error("the tint's style index {0} is outside H14 +34's list")]
    Index(i32),
    /// `0048E55D..0048E58A`: a style outside the H6 pool leaves NULL, which
    /// `0048E60A` reads.
    #[error("the tint's H6 style {0} does not resolve (0048E60A reads NULL)")]
    Style(i16),
}

impl WorldTintSchedule {
    /// `+1C`, `+30` and the relocated `+34` of the world record at
    /// `allocation_offset` in `bank`, whose bytes are `source`.
    pub fn from_world(
        bank: &WorldBank,
        source: &[u8],
        allocation_offset: u32,
        header_words: &[u32; 14],
    ) -> Result<Self, WorldTintError> {
        let period = header_words[PERIOD_WORD] as i32;
        let count = header_words[COUNT_WORD] as i32;
        let field = allocation_offset + LIST_FIELD;
        let target = bank
            .relocations
            .iter()
            .find(|relocation| relocation.field_offset == field)
            .ok_or(WorldTintError::MissingList)?
            .target_offset as usize;
        let start = bank.main_payload.start + target;
        let styles = (0..count.max(0) as usize)
            .map(|index| {
                source
                    .get(start + 2 * index..start + 2 * index + 2)
                    .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
                    .ok_or(WorldTintError::List(count))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { period, styles })
    }

    /// The tint for the mission's timer `timer` (milliseconds): `previous`
    /// is `0053DED0` as the last tick left it, whose high byte a blend keeps;
    /// `style` resolves an H6 style's DWORD.
    pub fn tint<E: From<WorldTintError>>(
        &self,
        timer: i32,
        previous: u32,
        infrared: bool,
        mut style: impl FnMut(i16) -> Result<Option<u32>, E>,
    ) -> Result<u32, E> {
        let period = self.period.wrapping_mul(1000);
        if period >> 8 == 0 {
            return Err(WorldTintError::Period(self.period).into());
        }
        let count = self.styles.len() as i32;
        let reached = timer.wrapping_div(period);
        let clamp = |index: i32| if count - 1 >= index { index } else { count - 1 };
        let mut lookup = |index: i32| -> Result<u32, E> {
            let slot = usize::try_from(index)
                .ok()
                .and_then(|index| self.styles.get(index))
                .copied()
                .ok_or(WorldTintError::Index(index))?;
            Ok(style(slot)?.ok_or(WorldTintError::Style(slot))?)
        };
        let from = lookup(clamp(reached))?;
        let to = lookup(clamp(reached.wrapping_add(1)))?;
        let mut tint = if timer < self.period.wrapping_mul(count).wrapping_mul(1000) {
            let fraction = (timer.wrapping_rem(period) << 7).wrapping_div(period >> 8);
            let mut bytes = previous.to_le_bytes();
            let (from, to) = (from.to_le_bytes(), to.to_le_bytes());
            for channel in 0..3 {
                let step = (i32::from(to[channel]) - i32::from(from[channel]))
                    .wrapping_mul(fraction)
                    >> 15;
                bytes[channel] = from[channel].wrapping_add(step as u8);
            }
            u32::from_le_bytes(bytes)
        } else {
            to
        };
        if infrared {
            let mut bytes = tint.to_le_bytes();
            bytes[..3].copy_from_slice(&INFRARED_RGB);
            tint = u32::from_le_bytes(bytes);
        }
        Ok(tint)
    }
}

/// `0048E521..0048E6BA` over a loaded world: with mission `+13` set the
/// timer `+2C` gains the tick's whole milliseconds, then the tint follows the
/// schedule, green under infrared vision (`infrared`, the world manager's
/// `+5C3`). A schedule retail would read past is a stop, logged once; the
/// tint keeps its last value.
pub(crate) fn tick_world_tint(
    world: &mut crate::world_backend::WorldSession,
    millis: u32,
    infrared: bool,
    residency: &crate::ResidentRuntime,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STOP_LOGGED: AtomicBool = AtomicBool::new(false);
    let result = (|| -> Result<u32, WorldTintError> {
        let timer = world.mission.dword_2c.ok_or(WorldTintError::Timer)?;
        let timer = if world.mission.byte_13.unwrap_or(0) != 0 {
            let timer = timer.wrapping_add(millis);
            world.mission.dword_2c = Some(timer);
            timer
        } else {
            timer
        };
        let resources = crate::render_resources::RenderResources::new(residency);
        world
            .tint_schedule
            .tint(timer as i32, world.scheduled_tint, infrared, |slot| {
                Ok(style_word(&resources, slot))
            })
    })();
    match result {
        Ok(tint) => {
            world.scheduled_tint = tint;
            world.presentation.tint_word = tint;
        }
        Err(error) => {
            if !STOP_LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!("World tint stopped: {error}; the tint keeps its last value");
            }
        }
    }
}

/// H6 style `slot`'s DWORD, none when it does not resolve.
fn style_word(resources: &crate::render_resources::RenderResources, slot: i16) -> Option<u32> {
    let request = crate::render_resources::RenderResourceRequest {
        kind: crate::render_resources::RenderResourceKind::Style,
        argument: i32::from(slot),
    };
    match resources.resolve(request).map(|resource| resource.data) {
        Ok(crate::render_resources::RenderResourceData::Style(word)) => Some(word),
        _ => None,
    }
}

/// The world's tint on its schedule now, as it is without infrared
/// vision: red, green and blue.
pub(crate) fn tint_now(
    world: &crate::world_backend::WorldSession,
    residency: &crate::ResidentRuntime,
) -> Option<[u8; 3]> {
    let resources = crate::render_resources::RenderResources::new(residency);
    let timer = world.mission.dword_2c? as i32;
    let word = world
        .tint_schedule
        .tint(timer, world.scheduled_tint, false, |slot| {
            Ok::<_, WorldTintError>(style_word(&resources, slot))
        })
        .ok()?;
    let [red, green, blue, _] = word.to_le_bytes();
    Some([red, green, blue])
}

/// How lit the world is on its schedule, for the port's ambience: the
/// tint's luminance from its darkest style's (0) to its brightest's (1), as
/// it is without infrared vision. None for a schedule of one light, or one
/// that does not resolve.
pub(crate) fn light(
    world: &crate::world_backend::WorldSession,
    residency: &crate::ResidentRuntime,
) -> Option<f32> {
    let resources = crate::render_resources::RenderResources::new(residency);
    let luminance = |word: u32| {
        let [red, green, blue, _] = word.to_le_bytes();
        0.2126 * f32::from(red) + 0.7152 * f32::from(green) + 0.0722 * f32::from(blue)
    };
    let (mut darkest, mut brightest) = (f32::MAX, f32::MIN);
    for &slot in &world.tint_schedule.styles {
        let level = luminance(style_word(&resources, slot)?);
        (darkest, brightest) = (darkest.min(level), brightest.max(level));
    }
    if brightest - darkest < 1.0 {
        return None;
    }
    let timer = world.mission.dword_2c? as i32;
    let tint = world
        .tint_schedule
        .tint(timer, world.scheduled_tint, false, |slot| {
            Ok::<_, WorldTintError>(style_word(&resources, slot))
        })
        .ok()?;
    Some(((luminance(tint) - darkest) / (brightest - darkest)).clamp(0.0, 1.0))
}

#[cfg(test)]
#[path = "world_tint_tests.rs"]
mod tests;
