use super::*;
use serde_json::{json, Value};

fn words<const N: usize>(value: &Value) -> [u16; N] {
    std::array::from_fn(|i| value[i].as_i64().unwrap() as u16)
}

#[test]
fn projectile_furniture_scan_matches_native_cell_order_flag_carry_and_stopped_prefixes() {
    let receipts: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/projectile_furniture_scan_native.json"
    ))
    .unwrap();
    for case in receipts["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut events = Vec::new();
        let mut flag = 0;
        let result = walk_cells(
            words(&input["center"]),
            input["radius"].as_i64().unwrap() as i32,
            input["spacing"].as_u64().unwrap() as u16,
            |xz| {
                events.push(json!({"xz":xz,"radius":input["radius"].as_i64().unwrap() as u32,"clock":input["clock"]}));
                if input["stop_query"].as_u64() == Some(events.len() as u64) {
                    return Err(ProjectileFurnitureError::MissingModel { slot: 123 });
                }
                let answers = input["answers"].as_array().unwrap();
                if let Some(answer) = answers[(events.len() - 1).min(answers.len() - 1)].as_u64() {
                    flag = answer;
                }
                Ok(flag == 1)
            },
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "{input}"
        );
        assert_eq!(events, *case["events"].as_array().unwrap(), "{input}");
        assert_eq!(flag, case["flag"].as_u64().unwrap(), "{input}");
    }
}

#[test]
fn projectile_furniture_signed_broadphase_matches_native_cell_admission() {
    let receipts: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/projectile_furniture_cell_native.json"
    ))
    .unwrap();
    for case in receipts["cases"].as_array().unwrap() {
        let input = &case["input"];
        if input["lookup"] == 0 || input["model_radius"] == 0 {
            continue;
        }
        let sample: [u16; 2] = words(&input["sample"]);
        let center: [u16; 3] = words(&input["center"]);
        let radius = input["radius"].as_i64().unwrap() as i16;
        let mask = input["spacing"].as_u64().unwrap() as u16 - 1;
        let height = (input["height"].as_i64().unwrap() as i8 as i32)
            .wrapping_shl(input["height_shift"].as_u64().unwrap() as u32)
            as u16;
        let origin = [sample[0] & !mask, height, sample[1] & !mask];
        let relative = std::array::from_fn(|i| i32::from(center[i].wrapping_sub(origin[i]) as i16));
        let geometry = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "geometry");
        assert_eq!(
            model_sphere_admits(
                relative,
                radius,
                input["model_radius"].as_u64().unwrap() as u16
            ),
            geometry.is_some(),
            "{input}"
        );
        if let Some(geometry) = geometry {
            assert_eq!(
                json!(relative.map(|v| v as u32)),
                geometry["relative"],
                "{input}"
            );
            assert_eq!(json!(origin), geometry["origin"], "{input}");
            assert_eq!(
                geometry["table"],
                if radius == 0 {
                    0x544bf0_u32
                } else {
                    0x544c08_u32
                },
                "{input}"
            );
        }
    }
}

#[test]
fn projectile_furniture_scan_detects_true_word_cycle_without_claiming_a_miss() {
    let mut queries = 0;
    assert!(matches!(
        walk_cells([0; 3], 32767, 256, |_| {
            queries += 1;
            Ok(false)
        }),
        Err(ProjectileFurnitureError::PortableNonProgressingFurnitureScan { radius: 32767 })
    ));
    assert_eq!(queries, 256);
}

#[test]
fn original_furniture_model_query_consumes_retained_sources_and_returns_stable_cell_identity() {
    use crate::{
        frontend_assets::FrontendAssets,
        frontend_scene::{ModelSurfaceTableOrigin, SceneSetup, SceneView},
        menu::MenuBasis,
        model_context::ModelDispatch,
        terrain_runtime::{TerrainSpacing, TerrainSparsePayload},
        world_placement::WorldPlacements,
        Installation, ResidencyRequest,
    };
    use inf_formats::{
        native_basis::NativeBasisMath, parse_frontend_sine_table, FrontendExecutable,
        TerrainResourceLookup, FRONTEND_SINE_TABLE_ADDRESS,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    let placements = WorldPlacements::new(&resident, TerrainSpacing::Units128).unwrap();
    let source = WorldTerrainSource::retain(placements.terrain_source());
    let mut terrain = placements.terrain().clone();
    let lookup = resident_placement_table(&resident)
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.resource_word == 51)
        .expect("original H11 has shared detail-sphere model51") as u8;
    let (id, record) = terrain
        .buckets()
        .iter()
        .enumerate()
        .find_map(|(bucket, records)| {
            records.records.first().map(|record| {
                (
                    crate::terrain_runtime::TerrainSparseRecordId {
                        bucket: bucket as u16,
                        index: 0,
                    },
                    *record,
                )
            })
        })
        .expect("original H12 furniture fixture");
    terrain
        .write_sparse_payload(
            id,
            TerrainSparsePayload {
                state: 0,
                resource_lookup: TerrainResourceLookup::from_raw(lookup),
            },
        )
        .unwrap();
    let xz = [record.x.wrapping_mul(128), id.bucket.wrapping_mul(128)];
    let y = i32::from(terrain.cell_at(xz).height_i8())
        .wrapping_shl(terrain.sampling().height_scale.shift()) as u16;
    let math = NativeBasisMath::new(
        parse_frontend_sine_table(exe.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values,
    );
    let furniture = FurnitureDrawState::new(&math);
    let mut process = ProcessFurnitureQueryState::native_startup(exe).unwrap();
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
            fade_factor: 65536,
        })
        .unwrap();
    owners
        .graphics
        .select_projection(
            ModelSurfaceTableOrigin::TemplateDefault,
            ModelDispatch::Near,
        )
        .unwrap();
    let mut query = WorldProjectileFurnitureQuery {
        residency: &resident,
        source: &source,
        terrain: &terrain,
        model_process: &owners,
        process: &mut process,
        furniture: &furniture,
    };
    let mut output = CollisionContact::unwritten();
    let mut hit = None;
    query
        .probe_cell(
            [xz[0], y.wrapping_sub(7), xz[1]],
            xz,
            4,
            123,
            &mut output,
            &mut hit,
        )
        .unwrap();
    assert_eq!(output.flag(), Some(1));
    let hit = hit.expect("actual model51 detail sphere contact");
    assert_eq!(hit.contact.xz, xz);
    assert_eq!(hit.record, source.query_record(id));
    assert_eq!(process.probe_basis_address(), 0x546b88);
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
}
