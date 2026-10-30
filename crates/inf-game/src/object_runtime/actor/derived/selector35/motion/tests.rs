use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest,
};
use inf_formats::native_basis::NativeBasis;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
const SINE: [u16; 4096] = [0; 4096];
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector35_actor(
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
                        sine_quarter: &SINE,
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
fn selector35_motion_forces_match_original_body_and_shared_owners() {
    let fixtures: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_motion_native.json"
    )))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let bytes = assets.executable_bytes();
    let exe = FrontendExecutable::parse(bytes).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixtures["provenance"]["exe_sha256"].as_str().unwrap()
    );
    let guard = &fixtures["guard"];
    let at = (u(&guard["address"]) - 0x400000) as usize;
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&bytes[at..at + u(&guard["size"]) as usize])
        ),
        guard["sha256"].as_str().unwrap()
    );
    for (index, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let (mut arena, actor) = fixture();
        arena.owned_mut(actor).unwrap().state.base.flags = u(&input["flags"]);
        let state = arena.actor_mut(actor).unwrap();
        state.word_c4 = u(&input["state"]);
        state.words_8e_92 = std::array::from_fn(|a| u(&input["velocity"][a]) as u16);
        state.words_94_9c = std::array::from_fn(|a| u(&input["angular"][a]));
        state.rotating.basis = NativeBasis {
            words: std::array::from_fn(|a| u(&input["basis"][a])),
        };
        let class = arena.selector35_actor_mut(actor).unwrap();
        class.word_258 = Some(u(&input["limit"]));
        class.word_25c = Some(u(&input["budget"]));
        class.word_264 = u(&input["drift"]);
        class.byte_251 = Some(u(&input["alternate"]) as u8);
        class.table_270 = Some(if u(&input["alternate"]) == 0 {
            0x533d68
        } else {
            0x533d70
        });
        let controls = &input["controls"];
        let mut controls = Selector35MotionControls {
            yaw: i(&controls[0]),
            vertical: i(&controls[1]),
            horizontal: i(&controls[2]),
            direction_x: i(&controls[3]),
            direction_z: i(&controls[4]),
        };
        arena
            .selector35_motion_forces(actor, u(&input["delta"]), &mut controls, exe)
            .unwrap_or_else(|e| panic!("case{index}: {e:?}"));
        let state = arena.actor(actor).unwrap();
        assert_eq!(
            json!({"velocity":state.words_8e_92,"angular":state.words_94_9c,"basis":state.rotating.basis.words,"controls":[controls.yaw as u32,controls.vertical as u32,controls.horizontal as u32,controls.direction_x as u32,controls.direction_z as u32],"physics_args":[u(&input["delta"]),31]}),
            case["output"],
            "case{index}"
        );
    }
}
#[test]
fn selector35_motion_runs_real_physics_before_clearing_override_byte() {
    let (mut arena, actor) = fixture();
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    arena.selector35_actor_mut(actor).unwrap().byte_251 = Some(0);
    arena.selector35_actor_mut(actor).unwrap().byte_250 = 1;
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    arena
        .tick_selector35_motion(
            actor,
            Selector35MotionRequest {
                microseconds: 1024,
                controls: &mut Selector35MotionControls::default(),
                executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
                sine_quarter: &SINE,
            },
        )
        .unwrap();
    //47B1E0 applies gravity first, then real linear drag clears the unit
    //velocity. The test must observe the complete physics phase, not gravity.
    assert_eq!(arena.actor(actor).unwrap().words_8e_92[1], 0);
    assert_eq!(arena.selector35_actor(actor).unwrap().byte_250, 0);
}
