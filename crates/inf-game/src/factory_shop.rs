//! The factory shop: the menu `00431000` builds over the docked factory's
//! state, and the product rules it shares with the purchase `00431740`.
//!
//! Every row is a copy of one of ten executable templates. Row 0 is the
//! header ("Equipment Factory", text `0x2F6`, or `0x2F7` for a factory
//! whose `+15C` is 1) and row 1 a title. The rest depend on the factory's
//! state `+160`:
//!
//! - 0, Options (`0x308`): Facilities (to state 1), Energy Distribution (to
//!   state 2), Evacuate factory and Disconnect. While the factory holds
//!   fewer scientists (`+140`) than it can (`+13C`), the title is its staff
//!   row and the options are Beam down scientist, Evacuate (only with a
//!   scientist inside) and Disconnect.
//! - 1, Facilities: a row per listed product, then Back.
//! - 2, Energy Distribution: seven rows, one per token level, then Back.
//! - 3 to 6, production: the title alone, drawn as the build, fitting or
//!   recovery progress.

use inf_formats::{
    ByteSpan, FactoryProduct, FactoryProducts, FactoryShopTableError, FrontendExecutable,
    FrontendMenuItem, FrontendMenuLayout, FACTORY_SHOP_LAYOUTS_ADDRESS, FACTORY_UPGRADE_ROWS,
    GRANT_MASK_WORDS,
};
use thiserror::Error;

use crate::menu::{MenuData, MenuDerivedData, MenuLayouts};

/// The shop menu's constructor and vtable (`00431000`, `00521A14`).
pub const SHOP_MENU_CONSTRUCTOR: u32 = 0x0043_1000;
pub const SHOP_MENU_VTABLE: u32 = 0x0052_1a14;

/// The row templates by their index from `00532478`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Template {
    Header = 0,
    Product = 1,
    Upgrade = 2,
    Staff = 3,
    Back = 4,
    /// Facilities, Energy Distribution, Beam down scientist, Evacuate
    /// factory and Disconnect, consecutively.
    Facilities = 5,
}

const TEMPLATE_BEAM_DOWN: usize = 7;
const TEMPLATE_DISCONNECT: usize = 9;

/// The header's text for factory `+15C` 1.
const HEADER_ENEMY_TEXT: u32 = 0x2f7;
const HEADER_TEXT: u32 = 0x2f6;
/// "Options", the state-0 title.
const OPTIONS_TEXT: u32 = 0x308;
/// The Facilities title, which `0042FD70` draws.
const FACILITIES_TITLE_TEXT: u32 = 0x30a;
/// "Energy Distribution", which `0042FC00` draws.
const DISTRIBUTION_TITLE_TEXT: u32 = 0x2f9;
/// "Recovering for next process", the state-6 title.
const RECOVERY_TEXT: u32 = 0x302;

/// The draw callbacks `00431000` substitutes in its copies.
pub const FACILITIES_TITLE_DRAW: u32 = 0x0042_fd70;
pub const PRODUCT_ROW_DRAW: u32 = 0x0042_f870;
pub const DISTRIBUTION_TITLE_DRAW: u32 = 0x0042_fc00;
pub const BUILD_PROGRESS_DRAW: u32 = 0x0043_0e30;
pub const FITTING_PROGRESS_DRAW: u32 = 0x0043_0d70;

/// Row flag 0x40 disables a product row; the Energy Distribution rows of
/// unowned upgrades also get 0x08.
const ROW_DISABLED: u32 = 0x40;
const ROW_UNOWNED_UPGRADE: u32 = 0x48;

/// At most eight product rows are visible.
const VISIBLE_PRODUCTS: i32 = 8;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FactoryShopError {
    #[error(transparent)]
    Tables(#[from] FactoryShopTableError),
    #[error("00431000 has {0} row templates, not ten")]
    Templates(usize),
    #[error("00431000 copies a layout group of {0} records, not four")]
    Layouts(usize),
    #[error("factory state {0} is outside 00431000's seven cases")]
    State(u32),
    #[error("0042F250 returns NULL for factory +15C {kind} at product {index}")]
    Product { kind: u32, index: u32 },
    #[error("0046D310 reads weapon entry {index} past the unit's {count} entries")]
    WeaponEntry { index: u32, count: usize },
    #[error("grant ID {0:#x} is outside the six-DWORD masks")]
    GrantId(u32),
}

/// The executable data the shop reads.
#[derive(Clone, Debug)]
pub struct FactoryShopTables {
    pub products: FactoryProducts,
    pub upgrade_ids: [u16; FACTORY_UPGRADE_ROWS],
    rows: Vec<FrontendMenuItem>,
    layouts: Vec<FrontendMenuLayout>,
    layout_terminator: ByteSpan,
}

impl FactoryShopTables {
    pub fn parse(source: FrontendExecutable<'_>) -> Result<Self, FactoryShopError> {
        let rows = inf_formats::parse_factory_shop_rows(source)?;
        if rows.len() != 10 {
            return Err(FactoryShopError::Templates(rows.len()));
        }
        let (layouts, layout_terminator) = inf_formats::parse_factory_shop_layouts(source)?;
        if layouts.len() != 4 {
            return Err(FactoryShopError::Layouts(layouts.len()));
        }
        Ok(Self {
            products: inf_formats::parse_factory_products(source)?,
            upgrade_ids: inf_formats::parse_factory_upgrade_ids(source)?,
            rows,
            layouts,
            layout_terminator,
        })
    }

    fn row(&self, index: usize) -> FrontendMenuItem {
        self.rows[index]
    }

    fn template(&self, template: Template) -> FrontendMenuItem {
        self.row(template as usize)
    }

    /// `0042F250`.
    pub fn product(&self, kind_15c: u32, index: u32) -> Result<&FactoryProduct, FactoryShopError> {
        self.products
            .product(kind_15c, index)
            .ok_or(FactoryShopError::Product {
                kind: kind_15c,
                index,
            })
    }
}

/// What the shop reads from the factory.
#[derive(Clone, Copy, Debug)]
pub struct ShopFactory<'a> {
    pub state_160: u32,
    pub kind_15c: u32,
    /// The scientists the factory holds at most (`+13C`) and now (`+140`).
    pub staff_13c: i32,
    pub staff_140: i32,
    /// The grant mask `+198`: the products the factory can make.
    pub grants_198: &'a [u32; GRANT_MASK_WORDS],
}

/// What it reads from the docked vehicle's player (`0046B090` of `+188`).
#[derive(Clone, Copy, Debug)]
pub struct ShopPlayer<'a> {
    pub crystals_118: i32,
    /// The three token counters `+120/+124/+128`.
    pub tokens_120: [i32; 3],
    /// The upgrade bits `+100` that `0046BDC0` tests.
    pub upgrades_100: &'a [u32; 6],
    /// `0046D310`'s source: the first player's unit's behavior `+18`
    /// entries. None when its unit or the unit's `+64` view is missing,
    /// for which `0046D310` returns 0.
    pub weapon_entries: Option<&'a [[u32; 8]]>,
}

fn owns(mask: &[u32; GRANT_MASK_WORDS], id: u16) -> Result<bool, FactoryShopError> {
    inf_formats::grant_is_owned(mask, u32::from(id))
        .map_err(|_| FactoryShopError::GrantId(u32::from(id)))
}

impl ShopPlayer<'_> {
    /// `0046BDC0`.
    pub fn owns(&self, id: u16) -> Result<bool, FactoryShopError> {
        owns(self.upgrades_100, id)
    }

    /// `0046D310`: entry `index`'s first DWORD, the weapon's stock.
    fn weapon_stock(&self, index: u32) -> Result<u32, FactoryShopError> {
        let Some(entries) = self.weapon_entries else {
            return Ok(0);
        };
        entries
            .get(index as usize)
            .map(|entry| entry[0])
            .ok_or(FactoryShopError::WeaponEntry {
                index,
                count: entries.len(),
            })
    }
}

/// `00431950`: a weapon product whose weapon is already stocked to the
/// product's amount is invalid ("That weapon is already fully stocked").
pub fn product_is_valid(
    product: &FactoryProduct,
    player: &ShopPlayer<'_>,
) -> Result<bool, FactoryShopError> {
    let entry = match product.id {
        0x25 => 8,
        0xa0 => 4,
        0xa3 => 5,
        0xa8 => 10,
        0xa9 => 9,
        0xaa => 11,
        0xab => 12,
        0xac => 13,
        0xad => 14,
        0xae => 15,
        _ => return Ok(true),
    };
    Ok(player.weapon_stock(entry)? != u32::from(product.amount))
}

/// The listing test of `0042F1D0` and `00431000`: the factory knows the
/// product and the player either may buy it again or does not own it.
fn product_is_listed(
    product: &FactoryProduct,
    factory: &ShopFactory<'_>,
    player: &ShopPlayer<'_>,
) -> Result<bool, FactoryShopError> {
    Ok(owns(factory.grants_198, product.id)?
        && (product.repeatable != 0 || !player.owns(product.id)?))
}

/// `0042F1D0`, with the player present: the listed products up to the
/// list's terminator. The product at index 0 is tested before any
/// terminator check.
pub fn listed_product_count(
    tables: &FactoryShopTables,
    factory: &ShopFactory<'_>,
    player: &ShopPlayer<'_>,
) -> Result<i32, FactoryShopError> {
    let mut count = 0;
    let mut index = 0;
    let mut product = tables.product(factory.kind_15c, 0)?;
    loop {
        if product_is_listed(product, factory, player)? {
            count += 1;
        }
        index += 1;
        product = tables.product(factory.kind_15c, index)?;
        if product.is_terminator() {
            return Ok(count);
        }
    }
}

/// `00431000`'s rows, layouts and selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopMenuPlan {
    /// None when the factory's `+188` names no player: the menu keeps the
    /// base constructor's empty bindings.
    pub rows: Option<(Vec<FrontendMenuItem>, Vec<FrontendMenuLayout>)>,
    pub initial_selection: i32,
}

/// `00431000` over the factory's state. `player` is `0046B090` of the
/// factory's `+188`.
pub fn plan_shop_menu(
    tables: &FactoryShopTables,
    factory: &ShopFactory<'_>,
    player: Option<&ShopPlayer<'_>>,
) -> Result<ShopMenuPlan, FactoryShopError> {
    let Some(player) = player else {
        return Ok(ShopMenuPlan {
            rows: None,
            initial_selection: 0,
        });
    };
    let (staff_max, staff) = (factory.staff_13c, factory.staff_140);
    // Signed, as 0043107A compares them.
    let short_staffed = if staff == 0 {
        staff_max != 0
    } else {
        staff < staff_max
    };
    let (options, back) = match factory.state_160 {
        0 if staff == 0 && short_staffed => (2, 0),
        0 if short_staffed || staff_max == 0 => (3, 0),
        0 => (4, 0),
        1 => (listed_product_count(tables, factory, player)?, 1),
        2 => (FACTORY_UPGRADE_ROWS as i32, 1),
        3..=6 => (0, 0),
        state => return Err(FactoryShopError::State(state)),
    };
    let count = (options + 2 + back) as usize;
    let mut rows = vec![tables.template(Template::Header); count];
    match factory.kind_15c {
        0 => rows[0].draw_arguments[0] = HEADER_TEXT,
        1 => rows[0].draw_arguments[0] = HEADER_ENEMY_TEXT,
        _ => {}
    }
    let mut selection = (options + 1 + back).min(2);
    match factory.state_160 {
        0 if short_staffed => {
            // Short of scientists: the staff row, then Beam down scientist
            // and the options after it, skipping Evacuate while empty.
            rows[1] = tables.template(Template::Staff);
            selection = 2;
            if staff == 0 {
                rows[2] = tables.row(TEMPLATE_BEAM_DOWN);
                rows[3] = tables.row(TEMPLATE_DISCONNECT);
            } else {
                for (row, template) in rows[2..].iter_mut().zip(TEMPLATE_BEAM_DOWN..) {
                    *row = tables.row(template);
                }
            }
        }
        0 => {
            rows[1].draw_arguments[0] = OPTIONS_TEXT;
            // Facilities and Energy Distribution, then Evacuate (skipped
            // when the factory holds no scientists at all) and Disconnect.
            for index in 0..options as usize {
                let mut template = Template::Facilities as usize + index;
                if index >= 2 {
                    template += 1;
                    if staff_max == 0 {
                        template += 1;
                    }
                }
                rows[2 + index] = tables.row(template);
            }
        }
        1 => {
            rows[1].draw_callback = FACILITIES_TITLE_DRAW;
            rows[1].draw_arguments[0] = FACILITIES_TITLE_TEXT;
            let mut index = 0;
            for row in rows[2..2 + options as usize].iter_mut() {
                let product = loop {
                    let product = tables.product(factory.kind_15c, index)?;
                    if product_is_listed(product, factory, player)? {
                        break product;
                    }
                    index += 1;
                };
                // The crystal cost is not tested here; 00431740's
                // payment tests all four.
                let affordable = (0..3)
                    .all(|token| player.tokens_120[token] >= i32::from(product.costs[token + 1]));
                let enabled = affordable && product_is_valid(product, player)?;
                *row = tables.template(Template::Product);
                row.draw_callback = PRODUCT_ROW_DRAW;
                if enabled {
                    row.flags &= !ROW_DISABLED;
                } else {
                    row.flags |= ROW_DISABLED;
                }
                row.draw_arguments[1] = product.virtual_address;
                row.action_arguments[0] = index;
                index += 1;
            }
        }
        2 => {
            rows[1].draw_callback = DISTRIBUTION_TITLE_DRAW;
            rows[1].draw_arguments[0] = DISTRIBUTION_TITLE_TEXT;
            for (level, row) in rows[2..2 + FACTORY_UPGRADE_ROWS].iter_mut().enumerate() {
                *row = tables.template(Template::Upgrade);
                row.draw_arguments[0] = level as u32;
                row.action_arguments[0] = level as u32;
                if !player.owns(tables.upgrade_ids[level])? {
                    row.flags |= ROW_UNOWNED_UPGRADE;
                }
            }
        }
        3 => rows[1].draw_callback = BUILD_PROGRESS_DRAW,
        4 | 5 => rows[1].draw_callback = FITTING_PROGRESS_DRAW,
        _ => rows[1].draw_arguments[0] = RECOVERY_TEXT,
    }
    if back != 0 {
        rows[options as usize + 2] = tables.template(Template::Back);
    }
    // The layout group's third record lists the options (at most eight
    // visible) and the fourth the Back row.
    let mut layouts = tables.layouts.clone();
    layouts[2].item_count = options as u8;
    layouts[2].visible_rows = options.min(VISIBLE_PRODUCTS) as u8;
    layouts[3].item_count = back as u8;
    layouts[3].visible_rows = back as u8;
    Ok(ShopMenuPlan {
        rows: Some((rows, layouts)),
        initial_selection: selection,
    })
}

impl ShopMenuPlan {
    /// The menu's own copies; the layout group keeps its executable source.
    pub fn menu_data(self, tables: &FactoryShopTables) -> MenuData {
        let (items, layouts) = match self.rows {
            Some((items, layouts)) => (
                Some(items),
                Some(MenuLayouts::new(
                    FACTORY_SHOP_LAYOUTS_ADDRESS,
                    tables.layout_terminator,
                    layouts,
                )),
            ),
            None => (None, None),
        };
        MenuData::derived(MenuDerivedData {
            constructor: SHOP_MENU_CONSTRUCTOR,
            vtable: SHOP_MENU_VTABLE,
            items,
            layouts,
            initial_selection: self.initial_selection,
            flags: 0,
        })
    }
}

#[cfg(test)]
#[path = "factory_shop_tests.rs"]
mod tests;
