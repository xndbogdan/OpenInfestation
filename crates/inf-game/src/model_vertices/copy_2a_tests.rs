use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    slots: Vec<Vec<u8>>,
    parent: Option<Vec<Vec<u8>>>,
    words: Vec<i16>,
    origin: [i32; 3],
    arguments: [i16; 2],
    command: u16,
    /// Target slot and the x/y/z the stub received.
    calls: Vec<[i32; 4]>,
    changed: Vec<(usize, Vec<u8>)>,
    next_words: i32,
}

fn seeded(bytes: &[Vec<u8>]) -> ModelVertices {
    let mut vertices = ModelVertices::new_invocation(bytes.len() as u16);
    for (slot, raw) in vertices.slots.iter_mut().zip(bytes) {
        *slot = decode_slot(std::array::from_fn(|i| Some(raw[i])));
    }
    vertices
}

#[derive(Default)]
struct Projector {
    calls: Vec<ModelVertexInput>,
}

impl ModelVertexProjector for Projector {
    type Error = Infallible;

    fn project(
        &mut self,
        input: ModelVertexInput,
        output: &mut ModelVertexProjection,
    ) -> Result<u32, Self::Error> {
        let token = self.calls.len() as u32;
        self.calls.push(input);
        // The receipt stub's own writes, then its returned depth token.
        output.write_u32(0, 0x1234_5678 + token);
        output.bytes[8] = Some(0x31);
        Ok(0x5000 + token)
    }
}

/// 004ED350 run unchanged over every mode nibble, sign-extended commands
/// and random bases, with and without a parent: every slot byte and every
/// local projection of the root's zero point.
#[test]
fn copy_2a_matches_original_004ed350() {
    let receipt: Receipt =
        serde_json::from_str(include_str!("../../tests/fixtures/vertex_copy_native.json")).unwrap();
    let (mut changed, mut projections) = (0, 0);
    for case in &receipt.cases {
        assert_eq!(case.next_words, 2, "{}", case.name);
        let mut vertices = seeded(&case.slots);
        let parent = case.parent.as_deref().map(seeded);
        let words: Vec<u16> = case.words.iter().map(|word| *word as u16).collect();
        let mut projector = Projector::default();
        vertices
            .execute_copy_2a(
                &super::tests::instruction(case.command, &case.arguments.map(|word| word as u16)),
                &words,
                parent.as_ref(),
                MirrorCopySource::Partner,
                case.origin,
                &mut projector,
            )
            .unwrap();
        let mut expected = case.slots.clone();
        for (index, bytes) in &case.changed {
            expected[*index] = bytes.clone();
        }
        let actual: Vec<Vec<u8>> = vertices
            .slots
            .iter()
            .map(|slot| {
                encode_slot(*slot)
                    .iter()
                    .map(|byte| byte.unwrap())
                    .collect()
            })
            .collect();
        assert_eq!(actual, expected, "{}", case.name);
        let calls: Vec<_> = case
            .calls
            .iter()
            .map(|call| [call[1], call[2], call[3]])
            .collect();
        let inputs: Vec<_> = projector
            .calls
            .iter()
            .map(|input| match input {
                ModelVertexInput::Local(local) => *local,
                ModelVertexInput::World(_) => {
                    panic!("{}: the root projects a local point", case.name)
                }
            })
            .collect();
        assert_eq!(inputs, calls, "{}", case.name);
        assert!(
            case.calls
                .iter()
                .all(|call| call[0] == i32::from(case.arguments[0])),
            "{}",
            case.name
        );
        changed += case.changed.len();
        projections += case.calls.len();
    }
    assert_eq!((receipt.cases.len(), changed, projections), (66, 105, 24));
}

/// [`MirrorCopySource::MirrorSide`] changes only the ACAM's gun mounts. In
/// Level 1's models (every world loads the same vehicle), the mirror-only
/// copies whose parent passes an odd slot are the right-side mounts of the
/// buggy model 118 (child 144) and of the hover morph model 120 (children
/// 149, 151 and 153). Their left-side twins get the even slots, and every
/// other form passes both sides the even ones.
#[test]
fn the_mirror_side_fix_reaches_only_the_vehicle_gun_mounts() {
    use crate::render_resources::{
        RenderResource, RenderResourceData, RenderResourceKind, RenderResourceRequest,
        RenderResources,
    };
    let (resident, _assets, _world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let resources = RenderResources::new(&resident);
    let model = |argument: i32| match resources.resolve(RenderResourceRequest {
        kind: RenderResourceKind::Model,
        argument,
    }) {
        Ok(RenderResource {
            data: RenderResourceData::Model(entry),
            ..
        }) => Some(entry),
        _ => None,
    };
    let mut odd = std::collections::BTreeMap::<_, Vec<u16>>::new();
    for slot in 0..resources.pool_capacity(RenderResourceKind::Model) as i32 {
        let Some(parent) = model(slot) else {
            continue;
        };
        for instruction in &parent.program.instructions {
            let ModelInstructionKind::ModelInstance(instance) = &instruction.kind else {
                continue;
            };
            let Some(child) = model(i32::from(instance.model.raw())) else {
                continue;
            };
            for copy in &child.program.instructions {
                let raw = copy.raw_command;
                let ModelInstructionKind::UnresolvedDynamic(command) = &copy.kind else {
                    continue;
                };
                if raw as u8 != 0x2a || raw & 0x0300 != 0 || raw & 0x0c00 == 0 {
                    continue;
                }
                let passed = instance.state_words[usize::from(command.arguments[1])];
                if passed & 1 == 1 {
                    odd.entry((slot, instruction.span.start, instance.model.raw()))
                        .or_default()
                        .push(passed);
                }
            }
        }
    }
    let hover = vec![29, 17, 15, 27];
    let buggy = vec![17, 11, 21, 15];
    assert_eq!(
        odd.into_iter().collect::<Vec<_>>(),
        [
            ((118, 0x122f8, 144), buggy.clone()),
            ((118, 0x12332, 144), buggy),
            ((120, 0x13e42, 149), hover.clone()),
            ((120, 0x13e84, 151), hover.clone()),
            ((120, 0x13ebe, 151), hover.clone()),
            ((120, 0x13f00, 153), hover.clone()),
            ((120, 0x13f3a, 153), hover),
        ]
    );
}
