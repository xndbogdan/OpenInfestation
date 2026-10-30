//! Selected default +10/+14 callbacks admit complete nonaliased XYZ storage.
//! Native partial-pointer and alias receipts retain their diagnostic custody.

use super::*;
use serde_json::Value;

fn word(value: &Value) -> i32 {
    value.as_i64().expect("native DWORD") as u32 as i32
}

#[test]
fn surface_default_callbacks_match_every_portable_native_receipt() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/model_default_terrain_project_native.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["executable_sha256"],
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let mut compared = [0; 2];
    for case in fixture["cases"].as_array().unwrap() {
        assert_eq!(case["portable_compare"], true);
        let callback = match case["callback"].as_str().unwrap() {
            "ground" => ModelSurfaceCallback::Ground,
            "project" => ModelSurfaceCallback::Project,
            value => panic!("unknown callback {value}"),
        };
        let source: [i32; 3] = std::array::from_fn(|index| word(&case["source_xyz"][index]));
        let expected: [i32; 3] = std::array::from_fn(|index| word(&case["final_words"][index]));
        let mut vertices = ModelVertices::new_invocation(1);
        vertices.slots[0].world = Some(source);
        let mut callbacks = ModelSurfaceCallbackInputs {
            binding: ModelSurfaceBinding::Default,
            table: None,
        };
        assert_eq!(
            callbacks.generate(&vertices, 0, callback),
            Ok(expected),
            "{}",
            case["name"]
        );
        assert_eq!(vertices.world(0), Ok(source));
        compared[usize::from(callback == ModelSurfaceCallback::Project)] += 1;
    }
    assert_eq!(compared, [29, 29]);
    // This fixture section preserves source read/write IPs and native faults.
    // Owned vectors do not invent an equivalent unmapped-pointer failure.
    assert_eq!(fixture["diagnostics"].as_array().unwrap().len(), 23);
    for diagnostic in fixture["diagnostics"].as_array().unwrap() {
        assert_eq!(diagnostic["portable_compare"], false);
    }
}

struct NativeGroundProjection {
    eax: u32,
    calls: Vec<ModelVertexInput>,
}

impl ModelVertexProjector for NativeGroundProjection {
    type Error = Infallible;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        self.calls.push(input);
        output.write_u32(0, 0x1234_5678);
        output.bytes[8] = Some(0x5a);
        Ok(self.eax)
    }
}

#[test]
fn surface_2b_matches_all_supplied_native_dispatch_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/model_default_terrain_project_native.json"
    ))
    .unwrap();
    let cases = fixture["ground_dispatch"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let native_command = word(&case["native_command"]);
        let command = if native_command == 0x4900 {
            ModelVertexCommand::AuxiliaryOpcode(0x49)
        } else {
            ModelVertexCommand::RendererWord(native_command as u16)
        };
        assert_eq!(command.native_value(), native_command);
        let mut vertices = ModelVertices::new_invocation(6);
        for (slot, source) in vertices
            .slots
            .iter_mut()
            .zip(case["source_xyz"].as_array().unwrap())
        {
            slot.world = Some(std::array::from_fn(|index| word(&source[index])));
        }
        let mut callbacks = ModelSurfaceCallbackInputs {
            binding: if case["zero_mode"] == true {
                ModelSurfaceBinding::Terrain {
                    terrain: None,
                    world_flags: 0,
                    wave_clock: 0,
                    renderer_xyz: [0; 3],
                }
            } else {
                ModelSurfaceBinding::Default
            },
            table: None,
        };
        let mut projection = NativeGroundProjection {
            eax: case["world_projector"]["eax"].as_u64().unwrap() as u32,
            calls: Vec::new(),
        };
        let source = if case["zero_mode"] == true { 32767 } else { 0 };
        vertices
            .execute_ground_2b(
                ModelSurfaceVertexRequest {
                    command,
                    destination: 4,
                    source,
                },
                &mut callbacks,
                &mut projection,
            )
            .unwrap();
        for (index, expected) in case["vertices"].as_array().unwrap().iter().enumerate() {
            let slot = vertices.slots[index + 4];
            let mut actual = [None; 24];
            if let Some(world) = slot.world {
                for (output, byte) in actual[..12]
                    .iter_mut()
                    .zip(world.into_iter().flat_map(i32::to_le_bytes))
                {
                    *output = Some(byte);
                }
            }
            actual[12..].copy_from_slice(&slot.projection.bytes);
            let expected: [Option<u8>; 24] = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(actual, expected, "{} slot {}", case["name"], index + 4);
        }
        let expected_calls: Vec<_> = case["services"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|service| service[0] == "supplied_world_projector")
            .map(|service| {
                ModelVertexInput::World(std::array::from_fn(|index| word(&service[2][index])))
            })
            .collect();
        assert_eq!(projection.calls, expected_calls, "{}", case["name"]);
    }
}

#[test]
fn surface_2b_masks_modes_bypasses_zero_services_and_keeps_unbiased_depth() {
    let mut projection = super::tests::RecordingProjection::default();
    let mut missing = super::tests::project_inputs(None);
    let mut zero = ModelVertices::new_invocation(0);
    zero.execute_ground_2b(
        super::tests::surface_request(0x102b, -1, -1),
        &mut missing,
        &mut projection,
    )
    .unwrap();
    assert!(projection.calls.is_empty());
    assert!(!zero.stopped());

    let mut vertices = ModelVertices::new_invocation(6);
    vertices.slots[0].world = Some([65537, i32::MIN, -65537]);
    vertices.slots[1].world = Some([17, i32::MAX, -19]);
    let mut callbacks = ModelSurfaceCallbackInputs {
        binding: ModelSurfaceBinding::Default,
        table: None,
    };
    vertices
        .execute_ground_2b(
            super::tests::surface_request(0x962b, 4, 0),
            &mut callbacks,
            &mut projection,
        )
        .unwrap();
    assert_eq!(vertices.slots[4].world, None);
    assert_eq!(vertices.slots[4].projection.read_u32(4), Some(1));
    assert_eq!(vertices.world(5), Ok([17, -300, -19]));
    assert_eq!(
        projection.calls,
        [ModelVertexInput::World([65537, -300, -65537])]
    );
}

#[test]
fn surface_selected_service_precedes_source_and_destination_custody() {
    let mut callbacks = super::tests::project_inputs(None);
    let mut projection = super::tests::RecordingProjection::default();
    let mut vertices = ModelVertices::new_invocation(0);
    assert_eq!(
        vertices.execute_ground_2b(
            super::tests::surface_request(0x012b, -1, -1),
            &mut callbacks,
            &mut projection,
        ),
        Err(ModelVertexError::Fault(
            ModelVertexFault::MissingSurfaceTerrain {
                callback: ModelSurfaceCallback::Ground,
            }
        ))
    );
    assert!(vertices.stopped());
    assert!(projection.calls.is_empty());
}

#[test]
fn surface_missing_terrain_keeps_slots_and_zero_modes_lazy() {
    for (callback, opcode) in [
        (ModelSurfaceCallback::Ground, 0x2b),
        (ModelSurfaceCallback::Project, 0x2c),
    ] {
        for (destination, source) in [(1, 0), (1, -1), (-1, 0)] {
            let mut vertices = ModelVertices::new_invocation(2);
            // Preserve a prior destination while source0 stays unknown. The
            // selected missing service precedes all source/destination reads.
            vertices.slots[1].world = Some([17, -19, 23]);
            vertices.slots[1].projection.bytes = [Some(0x5a); 12];
            let before = vertices.slots.clone();
            let mut callbacks = super::tests::project_inputs(None);
            let mut projection = super::tests::RecordingProjection::default();
            let request = super::tests::surface_request(0x0300 | opcode, destination, source);
            let result = match callback {
                ModelSurfaceCallback::Ground => {
                    vertices.execute_ground_2b(request, &mut callbacks, &mut projection)
                }
                ModelSurfaceCallback::Project => {
                    vertices.execute_project_2c(request, &mut callbacks, &mut projection)
                }
            };
            assert_eq!(
                result,
                Err(ModelVertexFault::MissingSurfaceTerrain { callback }.into())
            );
            assert_eq!(vertices.slots, before);
            assert!(vertices.stopped());
            assert!(projection.calls.is_empty());
        }
        // A zero-mode command never reaches the selected service, including
        // with absent terrain and invalid source/destination indices.
        let mut vertices = ModelVertices::new_invocation(0);
        let mut callbacks = super::tests::project_inputs(None);
        let mut projection = super::tests::RecordingProjection::default();
        let request = super::tests::surface_request(opcode, -1, -1);
        let result = match callback {
            ModelSurfaceCallback::Ground => {
                vertices.execute_ground_2b(request, &mut callbacks, &mut projection)
            }
            ModelSurfaceCallback::Project => {
                vertices.execute_project_2c(request, &mut callbacks, &mut projection)
            }
        };
        assert_eq!(result, Ok(()));
        assert!(!vertices.stopped());
        assert!(projection.calls.is_empty());
    }
}

/// Opcode 2B marks the slots it projects onto the ground for the scene feed,
/// whose tracer drops the darkening sprites laid over them (built-in ground
/// shadows); a later plain projection of a slot clears its mark.
#[test]
fn surface_2b_marks_its_projected_slots_for_the_scene_feed() {
    let mut vertices = ModelVertices::new_invocation(4);
    for slot in vertices.slots.iter_mut() {
        slot.world = Some([0, 0, 0]);
    }
    let mut callbacks = ModelSurfaceCallbackInputs {
        binding: ModelSurfaceBinding::Default,
        table: None,
    };
    let mut projection = NativeGroundProjection {
        eax: 100,
        calls: Vec::new(),
    };
    // Mode 0x0A: both chunks project, into slots 2 and 3.
    vertices
        .execute_ground_2b(
            ModelSurfaceVertexRequest {
                command: ModelVertexCommand::RendererWord(0x0a2b),
                destination: 2,
                source: 0,
            },
            &mut callbacks,
            &mut projection,
        )
        .unwrap();
    assert!(vertices.ground_projected(2) && vertices.ground_projected(3));
    assert!(!vertices.ground_projected(0) && !vertices.depth_biased(2));
    vertices
        .project::<Infallible, _>(2, ModelVertexInput::World([0, 0, 0]), &mut projection)
        .unwrap();
    assert!(!vertices.ground_projected(2) && vertices.ground_projected(3));
}
