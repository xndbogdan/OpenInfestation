use super::*;
use crate::{
    frontend_scene::SceneView,
    model_context::ModelProjection,
    model_primitives::{PrimitiveCommand, PrimitiveTables, TextureSortIdentity},
    render_resources::{
        RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderSource,
    },
    residency::ResidentRegistration,
    terrain_draw::{
        clip_terrain_tile, TerrainClippingHost, TerrainDrawInputs, TerrainFurnitureRequest,
        TerrainLighting, TerrainRow, TerrainStripMode,
    },
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, RequestKind,
    SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags, SpriteModeSelectors,
    SpriteVariant, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};

const REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 0,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 0,
};

fn value_i32(spec: &Value, field: &str, default: i32) -> i32 {
    spec[field].as_i64().map(|v| v as i32).unwrap_or(default)
}
fn vector<const N: usize>(spec: &Value, field: &str, default: [i32; N]) -> [i32; N] {
    spec[field]
        .as_array()
        .map(|a| std::array::from_fn(|i| a[i].as_i64().unwrap() as i32))
        .unwrap_or(default)
}
fn terrain(spec: &Value) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[..4].copy_from_slice(&value_i32(spec, "plane", 0).to_le_bytes());
    for i in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + i * 2] = value_i32(spec, "default_height", -1) as u8;
        bytes[TERRAIN_PREFIX_BYTES + i * 2 + 1] = 0x90;
    }
    let step = value_i32(spec, "step", 256) as u16;
    if let Some(cells) = spec["cells"].as_array() {
        for cell in cells {
            let x = cell[0].as_u64().unwrap() as u16;
            let z = cell[1].as_u64().unwrap() as u16;
            let mask = !((step - 1) >> 7);
            let index = usize::from((x >> 7) & mask) * 512 + usize::from((z >> 7) & mask);
            bytes[TERRAIN_PREFIX_BYTES + index * 2] = cell[2].as_i64().unwrap() as u8;
        }
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
            spacing: if step == 128 {
                TerrainSpacing::Units128
            } else {
                TerrainSpacing::Units256
            },
            height_scale: if value_i32(spec, "height_shift", 5) == 4 {
                TerrainHeightScale::Units16
            } else {
                TerrainHeightScale::Units32
            },
        },
    )
}
fn request<'a>(
    spec: &Value,
    terrain: &'a TerrainRuntime,
    tables: &'a PrimitiveTables,
    rows: &'a [TerrainRow],
) -> TerrainStripRequest<'a> {
    // Receipts supply published basis columns; the enclosing view owns rows.
    let values: [f32; 9] = spec["basis"]
        .as_array()
        .map(|a| std::array::from_fn(|i| a[i].as_f64().unwrap() as f32))
        .unwrap_or([1., 0., 0., 0., 1., 0., 0., 0., 1.]);
    let mut basis = MenuBasis::INITIAL;
    for row in 0..3 {
        for column in 0..3 {
            basis.words[1 + row * 3 + column] = values[column * 3 + row].to_bits();
        }
    }
    let fade = vector(spec, "fade", [64, 4096]);
    let step = value_i32(spec, "step", 256) as u16;
    TerrainStripRequest {
        terrain,
        rows,
        grid: super::super::TerrainGrid {
            spacing: step,
            mask: step - 1,
        },
        mode: TerrainStripMode::Near,
        fade,
        sort_bias: value_i32(spec, "sort_bias", -129),
        material: 0,
        far_color: value_i32(spec, "far_color", 0x87654321_u32 as i32) as u32,
        draw: TerrainDrawInputs {
            view: SceneView {
                eye: vector(spec, "eye", [0, 0, 0]),
                basis,
            },
            context_projection: ModelProjection {
                focal: vector(spec, "focal", [320, 240]),
                viewport: vector(spec, "viewport", [640, 480]),
                center: vector(spec, "center", [320, 240]),
                fade,
            },
            footprint: crate::terrain_draw::TerrainFootprintWidth::for_surface(vector(
                spec,
                "viewport",
                [640, 480],
            )),
            coordinate_shift: 16,
            tint: vector(spec, "tint", [240, 120, 60]).map(|v| v as u8),
            lighting: TerrainLighting::Disabled,
            clock: value_i32(spec, "clock", 123) as u32,
            world_flags: value_i32(spec, "world_flags", 0) as u32,
            night: spec["night_material"].as_u64().map(|v| v as u32),
            particles_enabled: false,
            tables,
        },
    }
}
fn sprite(spec: &Value) -> SpriteEntry {
    let dimensions = vector(spec, "dimensions", [7, 9]);
    SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: value_i32(spec, "selector", 255) as u8,
            mode1: 0,
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(value_i32(spec, "texture_flags", 0) as u16),
            color_count: 0,
            atlas_offset_bytes: 0,
            palette_offset: 0,
            width: dimensions[0] as u16,
            height: dimensions[1] as u16,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 2),
            },
        }],
    }
}
struct Host<'a> {
    spec: &'a Value,
    entry: &'a SpriteEntry,
    tables: &'a PrimitiveTables,
    events: Vec<Value>,
    packets: Vec<Value>,
}
impl Host<'_> {
    fn boundary(&self, name: &str) -> Result<(), &'static str> {
        if self.spec["fail"].as_str() == Some(name) {
            Err("required host failed")
        } else {
            Ok(())
        }
    }
    fn emit(
        &mut self,
        callback: u32,
        depth: u32,
        payload: &[Option<u8>],
        texture: bool,
    ) -> Result<(), &'static str> {
        self.events.push(json!(["submit", callback]));
        if self.spec["fail"].as_str() == Some("submit")
            && self.packets.len() == value_i32(self.spec, "fail_after", 0) as usize
        {
            return Err("required host failed");
        }
        self.packets.push(json!({"depth":depth,"callback":callback,"blocks":payload.len()/16,"payload":payload,"texture":texture}));
        Ok(())
    }
}
impl<'a> TerrainClippingHost<'a> for Host<'a> {
    type Error = &'static str;
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        TerrainDrawHost::material_sorting(self)
    }
    fn texture_sort_identity(
        &mut self,
        texture: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        TerrainDrawHost::texture_sort_identity(self, texture)
    }
    fn submit(&mut self, c: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.emit(
            c.callback.native_address(),
            c.depth,
            &c.payload,
            c.texture.is_some(),
        )
    }
}
impl<'a> TerrainDrawHost<'a> for Host<'a> {
    type Error = &'static str;
    fn material(&mut self, slot: i16) -> Result<u32, Self::Error> {
        self.events.push(json!(["material", slot]));
        self.boundary("material")?;
        Ok(value_i32(self.spec, "material", 0x12345678) as u32)
    }
    fn texture(&mut self, slot: i16) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.events.push(json!(["texture", slot]));
        self.boundary("texture")?;
        Ok(if self.spec["null_texture"].as_bool().unwrap_or(false) {
            None
        } else {
            Some(RenderResource {
                request: RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: i32::from(slot),
                },
                source: RenderSource {
                    registration: &REGISTRATION,
                    bytes: &[],
                },
                data: RenderResourceData::Sprite(self.entry),
            })
        })
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push(json!(["sorting"]));
        self.boundary("sorting")?;
        Ok(self.spec["sorting"].as_bool().unwrap_or(false))
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        Ok(TextureSortIdentity::NativeEntryAddress(0x896000))
    }
    fn clipped_tile(&mut self, r: TerrainClipRequest<'a>) -> Result<usize, Self::Error> {
        self.events.push(json!(["clip"]));
        clip_terrain_tile(r, self.tables, self).map_err(|_| "clipped tile failed")
    }
    fn submit_quad(&mut self, c: TerrainQuadCommand<'a>) -> Result<(), Self::Error> {
        self.emit(
            c.callback.native_address(),
            c.depth,
            &c.payload,
            c.texture.is_some(),
        )
    }
    fn publish_far_color(&mut self, _: u32) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn publish_fade(&mut self, _: [i32; 2]) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn begin_timing(&mut self) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn end_timing(&mut self) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn furniture(&mut self, _: TerrainFurnitureRequest<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn particles(
        &mut self,
        _: crate::terrain_draw::TerrainParticleEmissionRequest<'_>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn water(&mut self, _: TerrainStripRequest<'_>) -> Result<TerrainDrawOutcome, Self::Error> {
        unreachable!()
    }
    fn omitted(&mut self, _: super::super::TerrainFeature, _: TerrainDrawError<Self::Error>) {
        unreachable!()
    }
}

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/terrain_water_native.json"
    ))
    .unwrap()
}

#[test]
fn water_strips_match_original_instructions_packets_and_required_prefixes() {
    let tables = super::super::tests::tables();
    let fixtures = fixture();
    let mut count = 0;
    for spec in fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "strip")
    {
        let terrain = terrain(spec);
        let entry = sprite(spec);
        let rows: Vec<_> = spec["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| TerrainRow {
                z: r[0].as_i64().unwrap() as i16,
                left: r[1].as_i64().unwrap() as i16,
                right: r[2].as_i64().unwrap() as i16,
            })
            .collect();
        let mut host = Host {
            spec,
            entry: &entry,
            tables: &tables,
            events: Vec::new(),
            packets: Vec::new(),
        };
        let result = draw_water_strip(request(spec, &terrain, &tables, &rows), &mut host);
        assert_eq!(
            result.is_err(),
            !spec["error"].is_null(),
            "{}: {result:?}",
            spec["name"]
        );
        assert_eq!(
            host.events,
            spec["events"].as_array().unwrap().to_vec(),
            "{}",
            spec["name"]
        );
        let packets = spec["packets"].as_array().unwrap();
        assert_eq!(host.packets.len(), packets.len(), "{}", spec["name"]);
        for (actual, expected) in host.packets.iter().zip(packets) {
            for field in ["depth", "callback", "blocks", "texture"] {
                assert_eq!(actual[field], expected[field], "{} {field}", spec["name"]);
            }
            let actual_payload = actual["payload"].as_array().unwrap();
            let expected_payload = expected["payload"].as_array().unwrap();
            for (offset, (actual, expected)) in
                actual_payload.iter().zip(expected_payload).enumerate()
            {
                // E150 copies a DWORD from its intersection scratch. The RGB
                // owner writes only three bytes; a supplied zero high byte is
                // not newly initialized data in the portable intersection.
                if actual.is_null()
                    && expected.is_number()
                    && expected_payload.len() == 80
                    && [0x23, 0x27, 0x2b].contains(&offset)
                {
                    continue;
                }
                assert_eq!(actual, expected, "{} payload+{offset:02X}", spec["name"]);
            }
        }
        count += 1;
    }
    assert_eq!(count, 50);
}

#[test]
fn water_crt_projection_matches_original_word_and_unwritten_byte_rules() {
    let tables = super::super::tests::tables();
    let fixtures = fixture();
    let mut count = 0;
    for spec in fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "projection")
    {
        let terrain = terrain(spec);
        let r = request(spec, &terrain, &tables, &[]);
        let mut output = ModelVertexProjection::default();
        let depth = project(r, vector(spec, "point", [0; 3]), &mut output);
        assert_eq!(
            depth as u32,
            spec["returned"].as_u64().unwrap() as u32,
            "{}",
            spec["name"]
        );
        let actual: Value = serde_json::to_value(output.bytes).unwrap();
        assert_eq!(actual, spec["projection"], "{}", spec["name"]);
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn water_clipped_height_matches_original_full_plane_gate_and_low_ax() {
    let tables = super::super::tests::tables();
    let fixtures = fixture();
    let mut count = 0;
    for spec in fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "height")
    {
        let terrain = terrain(spec);
        let r = request(spec, &terrain, &tables, &[]);
        let xz = vector(spec, "xz", [0; 2]).map(|v| v as u16);
        assert_eq!(
            water_height(r, xz) as u16,
            spec["returned"].as_u64().unwrap() as u16,
            "{}",
            spec["name"]
        );
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn water_zero_shoreline_and_all_fog_gate_before_texture_or_sorting() {
    let fixture = fixture();
    for name in ["shoreline-0", "all-fogged"] {
        let spec = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let terrain = terrain(spec);
        let tables = super::super::tests::tables();
        let entry = sprite(spec);
        let rows: Vec<_> = spec["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| TerrainRow {
                z: r[0].as_i64().unwrap() as i16,
                left: r[1].as_i64().unwrap() as i16,
                right: r[2].as_i64().unwrap() as i16,
            })
            .collect();
        let mut host = Host {
            spec,
            entry: &entry,
            tables: &tables,
            events: Vec::new(),
            packets: Vec::new(),
        };
        let result = draw_water_strip(request(spec, &terrain, &tables, &rows), &mut host).unwrap();
        assert_eq!(host.events, vec![json!(["material", 116])]);
        assert!(host.packets.is_empty());
        assert_eq!(result.quads, 0);
        assert!(result.rejected > 0);
    }
}
