use super::*;
use inf_formats::{ByteSpan, Font, FontMode, GlyphMetrics};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

type Events = Rc<RefCell<Vec<Value>>>;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/gameplay_overlay_native.json"
    ))
    .unwrap()
}

struct Resources {
    input: Value,
    font: Font,
    text: Vec<u8>,
    events: Events,
}

impl Resources {
    fn lookup(&self, kind: &str, argument: i32) -> Result<(), &'static str> {
        self.events
            .borrow_mut()
            .push(json!({"kind":kind,"argument":argument}));
        if self.input["fail"].as_str() == Some(kind) {
            Err("supplied resource")
        } else {
            Ok(())
        }
    }
}

impl MenuTextResources for Resources {
    type Error = &'static str;
    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.lookup("string", argument)?;
        Ok(&self.text)
    }
    fn font(&self, argument: i32) -> Result<&Font, Self::Error> {
        self.lookup("font", argument)?;
        Ok(&self.font)
    }
    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        self.lookup("style", argument)?;
        Ok(self.input["style"].as_u64().unwrap() as u32)
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        panic!("fixture supplies the raw text boundary, not glyph rendering")
    }
}

impl GameplayOverlayResources for Resources {
    fn material_word(&self, argument: i32) -> Result<u32, Self::Error> {
        self.lookup("material", argument)?;
        Ok(self.input["material"].as_u64().unwrap() as u32)
    }
}

struct Host {
    events: Events,
    failure: Option<String>,
    quads: usize,
}

impl GameplayOverlayHost for Host {
    type Error = &'static str;
    fn flat_quad(&mut self, quad: DirectFlatQuad) -> Result<(), Self::Error> {
        self.events.borrow_mut().push(json!({"kind":"quad", "corners":quad.corners,
            "depths":quad.vertex_depths,"descriptor":quad.descriptor_word,"color":quad.color_word}));
        self.quads += 1;
        if self.failure.as_deref() == Some(format!("quad{}", self.quads).as_str()) {
            Err("supplied quad")
        } else {
            Ok(())
        }
    }
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        let MenuTextReveal::Timed { age_millis } = request.reveal else {
            panic!("native timed text")
        };
        assert_eq!(request.emission, MenuTextEmission::Draw);
        assert_eq!(request.context_origin_pixels, [0, 0]);
        assert_eq!(request.base_color, MenuColor::from_word(0x00808080));
        self.events.borrow_mut().push(json!({"kind":"text","position":request.position_pixels,
            "width":request.width_pixels,"color":u32::from_le_bytes([
                request.color.rgb[0], request.color.rgb[1], request.color.rgb[2], request.color.high_byte.unwrap()]),
            "callback":0x439de0,"flags":2,"age":age_millis}));
        if self.failure.as_deref() == Some("text") {
            Err("supplied text")
        } else {
            Ok(())
        }
    }
}

#[test]
fn gameplay_overlay_matches_native_viewport_caption_and_failure_prefixes() {
    let fixture = fixture();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let events = Events::default();
        let resources = Resources {
            input: input.clone(),
            text: input["text"].as_str().unwrap().as_bytes().to_vec(),
            font: Font {
                local_index: 0,
                header_span: ByteSpan::new(0, 64),
                raw_header: [0; 16],
                mode: FontMode::Direct,
                coordinate_divisor: input["divisor"].as_i64().unwrap() as i32,
                line_step_terms: [1000, 200],
                maximum_byte: 255,
                fallback_byte: b'?',
                metric_span: ByteSpan::new(64, 2112),
                sprite_span: ByteSpan::new(2112, 3136),
                metrics: vec![
                    GlyphMetrics {
                        advance: input["advance"].as_i64().unwrap() as i16,
                        spacing: input["spacing"].as_i64().unwrap() as i16,
                        offset: [0, 0]
                    };
                    256
                ],
                sprite_words: (1..=256).collect(),
            },
            events: events.clone(),
        };
        let mut host = Host {
            events: events.clone(),
            failure: input["fail"].as_str().map(str::to_owned),
            quads: 0,
        };
        let result = draw_gameplay_overlay(
            GameplayOverlayRequest {
                current_world: input["world"].as_u64().unwrap() as u32,
                surface_pixels: [
                    input["width"].as_i64().unwrap() as i32,
                    input["height"].as_i64().unwrap() as i32,
                ],
                screen_age_millis: input["screen_age"].as_i64().unwrap() as i32,
                glyph_base_color: MenuColor::from_word(0x00808080),
            },
            &resources,
            &mut host,
        );
        assert_eq!(
            result.is_ok(),
            case["stopped"].is_null(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(
            *events.borrow(),
            *case["events"].as_array().unwrap(),
            "{}",
            case["name"]
        );
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 42);
}

#[test]
fn gameplay_overlay_native_segment_and_clock_prefix_are_guarded() {
    use sha2::{Digest, Sha256};
    let fixture = fixture();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        fixture["retail_sha256"].as_str().unwrap()
    );
    for guard in fixture["source_guards"].as_array().unwrap() {
        let start = guard["address"].as_u64().unwrap() as usize - 0x400000;
        let length = guard["length"].as_u64().unwrap() as usize;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[start..start + length])),
            guard["sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn gameplay_overlay_resident_pixels_keep_world_view_and_reveal_real_caption() {
    use crate::{
        frontend_assets::FrontendAssets,
        frontend_display::frontend_startup,
        model_primitives::PrimitiveTables,
        render_resources::RenderResources,
        software_frame::SoftwareFrame,
        software_scene::{SoftwareScene, SoftwareSceneRequest},
        Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let tables = PrimitiveTables::from_executable(executable).unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 4,
                language: 0,
            },
        })
        .unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        // Mission selection 22 uses resource leaf 32; the two IDs differ.
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let resources = RenderResources::new(&resident);
    assert!(std::str::from_utf8(resources.string(223).unwrap())
        .unwrap()
        .contains("MilCon"));
    let startup = frontend_startup(
        &resident,
        crate::presentation::NativeFrame::original([1024, 768]),
    )
    .unwrap();
    let untouched = [31, 43, 61, 255];
    let mut pixels = SoftwareFrame::new(1024, 768, untouched);
    let mut scene = SoftwareScene::new(SoftwareSceneRequest {
        model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
            crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
            startup.scene.environment,
        ),
        resources,
        tables: &tables,
        frame: &mut pixels,
        config: startup.scene,
    })
    .unwrap();
    scene.begin_frame().unwrap();
    scene.set_depth_policy(1, 1).unwrap();
    // Same native +14(NULL) boundary as the production world-draw tail.
    scene.reset_renderer();
    assert!(!scene.pixels.pipeline.depth_enabled);
    assert!(scene.pixels.pipeline.depth_write);
    assert_eq!(scene.pixels.renderer.depth_enable, 1);
    let projection = scene.environment.projection;
    let queue = scene.queue.state();
    software::draw(&mut scene, resources, 22, 0).unwrap();
    for y in [0, 50, 94, 700, 750, 767] {
        assert_eq!(scene.pixels.frame.pixel_rgba(10, y), Some([0, 0, 0, 255]));
    }
    for y in [100, 300, 500, 650] {
        assert_eq!(scene.pixels.frame.pixel_rgba(10, y), Some(untouched));
    }
    let before = scene.pixels.frame.rgba().to_vec();
    software::draw(&mut scene, resources, 22, 6000).unwrap();
    assert_eq!(scene.environment.projection, projection);
    assert_eq!(scene.queue.state(), queue);
    let changed: Vec<_> = scene
        .pixels
        .frame
        .rgba()
        .chunks_exact(4)
        .zip(before.chunks_exact(4))
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert!(
        !changed.is_empty(),
        "actual caption must reach glyph pixels"
    );
    assert!(
        changed
            .iter()
            .all(|&index| (672..768).contains(&(index / 1024))),
        "caption stays within bottom mask"
    );
    let output = root.join("../.tmp/inf-gameplay-overlay-native");
    std::fs::create_dir_all(&output).unwrap();
    let file =
        std::io::BufWriter::new(std::fs::File::create(output.join("surface1024.png")).unwrap());
    let mut encoder = png::Encoder::new(file, 1024, 768);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(scene.pixels.frame.rgba())
        .unwrap();
}
