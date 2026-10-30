//! Unmodified 00448200/00479E10 receipts, before common completion.

use std::path::Path;

use inf_formats::{InstanceCommand, InstanceField};
use serde::Deserialize;

use super::*;
use crate::object_runtime::native_model_fixture::{Fixture, Models};
use crate::{
    object_runtime::{ObjectList, ObjectNodeKind, ObjectState, VisualObject, INITIALIZED_FLAG},
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    Installation, ResidencyRequest,
};

#[derive(Deserialize)]
struct Receipts {
    synthetic: Vec<Case>,
    corpus: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    input: Input,
    after_init: State,
    after_commands: State,
    accepted: bool,
}

#[derive(Deserialize)]
struct Input {
    placement_index: u32,
    resource_argument: u32,
    position_words: Option<[u16; 3]>,
    model_word: u16,
    h8: Models,
    payload: Payload,
    commands: Vec<Command>,
    seed_before_init: Option<Seed>,
    suffix_count: Option<i32>,
    #[serde(default)]
    null_suffix: bool,
}

#[derive(Deserialize)]
struct Payload {
    flags: u32,
    rgb_bytes: [u8; 3],
    draw_gate: u8,
    radius_word: u16,
}

#[derive(Deserialize)]
struct Command {
    code: u32,
    payload_bytes: Vec<u8>,
}

#[derive(Deserialize)]
struct Seed {
    flags: u32,
    dirty_word: u16,
}

#[derive(Clone, Deserialize)]
struct State {
    word_28: u16,
    placement_word: u16,
    flags: u32,
    resource_word: u16,
    position_words: [u16; 3],
    dirty_word: u16,
    word_3c: u32,
    word_40: u16,
    model_word: u16,
    rgb_bytes: [u8; 3],
    rgb_signed: [i8; 3],
    draw_gate: u8,
    radius_word: u16,
    lists: Lists,
}

#[derive(Clone, Deserialize)]
struct Lists {
    flag0: Vec<u32>,
    flag1: Vec<u32>,
    flag0_secondary: Vec<u32>,
    flag1_secondary: Vec<u32>,
    parking: Vec<u32>,
    disposal: Vec<u32>,
}

fn receipts() -> Receipts {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/object_light_native.json"
    ))
    .unwrap()
}

struct Commands {
    placement: PlacementInstanceAssets,
    source: Vec<u8>,
}

impl Commands {
    fn new(input: &Input) -> Self {
        let mut source = vec![];
        let commands = input
            .commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let offset = source.len();
                source.extend_from_slice(&command.code.to_le_bytes());
                source.extend_from_slice(&command.payload_bytes);
                InstanceCommand {
                    index: index as u32,
                    pointer_span: ByteSpan::new(0, 0),
                    allocation_offset: offset as u32,
                    code: InstanceField {
                        span: ByteSpan::new(offset, offset + 4),
                        value: command.code as i32,
                    },
                    payload_offset: (offset + 4) as u32,
                }
            })
            .collect();
        Self {
            placement: PlacementInstanceAssets {
                placement_index: input.placement_index,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: input.suffix_count.unwrap_or(input.commands.len() as i32),
                },
                command_pointer_span: ByteSpan::new(0, 0),
                commands,
            },
            source,
        }
    }

    fn request(&self) -> ObjectInstanceCommands<'_> {
        ObjectInstanceCommands {
            placement: &self.placement,
            source: &self.source,
            main_payload: ByteSpan::new(0, self.source.len()),
            relocations: &[],
        }
    }
}

fn initialize(arena: &mut ObjectArena, runtime: &ResidentRuntime, input: &Input) -> ObjectId {
    let id = arena.allocate(ObjectClassKind::TerrainLight);
    if let Some(seed) = &input.seed_before_init {
        let base = &mut arena.owned_mut(id).unwrap().state.base;
        base.flags = seed.flags;
        base.dirty_word = seed.dirty_word;
    }
    arena
        .initialize_visual_context(
            id,
            BaseContext {
                position_words: input.position_words,
                placement_index: input.placement_index,
            },
            Some(u32::from(input.model_word)),
            runtime,
        )
        .unwrap();
    arena
        .initialize_terrain_light_payload(
            id,
            input.payload.flags,
            TerrainLight {
                channels: input.payload.rgb_bytes.map(|byte| byte as i8),
                draw_gate: input.payload.draw_gate,
                radius: input.payload.radius_word,
            },
        )
        .unwrap();
    id
}

fn assert_state(
    arena: &ObjectArena,
    id: ObjectId,
    expected: &State,
    position_known: bool,
    label: &str,
) {
    assert_eq!(
        expected.rgb_bytes.map(|byte| byte as i8),
        expected.rgb_signed
    );
    let object = arena.object(id).unwrap();
    let expected_object = ObjectState {
        base: crate::object_runtime::BaseObject {
            word_28: expected.word_28,
            placement_word: expected.placement_word,
            flags: expected.flags,
            resource_word: expected.resource_word,
            // Native allocation poison is observable in the receipt but is
            // deliberately unknown in the safe arena until a position arrives.
            position_words: position_known.then_some(expected.position_words),
            dirty_word: expected.dirty_word,
            word_3c: expected.word_3c,
        },
        class: ObjectClass::TerrainLight {
            visual: VisualObject {
                radius_word: expected.word_40,
                model_word: expected.model_word,
            },
            light: Some(TerrainLight {
                channels: expected.rgb_signed,
                draw_gate: expected.draw_gate,
                radius: expected.radius_word,
            }),
        },
    };
    assert_eq!(*object, expected_object, "{label}");
    for (role, expected_nodes) in [
        (ObjectList::Flag0Primary, &expected.lists.flag0),
        (ObjectList::Flag1Primary, &expected.lists.flag1),
        (ObjectList::Flag0Secondary, &expected.lists.flag0_secondary),
        (ObjectList::Flag1Secondary, &expected.lists.flag1_secondary),
        (ObjectList::Parking, &expected.lists.parking),
        (ObjectList::Disposal, &expected.lists.disposal),
    ] {
        let offsets: Vec<_> = arena
            .list(role)
            .iter()
            .filter(|node| node.object == id)
            .map(|node| match node.kind {
                ObjectNodeKind::Flag0 => 4,
                ObjectNodeKind::Flag1 => 16,
                ObjectNodeKind::Lifecycle => 28,
            })
            .collect();
        assert_eq!(&offsets, expected_nodes, "{label}: {role:?}");
    }
}

fn reaches_inclusive_boundary(input: &Input) -> bool {
    input.commands.iter().any(|command| {
        command.code == 2 && {
            let word = u16::from_le_bytes(command.payload_bytes[..2].try_into().unwrap());
            usize::from(word) == input.h8.capacity
        }
    })
}

#[test]
fn synthetic_light_initialization_and_commands_match_unmodified_native_receipts() {
    let receipts = receipts();
    assert_eq!(receipts.synthetic.len(), 72);
    let mut compared = 0;
    for case in &receipts.synthetic {
        if reaches_inclusive_boundary(&case.input) {
            continue; // Separately pinned below, with the same raw table count.
        }
        let fixture = Fixture::new(&case.input.h8);
        let mut arena = ObjectArena::new();
        let id = initialize(&mut arena, &fixture.runtime, &case.input);
        let known = case.input.position_words.is_some();
        assert_state(&arena, id, &case.after_init, known, &case.id);
        let commands = Commands::new(&case.input);
        // Native poison-after-rejection becomes a deliberately absent final
        // command here: ordered admission must return before trying to read it.
        let result = run_commands(
            &mut arena,
            &fixture.runtime,
            id,
            CommandPolicy::Constructor(19),
            case.input.resource_argument,
            (!case.input.null_suffix).then(|| commands.request()),
        )
        .unwrap();
        assert_eq!(result.is_none(), case.accepted, "{}", case.id);
        if let Some(rejected) = result {
            let (index, command) = case
                .input
                .commands
                .iter()
                .enumerate()
                .find(|(_, command)| !(0..=7).contains(&(command.code as i32)))
                .unwrap();
            assert_eq!(
                (rejected.index, rejected.code),
                (index as u32, command.code as i32),
                "{}",
                case.id
            );
        }
        assert_state(&arena, id, &case.after_commands, known, &case.id);
        compared += 1;
    }
    assert_eq!(compared, 71);
}

#[test]
fn raw_inclusive_h8_boundary_is_an_explicit_portable_command_error() {
    let receipts = receipts();
    let cases: Vec<_> = receipts
        .synthetic
        .iter()
        .filter(|case| reaches_inclusive_boundary(&case.input))
        .collect();
    assert_eq!(cases.len(), 1);
    let case = cases[0];
    assert!(case.accepted); // Native oracle intentionally backs its extra slot.
    let fixture = Fixture::new(&case.input.h8);
    let mut arena = ObjectArena::new();
    let id = initialize(&mut arena, &fixture.runtime, &case.input);
    let commands = Commands::new(&case.input);
    assert!(matches!(
        run_commands(
            &mut arena,
            &fixture.runtime,
            id,
            CommandPolicy::Constructor(19),
            case.input.resource_argument,
            Some(commands.request())
        ),
        Err(ObjectConstructionError::Object(
            ObjectError::ModelSlotBoundary {
                slot: 2,
                capacity: 2
            }
        ))
    ));
    // Admission publishes the resource before reaching the rejected lookup.
    let mut expected = case.after_init.clone();
    expected.resource_word = case.input.resource_argument as u16;
    assert_state(&arena, id, &expected, true, &case.id);
}

#[test]
fn canonical_world_32_factory_matches_all_42_native_light_states() {
    let receipts = receipts();
    assert_eq!(receipts.corpus.len(), 42);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 32] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let source = placements.world_source();
    let mut arena = ObjectArena::new();
    for case in &receipts.corpus {
        assert!(case.accepted);
        let input = placements
            .input(case.input.placement_index as usize)
            .unwrap();
        assert_eq!(
            input.resource_argument, case.input.resource_argument,
            "{}",
            case.id
        );
        assert_eq!(
            Some(input.position_words),
            case.input.position_words,
            "{}",
            case.id
        );
        let outcome = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                resource_argument: input.resource_argument,
                context: BaseContext {
                    position_words: Some(input.position_words),
                    placement_index: input.placement_index,
                },
                suffix: Some(ObjectInstanceCommands {
                    placement: input.instance,
                    source: source.overlay.bytes(),
                    main_payload: placements.assets().worlds.main_payload,
                    relocations: &placements.assets().worlds.relocations,
                }),
                command_allocation: None,
                network_active: false,
                actor: None,
            },
        )
        .unwrap();
        let ObjectConstructionOutcome::Constructed(id) = outcome else {
            panic!("{}", case.id)
        };
        let mut expected = case.after_commands.clone();
        // The oracle stops before 004602D0. Apply only its separately proven
        // common completion effects when comparing the full production factory.
        expected.dirty_word = 0;
        expected.flags |= INITIALIZED_FLAG;
        assert_state(&arena, id, &expected, true, &case.id);
    }
    assert_eq!(arena.len(), 42);
}
