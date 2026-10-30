//! The factory shop's executable tables: the product records `0042F250`
//! returns, the row templates and layout group the shop menu `00431000`
//! copies, and the upgrade IDs its Energy Distribution rows test.

use thiserror::Error;

use crate::{
    parse_frontend_menu_items, parse_frontend_menu_layouts, ByteSpan, FrontendError,
    FrontendExecutable, FrontendExecutableError, FrontendMenuItem, FrontendMenuLayout,
};

/// `0042F250` below index 0x12: the products every factory lists.
pub const FACTORY_COMMON_PRODUCTS_ADDRESS: u32 = 0x0053_20d8;
pub const FACTORY_COMMON_PRODUCTS: usize = 0x12;
/// From index 0x12 on, the list named by factory `+15C` (0 or 1). Each list
/// ends with a record whose name WORD is `0xFFFF`.
pub const FACTORY_PRODUCT_LIST_ADDRESSES: [u32; 2] = [0x0053_2240, 0x0053_2348];
pub const FACTORY_PRODUCT_BYTES: usize = 0x14;
/// The longest list the parser follows before requiring its terminator.
const FACTORY_PRODUCT_LIST_LIMIT: usize = 64;
/// The ten 32-byte row templates `00431000` copies (`00532478..00532598`).
pub const FACTORY_SHOP_ROWS_ADDRESS: u32 = 0x0053_2478;
pub const FACTORY_SHOP_ROWS: u32 = 10;
/// The 0x50-byte layout group: four layouts and the zero terminator.
pub const FACTORY_SHOP_LAYOUTS_ADDRESS: u32 = 0x0053_25b8;
/// The WORD IDs `00431000` tests for its seven Energy Distribution rows.
pub const FACTORY_UPGRADE_IDS_ADDRESS: u32 = 0x0053_2608;
pub const FACTORY_UPGRADE_ROWS: usize = 7;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FactoryShopTableError {
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error(transparent)]
    Frontend(#[from] FrontendError),
    #[error("factory product list {address:#010x} has no terminator within {limit} records")]
    MissingTerminator { address: u32, limit: usize },
}

/// One 0x14-byte product record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FactoryProduct {
    pub virtual_address: u32,
    /// +0: the grant ID the factory's `+198` mask must own and the player
    /// may already own.
    pub id: u16,
    pub word_02: u16,
    /// +4: kind 9 grants the ID through `0046B2B0`; production spawns the
    /// other kinds as items.
    pub kind: u32,
    /// +8: the name text; `0xFFFF` ends a list.
    pub name: u16,
    /// +A: the amount granted or spawned, which `00431950` also compares
    /// with a weapon's stock.
    pub amount: u8,
    /// +B: the build time in seconds (`00431E50` stores it in microseconds).
    pub build_seconds: u8,
    /// +C: nonzero keeps an owned product listed.
    pub repeatable: u8,
    /// +D..+10: what `0046C6D0` takes from the player's crystals `+118` and
    /// the three token counters `+120/+124/+128`.
    pub costs: [u8; 4],
    pub tail: [u8; 3],
}

impl FactoryProduct {
    pub fn is_terminator(&self) -> bool {
        self.name == 0xffff
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactoryProducts {
    pub common: Vec<FactoryProduct>,
    /// The `+15C` lists, each ending with its terminator record.
    pub lists: [Vec<FactoryProduct>; 2],
}

impl FactoryProducts {
    /// `0042F250`: below 0x12 the common table, then the list of factory
    /// `+15C`. Any other `+15C` returns NULL, as does an index past the
    /// list's terminator (no caller reads past it).
    pub fn product(&self, kind_15c: u32, index: u32) -> Option<&FactoryProduct> {
        if index < FACTORY_COMMON_PRODUCTS as u32 {
            return self.common.get(index as usize);
        }
        self.lists
            .get(usize::try_from(kind_15c).ok()?)?
            .get((index - FACTORY_COMMON_PRODUCTS as u32) as usize)
    }
}

fn product(address: u32, bytes: &[u8]) -> FactoryProduct {
    let half = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    FactoryProduct {
        virtual_address: address,
        id: half(0),
        word_02: half(2),
        kind: u32::from_le_bytes(bytes[4..8].try_into().expect("four-byte kind")),
        name: half(8),
        amount: bytes[0xa],
        build_seconds: bytes[0xb],
        repeatable: bytes[0xc],
        costs: [bytes[0xd], bytes[0xe], bytes[0xf], bytes[0x10]],
        tail: [bytes[0x11], bytes[0x12], bytes[0x13]],
    }
}

pub fn parse_factory_products(
    source: FrontendExecutable<'_>,
) -> Result<FactoryProducts, FactoryShopTableError> {
    let common = source.mapped(
        FACTORY_COMMON_PRODUCTS_ADDRESS,
        FACTORY_COMMON_PRODUCTS * FACTORY_PRODUCT_BYTES,
    )?;
    let common = common
        .bytes
        .chunks_exact(FACTORY_PRODUCT_BYTES)
        .enumerate()
        .map(|(index, bytes)| {
            product(
                FACTORY_COMMON_PRODUCTS_ADDRESS + (index * FACTORY_PRODUCT_BYTES) as u32,
                bytes,
            )
        })
        .collect();
    let mut lists = [Vec::new(), Vec::new()];
    for (list, address) in lists.iter_mut().zip(FACTORY_PRODUCT_LIST_ADDRESSES) {
        for index in 0..FACTORY_PRODUCT_LIST_LIMIT {
            let record_address = address + (index * FACTORY_PRODUCT_BYTES) as u32;
            let range = source.mapped(record_address, FACTORY_PRODUCT_BYTES)?;
            let record = product(record_address, range.bytes);
            list.push(record);
            if record.is_terminator() {
                break;
            }
        }
        if !list.last().is_some_and(FactoryProduct::is_terminator) {
            return Err(FactoryShopTableError::MissingTerminator {
                address,
                limit: FACTORY_PRODUCT_LIST_LIMIT,
            });
        }
    }
    Ok(FactoryProducts { common, lists })
}

pub fn parse_factory_upgrade_ids(
    source: FrontendExecutable<'_>,
) -> Result<[u16; FACTORY_UPGRADE_ROWS], FactoryShopTableError> {
    let range = source.mapped(FACTORY_UPGRADE_IDS_ADDRESS, FACTORY_UPGRADE_ROWS * 2)?;
    let mut ids = [0; FACTORY_UPGRADE_ROWS];
    for (id, chunk) in ids.iter_mut().zip(range.bytes.chunks_exact(2)) {
        *id = u16::from_le_bytes([chunk[0], chunk[1]]);
    }
    Ok(ids)
}

/// The ten row templates, in address order.
pub fn parse_factory_shop_rows(
    source: FrontendExecutable<'_>,
) -> Result<Vec<FrontendMenuItem>, FactoryShopTableError> {
    Ok(parse_frontend_menu_items(
        source.data(),
        FACTORY_SHOP_ROWS_ADDRESS,
        FACTORY_SHOP_ROWS,
    )?)
}

/// The layout group up to its terminator, which `00431000` copies whole.
pub fn parse_factory_shop_layouts(
    source: FrontendExecutable<'_>,
) -> Result<(Vec<FrontendMenuLayout>, ByteSpan), FactoryShopTableError> {
    Ok(parse_frontend_menu_layouts(
        source.data(),
        FACTORY_SHOP_LAYOUTS_ADDRESS,
    )?)
}
