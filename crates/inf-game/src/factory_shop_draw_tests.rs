//! The shop's row draws against Infest01's (Level 1's truck factory, the
//! `shopdraw` receipts at 1024 by 768), through a host that records them.

use std::collections::HashMap;
use std::path::Path;

use super::*;
use crate::{
    factory_shop::{plan_shop_menu, ShopFactory},
    frontend_assets::FrontendAssets,
    menu::{MenuBasis, MenuDrawPlacement, MenuSource},
    menu_text::{measure_text_width, MenuTextResources},
    render_resources::RenderResources,
    Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
};

/// Infest01's style words, by the colours its draws used.
const STYLES: [(i32, u32); 12] = [
    (0, 0x486c21),
    (1, 0x304816),
    (5, 0x3d5c1c),
    (7, 0),
    (8, 0x121212),
    (0x21, 0x0a0b0c),
    (0x22, 0x602000),
    (0x23, 0x003800),
    (0x24, 0x08084e),
    (0x27, 0xffb45a),
    (0x28, 0x4040ff),
    (0x29, 0x40ff40),
];

const STRINGS: [(i32, &str); 13] = [
    (0x14c, "Cannons"),
    (0x17c, "Truck mode"),
    (0x17e, "Turbo boost"),
    // Stand-ins for two products' names.
    (0x1ec, "Product 0x11"),
    (0x1ee, "Product 0x25"),
    (0x2fd, "Manufacturing %s"),
    (0x300, "Insufficient Staff"),
    (0x301, "Fitting %s... Please wait"),
    (0x309, "Spare Tokens"),
    (0x30b, "Duration: %d secs "),
    (0x30d, "Fully Stocked"),
    (0x30e, "Insufficient Crystals"),
    (0x307, "Shielding"),
];

/// Product records: Truck mode, Turbo boost (five of the second token,
/// no build time), product 0x11 (two of the first, five seconds) and the
/// weapon 0x25 (amount 8, two of the second, three seconds).
const TRUCK_MODE: u32 = 0x0053_2254;
const TURBO_BOOST: u32 = 0x0053_2240;
const PRODUCT_11: u32 = 0x0053_2268;
const WEAPON_25: u32 = 0x0053_21a0;
const PLAYER: PlayerId = PlayerId::unissued(0);

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Text {
        at: [i16; 2],
        width: i16,
        color: MenuColor,
        left: Option<String>,
        right: Option<String>,
    },
    Quad(DirectSpriteQuad),
    Preview(ShopPreview),
    Command(PlayerId, u32),
}

struct Recorder {
    styles: HashMap<i32, u32>,
    strings: HashMap<i32, &'static str>,
    selector_one: u16,
    events: Vec<Event>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            styles: STYLES.into_iter().collect(),
            strings: STRINGS.into_iter().collect(),
            selector_one: 1,
            events: Vec::new(),
        }
    }
}

impl ShopDrawHost for Recorder {
    type Error = String;

    fn surface(&self) -> [i32; 2] {
        [1024, 768]
    }

    fn style(&self, slot: i32) -> std::result::Result<u32, String> {
        self.styles
            .get(&slot)
            .copied()
            .ok_or(format!("style {slot}"))
    }

    fn string(&self, id: i32) -> std::result::Result<Vec<u8>, String> {
        self.strings
            .get(&id)
            .map(|text| text.as_bytes().to_vec())
            .ok_or(format!("string {id:#x}"))
    }

    fn model_extent(&self, slot: i16) -> std::result::Result<Option<u32>, String> {
        // Model 0x75's first DWORD (the vehicle's radius); 50 for others.
        Ok(Some(if slot == 0x75 { 109 } else { 50 }))
    }

    fn text(
        &mut self,
        descriptor: &mut MenuRowDescriptor,
        _: [i32; 2],
        left: Option<&[u8]>,
        right: Option<&[u8]>,
    ) -> std::result::Result<(), String> {
        let text = |bytes: Option<&[u8]>| bytes.map(|b| String::from_utf8(b.to_vec()).unwrap());
        self.events.push(Event::Text {
            at: descriptor.position_pixels,
            width: descriptor.extent_pixels[0],
            color: descriptor.color,
            left: text(left),
            right: text(right),
        });
        Ok(())
    }

    fn quad(&mut self, quad: DirectSpriteQuad) -> std::result::Result<(), String> {
        self.events.push(Event::Quad(quad));
        Ok(())
    }

    fn preview(&mut self, preview: ShopPreview) -> std::result::Result<(), String> {
        self.events.push(Event::Preview(preview));
        Ok(())
    }

    fn selector_one(&self) -> u16 {
        self.selector_one
    }

    fn set_selector_one(&mut self, value: u16) {
        self.selector_one = value;
    }

    fn command(&mut self, player: PlayerId, raw_id: u32) -> std::result::Result<(), String> {
        self.events.push(Event::Command(player, raw_id));
        Ok(())
    }
}

fn tables() -> ShopDrawTables {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    ShopDrawTables::parse(FrontendExecutable::parse(assets.executable_bytes()).unwrap()).unwrap()
}

/// Infest01's player at the truck factory: ten spare tokens, no counters.
fn player() -> ShopDrawPlayer {
    ShopDrawPlayer {
        id: PLAYER,
        crystals_118: 10,
        tokens_120: [0; 3],
        levels_13c: [0; 8],
        upgrades_100: [0; 6],
        weapon_entries: None,
    }
}

fn state(clock: u32) -> ShopDrawState {
    ShopDrawState {
        factory: ShopDrawFactory {
            staff_13c: 0,
            staff_140: 0,
            build_158: None,
            progress_16c: 0,
            product_18c: None,
        },
        player: Some(player()),
        clock,
    }
}

/// A row as `00452570` hands it over: the descriptor's position, width
/// and height, its reveal word, flags and colour.
fn row(
    callback: u32,
    arguments: [u32; 2],
    [x, y, width, height]: [i16; 4],
    flags: u32,
    color: u32,
) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: MenuSource::Derived {
            constructor: crate::factory_shop::SHOP_MENU_CONSTRUCTOR,
            vtable: crate::factory_shop::SHOP_MENU_VTABLE,
        },
        layout_index: 1,
        item_index: 1,
        callback_address: callback,
        arguments,
        context_origin_pixels: [0, 0],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [x, y],
            extent_pixels: [width, height],
            reveal: 0xffff,
            flags,
            age_millis: 0,
            color: MenuColor::from_word(color),
        },
    }
}

/// The truck factory's Facilities menu: only Truck mode is listed, at
/// row 2, then Back.
fn facilities_menu(tables: &ShopDrawTables) -> MenuState {
    facilities_menu_at(tables, None)
}

fn facilities_menu_at(tables: &ShopDrawTables, selection: Option<i32>) -> MenuState {
    let mut grants = [0u32; 6];
    grants[0] = 1 << 2;
    let factory = ShopFactory {
        state_160: 1,
        kind_15c: 0,
        staff_13c: 0,
        staff_140: 0,
        grants_198: &grants,
    };
    let owner = player();
    let mut plan = plan_shop_menu(&tables.shop, &factory, Some(&owner.shop_player())).unwrap();
    if let Some(selection) = selection {
        plan.initial_selection = selection;
    }
    MenuState::new(plan.menu_data(&tables.shop), MenuBasis::INITIAL)
}

/// The Facilities menu with `product` in the listed row.
fn selecting(tables: &ShopDrawTables, product: u32) -> MenuState {
    let menu = facilities_menu(tables);
    menu.data().records_mut().items[2].draw_arguments[1] = product;
    menu
}

fn facilities(menu: &MenuState, tables: &ShopDrawTables, state: &ShopDrawState) -> Vec<Event> {
    let mut request = row(
        FACILITIES_TITLE_DRAW,
        [0x30a, 0],
        [494, 102, 510, 92],
        0,
        0x3d5c1c,
    );
    draw(&mut request, menu, tables, state).events
}

fn is_text(event: &Event, line: &str) -> bool {
    matches!(event, Event::Text { left: Some(left), .. } if left == line)
}

fn draw(
    request: &mut MenuRowDrawRequest,
    menu: &MenuState,
    tables: &ShopDrawTables,
    state: &ShopDrawState,
) -> Recorder {
    let mut recorder = Recorder::new();
    assert!(draw_shop_row(request, menu, ShopDraw { tables, state }, &mut recorder).unwrap());
    recorder
}

fn bar_quads(corners: [[[i16; 2]; 4]; 2], fill: u32, background: u32) -> [Event; 2] {
    let quad = |corners, color_word| {
        Event::Quad(DirectSpriteQuad {
            sprite_slot: 0x22b,
            corners,
            vertex_depths: [0x40; 4],
            descriptor_word: 0,
            mode: DirectSpriteMode::Effect { color_word },
        })
    };
    [quad(corners[0], fill), quad(corners[1], background)]
}

fn text(at: [i16; 2], width: i16, color: u32, left: &str, right: Option<&str>) -> Event {
    Event::Text {
        at,
        width,
        color: MenuColor::from_word(color),
        left: Some(left.into()),
        right: right.map(Into::into),
    }
}

#[test]
fn the_energy_distribution_title_matches_infest01() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    let mut request = row(
        DISTRIBUTION_TITLE_DRAW,
        [0x2f9, 0],
        [494, 102, 510, 92],
        0,
        0x3d5c1c,
    );
    let recorder = draw(&mut request, &menu, &tables, &state(0));
    let [fill, background] = bar_quads(
        [
            [[812, 102], [822, 102], [822, 86], [812, 86]],
            [[914, 102], [822, 102], [822, 86], [914, 86]],
        ],
        0x486c21,
        0x304816,
    );
    assert_eq!(
        recorder.events,
        [
            text([494, 102], 510, 0x3d5c1c, "Spare Tokens", Some("10")),
            fill,
            background,
        ]
    );
}

#[test]
fn token_levels_match_infest01() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    // Cannons, selected, at level 0: the bar's fill rises nothing.
    let mut request = row(LEVEL_ROW_DRAW, [0, 0], [494, 194, 510, 256], 1, 0x80b460);
    let recorder = draw(&mut request, &menu, &tables, &state(0));
    let [fill, background] = bar_quads(
        [
            [[812, 194], [914, 194], [914, 194], [812, 194]],
            [[812, 178], [914, 178], [914, 194], [812, 194]],
        ],
        0x486c21,
        0x304816,
    );
    assert_eq!(
        recorder.events,
        [
            text([494, 194], 510, 0x80b460, "Cannons", Some("0%")),
            fill,
            background,
        ]
    );
    // Shielding at 37: a signed byte across 37/500 of the width.
    let mut levels = state(0);
    levels.player.as_mut().unwrap().levels_13c[6] = 37;
    let mut request = row(LEVEL_ROW_DRAW, [6, 0], [494, 382, 510, 256], 0, 0x3d5c1c);
    let recorder = draw(&mut request, &menu, &tables, &levels);
    assert_eq!(
        recorder.events[0],
        text([494, 382], 510, 0x3d5c1c, "Shielding", Some("37%"))
    );
    assert_eq!(
        recorder.events[1..],
        bar_quads(
            [
                [[812, 382], [849, 382], [849, 366], [812, 366]],
                [[914, 382], [849, 382], [849, 366], [914, 366]],
            ],
            0x486c21,
            0x304816,
        )
    );
    let mut past = row(LEVEL_ROW_DRAW, [7, 0], [494, 382, 510, 256], 0, 0x3d5c1c);
    let error = draw_shop_row(
        &mut past,
        &menu,
        ShopDraw {
            tables: &tables,
            state: &levels,
        },
        &mut Recorder::new(),
    )
    .unwrap_err();
    assert!(matches!(error, ShopDrawError::LevelIndex(7)));
}

#[test]
fn the_product_row_matches_infest01() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    let mut request = row(
        PRODUCT_ROW_DRAW,
        [0x104, TRUCK_MODE],
        [494, 194, 510, 256],
        1,
        0x80b460,
    );
    let recorder = draw(&mut request, &menu, &tables, &state(0));
    // The name in four fifths of the width, then the three costs over the
    // whole width; the descriptor keeps the last colour.
    assert_eq!(
        recorder.events,
        [
            text([494, 194], 408, 0x80b460, "Truck mode", None),
            text([494, 194], 510, 0xffb45a, "", Some("0    ")),
            text([494, 194], 510, 0x40ff40, "", Some("0  ")),
            text([494, 194], 510, 0x4040ff, "", Some("0")),
        ]
    );
    assert_eq!(request.descriptor.extent_pixels[0], 510);
    // Turbo boost costs five of the second counter.
    let mut request = row(
        PRODUCT_ROW_DRAW,
        [0x104, TURBO_BOOST],
        [494, 226, 510, 256],
        0,
        0x303c15,
    );
    let recorder = draw(&mut request, &menu, &tables, &state(0));
    assert_eq!(
        recorder.events[0],
        text([494, 226], 408, 0x303c15, "Turbo boost", None)
    );
    assert_eq!(
        recorder.events[2],
        text([494, 226], 510, 0x40ff40, "", Some("5  "))
    );
}

#[test]
fn long_product_names_are_cut_to_seventeen_and_dots() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    let mut recorder = Recorder::new();
    recorder.strings.insert(0x17c, "Transformation to truck");
    let mut request = row(
        PRODUCT_ROW_DRAW,
        [0x104, TRUCK_MODE],
        [494, 194, 510, 256],
        0,
        0,
    );
    let state = state(0);
    draw_shop_row(
        &mut request,
        &menu,
        ShopDraw {
            tables: &tables,
            state: &state,
        },
        &mut recorder,
    )
    .unwrap();
    assert_eq!(
        recorder.events[0],
        text([494, 194], 408, 0, "Transformation to...", None)
    );
    // A record no product list holds stops the row.
    let mut stray = row(
        PRODUCT_ROW_DRAW,
        [0x104, 0x0053_2250],
        [494, 194, 510, 256],
        0,
        0,
    );
    let error = draw_shop_row(
        &mut stray,
        &menu,
        ShopDraw {
            tables: &tables,
            state: &state,
        },
        &mut Recorder::new(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ShopDrawError::ProductRecord {
            address: 0x0053_2250,
            ..
        }
    ));
}

/// Infest01's Facilities title for Truck mode, at clock 0x46E: the
/// vehicle's model as the truck, both counters' bars per token, and the
/// name; Truck mode has no build time, so no status.
#[test]
fn the_facilities_title_matches_infest01() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    assert_eq!(menu.selection(), 2);
    let mut request = row(
        FACILITIES_TITLE_DRAW,
        [0x30a, 0],
        [494, 102, 510, 92],
        0,
        0x3d5c1c,
    );
    let recorder = draw(&mut request, &menu, &tables, &state(0x46e));
    let mut expected = vec![Event::Preview(ShopPreview {
        model_slot: 0x75,
        center: [545, -78],
        fade: [3154, 3654],
        eye_y: 461,
        position: [0, 20, 1154],
        angle: 0x46e << 6,
        selector_one: 2,
    })];
    for (x, fill) in [(909, 0x602000), (947, 0x003800), (985, 0x08084e)] {
        let end = x + 19;
        let level = [[x, 102], [end, 102], [end, 102], [x, 102]];
        let column = [[x, 54], [end, 54], [end, 102], [x, 102]];
        expected.extend(bar_quads([level, column], fill, 0x121212));
        expected.extend(bar_quads([level, column], fill, 0));
    }
    expected.push(text([596, 126], 408, 0x3d5c1c, "Truck mode", None));
    assert_eq!(recorder.events, expected);
    assert_eq!(recorder.selector_one, 2);
    assert_eq!(request.descriptor.position_pixels, [596, 126]);
}

/// Turbo boost, unpaid: while `005471C0 / 25` is odd, "Insufficient
/// Crystals" a 64th of the surface higher and the unpaid cost's bar; when
/// even, neither. Its model is not the vehicle's, so the form word stays.
#[test]
fn the_facilities_title_warns_on_odd_blinks() {
    let tables = tables();
    let menu = selecting(&tables, TURBO_BOOST);
    let mut short = state(25);
    let events = facilities(&menu, &tables, &short);
    assert_eq!(
        events[0],
        Event::Preview(ShopPreview {
            model_slot: 0x107,
            center: [545, -78],
            fade: [2800, 3300],
            eye_y: 320,
            position: [0, 0, 800],
            angle: 25 << 6,
            selector_one: 1,
        })
    );
    assert_eq!(
        events[1],
        text([494, 90], 510, 0x3d5c1c, "Insufficient Crystals", None)
    );
    // Every counter and every cost, then the name.
    assert_eq!(events.len(), 1 + 1 + 12 + 1);
    short.clock = 50;
    let events = facilities(&menu, &tables, &short);
    assert!(!events
        .iter()
        .any(|event| is_text(event, "Insufficient Crystals")));
    // The second token's cost bar is gone with the warning.
    assert_eq!(events.len(), 1 + 10 + 1);
    // Paid for, without a build time: no status ("Crystals Required" is
    // formatted, never drawn).
    short.player.as_mut().unwrap().tokens_120 = [0, 5, 0];
    let events = facilities(&menu, &tables, &short);
    assert_eq!(events.len(), 1 + 12 + 1);
}

/// Product 0x11, paid for: its build time, steadily.
#[test]
fn the_facilities_title_shows_a_paid_products_duration() {
    let tables = tables();
    let menu = selecting(&tables, PRODUCT_11);
    let mut paid = state(50);
    paid.player.as_mut().unwrap().tokens_120 = [2, 0, 0];
    let events = facilities(&menu, &tables, &paid);
    assert_eq!(
        events[1],
        text([494, 90], 510, 0x3d5c1c, "Duration: 5 secs ", None)
    );
    paid.clock = 25;
    assert_eq!(facilities(&menu, &tables, &paid)[1], events[1]);
}

/// A weapon stocked to its amount (`00431950`): "Fully Stocked" while the
/// blink is odd.
#[test]
fn the_facilities_title_blinks_a_stocked_weapon() {
    let tables = tables();
    let menu = selecting(&tables, WEAPON_25);
    let mut stocked = state(25);
    let owner = stocked.player.as_mut().unwrap();
    owner.tokens_120 = [0, 2, 0];
    let mut entries = vec![[0u32; 8]; 16];
    entries[8][0] = 8;
    owner.weapon_entries = Some(entries);
    let events = facilities(&menu, &tables, &stocked);
    assert_eq!(
        events[1],
        text([494, 90], 510, 0x3d5c1c, "Fully Stocked", None)
    );
    stocked.clock = 50;
    let events = facilities(&menu, &tables, &stocked);
    assert!(!events.iter().any(|event| is_text(event, "Fully Stocked")));
    // One short of its amount: its duration.
    let owner = stocked.player.as_mut().unwrap();
    owner.weapon_entries.as_mut().unwrap()[8][0] = 7;
    let events = facilities(&menu, &tables, &stocked);
    assert_eq!(
        events[1],
        text([494, 90], 510, 0x3d5c1c, "Duration: 3 secs ", None)
    );
}

#[test]
fn back_selected_leaves_the_facilities_title_empty() {
    let tables = tables();
    let menu = facilities_menu_at(&tables, Some(3));
    let mut request = row(
        FACILITIES_TITLE_DRAW,
        [0x30a, 0],
        [494, 102, 510, 92],
        0,
        0x3d5c1c,
    );
    let recorder = draw(&mut request, &menu, &tables, &state(0));
    assert!(recorder.events.is_empty());
    assert_eq!(request.descriptor.position_pixels, [494, 102]);
}

#[test]
fn production_rows_name_the_grant() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    let mut fitting = state(0);
    fitting.factory.product_18c = Some(2);
    let mut request = row(
        FITTING_PROGRESS_DRAW,
        [0x2f6, 0],
        [494, 102, 510, 92],
        1,
        0x80b460,
    );
    let recorder = draw(&mut request, &menu, &tables, &fitting);
    assert_eq!(
        recorder.events,
        [text(
            [494, 102],
            510,
            0x80b460,
            "Fitting Truck mode... Please wait",
            None
        )]
    );
    // Half of three seconds built: a bar from a sixth across, two thirds
    // long, a 24th of the surface above the row's bottom.
    fitting.factory.build_158 = Some(3_000_000);
    fitting.factory.progress_16c = 1_500_000;
    let mut request = row(
        BUILD_PROGRESS_DRAW,
        [0x2f6, 0],
        [494, 102, 510, 92],
        0,
        0x3d5c1c,
    );
    let recorder = draw(&mut request, &menu, &tables, &fitting);
    let mut expected = bar_quads(
        [
            [[579, 162], [749, 162], [749, 146], [579, 146]],
            [[919, 162], [749, 162], [749, 146], [919, 146]],
        ],
        0x486c21,
        0x0a0b0c,
    )
    .to_vec();
    expected.push(text(
        [494, 102],
        510,
        0x3d5c1c,
        "Manufacturing Truck mode",
        None,
    ));
    assert_eq!(recorder.events, expected);
    // Without a build time the bar is full.
    fitting.factory.build_158 = Some(0);
    let recorder = draw(&mut request, &menu, &tables, &fitting);
    assert!(matches!(&recorder.events[0], Event::Quad(quad) if quad.corners[1][0] == 919));
}

#[test]
fn the_staff_row_blinks_and_sets_its_command() {
    let tables = tables();
    let menu = facilities_menu(&tables);
    let mut staffed = state(0);
    staffed.factory.staff_13c = 3;
    staffed.factory.staff_140 = 1;
    let mut request = row(STAFF_ROW_DRAW, [0, 0], [494, 102, 510, 92], 0, 0x3d5c1c);
    let recorder = draw(&mut request, &menu, &tables, &staffed);
    assert_eq!(
        recorder.events,
        [
            Event::Command(PLAYER, STAFF_COMMAND),
            text([494, 102], 510, 0x4040ff, "Insufficient Staff (1/3)", None),
        ]
    );
    staffed.clock = 25;
    let recorder = draw(&mut request, &menu, &tables, &staffed);
    assert_eq!(recorder.events, [Event::Command(PLAYER, STAFF_COMMAND)]);
}

/// Font 0 at 1024 by 768 right-aligns the receipts' right texts on the
/// 494..1004 rows where Infest01 drew them.
#[test]
fn right_texts_align_where_infest01_drew_them() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 4,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let resources = RenderResources::new(&runtime);
    let font = resources.font(0).unwrap();
    for (text, x) in [
        (&b"10"[..], 984),
        (b"0%", 974),
        (b"0    ", 949),
        (b"0  ", 970),
        (b"0", 991),
    ] {
        let width = i32::from(measure_text_width(font, text).unwrap() as i16);
        assert_eq!(494 + 510 - width, x, "{text:?}");
    }
}

#[test]
fn formats_take_only_the_shops_directives() {
    assert_eq!(
        format::<()>(
            0,
            b"%s (%d/%d)\0junk",
            &[Arg::Text(b"A\0B"), Arg::Int(-1), Arg::Int(2)]
        )
        .unwrap(),
        b"A (-1/2)"
    );
    assert_eq!(format::<()>(0, b"%d%%", &[Arg::Int(5)]).unwrap(), b"5%");
    assert!(format::<()>(0, b"%d", &[]).is_err());
    assert!(format::<()>(0, b"%x", &[Arg::Int(5)]).is_err());
    assert!(format::<()>(0, b"%s", &[Arg::Int(5)]).is_err());
}
