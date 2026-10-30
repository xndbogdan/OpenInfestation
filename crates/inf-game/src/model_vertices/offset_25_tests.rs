use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
pub(super) struct Receipt {
    pub(super) cases: Vec<Case>,
}

#[derive(Deserialize)]
pub(super) struct Case {
    pub(super) name: String,
    pub(super) worlds: Vec<[i32; 3]>,
    pub(super) origin: [i32; 3],
    pub(super) arguments: [i16; 3],
    pub(super) command: u16,
    turned: Vec<[i32; 3]>,
    /// Target slot, x, y, z and the depth the stub returned.
    calls: Vec<[i64; 5]>,
    pub(super) writes: Vec<(usize, [i32; 3])>,
    depths: Vec<(usize, u32)>,
    next_words: i32,
}

pub(super) fn receipt() -> Receipt {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/vertex_offset_native.json"
    ))
    .unwrap()
}

pub(super) fn seeded(case: &Case) -> ModelVertices {
    let mut vertices = ModelVertices::new_invocation(case.worlds.len() as u16);
    for (slot, world) in vertices.slots.iter_mut().zip(&case.worlds) {
        slot.world = Some(*world);
    }
    vertices
}

pub(super) fn expected_worlds(case: &Case) -> Vec<[i32; 3]> {
    let mut worlds = case.worlds.clone();
    for (index, world) in &case.writes {
        worlds[*index] = *world;
    }
    worlds
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
            panic!("004EC810 projects generated world vertices")
        };
        self.calls.push(world);
        Ok(0x5000 + self.calls.len() as u32 - 1)
    }
}

/// 004EC810 run unchanged over every mode nibble, sign-extended commands,
/// aliased sources and wrapping edges: world writes, projector inputs and
/// the depth DWORD each projection stored.
#[test]
fn origin_offset_matches_original_004ec810() {
    let receipt = receipt();
    let (mut writes, mut projections) = (0, 0);
    for case in &receipt.cases {
        assert!(
            case.turned.iter().all(|turned| *turned == [0; 3]),
            "{}",
            case.name
        );
        assert_eq!(case.next_words, 3, "{}", case.name);
        let mut vertices = seeded(case);
        let mut projector = Projector::default();
        let words = case.arguments.map(|word| word as u16);
        vertices
            .execute_origin_offset(
                &super::tests::instruction(case.command, &words),
                case.origin,
                &mut projector,
            )
            .unwrap();
        let worlds: Vec<_> = vertices
            .slots
            .iter()
            .map(|slot| slot.world.unwrap())
            .collect();
        assert_eq!(worlds, expected_worlds(case), "{}", case.name);
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
        for call in &case.calls {
            assert!(
                case.depths.contains(&(call[0] as usize, call[4] as u32)),
                "{}",
                case.name
            );
        }
        writes += case.writes.len();
        projections += case.calls.len();
    }
    assert_eq!((receipt.cases.len(), writes, projections), (56, 67, 65));
}

#[test]
fn origin_offset_rejects_other_records_and_stops_the_invocation() {
    let mut vertices = ModelVertices::new_invocation(4);
    let error = vertices
        .execute_origin_offset(
            &super::tests::instruction(0x0123, &[0, 1, 2]),
            [0; 3],
            &mut Projector::default(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        ModelVertexError::Fault(ModelVertexFault::InvalidInstruction { opcode: 0x23 })
    );
    let error = vertices
        .execute_origin_offset(
            &super::tests::instruction(0x0125, &[0, 1, 2]),
            [0; 3],
            &mut Projector::default(),
        )
        .unwrap_err();
    assert_eq!(error, ModelVertexError::Fault(ModelVertexFault::Stopped));
}
