//! Native supplied-host receipts for both1A dispatches and shared1B geometry.
//! Expected bytes are produced by authenticated PE instructions, not Rust math.

use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipts {
    exe_sha256: String,
    texture_address: u32,
    cases: Vec<Receipt>,
}

#[derive(Deserialize)]
struct Input {
    name: String,
    opcode: u16,
    far: bool,
    destination: u16,
    slot: i32,
    center: [i16; 2],
    depth: u32,
    clip: u8,
    fog: u8,
    operands: [u16; 2],
    dimensions: [u16; 2],
    focal: [i32; 2],
    bounds: [i32; 2],
    mode: u32,
    selectors: [u8; 2],
    flags: [u8; 2],
    sorting: bool,
    manages_depth: bool,
    far_color: u32,
    effect_color: u32,
    reject_all: bool,
    null_texture: bool,
    fail: Option<String>,
}

#[derive(Deserialize)]
struct NativeCommand {
    key: u32,
    header: [u32; 4],
    blocks: usize,
    payload: Option<Vec<Option<u8>>>,
}

#[derive(Deserialize)]
struct Receipt {
    input: Input,
    events: Vec<String>,
    error: Option<String>,
    register0: u16,
    command: Option<NativeCommand>,
}

struct NativeHost<'a> {
    input: &'a Input,
    texture: Option<RenderResource<'a>>,
    texture_address: u32,
    events: Vec<String>,
    attempted: Option<PrimitiveCommand<'a>>,
}

impl NativeHost<'_> {
    fn fail(&self, boundary: &str) -> Result<(), &'static str> {
        if self.input.fail.as_deref() == Some(boundary) {
            Err("supplied required host failure")
        } else {
            Ok(())
        }
    }
}

impl ModelValueCallback for NativeHost<'_> {
    type Error = &'static str;
    fn packed_value(
        &mut self,
        values: &mut ModelValues,
        selector: u32,
    ) -> Result<u16, Self::Error> {
        let boundary = format!("value:{selector}");
        self.events.push(boundary.clone());
        values.write_register(0, 99).unwrap();
        self.fail(&boundary)?;
        Ok(self.input.operands[selector as usize - 1])
    }
}

impl<'a> PrimitiveHost<'a> for NativeHost<'a> {
    fn texture(&mut self, argument: i32) -> Result<Option<RenderResource<'a>>, Self::Error> {
        self.events.push(format!("texture:{argument}"));
        self.fail("texture")?;
        Ok(self.texture)
    }
    fn material(&mut self, _: i32) -> Result<u32, Self::Error> {
        Err("unexpected material")
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.events.push("sort".into());
        self.fail("sort")?;
        Ok(self.input.manages_depth && self.input.sorting)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'a>,
    ) -> Result<crate::model_primitives::TextureSortIdentity, Self::Error> {
        self.events.push("identity".into());
        Ok(crate::model_primitives::TextureSortIdentity::NativeEntryAddress(self.texture_address))
    }
    fn submit(&mut self, command: PrimitiveCommand<'a>) -> Result<(), Self::Error> {
        self.events.push("submit".into());
        self.attempted = Some(command);
        self.fail("submit")
    }
}

#[test]
fn near_far_ground_and_effect_match_unchanged_native_packets_and_prefixes() {
    let receipts: Receipts =
        serde_json::from_str(include_str!("../../tests/fixtures/ground_quad_native.json")).unwrap();
    assert_eq!(
        receipts.exe_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(receipts.cases.len(), 96);
    let executable = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let parsed = inf_formats::FrontendExecutable::parse(&executable).unwrap();
    let tables = PrimitiveTables::from_executable(parsed).unwrap();
    for receipt in &receipts.cases {
        let input = &receipt.input;
        let mut entry = sprite_entry(input.dimensions[0], input.dimensions[1]);
        entry.selectors.mode0 = input.selectors[0];
        entry.selectors.mode1 = input.selectors[1];
        entry.variants[0].flags = SpriteFlags::new(u16::from(input.flags[0]));
        let mut second = entry.variants[0].clone();
        second.physical_index = 1;
        second.flags = SpriteFlags::new(u16::from(input.flags[1]));
        entry.variants.push(second);
        let mut points = vec![point(0, 0, 0, 0, Some(0)); 2];
        points[usize::from(input.destination)] = point(
            input.center[0],
            input.center[1],
            input.depth,
            input.clip,
            Some(input.fog),
        );
        let fixture = Fixture::new(
            &[
                input.opcode,
                input.destination,
                input.slot as u16,
                0x81,
                0x82,
                0,
            ],
            points,
        );
        let mut state = state();
        state.mode = input.mode;
        state.far_color = input.far_color;
        state.effect_color = input.effect_color;
        state.projection.focal = input.focal;
        // 1A and1B both clip against the published viewport0055F234/38,
        // which this native matrix supplies as its bounds.
        state.projection.viewport = input.bounds;
        let mut host = NativeHost {
            input,
            texture: (!input.null_texture).then(|| textured_sprite(&entry, input.slot)),
            texture_address: receipts.texture_address,
            events: Vec::new(),
            attempted: None,
        };
        let mut values = values();
        values.write_register(0, 0).unwrap();
        let mut case_tables = tables.clone();
        if input.reject_all {
            case_tables.clip_acceptance = [0; 256];
        }
        let request = fixture.request(if input.far {
            ModelDispatch::Far
        } else {
            ModelDispatch::Near
        });
        let result = if input.opcode == 26 {
            prepare_ground(&request, state, &case_tables, &mut values, &mut host)
        } else {
            crate::model_primitives::effect::prepare_effect(
                &request,
                state,
                &case_tables,
                &mut values,
                &mut host,
            )
        };
        assert_eq!(
            result.is_err(),
            receipt.error.is_some(),
            "{}: {result:?}",
            input.name
        );
        match receipt.error.as_deref() {
            Some("texture" | "sort" | "submit") => {
                assert!(
                    matches!(&result, Err(PrimitiveError::Host(_))),
                    "{}",
                    input.name
                );
            }
            Some("value:1" | "value:2") => {
                assert!(
                    matches!(&result, Err(PrimitiveError::Value(_))),
                    "{}",
                    input.name
                );
            }
            Some("null-texture" | "division" | "unknown-depth") => {
                assert!(
                    matches!(
                        &result,
                        Err(PrimitiveError::Unsupported { opcode: 0x1a, .. })
                    ),
                    "{}",
                    input.name
                );
            }
            None => {}
            other => panic!("unexpected native boundary {other:?}"),
        }
        assert_eq!(host.events, receipt.events, "{}", input.name);
        assert_eq!(
            values.register(0).unwrap(),
            Some(receipt.register0),
            "{}",
            input.name
        );
        match (&host.attempted, &receipt.command) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                assert_eq!(actual.depth, expected.key, "{}", input.name);
                assert_eq!(actual.header_words, expected.header[1..], "{}", input.name);
                let callback = match actual.callback {
                    PrimitiveCallback::TexturedQuadNear => 0x466150,
                    PrimitiveCallback::TexturedQuadFar => 0x466160,
                    PrimitiveCallback::EffectQuadNear => 0x40ef30,
                    PrimitiveCallback::EffectQuadFar => 0x411d50,
                    other => panic!("unexpected {other:?}"),
                };
                assert_eq!(callback, expected.header[0], "{}", input.name);
                assert_eq!(actual.payload_blocks, expected.blocks, "{}", input.name);
                if let Some(payload) = &expected.payload {
                    assert_eq!(&actual.payload, payload, "{}", input.name);
                    let (offset, texture) = actual.texture.unwrap();
                    assert_eq!(offset, 0x24);
                    assert_eq!(texture.request.argument, input.slot);
                } else {
                    assert_eq!(receipt.error.as_deref(), Some("submit"));
                }
            }
            _ => panic!("{}: native allocation prefix disagrees", input.name),
        }
        if let Ok(outcome) = result {
            assert_eq!(outcome.commands, usize::from(receipt.command.is_some()));
            assert_eq!(outcome.rejected, usize::from(receipt.command.is_none()));
        }
    }
}
