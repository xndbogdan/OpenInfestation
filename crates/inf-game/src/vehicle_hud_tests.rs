use super::*;

fn quads(ops: &[HudOp]) -> Vec<&DirectSpriteQuad> {
    ops.iter()
        .filter_map(|op| match op {
            HudOp::Quad(quad) => Some(quad),
            _ => None,
        })
        .collect()
}

fn corners(quad: &DirectSpriteQuad) -> [[i16; 2]; 4] {
    quad.corners
}

#[test]
fn small_gauges_frame_at_seven_eighths_and_fill_a_disc() {
    let mut ops = Vec::new();
    // 640x480's health gauge: size 24, half full.
    gauge(&mut ops, [0x200, 0x200], [507, 375], 24, 0x4000, 0x2040).unwrap();
    let drawn = quads(&ops);
    assert_eq!(drawn.len(), 2);
    assert_eq!(drawn[0].sprite_slot, 0x2ac);
    assert_eq!(
        corners(drawn[0]),
        [[486, 354], [528, 354], [528, 396], [486, 396]]
    );
    assert_eq!(drawn[0].mode, DirectSpriteMode::Textured);
    assert_eq!(drawn[0].vertex_depths, [0x4000; 4]);
    assert_eq!(drawn[0].descriptor_word, 0);
    // 24 * 6 / 8 = 18, times half = 9.
    assert_eq!(drawn[1].sprite_slot, 0x2ab);
    assert_eq!(
        corners(drawn[1]),
        [[498, 366], [516, 366], [516, 384], [498, 384]]
    );
    assert_eq!(
        drawn[1].mode,
        DirectSpriteMode::Effect { color_word: 0x2040 }
    );
}

#[test]
fn large_gauges_draw_four_mirrored_quarters_and_no_empty_fill() {
    let mut ops = Vec::new();
    gauge(&mut ops, [0x200, 0x200], [788, 691], 61, 0, 7).unwrap();
    let drawn = quads(&ops);
    assert_eq!(drawn.len(), 4);
    assert!(drawn.iter().all(|quad| quad.sprite_slot == 0x2a6));
    assert_eq!(
        drawn.iter().map(|quad| corners(quad)).collect::<Vec<_>>(),
        [
            [[788, 630], [727, 630], [727, 691], [788, 691]],
            [[788, 630], [849, 630], [849, 691], [788, 691]],
            [[788, 752], [727, 752], [727, 691], [788, 691]],
            [[788, 752], [849, 752], [849, 691], [788, 691]],
        ]
    );
}

#[test]
fn fills_keep_at_least_one_pixel_and_follow_the_focal_ratio() {
    let mut ops = Vec::new();
    gauge(&mut ops, [0x400, 0x200], [100, 100], 10, 1, 0).unwrap();
    let fill = quads(&ops)[1];
    // (10 * 6 / 8) * 1 >> 15 = 0, kept at one; twice as wide.
    assert_eq!(corners(fill), [[98, 99], [102, 99], [102, 101], [98, 101]]);
    assert_eq!(
        gauge(&mut ops, [1, 0], [0, 0], 10, 0, 0),
        Err(VehicleHudError::Division {
            numerator: 10,
            divisor: 0
        })
    );
}

#[test]
fn counter_fills_ease_through_the_quarter_sine() {
    let sine: [u16; 4096] = std::array::from_fn(|index| index as u16);
    // 15 of 30 is 0x4000; half of it is the angle 0x2000.
    assert_eq!(eased(((15u32 << 15) as i32) / 30, &sine), 0x800);
    assert_eq!(eased(0x10000, &sine), sine_q15(0x4000, &sine));
}

fn sine() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

fn basis(words: [u32; 9]) -> [[f32; 3]; 3] {
    std::array::from_fn(|axis| std::array::from_fn(|i| f32::from_bits(words[axis * 3 + i])))
}

/// Level 1's five map annotations, position X/Z and label, in `0055F170`
/// order (bundle `.tmp/infestation-ttd/20261008-172740-943-radarinputs`).
const LEVEL1_ANNOTATIONS: [([u16; 2], u32); 5] = [
    ([32682, 34243], 1),
    ([46730, 26983], 2),
    ([17584, 34883], 3),
    ([11476, 35786], 4),
    ([37232, 34143], 2),
];

/// Infest01's published projection at 1024x768 (`0055F22C..0055F24C`), the
/// same in every receipt.
const INFEST01_PROJECTION: ModelProjection = ModelProjection {
    focal: [0x200, 0x200],
    viewport: [0x400, 0x300],
    center: [0x200, 0x180],
    fade: [0x1900, 0x1e00],
};

/// The world draw's view as the HUD's view record holds it: the eye (+4) and
/// the counter and basis (+10..+34), with Infest01's projection and shift 16.
fn hud_projection(eye: [i32; 3], view: [u32; 9]) -> HudProjection {
    let mut words = [0; 10];
    words[1..].copy_from_slice(&view);
    HudProjection {
        view: SceneView {
            eye,
            basis: MenuBasis { words },
        },
        projection: INFEST01_PROJECTION,
        coordinate_shift: 16,
    }
}

/// The RGB of the reticle's styles in every receipt: 1 the sight's,
/// 0x21 the box's and 0 the bar's.
const SIGHT_RGB: u32 = 0x0030_4816;
const BOX_RGB: u32 = 0x002c_2c90;
const BAR_RGB: u32 = 0x0048_6c21;

/// Infest01's HUD inputs at 1024x768 (bundle
/// `.tmp/infestation-ttd/20261008-171424-219-hudreceipt3`; the reticle's
/// from `20261012-targeting`). Style 0x24 is retail's `0008084E` and the
/// reticle's are their quads' RGB; the others, which no planned fill in
/// these calls reads, are zero. H10 slot 0 is 76 by 76.
fn infest01(
    clock: i32,
    view: [u32; 9],
    eye: [i32; 3],
    vehicle: HudVehicle,
    seconds: u32,
) -> HudInputs {
    let eye_words = [eye[0] as u16, eye[2] as u16];
    HudInputs {
        frame: HudFrame {
            surface: [1024, 768],
            focal: [0x200, 0x200],
            layout: 0,
            clock,
            hud_setting: 1,
            font_quarter_height: 4,
            view_basis: basis(view),
            projection: hud_projection(eye, view),
            sine_quarter: sine(),
            styles: HudStyles {
                frame: SIGHT_RGB,
                turbo: BOX_RGB,
                bar: BAR_RGB,
                counters: [0, 0, 0x0008_084e],
            },
            radar_size: [76, 76],
            viewport: [0, 0, 1024, 768],
        },
        vehicle,
        player: HudPlayer {
            message_shown: 0,
            milestones: 0,
            counters: [0; 3],
            turbo: 0,
            radar_upgrades: [false, false, false],
            weapons_owned: [false; 16],
        },
        world: HudWorld {
            milestone_total: 0,
            seconds,
            eye_words,
            annotations: LEVEL1_ANNOTATIONS
                .map(|(position_words, label)| HudAnnotation {
                    position_words,
                    label,
                })
                .to_vec(),
            radar_actors: Vec::new(),
            radar_jitter: 0,
            network: None,
        },
    }
}

fn level1_vehicle(health: u16, mode: u16, notices: [u8; 2]) -> HudVehicle {
    HudVehicle {
        drawn: true,
        health,
        maximum_health: 0x1388,
        mode,
        target: None,
        corners: [[0; 2]; 3],
        position: [0; 3],
        basis_bits: [
            [1f32.to_bits(), 0, 0],
            [0, 1f32.to_bits(), 0],
            [0, 0, 1f32.to_bits()],
        ],
        team: 2,
        sight_direction: None,
        carried: [None; 3],
        dword_218: 0,
        switching: [0, 0],
        switch_timers: [0, 0],
        switch_slides: [0, 0],
        weapons: HudWeapons {
            primary: 0,
            secondary: 8,
            primary_ammo: -1,
            secondary_ammo: 0,
            primary_charge: 0,
            ammo: [0; 16],
        },
        notices,
        dword_5c0: 0x3d090,
    }
}

type Corners = [[i16; 2]; 4];

fn quarters(x: i16, y: i16, half: i16) -> [Corners; 4] {
    let (top, bottom, left, right) = (y - half, y + half, x - half, x + half);
    [
        [[x, top], [left, top], [left, y], [x, y]],
        [[x, top], [right, top], [right, y], [x, y]],
        [[x, bottom], [left, bottom], [left, y], [x, y]],
        [[x, bottom], [right, bottom], [right, y], [x, y]],
    ]
}

/// Call 5's view record eye; the radar reads its X and Z WORDs.
const CALL_5_EYE: [i32; 3] = [0x7d84, 0x216, -0x5cc4];
const CALL_5_EYE_WORDS: [u16; 2] = [0x7d84, 0xa33c];

const CALL_5_VIEW: [u32; 9] = [
    0xbf80_0000,
    0,
    0,
    0,
    0x3f79_1606,
    0xbe6c_60a8,
    0,
    0xbe6c_60a8,
    0xbf79_1606,
];

const CALL_300_EYE: [i32; 3] = [-0x5e21, 0x34, -0x7db5];

const CALL_300_VIEW: [u32; 9] = [
    0x3dbb_72ee,
    0,
    0xbf7e_ecea,
    0x3e5c_70c6,
    0x3f79_f249,
    0x3ca2_1785,
    0x3f78_e564,
    0xbe5d_5ea6,
    0x3db7_1f62,
];

/// Call 5 has no target. Its corners are frame 0's heap residue (the text
/// "mmence.\0Good") five frames into easing toward the sight, and its fire
/// direction is the forward row times 4096.
fn call_5() -> HudInputs {
    infest01(
        0x22,
        CALL_5_VIEW,
        CALL_5_EYE,
        HudVehicle {
            corners: [[7040, 6961], [6551, 241], [7152, 6320]],
            position: [0x7d84, 0x015c, 0xa1bc],
            sight_direction: Some([0, 0, -4096]),
            ..level1_vehicle(0x1388, 1, [0, 0])
        },
        67,
    )
}

/// Infest04's HUD at `5EB074:D3`, in the first-person layout F12 gave it
/// on Artoria: the vehicle at X/Z `5C0C`/`3485` with its basis (`+44`), the
/// camera's eye at `5C19`/`3476` with a view whose up row the camera's tilt
/// moved. Its terrain radar `00470170` got the vehicle's X/Z and basis.
const INFEST04_FIRST_PERSON_VEHICLE_BASIS: [[u32; 3]; 3] = [
    [0xbf3d_da97, 0xb9ff_f8b4, 0xbf2b_bb1b],
    [0x3e8c_05b2, 0x3f69_aefb, 0xbe9b_2233],
    [0x3f1c_cda9, 0xbed0_fadb, 0xbf2d_43cd],
];
const INFEST04_FIRST_PERSON_VIEW: [u32; 9] = [
    0xbf3d_da97,
    0xb9ff_f8b4,
    0xbf2b_bb1b,
    0x3e0f_88d6,
    0x3f7a_484d,
    0xbe1f_687a,
    0x3f27_eb89,
    0xbe56_818d,
    0xbf39_9764,
];

fn infest04_first_person(layout: u8) -> HudInputs {
    let mut inputs = infest01(
        0x22,
        INFEST04_FIRST_PERSON_VIEW,
        [0x5c19, 0x0300, 0x3476],
        HudVehicle {
            position: [0x5c0c, 0x02c0, 0x3485],
            basis_bits: INFEST04_FIRST_PERSON_VEHICLE_BASIS,
            sight_direction: Some([0, 0, -4096]),
            ..level1_vehicle(0x1388, 1, [0, 0])
        },
        67,
    );
    inputs.frame.layout = layout;
    inputs.player.radar_upgrades = [false, true, false];
    inputs
}

fn terrain_radar(ops: &[HudOp]) -> &TerrainRadarOp {
    ops.iter()
        .find_map(|op| match op {
            HudOp::TerrainRadar(op) => Some(op),
            _ => None,
        })
        .unwrap()
}

/// `0041E2CB..0041E33E`: in the first-person layout (F12's First Person 1)
/// the terrain radar centres on the vehicle and turns by its basis, as
/// Infest04's did; the third-person layout keeps the eye and the view. The
/// cockpit layout 2 stays the named omission.
#[test]
fn a_first_person_layout_centres_the_radar_on_the_vehicle() {
    let first = plan_vehicle_hud(&infest04_first_person(1)).unwrap();
    assert!(!first.iter().any(|op| matches!(op, HudOp::Omission(_))));
    let radar = terrain_radar(&first);
    assert_eq!(radar.eye_words, [0x5c0c, 0x3485]);
    let vehicle_x = INFEST04_FIRST_PERSON_VEHICLE_BASIS[0].map(f32::from_bits);
    assert_eq!(radar.rotation, terrain_heading_rotation(vehicle_x));
    let third = plan_vehicle_hud(&infest04_first_person(0)).unwrap();
    let radar = terrain_radar(&third);
    assert_eq!(radar.eye_words, [0x5c19, 0x3476]);
    // The camera copied the vehicle's X row; only the centre moves.
    assert_eq!(radar.rotation, terrain_heading_rotation(vehicle_x));
    assert_eq!(
        plan_vehicle_hud(&infest04_first_person(2)).unwrap(),
        vec![HudOp::Omission(HudOmission::FirstPerson { layout: 2 })]
    );
}

/// Call 300's target is a terrain point (type 8, `0051F154`), which the
/// fire direction reaches from the hull.
fn call_300() -> HudInputs {
    infest01(
        0x756,
        CALL_300_VIEW,
        CALL_300_EYE,
        HudVehicle {
            target: Some(HudTarget {
                kind: 8,
                focus: Some([0xa871, 0xff7e, 0x82dc]),
                actor: None,
            }),
            corners: [[508, 392], [513, 355], [513, 325]],
            position: [0xa375, 0xff7e, 0x8283],
            sight_direction: Some([1276, 0, 89]),
            dword_5c0: 500_000,
            ..level1_vehicle(0x10c6, 2, [1, 0])
        },
        104,
    )
}

/// The reticle's quads: sprite, corners and colour, in order.
fn reticle_quads(ops: &[HudOp]) -> Vec<(u16, Corners, u32)> {
    quads(ops)
        .into_iter()
        .filter(|quad| {
            matches!(
                quad.sprite_slot,
                BRACKET_SPRITE | BAR_FRAME_SPRITE | BAR_SPRITE
            )
        })
        .map(|quad| {
            assert_eq!((quad.vertex_depths, quad.descriptor_word), ([0x40; 4], 0));
            let DirectSpriteMode::Effect { color_word } = quad.mode else {
                panic!("a textured reticle quad: {quad:?}");
            };
            (quad.sprite_slot, quad.corners, color_word)
        })
        .collect()
}

/// A bracket's three retail quads, from its line's and sides' corners as
/// the receipts print them.
fn retail_bracket(corners: [[i16; 8]; 3], colour: u32) -> [(u16, Corners, u32); 3] {
    corners.map(|line| {
        (
            BRACKET_SPRITE,
            std::array::from_fn(|corner| [line[corner * 2], line[corner * 2 + 1]]),
            colour,
        )
    })
}

/// Retail quads of HUD call 5 (AFB9B5:19BC) without the sight: the health
/// frame, the mode icon, both weapon frames, both radar frames, the
/// counters and the turbo frame.
#[test]
fn quads_match_infest01_call_5() {
    let ops = plan_vehicle_hud(&call_5()).unwrap();
    let small = |x: i16, y: i16, h: i16| {
        [
            [x - h, y - h],
            [x + h, y - h],
            [x + h, y + h],
            [x - h, y + h],
        ]
    };
    let mut expected: Vec<(u16, Corners, DirectSpriteMode)> = vec![
        (0x2ac, small(812, 600, 33), DirectSpriteMode::Textured),
        (
            0x2ad,
            [[792, 579], [832, 579], [832, 621], [792, 621]],
            DirectSpriteMode::Effect { color_word: 0xff00 },
        ),
    ];
    for (x, y, half) in [
        (788, 691, 61),
        (236, 691, 61),
        (920, 652, 85),
        (104, 652, 85),
    ] {
        expected.extend(
            quarters(x, y, half).map(|corners| (0x2a6, corners, DirectSpriteMode::Textured)),
        );
    }
    for (x, y) in [(140, 561), (176, 588), (200, 627), (864, 564)] {
        expected.push((0x2ac, small(x, y, 20), DirectSpriteMode::Textured));
    }
    let planned: Vec<_> = quads(&ops)
        .into_iter()
        .filter(|quad| quad.sprite_slot != BRACKET_SPRITE)
        .map(|quad| {
            assert_eq!((quad.vertex_depths, quad.descriptor_word), ([0x4000; 4], 0));
            (quad.sprite_slot, quad.corners, quad.mode)
        })
        .collect();
    assert_eq!(planned, expected);
    assert_eq!(
        ops.iter()
            .filter(|op| **op == HudOp::RadarContents(None))
            .count(),
        1
    );
    let models: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            HudOp::WeaponModel {
                model,
                centre,
                basis_angle,
                distance: 1000,
            } => Some((*model, *centre, *basis_angle)),
            _ => None,
        })
        .collect();
    assert_eq!(
        models,
        [
            (0xb4, [788, 691], 0x22 * 400),
            (0xaf, [236, 691], 0x22 * 400)
        ]
    );
}

/// Retail texts of call 5: the secondary ammo, the milestones, the 32
/// compass marks and the clock. A centred text moves left by half its font
/// width, the same for every equal string.
#[test]
fn texts_match_infest01_call_5() {
    let ops = plan_vehicle_hud(&call_5()).unwrap();
    let retail: [(&[u8], [i32; 2]); 35] = [
        (b"0", [782, 737]),
        (b"0/0", [80, 721]),
        (b"n", [915, 737]),
        (b".", [933, 731]),
        (b".", [948, 726]),
        (b".", [962, 719]),
        (b".", [975, 709]),
        (b".", [985, 696]),
        (b".", [992, 682]),
        (b".", [997, 667]),
        (b"w", [994, 656]),
        (b".", [997, 637]),
        (b".", [992, 622]),
        (b".", [985, 608]),
        (b".", [975, 595]),
        (b".", [962, 585]),
        (b".", [948, 578]),
        (b".", [933, 573]),
        (b"s", [914, 576]),
        (b".", [903, 573]),
        (b".", [888, 578]),
        (b".", [874, 585]),
        (b".", [861, 595]),
        (b".", [851, 608]),
        (b".", [844, 622]),
        (b".", [839, 637]),
        (b"e", [836, 656]),
        (b".", [839, 667]),
        (b".", [844, 682]),
        (b".", [851, 696]),
        (b".", [861, 709]),
        (b".", [874, 719]),
        (b".", [888, 726]),
        (b".", [903, 731]),
        (b"00:01:07", [432, 744]),
    ];
    let texts: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            HudOp::Text {
                text: HudText::Bytes(text),
                position,
            } => Some((text.clone(), *position)),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), retail.len());
    let mut half_widths = std::collections::HashMap::new();
    for ((text, position), (expected, [x, y])) in texts.iter().zip(&retail) {
        assert_eq!(text.as_slice(), *expected);
        match *position {
            HudTextPosition::At(point) => assert_eq!(point, [*x, *y]),
            HudTextPosition::CentredOn([centre, row]) => {
                assert_eq!(row, *y, "{}", String::from_utf8_lossy(text));
                let half = centre - x;
                assert_eq!(*half_widths.entry(text.clone()).or_insert(half), half);
            }
            HudTextPosition::ScreenCentred { .. } | HudTextPosition::CentredUnder { .. } => {
                panic!("call 5 has no notice or switch panel")
            }
        }
    }
}

/// Call 5's sight (AFB9B5:19BC): with no target +5C0 rises, then each ring
/// moves a quarter of the way from the residue toward the forward point and
/// draws in style 1, far off the screen as retail's do.
#[test]
fn the_sight_matches_infest01_call_5() {
    let ops = plan_vehicle_hud(&call_5()).unwrap();
    assert_eq!(ops[0], HudOp::SetReticle5c0(0x3d090 + 50_000));
    let mut expected = Vec::new();
    for bracket in [
        [
            [5393, 5294, 5423, 5294, 5423, 5295, 5393, 5295],
            [5394, 5295, 5408, 5321, 5409, 5321, 5395, 5295],
            [5422, 5295, 5408, 5321, 5409, 5321, 5423, 5295],
        ],
        [
            [5030, 248, 5054, 248, 5054, 249, 5030, 249],
            [5030, 249, 5041, 269, 5042, 269, 5031, 249],
            [5054, 249, 5043, 269, 5044, 269, 5055, 249],
        ],
        [
            [5483, 4806, 5501, 4806, 5501, 4807, 5483, 4807],
            [5483, 4806, 5492, 4822, 5493, 4822, 5484, 4806],
            [5501, 4806, 5492, 4822, 5493, 4822, 5502, 4806],
        ],
    ] {
        expected.extend(retail_bracket(bracket, SIGHT_RGB));
    }
    assert_eq!(reticle_quads(&ops), expected);
    assert_eq!(
        ops[10],
        HudOp::SetReticleCorners([[5408, 5304], [5042, 256], [5492, 4812]])
    );
}

/// Call 300 (CD246E:23FA): the truck at 4294 of 5000 with a terrain point
/// targeted and the water notice. The fill disc, the tint and "Water Hazard"
/// match retail; the point keeps the sight, after +5C0's rise (held at
/// 500000) and a bracket on the first corner, a thousand pixels tall and
/// so off the screen.
#[test]
fn damage_and_notice_match_infest01_call_300() {
    let ops = plan_vehicle_hud(&call_300()).unwrap();
    assert_eq!(ops[3], HudOp::SetReticle5c0(500_000));
    let mut expected = retail_bracket(
        [
            [501, -608, 515, -608, 515, -607, 501, -607],
            [-383, 830, -377, 842, -376, 842, -382, 830],
            [1399, 830, 1393, 842, 1394, 842, 1400, 830],
        ],
        BOX_RGB,
    )
    .to_vec();
    for bracket in [
        [
            [493, 383, 523, 383, 523, 384, 493, 384],
            [494, 384, 508, 410, 509, 410, 495, 384],
            [522, 384, 508, 410, 509, 410, 523, 384],
        ],
        [
            [501, 348, 525, 348, 525, 349, 501, 349],
            [501, 349, 512, 369, 513, 369, 502, 349],
            [525, 349, 514, 369, 515, 369, 526, 349],
        ],
        [
            [504, 319, 522, 319, 522, 320, 504, 320],
            [504, 319, 513, 335, 514, 335, 505, 319],
            [522, 319, 513, 335, 514, 335, 523, 319],
        ],
    ] {
        expected.extend(retail_bracket(bracket, SIGHT_RGB));
    }
    assert_eq!(reticle_quads(&ops), expected);
    let drawn: Vec<_> = quads(&ops)
        .into_iter()
        .filter(|quad| quad.sprite_slot != BRACKET_SPRITE)
        .collect();
    assert_eq!(
        (drawn[1].sprite_slot, drawn[1].corners, drawn[1].mode),
        (
            0x2ab,
            [[809, 597], [815, 597], [815, 603], [809, 603]],
            DirectSpriteMode::Effect { color_word: 0x2040 }
        )
    );
    assert_eq!(
        (drawn[2].sprite_slot, drawn[2].mode),
        (0x2ae, DirectSpriteMode::Effect { color_word: 0xda25 })
    );
    let tail = &ops[ops.len() - 3..];
    assert_eq!(
        tail,
        [
            HudOp::Text {
                text: HudText::Bytes(b"00:01:44".to_vec()),
                position: HudTextPosition::At([432, 744]),
            },
            HudOp::Text {
                text: HudText::String(0x135),
                position: HudTextPosition::ScreenCentred { y: 384 },
            },
            HudOp::ClearNotice(0),
        ]
    );
}

fn radar_labels(ops: &[HudOp]) -> Vec<(Vec<u8>, [i32; 2], u32)> {
    ops.iter()
        .filter_map(|op| match op {
            HudOp::RadarLabel {
                text,
                position,
                color_word,
            } => Some((text.clone(), *position, *color_word)),
            _ => None,
        })
        .collect()
}

/// The radar labels of calls 5 and 300: retail's texts, positions and the
/// style's pulse (`06063A` at clock 0x22, `09095F` at 0x756).
#[test]
fn radar_labels_match_infest01() {
    let label = |text: &str, x, y, colour| (text.as_bytes().to_vec(), [x, y], colour);
    assert_eq!(
        radar_labels(&plan_vehicle_hud(&call_5()).unwrap()),
        [
            label("1", 917, 602, 0x0006_063a),
            label("2", 867, 598, 0x0006_063a),
            label("3", 988, 620, 0x0006_063a),
            label("4", 992, 631, 0x0006_063a),
            label("2", 888, 604, 0x0006_063a),
        ]
    );
    assert_eq!(
        radar_labels(&plan_vehicle_hud(&call_300()).unwrap()),
        [
            label("1", 910, 707, 0x0009_095f),
            label("2", 964, 622, 0x0009_095f),
            label("3", 909, 727, 0x0009_095f),
            label("4", 907, 726, 0x0009_095f),
            label("2", 912, 681, 0x0009_095f),
        ]
    );
}

#[test]
fn the_scanner_walks_the_actors_and_the_radar_draws_the_terrain() {
    let mut inputs = call_5();
    inputs.player.radar_upgrades = [true, false, true];
    inputs.world.radar_jitter = 9;
    let ops = plan_vehicle_hud(&inputs).unwrap();
    let pass = ops
        .iter()
        .find_map(|op| match op {
            HudOp::RadarContents(Some(pass)) => Some(pass),
            _ => None,
        })
        .unwrap();
    assert_eq!(pass.jitter, 9);
    assert!(pass.resource_scanner);
    assert_eq!(pass.eye_words, CALL_5_EYE_WORDS);
    assert_eq!(radar_labels(&ops).len(), 5);
    // The Radar replaces the first radar and its frame: the terrain radar,
    // then its contents, which always walk the actors.
    let frames = quads(&ops).len();
    inputs.player.radar_upgrades = [false, true, false];
    let ops = plan_vehicle_hud(&inputs).unwrap();
    let terrain = ops
        .iter()
        .find_map(|op| match op {
            HudOp::TerrainRadar(op) => Some(*op),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        terrain,
        TerrainRadarOp {
            centre: [920, 652],
            eye_words: CALL_5_EYE_WORDS,
            rotation: crate::terrain_radar::terrain_heading_rotation(inputs.frame.view_basis[0]),
            jitter: 9,
            viewport: [0, 0, 1024, 768],
        }
    );
    let pass = ops
        .iter()
        .find_map(|op| match op {
            HudOp::RadarContents(Some(pass)) => Some(pass),
            _ => None,
        })
        .unwrap();
    assert_eq!(pass.origin, [844, 576]);
    assert!(!pass.resource_scanner);
    assert_eq!(radar_labels(&ops).len(), 5);
    assert_eq!(quads(&ops).len(), frames - 4);
    // A jitter of 0x4000 or more leaves out the contents, labels and all.
    inputs.world.radar_jitter = 0x4000;
    let ops = plan_vehicle_hud(&inputs).unwrap();
    assert!(ops.iter().any(|op| matches!(op, HudOp::TerrainRadar(_))));
    assert!(!ops.iter().any(|op| matches!(op, HudOp::RadarContents(_))));
    assert!(radar_labels(&ops).is_empty());
    // So does a corner left of the clip rectangle, which draws nothing.
    inputs.world.radar_jitter = 9;
    inputs.frame.viewport = [845, 0, 1024, 768];
    let ops = plan_vehicle_hud(&inputs).unwrap();
    assert!(!ops.iter().any(|op| matches!(op, HudOp::RadarContents(_))));
}

/// `0041C4B9..0041C4D1`: layout 2, or an undrawn vehicle, which forces it,
/// reaches the cockpit prefix and stops; layout 1 draws the HUD.
#[test]
fn the_cockpit_layout_stops_before_drawing() {
    let mut inputs = call_5();
    inputs.frame.layout = 1;
    let ops = plan_vehicle_hud(&inputs).unwrap();
    assert!(!ops.iter().any(|op| matches!(op, HudOp::Omission(_))));
    assert!(ops.iter().any(|op| matches!(op, HudOp::Quad(_))));
    inputs.frame.layout = 2;
    assert_eq!(
        plan_vehicle_hud(&inputs).unwrap(),
        [HudOp::Omission(HudOmission::FirstPerson { layout: 2 })]
    );
    inputs.frame.layout = 0;
    inputs.vehicle.drawn = false;
    assert_eq!(
        plan_vehicle_hud(&inputs).unwrap(),
        [HudOp::Omission(HudOmission::FirstPerson { layout: 2 })]
    );
}

/// At 16:9 each cluster keeps retail's 1024x768 shapes and spacing and moves
/// to its own edge: the left one stays, the right one moves by the 341
/// extra pixels, the clock by half of that.
#[test]
fn a_widescreen_hud_keeps_retails_clusters_against_its_edges() {
    let four_three = call_5();
    let mut wide = call_5();
    wide.frame.surface = [1365, 768];
    let shift = |x: i32| {
        if x < 1024 / 3 {
            0
        } else if x > 1024 * 2 / 3 {
            341
        } else {
            170
        }
    };
    let retail = plan_vehicle_hud(&four_three).unwrap();
    let moved = plan_vehicle_hud(&wide).unwrap();
    assert_eq!(retail.len(), moved.len());
    let mut shifts = Vec::new();
    for (retail, moved) in retail.iter().zip(&moved) {
        match (retail, moved) {
            (HudOp::Quad(retail), HudOp::Quad(moved)) if retail.sprite_slot == BRACKET_SPRITE => {
                assert_eq!(retail, moved);
            }
            (HudOp::Quad(retail), HudOp::Quad(moved)) => {
                let centre = (retail.corners.iter().map(|c| i32::from(c[0])).sum::<i32>()) / 4;
                let dx = shift(centre) as i16;
                for (a, b) in retail.corners.iter().zip(&moved.corners) {
                    assert_eq!([a[0] + dx, a[1]], *b, "{retail:?}");
                }
                shifts.push(dx);
            }
            (
                HudOp::Text {
                    text: a,
                    position: HudTextPosition::At([x, y]),
                },
                HudOp::Text {
                    text: b,
                    position: HudTextPosition::At(moved),
                },
            )
            | (
                HudOp::Text {
                    text: a,
                    position: HudTextPosition::CentredOn([x, y]),
                },
                HudOp::Text {
                    text: b,
                    position: HudTextPosition::CentredOn(moved),
                },
            ) => {
                assert_eq!(a, b);
                assert_eq!([x + shift(*x), *y], *moved);
            }
            (
                HudOp::WeaponModel {
                    model: a,
                    centre: [x, y],
                    basis_angle: c,
                    distance: e,
                },
                HudOp::WeaponModel {
                    model: b,
                    centre,
                    basis_angle: d,
                    distance: f,
                },
            ) => {
                assert_eq!((a, c, e), (b, d, f));
                assert_eq!([x + shift(*x), *y], *centre);
            }
            (
                HudOp::RadarLabel {
                    position: [x, y], ..
                },
                HudOp::RadarLabel { position, .. },
            ) => {
                assert_eq!([x + 341, *y], *position);
            }
            (a, b) => assert_eq!(a, b),
        }
    }
    assert!(shifts.contains(&0) && shifts.contains(&341), "{shifts:?}");
}

/// Every switch panel `0041A740` draws in Infest02 and Infest03 (the
/// `weapon-panel.js` captures): the inputs it read, then its eight models'
/// projection centres, distances and models, its four frame quads and the
/// text packets the flush replays, in the order queued. Retail measured
/// each text with font 0, so a text's half width, and the count's drop below
/// its name, must stay the same across every panel.
#[test]
fn switch_panels_match_infest02_and_infest03() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/weapon_panels_native.json");
    let panels: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(panels.len(), 40);
    let sine = sine();
    let int = |value: &serde_json::Value| value.as_i64().unwrap() as i32;
    let ints = |value: &serde_json::Value| -> Vec<i32> {
        value.as_array().unwrap().iter().map(int).collect()
    };
    let names = [
        "Cannons",
        "Plasmas",
        "Lasers",
        "Flamethrower",
        "Rockets",
        "Guideds",
        "Detonator",
        "Cranial Implanter",
        "Grenade",
        "Frag",
        "Smoke",
        "Prox",
        "Limpet",
        "Laser",
        "Stun",
        "Napalm",
    ];
    let mut half_widths = std::collections::HashMap::new();
    let mut count_drop = None;
    for panel in &panels {
        let at = panel["at"].as_str().unwrap();
        let row = int(&panel["row"]) as usize;
        let selected = ints(&panel["selected"]);
        let ammo = ints(&panel["ammo"]);
        let centre = ints(&panel["centre"]);
        let mut weapons_owned = [false; 16];
        for (slot, owned) in panel["owns"].as_array().unwrap().iter().enumerate() {
            weapons_owned[row * 8 + slot] = int(&owned[1]) != 0;
        }
        let mut switch_timers = [0; 2];
        switch_timers[row] = int(&panel["timer"]) as u32;
        let mut switch_slides = [0; 2];
        switch_slides[row] = int(&panel["slide"]) as u32;
        let frame = HudFrame {
            surface: [1024, 768],
            focal: [int(&panel["focal"][0]), int(&panel["focal"][1])],
            layout: 0,
            clock: int(&panel["clock"]),
            hud_setting: 1,
            font_quarter_height: 4,
            view_basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            projection: hud_projection(CALL_5_EYE, CALL_5_VIEW),
            sine_quarter: sine,
            styles: HudStyles {
                frame: 0,
                turbo: 0,
                bar: 0,
                counters: [0; 3],
            },
            radar_size: [76, 76],
            viewport: [0, 0, 1024, 768],
        };
        let vehicle = HudVehicle {
            switching: [u8::from(row == 0), u8::from(row == 1)],
            switch_timers,
            switch_slides,
            weapons: HudWeapons {
                primary: selected[0] as i8,
                secondary: selected[1] as i8,
                primary_ammo: ammo[selected[0] as usize],
                secondary_ammo: ammo[selected[1] as usize],
                primary_charge: -1,
                ammo: std::array::from_fn(|index| ammo[index]),
            },
            ..level1_vehicle(0x1388, 1, [0, 0])
        };
        let player = HudPlayer {
            weapons_owned,
            ..call_5().player
        };
        let mut ops = Vec::new();
        let which = if row == 0 {
            SwitchPanel::Primary
        } else {
            SwitchPanel::Secondary
        };
        weapon_switch_panel(
            &mut ops,
            &frame,
            &vehicle,
            &player,
            which,
            [centre[0], centre[1]],
        )
        .unwrap();
        let models: Vec<Vec<i32>> = ops
            .iter()
            .filter_map(|op| match op {
                HudOp::WeaponModel {
                    model,
                    centre: [x, y],
                    distance,
                    basis_angle,
                } => {
                    assert_eq!(*basis_angle, frame.clock * 400);
                    Some(vec![*x, *y, *distance, i32::from(*model)])
                }
                _ => None,
            })
            .collect();
        let expected: Vec<Vec<i32>> = panel["draws"]
            .as_array()
            .unwrap()
            .iter()
            .map(ints)
            .collect();
        assert_eq!(models, expected, "{at}");
        let drawn: Vec<[[i16; 2]; 4]> = quads(&ops).iter().map(|quad| quad.corners).collect();
        let expected: Vec<[[i16; 2]; 4]> = panel["quads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|quad| {
                std::array::from_fn(|corner| {
                    std::array::from_fn(|axis| int(&quad["corners"][corner][axis]) as i16)
                })
            })
            .collect();
        assert_eq!(drawn, expected, "{at}");
        assert!(quads(&ops).iter().all(|quad| quad.sprite_slot == 0x2a6));
        let texts: Vec<(Vec<u8>, HudTextPosition)> = ops
            .iter()
            .filter_map(|op| match op {
                HudOp::PanelText { text, position } => Some((
                    match text {
                        HudText::Bytes(bytes) => bytes.clone(),
                        HudText::String(id) => {
                            let index = WEAPON_NAME_STRINGS
                                .iter()
                                .position(|string| string == id)
                                .unwrap();
                            names[index].as_bytes().to_vec()
                        }
                    },
                    *position,
                )),
                _ => None,
            })
            .collect();
        let expected = panel["texts"].as_array().unwrap();
        assert_eq!(texts.len(), expected.len(), "{at}");
        for ((text, position), retail) in texts.iter().zip(expected) {
            assert_eq!(
                text.as_slice(),
                retail[2].as_str().unwrap().as_bytes(),
                "{at}"
            );
            let [x, y] = [int(&retail[0]), int(&retail[1])];
            let anchor = match *position {
                HudTextPosition::CentredOn(anchor) => {
                    assert_eq!(y, anchor[1], "{at}");
                    anchor
                }
                HudTextPosition::CentredUnder { point, spacer } => {
                    assert_eq!(spacer, PANEL_COUNT_SPACER);
                    let drop = y - point[1];
                    assert_eq!(*count_drop.get_or_insert(drop), drop, "{at}");
                    point
                }
                other => panic!("{at}: a panel text at {other:?}"),
            };
            let half = anchor[0] - x;
            assert_eq!(
                *half_widths.entry(text.clone()).or_insert(half),
                half,
                "{at}"
            );
        }
    }
    assert!(count_drop.is_some());
}

/// Infest02's and Infest03's carried scientists (`hud-carried.js`): each
/// hold slot's actor draws at (72, 683), (106, 645) and (140, 683) on
/// 1024 by 768, 1032 in front (size 132 plus 900), the identity basis
/// turned 400 times the clock.
#[test]
fn the_hold_draws_each_carried_actor_in_its_slot() {
    let mut arena = crate::object_runtime::ObjectArena::new();
    let actors: Vec<_> = (0..3)
        .map(|_| arena.allocate(crate::object_runtime::ObjectClassKind::Base))
        .collect();
    let mut inputs = call_5();
    inputs.vehicle.carried = std::array::from_fn(|slot| {
        Some(HudCarriedActor {
            actor: actors[slot],
            size_28: 132,
        })
    });
    let ops = plan_vehicle_hud(&inputs).unwrap();
    let carried: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            HudOp::CarriedActor {
                actor,
                centre,
                distance,
                basis_angle,
            } => Some((*actor, *centre, *distance, *basis_angle)),
            _ => None,
        })
        .collect();
    let angle = inputs.frame.clock * 400;
    assert_eq!(
        carried,
        [
            (actors[0], [72, 683], 1032, angle),
            (actors[1], [106, 645], 1032, angle),
            (actors[2], [140, 683], 1032, angle),
        ]
    );
    // An empty slot draws nothing, and the others keep their places.
    inputs.vehicle.carried[1] = None;
    let ops = plan_vehicle_hud(&inputs).unwrap();
    let centres: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            HudOp::CarriedActor { centre, .. } => Some(*centre),
            _ => None,
        })
        .collect();
    assert_eq!(centres, [[72, 683], [140, 683]]);
    // They follow the models' light.
    let light = ops.iter().position(|op| *op == HudOp::WeaponLight).unwrap();
    let first = ops
        .iter()
        .position(|op| matches!(op, HudOp::CarriedActor { .. }))
        .unwrap();
    assert_eq!(first, light + 1);
}

/// The view of targeting frames 427 and 428 (the factory) and of 39 and 50.
const FRAME_427_VIEW: [u32; 9] = [
    0xbf69_984d,
    0,
    0x3ed1_7849,
    0xbdc6_6da4,
    0x3f78_b7b3,
    0xbe5d_481b,
    0xbecb_7e72,
    0xbe72_8177,
    0xbf62_f427,
];

const FRAME_39_VIEW: [u32; 9] = [
    0xbf80_0000,
    0,
    0,
    0,
    0x3f77_1d64,
    0xbe85_b85b,
    0,
    0xbe85_b85b,
    0xbf77_1d64,
];

/// One frame of the targeting receipts (`receipt3-Infest01.txt` in
/// `.tmp/infestation-ttd/20261012-targeting`): the reticle alone, as
/// `0041C92E` draws it once no menu is shown.
fn reticle_ops(clock: i32, view: [u32; 9], eye: [i32; 3], vehicle: HudVehicle) -> Vec<HudOp> {
    let inputs = infest01(clock, view, eye, vehicle, 0);
    let mut ops = Vec::new();
    reticle(&mut ops, &inputs.frame, &inputs.vehicle).unwrap();
    ops
}

fn non_quads(ops: &[HudOp]) -> Vec<HudOp> {
    ops.iter()
        .filter(|op| !matches!(op, HudOp::Quad(_)))
        .cloned()
        .collect()
}

/// Infest01's factory (side 0, at 1 of 1), the type-2 target of frames 427
/// and 428, at its focus +14.
const FACTORY: HudTarget = HudTarget {
    kind: 2,
    focus: Some([0xb877, 0x0060, 0x665f]),
    actor: Some(HudTargetActor {
        health: 1,
        maximum_health: Some(1),
        team: 0,
    }),
};

fn factory_vehicle(corners: [[u16; 2]; 3], dword_5c0: u32) -> HudVehicle {
    HudVehicle {
        target: Some(FACTORY),
        corners,
        position: [0xba79, 0x0040, 0x6bd6],
        dword_5c0,
        ..level1_vehicle(0x1388, 1, [0, 0])
    }
}

/// The full bar under `[x, y]` on 768 lines: 107 long and 7 thick, two
/// percent of the height below the focus.
fn full_bar([x, y]: [i16; 2]) -> [(u16, Corners, u32); 3] {
    let (bottom, top, end) = (y + 15, y + 8, x + 107);
    [
        (
            BAR_FRAME_SPRITE,
            [
                [end + 1, bottom + 1],
                [x + 1, bottom + 1],
                [x + 1, top + 1],
                [end + 1, top + 1],
            ],
            BAR_RGB,
        ),
        (
            BAR_SPRITE,
            [[x, bottom], [end, bottom], [end, top], [x, top]],
            BAR_RGB,
        ),
        (
            BAR_SPRITE,
            [[end, bottom], [end, bottom], [end, top], [end, top]],
            BOX_RGB,
        ),
    ]
}

/// Frame 427 (D75C02:A6E): the box on the factory. Retail's three corners
/// (sized 15x10, 12x8 and 9x6), the bracket closing on the projected focus
/// (half +5C0's 500000, so 500 tall), the full bar under it and "Caution";
/// each corner then eases a quarter of the way to the next, the last onto
/// the focus.
#[test]
fn the_box_matches_infest01_frame_427() {
    let ops = reticle_ops(
        0xa70,
        FRAME_427_VIEW,
        [-17600, 287, 28001],
        factory_vehicle([[519, 405], [522, 369], [523, 339]], 500_000),
    );
    let mut expected = Vec::new();
    for bracket in [
        [
            [504, 395, 534, 395, 534, 396, 504, 396],
            [505, 396, 519, 422, 520, 422, 506, 396],
            [533, 396, 519, 422, 520, 422, 534, 396],
        ],
        [
            [510, 361, 534, 361, 534, 362, 510, 362],
            [510, 362, 521, 382, 522, 382, 511, 362],
            [534, 362, 523, 382, 524, 382, 535, 362],
        ],
        [
            [514, 333, 532, 333, 532, 334, 514, 334],
            [514, 333, 523, 349, 524, 349, 515, 333],
            [532, 333, 523, 349, 524, 349, 533, 333],
        ],
        [
            [483, -188, 497, -188, 497, -187, 483, -187],
            [43, 528, 49, 540, 50, 540, 44, 528],
            [937, 528, 931, 540, 932, 540, 938, 528],
        ],
    ] {
        expected.extend(retail_bracket(bracket, BOX_RGB));
    }
    expected.extend(full_bar([490, 312]));
    assert_eq!(reticle_quads(&ops), expected);
    assert_eq!(
        non_quads(&ops),
        [
            HudOp::Text {
                text: HudText::String(CAPTION),
                position: HudTextPosition::At([490, 312]),
            },
            HudOp::SetReticle5c0(250_000),
            HudOp::SetReticleCorners([[519, 396], [522, 362], [490, 312]]),
        ]
    );
}

/// Frame 428 (D77D3E:1B81), the next: the corners frame 427 left, the
/// focus four lines lower as the camera moves and the closing bracket 250
/// tall.
#[test]
fn the_box_matches_infest01_frame_428() {
    let ops = reticle_ops(
        0xa76,
        FRAME_427_VIEW,
        [-17646, 289, 27885],
        factory_vehicle([[519, 396], [522, 362], [490, 312]], 250_000),
    );
    let mut expected = Vec::new();
    for bracket in [
        [
            [504, 386, 534, 386, 534, 387, 504, 387],
            [505, 387, 519, 413, 520, 413, 506, 387],
            [533, 387, 519, 413, 520, 413, 534, 387],
        ],
        [
            [510, 354, 534, 354, 534, 355, 510, 355],
            [510, 355, 521, 375, 522, 375, 511, 355],
            [534, 355, 523, 375, 524, 375, 535, 355],
        ],
        [
            [481, 306, 499, 306, 499, 307, 481, 307],
            [481, 306, 490, 322, 491, 322, 482, 306],
            [499, 306, 490, 322, 491, 322, 500, 306],
        ],
        [
            [483, 66, 497, 66, 497, 67, 483, 67],
            [265, 421, 271, 433, 272, 433, 266, 421],
            [715, 421, 709, 433, 710, 433, 716, 421],
        ],
    ] {
        expected.extend(retail_bracket(bracket, BOX_RGB));
    }
    expected.extend(full_bar([490, 316]));
    assert_eq!(reticle_quads(&ops), expected);
    assert_eq!(
        non_quads(&ops),
        [
            HudOp::Text {
                text: HudText::String(CAPTION),
                position: HudTextPosition::At([490, 316]),
            },
            HudOp::SetReticle5c0(125_000),
            HudOp::SetReticleCorners([[519, 388], [514, 350], [490, 316]]),
        ]
    );
}

/// Frame 50 (B3BED8:B8B): furniture (type 4, `00528060`) gets the box on
/// its focus +14, but no bar and no caption, and no sight.
#[test]
fn furniture_gets_the_box_alone_as_in_infest01_frame_50() {
    let vehicle = HudVehicle {
        target: Some(HudTarget {
            kind: 4,
            focus: Some([0x7d88, 0x013e, 0x82ce]),
            actor: None,
        }),
        corners: [[513, 401], [514, 367], [514, 338]],
        position: [0x7daa, 0x00ab, 0x8e7c],
        dword_5c0: 500_000,
        ..level1_vehicle(0x1388, 1, [0, 0])
    };
    let ops = reticle_ops(0x13b, FRAME_39_VIEW, [32170, 421, -28629], vehicle);
    let mut expected = Vec::new();
    for bracket in [
        [
            [498, 391, 528, 391, 528, 392, 498, 392],
            [499, 392, 513, 418, 514, 418, 500, 392],
            [527, 392, 513, 418, 514, 418, 528, 392],
        ],
        [
            [502, 359, 526, 359, 526, 360, 502, 360],
            [502, 360, 513, 380, 514, 380, 503, 360],
            [526, 360, 515, 380, 516, 380, 527, 360],
        ],
        [
            [505, 332, 523, 332, 523, 333, 505, 333],
            [505, 332, 514, 348, 515, 348, 506, 332],
            [523, 332, 514, 348, 515, 348, 524, 332],
        ],
        [
            [510, -238, 524, -238, 524, -237, 510, -237],
            [70, 478, 76, 490, 77, 490, 71, 478],
            [964, 478, 958, 490, 959, 490, 965, 478],
        ],
    ] {
        expected.extend(retail_bracket(bracket, BOX_RGB));
    }
    assert_eq!(reticle_quads(&ops), expected);
    assert_eq!(
        non_quads(&ops),
        [
            HudOp::SetReticle5c0(250_000),
            HudOp::SetReticleCorners([[513, 393], [514, 360], [517, 262]]),
        ]
    );
}

/// Frame 39 (B2991B:61D): a terrain point (type 8) keeps the sight. +5C0
/// stays at its 500000 cap and the first corner gets a bracket a thousand
/// pixels tall, off the screen; then the rings ease toward the vehicle's
/// position plus a quarter, a half and all of the direction to the point.
#[test]
fn a_point_keeps_the_sight_as_in_infest01_frame_39() {
    let vehicle = HudVehicle {
        target: Some(HudTarget {
            kind: 8,
            focus: Some([0x7d97, 0x006b, 0x8b7a]),
            actor: None,
        }),
        corners: [[513, 343], [511, 308], [509, 286]],
        position: [0x7db1, 0x0139, 0x93af],
        sight_direction: Some([-26, -206, -2101]),
        dword_5c0: 500_000,
        ..level1_vehicle(0x1388, 1, [0, 0])
    };
    let ops = reticle_ops(0xf7, FRAME_39_VIEW, [32177, 563, -27298], vehicle);
    let mut expected = retail_bracket(
        [
            [506, -657, 520, -657, 520, -656, 506, -656],
            [-378, 781, -372, 793, -371, 793, -377, 781],
            [1404, 781, 1398, 793, 1399, 793, 1405, 781],
        ],
        BOX_RGB,
    )
    .to_vec();
    for bracket in [
        [
            [498, 348, 528, 348, 528, 349, 498, 349],
            [499, 349, 513, 375, 514, 375, 500, 349],
            [527, 349, 513, 375, 514, 375, 528, 349],
        ],
        [
            [500, 315, 524, 315, 524, 316, 500, 316],
            [500, 316, 511, 336, 512, 336, 501, 316],
            [524, 316, 513, 336, 514, 336, 525, 316],
        ],
        [
            [502, 293, 520, 293, 520, 294, 502, 294],
            [502, 293, 511, 309, 512, 309, 503, 293],
            [520, 293, 511, 309, 512, 309, 521, 293],
        ],
    ] {
        expected.extend(retail_bracket(bracket, SIGHT_RGB));
    }
    assert_eq!(reticle_quads(&ops), expected);
    assert_eq!(
        non_quads(&ops),
        [
            HudOp::SetReticle5c0(500_000),
            HudOp::SetReticleCorners([[513, 358], [512, 323], [511, 299]]),
        ]
    );
}

/// Below 1/32 of its maximum the bar blinks: empty, then 0x800/0x10000 of
/// it every other 15 clock ticks; at no health it stays empty.
#[test]
fn a_nearly_dead_targets_bar_blinks_with_the_clock() {
    let filled = |health: u16, clock: i32| {
        let vehicle = HudVehicle {
            target: Some(HudTarget {
                actor: Some(HudTargetActor {
                    health,
                    maximum_health: Some(5000),
                    team: 0,
                }),
                ..FACTORY
            }),
            ..factory_vehicle([[519, 405], [522, 369], [523, 339]], 500_000)
        };
        let ops = reticle_ops(clock, FRAME_427_VIEW, [-17600, 287, 28001], vehicle);
        let quads = reticle_quads(&ops);
        let (_, fill, _) = quads[quads.len() - 2];
        i32::from(fill[1][0] - fill[0][0])
    };
    // 4999 missing of 5000 leaves 0x1B of 0x10000: the blink.
    assert_eq!(filled(1, 15 * 178), 0);
    assert_eq!(filled(1, 15 * 179), 3);
    assert_eq!(filled(0, 15 * 179), 0);
    // 157 of 5000 is above 1/32.
    assert_eq!(filled(157, 0), 3);
    assert_eq!(filled(2500, 0), 53);
}

/// The box's sides: another side than the vehicle's and not side 0 or 1
/// is hostile ("Lock", after player command 0x21); the vehicle's own and
/// sides 0 and 1 are "Caution".
#[test]
fn hostile_actors_are_captioned_lock_after_the_hint() {
    let captions = |team: u32| {
        let vehicle = HudVehicle {
            target: Some(HudTarget {
                actor: Some(HudTargetActor {
                    team,
                    ..FACTORY.actor.unwrap()
                }),
                ..FACTORY
            }),
            ..factory_vehicle([[519, 405], [522, 369], [523, 339]], 500_000)
        };
        let ops = reticle_ops(0xa70, FRAME_427_VIEW, [-17600, 287, 28001], vehicle);
        let rest = non_quads(&ops);
        rest[..rest.len() - 2].to_vec()
    };
    let text = |id| HudOp::Text {
        text: HudText::String(id),
        position: HudTextPosition::At([490, 312]),
    };
    for team in [0, 1, 2] {
        assert_eq!(captions(team), [text(CAPTION)]);
    }
    for team in [3, 4, 0xffff_ffff] {
        assert_eq!(
            captions(team),
            [HudOp::PlayerCommand(HOSTILE_HINT), text(HOSTILE_CAPTION)]
        );
    }
}

/// With the HUD setting off nothing draws and +5C0 rises, target or not;
/// a type-2 target whose actor is missing (or is the vehicle) gets the box
/// without a bar.
#[test]
fn the_hud_setting_and_a_missing_actor_leave_parts_out() {
    let mut inputs = infest01(
        0xa70,
        FRAME_427_VIEW,
        [-17600, 287, 28001],
        factory_vehicle([[519, 405], [522, 369], [523, 339]], 400_000),
        0,
    );
    inputs.frame.hud_setting = 0;
    let mut ops = Vec::new();
    reticle(&mut ops, &inputs.frame, &inputs.vehicle).unwrap();
    assert_eq!(
        ops,
        [
            HudOp::SetReticle5c0(450_000),
            HudOp::SetReticleCorners([[519, 405], [522, 369], [523, 339]]),
        ]
    );
    let vehicle = HudVehicle {
        target: Some(HudTarget {
            actor: None,
            ..FACTORY
        }),
        ..factory_vehicle([[519, 405], [522, 369], [523, 339]], 500_000)
    };
    let ops = reticle_ops(0xa70, FRAME_427_VIEW, [-17600, 287, 28001], vehicle);
    assert_eq!(reticle_quads(&ops).len(), 12);
    assert_eq!(non_quads(&ops).len(), 2);
}

/// Unwritten inputs stop the plan: the sight's direction under the special
/// view, a box target's focus and a framed actor's maximum health.
#[test]
fn unwritten_reticle_inputs_stop_the_plan() {
    let plan = |vehicle: HudVehicle| {
        let inputs = infest01(0xa70, FRAME_427_VIEW, [-17600, 287, 28001], vehicle, 0);
        reticle(&mut Vec::new(), &inputs.frame, &inputs.vehicle)
    };
    let factory = factory_vehicle([[519, 405], [522, 369], [523, 339]], 500_000);
    assert_eq!(
        plan(HudVehicle {
            target: None,
            ..factory
        }),
        Err(VehicleHudError::SightDirection)
    );
    assert_eq!(
        plan(HudVehicle {
            target: Some(HudTarget {
                focus: None,
                ..FACTORY
            }),
            ..factory
        }),
        Err(VehicleHudError::TargetFocus)
    );
    assert_eq!(
        plan(HudVehicle {
            target: Some(HudTarget {
                actor: Some(HudTargetActor {
                    maximum_health: None,
                    ..FACTORY.actor.unwrap()
                }),
                ..FACTORY
            }),
            ..factory
        }),
        Err(VehicleHudError::TargetMaximumHealth)
    );
}

/// Infest02's frame 417 (2F014F:AFE, `receipt-hostile-Infest02.txt`): a
/// hostile lock. The side-18 actor at 200 of 200 gets the full bar, then
/// player command 0x21 and "Lock" on its focus.
#[test]
fn a_hostile_lock_matches_infest02_frame_417() {
    let vehicle = HudVehicle {
        target: Some(HudTarget {
            kind: 2,
            focus: Some([0x597b, 0x0288, 0x97a7]),
            actor: Some(HudTargetActor {
                health: 200,
                maximum_health: Some(200),
                team: 18,
            }),
        }),
        corners: [[516, 423], [508, 401], [495, 376]],
        position: [0x5538, 0x0052, 0x8ee3],
        dword_5c0: 500_000,
        ..level1_vehicle(0x1388, 1, [0, 0])
    };
    let view = [
        0x3f61_5ba9,
        0,
        0xbef2_e5a9,
        0x3d84_6296,
        0x3f7d_9cc9,
        0x3df5_a6d7,
        0x3ef0_aee7,
        0xbe0b_86c9,
        0x3f5f_3e0e,
    ];
    let mut inputs = infest01(0xa4a, view, [21603, 264, -29331], vehicle, 0);
    // Infest02's world fades from 0x193C to 0x1E48.
    inputs.frame.projection.projection.fade = [0x193c, 0x1e48];
    let mut ops = Vec::new();
    reticle(&mut ops, &inputs.frame, &inputs.vehicle).unwrap();
    let mut expected = Vec::new();
    for bracket in [
        [
            [501, 413, 531, 413, 531, 414, 501, 414],
            [502, 414, 516, 440, 517, 440, 503, 414],
            [530, 414, 516, 440, 517, 440, 531, 414],
        ],
        [
            [496, 393, 520, 393, 520, 394, 496, 394],
            [496, 394, 507, 414, 508, 414, 497, 394],
            [520, 394, 509, 414, 510, 414, 521, 394],
        ],
        [
            [486, 370, 504, 370, 504, 371, 486, 371],
            [486, 370, 495, 386, 496, 386, 487, 370],
            [504, 370, 495, 386, 496, 386, 505, 370],
        ],
        [
            [488, -256, 502, -256, 502, -255, 488, -255],
            [48, 460, 54, 472, 55, 472, 49, 460],
            [942, 460, 936, 472, 937, 472, 943, 460],
        ],
    ] {
        expected.extend(retail_bracket(bracket, BOX_RGB));
    }
    expected.extend(full_bar([495, 244]));
    assert_eq!(reticle_quads(&ops), expected);
    assert_eq!(
        non_quads(&ops),
        [
            HudOp::PlayerCommand(HOSTILE_HINT),
            HudOp::Text {
                text: HudText::String(HOSTILE_CAPTION),
                position: HudTextPosition::At([495, 244]),
            },
            HudOp::SetReticle5c0(250_000),
            HudOp::SetReticleCorners([[514, 418], [505, 395], [495, 244]]),
        ]
    );
}

/// The shipped H6 styles the reticle reads are the receipts' colours: the
/// sight's dark green, the box's red and the bar's green.
#[test]
fn the_reticle_styles_are_retails() {
    use crate::menu_text::MenuTextResources;
    let (runtime, _, _) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units128);
    let resources = crate::render_resources::RenderResources::new(&runtime);
    let rgb = |slot| resources.style(slot).unwrap() & 0x00ff_ffff;
    assert_eq!(
        [rgb(STYLE_FRAME), rgb(STYLE_TURBO), rgb(STYLE_BAR)],
        [SIGHT_RGB, BOX_RGB, BAR_RGB]
    );
}

/// `check_hud_ring.py`: the original `0041BF60` over 60 views, both rings
/// and both halves; every mark's text and centred position match.
#[test]
fn the_hud_ring_matches_its_original_in_every_mode() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/hud_ring_native.json")).unwrap();
    let sine = sine();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let int = |value: &serde_json::Value| value.as_i64().unwrap() as i32;
        let pair = |key: &str| [int(&input[key][0]), int(&input[key][1])];
        let rows: Vec<[f32; 3]> = input["basis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].as_f64().unwrap() as f32))
            .collect();
        let [font_10, font_14] = pair("font");
        let height = font_14 / font_10;
        let mut frame = infest01(0, [0; 9], [0; 3], level1_vehicle(1, 1, [0, 0]), 0).frame;
        frame.focal = pair("focal");
        frame.font_quarter_height = (height + if height < 0 { 3 } else { 0 }) >> 2;
        frame.view_basis = [rows[0], rows[1], rows[2]];
        frame.sine_quarter = sine;
        let mut ops = Vec::new();
        hud_ring(
            &mut ops,
            &frame,
            [int(&input["x"]), int(&input["y"])],
            int(&input["radius"]),
            input["vertical"].as_bool().unwrap(),
            input["both"].as_bool().unwrap(),
        )
        .unwrap();
        let [mul, add] = pair("width");
        let marks: Vec<serde_json::Value> = ops
            .iter()
            .map(|op| match op {
                HudOp::Text {
                    text: HudText::Bytes(text),
                    position: HudTextPosition::CentredOn([x, y]),
                } => {
                    let width = (text.len() as i32).wrapping_mul(mul).wrapping_add(add) as i16;
                    serde_json::json!([
                        String::from_utf8(text.clone()).unwrap(),
                        x - i32::from(width) / 2,
                        y
                    ])
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(serde_json::Value::Array(marks), case["marks"], "{input}");
    }
}

fn byte_texts(ops: &[HudOp]) -> Vec<(Vec<u8>, HudTextPosition)> {
    ops.iter()
        .filter_map(|op| match op {
            HudOp::Text {
                text: HudText::Bytes(text),
                position,
            } => Some((text.clone(), *position)),
            _ => None,
        })
        .collect()
}

fn online(mode: u32, score: HudScore) -> HudInputs {
    let mut inputs = call_5();
    inputs.world.network = Some(HudNetwork {
        mode,
        holds_ball: false,
        score,
    });
    inputs
}

/// 0041E44E..0041E9DB online: the race's laps at 88/100 H in place of the
/// counters and the milestones, the place 4% up and left of them, and the
/// terrain radar without the Radar (0041E352).
#[test]
fn online_the_race_shows_its_laps_and_place() {
    let score = HudScore::Race {
        laps: 1,
        total: 3,
        place: Some(2),
    };
    let ops = plan_vehicle_hud(&online(3, score)).unwrap();
    let texts = byte_texts(&ops);
    assert!(texts.contains(&(b"1/3".to_vec(), HudTextPosition::At([80, 675]))));
    assert!(texts.contains(&(b"2".to_vec(), HudTextPosition::At([40, 645]))));
    assert!(!texts.iter().any(|(text, _)| text == b"0/0"));
    terrain_radar(&ops);
    let offline = plan_vehicle_hud(&call_5()).unwrap();
    // The three counter gauges' quads are gone.
    assert!(quads(&ops).len() < quads(&offline).len());
    let team = plan_vehicle_hud(&online(0, HudScore::Team([3, 1]))).unwrap();
    assert!(byte_texts(&team).contains(&(b"3/1".to_vec(), HudTextPosition::At([80, 675]))));
    let own = plan_vehicle_hud(&online(2, HudScore::Own(7))).unwrap();
    assert!(byte_texts(&own).contains(&(b"07".to_vec(), HudTextPosition::At([80, 675]))));
}

/// Powerball (mode 4) leaves out the sight, the health gauge, the
/// secondary's count and the primary, and without the ball the secondary
/// shows model 0.
#[test]
fn powerball_leaves_out_the_weapons_and_gauges() {
    let ops = plan_vehicle_hud(&online(4, HudScore::Own(0))).unwrap();
    assert!(reticle_quads(&ops).is_empty());
    let models: Vec<i16> = ops
        .iter()
        .filter_map(|op| match op {
            HudOp::WeaponModel { model, .. } => Some(*model),
            _ => None,
        })
        .collect();
    assert_eq!(models, [0]);
    let texts = byte_texts(&ops);
    assert!(!texts.iter().any(|(text, _)| text == b"0"), "{texts:?}");
}
