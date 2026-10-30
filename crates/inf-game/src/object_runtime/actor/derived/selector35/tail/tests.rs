use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, FixedPositionTarget, NativeTarget,
    ProcessActorBehaviorTables,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling};
use inf_formats::{
    native_basis::NativeBasis, parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId,
    HandlerRegion, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
const SINE: [u16; 4096] = [0; 4096];
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn point(v: &Value) -> [u16; 3] {
    std::array::from_fn(|n| u(&v[n]) as u16)
}
struct Host {
    terrain: TerrainRuntime,
}
impl ActorTerrainHost for Host {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        Ok(&self.terrain)
    }
}
impl Selector35ClearanceHost for Host {
    fn spacing(&mut self) -> TerrainSpacing {
        self.terrain.sampling().spacing
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.terrain.sampling().spacing
    }
    fn height_shift(&mut self) -> u32 {
        self.terrain.sampling().height_scale.shift()
    }
    fn fallback_placement(&self) -> u8 {
        0
    }
    fn furniture_model_radius(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u8,
    ) -> Result<Option<u16>, Self::Error> {
        panic!("no placement in these native cells")
    }
}
fn host(input: &Value) -> Host {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for n in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + n * 2] = input["height"].as_i64().unwrap() as i8 as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    Host {
        terrain: TerrainRuntime::new(
            &bank.entries[0],
            TerrainSampling {
                spacing: if u(&input["spacing"]) == 128 {
                    TerrainSpacing::Units128
                } else {
                    TerrainSpacing::Units256
                },
                height_scale: if u(&input["height_shift"]) == 5 {
                    TerrainHeightScale::Units32
                } else {
                    TerrainHeightScale::Units16
                },
            },
        ),
    }
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
fn target(arena: &mut ObjectArena, position: [u16; 3]) -> NativeHeapRef {
    arena.import_fixture_target(NativeTarget::FixedPosition(
        FixedPositionTarget::from_point(ActorHandle::from_raw(77), Some(position)),
    ))
}
#[test]
fn selector35_tail_matches_21_native_target_clearance_and_control_cases() {
    let fixture_json: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_tail_native.json"
    )))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture_json["provenance"]["exe_sha256"].as_str().unwrap()
    );
    let guard = &fixture_json["guard"];
    let start = (u(&guard["address"]) - 0x400000) as usize;
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(&bytes[start..start + u(&guard["size"]) as usize])
        ),
        guard["sha256"].as_str().unwrap()
    );
    for (index, case) in fixture_json["cases"].as_array().unwrap().iter().enumerate() {
        let i = &case["input"];
        let (mut arena, actor) = fixture();
        let primary = target(&mut arena, point(&i["focus"]));
        let secondary = target(&mut arena, point(&i["secondary_focus"]));
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.position_words = Some(point(&i["position"]));
        base.word_28 = u(&i["size"]) as u16;
        base.flags = u(&i["flags"]);
        let state = arena.actor_mut(actor).unwrap();
        state.words_8e_92 = point(&i["velocity"]);
        state.rotating.basis = NativeBasis {
            words: std::array::from_fn(|n| u(&i["source_basis"][n])),
        };
        let derived = arena.derived_actor_mut(actor).unwrap();
        derived.target_148 = i["current"].as_bool().unwrap().then_some(primary);
        derived.target_154 = i["secondary"].as_bool().unwrap().then_some(secondary);
        derived.word_14c = Some(0x87654321);
        derived.word_150 = Some(0x98765432);
        derived.word_158 = Some(0xabcdef01);
        derived.word_15c = Some(u(&i["secondary_timer"]));
        arena.family428650_actor_mut(actor).unwrap().words_204_208[1] =
            Some(i["speed"].as_i64().unwrap() as u32);
        let class = arena.selector35_actor_mut(actor).unwrap();
        class.word_230 = u(&i["offset"]);
        class.word_234 = u(&i["desired_height"]);
        class.byte_238 = u(&i["byte238"]) as u8;
        class.byte_250 = u(&i["byte250"]) as u8;
        class.byte_251 = Some(u(&i["alternate"]) as u8);
        class.word_260 = Some(0xbadf00d);
        let mut terrain = host(i);
        let mut rng = 123;
        let mut tables = ProcessActorBehaviorTables::from_executable(
            inf_formats::FrontendExecutable::parse(bytes).unwrap(),
        )
        .unwrap();
        let controls = arena
            .tick_selector35_tail(
                actor,
                Selector35TailRequest {
                    microseconds: u(&i["delta"]),
                    settings_7c: 0,
                    firing: ActorWeaponFireContext {
                        tables: &mut tables,
                        rng: &mut rng,
                        process_velocity: None,
                    },
                    clearance: &mut terrain,
                },
            )
            .unwrap_or_else(|e| panic!("case{index}: {e:?}"));
        let d = arena.derived_actor(actor).unwrap();
        let s = arena.selector35_actor(actor).unwrap();
        assert_eq!(
            json!({"controls":[controls.yaw as u32,controls.vertical as u32,controls.horizontal as u32,controls.direction_x as u32,controls.direction_z as u32],"word14c":d.word_14c.unwrap(),"word150":d.word_150.unwrap(),"secondary":d.target_154.is_some(),"word158":d.word_158.unwrap(),"word15c":d.word_15c.unwrap(),"word260":s.word_260.unwrap(),"byte250":s.byte_250,"flags":arena.object(actor).unwrap().base.flags}),
            case["output"],
            "case{index}"
        );
        assert_eq!(rng, 123);
    }
}
