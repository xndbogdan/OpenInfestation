//! The dome against `check_selector18_draw.py`'s receipts: the unchanged
//! 00477370 over supplied fields, ground and views.

use serde::Deserialize;

use super::*;
use crate::frontend_scene::SceneView;
use crate::menu::MenuBasis;
use crate::model_context::{ModelContext, ModelDispatch, ModelProjection};
use crate::model_primitives::TextureSortIdentity;
use crate::model_projection::ModelProjectionBackend;
use crate::model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector};
use crate::object_runtime::ribbon_sort_word;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Terrain {
    base: i32,
    slope_x: i32,
    slope_z: i32,
}

#[derive(Deserialize)]
struct Globals {
    direction: [f32; 3],
    half: u32,
    full: u32,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    eye: [i32; 3],
    view_basis: [f32; 9],
    focal: [i32; 2],
    viewport: [i32; 2],
    center: [i32; 2],
    fog_factor: i32,
    fade: [i32; 2],
    coordinate_shift: u8,
    far_color: u32,
    sorting: bool,
    selector: u8,
    texture_flags: u16,
    light: [i32; 3],
    world_flags: u32,
    detail: u32,
    ambient: u32,
    clock: u32,
    position: [i32; 3],
    radius: u32,
    height: u32,
    on: bool,
    field_fade: i32,
    terrain: Terrain,
    water: Option<i32>,
    packets: Vec<Packet>,
    heights: Vec<[u16; 2]>,
    waters: Vec<[i32; 4]>,
    light_globals: Globals,
    sort_queries: usize,
}

#[derive(Deserialize)]
struct Packet {
    key: u32,
    callback: u32,
    texture_slot: i32,
    payload: String,
}

fn basis(values: [f32; 9]) -> MenuBasis {
    let mut result = MenuBasis::INITIAL;
    result.words[1..].copy_from_slice(&values.map(f32::to_bits));
    result
}

/// check_selector18_draw.py's distinct descriptor per sprite.
fn descriptor_address(slot: i32) -> u32 {
    match slot {
        FORCE_FIELD_DECAL_SPRITE => 0x0075_6000,
        FORCE_FIELD_BAND_SPRITE => 0x0075_7000,
        FORCE_FIELD_BOTTOM_SPRITE => 0x0075_8000,
        other => panic!("{other:X}"),
    }
}

/// ribbon_sort_word over the supplied descriptor's selector and flags.
fn sorted(case: &Case, depth: u32, slot: i32) -> u32 {
    if !case.sorting {
        return depth;
    }
    ribbon_sort_word::<std::convert::Infallible>(
        depth,
        case.selector,
        |_| Ok(case.texture_flags),
        || {
            Ok(TextureSortIdentity::NativeEntryAddress(descriptor_address(
                slot,
            )))
        },
    )
    .unwrap()
}

/// 00490030 over the case's ambient: + grey / 2 at 0055F268, + grey at
/// 0055F26C, each channel saturating, the fourth bytes unwritten (zero).
fn light_words(ambient: u32) -> (u32, u32) {
    let bytes = ambient.to_le_bytes();
    let sum = |shift: u8| {
        u32::from_le_bytes([
            bytes[0].saturating_add(FORCE_FIELD_LIGHT_GREY >> shift),
            bytes[1].saturating_add(FORCE_FIELD_LIGHT_GREY >> shift),
            bytes[2].saturating_add(FORCE_FIELD_LIGHT_GREY >> shift),
            0,
        ])
    };
    (sum(1), sum(0))
}

/// 00477370 against the unchanged executable: its light, and the dome's
/// every packet kind, sort key, sprite and payload byte, with the ground
/// samples in order, over front, faded, coarse and fine, cut by sloped
/// ground or water, inside, fogged, sorted and yawed fields, and one faded
/// far past its cap, whose pulse needs the exact magic division.
#[test]
fn force_field_draw_matches_native_dome_packets() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector18_draw_native.json"
    ))
    .unwrap();
    let sine = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap()
    .sine_table()
    .values;
    let mut packets = 0;
    for case in &receipt.cases {
        let [x, y, z] = case.light;
        assert_eq!(
            crate::terrain_runtime::normalize_terrain_light([x / 4, y, z / 4]),
            case.light_globals.direction,
            "{} light",
            case.name
        );
        let (half, full) = light_words(case.ambient);
        assert_eq!(
            (half, full),
            (case.light_globals.half, case.light_globals.full)
        );
        let position = case.position.map(|value| value as u16);
        let (segments, meridians) = ForceFieldDome::rings(case.detail);
        let dome = ForceFieldDome {
            position,
            radius: case.radius,
            height: case.height,
            fade: case.field_fade,
            segments,
            meridians,
            clock: case.clock,
            light: full,
        };
        let view = SceneView {
            eye: case.eye,
            basis: basis(case.view_basis),
        };
        // 0048FE60 receives the field's sign-extended WORDs.
        let mut projector = ModelProjectionBackend {
            transform: ModelContext::new(
                position.map(|value| i32::from(value as i16)),
                MenuBasis::INITIAL,
            )
            .publish(view, case.coordinate_shift),
            view,
            projection: ModelProjection {
                focal: case.focal,
                viewport: case.viewport,
                center: case.center,
                fade: case.fade,
            },
            dispatch: ModelDispatch::Far,
            coordinate_shift: case.coordinate_shift,
            fog_factor: case.fog_factor,
        };
        let mut heights = Vec::new();
        let mut waters = Vec::new();
        let mut quads = Vec::new();
        if case.on {
            force_field_dome::<std::convert::Infallible>(
                &dome,
                &sine,
                |x, z| {
                    heights.push([x, z]);
                    let terrain = case.terrain.base
                        + ((i32::from(x as i16) * case.terrain.slope_x
                            + i32::from(z as i16) * case.terrain.slope_z)
                            >> 8);
                    let terrain = terrain as i16;
                    waters.push([
                        i32::from(x),
                        i32::from(z),
                        (case.world_flags & 0x40) as i32,
                        i32::from(terrain),
                    ]);
                    let water = case.water.map_or(terrain, |water| water as i16);
                    Ok(terrain.max(water))
                },
                |local| {
                    let mut output = ModelVertexProjection::default();
                    let depth = match projector.project(ModelVertexInput::Local(local), &mut output)
                    {
                        Ok(depth) => depth,
                        Err(error) => match error {},
                    };
                    Ok(DomePoint {
                        xy: output.read_u32(0).unwrap(),
                        depth,
                        clip: output.bytes[8].unwrap(),
                        fog: output.bytes[9],
                    })
                },
                |quad| {
                    quads.push(quad);
                    Ok(())
                },
            )
            .unwrap();
        }
        assert_eq!(heights, case.heights, "{} heights", case.name);
        assert_eq!(waters, case.waters, "{} waters", case.name);
        assert_eq!(quads.len(), case.packets.len(), "{} packets", case.name);
        assert_eq!(quads.len(), case.sort_queries, "{} sort queries", case.name);
        for (index, (quad, native)) in quads.iter().zip(&case.packets).enumerate() {
            let label = format!("{} packet {index}", case.name);
            let command = quad.command(
                sorted(case, quad.depth, quad.sprite),
                None,
                full,
                case.far_color,
            );
            assert_eq!(
                command.callback.native_address(),
                native.callback,
                "{label}"
            );
            assert_eq!(command.depth, native.key, "{label} key");
            assert_eq!(quad.sprite, native.texture_slot, "{label} sprite");
            assert_eq!(command.payload_blocks, 4, "{label}");
            for offset in 0..64 {
                let text = &native.payload[offset * 2..offset * 2 + 2];
                if (0x24..0x28).contains(&offset) {
                    continue;
                }
                let expected = match (text, offset) {
                    ("??", _) => None,
                    // The draw never stores the colours' fourth bytes.
                    (_, 0x2f | 0x33 | 0x37) => {
                        assert_eq!(text, "a5", "{label} +{offset:02X}");
                        None
                    }
                    (text, _) => Some(u8::from_str_radix(text, 16).unwrap()),
                };
                assert_eq!(command.payload[offset], expected, "{label} +{offset:02X}");
            }
            packets += 1;
        }
    }
    assert_eq!((receipt.cases.len(), packets), (18, 2702));
}
