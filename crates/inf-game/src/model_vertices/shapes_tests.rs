use std::sync::Arc;

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    worlds: Vec<[i32; 3]>,
    origin: [i32; 3],
    registers: Vec<u16>,
    arguments: Vec<u16>,
    command: u16,
    /// Target slot, x, y, z and the depth the stub returned.
    calls: Vec<[i64; 5]>,
    writes: Vec<(usize, [i32; 3])>,
    depths: Vec<(usize, u32)>,
    next_words: i32,
}

#[derive(Default)]
struct Projector {
    calls: Vec<[i32; 3]>,
}

impl ModelVertexProjector for Projector {
    type Error = Infallible;

    fn project(
        &mut self,
        input: ModelVertexInput,
        _: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        let ModelVertexInput::World(world) = input else {
            panic!("the shaped handlers project generated world vertices")
        };
        self.calls.push(world);
        Ok(0x5000 + self.calls.len() as u32 - 1)
    }
}

/// The receipt's weights are literals or registers: no callback runs.
struct NoCallback;

impl ModelValueCallback for NoCallback {
    type Error = Infallible;

    fn packed_value(&mut self, _: &mut ModelValues, selector: u32) -> Result<u16, Self::Error> {
        panic!("no packed callback expected ({selector:#x})")
    }
}

/// 004EC1E0, 004EC710, 004ECCE0, 004ECAE0 and 004ECFE0 run unchanged over
/// every mode nibble, sign-extended commands, aliased and mirrored sources,
/// small and wrapping worlds and literal or register weights: world writes,
/// projector inputs and the depth DWORD each projection stored.
#[test]
fn shaped_vertices_match_the_original_handlers() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../tests/fixtures/vertex_shapes_native.json"
    ))
    .unwrap();
    let (mut writes, mut projections) = (0, 0);
    for case in &receipt.cases {
        assert_eq!(
            case.next_words as usize,
            case.arguments.len(),
            "{}",
            case.name
        );
        let mut vertices = ModelVertices::new_invocation(case.worlds.len() as u16);
        for (slot, world) in vertices.slots.iter_mut().zip(&case.worlds) {
            slot.world = Some(*world);
        }
        let mut values = ModelValues::new_invocation(Arc::new([0; 4096]));
        for (index, value) in case.registers.iter().take(22).enumerate() {
            values.write_register(index as i16, *value).unwrap();
        }
        let mut projector = Projector::default();
        vertices
            .execute_shaped(
                &super::tests::instruction(case.command, &case.arguments),
                case.origin,
                &mut values,
                &mut NoCallback,
                &mut projector,
            )
            .unwrap();
        let mut expected = case.worlds.clone();
        for (index, world) in &case.writes {
            expected[*index] = *world;
        }
        let worlds: Vec<_> = vertices
            .slots
            .iter()
            .map(|slot| slot.world.unwrap())
            .collect();
        assert_eq!(worlds, expected, "{}", case.name);
        let calls: Vec<[i64; 3]> = case
            .calls
            .iter()
            .map(|call| [call[1], call[2], call[3]])
            .collect();
        let ported: Vec<[i64; 3]> = projector
            .calls
            .iter()
            .map(|call| call.map(i64::from))
            .collect();
        assert_eq!(ported, calls, "{}", case.name);
        let depths: Vec<_> = vertices
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.projection.read_u32(4).map(|depth| (index, depth)))
            .collect();
        assert_eq!(depths, case.depths, "{}", case.name);
        writes += case.writes.len();
        projections += case.calls.len();
    }
    assert_eq!((receipt.cases.len(), writes, projections), (180, 194, 190));
}
