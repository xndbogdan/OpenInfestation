use std::path::Path;

use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    menu::MenuBasis,
    model_vertices::ModelVertexProjection,
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
};

pub(super) fn tables() -> PrimitiveTables {
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    PrimitiveTables::from_executable(FrontendExecutable::parse(assets.executable_bytes()).unwrap())
        .unwrap()
}

fn terrain(raw: u16) -> TerrainRuntime {
    // Explicit synthetic H12 height/material test data, never a replacement
    // presentation mesh. The canonical parser owns the fixed binary layout.
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[0..4].copy_from_slice(&(-2_000_000_i32).to_le_bytes());
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2..TERRAIN_PREFIX_BYTES + index * 2 + 2]
            .copy_from_slice(&raw.to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units256,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn inputs(tables: &PrimitiveTables) -> TerrainDrawInputs<'_> {
    TerrainDrawInputs {
        view: SceneView {
            eye: [0, 0, 0],
            basis: MenuBasis::INITIAL,
        },
        context_projection: ModelProjection {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [6144, 6656],
        },
        footprint: crate::terrain_draw::TerrainFootprintWidth::Retail,
        coordinate_shift: 16,
        tint: [240, 120, 60],
        lighting: TerrainLighting::Disabled,
        clock: 123,
        world_flags: 0,
        night: None,
        particles_enabled: false,
        tables,
    }
}

#[derive(Default)]
struct Host<'a> {
    events: Vec<String>,
    commands: Vec<TerrainQuadCommand<'a>>,
    clips: Vec<TerrainClipRequest<'a>>,
    omitted: Vec<TerrainFeature>,
    fail_texture: bool,
    fail_submit_after: Option<usize>,
}

impl<'a> TerrainDrawHost<'a> for Host<'a> {
    type Error = &'static str;
    fn material(&mut self, slot: i16) -> Result<u32, Self::Error> {
        self.events.push(format!("H7/{slot}"));
        Ok(0x00605040)
    }
    fn texture(&mut self, slot: i16) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.events.push(format!("H3/{slot}"));
        if self.fail_texture {
            Err("missing texture")
        } else {
            Ok(None)
        }
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push("sorting".into());
        Ok(false)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        panic!("synthetic null texture has no identity")
    }
    fn publish_far_color(&mut self, color: u32) -> Result<(), Self::Error> {
        self.events.push(format!("color{color:#x}"));
        Ok(())
    }
    fn publish_fade(&mut self, fade: [i32; 2]) -> Result<(), Self::Error> {
        self.events.push(format!("fade{fade:?}"));
        Ok(())
    }
    fn begin_timing(&mut self) -> Result<(), Self::Error> {
        self.events.push("begin".into());
        Ok(())
    }
    fn end_timing(&mut self) -> Result<(), Self::Error> {
        self.events.push("end".into());
        Ok(())
    }
    fn furniture(&mut self, request: TerrainFurnitureRequest<'_>) -> Result<(), Self::Error> {
        let domain = request.domain;
        assert_eq!(domain.coords.len(), domain.rows * 3);
        self.events.push("furniture".into());
        Err("unowned furniture draw")
    }
    fn particles(&mut self, _: TerrainParticleEmissionRequest<'_>) -> Result<(), Self::Error> {
        Err("unowned particles")
    }
    fn water(&mut self, _: TerrainStripRequest<'_>) -> Result<TerrainDrawOutcome, Self::Error> {
        Err("unowned water")
    }
    fn clipped_tile(&mut self, request: TerrainClipRequest<'a>) -> Result<usize, Self::Error> {
        self.events.push("clip".into());
        self.clips.push(request);
        Ok(0)
    }
    fn submit_quad(&mut self, command: TerrainQuadCommand<'a>) -> Result<(), Self::Error> {
        if self.fail_submit_after == Some(self.commands.len()) {
            return Err("submit failed");
        }
        self.commands.push(command);
        Ok(())
    }
    fn omitted(&mut self, feature: TerrainFeature, _: TerrainDrawError<Self::Error>) {
        self.omitted.push(feature);
    }
}

#[test]
fn terrain_rows_keep_bottom_edge_reversal_and_cap_overwrite() {
    let grid = TerrainGrid {
        spacing: 256,
        mask: 255,
    };
    let domain = terrain_row_domain(
        [[-512, 512], [512, 512], [512, -512], [-512, -512]],
        grid,
        None,
    )
    .unwrap();
    assert_eq!(
        domain.rows,
        [
            TerrainRow {
                z: 2,
                left: -2,
                right: 2
            },
            TerrainRow {
                z: 1,
                left: -2,
                right: 2
            },
            TerrainRow {
                z: 0,
                left: -2,
                right: 2
            },
            TerrainRow {
                z: -1,
                left: -2,
                right: 2
            },
            TerrainRow {
                z: -2,
                left: 2,
                right: -2
            },
        ]
    );
    let whole = |capacity| {
        terrain_row_domain(
            [
                [-32000, 32767],
                [32000, 32767],
                [32000, -32768],
                [-32000, -32768],
            ],
            TerrainGrid {
                spacing: 128,
                mask: 127,
            },
            capacity,
        )
        .unwrap()
    };
    let capped = whole(Some(super::domain::RETAIL_TERRAIN_ROWS));
    assert_eq!(capped.rows.len(), 176);
    assert_eq!(capped.rows[174].z, 81);
    assert_eq!(capped.rows[175].z, -256);
    assert_eq!((capped.minimum_x, capped.maximum_x), (-250, 250));
    // Uncapped, every row stays, down to the same last one.
    let every = whole(None);
    assert_eq!(every.rows.len(), 512);
    assert_eq!(every.rows[..175], capped.rows[..175]);
    assert_eq!(every.rows[511], capped.rows[175]);
    assert_eq!((every.minimum_x, every.maximum_x), (-250, 250));
    assert!(matches!(
        terrain_row_domain([[0, 0]; 4], grid, None),
        Err(TerrainDrawFault::FlatFootprint)
    ));
}

#[test]
fn terrain_plan_preserves_native_fade_and_far_spacing() {
    let plan = terrain_draw_plan(
        MenuBasis::INITIAL,
        [6144, 6656],
        TerrainGrid {
            spacing: 256,
            mask: 255,
        },
        TerrainFootprintWidth::Retail,
    )
    .unwrap();
    assert_eq!(plan.near_fade, [1996, 3660]);
    assert_eq!(plan.far_fade, [8320, 9984]);
    assert_eq!(plan.sort_bias, 128);
    assert_eq!(plan.furniture.rows[0].z, 28);
    assert_eq!(plan.near.rows[0].z, 16);
    assert_eq!(plan.far.rows[0].z, 21);
    for strip in plan.near.strips().into_iter().chain(plan.far.strips()) {
        let min = strip.iter().map(|r| r.left).min().unwrap();
        let max = strip.iter().map(|r| r.right).max().unwrap();
        assert!(i32::from(max) - i32::from(min) <= 14);
    }
}

fn corner(x: i16, y: i16, depth: i32, flags: u8, fog: u8) -> TerrainVertex {
    let mut projection = ModelVertexProjection::default();
    projection.write_u32(0, u32::from(x as u16) | (u32::from(y as u16) << 16));
    projection.write_u32(4, depth as u32);
    projection.bytes[8] = Some(0x12);
    projection.bytes[9] = Some(fog);
    TerrainVertex {
        rgb: [240, 120, 60],
        cell_flags: flags,
        projection,
        world: [0; 3],
    }
}

#[test]
fn terrain_quad_callbacks_keep_far_depth_shades_and_material_order() {
    let terrain = terrain(0);
    let tables = tables();
    let draw = inputs(&tables);
    let mut host = Host::default();
    let corners = [
        corner(100, 100, 2000, 0x10, 0),
        corner(100, 200, 2000, 0x20, 0),
        corner(200, 200, 2000, 0x30, 0),
        corner(200, 100, 2000, 0x40, 0),
    ];
    let near = TerrainStripRequest {
        terrain: &terrain,
        draw,
        rows: &[],
        grid: TerrainGrid {
            spacing: 256,
            mask: 255,
        },
        mode: TerrainStripMode::Near,
        fade: [1996, 3660],
        sort_bias: 128,
        material: 0x605040,
        far_color: 0x102030,
    };
    assert!(quad::submit_tile(
        near,
        corners,
        quad::TerrainTileCoordinates::default(),
        &mut host
    )
    .unwrap());
    assert_eq!(host.events, ["H3/1276", "sorting"]);
    let command = &host.commands[0];
    assert_eq!(command.callback, TerrainQuadCallback::TexturedNear);
    assert_eq!(command.depth, 2489); //128+256*5656/4000, then geometric2000.
    assert_eq!(command.read_u32(0x28), Some(0x403c78f0));
    assert_eq!(command.read_u32(0x38), None);
    let mut fogged = corners;
    fogged[1].projection.bytes[9] = Some(128);
    assert!(quad::submit_tile(
        near,
        fogged,
        quad::TerrainTileCoordinates::default(),
        &mut host
    )
    .unwrap());
    assert_eq!(host.commands[1].callback, TerrainQuadCallback::TexturedFar);
    assert_eq!(host.commands[1].read_u32(0x38), Some(0x605040));
    let far = TerrainStripRequest {
        grid: TerrainGrid::FAR,
        mode: TerrainStripMode::Far,
        fade: [8320, 9984],
        ..near
    };
    assert!(quad::submit_tile(
        far,
        fogged,
        quad::TerrainTileCoordinates::default(),
        &mut host
    )
    .unwrap());
    let command = &host.commands[2];
    assert_eq!(command.callback, TerrainQuadCallback::ColoredFar);
    assert_eq!(command.callback.renderer_virtual_offset(), 0x98);
    assert_eq!(command.read_u32(4), Some(3024));
    assert_eq!(command.read_u32(0x24), Some(0x605040));
    assert_eq!(
        &command.payload[0x28..0x2c],
        &[Some(128), Some(128), Some(128), None]
    );
    assert_eq!(command.read_u32(0x38), Some(0x102030));
}

#[test]
fn terrain_clipped_tile_resolves_material_before_required_clip_service() {
    let terrain = terrain(0);
    let tables = tables();
    let draw = inputs(&tables);
    let strip = TerrainStripRequest {
        terrain: &terrain,
        draw,
        rows: &[],
        grid: TerrainGrid {
            spacing: 256,
            mask: 255,
        },
        mode: TerrainStripMode::Near,
        fade: [1996, 3660],
        sort_bias: 0,
        material: 0,
        far_color: 0,
    };
    let mut corners = [corner(100, 100, 2000, 0, 0); 4];
    corners[0].projection.bytes[8] = Some(0xc0);
    let mut host = Host::default();
    let coordinates = quad::TerrainTileCoordinates {
        x: [256, 512],
        z: [0, 256],
    };
    assert!(!quad::submit_tile(strip, corners, coordinates, &mut host).unwrap());
    assert_eq!(host.events, ["H3/1276", "clip"]);
    assert_eq!(host.clips[0].sort_bias, 0); // No direct-quad spacing adjustment.
    assert_eq!(
        host.clips[0].eye_relative,
        [[256, 0, 0], [512, 0, 256], [512, 0, 256], [768, 0, 0]]
    );
    assert_eq!(host.clips[0].context.projection.fade, strip.fade);
    host.events.clear();
    assert!(!quad::submit_tile(
        TerrainStripRequest {
            mode: TerrainStripMode::Far,
            ..strip
        },
        corners,
        coordinates,
        &mut host
    )
    .unwrap());
    assert!(host.events.is_empty());
}

#[test]
fn terrain_full_traversal_keeps_furniture_failure_and_far_global_fade() {
    let mut terrain = terrain(0x7000);
    let tables = tables();
    let draw = inputs(&tables);
    let mut host = Host::default();
    let outcome = draw_terrain(
        TerrainDrawRequest {
            terrain: &mut terrain,
            inputs: draw,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(
        &host.events[..6],
        [
            "H7/116",
            "color0x605040",
            "fade[6144, 6656]",
            "begin",
            "furniture",
            "end"
        ]
    );
    assert_eq!(
        host.events
            .iter()
            .filter(|s| s.starts_with("fade"))
            .collect::<Vec<_>>(),
        ["fade[6144, 6656]", "fade[1996, 3660]", "fade[8320, 9984]"]
    );
    assert!(host.omitted.contains(&TerrainFeature::Furniture));
    assert!(!host.commands.is_empty());
    assert!(outcome.quads > 0);
    assert!(host.commands.iter().any(|c| matches!(
        c.callback,
        TerrainQuadCallback::TexturedNear | TerrainQuadCallback::TexturedFar
    )));
    assert!(host.commands.iter().any(|c| matches!(
        c.callback,
        TerrainQuadCallback::ColoredNear | TerrainQuadCallback::ColoredFar
    )));
    assert_eq!(host.events.last().unwrap(), "end");
}

#[test]
fn terrain_missing_texture_preserves_committed_prefix_and_next_strip() {
    let mut terrain = terrain(0x7000);
    let tables = tables();
    let draw = inputs(&tables);
    let mut host = Host {
        fail_submit_after: Some(3),
        ..Host::default()
    };
    let _ = draw_terrain(
        TerrainDrawRequest {
            terrain: &mut terrain,
            inputs: draw,
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(host.commands.len(), 3);
    assert!(host.omitted.contains(&TerrainFeature::NearStrip));
    assert!(host.omitted.contains(&TerrainFeature::FarStrip));
    assert_eq!(host.events.last().unwrap(), "end");
}

/// A 4:3 surface keeps retail's footprints. A 16:9 one (1365x768) reaches
/// 4/3 as far to each side in every footprint, from the back corners to the
/// far edge, and the strips still keep to fourteen tiles.
#[test]
fn widescreen_footprints_reach_as_far_to_each_side_as_the_frame_sees() {
    for surface in [[1024, 768], [640, 480], [800, 768]] {
        assert_eq!(
            TerrainFootprintWidth::for_surface(surface),
            TerrainFootprintWidth::Retail
        );
    }
    let wide = TerrainFootprintWidth::for_surface([1365, 768]);
    assert_eq!(
        wide,
        TerrainFootprintWidth::Wide {
            across: 1365 * 3,
            retail: 768 * 4
        }
    );
    let grid = TerrainGrid {
        spacing: 256,
        mask: 255,
    };
    let plan =
        |footprint| terrain_draw_plan(MenuBasis::INITIAL, [6144, 6656], grid, footprint).unwrap();
    let (retail, widescreen) = (plan(TerrainFootprintWidth::Retail), plan(wide));
    assert_eq!(
        (
            widescreen.near_fade,
            widescreen.far_fade,
            widescreen.sort_bias
        ),
        (retail.near_fade, retail.far_fade, retail.sort_bias)
    );
    let reach = |domain: &TerrainDomain| (domain.minimum_x, domain.maximum_x);
    for (retail, widescreen) in [
        (&retail.furniture, &widescreen.furniture),
        (&retail.near, &widescreen.near),
        (&retail.far, &widescreen.far),
    ] {
        let (left, right) = reach(retail);
        let (wide_left, wide_right) = reach(widescreen);
        assert!(
            wide_left < left && wide_right > right,
            "{:?} {:?}",
            reach(retail),
            reach(widescreen)
        );
        // The same rows, each reaching about 4/3 as far: the far rows' X runs
        // grow by a third, give or take a tile at each end.
        assert_eq!(
            widescreen.rows.iter().map(|row| row.z).collect::<Vec<_>>(),
            retail.rows.iter().map(|row| row.z).collect::<Vec<_>>()
        );
        let (far_row, wide_row) = (retail.rows[0], widescreen.rows[0]);
        let span = |row: TerrainRow| i32::from(row.right) - i32::from(row.left);
        assert!(
            (span(wide_row) * 3 - span(far_row) * 4).abs() <= 6,
            "{far_row:?} {wide_row:?}"
        );
    }
    for strip in widescreen
        .near
        .strips()
        .into_iter()
        .chain(widescreen.far.strips())
    {
        let min = strip.iter().map(|r| r.left).min().unwrap();
        let max = strip.iter().map(|r| r.right).max().unwrap();
        assert!(i32::from(max) - i32::from(min) <= 14);
    }
    // A widened corner stops at 0x7F00 from the eye; one retail places
    // farther keeps retail's extent.
    assert_eq!(wide.lateral(3000, 0), 3999);
    assert_eq!(wide.lateral(25740, 19720), 25848);
    assert_eq!(wide.lateral(30000, 19720), 30000);
    assert_eq!(wide.lateral(30000, 0), 0x7f00);
    assert_eq!(TerrainFootprintWidth::Retail.lateral(30000, 19720), 30000);
}

/// At the full draw distance (far 0x3100) on fine terrain, a view along X
/// turns the footprints' lateral reach into rows. Retail's furniture
/// footprint then spans 270 rows and its buffer keeps 176; a 16:9 one spans
/// 360 and keeps them all, so each of retail's rows lies inside one of its
/// own, and its near footprint keeps all 198 rows.
#[test]
fn widescreen_footprints_keep_every_row_at_the_full_draw_distance() {
    let grid = TerrainGrid {
        spacing: 128,
        mask: 127,
    };
    // Turned a quarter: the forward axis along +X.
    let mut words = [0u32; 10];
    for (i, v) in [0.0f32, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0]
        .into_iter()
        .enumerate()
    {
        words[1 + i] = v.to_bits();
    }
    let plan = |footprint| {
        terrain_draw_plan(MenuBasis { words }, [0x2000, 0x3100], grid, footprint).unwrap()
    };
    let span = |domain: &TerrainDomain| {
        i32::from(domain.rows[0].z) - i32::from(domain.rows[domain.rows.len() - 1].z) + 1
    };
    let retail = plan(TerrainFootprintWidth::Retail);
    assert_eq!(
        (span(&retail.furniture), retail.furniture.rows.len()),
        (270, 176)
    );
    assert_eq!(retail.near.rows.len(), 149);
    let wide = plan(TerrainFootprintWidth::for_surface([1365, 768]));
    assert_eq!(
        (span(&wide.furniture), wide.furniture.rows.len()),
        (360, 360)
    );
    assert_eq!((span(&wide.near), wide.near.rows.len()), (198, 198));
    for row in &retail.furniture.rows[..175] {
        let wide_row = wide
            .furniture
            .rows
            .iter()
            .find(|wide_row| wide_row.z == row.z)
            .unwrap_or_else(|| panic!("{row:?}"));
        assert!(
            wide_row.left <= row.left && wide_row.right >= row.right,
            "{row:?} {wide_row:?}"
        );
    }
}

/// The footprints reach as far as the view, not only as far as its aspect:
/// retail's tier at its own focal (or zoomed in) keeps retail's; the
/// Remastered camera's 100 degrees across a 4:3 frame (focal 430 on the
/// 1024x768 tier, an Artoria run, 2026-10-29) widens by 512/430; a
/// widescreen frame at retail's focal widens as its surface does; and a
/// view taller than retail's widens by its height.
#[test]
fn footprints_reach_as_far_as_the_view_sees() {
    for focal in [512, 600] {
        assert_eq!(
            TerrainFootprintWidth::for_view([512, 384], [focal, focal]),
            TerrainFootprintWidth::Retail
        );
    }
    assert_eq!(
        TerrainFootprintWidth::for_view([512, 384], [430, 430]),
        TerrainFootprintWidth::Wide {
            across: 512,
            retail: 430
        }
    );
    let (view, surface) = (
        TerrainFootprintWidth::for_view([683, 384], [512, 512]),
        TerrainFootprintWidth::for_surface([1366, 768]),
    );
    for extent in [0x300, 3000, 17248] {
        assert_eq!(view.lateral(extent, 0), surface.lateral(extent, 0));
    }
    assert_eq!(
        TerrainFootprintWidth::for_view([400, 400], [512, 512]),
        TerrainFootprintWidth::Wide {
            across: 1600,
            retail: 1536
        }
    );
}

/// At the full draw distance on fine terrain, looking along X, the
/// Remastered camera on a 4:3 frame sees the ground 512/430 times its depth
/// to each side. Retail's furniture footprint keeps 176 of its rows, so a
/// crystal at the view's right edge (10000 ahead, 11907 to the side: rows
/// run along X here) lies on a dropped row; the view's own footprint keeps
/// every row and reaches it. Turned the other way, the furniture at the
/// left edge dropped instead.
#[test]
fn the_remastered_cameras_footprint_reaches_the_edges_of_a_4_3_view() {
    let grid = TerrainGrid {
        spacing: 128,
        mask: 127,
    };
    let mut words = [0u32; 10];
    for (i, v) in [0.0f32, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0]
        .into_iter()
        .enumerate()
    {
        words[1 + i] = v.to_bits();
    }
    let plan = |footprint| {
        terrain_draw_plan(MenuBasis { words }, [0x2000, 0x3100], grid, footprint).unwrap()
    };
    let reaches = |domain: &TerrainDomain, [x, z]: [i32; 2]| {
        domain.rows.iter().any(|row| {
            i32::from(row.z) == z / 128
                && i32::from(row.left) <= x / 128
                && x / 128 <= i32::from(row.right)
        })
    };
    let edge = [10_000, -10_000 * 512 / 430];
    let retail = plan(TerrainFootprintWidth::Retail);
    assert!(!reaches(&retail.furniture, edge));
    let view = plan(TerrainFootprintWidth::for_view([512, 384], [430, 430]));
    assert!(reaches(&view.furniture, edge));
    assert!(view.furniture.rows.len() > domain::RETAIL_TERRAIN_ROWS);
    for row in &retail.furniture.rows[..domain::RETAIL_TERRAIN_ROWS - 1] {
        assert!(
            view.furniture
                .rows
                .iter()
                .any(|own| own.z == row.z && own.left <= row.left && own.right >= row.right),
            "{row:?}"
        );
    }
}

/// `0044BD7D..0044BDDE`: the pane colour packs each halved channel's top
/// five bits into the display layout. Infrared's (0, 127, 0) on the RGB555
/// fields [6, 1, 4] is green 7 of 31; a colour in every channel shows each
/// shift's direction.
#[test]
fn night_material_packs_infrared_into_the_display_layout() {
    assert_eq!(night_material([0, 127, 0], [6, 1, 4]), 0x00e0);
    assert_eq!(
        night_material([0xff, 0x80, 0x41], [6, 1, 4]),
        0x3c00 | 0x0100 | 0x0004
    );
}
