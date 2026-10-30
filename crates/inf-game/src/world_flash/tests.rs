use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
        ActorTerrainInputs, BaseContext,
    },
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderResources},
    Installation, ResidencyRequest, ResidentRuntime,
};
use serde_json::{json, Value};
use std::path::Path;

fn u(value: &Value) -> u32 {
    value.as_i64().unwrap() as u32
}
fn xyz(value: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&value[i]) as u16)
}
fn actor(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap()
}
struct TriggerHost<'a> {
    camera: ObjectId,
    source: ObjectId,
    input: &'a Value,
    events: Vec<Value>,
}
impl WorldFlashHost for TriggerHost<'_> {
    type Error = &'static str;
    fn acquire_camera(
        &mut self,
        arena: &mut ObjectArena,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.events.push(json!("camera"));
        for (id, key) in [
            (self.camera, "camera_after"),
            (self.source, "position_after"),
        ] {
            if !self.input[key].is_null() {
                arena
                    .initialize_context(
                        id,
                        BaseContext {
                            position_words: Some(xyz(&self.input[key])),
                            placement_index: u32::MAX,
                        },
                    )
                    .unwrap();
            }
        }
        Ok(Some(self.camera))
    }
    fn camera_basis(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<NativeBasis, Self::Error> {
        Ok(if self.input["basis"].is_null() {
            NativeBasis::INITIAL
        } else {
            NativeBasis {
                words: std::array::from_fn(|i| u(&self.input["basis"][i])),
            }
        })
    }
    fn style_word(&mut self, argument: u32) -> Result<u32, Self::Error> {
        self.events.push(
            json!({"style_argument":argument,"sequence":if self.input["style"] == 0 {1} else {4}}),
        );
        if self.input["stop_style"] == true {
            Err("supplied style boundary")
        } else {
            Ok(u(&self.input["style_word"]))
        }
    }
}

#[test]
fn world_flash_trigger_matches_15_original_native_segments() {
    let receipt: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world_flash_native.json")).unwrap();
    for (index, case) in receipt["trigger"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
            owner_count: 2,
            initial_capacity: 8,
            null_handle: ActorHandle::from_raw(0),
        })
        .unwrap();
        let camera = actor(&mut arena, xyz(&input["camera_position"]));
        let source = actor(&mut arena, xyz(&input["position"]));
        let mut state = WorldFlashState {
            sequence_5b8: u(&input["sequence"]),
            color_5bc: Some(u(&input["color"])),
            ..Default::default()
        };
        let mut host = TriggerHost {
            camera,
            source,
            input,
            events: Vec::new(),
        };
        let result = state.trigger(
            &mut arena,
            WorldFlashRequest {
                position: if input["null_position"] == true {
                    WorldFlashPosition::Null
                } else {
                    WorldFlashPosition::Object(source)
                },
                radius: u(&input["radius"]),
                style_argument: u(&input["style"]),
            },
            &mut host,
            &mut 1234,
        );
        assert_eq!(
            result.is_err(),
            case["stopped"] == true,
            "{index}: {result:?}"
        );
        assert_eq!(json!(state.sequence_5b8), case["sequence"], "{index}");
        assert_eq!(json!(state.color_5bc.unwrap()), case["color"], "{index}");
        assert_eq!(json!(host.events), case["events"], "{index}");
    }
}

struct DrawHost<'r> {
    texture: RenderResource<'r>,
    sorting: bool,
    packets: Vec<PrimitiveCommand<'r>>,
    fail: bool,
}
impl<'r> WorldFlashDrawHost<'r> for DrawHost<'r> {
    type Error = &'static str;
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        assert!((554..=559).contains(&slot));
        Ok(Some(self.texture))
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        Ok(self.sorting)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        Ok(TextureSortIdentity::NativeEntryAddress(0x4005000))
    }
    fn submit(&mut self, command: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        self.packets.push(command);
        if self.fail {
            Err("supplied submit boundary")
        } else {
            Ok(())
        }
    }
}
#[test]
fn world_flash_draw_matches_9_actual_allocator_packets_and_keeps_failed_sequence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    resident
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let resource = RenderResources::new(&resident)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 554,
        })
        .unwrap();
    let RenderResourceData::Sprite(original) = resource.data else {
        panic!("H3");
    };
    let receipt: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world_flash_native.json")).unwrap();
    for (index, case) in receipt["draw"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let mut entry = original.clone();
        entry.selectors.mode0 = u(&input["selector"]) as u8;
        entry.variants[0].flags = inf_formats::SpriteFlags::new(u(&input["flags"]) as u16);
        let mut host = DrawHost {
            texture: RenderResource {
                data: RenderResourceData::Sprite(&entry),
                ..resource
            },
            sorting: input["sorting"] == true,
            packets: Vec::new(),
            fail: false,
        };
        let mut state = WorldFlashState {
            sequence_5b8: u(&input["sequence"]),
            color_5bc: Some(u(&input["color"])),
            ..Default::default()
        };
        let carry = u(&input["carry"]) as u8;
        let viewport = std::array::from_fn(|i| u(&input["viewport"][i]));
        state
            .draw(
                viewport,
                Some(u32::from_le_bytes([carry; 4])),
                exe,
                &mut host,
            )
            .unwrap();
        assert_eq!(json!(state.sequence_5b8), case["sequence"], "{index}");
        if input["sequence"] == 0 {
            assert!(host.packets.is_empty());
            continue;
        }
        let packet = &host.packets[0];
        assert_eq!(packet.callback, PrimitiveCallback::EffectQuadNear);
        assert_eq!(json!(packet.depth), case["sort"], "{index}");
        let mut payload = packet.payload.clone();
        for (i, byte) in payload.iter_mut().enumerate() {
            if !(0x24..0x28).contains(&i) && byte.is_none() {
                *byte = Some(carry);
            }
        }
        assert_eq!(json!(payload), case["payload"], "{index}");
        state.sequence_5b8 = u(&input["sequence"]);
        host.fail = true;
        assert!(state.draw(viewport, Some(0), exe, &mut host).is_err());
        assert_eq!(state.sequence_5b8, u(&input["sequence"]));
    }
}

#[test]
fn world_flash_source_and_family_retirement_virtuals_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(assets.executable_bytes())),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for table in [0x520b84, 0x520cc8, 0x527ee0] {
        for (offset, value) in [(0x48, 0x4026b0_u32), (0xac, 0x428140), (0xb0, 0x4026d0)] {
            assert_eq!(
                exe.mapped(table + offset, 4).unwrap().bytes,
                value.to_le_bytes()
            );
        }
    }
    assert_eq!(
        &assets.executable_bytes()[0x26b0..0x26b4],
        [0x8d, 0x41, 0x44, 0xc3]
    );
    assert_eq!(&assets.executable_bytes()[0x26d0..0x26d3], [0xc2, 8, 0]);
}
