use super::*;
use serde_json::{json, Value};

struct Projector {
    positions: Vec<[u32; 3]>,
    stop_at: Option<usize>,
}

impl ModelVertexProjector for Projector {
    type Error = &'static str;

    fn project(
        &mut self,
        input: ModelVertexInput,
        _output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        let ModelVertexInput::World(position) = input else {
            panic!("auxiliary pair must project generated world coordinates");
        };
        self.positions.push(position.map(|v| v as u32));
        if self.stop_at == Some(self.positions.len()) {
            return Err("stop");
        }
        Ok(100 + self.positions.len() as u32)
    }
}

#[test]
fn combat_auxiliary_pair_commands_match_native_modes_aliases_and_projector_stops() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/combat_auxiliary_pair_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let source = input["vertices"].as_array().unwrap();
        let mut vertices = ModelVertices::new_invocation(source.len() as u16);
        for (slot, words) in vertices.slots.iter_mut().zip(source) {
            slot.world = Some(std::array::from_fn(|i| words[i].as_i64().unwrap() as i32));
            slot.projection.bytes = [Some(0xa5); 12];
        }
        let opcode = input["opcode"].as_u64().unwrap() as u8;
        let mut projector = Projector {
            positions: Vec::new(),
            stop_at: input["stop_at"].as_u64().map(|n| n as usize),
        };
        let result = vertices.execute_pair(
            ModelPairVertexRequest {
                command: ModelVertexCommand::AuxiliaryOpcode(opcode),
                destination: input["destination"].as_i64().unwrap() as i16,
                sources: std::array::from_fn(|i| input["sources"][i].as_i64().unwrap() as i16),
                operation: if opcode == 0x18 {
                    ModelPairOperation::Midpoint
                } else {
                    ModelPairOperation::Interpolate {
                        weight: case["weight"].as_u64().unwrap() as u16,
                    }
                },
            },
            &mut projector,
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case{index}: {result:?}"
        );
        let expected_positions: Vec<_> = case["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["position"].clone())
            .collect();
        assert_eq!(
            json!(projector.positions),
            json!(expected_positions),
            "case{index}"
        );
        let words: Vec<[u32; 6]> = vertices
            .slots
            .iter()
            .map(|slot| {
                let world = slot.world.unwrap();
                [
                    world[0] as u32,
                    world[1] as u32,
                    world[2] as u32,
                    slot.projection.read_u32(0).unwrap(),
                    slot.projection.read_u32(4).unwrap(),
                    slot.projection.read_u32(8).unwrap(),
                ]
            })
            .collect();
        assert_eq!(json!(words), case["vertices"], "case{index}");
    }
}
