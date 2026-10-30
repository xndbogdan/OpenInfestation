use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_scene::{ModelSurfaceTableOrigin, SceneSetup, SceneView},
    menu::MenuBasis,
    model_context::ModelDispatch,
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs,
    },
    terrain_runtime::TerrainSpacing,
    world_furniture_effect::LiteralRetExitRegistrationApproximation,
    world_placement::WorldPlacements,
    Installation, ResidencyRequest,
};
use inf_formats::{
    native_basis::NativeBasisMath, parse_frontend_sine_table, FrontendExecutable,
    FRONTEND_SINE_TABLE_ADDRESS,
};
use serde_json::Value;

struct Fixture {
    resident: ResidentRuntime,
    assets: FrontendAssets,
}

/// The Level-1 world's leaves: 48, then mission 18 + 10.
fn level1() -> Fixture {
    world(28)
}

/// A world's leaves: 48, then its mission + 10.
fn world(leaf: u8) -> Fixture {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    for level in [48, leaf] {
        resident
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    Fixture { resident, assets }
}

fn model_owners(exe: FrontendExecutable<'_>) -> (ModelProcessOwners, ProcessFurnitureQueryState) {
    let owners = ModelProcessOwners::native_startup(exe).unwrap();
    owners.selected_renderer_constructed().unwrap();
    owners
        .graphics
        .publish_view(SceneView {
            eye: [0, 0, -1024],
            basis: MenuBasis::INITIAL,
        })
        .unwrap();
    owners.graphics.publish_shift(0).unwrap();
    owners
        .graphics
        .publish_setup(SceneSetup {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [128, 4096],
            fade_factor: 0x10000,
        })
        .unwrap();
    owners
        .graphics
        .select_projection(
            ModelSurfaceTableOrigin::TemplateDefault,
            ModelDispatch::Near,
        )
        .unwrap();
    let mut process = ProcessFurnitureQueryState::native_startup(exe).unwrap();
    process.initialize_query(LiteralRetExitRegistrationApproximation);
    (owners, process)
}

/// One disposed actor, so without flag 4: the pass walks it and skips it.
fn hidden_actor() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap();
    arena.dispose(actor).unwrap();
    assert_eq!(arena.object(actor).unwrap().base.flags & 4, 0);
    (arena, actor)
}

fn words<const N: usize>(case: &[u32], first: usize) -> [u16; N] {
    std::array::from_fn(|index| case[first + index] as u16)
}

/// Every Infest01 sight test (`sight_infest01.json`): the actor pass's
/// result as captured, then the march. Without an actor hit the pass is
/// replayed too, over an actor it must skip.
#[test]
fn the_sight_test_matches_every_infest01_receipt() {
    let fixture = level1();
    let exe = FrontendExecutable::parse(fixture.assets.executable_bytes()).unwrap();
    let placements = WorldPlacements::new(&fixture.resident, TerrainSpacing::Units128).unwrap();
    let source = WorldTerrainSource::retain(placements.terrain_source());
    let terrain = placements.terrain().clone();
    let math = NativeBasisMath::new(
        parse_frontend_sine_table(exe.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values,
    );
    let furniture = FurnitureDrawState::new(&math);
    let (owners, mut process) = model_owners(exe);
    let animation = crate::frontend_camera::AnimationClock::default();
    let (arena, actor) = hidden_actor();
    let receipts: Value =
        serde_json::from_str(include_str!("../tests/fixtures/sight_infest01.json")).unwrap();
    let cases = receipts["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 707);
    let mut kinds = [0usize; 4];
    for (index, case) in cases.iter().enumerate() {
        let case: Vec<u32> = case
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u32)
            .collect();
        let origin = words::<3>(&case, 0);
        let ray = SightRay::new(origin, words::<3>(&case, 3));
        let mut query = WorldSightQuery {
            residency: &fixture.resident,
            model_fallback: None,
            source: &source,
            terrain: &terrain,
            model_process: &owners,
            process: &mut process,
            furniture: &furniture,
        };
        let pass_length = case[10] as u16;
        let pass = if case[6] == 3 {
            Sight {
                hit: Some(SightHit::Actor {
                    point: words::<3>(&case, 7),
                    actor,
                }),
                length: pass_length,
            }
        } else {
            assert_eq!(case[6], 0, "case {index}");
            let pass = query
                .actor_pass(
                    &ray,
                    SightActors {
                        arena: &arena,
                        animation: &animation,
                        executable: exe,
                    },
                )
                .unwrap();
            assert_eq!(
                pass,
                Sight {
                    hit: None,
                    length: pass_length
                },
                "case {index}"
            );
            pass
        };
        let sight = query
            .march(&ray, pass)
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        let point = words::<3>(&case, 12);
        let expected = match case[11] {
            0 => None,
            1 => Some(SightHit::Terrain { point }),
            2 => Some(SightHit::Furniture {
                point,
                position: words::<3>(&case, 15),
            }),
            3 => Some(SightHit::Actor { point, actor }),
            kind => panic!("case {index}: type {kind}"),
        };
        kinds[case[11] as usize] += 1;
        assert_eq!(
            sight,
            Sight {
                hit: expected,
                length: case[18] as u16,
            },
            "case {index}"
        );
        assert_eq!(u32::from(sight.hit.is_some()), case[19], "case {index}");
    }
    assert_eq!(kinds, [229, 210, 134, 134]);
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
}

/// Infest03's sight tests on world 6 whose furniture march runs a model's
/// group test (`sight_infest03_groups.json`, 139 calls of `004F3B20` in
/// 47 tests, all missing): the actor pass found nothing in each, and the
/// march over the world's coarse terrain and furniture gives the recorded
/// type, point and length.
#[test]
fn sight_tests_through_furniture_groups_match_infest03() {
    let fixture = world(16);
    let exe = FrontendExecutable::parse(fixture.assets.executable_bytes()).unwrap();
    let placements = WorldPlacements::new(&fixture.resident, TerrainSpacing::Units256).unwrap();
    let source = WorldTerrainSource::retain(placements.terrain_source());
    let terrain = placements.terrain().clone();
    let math = NativeBasisMath::new(
        parse_frontend_sine_table(exe.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values,
    );
    let furniture = FurnitureDrawState::new(&math);
    let (owners, mut process) = model_owners(exe);
    let animation = crate::frontend_camera::AnimationClock::default();
    let (arena, _) = hidden_actor();
    let receipts: Value =
        serde_json::from_str(include_str!("../tests/fixtures/sight_infest03_groups.json")).unwrap();
    let cases = receipts["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 47);
    let mut kinds = [0usize; 4];
    for (index, case) in cases.iter().enumerate() {
        let case: Vec<u32> = case
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u32)
            .collect();
        let ray = SightRay::new(words::<3>(&case, 0), words::<3>(&case, 3));
        let mut query = WorldSightQuery {
            residency: &fixture.resident,
            model_fallback: None,
            source: &source,
            terrain: &terrain,
            model_process: &owners,
            process: &mut process,
            furniture: &furniture,
        };
        assert_eq!(case[6], 0, "case {index}");
        let pass = query
            .actor_pass(
                &ray,
                SightActors {
                    arena: &arena,
                    animation: &animation,
                    executable: exe,
                },
            )
            .unwrap();
        assert_eq!(
            pass,
            Sight {
                hit: None,
                length: case[10] as u16
            },
            "case {index}"
        );
        let sight = query
            .march(&ray, pass)
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        let point = words::<3>(&case, 12);
        let expected = match case[11] {
            0 => None,
            1 => Some(SightHit::Terrain { point }),
            2 => Some(SightHit::Furniture {
                point,
                position: words::<3>(&case, 15),
            }),
            kind => panic!("case {index}: type {kind}"),
        };
        kinds[case[11] as usize] += 1;
        assert_eq!(
            sight,
            Sight {
                hit: expected,
                length: case[18] as u16,
            },
            "case {index}"
        );
        assert_eq!(u32::from(sight.hit.is_some()), case[19], "case {index}");
    }
    assert_eq!(kinds, [45, 2, 0, 0]);
}

#[test]
fn a_steep_ray_turns_about_x() {
    let row = |ray: &SightRay, first: usize| -> [f32; 3] {
        std::array::from_fn(|axis| f32::from_bits(ray.basis.words[first + axis]))
    };
    let level = SightRay::new([0, 0, 0], [0, 0, 1000]);
    assert_eq!(level.forward(), [0.0, 0.0, 1.0]);
    assert_eq!(row(&level, 1), [1.0, 0.0, 0.0]);
    assert_eq!(row(&level, 4), [0.0, 1.0, 0.0]);
    // Steeper than 0.75: X = normalize(X axis cross forward) has no X part.
    let steep = SightRay::new([0, 0, 0], [0, 1000, 10]);
    assert_eq!(row(&steep, 1)[0], 0.0);
    let zero = SightRay::new([5, 5, 5], [5, 5, 5]);
    assert_eq!(zero.direction, [1.0, 0.0, 0.0]);
    assert_eq!(zero.squared, 0);
}
