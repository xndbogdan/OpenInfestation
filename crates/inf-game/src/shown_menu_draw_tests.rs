use super::*;

/// Infest01's panel around the shop's viewport at 1024 by 768 (the
/// `panelquads` receipt): (350, -225)..(1180, 615), centred on (765, 195).
#[test]
fn the_shop_panel_matches_infest01() {
    let quads = shown_menu_panel([470, 0, 980, 700], [1024, 768]).unwrap();
    let corners = quads.map(|quad| quad.corners);
    assert_eq!(
        corners,
        [
            [[350, 195], [350, -225], [765, -225], [765, 195]],
            [[765, -225], [1180, -225], [1180, 195], [765, 195]],
            [[1180, 195], [1180, 615], [765, 615], [765, 195]],
            [[765, 615], [350, 615], [350, 195], [765, 195]],
        ]
    );
    for quad in quads {
        assert_eq!(quad.sprite_slot, 0x2a6);
        assert_eq!(quad.vertex_depths, [0x4000; 4]);
        assert_eq!(quad.descriptor_word, 2);
        assert_eq!(quad.mode, DirectSpriteMode::Textured);
    }
}

/// The scale truncates toward zero, as `004141B0`'s shifts do.
#[test]
fn the_panel_scales_toward_zero() {
    let quads = shown_menu_panel([470, 0, 980, 700], [640, 480]).unwrap();
    // x: (350 * 640) / 1024 = 218, (1180 * 640) / 1024 = 737;
    // y: (-300 * 480) / 1024 = -140, (820 * 480) / 1024 = 384.
    assert_eq!(
        quads[0].corners,
        [[218, 122], [218, -140], [477, -140], [477, 122]]
    );
}

#[test]
fn a_full_width_viewport_has_no_panel() {
    assert!(shown_menu_panel([0, 0, 0x400, 0x400], [1024, 768]).is_none());
}

/// One database row drawn over `current`'s resources (and `models`'
/// second residency) at 1024x768, every entry known.
struct DatabaseDraw {
    failures: Vec<(i16, String)>,
    clip_restored: bool,
    stopped: bool,
    models: usize,
    prefix_skips: usize,
    /// The frame's RGBA after the row.
    pixels: Vec<u8>,
}

struct DatabaseFixture {
    assets: crate::frontend_assets::FrontendAssets,
    tables: crate::model_primitives::PrimitiveTables,
    menus: crate::object_runtime::PlayerMessageMenus,
    installation: crate::Installation,
}

impl DatabaseFixture {
    fn new() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root)
            .expect("retail Infestation data required");
        let tables = crate::model_primitives::PrimitiveTables::from_executable(
            inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        )
        .unwrap();
        let menus = crate::object_runtime::PlayerMessageMenus::parse(&assets).unwrap();
        let installation = crate::Installation::open(&root).unwrap();
        Self {
            assets,
            tables,
            menus,
            installation,
        }
    }

    fn resident(&self, level: u8) -> crate::ResidentRuntime {
        use crate::{ResidencyRequest, ResourceSelection};
        let mut runtime = crate::ResidentRuntime::new(self.installation.clone()).unwrap();
        runtime
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: 4,
                    language: 0,
                },
            })
            .unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        runtime
    }

    /// The first leaf that loads `model`.
    fn leaf(&self, model: u16) -> u8 {
        *crate::ResidentRuntime::new(self.installation.clone())
            .unwrap()
            .leaves_registering(RenderResourceKind::Model.handler(), model)
            .first()
            .unwrap()
    }

    fn draw(
        &self,
        current: &crate::ResidentRuntime,
        models: Option<RenderResources<'_>>,
        entry: u32,
    ) -> DatabaseDraw {
        use crate::{
            frontend_display::frontend_startup,
            menu::{MenuColor, MenuData, MenuDrawPlacement, MenuSource},
            software_frame::SoftwareFrame,
            software_menu::{SoftwareMenuRequest, SoftwareMenuState},
            software_scene::{SoftwareScene, SoftwareSceneRequest},
        };
        let resources = RenderResources::new(current);
        let startup = frontend_startup(
            current,
            crate::presentation::NativeFrame::original([1024, 768]),
        )
        .unwrap();
        let process = FrontendProcessState::default();
        process.publish_colors(MenuDrawColors {
            selected_start: resources.style(3).unwrap(),
            ordinary: resources.style(2).unwrap(),
            selected_end: resources.style(2).unwrap(),
        });
        let math = MenuBasisMath::new(self.assets.sine_table().values);
        let mut frame = SoftwareFrame::new(1024, 768, [17, 23, 31, 255]);
        let mut menu_state = SoftwareMenuState::default();
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                startup.scene.environment,
            ),
            resources,
            tables: &self.tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        scene.begin_frame().unwrap();
        let mut software = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut scene,
            resources,
            focal: startup.projection.focal,
            process: &process,
            state: &mut menu_state,
        })
        .unwrap();
        let clip = [10, 20, 500, 400];
        software.set_clip_rect(clip).unwrap();
        let (mut rng, mut commands, mut omissions, mut failures) =
            (1, Vec::new(), ShownMenuOmissions::new(), Vec::new());
        let mut host = ShownMenuDrawHost {
            menu: &mut software,
            surface: [1024, 768],
            // 004140A0(0)'s viewport.
            viewport: [0x1d6, 0, 0x3d4, 700],
            context: ShownMenuDrawContext {
                shop: None,
                settings: None,
                device_name: None,
                computer: Some(ComputerDraw {
                    database: &self.menus.database,
                    hint_texts: &self.menus.hint_texts,
                    grants: &[u32::MAX; 8],
                    rng: &mut rng,
                    models,
                    model_failures: &mut failures,
                    skip_world: 2,
                }),
                process: &process,
                focal: startup.projection.focal,
                math: &math,
                commands: &mut commands,
            },
            omissions: &mut omissions,
        };
        host.begin_controller(MenuControllerDrawSetup {
            surface_pixels: [1024, 768],
            context_4c: 0,
            context_64: 2048,
            context_68: 4096,
            light_direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            light_color: [128; 3],
            ambient_color: [32; 3],
        })
        .unwrap();
        let mut request = MenuRowDrawRequest {
            menu_source: MenuSource::AuthoredDefinition {
                address: 0x0052_fa60,
                span: inf_formats::ByteSpan::new(0, 20),
            },
            layout_index: 0,
            item_index: 1,
            callback_address: DATABASE_ROW,
            arguments: [entry, 0],
            context_origin_pixels: [600, 100],
            placement: MenuDrawPlacement::Linear,
            descriptor: MenuRowDescriptor {
                position_pixels: [30, 40],
                extent_pixels: [300, 40],
                reveal: 32767,
                flags: 0,
                age_millis: 1000,
                color: MenuColor::from_rgb([128; 3]),
            },
        };
        let data = MenuData::new(self.assets.menu_definition(0x0052_fa60).unwrap());
        let mut state = MenuState::new(data, MenuBasis::INITIAL);
        host.draw_row(&mut request, &mut state).unwrap();
        let clip_restored = software.clip_rect().unwrap() == clip;
        let drawn = software.scene_mut();
        let (stopped, models, prefix_skips) = (
            drawn.stopped(),
            drawn.stats.models,
            drawn.stats.model_value_prefix_skips,
        );
        drop(software);
        drop(scene);
        DatabaseDraw {
            failures,
            clip_restored,
            stopped,
            models,
            prefix_skips,
            pixels: frame.rgba().to_vec(),
        }
    }
}

impl DatabaseFixture {
    /// The shop's turning preview (`0042FD70`) of `model` in form
    /// `selector_one` over `current`'s resources at 1024x768: whether the
    /// scene stopped, and its drawn models and prefix skips.
    fn shop_preview(
        &self,
        current: &crate::ResidentRuntime,
        model: i16,
        selector_one: u16,
    ) -> (Result<(), ShownMenuDrawError>, bool, usize, usize) {
        use crate::{
            frontend_display::frontend_startup,
            software_frame::SoftwareFrame,
            software_menu::{SoftwareMenuRequest, SoftwareMenuState},
            software_scene::{SoftwareScene, SoftwareSceneRequest},
        };
        let resources = RenderResources::new(current);
        let startup = frontend_startup(
            current,
            crate::presentation::NativeFrame::original([1024, 768]),
        )
        .unwrap();
        let process = FrontendProcessState::default();
        process.publish_colors(MenuDrawColors {
            selected_start: resources.style(3).unwrap(),
            ordinary: resources.style(2).unwrap(),
            selected_end: resources.style(2).unwrap(),
        });
        let math = MenuBasisMath::new(self.assets.sine_table().values);
        let mut frame = SoftwareFrame::new(1024, 768, [17, 23, 31, 255]);
        let mut menu_state = SoftwareMenuState::default();
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                startup.scene.environment,
            ),
            resources,
            tables: &self.tables,
            frame: &mut frame,
            config: startup.scene,
        })
        .unwrap();
        scene.begin_frame().unwrap();
        let mut software = SoftwareMenu::new(SoftwareMenuRequest {
            scene: &mut scene,
            resources,
            focal: startup.projection.focal,
            process: &process,
            state: &mut menu_state,
        })
        .unwrap();
        let mut commands = Vec::new();
        let mut host = ShopRowHost {
            menu: &mut software,
            surface: [1024, 768],
            process: &process,
            focal: startup.projection.focal,
            math: &math,
            commands: &mut commands,
        };
        let distance = 3000;
        let result = host.preview(ShopPreview {
            model_slot: model,
            center: [700, 120],
            fade: [distance + 2000, distance + 2500],
            eye_y: distance * 2 / 5,
            position: [0, 0x14, distance],
            angle: 0x2000,
            selector_one,
        });
        let drawn = software.scene_mut();
        (
            result,
            drawn.stopped(),
            drawn.stats.models,
            drawn.stats.model_value_prefix_skips,
        )
    }
}

/// Skimmer mode's preview in Artoria's shop (a player's report): the ACAM,
/// model 117, in form 5, whose child 127 reads register 15 unwritten. The
/// shop's own seed (`ShopPreviewModel127StackHistoryApproximation`) draws
/// the whole model; before it, the read failed the whole menu.
#[test]
fn the_shop_previews_the_skimmer_whole() {
    let fixture = DatabaseFixture::new();
    let (result, stopped, models, prefix_skips) =
        fixture.shop_preview(&fixture.resident(21), 117, 5);
    result.unwrap();
    assert!(!stopped);
    assert_eq!((models, prefix_skips), (1, 0));
}

/// Every listed entry whose model the frontend lacks, drawn from another
/// world through the second residency (the port's database model fix), lands
/// on the very pixels of its own world's draw: the fix lends the resources
/// and keeps retail's placement (`00414C80`). The lab's `database.py`
/// checks the own world's draw against retail's on every world.
#[test]
fn every_database_model_draws_from_another_world_as_from_its_own() {
    let fixture = DatabaseFixture::new();
    let handler = RenderResourceKind::Model.handler();
    let frontend = fixture.resident(47);
    let mut homes = std::collections::BTreeMap::new();
    let (mut compared, mut drew) = (0, 0);
    for (index, entry) in fixture.menus.database.iter().enumerate().skip(1) {
        if entry.model == 0
            || entry.flags & 2 != 0
            || frontend.resource(handler, entry.model).is_some()
        {
            continue;
        }
        let leaf = fixture.leaf(entry.model);
        let home = &*homes.entry(leaf).or_insert_with(|| fixture.resident(leaf));
        let own = fixture.draw(home, None, index as u32);
        let lent = fixture.draw(&frontend, Some(RenderResources::new(home)), index as u32);
        assert_eq!(own.failures, lent.failures, "entry {index:#x}");
        assert!(
            own.pixels == lent.pixels,
            "entry {index:#x}: model {:#x} from overlay {leaf} drew elsewhere",
            entry.model
        );
        compared += 1;
        drew += usize::from(own.models > 0);
    }
    assert!(
        compared > 50 && drew > 50,
        "{compared} compared, {drew} drew a model"
    );
}

/// The gate's entry (0x0B): model 62 jitters vertices (`0022`) through the
/// process RNG, which this fixture's scene does not lend, so its draw
/// stops. The row still returns, its clip restored, with the failure for
/// the log, and the scene draws on: the current world's model and another
/// world's alike.
#[test]
fn a_database_model_that_stops_leaves_the_menu_drawing() {
    const GATE: u32 = 0x0b;
    let fixture = DatabaseFixture::new();
    let model = fixture.menus.database[GATE as usize].model;
    assert_eq!(model, 62);
    let handler = RenderResourceKind::Model.handler();
    let gate = fixture.resident(fixture.leaf(model));
    let frontend = fixture.resident(47);
    assert!(gate.resource(handler, model).is_some());
    assert!(frontend.resource(handler, model).is_none());
    for draw in [
        fixture.draw(&gate, None, GATE),
        fixture.draw(&frontend, Some(RenderResources::new(&gate)), GATE),
    ] {
        assert_eq!(draw.failures.len(), 1, "{:?}", draw.failures);
        assert_eq!(draw.failures[0].0, 62);
        assert!(
            draw.failures[0]
                .1
                .contains("UnsupportedOpcode { opcode: 34 }"),
            "{:?}",
            draw.failures
        );
        assert!(draw.clip_restored);
        assert!(!draw.stopped);
    }
}

/// The Generator's entry (0x4E): the thumper, model 46, reads register 1
/// before it writes it, so it takes the object draw's seed
/// (`ThumperModel46StackHistoryApproximation`) and the whole model draws.
#[test]
fn the_generator_entry_takes_the_object_draws_register_seed() {
    const GENERATOR: u32 = 0x4e;
    let fixture = DatabaseFixture::new();
    let model = fixture.menus.database[GENERATOR as usize].model;
    assert_eq!(model, 46);
    let draw = fixture.draw(&fixture.resident(fixture.leaf(model)), None, GENERATOR);
    assert!(draw.failures.is_empty(), "{:?}", draw.failures);
    assert_eq!((draw.models, draw.prefix_skips), (1, 0));
    assert!(draw.clip_restored);
    assert!(!draw.stopped);
}

/// The Skimmer's entry (0xA6) shows the ACAM, model 117, in form 5: its
/// child 127 reads register 15 as the object draw does, so it takes the
/// object draw's seed (`SkimmerModel127StackHistoryApproximation`) and the
/// whole model draws.
#[test]
fn the_skimmer_entry_takes_the_object_draws_register_seed() {
    const SKIMMER: u32 = 0xa6;
    let fixture = DatabaseFixture::new();
    assert_eq!(fixture.menus.database[SKIMMER as usize].model, 117);
    let draw = fixture.draw(&fixture.resident(48), None, SKIMMER);
    assert!(draw.failures.is_empty(), "{:?}", draw.failures);
    assert_eq!((draw.models, draw.prefix_skips), (1, 0));
    assert!(draw.clip_restored);
    assert!(!draw.stopped);
}

/// The Demons' entry (0x5F): model 638 reads register 10 at its root before
/// any write, so under the same policy the reached prefix stays and the
/// row records no failure.
#[test]
fn the_demons_entry_keeps_its_prefix_at_an_unseeded_register() {
    const DEMONS: u32 = 0x5f;
    let fixture = DatabaseFixture::new();
    let model = fixture.menus.database[DEMONS as usize].model;
    assert_eq!(model, 638);
    let draw = fixture.draw(&fixture.resident(fixture.leaf(model)), None, DEMONS);
    assert!(draw.failures.is_empty(), "{:?}", draw.failures);
    assert_eq!((draw.models, draw.prefix_skips), (0, 1));
    assert!(draw.clip_restored);
    assert!(!draw.stopped);
}
