//! 00452570 authored row execution and the base-controller 00453610 boundary.
//! Callback addresses remain dispatch identifiers. A host performs each actual
//! callback; its mutated descriptor Y supplies the native shared height result.

use crate::menu_basis::sine_q31;
use inf_formats::{FrontendMenuItem, FrontendMenuLayout};

use crate::{
    profile_menu::{ProfileMenu, ProfileMenuState, ProfileTimer},
    profile_store::ProfileStore,
};

use super::{
    MenuBasis, MenuError, MenuOwner, MenuSource, MenuStack, MenuState, MenuStateError,
    MenuVisibility,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDrawColors {
    /// 005483C8, first endpoint of the selected row's 400ms sawtooth.
    pub selected_start: u32,
    /// 005483F8, ordinary rows and item flag 0x10.
    pub ordinary: u32,
    /// 005483CC, second endpoint and item flag 0x40.
    pub selected_end: u32,
}

/// Native color bytes, preserving whether the fourth byte has a known writer.
/// The fourth byte is not an inferred alpha value. At 00452A31..00452A97 the
/// animated selected branch writes only RGB; 00452AB9 copies a complete word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuColor {
    pub rgb: [u8; 3],
    pub high_byte: Option<u8>,
}

impl MenuColor {
    pub const fn from_word(word: u32) -> Self {
        let bytes = word.to_le_bytes();
        Self {
            rgb: [bytes[0], bytes[1], bytes[2]],
            high_byte: Some(bytes[3]),
        }
    }

    /// Only the RGB bytes are established by this write.
    pub const fn from_rgb(rgb: [u8; 3]) -> Self {
        Self {
            rgb,
            high_byte: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MenuDrawInput<'a> {
    /// 00495900/10 return the actual surface dimensions, in pixels.
    pub surface_pixels: [i32; 2],
    pub visibility: MenuVisibility,
    pub sine_quarter: &'a [u16; 4096],
}

/// 00453610 -> 0043B4A0/0043B5D0. The host copies its original render-context
/// template, copies projection globals 00546F20/24 to context+38/+3C, replaces
/// context+0 and +6C with the caller's values, and applies these overrides.
/// Surface width/height occupy +40/+44; half the width occupies +48 before the
/// row walk replaces the origin. Unnamed context fields retain offsets until
/// their rendering consumers establish units; they are not guessed lens APIs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuControllerDrawSetup {
    pub surface_pixels: [i32; 2],
    pub context_4c: i32,
    pub context_64: i32,
    pub context_68: i32,
    pub light_direction: [f32; 3],
    pub light_color: [u8; 3],
    pub ambient_color: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuDrawPlacement {
    Linear,
    /// Native integer table/Q31 positions, not floating-point sin/cos.
    Circular {
        angle: i32,
    },
}

/// The initialized portion of the native callback descriptor at 00452CD9.
/// The color records the branch-specific fourth-byte provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuRowDescriptor {
    pub position_pixels: [i16; 2],
    pub extent_pixels: [i16; 2],
    /// +08: FFFF without layout flag2, otherwise min(item_age*50, 7FFF).
    pub reveal: u16,
    pub flags: u32,
    pub age_millis: i32,
    pub color: MenuColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuRowDrawRequest {
    pub menu_source: MenuSource,
    pub layout_index: usize,
    pub item_index: usize,
    pub callback_address: u32,
    pub arguments: [u32; 2],
    /// Context+48/+4C. These are full i32 pixels, unlike descriptor i16 fields.
    /// The backend also republishes context+38..+4F at 0055F22C before dispatch.
    pub context_origin_pixels: [i32; 2],
    pub placement: MenuDrawPlacement,
    pub descriptor: MenuRowDescriptor,
}

pub trait MenuDrawHost {
    type Error;

    /// Perform actual controller context/light setup. There is no default
    /// implementation: an unavailable backend must return an error.
    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error>;
    /// Controller virtual +0C, after the setup and before the current menu,
    /// at any fade. The base controller's 004537D0 is a RET; the player
    /// message boxes' 00413F30 draws their panel frame here.
    fn draw_controller_frame(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    /// Publish all ten words to the native menu-basis domain 005483D0..3F4.
    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error>;
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error>;
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error>;
    /// Read current colors, so callback changes affect later rows.
    fn colors(&self) -> MenuDrawColors;
    /// Dispatch the actual callback, including text/model operations. Modify
    /// descriptor.position_pixels[1] exactly as the callback does. Shared item
    /// and layout records are not borrowed during this call; age resets and
    /// other callback mutations are visible to subsequent native reads.
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error>;

    /// 004483B0 resolves font 0/style 3 before checking the derived draw gate.
    fn begin_profile_draw(&mut self) -> Result<(), MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook {
            hook: "draw resources",
        }
        .into())
    }

    fn profile_store(&mut self) -> Result<&mut ProfileStore, MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook { hook: "save owner" }.into())
    }

    fn draw_profile_row(
        &mut self,
        _request: &mut MenuRowDrawRequest,
        _base: &mut MenuState,
        _state: &mut ProfileMenuState,
    ) -> Result<(), MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook { hook: "row draw" }.into())
    }

    fn draw_profile_status(
        &mut self,
        _timer: ProfileTimer,
        _surface_pixels: [i32; 2],
    ) -> Result<(), MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook {
            hook: "status draw",
        }
        .into())
    }

    /// `00453810` with the row's own text instead of an H0 string: the
    /// network lists' names (`0045F700`).
    fn draw_text_row(
        &mut self,
        _request: &mut MenuRowDrawRequest,
        _text: &[u8],
    ) -> Result<(), MenuError<Self::Error>> {
        Err(MenuStateError::UnsupportedProfileHook { hook: "text row" }.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDrawOutcome {
    pub rows: usize,
    pub menu_drawn: bool,
}

impl MenuStack {
    /// The controller draw 00453610. Base controller 0052561C: virtual+0C at
    /// 004537D0 is a proven RET; derived frames come from the host. The
    /// current menu draws only at full fade. Successful setup/row execution
    /// sets +25 even during partial fade; failure never manufactures that flag.
    pub fn draw<H: MenuDrawHost>(
        &mut self,
        input: MenuDrawInput<'_>,
        host: &mut H,
    ) -> Result<MenuDrawOutcome, MenuError<H::Error>> {
        self.draw_catching(input, host, |_| None)
    }

    /// The enclosing frontend frame alone consumes native 0x301. Its longjmp
    /// skips clip restoration; ordinary host/state failures retain containment.
    pub(crate) fn draw_catching<H: MenuDrawHost>(
        &mut self,
        input: MenuDrawInput<'_>,
        host: &mut H,
        native_error_code: impl Fn(&H::Error) -> Option<u32>,
    ) -> Result<MenuDrawOutcome, MenuError<H::Error>> {
        host.begin_controller(MenuControllerDrawSetup {
            surface_pixels: input.surface_pixels,
            context_4c: 0,
            context_64: 0x800,
            context_68: 0x1000,
            light_direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            light_color: [128; 3],
            ambient_color: [32; 3],
        })
        .map_err(MenuError::Host)?;
        host.draw_controller_frame().map_err(MenuError::Host)?;
        let viewport = self.spec.viewport;
        let mut outcome = MenuDrawOutcome {
            rows: 0,
            menu_drawn: false,
        };
        if self.fade == 65535 {
            if let Some(owner) = self.current_owner_mut() {
                outcome.rows = match owner {
                    MenuOwner::Authored(menu) => {
                        draw_menu(menu, viewport, input, host, native_error_code)?
                    }
                    MenuOwner::Profile(profile) => {
                        draw_profile(profile, viewport, input, host, native_error_code)?
                    }
                    MenuOwner::Network(network) => {
                        // The names 0045F700 draws, by their row's argument.
                        let texts: Vec<(u32, String)> = network
                            .base()
                            .data()
                            .records()
                            .items
                            .iter()
                            .filter(|item| item.draw_callback == crate::network_menu::NAME_ROW)
                            .map(|item| {
                                (
                                    item.draw_arguments[0],
                                    network.row_text(item.draw_arguments[0]),
                                )
                            })
                            .collect();
                        let mut adapter = NetworkRowHost { host, texts };
                        draw_menu(network.base_mut(), viewport, input, &mut adapter, |error| {
                            match error {
                                MenuError::Host(error) => native_error_code(error),
                                MenuError::State(_) | MenuError::ProfileStore(_) => None,
                            }
                        })
                        .map_err(flatten_profile_error)?
                    }
                };
                outcome.menu_drawn = true;
            }
        }
        self.drawn = true;
        Ok(outcome)
    }
}

fn draw_profile<H: MenuDrawHost>(
    profile: &mut ProfileMenu,
    viewport: [i16; 4],
    input: MenuDrawInput<'_>,
    host: &mut H,
    native_error_code: impl Fn(&H::Error) -> Option<u32>,
) -> Result<usize, MenuError<H::Error>> {
    host.begin_profile_draw()?;
    let gate = profile.prepare_draw(host.profile_store()?)?;
    let rows = if gate.draw_rows {
        let (base, state) = profile.split_mut();
        let mut adapter = ProfileRowHost { host, state };
        draw_menu(base, viewport, input, &mut adapter, |error| match error {
            MenuError::Host(error) => native_error_code(error),
            MenuError::State(_) | MenuError::ProfileStore(_) => None,
        })
        .map_err(flatten_profile_error)?
    } else {
        0
    };
    if let Some(timer) = gate.timer {
        host.draw_profile_status(timer, input.surface_pixels)?;
    }
    profile.finish_draw();
    Ok(rows)
}

/// The base row walk retains its clipping/height behavior while the vtable's
/// derived callbacks receive the live profile state, without a table alias.
struct ProfileRowHost<'a, H> {
    host: &'a mut H,
    state: &'a mut ProfileMenuState,
}

impl<H: MenuDrawHost> MenuDrawHost for ProfileRowHost<'_, H> {
    type Error = MenuError<H::Error>;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.host.begin_controller(setup).map_err(MenuError::Host)
    }
    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.host.publish_menu_basis(basis).map_err(MenuError::Host)
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        self.host.clip_rect().map_err(MenuError::Host)
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.host.set_clip_rect(clip).map_err(MenuError::Host)
    }
    fn colors(&self) -> MenuDrawColors {
        self.host.colors()
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        self.host.draw_profile_row(request, menu, self.state)
    }
}

/// The network lists' rows: their names through [`MenuDrawHost::draw_text_row`],
/// every other row through the host.
struct NetworkRowHost<'a, H> {
    host: &'a mut H,
    texts: Vec<(u32, String)>,
}

impl<H: MenuDrawHost> MenuDrawHost for NetworkRowHost<'_, H> {
    type Error = MenuError<H::Error>;

    fn begin_controller(&mut self, setup: MenuControllerDrawSetup) -> Result<(), Self::Error> {
        self.host.begin_controller(setup).map_err(MenuError::Host)
    }
    fn publish_menu_basis(&mut self, basis: MenuBasis) -> Result<(), Self::Error> {
        self.host.publish_menu_basis(basis).map_err(MenuError::Host)
    }
    fn clip_rect(&mut self) -> Result<[i16; 4], Self::Error> {
        self.host.clip_rect().map_err(MenuError::Host)
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.host.set_clip_rect(clip).map_err(MenuError::Host)
    }
    fn colors(&self) -> MenuDrawColors {
        self.host.colors()
    }
    fn draw_row(
        &mut self,
        request: &mut MenuRowDrawRequest,
        menu: &mut MenuState,
    ) -> Result<(), Self::Error> {
        if request.callback_address != crate::network_menu::NAME_ROW {
            return self.host.draw_row(request, menu).map_err(MenuError::Host);
        }
        let text = self
            .texts
            .iter()
            .find(|(index, _)| *index == request.arguments[0])
            .map(|(_, text)| text.clone())
            .unwrap_or_default();
        self.host.draw_text_row(request, text.as_bytes())
    }
}

fn flatten_profile_error<E>(error: MenuError<MenuError<E>>) -> MenuError<E> {
    match error {
        MenuError::State(error) => MenuError::State(error),
        MenuError::Host(error) => error,
        MenuError::ProfileStore(error) => MenuError::ProfileStore(error),
    }
}

#[derive(Clone, Copy)]
struct PixelViewport {
    origin: [i32; 2],
    size: [i32; 2],
}

impl PixelViewport {
    fn new(viewport: [i16; 4], surface: [i32; 2]) -> Self {
        Self {
            origin: [
                scale(i32::from(viewport[0]), surface[0]),
                scale(i32::from(viewport[1]), surface[1]),
            ],
            size: [
                scale(i32::from(viewport[2]) - i32::from(viewport[0]), surface[0]),
                scale(i32::from(viewport[3]) - i32::from(viewport[1]), surface[1]),
            ],
        }
    }
    fn point(self, x: i16, y: i16) -> [i32; 2] {
        [
            scale(i32::from(x), self.size[0]).wrapping_add(self.origin[0]),
            scale(i32::from(y), self.size[1]).wrapping_add(self.origin[1]),
        ]
    }
    fn extent(self, layout: FrontendMenuLayout) -> [i16; 2] {
        [
            scale(i32::from(layout.width), self.size[0]) as i16,
            scale(i32::from(layout.height), self.size[1]) as i16,
        ]
    }
}

fn scale(value: i32, pixels: i32) -> i32 {
    value.wrapping_mul(pixels) / 1024
}

fn layout(menu: &MenuState, index: usize) -> Result<FrontendMenuLayout, MenuStateError> {
    menu.data
        .records()
        .layouts
        .get(index)
        .copied()
        .ok_or(MenuStateError::LayoutOutOfBounds { index })
}

fn item(menu: &MenuState, index: i32) -> Result<(usize, FrontendMenuItem), MenuStateError> {
    let records = menu.data.records();
    let index = super::navigation::item_index(index, records.items.len())?;
    Ok((index, records.items[index]))
}

fn signed_div(value: i32, divisor: i32, field: &'static str) -> Result<i32, MenuStateError> {
    if divisor == 0 {
        return Err(MenuStateError::ZeroDrawDivisor { field });
    }
    value
        .checked_div(divisor)
        .ok_or(MenuStateError::DrawDivisionOverflow)
}

fn draw_menu<H: MenuDrawHost>(
    menu: &mut MenuState,
    viewport: [i16; 4],
    input: MenuDrawInput<'_>,
    host: &mut H,
    native_error_code: impl Fn(&H::Error) -> Option<u32>,
) -> Result<usize, MenuError<H::Error>> {
    let viewport = PixelViewport::new(viewport, input.surface_pixels);
    host.publish_menu_basis(menu.basis)
        .map_err(MenuError::Host)?;
    let saved_clip = host.clip_rect().map_err(MenuError::Host)?;
    let result = draw_layouts(menu, viewport, input, saved_clip, host);
    if matches!(&result, Err(MenuError::Host(error)) if native_error_code(error) == Some(0x301)) {
        return result;
    }
    // Restore on failures too. Keep the original error if both operations fail;
    // successful completion requires successful restoration.
    let restored = host.set_clip_rect(saved_clip).map_err(MenuError::Host);
    match result {
        Ok(rows) => {
            restored?;
            Ok(rows)
        }
        Err(error) => Err(error),
    }
}

fn initial_skip(
    menu: &MenuState,
    start: i32,
    layout: FrontendMenuLayout,
    mask: u32,
) -> Result<i32, MenuStateError> {
    let count = i32::from(layout.item_count);
    // Native Options (00539378) has eight logical items, but its second
    // layout counts thirteen starting at item one. Its read-only visibility
    // census therefore crosses into adjacent authored records. With no forced
    // scrolling and enough rows for the entire layout, EVERY possible census
    // result selects zero: visible <= item_count <= visible_rows. Preserve that
    // result without inventing ownership of the adjacent mutable records. The
    // subsequent native row walk still stops at the logical menu item count.
    if layout.flags & 0x10 == 0 && count <= i32::from(layout.visible_rows) {
        return Ok(0);
    }
    let mut visible = 0;
    for offset in 0..count {
        let index = start + offset;
        let flags = usize::try_from(index)
            .ok()
            .and_then(|index| menu.data.census_flags(index))
            .ok_or(MenuStateError::ItemOutOfBounds {
                index,
                count: menu.data.records().items.len(),
            })?;
        if flags & mask != 0 {
            visible += 1;
        }
    }
    if (layout.flags & 0x10 == 0 && visible <= i32::from(layout.visible_rows))
        || menu.selection < start
    {
        return Ok(0);
    }
    if menu.selection < start + count {
        return Ok(menu.scroll_start);
    }
    if layout.flags & 0x10 != 0 {
        return Ok(count - 1);
    }
    // Native walks backwards, accumulating unsigned height feedback.
    let mut skip = count;
    let mut height = 0_i32;
    loop {
        skip -= 1;
        height += i32::from(item(menu, start + skip)?.1.row_height);
        if height >= i32::from(layout.visible_rows) {
            return Ok(skip);
        }
    }
}

fn layout_clip(layout: FrontendMenuLayout, viewport: PixelViewport, saved: [i16; 4]) -> [i16; 4] {
    if layout.flags & 0x20 != 0 {
        return saved;
    }
    let mut left = viewport.point(layout.x, layout.y)[0] as i16;
    if layout.flags & 4 != 0 {
        left = left.wrapping_add(scale(-(i32::from(layout.width) / 2), viewport.size[0]) as i16);
    }
    let top = scale(
        i32::from(layout.y) - i32::from(layout.row_spacing),
        viewport.size[1],
    )
    .wrapping_add(viewport.origin[1]) as i16;
    let [width, height] = viewport.extent(layout);
    [
        left,
        top,
        left.wrapping_add(width),
        top.wrapping_add(height),
    ]
}

fn row_style(
    menu: &MenuState,
    index: i32,
    item: FrontendMenuItem,
    layout: FrontendMenuLayout,
    colors: MenuDrawColors,
) -> (u32, MenuColor) {
    let (mut flags, color) = if item.flags & 0x10 != 0 {
        (0, MenuColor::from_word(colors.ordinary))
    } else {
        let selected = index == menu.selection;
        let flags = u32::from(selected) | if layout.flags & 0x80 != 0 { 0x40 } else { 0 };
        let color = if item.flags & 0x48 == 0x48 {
            MenuColor::from_word(colors.selected_end)
        } else if selected {
            let phase = menu.age_millis % 400;
            let start = colors.selected_start.to_le_bytes();
            let end = colors.selected_end.to_le_bytes();
            MenuColor::from_rgb(std::array::from_fn(|component| {
                let start = i32::from(start[component]);
                let end = i32::from(end[component]);
                (start + (end - start) * phase / 400) as u8
            }))
        } else if item.flags & 0x40 != 0 {
            MenuColor::from_word(colors.selected_end)
        } else {
            MenuColor::from_word(colors.ordinary)
        };
        (flags, color)
    };
    if layout.flags & 4 != 0 {
        flags |= 2;
    }
    (flags, color)
}

fn circular_point(angle: i32, spacing: u16, table: &[u16; 4096]) -> [i16; 2] {
    let x = (i64::from(sine_q31(angle, table)) * i64::from(spacing)) >> 31;
    let y = (i64::from(sine_q31(angle.wrapping_add(0x4000), table))
        * i64::from(u32::from(spacing) * 3 / 2))
        >> 31;
    [x as i16, (y as i32).wrapping_neg() as i16]
}

#[derive(Clone, Copy)]
struct DrawFrame<'a> {
    viewport: PixelViewport,
    input: MenuDrawInput<'a>,
}

struct RowVisit {
    layout_index: usize,
    item_index: usize,
    layout: FrontendMenuLayout,
    entry: FrontendMenuItem,
    drawn: i32,
    count: i32,
}

/// Prepare exactly one callback, after any mutations from the preceding row.
/// The local position persists through the layout; circular rows replace it.
fn prepare_row(
    menu: &MenuState,
    frame: DrawFrame<'_>,
    visit: RowVisit,
    position: &mut [i16; 2],
    colors: MenuDrawColors,
) -> MenuRowDrawRequest {
    let RowVisit {
        layout_index,
        item_index,
        layout,
        entry,
        drawn,
        count,
    } = visit;
    let viewport = frame.viewport;
    let index = item_index as i32;
    let (mut flags, color) = row_style(menu, index, entry, layout, colors);
    let (context_origin_pixels, placement) = if layout.flags & 0x20 == 0 {
        (
            viewport.point(position[0], position[1]),
            MenuDrawPlacement::Linear,
        )
    } else {
        let angle = if entry.flags & 8 != 0 {
            0
        } else {
            i32::from(layout.scroll_animation)
                .wrapping_add(index.wrapping_sub(menu.selection).wrapping_mul(200))
                .wrapping_mul(65535 / (count * 200))
        };
        *position = circular_point(angle, layout.row_spacing, frame.input.sine_quarter);
        flags |= 0x10;
        (
            viewport.point(layout.x, layout.y),
            MenuDrawPlacement::Circular { angle },
        )
    };
    let (age_millis, reveal) = if layout.flags & 2 != 0 {
        if (layout.scroll_animation > 0 && drawn == 0)
            || (layout.scroll_animation < 0 && drawn >= i32::from(layout.visible_rows))
        {
            flags |= 0x20;
        }
        (
            i32::from(entry.age),
            (u32::from(entry.age) * 50).min(0x7fff) as u16,
        )
    } else {
        (menu.age_millis, 0xffff)
    };
    MenuRowDrawRequest {
        menu_source: menu.data.source(),
        layout_index,
        item_index,
        callback_address: entry.draw_callback,
        arguments: entry.draw_arguments,
        context_origin_pixels,
        placement,
        descriptor: MenuRowDescriptor {
            position_pixels: viewport
                .point(position[0], position[1])
                .map(|value| value as i16),
            extent_pixels: viewport.extent(layout),
            reveal,
            flags,
            age_millis,
            color,
        },
    }
}

/// 00452D76..00452DAF: callback Y is converted back through the signed pixel
/// viewport, then rounded in unsigned spacing units and narrowed to a WORD.
/// Spacing is read after dispatch; no old RefCell borrow survives the callback.
fn commit_row_height(
    menu: &mut MenuState,
    viewport: PixelViewport,
    request: MenuRowDrawRequest,
    authored_y: i16,
    original_item_index: usize,
    original_layout_index: usize,
) -> Result<i16, MenuStateError> {
    let spacing = i32::from(layout(menu, original_layout_index)?.row_spacing);
    let returned = i32::from(request.descriptor.position_pixels[1])
        .wrapping_sub(viewport.origin[1])
        .wrapping_shl(10);
    let row_units = signed_div(returned, viewport.size[1], "viewport height")?;
    let height = signed_div(
        row_units
            .wrapping_add(spacing >> 1)
            .wrapping_sub(i32::from(authored_y)),
        spacing,
        "row spacing",
    )?
    .wrapping_add(1) as u16;
    let mut records = menu.data.records_mut();
    let len = records.items.len();
    records
        .items
        .get_mut(original_item_index)
        .ok_or(MenuStateError::ItemOutOfBounds {
            index: original_item_index as i32,
            count: len,
        })?
        .row_height = height;
    Ok(authored_y.wrapping_add((spacing as u16).wrapping_mul(height) as i16))
}

fn draw_layouts<H: MenuDrawHost>(
    menu: &mut MenuState,
    viewport: PixelViewport,
    input: MenuDrawInput<'_>,
    saved_clip: [i16; 4],
    host: &mut H,
) -> Result<usize, MenuError<H::Error>> {
    let mut start = 0_i32;
    let mut layout_index = 0;
    let mut rows = 0;
    loop {
        let Some(initial) = menu.data.records().layouts.get(layout_index).copied() else {
            break;
        };
        if initial.flags == 0 {
            break;
        }
        let mut skip = initial_skip(menu, start, initial, input.visibility.mask())?;
        let mut position = [initial.x, initial.y];
        host.set_clip_rect(layout_clip(initial, viewport, saved_clip))
            .map_err(MenuError::Host)?;
        let current = layout(menu, layout_index)?;
        position[1] = position[1].wrapping_add(
            (i32::from(current.row_spacing) * i32::from(current.scroll_animation) / 200) as i16,
        );
        let mut limit = i32::from(current.visible_rows);
        if current.flags & 0x20 == 0 && current.scroll_animation != 0 {
            limit += 1;
            if current.scroll_animation > 0 {
                skip -= 1;
                let preceding = if skip < 0 {
                    i32::from(current.item_count) - 1
                } else {
                    skip
                };
                position[1] = position[1].wrapping_sub(
                    item(menu, start + preceding)?
                        .1
                        .row_height
                        .wrapping_mul(current.row_spacing) as i16,
                );
            }
        }
        let mut count = i32::from(current.item_count);
        let mut drawn = 0_i32;
        let mut scanned = 0_i32;
        let mut visited = std::collections::HashSet::new();
        while count > 0 && drawn < limit {
            let mut index = start.wrapping_add(scanned);
            if skip < 0 {
                index = index.wrapping_add(skip);
                if index < start {
                    index = index.wrapping_add(count);
                }
            }
            let current = layout(menu, layout_index)?;
            let total = i32::try_from(menu.data.records().items.len())
                .map_err(|_| MenuStateError::RecordCountOverflow)?;
            if index >= total {
                if current.flags & 8 == 0 {
                    break;
                }
                index = index.wrapping_sub(count);
            }
            // Original malformed wrap/visibility inputs can loop indefinitely.
            if !visited.insert((index, skip, drawn)) {
                return Err(MenuStateError::DrawTraversalCycle.into());
            }
            let (item_index, entry) = item(menu, index)?;
            if entry.flags & input.visibility.mask() != 0 {
                if skip > 0 {
                    skip -= 1;
                } else {
                    if (current.scroll_animation == 0 && drawn >= count)
                        || (current.scroll_animation != 0 && drawn > count)
                    {
                        break;
                    }
                    let mut request = prepare_row(
                        menu,
                        DrawFrame { viewport, input },
                        RowVisit {
                            layout_index,
                            item_index,
                            layout: current,
                            entry,
                            drawn,
                            count,
                        },
                        &mut position,
                        host.colors(),
                    );
                    let authored_y = position[1];
                    host.draw_row(&mut request, menu).map_err(MenuError::Host)?;
                    position[1] = commit_row_height(
                        menu,
                        viewport,
                        request,
                        authored_y,
                        item_index,
                        layout_index,
                    )?;
                    drawn += 1;
                    rows += 1;
                }
            }
            scanned = scanned.wrapping_add(1);
            count = i32::from(layout(menu, layout_index)?.item_count);
            if drawn >= count {
                break;
            }
        }
        start = start.wrapping_add(i32::from(layout(menu, layout_index)?.item_count));
        layout_index += 1;
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "menu_draw_tests.rs"]
mod tests;
