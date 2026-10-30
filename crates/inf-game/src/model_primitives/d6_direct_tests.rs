use super::*;
use crate::{
    model_lighting::{ModelLighting, ModelScratchAllocation},
    model_primitives::TextureSortIdentity,
    render_resources::{RenderResourceKind, RenderResourceRequest},
};
use inf_formats::{
    SpriteAtlasOrigin, SpriteEncoding, SpriteEntry, SpriteFlags, SpriteModeSelectors, SpriteVariant,
};
use serde_json::{json, Value};

fn u32_field(spec: &Value, key: &str, fallback: u32) -> u32 {
    spec[key].as_u64().map(|v| v as u32).unwrap_or(fallback)
}
fn sprite(spec: &Value) -> SpriteEntry {
    let selectors = spec["selectors"]
        .as_array()
        .map(|a| [a[0].as_u64().unwrap() as u8, a[1].as_u64().unwrap() as u8])
        .unwrap_or([0, 1]);
    let flags = spec["texture_flags"]
        .as_array()
        .map(|a| [a[0].as_u64().unwrap() as u16, a[1].as_u64().unwrap() as u16])
        .unwrap_or([0, 8]);
    SpriteEntry {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        selectors: SpriteModeSelectors {
            mode0: selectors[0],
            mode1: selectors[1],
            reserved: 0,
        },
        variants: (0..2)
            .map(|i| SpriteVariant {
                physical_index: i,
                span: ByteSpan::new(0, 0),
                sprite_id: 0,
                reserved: 0,
                flags: SpriteFlags::new(flags[usize::from(i)]),
                color_count: 0,
                atlas_offset_bytes: 0,
                palette_offset: 0,
                width: 1,
                height: 1,
                runtime_slots: [0; 2],
                atlas_origin: SpriteAtlasOrigin { x_bytes: 0, y: 0 },
                atlas_record_index: 0,
                encoding: SpriteEncoding::DirectRgb555 {
                    pixels: ByteSpan::new(0, 2),
                },
            })
            .collect(),
    }
}
const TEXTURE_REGISTRATION: ResidentRegistration = ResidentRegistration {
    level: 0,
    phase: RequestKind::Base,
    handler: 3,
    region_index: 0,
    local_index: 0,
    global_index: 10,
};
struct DirectHost<'a> {
    spec: &'a Value,
    entry: &'a SpriteEntry,
    events: Vec<Value>,
    commands: Vec<PrimitiveCommand<'a>>,
    lookups: usize,
}
impl DirectHost<'_> {
    fn boundary(&self, name: &str) -> Result<(), &'static str> {
        if self.spec["fail"].as_str() == Some(name) {
            Err("required host failed")
        } else {
            Ok(())
        }
    }
}
impl ModelValueCallback for DirectHost<'_> {
    type Error = &'static str;
    fn packed_value(&mut self, _: &mut ModelValues, _: u32) -> Result<u16, Self::Error> {
        unreachable!()
    }
}
impl<'a> PrimitiveHost<'a> for DirectHost<'a> {
    fn texture(&mut self, slot: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.events.push(json!(["texture", slot]));
        self.lookups += 1;
        self.boundary(&format!("texture:{}", self.lookups))?;
        if slot < 0
            || slot as u32 > u32_field(self.spec, "capacity", 10)
            || (if self.lookups % 2 == 0 && self.spec["second_null"].is_boolean() {
                self.spec["second_null"].as_bool().unwrap()
            } else {
                self.spec["null_texture"].as_bool().unwrap_or(false)
            })
        {
            return Ok(None);
        }
        Ok(Some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: slot,
            },
            source: RenderSource {
                registration: &TEXTURE_REGISTRATION,
                bytes: &[],
            },
            data: RenderResourceData::Sprite(self.entry),
        }))
    }
    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        unreachable!()
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
        let selector = if u32_field(self.spec, "mode", 0) & 1 == 0 {
            self.entry.selectors.mode0
        } else {
            self.entry.selectors.mode1
        };
        self.events.push(json!(["identity", selector]));
        self.boundary("identity")?;
        Ok(TextureSortIdentity::NativeEntryAddress(0x72b000))
    }
    fn submit(&mut self, c: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push(json!(["submit"]));
        if self.spec["fail"].as_str() == Some("submit")
            && self.commands.len() == u32_field(self.spec, "fail_after", 0) as usize
        {
            return Err("required host failed");
        }
        self.commands.push(c);
        Ok(())
    }
}
fn make_fixture(spec: &Value) -> Fixture {
    let raw = u32_field(spec, "raw", 0xd6) as u16;
    let slots = spec["slots"]
        .as_array()
        .map(|a| std::array::from_fn::<_, 4, _>(|i| a[i].as_u64().unwrap() as u16))
        .unwrap_or([0, 1, 2, 3]);
    let points = spec["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let mut v = ModelVertexProjection::default();
            v.write_u32(0, p[0].as_u64().unwrap() as u32);
            v.write_u32(4, p[1].as_i64().unwrap() as u32);
            v.bytes[8] = Some(p[2].as_u64().unwrap() as u8);
            v.bytes[9] = Some(p[3].as_u64().unwrap() as u8);
            v
        })
        .collect();
    let refs = spec["shade_slots"]
        .as_array()
        .map(|a| std::array::from_fn::<_, 4, _>(|i| a[i].as_i64().unwrap() as u16))
        .unwrap_or([0, 1, 0, 1]);
    let mut f = Fixture::new(
        &[
            raw,
            spec["texture_slot"].as_i64().unwrap_or(10) as u16,
            slots[0],
            slots[1],
            slots[2],
            slots[3],
            refs[0],
            refs[1],
            refs[2],
            refs[3],
            0,
        ],
        points,
    );
    f.shades =
        ModelShades::in_scratch(ModelScratchAllocation::new(0, 4, 4).unwrap(), 0xff808080).unwrap();
    if spec["name"].as_str().unwrap().ends_with("computed-shade") {
        let normal_start = f.bytes.len();
        f.bytes
            .extend([4096_i16, 0, 0].into_iter().flat_map(i16::to_le_bytes));
        f.model.span = ByteSpan::new(0, f.bytes.len());
        f.model.content_span = f.model.span;
        f.model.normals_span = ByteSpan::new(normal_start, normal_start + 6);
        f.model.header.reserved_1a = 2;
        f.model.header.shade_ranges = [2, 4];
        f.shades
            .initialize_normals(
                &f.model,
                &f.bytes,
                &MenuBasis::INITIAL,
                ModelLighting {
                    direction: [1., 0., 0.],
                    directional_word: 0x00402010,
                    ambient: [7, 9, 11],
                    ambient_word: 0xff808080,
                },
            )
            .unwrap();
    }
    f
}

fn complete_packet_events(native: &[Value]) -> Vec<Value> {
    // Native allocates before the second H3 lookup; the portable sink
    // receives a complete packet. This adapter compares the reached
    // non-allocation services while naming the single protocol difference.
    let mut events = native.to_vec();
    let mut index = 0;
    while index + 1 < events.len() {
        if events[index][0] == "submit" && events[index + 1][0] == "texture" {
            events.swap(index, index + 1);
            index += 1;
        }
        index += 1;
    }
    events
}

#[test]
fn direct_d6_matches_original_double_lookup_signed_shades_and_packets() {
    let receipt: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/d6_native.json")).unwrap();
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 160);
    let tables = tables();
    for spec in receipt["cases"].as_array().unwrap() {
        let fixture = make_fixture(spec);
        let entry = sprite(spec);
        let mut host = DirectHost {
            spec,
            entry: &entry,
            events: Vec::new(),
            commands: Vec::new(),
            lookups: 0,
        };
        let mut state = state();
        state.mode = u32_field(spec, "mode", 0);
        state.far_color = u32_field(spec, "far_color", 0x12345678);
        let result = prepare_d6(
            &fixture.request(if spec["far"].as_bool().unwrap() {
                ModelDispatch::Far
            } else {
                ModelDispatch::Near
            }),
            state,
            &tables,
            &mut host,
        );
        assert_eq!(
            result.is_err(),
            !spec["error"].is_null(),
            "{} {result:?}",
            spec["name"]
        );
        let native = spec["events"].as_array().unwrap();
        let partial = spec["error"].as_str() == Some("texture:2");
        // Allocation failure and later packet-copy failure are deliberately
        // outside complete-packet fault parity. Earlier service errors and
        // every successful complete packet still compare exactly.
        if spec["error"].as_str() != Some("submit") && !partial {
            assert_eq!(
                host.events,
                complete_packet_events(native),
                "{}",
                spec["name"]
            );
        }
        let packets = spec["packets"].as_array().unwrap();
        assert_eq!(
            host.commands.len(),
            packets.len() - usize::from(partial),
            "{}",
            spec["name"]
        );
        for (actual, expected) in host.commands.iter().zip(packets) {
            assert_eq!(
                actual.callback.native_address(),
                expected["callback"].as_u64().unwrap() as u32,
                "{}",
                spec["name"]
            );
            assert_eq!(
                actual.depth,
                expected["depth"].as_u64().unwrap() as u32,
                "{}",
                spec["name"]
            );
            assert_eq!(
                json!(actual.header_words),
                expected["header_words"],
                "{}",
                spec["name"]
            );
            assert_eq!(
                actual.payload_blocks,
                expected["blocks"].as_u64().unwrap() as usize,
                "{}",
                spec["name"]
            );
            assert_eq!(
                json!(actual.payload),
                expected["payload"],
                "{}",
                spec["name"]
            );
            assert_eq!(
                actual.texture.is_some(),
                expected["texture"].as_bool().unwrap(),
                "{}",
                spec["name"]
            );
        }
        if partial {
            let payload = packets.last().unwrap()["payload"].as_array().unwrap();
            assert!(
                payload[..0x24].iter().all(|byte| !byte.is_null()),
                "{} native geometry/mode prefix",
                spec["name"]
            );
            assert!(
                payload[0x24..].iter().all(Value::is_null),
                "{} native later fields unwritten",
                spec["name"]
            );
            assert_eq!(native.last().unwrap(), &json!(["texture", 10]));
            assert!(
                host.commands.is_empty(),
                "complete-packet sink has no partial current allocation"
            );
        }
    }
}

#[test]
fn mapped_negative_shades_retain_unknown_bytes_after_fresh_texture_lookup() {
    let spec = json!({"name":"negative", "far":false,"raw":0xd6,"shade_slots":[-1,0,1,-1],
            "points":[[300|(220<<16),128,0x12,10],[340|(220<<16),129,0x12,20],
                       [340|(260<<16),130,0x12,30],[300|(260<<16),131,0x12,40]]});
    let fixture = make_fixture(&spec);
    let entry = sprite(&spec);
    let mut host = DirectHost {
        spec: &spec,
        entry: &entry,
        events: Vec::new(),
        commands: Vec::new(),
        lookups: 0,
    };
    assert_eq!(
        prepare_d6(
            &fixture.request(ModelDispatch::Near),
            state(),
            &tables(),
            &mut host
        )
        .unwrap()
        .commands,
        1
    );
    assert_eq!(
        host.events,
        vec![
            json!(["texture", 10]),
            json!(["sorting"]),
            json!(["texture", 10]),
            json!(["submit"])
        ]
    );
    assert_eq!(host.commands[0].payload[0x28..0x2c], [None; 4]);
    assert_eq!(host.commands[0].payload[0x34..0x38], [None; 4]);
}
