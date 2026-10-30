//! The factory shop's own row draws in the shown menu, the callbacks
//! `00431000` installs.
//!
//! Each callback reads the menu's factory (`+5C`) and, through `0046B090`
//! of the factory's `+188`, the docked player; the port gathers both before
//! the draw ([`ShopDrawState`]). Text goes through `00453810` on the row's
//! descriptor (a left text, then a right-aligned one), bars through
//! `0042FA70`.
//!
//! - `0042FC00`, the Energy Distribution title: "Spare Tokens" with the
//!   player's `+118` right-aligned, and a bar of it out of 100.
//! - `00430AB0`, a token level: the weapon's name with its level
//!   (`+13C + index`) as a percentage, and a bar of it.
//! - `0042F870`, a product: its name in four fifths of the width (cut to
//!   17 characters and "..." beyond 20), then its three token costs
//!   right-aligned in styles `0x27`, `0x29` and `0x28`, padded by four, two
//!   and no spaces. A descriptor with flag 0x40 (from a layout with flag
//!   0x80, which the shop's lack) takes style 5; the menu walk colours a
//!   disabled product (item flag 0x40).
//! - `00430C50`, the staff row: "Insufficient Staff (present/most)" in
//!   style `0x28` while `005471C0 / 25` is even. Every draw sets the
//!   player's command `0x43`.
//! - `00430E30` and `00430D70`, building and fitting: "Manufacturing %s"
//!   over a progress bar, and "Fitting %s... Please wait", with the name of
//!   the product's grant (`+18C`).
//! - `0042FD70`, the Facilities title: the selected product's model,
//!   turning; its status; the player's three token counters, each against
//!   the product's cost; and the product's name.

use inf_formats::{FactoryProduct, FrontendExecutable, GrantTableError, GRANT_TEXT_IDS};
use thiserror::Error;

use crate::{
    factory_shop::{product_is_valid, FactoryShopError, FactoryShopTables, ShopPlayer},
    menu::{MenuColor, MenuRowDescriptor, MenuRowDrawRequest, MenuState},
    object_runtime::PlayerId,
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
};

pub const DISTRIBUTION_TITLE_DRAW: u32 = crate::factory_shop::DISTRIBUTION_TITLE_DRAW;
pub const LEVEL_ROW_DRAW: u32 = 0x0043_0ab0;
pub const PRODUCT_ROW_DRAW: u32 = crate::factory_shop::PRODUCT_ROW_DRAW;
pub const STAFF_ROW_DRAW: u32 = 0x0043_0c50;
pub const BUILD_PROGRESS_DRAW: u32 = crate::factory_shop::BUILD_PROGRESS_DRAW;
pub const FITTING_PROGRESS_DRAW: u32 = crate::factory_shop::FITTING_PROGRESS_DRAW;
pub const FACILITIES_TITLE_DRAW: u32 = crate::factory_shop::FACILITIES_TITLE_DRAW;

/// `0042FA70`'s sprite and vertex DWORD.
const BAR_SPRITE: u16 = 0x22b;
const BAR_VERTEX: u32 = 0x40;

/// `00430AB0`'s texts, by level: Cannons, Plasmas, Lasers, Flamethrower,
/// Rockets, Grenade and Shielding.
const LEVEL_TEXTS: [i32; 7] = [0x14c, 0x14d, 0x14e, 0x14f, 0x150, 0x144, 0x307];
const SPARE_TOKENS_TEXT: i32 = 0x309;
const STAFF_TEXT: i32 = 0x300;
const MANUFACTURING_TEXT: i32 = 0x2fd;
const FITTING_TEXT: i32 = 0x301;
/// The Facilities title's status: "Duration: %d secs ", "Fully Stocked",
/// "Insufficient Crystals" and "Crystals Required". The last is formatted
/// for a product without a build time and never drawn.
const DURATION_TEXT: i32 = 0x30b;
const STOCKED_TEXT: i32 = 0x30d;
const SHORT_TEXT: i32 = 0x30e;
/// The command `00430C50` sets on every draw.
pub const STAFF_COMMAND: u32 = 0x43;
/// The vehicle's model, whose preview picks the form it shows.
const VEHICLE_MODEL: i16 = 0x75;
/// Product names longer than this are cut to 17 characters and "...".
const NAME_LIMIT: usize = 20;

/// The executable data the draws read.
#[derive(Clone, Debug)]
pub struct ShopDrawTables {
    pub shop: FactoryShopTables,
    /// `00475320`'s record WORDs +0 (the grant's name) and +2 (its model).
    pub grant_texts: [u16; GRANT_TEXT_IDS],
    pub grant_models: [u16; GRANT_TEXT_IDS],
}

#[derive(Debug, Error)]
pub enum ShopDrawTablesError {
    #[error(transparent)]
    Shop(#[from] FactoryShopError),
    #[error(transparent)]
    Grants(#[from] GrantTableError),
}

impl ShopDrawTables {
    pub fn parse(source: FrontendExecutable<'_>) -> Result<Self, ShopDrawTablesError> {
        Ok(Self {
            shop: FactoryShopTables::parse(source)?,
            grant_texts: inf_formats::parse_grant_texts(source)?,
            grant_models: inf_formats::parse_grant_models(source)?,
        })
    }

    /// A row's product record by its address (`0042F870`'s second
    /// argument, the selected row's `+0C` for `0042FD70`).
    fn product_at(&self, address: u32) -> Option<&FactoryProduct> {
        let products = &self.shop.products;
        products
            .common
            .iter()
            .chain(products.lists.iter().flatten())
            .find(|product| product.virtual_address == address)
    }
}

/// The factory fields the draws read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShopDrawFactory {
    /// The scientists it holds at most (`+13C`) and now (`+140`).
    pub staff_13c: i32,
    pub staff_140: i32,
    /// The product's build time in microseconds (`+158`) and the build
    /// progress (`+16C`).
    pub build_158: Option<u32>,
    pub progress_16c: u32,
    /// The product's grant ID (`+18C`).
    pub product_18c: Option<u16>,
}

/// The docked player's fields the draws read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopDrawPlayer {
    pub id: PlayerId,
    pub crystals_118: i32,
    /// The token counters `+120/+124/+128`.
    pub tokens_120: [i32; 3],
    /// The token levels `+13C..+142`.
    pub levels_13c: [u8; 8],
    pub upgrades_100: [u32; 6],
    /// `00431950`'s weapon stocks: the first player's unit's entries.
    pub weapon_entries: Option<Vec<[u32; 8]>>,
}

impl ShopDrawPlayer {
    fn shop_player(&self) -> ShopPlayer<'_> {
        ShopPlayer {
            crystals_118: self.crystals_118,
            tokens_120: self.tokens_120,
            upgrades_100: &self.upgrades_100,
            weapon_entries: self.weapon_entries.as_deref(),
        }
    }
}

/// What a shop menu's draw reads from the world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopDrawState {
    pub factory: ShopDrawFactory,
    /// `0046B090` of the factory's `+188`.
    pub player: Option<ShopDrawPlayer>,
    /// The animation clock `005471C0`.
    pub clock: u32,
}

/// The Facilities title's model (`0042FDF5..00430040`): the context's
/// projection centred on the shifted descriptor, `0048FB00`'s fade, the
/// eye raised above the model, and `0049FEA0` with `0043A150`'s
/// descriptor (vtable `00522360`), turned in XZ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShopPreview {
    pub model_slot: i16,
    /// Context `+48/+4C`.
    pub center: [i32; 2],
    /// Context `+64/+68`.
    pub fade: [i32; 2],
    /// Context `+8`, the eye's height.
    pub eye_y: i32,
    /// Descriptor `+4/+8/+C`.
    pub position: [i32; 3],
    /// `00491530`'s angle: the clock times 64.
    pub angle: i32,
    /// Word `00533760` when the model draws.
    pub selector_one: u16,
}

/// The draws' services.
pub trait ShopDrawHost {
    type Error;
    /// `00495900`/`00495910`.
    fn surface(&self) -> [i32; 2];
    /// The first DWORD of style `slot`.
    fn style(&self, slot: i32) -> Result<u32, Self::Error>;
    /// H0 text `id`.
    fn string(&self, id: i32) -> Result<Vec<u8>, Self::Error>;
    /// The first DWORD of model `slot`, or None where `0042FE16..0042FE53`
    /// skip the model: past the pool's count or with nothing loaded.
    fn model_extent(&self, slot: i16) -> Result<Option<u32>, Self::Error>;
    /// `00453810`: the left text, then the right one right-aligned, on the
    /// descriptor.
    fn text(
        &mut self,
        descriptor: &mut MenuRowDescriptor,
        origin: [i32; 2],
        left: Option<&[u8]>,
        right: Option<&[u8]>,
    ) -> Result<(), Self::Error>;
    /// The renderer's +A4 (bars) or +9C (the panel).
    fn quad(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error>;
    /// The full-surface clip, a batch, the model, the flush and the clip
    /// restored.
    fn preview(&mut self, preview: ShopPreview) -> Result<(), Self::Error>;
    /// Word `00533760`.
    fn selector_one(&self) -> u16;
    fn set_selector_one(&mut self, value: u16);
    /// `0046C760(raw_id, 0)` on the player.
    fn command(&mut self, player: PlayerId, raw_id: u32) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum ShopDrawError<E> {
    #[error("shop draw host: {0:?}")]
    Host(E),
    #[error("{callback:#010x} reads product record {address:#010x}, which no product list holds")]
    ProductRecord { callback: u32, address: u32 },
    #[error("00430AB0 reads token level {0}, past the seven")]
    LevelIndex(u32),
    #[error("0042FD70 reads selection {0}, outside the menu's items")]
    Selection(i32),
    #[error("{callback:#010x} reads the factory's unwritten {field}")]
    Unwritten { callback: u32, field: &'static str },
    #[error("grant ID {0:#x} is past 00475320's tables")]
    GrantId(u16),
    #[error("{callback:#010x} formats {format:?} with arguments it does not pass")]
    Format { callback: u32, format: String },
    #[error(transparent)]
    Shop(#[from] FactoryShopError),
}

type ShopResult<T, E> = Result<T, ShopDrawError<E>>;

/// The shop's row draws for one menu.
#[derive(Clone, Copy)]
pub struct ShopDraw<'a> {
    pub tables: &'a ShopDrawTables,
    pub state: &'a ShopDrawState,
}

/// The row's own callback, if the shop installed it. Returns false for
/// any other callback.
pub fn draw_shop_row<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    menu: &MenuState,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<bool, H::Error> {
    match request.callback_address {
        DISTRIBUTION_TITLE_DRAW => distribution_title(request, shop, host)?,
        LEVEL_ROW_DRAW => level_row(request, shop, host)?,
        PRODUCT_ROW_DRAW => product_row(request, shop, host)?,
        STAFF_ROW_DRAW => staff_row(request, shop, host)?,
        BUILD_PROGRESS_DRAW => build_row(request, shop, host)?,
        FITTING_PROGRESS_DRAW => fitting_row(request, shop, host)?,
        FACILITIES_TITLE_DRAW => facilities_title(request, menu, shop, host)?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// `0042FA70(x, y, horizontal, vertical, length, height, background,
/// fill)`: a bar of `length` by `height` up from `y`. A nonzero
/// `horizontal` fills that far across; otherwise the fill rises
/// `vertical`. Two +A4 quads of sprite 0x22B, the fill first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShopBar {
    pub x: i32,
    pub y: i32,
    pub horizontal: i32,
    pub vertical: i32,
    pub length: i32,
    pub height: i32,
    pub background: u32,
    pub fill: u32,
}

impl ShopBar {
    pub fn quads(self) -> [DirectSpriteQuad; 2] {
        let Self { x, y, .. } = self;
        let end = x.wrapping_add(self.length);
        let top = y.wrapping_sub(self.height);
        let (fill, rest) = if self.horizontal != 0 {
            let split = x.wrapping_add(self.horizontal);
            (
                [[x, y], [split, y], [split, top], [x, top]],
                [[end, y], [split, y], [split, top], [end, top]],
            )
        } else {
            let level = y.wrapping_sub(self.vertical);
            (
                [[x, y], [end, y], [end, level], [x, level]],
                [[x, top], [end, top], [end, level], [x, level]],
            )
        };
        let quad = |corners: [[i32; 2]; 4], color_word| DirectSpriteQuad {
            sprite_slot: BAR_SPRITE,
            corners: corners.map(|[x, y]| [x as i16, y as i16]),
            vertex_depths: [BAR_VERTEX; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Effect { color_word },
        };
        [quad(fill, self.fill), quad(rest, self.background)]
    }
}

fn bar<H: ShopDrawHost>(host: &mut H, bar: ShopBar) -> ShopResult<(), H::Error> {
    for quad in bar.quads() {
        host.quad(quad).map_err(ShopDrawError::Host)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Arg<'a> {
    Int(i32),
    Text(&'a [u8]),
}

fn terminated(text: &[u8]) -> &[u8] {
    &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())]
}

/// `sprintf` over the directives the shop's formats use: `%d`, `%s` and
/// `%%`. Any other, or a missing argument, is an error.
fn format<E>(callback: u32, format: &[u8], args: &[Arg<'_>]) -> ShopResult<Vec<u8>, E> {
    let format = terminated(format);
    let error = || ShopDrawError::Format {
        callback,
        format: String::from_utf8_lossy(format).into_owned(),
    };
    let mut out = Vec::new();
    let mut args = args.iter();
    let mut bytes = format.iter();
    while let Some(&byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        match (bytes.next(), args.clone().next()) {
            (Some(b'%'), _) => out.push(b'%'),
            (Some(b'd'), Some(Arg::Int(value))) => {
                out.extend_from_slice(value.to_string().as_bytes());
                args.next();
            }
            (Some(b's'), Some(Arg::Text(text))) => {
                out.extend_from_slice(terminated(text));
                args.next();
            }
            _ => return Err(error()),
        }
    }
    Ok(out)
}

fn style<H: ShopDrawHost>(host: &H, slot: i32) -> ShopResult<u32, H::Error> {
    host.style(slot).map_err(ShopDrawError::Host)
}

fn string<H: ShopDrawHost>(host: &H, id: i32) -> ShopResult<Vec<u8>, H::Error> {
    host.string(id).map_err(ShopDrawError::Host)
}

fn text<H: ShopDrawHost>(
    host: &mut H,
    request: &mut MenuRowDrawRequest,
    left: Option<&[u8]>,
    right: Option<&[u8]>,
) -> ShopResult<(), H::Error> {
    let origin = request.context_origin_pixels;
    host.text(&mut request.descriptor, origin, left, right)
        .map_err(ShopDrawError::Host)
}

fn product<'a, E>(
    shop: ShopDraw<'a>,
    callback: u32,
    address: u32,
) -> ShopResult<&'a FactoryProduct, E> {
    shop.tables
        .product_at(address)
        .ok_or(ShopDrawError::ProductRecord { callback, address })
}

/// The name of the grant the factory produces (`00475320` of `+18C`).
fn grant_name<H: ShopDrawHost>(
    shop: ShopDraw<'_>,
    callback: u32,
    host: &H,
) -> ShopResult<Vec<u8>, H::Error> {
    let id = shop
        .state
        .factory
        .product_18c
        .ok_or(ShopDrawError::Unwritten {
            callback,
            field: "product +18C",
        })?;
    let text = *shop
        .tables
        .grant_texts
        .get(usize::from(id))
        .ok_or(ShopDrawError::GrantId(id))?;
    string(host, i32::from(text as i16))
}

/// Descriptor WORD arithmetic (`add word ptr`).
fn add_word(word: &mut i16, value: i32) {
    *word = word.wrapping_add(value as i16);
}

/// The descriptor's position and width, sign-extended.
fn frame(descriptor: &MenuRowDescriptor) -> (i32, i32, i32, i32) {
    (
        i32::from(descriptor.position_pixels[0]),
        i32::from(descriptor.position_pixels[1]),
        i32::from(descriptor.extent_pixels[0]),
        i32::from(descriptor.extent_pixels[1]),
    )
}

/// The bar both token titles and the token levels draw: from ten
/// sixteenths across, a fifth of the width long and a 48th of the surface
/// high, filled across `value` out of 100.
fn tokens_bar<H: ShopDrawHost>(
    host: &mut H,
    descriptor: &MenuRowDescriptor,
    value: i32,
) -> ShopResult<(), H::Error> {
    let (x, y, width, _) = frame(descriptor);
    let surface = host.surface();
    let bar_value = ShopBar {
        x: x.wrapping_add(width.wrapping_mul(10) / 16),
        y,
        horizontal: width.wrapping_mul(value) / 500,
        vertical: 0,
        length: width / 5,
        height: surface[1] / 48,
        background: style(host, 1)?,
        fill: style(host, 0)?,
    };
    bar(host, bar_value)
}

/// `0042FC00`.
fn distribution_title<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let Some(player) = &shop.state.player else {
        return Ok(());
    };
    let count = format(
        DISTRIBUTION_TITLE_DRAW,
        b"%d",
        &[Arg::Int(player.crystals_118)],
    )?;
    let label = string(host, SPARE_TOKENS_TEXT)?;
    text(host, request, Some(&label), Some(&count))?;
    tokens_bar(host, &request.descriptor, player.crystals_118.min(100))
}

/// `00430AB0`: the level of argument 0.
fn level_row<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let Some(player) = &shop.state.player else {
        return Ok(());
    };
    let index = request.arguments[0];
    let (Some(&level), Some(&name)) = (
        player.levels_13c.get(index as usize),
        LEVEL_TEXTS.get(index as usize),
    ) else {
        return Err(ShopDrawError::LevelIndex(index));
    };
    let level = i32::from(level as i8);
    let percent = format(LEVEL_ROW_DRAW, b"%d%%", &[Arg::Int(level)])?;
    let label = string(host, name)?;
    text(host, request, Some(&label), Some(&percent))?;
    tokens_bar(host, &request.descriptor, level)
}

/// `0042F870`: the product record of argument 1.
fn product_row<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    if request.descriptor.flags & 0x40 != 0 {
        request.descriptor.color = MenuColor::from_word(style(host, 5)?);
    }
    let product = product(shop, PRODUCT_ROW_DRAW, request.arguments[1])?;
    let width = request.descriptor.extent_pixels[0];
    request.descriptor.extent_pixels[0] = (i32::from(width).wrapping_mul(80) / 100) as i16;
    // The name is the format, without arguments.
    let mut name = format(
        PRODUCT_ROW_DRAW,
        &string(host, i32::from(product.name as i16))?,
        &[],
    )?;
    if name.len() > NAME_LIMIT {
        name.truncate(NAME_LIMIT);
        name[NAME_LIMIT - 3..].copy_from_slice(b"...");
    }
    text(host, request, Some(&name), None)?;
    request.descriptor.extent_pixels[0] = width;
    for (slot, padding, cost) in [
        (0x27, &b"%d    "[..], product.costs[1]),
        (0x29, &b"%d  "[..], product.costs[2]),
        (0x28, &b"%d"[..], product.costs[3]),
    ] {
        request.descriptor.color = MenuColor::from_word(style(host, slot)?);
        let value = format(PRODUCT_ROW_DRAW, padding, &[Arg::Int(i32::from(cost))])?;
        text(host, request, Some(b""), Some(&value))?;
    }
    Ok(())
}

/// Whether the half of `005471C0 / 25`'s cycle that blinks a warning off
/// is running: the parity of the signed quotient.
fn odd_blink(clock: u32) -> bool {
    (clock as i32 / 25) % 2 != 0
}

/// `00430C50`.
fn staff_row<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let shown = !odd_blink(shop.state.clock);
    request.descriptor.color = MenuColor::from_word(style(host, 0x28)?);
    if let Some(player) = &shop.state.player {
        host.command(player.id, STAFF_COMMAND)
            .map_err(ShopDrawError::Host)?;
    }
    if !shown {
        return Ok(());
    }
    let factory = shop.state.factory;
    // 0040E090(0): the scientists the factory lacks.
    let lacking = factory.staff_13c.wrapping_sub(factory.staff_140);
    let label = string(host, STAFF_TEXT)?;
    let line = format(
        STAFF_ROW_DRAW,
        b"%s (%d/%d)",
        &[
            Arg::Text(&label),
            Arg::Int(factory.staff_13c.wrapping_sub(lacking)),
            Arg::Int(factory.staff_13c),
        ],
    )?;
    text(host, request, Some(&line), None)
}

/// `00430D70`.
fn fitting_row<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let name = grant_name(shop, FITTING_PROGRESS_DRAW, host)?;
    let line = format(
        FITTING_PROGRESS_DRAW,
        &string(host, FITTING_TEXT)?,
        &[Arg::Text(&name)],
    )?;
    text(host, request, Some(&line), None)
}

/// `00430E30`: the progress `+16C` out of `+158`, as a percentage (100
/// without a build time), filling a bar two thirds of the width long from
/// a sixth across, a 24th of the surface above the row's bottom.
fn build_row<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let name = grant_name(shop, BUILD_PROGRESS_DRAW, host)?;
    let line = format(
        BUILD_PROGRESS_DRAW,
        &string(host, MANUFACTURING_TEXT)?,
        &[Arg::Text(&name)],
    )?;
    let fill = style(host, 0)?;
    let background = style(host, 0x21)?;
    let factory = shop.state.factory;
    let build = factory.build_158.ok_or(ShopDrawError::Unwritten {
        callback: BUILD_PROGRESS_DRAW,
        field: "build time +158",
    })? as i32;
    let percent = if build != 0 {
        (factory.progress_16c as i32)
            .wrapping_mul(100)
            .wrapping_div(build)
    } else {
        100
    };
    let (x, y, width, height) = frame(&request.descriptor);
    let surface = host.surface();
    bar(
        host,
        ShopBar {
            x: x.wrapping_add(width / 6),
            y: y.wrapping_add(height).wrapping_sub(surface[1] / 24),
            horizontal: width.wrapping_mul(percent).wrapping_mul(2) / 300,
            vertical: 0,
            length: width.wrapping_mul(2) / 3,
            height: surface[1] / 48,
            background,
            fill,
        },
    )?;
    text(host, request, Some(&line), None)
}

/// `0042FD70`: the selected row's product (its `+0C`); nothing when that
/// row has none, as Back has.
fn facilities_title<H: ShopDrawHost>(
    request: &mut MenuRowDrawRequest,
    menu: &MenuState,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let selection = menu.selection();
    let address = usize::try_from(selection)
        .ok()
        .and_then(|index| menu.data().records().items.get(index).copied())
        .ok_or(ShopDrawError::Selection(selection))?
        .draw_arguments[1];
    if address == 0 {
        return Ok(());
    }
    let product = *product(shop, FACILITIES_TITLE_DRAW, address)?;
    let [width, height] = host.surface();
    let descriptor = &mut request.descriptor;
    add_word(
        &mut descriptor.position_pixels[1],
        height.wrapping_mul(-60) / 256,
    );
    add_word(&mut descriptor.position_pixels[0], width / 20);
    preview(&product, request, shop, host)?;
    let descriptor = &mut request.descriptor;
    add_word(
        &mut descriptor.position_pixels[1],
        height.wrapping_mul(60) / 256,
    );
    add_word(&mut descriptor.position_pixels[0], -(width / 20));
    if let Some(player) = &shop.state.player {
        tokens(&product, request, player, shop, host)?;
    }
    let descriptor = &mut request.descriptor;
    descriptor.color = MenuColor::from_word(style(host, 5)?);
    add_word(&mut descriptor.position_pixels[0], width / 10);
    add_word(
        &mut descriptor.position_pixels[1],
        height.wrapping_mul(8) / 256,
    );
    add_word(&mut descriptor.extent_pixels[0], -(width / 10));
    let name = string(host, i32::from(product.name as i16))?;
    text(host, request, Some(&name), None)
}

/// `0042FDF5..00430040`: the product's grant model (`00475320` WORD +2)
/// at six times its extent plus 500, the eye raised two fifths of that,
/// turned by the clock in XZ. The vehicle's model (0x75) sits 0x14 higher
/// and shows the product's form (word `00533760`): the truck 2, the
/// hovercraft 3, the VTOL 4 and the skimmer 5 (products 2, 0xA7, 0xA5 and
/// 0xA6), the buggy 1.
fn preview<H: ShopDrawHost>(
    product: &FactoryProduct,
    request: &MenuRowDrawRequest,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let model = *shop
        .tables
        .grant_models
        .get(usize::from(product.id))
        .ok_or(ShopDrawError::GrantId(product.id))? as i16;
    if model < 0 {
        return Ok(());
    }
    let Some(extent) = host.model_extent(model).map_err(ShopDrawError::Host)? else {
        return Ok(());
    };
    let distance = (extent as i32).wrapping_mul(6).wrapping_add(500);
    let mut position = [0, 0, distance];
    let mut selector_one = host.selector_one();
    if model == VEHICLE_MODEL {
        position[1] += 0x14;
        selector_one = match product.id {
            2 => 2,
            0xa5 => 4,
            0xa6 => 5,
            0xa7 => 3,
            _ => 1,
        };
        host.set_selector_one(selector_one);
    }
    let (x, y, _, _) = frame(&request.descriptor);
    host.preview(ShopPreview {
        model_slot: model,
        center: [x, y],
        fade: [distance.wrapping_add(2000), distance.wrapping_add(2500)],
        eye_y: distance.wrapping_mul(2) / 5,
        position,
        angle: (shop.state.clock as i32).wrapping_shl(6),
        selector_one,
    })
    .map_err(ShopDrawError::Host)
}

/// `0043009D..00430900`: the status a 64th of the surface higher, then a
/// column per token counter at five, three and one 52nds of the surface
/// from the row's right end, a 16th of the surface high: the counter out
/// of 20 over style 8, then the cost over style 7, in the counter's style
/// (`0x22`, `0x23`, `0x24`). A warning, and the cost the player cannot
/// pay, blink while `005471C0 / 25` is odd. Each column's "%d/%d" is
/// formatted, in styles `0x27`, `0x29` and `0x28`, and never drawn.
fn tokens<H: ShopDrawHost>(
    product: &FactoryProduct,
    request: &mut MenuRowDrawRequest,
    player: &ShopDrawPlayer,
    shop: ShopDraw<'_>,
    host: &mut H,
) -> ShopResult<(), H::Error> {
    let [width, height] = host.surface();
    let (column, column_height) = (width / 52, height / 16);
    let blink = odd_blink(shop.state.clock);
    let costs = [product.costs[1], product.costs[2], product.costs[3]].map(i32::from);
    let covered = |token: usize| player.tokens_120[token] >= costs[token];
    add_word(
        &mut request.descriptor.position_pixels[1],
        height.wrapping_mul(-4) / 256,
    );
    let (status, args) = if (0..3).all(covered) {
        if !product_is_valid(product, &player.shop_player())? {
            (blink.then_some(STOCKED_TEXT), Vec::new())
        } else if product.build_seconds != 0 {
            (
                Some(DURATION_TEXT),
                vec![Arg::Int(i32::from(product.build_seconds))],
            )
        } else {
            (None, Vec::new())
        }
    } else {
        (blink.then_some(SHORT_TEXT), Vec::new())
    };
    if let Some(status) = status {
        let line = format(FACILITIES_TITLE_DRAW, &string(host, status)?, &args)?;
        text(host, request, Some(&line), None)?;
    }
    add_word(
        &mut request.descriptor.position_pixels[1],
        height.wrapping_mul(4) / 256,
    );
    for (token, (offset, slot)) in [(5, 0x22), (3, 0x23), (1, 0x24)].into_iter().enumerate() {
        let (x, y, row_width, _) = frame(&request.descriptor);
        let column_bar = |vertical: i32, background: u32, fill: u32| ShopBar {
            x: x.wrapping_sub(column.wrapping_mul(offset))
                .wrapping_add(row_width),
            y,
            horizontal: 0,
            vertical,
            length: column,
            height: column_height,
            background,
            fill,
        };
        let fill = style(host, slot)?;
        let counter = player.tokens_120[token].wrapping_mul(column_height) / 20;
        bar(host, column_bar(counter, style(host, 8)?, fill))?;
        if covered(token) || blink {
            let cost = costs[token].wrapping_mul(column_height) / 20;
            bar(host, column_bar(cost, style(host, 7)?, fill))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "factory_shop_draw_tests.rs"]
mod tests;
