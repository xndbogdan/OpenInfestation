use super::*;
use crate::{
    menu::MenuBasis,
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderSource,
    },
    ResidentRegistration,
};
use inf_formats::{
    ByteSpan, RequestKind, SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags,
    SpriteModeSelectors, SpriteVariant,
};

fn tables() -> PrimitiveTables {
    super::super::tests::tables()
}

fn context() -> TerrainClipContext {
    TerrainClipContext {
        transform: ModelTransform {
            view_position: [0; 3],
            view_basis: MenuBasis::INITIAL,
        },
        projection: ModelProjection {
            focal: [64, 64],
            viewport: [640, 480],
            center: [320, 240],
            fade: [64, 256],
        },
        fog_factor: 0x1000000 / 192,
    }
}

fn entry(selector: u8, flags: u16) -> SpriteEntry {
    let mut entry = SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: selector,
            mode1: 0,
            reserved: 0,
        },
        variants: vec![SpriteVariant {
            physical_index: 0,
            span: ByteSpan::new(0, 0),
            sprite_id: 0,
            reserved: 0,
            flags: SpriteFlags::new(flags),
            color_count: 0,
            atlas_offset_bytes: 0,
            palette_offset: 0,
            width: 7,
            height: 9,
            runtime_slots: [0; 2],
            atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
            atlas_record_index: 0,
            encoding: SpriteEncoding::DirectRgb555 {
                pixels: ByteSpan::new(0, 0),
            },
        }],
    };
    // Logical +14/+16 dimensions remain first-descriptor values even when
    // material sorting selects a different physical descriptor.
    if selector == 1 {
        let mut second = entry.variants[0].clone();
        second.physical_index = 1;
        second.width = 17;
        second.height = 19;
        entry.variants.push(second);
    }
    entry
}

static REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 0,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 0,
};

fn texture(entry: &SpriteEntry) -> RenderResource<'_> {
    RenderResource {
        request: RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 0,
        },
        source: RenderSource {
            registration: &REGISTRATION,
            bytes: &[],
        },
        data: RenderResourceData::Sprite(entry),
    }
}

fn request(
    entry: &SpriteEntry,
    mask: u8,
    orientation: [u8; 4],
    diagonal: u8,
) -> TerrainClipRequest<'_> {
    let colors = [[240, 120, 60], [10, 20, 30], [127, 128, 129], [0, 255, 64]];
    let flags = [0x10, 0x20 | diagonal, 0x30, 0x40];
    let corners = std::array::from_fn(|i| {
        let mut projection = crate::model_vertices::ModelVertexProjection::default();
        projection.write_u32(0, 0x00e6014a + i as u32);
        projection.write_u32(4, if mask & (1 << i) != 0 { 16 } else { 256 });
        projection.bytes[8] = Some(if mask & (1 << i) != 0 { 0x40 } else { 0x12 });
        projection.bytes[9] = Some(10 + i as u8 * 20);
        TerrainVertex {
            rgb: colors[i],
            cell_flags: flags[i],
            projection,
            world: [0xffff; 3],
        }
    });
    TerrainClipRequest {
        corners,
        eye_relative: [[0, -32, 0], [0, 32, 256], [256, 64, 256], [256, -64, 0]],
        context: context(),
        texture: Some(texture(entry)),
        orientation: orientation
            .into_iter()
            .enumerate()
            .fold(0, |word, (i, target)| word | (u32::from(target) << (i * 8))),
        sort_bias: -129,
        material: 0x12345678,
    }
}

#[derive(Default)]
struct Host<'a> {
    commands: Vec<PrimitiveCommand<'a>>,
    sorting: bool,
    sort_calls: usize,
    identity_calls: usize,
    fail_after: Option<usize>,
}
impl<'a> TerrainClippingHost<'a> for Host<'a> {
    type Error = &'static str;
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.sort_calls += 1;
        Ok(self.sorting)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        self.identity_calls += 1;
        Ok(TextureSortIdentity::NativeEntryAddress(0x01003000))
    }
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        if self.fail_after == Some(self.commands.len()) {
            return Err("queue full");
        }
        self.commands.push(command);
        Ok(())
    }
}

#[test]
fn terrain_clip_gate_precedes_texture_and_geometry_reads() {
    let sprite = entry(0, 0);
    for mask in [0, 15] {
        let mut request = request(&sprite, mask, [0, 1, 2, 3], 0);
        request.texture = None;
        for corner in &mut request.corners {
            corner.projection.bytes[..8].fill(None);
        }
        let mut host = Host::default();
        assert_eq!(clip_terrain_tile(request, &tables(), &mut host).unwrap(), 0);
        assert_eq!(host.sort_calls, 0);
    }
}

#[test]
fn terrain_clip_material_policy_uses_raw_signed_bias_and_first_dimensions() {
    for (selector, flags, sorting, key, identities) in [
        (0, 0, false, (-65_i32) as u32, 0),
        (1, 0, true, 0x40801810, 3),
        (0, 8, true, 0x3fffffdf, 0),
        (255, 0, true, 0x3fffffdf, 0),
    ] {
        let sprite = entry(selector, flags);
        let mut host = Host {
            sorting,
            ..Host::default()
        };
        let count =
            clip_terrain_tile(request(&sprite, 9, [0, 1, 2, 3], 0), &tables(), &mut host).unwrap();
        assert_eq!(count, 3);
        assert_eq!(host.sort_calls, count);
        assert_eq!(host.identity_calls, identities);
        for command in &host.commands {
            assert_eq!(
                (
                    command.callback,
                    command.depth,
                    command.payload_blocks,
                    command.header_words
                ),
                (PrimitiveCallback::TexturedTriangleFar, key, 5, [0; 3])
            );
            assert_eq!(command.texture.unwrap().0, 0x1c);
            assert_eq!(command.read_u32(0x18), Some(0));
            assert_eq!(command.read_u32(0x44), Some(0x12345678));
            assert!(command.payload[0x4b..].iter().all(Option::is_none));
        }
    }
}

#[test]
fn terrain_clip_keeps_submission_prefix_and_per_triangle_sorting() {
    let sprite = entry(0, 0);
    let mut host = Host {
        fail_after: Some(1),
        ..Host::default()
    };
    assert!(matches!(
        clip_terrain_tile(request(&sprite, 9, [0, 1, 2, 3], 0), &tables(), &mut host),
        Err(TerrainDrawError::Host("queue full"))
    ));
    assert_eq!(host.commands.len(), 1);
    assert_eq!(host.sort_calls, 2);
}

#[test]
fn terrain_crossing_ignores_behind_xy_and_preserves_visible_fog() {
    let sprite = entry(0, 0);
    let mut request = request(&sprite, 9, [0, 1, 2, 3], 0);
    for index in [0, 3] {
        request.corners[index].projection.bytes[..4].fill(None);
    }
    let mut host = Host::default();
    assert_eq!(clip_terrain_tile(request, &tables(), &mut host).unwrap(), 3);
    assert!(host
        .commands
        .iter()
        .flat_map(|c| &c.payload[0x48..0x4b])
        .any(|fog| *fog == Some(30)));
}

#[test]
fn terrain_projection_has_native_truncation_order_and_fog_product() {
    let mut context = context();
    context.transform.view_basis.words[1] = 0.5_f32.to_bits();
    let result = project([3, -3, 65], context);
    // View X1.5 truncates1 before projection; negative Y uses truncation too.
    assert_eq!(result.xy, 320 | (242 << 16));
    assert_eq!((result.depth, result.clip, result.fog), (65, 0x12, Some(1)));
    assert_eq!(divide_q31(0, 0), i32::MAX);
    assert_eq!(divide_q31(-64, 32), -i32::MAX);
    assert_eq!(multiply_q31(-3, 0x40000000), -2);
}

#[test]
fn terrain_clipping_matches_eight_native_supplied_input_packet_cases() {
    // First accepted packet and total command count from eight cases in the
    // tracked check_terrain_tile_clipping.py matrix (identity orientation,
    // no sorting). RGB DWORD high bytes and pointer relocations are separate.
    let cases: [(u8, u8, usize, [u32; 18], [u8; 3]); 8] = [
        (
            1,
            0,
            2,
            [
                0x00e6014b, 0x100, 0x00e6014c, 0x100, 0x00e6014d, 0x100, 0, 0, 0x001e140a,
                0x0081807f, 0x0040ff00, 0x10000, 0, 0x10000, 0x10000, 0, 0x10000, 0x12345678,
            ],
            [30, 50, 70],
        ),
        (
            1,
            8,
            3,
            [
                0x00f80180, 0x40, 0x01000140, 0x40, 0x00e6014b, 0x100, 0, 0, 0x004a7ada,
                0x003664c2, 0x001e140a, 0x4924, 0x38e3, 0x4924, 0, 0x10000, 0, 0x12345678,
            ],
            [0, 0, 30],
        ),
        (
            2,
            0,
            4,
            [
                0x00ec0150, 0xcd, 0x00e7014d, 0x100, 0x00e6014c, 0x100, 0, 0, 0x00254408,
                0x00322a22, 0x0081807f, 0xdb6d, 0x38e3, 0x10000, 0x38e3, 0x10000, 0x10000,
                0x12345678,
            ],
            [187, 255, 50],
        ),
        (
            2,
            8,
            3,
            [
                0x00e6014a, 0x100, 0x00ea0140, 0xcd, 0x00e7014d, 0x100, 0, 0, 0x003c78f0,
                0x00252939, 0x00322a22, 0, 0, 0xdb6d, 0, 0x10000, 0x38e3, 0x12345678,
            ],
            [10, 187, 255],
        ),
        (
            3,
            0,
            2,
            [
                0x00ec0150, 0xcd, 0x00e7014d, 0x100, 0x00e6014c, 0x100, 0, 0, 0x00254408,
                0x00322a22, 0x0081807f, 0xdb6d, 0x38e3, 0x10000, 0x38e3, 0x10000, 0x10000,
                0x12345678,
            ],
            [187, 255, 50],
        ),
        (
            3,
            8,
            2,
            [
                0x00f80180, 0x40, 0x00e7014d, 0x100, 0x00e6014c, 0x100, 0, 0, 0x004a7ada,
                0x00322a22, 0x0081807f, 0x4924, 0x38e3, 0x10000, 0x38e3, 0x10000, 0x10000,
                0x12345678,
            ],
            [0, 255, 50],
        ),
        (
            9,
            0,
            3,
            [
                0x01180201, 0x40, 0x00e6014b, 0x100, 0x00e6014c, 0x100, 0, 0, 0x003ad003,
                0x001e140a, 0x0081807f, 0x4924, 0xe38e, 0x10000, 0, 0x10000, 0x10000, 0x12345678,
            ],
            [0, 30, 50],
        ),
        (
            9,
            8,
            3,
            [
                0x00f80180, 0x40, 0x01000140, 0x40, 0x00e6014b, 0x100, 0, 0, 0x004a7ada,
                0x003664c2, 0x001e140a, 0x4924, 0x38e3, 0x4924, 0, 0x10000, 0, 0x12345678,
            ],
            [0, 0, 30],
        ),
    ];
    let sprite = entry(0, 0);
    for (mask, diagonal, count, words, fog) in cases {
        let mut host = Host::default();
        assert_eq!(
            clip_terrain_tile(
                request(&sprite, mask, [0, 1, 2, 3], diagonal),
                &tables(),
                &mut host
            )
            .unwrap(),
            count
        );
        let command = &host.commands[0];
        for (i, expected) in words.into_iter().enumerate() {
            if i == 7 {
                continue;
            } // Borrowed native texture relocation.
            let actual = if (8..11).contains(&i) {
                let bytes = &command.payload[i * 4..i * 4 + 3];
                u32::from_le_bytes([bytes[0].unwrap(), bytes[1].unwrap(), bytes[2].unwrap(), 0])
            } else {
                command.read_u32(i * 4).unwrap()
            };
            assert_eq!(
                actual,
                expected,
                "mask{mask:x} diagonal{diagonal:x} packet+{:x}",
                i * 4
            );
        }
        assert_eq!(command.payload[0x48..0x4b], fog.map(Some));
    }
}
