//! Real resident resources through typed presentation requests. The downstream
//! recorder proves bytes, registration and command data, not rasterized pixels.

use std::{collections::BTreeMap, path::Path};

use inf_formats::{MaterialValue, SpriteModeSelectors, SpriteRenderMode, SpriteShade};
use inf_game::frontend_assets::FrontendAssets;
use inf_game::frontend_camera::AnimationClock;
use inf_game::frontend_scene::{
    draw_footer, FrontendScene, FrontendSceneHost, SceneCommand, SceneFrame, SceneProjection,
    SceneView,
};
use inf_game::menu::MenuBasis;
use inf_game::render_resources::*;
use inf_game::scene_resources::*;
use inf_game::{Installation, ResidencyRequest, ResidentRuntime, ResourceSelection};

fn runtime() -> ResidentRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    runtime
}
fn request(kind: RenderResourceKind, argument: i32) -> RenderResourceRequest {
    RenderResourceRequest { kind, argument }
}

#[test]
fn typed_frontend_resources_use_local_indices_and_keep_original_bytes() {
    let runtime = runtime();
    let resources = RenderResources::new(&runtime);
    for slot in [204, 158, 210] {
        let resource = resources
            .resolve(request(RenderResourceKind::String, slot))
            .unwrap();
        let RenderResourceData::String(text) = resource.data else {
            panic!("string")
        };
        assert!(!text.is_empty());
        assert!(!text.contains(&0));
        assert_eq!(resource.source.registration.handler, 0);
    }
    for slot in [744, 749, 754, 756, 745, 748, 747, 755, 757] {
        let resource = resources
            .resolve(request(RenderResourceKind::Model, slot))
            .unwrap();
        let RenderResourceData::Model(model) = resource.data else {
            panic!("model")
        };
        assert_eq!(model.identity.global_index, slot as u16);
        assert_eq!(
            model.identity.local_index,
            resource.source.registration.local_index
        );
        assert_ne!(resource.source.registration.local_index, slot as u16);
        assert!(!model
            .content_span
            .get(resource.source.bytes)
            .unwrap()
            .is_empty());
    }
    let font = resources
        .resolve(request(RenderResourceKind::Font, 0))
        .unwrap();
    let RenderResourceData::Font(font) = font.data else {
        panic!("font")
    };
    assert_eq!(font.coordinate_divisor, 400);
    assert_eq!(font.metrics.len(), 128);
    for slot in [1807, 1806, 1809, 1808, 1810, 1811] {
        let resource = resources
            .resolve(request(RenderResourceKind::Sprite, slot))
            .unwrap();
        let sprite = resource.select_sprite(SpriteRenderMode::Mode0).unwrap();
        let pixels = sprite
            .variant
            .decode_rgba(sprite.source.bytes, SpriteShade::Base)
            .unwrap();
        assert_eq!(
            pixels.len(),
            usize::from(sprite.variant.width) * usize::from(sprite.variant.height)
        );
        assert!(pixels.iter().any(|pixel| pixel.alpha != 0));
    }
}

#[test]
fn low_word_sign_and_residency_are_explicit_without_fallback() {
    let mut runtime = runtime();
    let resources = RenderResources::new(&runtime);
    let ordinary = resources
        .resolve(request(RenderResourceKind::Style, 14))
        .unwrap();
    let high = resources
        .resolve(request(RenderResourceKind::Style, 0x1234_000e))
        .unwrap();
    assert_eq!(ordinary.source.registration, high.source.registration);
    assert_eq!(high.request.argument, 0x1234_000e);
    assert!(matches!(
        resources.resolve(request(RenderResourceKind::Style, 0xffff)),
        Err(RenderResourceError::NegativeIndex { .. })
    ));
    assert!(matches!(
        resources.resolve(request(RenderResourceKind::Model, 30000)),
        Err(RenderResourceError::NotResident { .. })
    ));
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    assert!(matches!(
        RenderResources::new(&runtime).resolve(request(RenderResourceKind::Model, 744)),
        Err(RenderResourceError::NotResident { .. })
    ));
}

#[test]
fn font_lookup_tracks_difficulty_reload_instead_of_caching_font_zero() {
    let mut runtime = runtime();
    let old = RenderResources::new(&runtime)
        .resolve(request(RenderResourceKind::Font, 0))
        .unwrap();
    let RenderResourceData::Font(font) = old.data else {
        panic!("font")
    };
    assert_eq!(font.coordinate_divisor, 400);
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 1,
                language: 0,
            },
        })
        .unwrap();
    let new = RenderResources::new(&runtime)
        .resolve(request(RenderResourceKind::Font, 0))
        .unwrap();
    let RenderResourceData::Font(font) = new.data else {
        panic!("font")
    };
    assert_eq!(font.coordinate_divisor, 500);
    assert_eq!(font.metrics.len(), 256);
}

#[test]
fn logical_dimensions_and_render_mode_selection_are_independent() {
    let runtime = runtime();
    let resource = RenderResources::new(&runtime)
        .resolve(request(RenderResourceKind::Sprite, 1807))
        .unwrap();
    let RenderResourceData::Sprite(original) = resource.data else {
        panic!("sprite")
    };
    let mut entry = original.clone();
    entry.selectors = SpriteModeSelectors {
        mode0: 0,
        mode1: 1,
        reserved: 0,
    };
    entry.variants.truncate(1);
    let mut second = entry.variants[0].clone();
    second.physical_index = 1;
    second.width = 7;
    second.height = 9;
    entry.variants.push(second);
    let source = resource.source;
    let original_request = resource.request;
    let resource = RenderResource {
        data: RenderResourceData::Sprite(&entry),
        source,
        request: original_request,
    };
    assert_eq!(
        resource.sprite_dimensions().unwrap(),
        [entry.variants[0].width, entry.variants[0].height]
    );
    assert_eq!(
        resource
            .select_sprite(SpriteRenderMode::Mode1)
            .unwrap()
            .variant
            .width,
        7
    );
    entry.selectors.mode1 = 255;
    let resource = RenderResource {
        data: RenderResourceData::Sprite(&entry),
        source,
        request: original_request,
    };
    assert!(matches!(
        resource.select_sprite(SpriteRenderMode::Mode1),
        Err(RenderResourceError::SpriteVariant { .. })
    ));
}

#[test]
fn material_conversion_uses_both_words_wrapping_add_and_x86_shift_counts() {
    let value = MaterialValue::from_raw(0x7c00_03e0);
    let rgb565 = NativeColorLayout {
        red_shift: 11,
        green_shift: 6,
        blue_shift: 0,
    };
    assert_eq!(rgb565.convert_material(value), 0xf800_07c0);
    assert_eq!(
        NativeColorLayout {
            red_shift: 43,
            green_shift: 38,
            blue_shift: 32
        }
        .convert_material(value),
        0xf800_07c0
    );
    assert_eq!(
        NativeColorLayout {
            red_shift: 0,
            green_shift: 0,
            blue_shift: 0
        }
        .convert_material(MaterialValue::from_raw(0x0421_0421)),
        0x0003_0003
    );
    assert_eq!(value.raw(), 0x7c00_03e0);
}

#[derive(Default)]
struct Recorder {
    stars: usize,
    colors: BTreeMap<u16, u32>,
    models: Vec<u16>,
    sprites: Vec<u16>,
    fail_model: bool,
}
impl<'a> SceneResourceBackend<'a> for Recorder {
    type Error = &'static str;
    fn color_layout(&self) -> NativeColorLayout {
        NativeColorLayout {
            red_shift: 11,
            green_shift: 6,
            blue_shift: 0,
        }
    }
    fn sprite_mode(&self) -> SpriteRenderMode {
        SpriteRenderMode::Mode0
    }
    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        Ok([800, 600])
    }
    fn submit(&mut self, command: ResolvedSceneCommand<'_>) -> Result<(), Self::Error> {
        match command {
            ResolvedSceneCommand::Star {
                quad,
                source,
                color_word,
            } => {
                assert_eq!(source.registration.handler, 7);
                assert_eq!(source.registration.global_index, quad.color_slot() as u16);
                self.colors
                    .insert(source.registration.global_index, color_word);
                self.stars += 1;
            }
            ResolvedSceneCommand::Model {
                submission,
                source,
                model,
            } => {
                assert_eq!(
                    source.registration.global_index,
                    model.identity.global_index
                );
                assert_eq!(submission.model_slot as u16, model.identity.global_index);
                if self.fail_model {
                    return Err("runtime model interpreter unavailable");
                }
                self.models.push(model.identity.global_index);
            }
            ResolvedSceneCommand::Footer { quad, sprite } => {
                assert_eq!(sprite.source.registration.global_index, quad.sprite_slot);
                self.sprites.push(quad.sprite_slot);
            }
            _ => {}
        }
        Ok(())
    }
}
fn frame() -> SceneFrame {
    SceneFrame {
        projection: SceneProjection {
            focal: [400, 300],
            viewport: [800, 600],
            fade: [2048, 4096],
        },
        inherited_view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        camera_eye: [0, 0, -70000],
        camera_look_at: [0; 3],
        animation: AnimationClock::default(),
        age_millis: 3000,
        exit_millis: 0,
    }
}

#[test]
fn scene_submission_resolves_actual_models_colors_and_footer_sprites() {
    let runtime = runtime();
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let scene = FrontendScene::new(
        assets.scene_models().to_vec(),
        assets.position_scale(),
        assets.sine_table().values,
    );
    let mut backend = Recorder::default();
    let mut host = ResidentSceneHost {
        resources: RenderResources::new(&runtime),
        backend: &mut backend,
    };
    let outcome = scene.draw_scene(frame(), &mut host).unwrap();
    draw_footer(&mut host).unwrap();
    host.submit(SceneCommand::FlushBatch).unwrap();
    assert_eq!(outcome.generated_stars, 10000);
    assert!(backend.stars > 0);
    for (&slot, &converted) in &backend.colors {
        let resource = RenderResources::new(&runtime)
            .resolve(request(RenderResourceKind::Material, i32::from(slot)))
            .unwrap();
        let RenderResourceData::Material(raw) = resource.data else {
            panic!("material")
        };
        assert_eq!(converted, backend.color_layout().convert_material(raw));
    }
    assert_eq!(
        backend.models,
        [744, 749, 754, 756, 745, 748, 747, 755, 757]
    );
    assert_eq!(backend.sprites, [1807, 1806, 1809, 1808, 1810, 1811]);
    backend.fail_model = true;
    let mut host = ResidentSceneHost {
        resources: RenderResources::new(&runtime),
        backend: &mut backend,
    };
    assert!(matches!(
        scene.draw_scene(frame(), &mut host),
        Err(SceneResourceError::Backend(
            "runtime model interpreter unavailable"
        ))
    ));
}
