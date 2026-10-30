use std::path::{Path, PathBuf};

use super::*;
use crate::frontend_assets::FrontendAssets;

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn tables() -> FactoryShopTables {
    let assets = FrontendAssets::open(&data_root()).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    FactoryShopTables::parse(executable).unwrap()
}

/// A row as Infest01's shopmenus query prints it: its eight DWORDs.
fn row_words(row: &FrontendMenuItem) -> [u32; 8] {
    [
        row.flags,
        row.draw_callback,
        row.draw_arguments[0],
        row.draw_arguments[1],
        row.action_callback,
        row.action_arguments[0],
        row.action_arguments[1],
        u32::from(row.row_height) | u32::from(row.age) << 16,
    ]
}

fn layout_words(layout: &FrontendMenuLayout) -> [u32; 4] {
    [
        u32::from_le_bytes([
            layout.flags,
            layout.item_count,
            layout.visible_rows,
            layout.reserved,
        ]),
        layout.x as u16 as u32 | (layout.y as u16 as u32) << 16,
        layout.width as u16 as u32 | (layout.height as u16 as u32) << 16,
        u32::from(layout.row_spacing) | (layout.scroll_animation as u16 as u32) << 16,
    ]
}

/// Infest01's grant mask at both factories' first builds: the player's,
/// which docking copies into the factory. The second factory's also owns
/// turbo (3) and ID 0xF.
const FIRST_FACTORY_MASK: [u32; 6] = [0x24f6, 0x40000, 0x4100, 0x1440_0000, 8, 0];
const SECOND_FACTORY_MASK: [u32; 6] = [0xa4fe, 0x40000, 0x4100, 0x1440_0000, 8, 0];
/// The player's `+100` bits at the first factory (upgrade 0x32 owned) and
/// at the second, after the truck (2) and ID 0xF.
const FIRST_PLAYER_BITS: [u32; 6] = [0x92, 0x40000, 0, 0, 0, 0];
const SECOND_PLAYER_BITS: [u32; 6] = [0x8096, 0x40000, 0, 0, 0, 0];

fn factory(state: u32, grants: &[u32; 6]) -> ShopFactory<'_> {
    ShopFactory {
        state_160: state,
        kind_15c: 0,
        staff_13c: 2,
        staff_140: 2,
        grants_198: grants,
    }
}

fn player(bits: &[u32; 6], tokens: [i32; 3]) -> ShopPlayer<'_> {
    ShopPlayer {
        crystals_118: 10,
        tokens_120: tokens,
        upgrades_100: bits,
        weapon_entries: None,
    }
}

fn plan(factory: &ShopFactory<'_>, player: &ShopPlayer<'_>) -> (Vec<[u32; 8]>, Vec<[u32; 4]>, i32) {
    let plan = plan_shop_menu(&tables(), factory, Some(player)).unwrap();
    let (rows, layouts) = plan.rows.unwrap();
    (
        rows.iter().map(row_words).collect(),
        layouts.iter().map(layout_words).collect(),
        plan.initial_selection,
    )
}

/// The layout group's first and last fixed records, which every build
/// keeps.
const LAYOUT_HEADER: [u32; 4] = [0x10101, 0x640050, 0x780400, 0x3d];
const LAYOUT_TITLE: [u32; 4] = [0x10101, 0xc80032, 0xb40400, 0x3d];

fn layouts(options: u32, back: u32) -> Vec<[u32; 4]> {
    vec![
        LAYOUT_HEADER,
        LAYOUT_TITLE,
        [
            2 | options << 8 | options.min(8) << 16,
            0x17c0032,
            0x1f40400,
            0x3d,
        ],
        [1 | back << 8 | back << 16, 0x38400b4, 0xb40400, 0x3d],
    ]
}

#[test]
fn tables_parse_the_executable_shop_data() {
    let tables = tables();
    assert_eq!(tables.products.common.len(), 18);
    // Each list ends with its terminator: twelve and fourteen products.
    assert_eq!(tables.products.lists[0].len(), 13);
    assert_eq!(tables.products.lists[1].len(), 15);
    let truck = tables.product(0, 0x13).unwrap();
    assert_eq!(
        (truck.virtual_address, truck.id, truck.kind, truck.name),
        (0x0053_2254, 2, 9, 0x17c)
    );
    let turbo = tables.product(0, 0x12).unwrap();
    assert_eq!(
        (turbo.id, turbo.amount, turbo.build_seconds, turbo.costs),
        (3, 3, 0, [0, 0, 5, 0])
    );
    assert_eq!(
        tables.product(2, 0x12),
        Err(FactoryShopError::Product {
            kind: 2,
            index: 0x12
        })
    );
    assert_eq!(tables.upgrade_ids, [0x32, 0x9b, 0x98, 0xa1, 0xa0, 0x25, 0]);
}

/// Infest01 `BB919D:356`, the first factory's Options on docking.
#[test]
fn options_match_infest01() {
    let (rows, layouts_, selection) = plan(
        &factory(0, &FIRST_FACTORY_MASK),
        &player(&FIRST_PLAYER_BITS, [0; 3]),
    );
    assert_eq!(
        rows,
        vec![
            [9, 0x453a80, 0x2f6, 0, 0, 0, 0, 0],
            [9, 0x453a80, 0x308, 0, 0, 0, 0, 0],
            [1, 0x453a80, 0x2f8, 0, 0x42f630, 1, 0, 0],
            [1, 0x453a80, 0x2f9, 0, 0x42f630, 2, 0, 0],
            [1, 0x453a80, 0x305, 0, 0x42f780, 0, 0, 0],
            [1, 0x453a80, 0x2fa, 0, 0x42f620, 0, 0, 0],
        ]
    );
    assert_eq!(layouts_, layouts(4, 0));
    assert_eq!(selection, 2);
}

/// Infest01 `BDAFC9:51`: Energy Distribution. Only upgrade 0x32 and the
/// always-owned ID 0 leave their rows enabled.
#[test]
fn energy_distribution_matches_infest01() {
    let (rows, layouts_, selection) = plan(
        &factory(2, &FIRST_FACTORY_MASK),
        &player(&FIRST_PLAYER_BITS, [0; 3]),
    );
    let mut expected = vec![
        [9, 0x453a80, 0x2f6, 0, 0, 0, 0, 0],
        [9, 0x42fc00, 0x2f9, 0, 0, 0, 0, 0],
    ];
    for (level, flags) in [1, 0x49, 0x49, 0x49, 0x49, 0x49, 1].into_iter().enumerate() {
        let level = level as u32;
        expected.push([flags, 0x430ab0, level, 0, 0x42f800, level, 0, 0]);
    }
    expected.push([1, 0x453a80, 0x306, 0, 0x42f620, 0, 0, 0]);
    assert_eq!(rows, expected);
    assert_eq!(layouts_, layouts(7, 1));
    assert_eq!(selection, 2);
}

/// Infest01 `C192F4:17A` and `DA7EFE:51`: each factory's Facilities list
/// one product, the truck (index 0x13) and then turbo (index 0x12, paid
/// from the second token counter).
#[test]
fn facilities_match_infest01() {
    for (mask, bits, tokens, product, index) in [
        (
            &FIRST_FACTORY_MASK,
            &FIRST_PLAYER_BITS,
            [0, 0, 0],
            0x532254,
            0x13,
        ),
        (
            &SECOND_FACTORY_MASK,
            &SECOND_PLAYER_BITS,
            [0, 8, 0],
            0x532240,
            0x12,
        ),
    ] {
        let factory = factory(1, mask);
        let player = player(bits, tokens);
        assert_eq!(listed_product_count(&tables(), &factory, &player), Ok(1));
        let (rows, layouts_, selection) = plan(&factory, &player);
        assert_eq!(
            rows,
            vec![
                [9, 0x453a80, 0x2f6, 0, 0, 0, 0, 0],
                [9, 0x42fd70, 0x30a, 0, 0, 0, 0, 0],
                [1, 0x42f870, 0x104, product, 0x42f7e0, index, 0, 0],
                [1, 0x453a80, 0x306, 0, 0x42f620, 0, 0, 0],
            ]
        );
        assert_eq!(layouts_, layouts(1, 1));
        assert_eq!(selection, 2);
    }
}

/// An unaffordable product keeps its row, disabled.
#[test]
fn unaffordable_products_are_disabled() {
    let factory = factory(1, &SECOND_FACTORY_MASK);
    let (rows, _, _) = plan(&factory, &player(&SECOND_PLAYER_BITS, [0, 4, 0]));
    assert_eq!(
        rows[2],
        [0x41, 0x42f870, 0x104, 0x532240, 0x42f7e0, 0x12, 0, 0]
    );
}

/// Infest01 `C1DFAF:136`, `C1DFBE:5BE` and `C5399C:15A7`: the build, the
/// fitting and the recovery show only their title.
#[test]
fn production_states_match_infest01() {
    for (state, title) in [
        (3, [9, 0x430e30, 0x2f6, 0, 0, 0, 0, 0]),
        (4, [9, 0x430d70, 0x2f6, 0, 0, 0, 0, 0]),
        (5, [9, 0x430d70, 0x2f6, 0, 0, 0, 0, 0]),
        (6, [9, 0x453a80, 0x302, 0, 0, 0, 0, 0]),
    ] {
        let (rows, layouts_, selection) = plan(
            &factory(state, &FIRST_FACTORY_MASK),
            &player(&FIRST_PLAYER_BITS, [0; 3]),
        );
        assert_eq!(rows, vec![[9, 0x453a80, 0x2f6, 0, 0, 0, 0, 0], title]);
        assert_eq!(layouts_, layouts(0, 0));
        assert_eq!(selection, 1);
    }
}

/// Not reached in Infest01 (both factories were fully staffed): a factory
/// short of scientists shows its staff row, Beam down scientist, Evacuate
/// once a scientist is inside, and Disconnect. One that needs none drops
/// Evacuate from the Options.
#[test]
fn staff_shapes_follow_00431000() {
    let tables = tables();
    let bits = FIRST_PLAYER_BITS;
    let player = player(&bits, [0; 3]);
    let words = |staff_max: i32, staff: i32| {
        let plan = plan_shop_menu(
            &tables,
            &ShopFactory {
                staff_13c: staff_max,
                staff_140: staff,
                ..factory(0, &FIRST_FACTORY_MASK)
            },
            Some(&player),
        )
        .unwrap();
        let (rows, layouts) = plan.rows.unwrap();
        let titles: Vec<_> = rows
            .iter()
            .map(|row| (row.draw_callback, row.draw_arguments[0]))
            .collect();
        (titles, layouts[2].item_count)
    };
    let staff_row = (
        0x430c50,
        tables.row(Template::Staff as usize).draw_arguments[0],
    );
    assert_eq!(
        words(2, 0),
        (
            vec![
                (0x453a80, 0x2f6),
                staff_row,
                (0x453a80, 0x310),
                (0x453a80, 0x2fa)
            ],
            2
        )
    );
    assert_eq!(
        words(2, 1),
        (
            vec![
                (0x453a80, 0x2f6),
                staff_row,
                (0x453a80, 0x310),
                (0x453a80, 0x305),
                (0x453a80, 0x2fa)
            ],
            3
        )
    );
    assert_eq!(
        words(0, 0),
        (
            vec![
                (0x453a80, 0x2f6),
                (0x453a80, 0x308),
                (0x453a80, 0x2f8),
                (0x453a80, 0x2f9),
                (0x453a80, 0x2fa)
            ],
            3
        )
    );
}

#[test]
fn enemy_factories_retitle_the_header() {
    let plan = plan_shop_menu(
        &tables(),
        &ShopFactory {
            kind_15c: 1,
            ..factory(6, &FIRST_FACTORY_MASK)
        },
        Some(&player(&FIRST_PLAYER_BITS, [0; 3])),
    )
    .unwrap();
    assert_eq!(plan.rows.unwrap().0[0].draw_arguments[0], 0x2f7);
}

#[test]
fn a_missing_player_leaves_the_menu_unbound() {
    let plan = plan_shop_menu(&tables(), &factory(0, &FIRST_FACTORY_MASK), None).unwrap();
    assert_eq!(
        plan,
        ShopMenuPlan {
            rows: None,
            initial_selection: 0
        }
    );
    let data = plan.menu_data(&tables());
    assert!(!data.records().has_items());
}

#[test]
fn states_past_six_are_outside_the_builder() {
    assert_eq!(
        plan_shop_menu(
            &tables(),
            &factory(7, &FIRST_FACTORY_MASK),
            Some(&player(&FIRST_PLAYER_BITS, [0; 3]))
        ),
        Err(FactoryShopError::State(7))
    );
}

/// `00431950`: Standard Grenades (0x25) read weapon entry 8.
#[test]
fn weapon_products_compare_the_stock() {
    let tables = tables();
    let grenades = *tables.product(0, 10).unwrap();
    assert_eq!((grenades.id, grenades.amount), (0x25, 8));
    let mut entries = vec![[0u32; 8]; 16];
    let bits = FIRST_PLAYER_BITS;
    let mut player = player(&bits, [0; 3]);
    assert_eq!(product_is_valid(&grenades, &player), Ok(true));
    entries[8][0] = 8;
    player.weapon_entries = Some(&entries);
    assert_eq!(product_is_valid(&grenades, &player), Ok(false));
    let short = vec![[8u32; 8]; 8];
    player.weapon_entries = Some(&short);
    assert_eq!(
        product_is_valid(&grenades, &player),
        Err(FactoryShopError::WeaponEntry { index: 8, count: 8 })
    );
    // Non-weapon products are always valid.
    assert_eq!(
        product_is_valid(tables.product(0, 0x13).unwrap(), &player),
        Ok(true)
    );
}

#[test]
fn menu_data_keeps_the_layout_source() {
    let tables = tables();
    let plan = plan_shop_menu(
        &tables,
        &factory(0, &FIRST_FACTORY_MASK),
        Some(&player(&FIRST_PLAYER_BITS, [0; 3])),
    )
    .unwrap();
    let data = plan.menu_data(&tables);
    assert_eq!(
        data.source(),
        crate::menu::MenuSource::Derived {
            constructor: 0x0043_1000,
            vtable: 0x0052_1a14
        }
    );
    assert_eq!(
        data.layout_source().map(|source| source.address),
        Some(0x0053_25b8)
    );
    assert_eq!(data.records().items.len(), 6);
    assert_eq!(data.records().initial_selection, 2);
}
