use crate::{
    frontend_assets::FrontendAssets,
    frontend_scene::{ModelSurfaceTableOrigin, SceneSetup, SceneView},
    menu::MenuBasis,
    model_auxiliary::ray::RayRequest,
    model_process::ModelProcessOwners,
    model_scratch::ModelScratchDrawState,
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, ObjectArena, ObjectModelCallbacks,
    },
    world_furniture_effect::{LiteralRetExitRegistrationApproximation, ProcessFurnitureQueryState},
    world_projectile_collision::{ProjectileGeometryError, ProjectileGeometryOwners},
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::native_basis::NativeBasis;

/// 0047D080 receipts from the sight test's actor pass (0043B3EC, Infest01,
/// `.tmp/infestation-ttd/20261009-003627-540-actorray`): the actor's model
/// slot, position WORDs and basis, the ray's basis, origin and length, and
/// the result's +10 for a hit.
struct Receipt {
    slot: u16,
    position: [i16; 3],
    basis: [u32; 10],
    ray: [u32; 10],
    origin: [i16; 3],
    length: i32,
    hit: Option<i32>,
}

const IDENTITY: [u32; 10] = [3, 0x3f800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0x3f800000];
const TURNED: [u32; 10] = [3, 0xbf800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0xbf800000];
const UNTOUCHED: [u32; 10] = [0, 0x3f800000, 0, 0, 0, 0x3f800000, 0, 0, 0, 0x3f800000];
const FACTORY: [i16; 3] = [-28451, 80, -31173];
const OTHER: [i16; 3] = [-27654, 80, -31230];
const RAYS: [[u32; 10]; 7] = [
    [
        0, 0x3d656cb6, 0, 0xbf7f991d, 0xbd7ae140, 0x3f7f8491, 0xbb613089, 0x3f7f1d7d, 0x3d7b463d,
        0x3d656ca7,
    ],
    [
        0, 0x3df692b5, 0, 0xbf7e2348, 0xbcc8af3c, 0x3f7fec0c, 0xbb42b5fc, 0x3f7e0f47, 0x3cca27af,
        0x3df68c12,
    ],
    [
        0, 0x3e5ac5fe, 0, 0xbf7a16da, 0x39aac483, 0x3f800000, 0x38956263, 0x3f7a15d5, 0xb9aecdbb,
        0x3e5ad891,
    ],
    [
        0, 0x3e6ec1a5, 0, 0xbf78f1b7, 0xbc34a3c1, 0x3f7ffbca, 0xbb2d3f47, 0x3f78ee71, 0x3c39c270,
        0x3e6eaff4,
    ],
    [
        0, 0x3e6ed028, 0, 0xbf78f0d8, 0xbcb4a933, 0x3f7fef25, 0xbbad4fa4, 0x3f78e0f5, 0x3cb9c8b0,
        0x3e6eb7fc,
    ],
    [
        0, 0x3e6f0cb0, 0, 0xbf78ed36, 0xbcbc99fa, 0x3f7feda0, 0xbbb51e38, 0x3f78da86, 0x3cc1f5f0,
        0x3e6f093b,
    ],
    [
        0, 0x3e6ec1a5, 0, 0xbf78f1b7, 0xbc64727f, 0x3f7ff944, 0xbb5b1929, 0x3f78ebfd, 0x3c6aec0f,
        0x3e6ead9a,
    ],
];
const LAST_RAY: [u32; 10] = [
    0, 0x3e6ec1a5, 0, 0xbf78f1b7, 0xbb69c84e, 0x3f7fff90, 0xba603713, 0x3f78f21c, 0x3b706894,
    0x3e6eb379,
];

const fn miss(
    slot: u16,
    position: [i16; 3],
    basis: [u32; 10],
    ray: [u32; 10],
    origin: [i16; 3],
    length: i32,
) -> Receipt {
    Receipt {
        slot,
        position,
        basis,
        ray,
        origin,
        length,
        hit: None,
    }
}

const RECEIPTS: [Receipt; 20] = [
    miss(157, FACTORY, IDENTITY, RAYS[0], [-29866, 129, -32026], 2271),
    miss(157, FACTORY, IDENTITY, RAYS[1], [-29790, 133, -32022], 2246),
    miss(157, FACTORY, IDENTITY, RAYS[2], [-29716, 136, -32014], 2232),
    miss(552, OTHER, TURNED, RAYS[2], [-29716, 136, -32014], 2555),
    miss(157, FACTORY, IDENTITY, RAYS[3], [-29646, 141, -31999], 2170),
    miss(552, OTHER, TURNED, RAYS[3], [-29646, 141, -31999], 2489),
    miss(157, FACTORY, IDENTITY, RAYS[4], [-29570, 144, -31981], 2091),
    miss(552, OTHER, TURNED, RAYS[4], [-29570, 144, -31981], 2410),
    miss(157, FACTORY, IDENTITY, RAYS[5], [-29488, 144, -31962], 2007),
    miss(552, OTHER, TURNED, RAYS[5], [-29488, 144, -31962], 2326),
    miss(157, FACTORY, IDENTITY, RAYS[6], [-29407, 144, -31943], 1925),
    miss(552, OTHER, TURNED, RAYS[6], [-29407, 144, -31943], 2243),
    miss(
        157,
        FACTORY,
        IDENTITY,
        LAST_RAY,
        [-29320, 144, -31923],
        1836,
    ),
    miss(552, OTHER, TURNED, LAST_RAY, [-29320, 144, -31923], 2155),
    Receipt {
        slot: 157,
        position: [-18313, 16, 26207],
        basis: TURNED,
        ray: [
            0, 0xbf56e208, 0, 0x3f0b251e, 0xbcea9053, 0x3f7fa501, 0xbd351e9a, 0xbf0af503,
            0xbd57c6ae, 0xbf5694c5,
        ],
        origin: [-17626, 72, 27937],
        length: 2642,
        hit: Some(1063),
    },
    Receipt {
        slot: 157,
        position: [-18313, 16, 26207],
        basis: TURNED,
        ray: [
            0, 0xbf638a57, 0, 0x3eea9e25, 0xbcf199c8, 0x3f7f7821, 0xbd6a500d, 0xbeea14e9,
            0xbd83cf47, 0xbf6314d9,
        ],
        origin: [-17694, 67, 27832],
        length: 2544,
        hit: Some(1208),
    },
    Receipt {
        slot: 157,
        position: [-18313, 16, 26207],
        basis: TURNED,
        ray: [
            0, 0xbf6ba14b, 0, 0x3ec8250d, 0xbc9f18d8, 0x3f7faf12, 0xbd3b4e00, 0xbec7e970,
            0xbd4b7f39, 0xbf6b5607,
        ],
        origin: [-17751, 65, 27721],
        length: 2430,
        hit: Some(1250),
    },
    Receipt {
        slot: 223,
        position: [-18312, 0, 27886],
        basis: UNTOUCHED,
        ray: [
            0, 0x3dac05ab, 0, 0x3f7f1868, 0x3bef8de2, 0x3f7ffe3c, 0xba218ac6, 0xbf7f1668,
            0x3bf0675e, 0x3dac1b75,
        ],
        origin: [-17863, 63, 27848],
        length: 562,
        hit: Some(159),
    },
    Receipt {
        slot: 223,
        position: [-18312, 0, 27886],
        basis: UNTOUCHED,
        ray: [
            0, 0x3dac05ab, 0, 0x3f7f1868, 0x3c081bfa, 0x3f7ffdb9, 0xba3791b1, 0xbf7f15e5,
            0x3c08978c, 0x3dac1b1c,
        ],
        origin: [-17871, 63, 27848],
        length: 554,
        hit: Some(158),
    },
    Receipt {
        slot: 223,
        position: [-18312, 0, 27886],
        basis: UNTOUCHED,
        ray: [
            0, 0x3dac05ab, 0, 0x3f7f1868, 0x3c12ff48, 0x3f7ffd58, 0xba4640e5, 0xbf7f1584,
            0x3c1384bc, 0x3dac1adb,
        ],
        origin: [-17879, 63, 27848],
        length: 546,
        hit: Some(158),
    },
];

fn model_owners(
    executable: inf_formats::FrontendExecutable<'_>,
) -> (ModelProcessOwners, ProcessFurnitureQueryState) {
    let owners = ModelProcessOwners::native_startup(executable).unwrap();
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
            crate::model_context::ModelDispatch::Near,
        )
        .unwrap();
    let mut process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
    process.initialize_query(LiteralRetExitRegistrationApproximation);
    (owners, process)
}

#[test]
fn the_actor_ray_matches_the_sight_tests_receipts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let animation = crate::frontend_camera::AnimationClock::default();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    // The Level-1 world's leaves: 48, then mission 18 + 10.
    for level in [48, 28] {
        residency
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
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
    let (owners, mut process) = model_owners(executable);
    for (index, receipt) in RECEIPTS.iter().enumerate() {
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(receipt.position.map(|word| word as u16));
        let pose = &mut arena.actor_mut(actor).unwrap().rotating;
        pose.visual.model_word = receipt.slot;
        pose.basis.words = receipt.basis;
        let actual = ProjectileGeometryOwners {
            residency: &residency,
            model_fallback: None,
            model_process: &owners,
            furniture_query: &mut process,
        }
        .actor_ray(
            ObjectModelCallbacks {
                arena: &arena,
                source: actor,
                animation: &animation,
                executable,
            },
            receipt.origin.map(|word| word as u16),
            RayRequest {
                basis: NativeBasis { words: receipt.ray },
                length: receipt.length,
            },
        )
        .unwrap_or_else(|error| panic!("receipt {index}: {error}"));
        assert_eq!(actual, receipt.hit, "receipt {index}");
        assert_eq!(
            owners.scratch.state().unwrap(),
            ModelScratchDrawState::Ready,
            "receipt {index}"
        );
    }
}

#[test]
fn a_missing_actor_model_fails_closed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let animation = crate::frontend_camera::AnimationClock::default();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
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
    arena.actor_mut(actor).unwrap().rotating.visual.model_word = u16::MAX;
    let (owners, mut process) = model_owners(executable);
    let result = ProjectileGeometryOwners {
        residency: &residency,
        model_fallback: None,
        model_process: &owners,
        furniture_query: &mut process,
    }
    .actor_ray(
        ObjectModelCallbacks {
            arena: &arena,
            source: actor,
            animation: &animation,
            executable,
        },
        [0; 3],
        RayRequest {
            basis: NativeBasis::INITIAL,
            length: 100,
        },
    );
    assert!(
        matches!(
            result,
            Err(ProjectileGeometryError::MissingModel { slot: u16::MAX, .. })
        ),
        "{result:?}"
    );
}

/// An actor on the aim's ray whose model is not resident (world 1's ships)
/// stops only for the sight test: the actor pass passes it and the aim goes
/// on. Its stop used to end the player's vehicle update (a crash log of
/// 2026-10-23 on the prototype map).
#[test]
fn the_sight_test_passes_an_actor_without_a_model() {
    use crate::{
        terrain_furniture::draw::FurnitureDrawState,
        terrain_runtime::TerrainSpacing,
        world_actor_terrain::WorldTerrainSource,
        world_placement::WorldPlacements,
        world_sight::{SightActors, SightRay, WorldSightQuery},
    };
    use inf_formats::{
        native_basis::NativeBasisMath, parse_frontend_sine_table, FRONTEND_SINE_TABLE_ADDRESS,
    };

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let animation = crate::frontend_camera::AnimationClock::default();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    for level in [48, 28] {
        residency
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    let placements = WorldPlacements::new(&residency, TerrainSpacing::Units128).unwrap();
    let source = WorldTerrainSource::retain(placements.terrain_source());
    let math = NativeBasisMath::new(
        parse_frontend_sine_table(executable.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values,
    );
    let furniture = FurnitureDrawState::new(&math);
    let (owners, mut process) = model_owners(executable);
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
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0, 0, 500]);
    let pose = &mut arena.actor_mut(actor).unwrap().rotating;
    pose.visual.radius_word = 100;
    pose.visual.model_word = u16::MAX;
    let sight = WorldSightQuery {
        residency: &residency,
        model_fallback: None,
        source: &source,
        terrain: placements.terrain(),
        model_process: &owners,
        process: &mut process,
        furniture: &furniture,
    }
    .actor_pass(
        &SightRay::new([0; 3], [0, 0, 1000]),
        SightActors {
            arena: &arena,
            animation: &animation,
            executable,
        },
    )
    .unwrap();
    assert_eq!(sight.hit, None);
}

/// The sight test's actor pass (0043B2E0) over Infest01's actor lists
/// (`sight_actor_pass_infest01.json`): every flag-4 actor in list order with
/// its position, radius, model and basis, and the pass's result. The ray is
/// the native one; `SightRay::new` must rebuild it from the sight's ends.
#[test]
fn the_sight_tests_actor_pass_matches_its_receipts() {
    use crate::{
        terrain_furniture::draw::FurnitureDrawState,
        terrain_runtime::TerrainSpacing,
        world_actor_terrain::WorldTerrainSource,
        world_placement::WorldPlacements,
        world_sight::{Sight, SightActors, SightHit, SightRay, WorldSightQuery},
    };
    use inf_formats::{
        native_basis::NativeBasisMath, parse_frontend_sine_table, FRONTEND_SINE_TABLE_ADDRESS,
    };
    use serde_json::Value;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let animation = crate::frontend_camera::AnimationClock::default();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    for level in [48, 28] {
        residency
            .apply(ResidencyRequest::Activate { level })
            .unwrap();
    }
    let placements = WorldPlacements::new(&residency, TerrainSpacing::Units128).unwrap();
    let source = WorldTerrainSource::retain(placements.terrain_source());
    let math = NativeBasisMath::new(
        parse_frontend_sine_table(executable.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values,
    );
    let furniture = FurnitureDrawState::new(&math);
    let (owners, mut process) = model_owners(executable);
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/sight_actor_pass_infest01.json"
    ))
    .unwrap();
    let passes = fixture["passes"].as_array().unwrap();
    assert_eq!(passes.len(), 12);
    let words = |value: &Value| -> Vec<u32> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|word| word.as_u64().unwrap() as u32)
            .collect()
    };
    let mut hits = 0;
    for entry in passes {
        let pass = &entry["pass"];
        let index = pass["sight"].as_u64().unwrap();
        let origin: [u16; 3] = std::array::from_fn(|axis| words(&pass["origin"])[axis] as u16);
        let target: [u16; 3] = std::array::from_fn(|axis| words(&pass["target"])[axis] as u16);
        let basis = NativeBasis {
            words: words(&pass["basis"]).try_into().unwrap(),
        };
        let squared = pass["squared"].as_u64().unwrap() as i32;
        let ray = SightRay::new(origin, target);
        assert_eq!((ray.basis, ray.squared), (basis, squared), "sight {index}");

        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 64,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let mut ids = Vec::new();
        for actor in entry["actors"].as_array().unwrap() {
            let actor = words(actor);
            let id = arena
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
            assert_ne!(arena.object(id).unwrap().base.flags & 4, 0);
            arena.owned_mut(id).unwrap().state.base.position_words =
                Some(std::array::from_fn(|axis| actor[axis] as u16));
            let pose = &mut arena.actor_mut(id).unwrap().rotating;
            pose.visual.radius_word = actor[3] as u16;
            pose.visual.model_word = actor[4] as u16;
            pose.basis.words = actor[5..15].try_into().unwrap();
            ids.push(id);
        }
        let expected = match pass["hit"].as_u64() {
            Some(hit) => {
                hits += 1;
                Some(SightHit::Actor {
                    point: std::array::from_fn(|axis| words(&pass["point"])[axis] as u16),
                    actor: ids[hit as usize],
                })
            }
            None => None,
        };
        let actual = WorldSightQuery {
            residency: &residency,
            model_fallback: None,
            source: &source,
            terrain: placements.terrain(),
            model_process: &owners,
            process: &mut process,
            furniture: &furniture,
        }
        .actor_pass(
            &ray,
            SightActors {
                arena: &arena,
                animation: &animation,
                executable,
            },
        )
        .unwrap_or_else(|error| panic!("sight {index}: {error}"));
        assert_eq!(
            actual,
            Sight {
                hit: expected,
                length: pass["length"].as_u64().unwrap() as u16,
            },
            "sight {index}"
        );
    }
    assert_eq!(hits, 5);
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
}
