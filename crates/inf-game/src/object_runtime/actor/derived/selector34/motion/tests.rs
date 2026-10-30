use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};
use inf_formats::native_basis::NativeBasis;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn number(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector34_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
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
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    (arena, actor)
}
#[test]
fn selector34_motion_native_force_phase_preserves_signed_float_and_override_order() {
    let f: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector34_motion_native.json"
    )))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let bytes = assets.executable_bytes();
    let exe = FrontendExecutable::parse(bytes).unwrap();
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        exe.data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        f["retail_sha256"].as_str().unwrap()
    );
    for guard in f["guards"].as_array().unwrap() {
        let at = number(&guard["address"]) as usize - 0x400000;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[at..at + number(&guard["length"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
    assert_eq!(f["cases"].as_array().unwrap().len(), 17);
    for case in f["cases"].as_array().unwrap() {
        let i = &case["input"];
        let (mut arena, actor) = fixture();
        arena.owned_mut(actor).unwrap().state.base.word_28 = number(&i["size"]) as u16;
        let state = arena.actor_mut(actor).unwrap();
        state.words_8e_92 = std::array::from_fn(|a| number(&i["velocity"][a]) as u16);
        state.words_94_9c = std::array::from_fn(|a| number(&i["angular"][a]));
        state.rotating.basis = NativeBasis {
            words: std::array::from_fn(|a| number(&i["basis"][a])),
        };
        let class = arena.selector34_actor_mut(actor).unwrap();
        class.byte_239 = number(&i["alternate"]) as u8;
        class.table_24c = Some(0x5329f0);
        class.table_250 = Some(0x5329e8);
        class.table_254 = Some(0x5329e0);
        class.table_258 = Some(0x5329d0);
        let controls = Selector34MotionControls {
            pitch: number(&i["controls"][0]) as i32,
            yaw: number(&i["controls"][1]) as i32,
            thrust: number(&i["controls"][2]) as i32,
        };
        let result = arena.selector34_motion_forces::<()>(
            actor,
            number(&i["delta"]),
            &controls,
            exe,
            &inverse.values,
        );
        assert_eq!(
            result.is_ok(),
            case["trap"].is_null(),
            "{}: {result:?}",
            case["name"]
        );
        let state = arena.actor(actor).unwrap();
        assert_eq!(
            json!({"velocity":state.words_8e_92,"angular":state.words_94_9c,
            "alternate":arena.selector34_actor(actor).unwrap().byte_239,
            "controls":[controls.pitch as u32,controls.yaw as u32,controls.thrust as u32],
            "physics_args":if result.is_ok(){json!([number(&i["delta"]),25])}else{Value::Null}}),
            case["output"],
            "{}",
            case["name"]
        );
    }
}
