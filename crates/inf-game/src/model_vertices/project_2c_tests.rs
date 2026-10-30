//! Original 004EC5C0 receipts with real flat-H12/copy-path 0044FFA0.
//! The selected world projector is a supplied mutable service. Two actual
//! invalid source-pointer receipts retain their native diagnostics but lie
//! outside this owned-vector API; context-buffer rebinding is not fabricated.

use std::{cell::RefCell, rc::Rc};

use serde::Deserialize;
use serde_json::{json, Value};

use super::*;

#[derive(Deserialize)]
struct NativeFixture {
    executable_sha256: String,
    entry: String,
    terrain_entry: String,
    source_xyz: [[i32; 3]; 2],
    destination: i16,
    slot_count: u16,
    cases: Vec<NativeCase>,
}

#[derive(Deserialize)]
struct NativeCase {
    name: String,
    origin: String,
    command_word: Option<u16>,
    auxiliary_opcode: Option<u8>,
    native_command: i32,
    source: Option<i16>,
    project_eax: Option<u32>,
    stop_project: Option<usize>,
    stop_terrain: Option<usize>,
    #[serde(default)]
    partial_terrain: bool,
    #[serde(default)]
    unmapped_services: bool,
    stopped: Option<String>,
    invalid: Option<Value>,
    events: Vec<Value>,
    slots: Vec<NativeSlot>,
    temporary: Option<[Option<u8>; 12]>,
}

#[derive(Deserialize)]
struct NativeSlot {
    index: usize,
    bytes: [Option<u8>; 24],
}

type Events = Rc<RefCell<Vec<Value>>>;

struct TerrainService<'a> {
    inputs: ModelSurfaceCallbackInputs<'a>,
    events: Events,
    calls: usize,
    stop_at: Option<usize>,
    partial: bool,
    // Host-owned callback temporary, independent of published vertex custody.
    temporary: Option<[Option<u8>; 12]>,
}

impl ModelSurfaceVertexCallbacks for TerrainService<'_> {
    fn generate(
        &mut self,
        vertices: &ModelVertices,
        source: i32,
        callback: ModelSurfaceCallback,
    ) -> Result<[i32; 3], ModelVertexFault> {
        self.calls += 1;
        self.events
            .borrow_mut()
            .push(json!({"service":"terrain", "source_slot":source}));
        if self.stop_at == Some(self.calls) {
            // 004EC5C0 reuses one stack temporary across chunks. A later
            // stopped callback retains the previous unwritten components.
            let mut bytes = self.temporary.unwrap_or([None; 12]);
            if self.partial {
                for (target, byte) in bytes.iter_mut().zip(0x8000_0001_u32.to_le_bytes()) {
                    *target = Some(byte);
                }
            }
            self.temporary = Some(bytes);
            return Err(ModelVertexFault::UnsupportedOpcode { opcode: 0x2c });
        }
        let result = self.inputs.generate(vertices, source, callback)?;
        self.temporary = Some(world_bytes(result));
        Ok(result)
    }
}

struct ProjectionService {
    events: Events,
    outputs: Vec<usize>,
    calls: usize,
    eax: u32,
    stop_at: Option<usize>,
}

impl ModelVertexProjector for ProjectionService {
    type Error = &'static str;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        let ModelVertexInput::World(xyz) = input else {
            panic!("2C must select the world projection entry");
        };
        let address = output as *mut ModelVertexProjection as usize;
        let target = self
            .outputs
            .iter()
            .position(|&value| value == address)
            .unwrap();
        self.calls += 1;
        let partial = self.stop_at == Some(self.calls);
        self.events.borrow_mut().push(json!({
            "service":"projector", "target":target, "xyz":xyz,
            "eax":self.eax, "partial":partial
        }));
        output.write_u32(0, 0x1234_5678);
        output.bytes[8] = Some(0x5a);
        output.write_u32(4, self.eax);
        if partial {
            return Err("projection boundary");
        }
        output.bytes[9] = Some(0x37);
        Ok(self.eax)
    }
}

fn world_bytes(world: [i32; 3]) -> [Option<u8>; 12] {
    let bytes: Vec<_> = world
        .into_iter()
        .flat_map(i32::to_le_bytes)
        .map(Some)
        .collect();
    bytes.try_into().unwrap()
}

fn slot_bytes(slot: ModelVertexSlot) -> [Option<u8>; 24] {
    let mut result = [None; 24];
    if let Some(world) = slot.world {
        result[..12].copy_from_slice(&world_bytes(world));
    }
    result[12..].copy_from_slice(&slot.projection.bytes);
    result
}

#[test]
fn project_2c_matches_all_portable_native_command_and_stopped_prefix_receipts() {
    let fixture: NativeFixture = serde_json::from_str(include_str!(
        "../../tests/fixtures/model_terrain_project_command_native.json"
    ))
    .unwrap();
    assert_eq!(
        fixture.executable_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert_eq!(fixture.entry, "004EC5C0");
    assert_eq!(fixture.terrain_entry, "0044FFA0");
    assert_eq!(fixture.cases.len(), 32);
    let terrain = super::tests::flat_terrain(7, 0x10000);
    let mut compared = 0;
    let mut native_only = 0;
    for case in fixture.cases {
        if case.stopped.as_deref() == Some("native_source_read_fault") {
            // Native44FFD6 reads source+04 after terrain holder/header reads.
            // InvalidSlot/UnknownWorld do not claim that raw-pointer prefix,
            // and this API cannot rebind context+00 during projection.
            let invalid = case.invalid.as_ref().unwrap();
            assert_eq!(invalid["ip"], json!(0x44ffd6));
            assert_eq!(invalid["size"], json!(4));
            assert_eq!(case.events.last().unwrap()["service"], "terrain");
            native_only += 1;
            continue;
        }
        let command = match case.origin.as_str() {
            "renderer_word" => ModelVertexCommand::RendererWord(case.command_word.unwrap()),
            "auxiliary_opcode" => {
                ModelVertexCommand::AuxiliaryOpcode(case.auxiliary_opcode.unwrap())
            }
            other => panic!("unknown command origin {other}"),
        };
        assert_eq!(command.native_value(), case.native_command, "{}", case.name);
        let mut vertices = ModelVertices::new_invocation(fixture.slot_count);
        for (slot, world) in vertices.slots.iter_mut().zip(fixture.source_xyz) {
            slot.world = Some(world);
        }
        let events = Events::default();
        let mut terrain_service = TerrainService {
            inputs: ModelSurfaceCallbackInputs {
                binding: ModelSurfaceBinding::Terrain {
                    terrain: (!case.unmapped_services).then_some(&terrain),
                    world_flags: 0,
                    wave_clock: 0,
                    renderer_xyz: [1, 2, 3],
                },
                table: None,
            },
            events: events.clone(),
            calls: 0,
            stop_at: case.stop_terrain,
            partial: case.partial_terrain,
            temporary: None,
        };
        let mut projection = ProjectionService {
            events: events.clone(),
            outputs: vertices
                .slots
                .iter()
                .map(|slot| &slot.projection as *const _ as usize)
                .collect(),
            calls: 0,
            eax: case.project_eax.unwrap_or(0x8000_0010),
            stop_at: case.stop_project,
        };
        let result = vertices.execute_project_2c(
            ModelSurfaceVertexRequest {
                command,
                destination: fixture.destination,
                source: case.source.unwrap_or(0),
            },
            &mut terrain_service,
            &mut projection,
        );
        let expected = match case.stopped.as_deref() {
            None => Ok(()),
            Some("projection_boundary") => Err(ModelVertexError::Projection("projection boundary")),
            Some("terrain_boundary") => {
                Err(ModelVertexFault::UnsupportedOpcode { opcode: 0x2c }.into())
            }
            other => panic!("unhandled native boundary {other:?}"),
        };
        assert_eq!(result, expected, "{}", case.name);
        assert_eq!(vertices.stopped(), case.stopped.is_some(), "{}", case.name);
        let expected_events: Vec<_> = case
            .events
            .iter()
            .map(|event| match event["service"].as_str().unwrap() {
                "terrain" => json!({"service":"terrain", "source_slot":event["source_slot"]}),
                "projector" => event.clone(),
                other => panic!("unknown native service {other}"),
            })
            .collect();
        assert_eq!(*events.borrow(), expected_events, "{}", case.name);
        assert_eq!(terrain_service.temporary, case.temporary, "{}", case.name);
        for index in usize::try_from(fixture.destination).unwrap()..usize::from(fixture.slot_count)
        {
            let expected = case
                .slots
                .iter()
                .find(|slot| slot.index == index)
                .map_or([None; 24], |slot| slot.bytes);
            assert_eq!(
                slot_bytes(vertices.slots[index]),
                expected,
                "{} slot{index}",
                case.name
            );
        }
        compared += 1;
    }
    assert_eq!((compared, native_only), (30, 2));
}

#[test]
fn project_2c_adapter_and_destination_faults_follow_the_reached_service_order() {
    let request = ModelSurfaceVertexRequest {
        command: ModelVertexCommand::RendererWord(0x032c),
        destination: -1,
        source: 0,
    };
    let terrain = super::tests::flat_terrain(7, 0x10000);
    let mut projection = super::tests::RecordingProjection::default();
    // Missing callback/resident terrain is reached before unknown source or
    // invalid destination. With the callback admitted, source is reached next.
    for (resident, source_world, expected) in [
        (
            None,
            None,
            ModelVertexFault::MissingSurfaceTerrain {
                callback: ModelSurfaceCallback::Project,
            },
        ),
        (
            Some(&terrain),
            None,
            ModelVertexFault::UnknownWorld { index: 0 },
        ),
        (
            Some(&terrain),
            Some([1000, 260, 2000]),
            ModelVertexFault::InvalidSlot {
                index: -1,
                count: 1,
            },
        ),
    ] {
        let mut vertices = ModelVertices::new_invocation(1);
        vertices.slots[0].world = source_world;
        assert_eq!(
            vertices.execute_project_2c(
                request,
                &mut super::tests::project_inputs(resident),
                &mut projection
            ),
            Err(expected.into())
        );
        assert!(vertices.stopped());
    }
    assert!(projection.calls.is_empty());
    let mut zero = ModelVertices::new_invocation(0);
    let mut inputs = super::tests::project_inputs(None);
    assert_eq!(
        zero.execute_project_2c(
            ModelSurfaceVertexRequest {
                command: ModelVertexCommand::RendererWord(0x002c),
                ..request
            },
            &mut inputs,
            &mut projection
        ),
        Ok(())
    );
    assert!(!zero.stopped());
}

#[test]
fn project_2c_renderer_adapter_preserves_signed_slots_and_rejects_wrong_record_shapes() {
    let instruction = super::tests::instruction(0x962c, &[0x8000, 0xffff]);
    assert_eq!(
        ModelSurfaceVertexRequest::from_renderer(&instruction, ModelSurfaceCallback::Project),
        Ok(ModelSurfaceVertexRequest {
            command: ModelVertexCommand::RendererWord(0x962c),
            destination: i16::MIN,
            source: -1
        })
    );
    let wrong = super::tests::instruction(0x012b, &[4, 0]);
    assert_eq!(
        ModelSurfaceVertexRequest::from_renderer(&wrong, ModelSurfaceCallback::Project),
        Err(ModelVertexFault::UnsupportedOpcode { opcode: 0x2b })
    );
    let mut malformed = instruction.clone();
    let ModelInstructionKind::UnresolvedDynamic(command) = &mut malformed.kind else {
        unreachable!()
    };
    command.arguments.pop();
    assert_eq!(
        ModelSurfaceVertexRequest::from_renderer(&malformed, ModelSurfaceCallback::Project),
        Err(ModelVertexFault::InvalidInstruction { opcode: 0x2c })
    );
    malformed.kind = super::tests::instruction(0x0123, &[4, 0, 1]).kind;
    assert_eq!(
        ModelSurfaceVertexRequest::from_renderer(&malformed, ModelSurfaceCallback::Project),
        Err(ModelVertexFault::InvalidInstruction { opcode: 0x2c })
    );
}
