use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use inf_formats::{InstanceCommand, InstanceField, HANDLER_COUNT};
use serde::Deserialize;

use super::*;
use crate::{
    object_runtime::{ObjectList, ObjectNode, VisualObject, INITIALIZED_FLAG},
    terrain_runtime::TerrainSpacing,
    world_placement::WorldPlacements,
    HandlerResources, Installation, ResidencyRequest,
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
const LISTS: [ObjectList; 6] = [
    ObjectList::Flag0Primary,
    ObjectList::Flag0Secondary,
    ObjectList::Flag1Primary,
    ObjectList::Flag1Secondary,
    ObjectList::Parking,
    ObjectList::Disposal,
];

/// No selector-1 H13 exists in the installed corpus. These isolated bytes go
/// through the real Installation/residency/parser path without altering OVLs.
struct Fixture {
    root: PathBuf,
    runtime: ResidentRuntime,
}

impl Fixture {
    fn new() -> Self {
        Self::with_model_word(u16::MAX)
    }

    fn with_model_word(model_word: u16) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-object-construction-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            preload.extend_from_slice(&(if handler == 13 { 2_u16 } else { 0 }).to_le_bytes());
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]); // allocation, root child, leaf tag/id
        for table in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let populated = table == 0 && handler == 13;
                preload.extend_from_slice(&(if populated { 2_i16 } else { 0 }).to_le_bytes());
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        let mut data = vec![];
        for (selector, flags) in [(1_u16, 0xa040_01fc_u32), (19, 0x0b)] {
            data.extend_from_slice(&(selector - 1).to_le_bytes());
            data.extend_from_slice(&model_word.to_le_bytes());
            data.extend_from_slice(&flags.to_le_bytes());
        }
        data.extend_from_slice(&[249, 13, 127, 0]); // signed RGB and draw gate
        data.extend_from_slice(&1200_u16.to_le_bytes());
        let mut overlay = (data.len() as u32).to_le_bytes().to_vec();
        overlay.extend_from_slice(&data);
        overlay.extend_from_slice(&0_u32.to_le_bytes()); // no relocations
        for offset in [0_u32, 8] {
            overlay.extend_from_slice(&offset.to_le_bytes());
        }
        fs::write(root.join("Overlay/0.ovl"), overlay).unwrap();
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        Self { root, runtime }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("inf-object-construction-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

pub(super) fn retail_runtime() -> ResidentRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 32] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    runtime
}

pub(super) fn resource_with_selector(runtime: &ResidentRuntime, selector: u16) -> u32 {
    for slot in 0..runtime.resource_capacity(13).unwrap() {
        let Some(resource) = runtime.resource(13, slot as u16) else {
            continue;
        };
        let region = &resource.overlay.layout().regions[resource.registration.region_index];
        let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
        if bank.prefixes.entries[usize::from(resource.registration.local_index)]
            .constructor_selector
            == selector
        {
            return slot as u32;
        }
    }
    panic!("test residency lacks required selector {selector}");
}

pub(super) struct Commands {
    placement: PlacementInstanceAssets,
    source: Vec<u8>,
}

impl Commands {
    pub(super) fn new(count: i32, inputs: &[(i32, &[u8])]) -> Self {
        let mut source = vec![];
        let commands = inputs
            .iter()
            .enumerate()
            .map(|(index, (code, payload))| {
                let offset = source.len();
                source.extend_from_slice(&code.to_le_bytes());
                source.extend_from_slice(payload);
                InstanceCommand {
                    index: index as u32,
                    pointer_span: ByteSpan::new(0, 0),
                    allocation_offset: offset as u32,
                    code: InstanceField {
                        span: ByteSpan::new(offset, offset + 4),
                        value: *code,
                    },
                    payload_offset: (offset + 4) as u32,
                }
            })
            .collect();
        Self {
            placement: PlacementInstanceAssets {
                placement_index: 0,
                suffix_offset: 0,
                count: InstanceField {
                    span: ByteSpan::new(0, 4),
                    value: count,
                },
                command_pointer_span: ByteSpan::new(0, 0),
                commands,
            },
            source,
        }
    }

    pub(super) fn codes(count: i32, codes: &[i32]) -> Self {
        let inputs: Vec<_> = codes.iter().map(|&code| (code, &[][..])).collect();
        Self::new(count, &inputs)
    }

    pub(super) fn request(&self) -> ObjectInstanceCommands<'_> {
        ObjectInstanceCommands {
            placement: &self.placement,
            source: &self.source,
            main_payload: ByteSpan::new(0, self.source.len()),
            relocations: &[],
        }
    }
}

pub(super) fn request<'a>(resource_argument: u32) -> ObjectConstructionRequest<'a> {
    ObjectConstructionRequest {
        resource_argument,
        context: BaseContext {
            position_words: Some([0xfffe, 0x8000, 1]),
            placement_index: 0xabcd_8000,
        },
        suffix: None,
        command_allocation: None,
        network_active: false,
        actor: None,
    }
}

pub(super) fn lists(arena: &ObjectArena) -> [Vec<ObjectNode>; 6] {
    LISTS.map(|list| arena.list(list).to_vec())
}

pub(super) fn constructed(
    result: Result<ObjectConstructionOutcome, ObjectConstructionError>,
) -> ObjectId {
    let ObjectConstructionOutcome::Constructed(id) = result.unwrap() else {
        panic!("expected successful exact-class construction");
    };
    id
}

#[test]
fn synthetic_resident_base_initializes_and_accepts_only_payload_free_base_commands() {
    let fixture = Fixture::new();
    let runtime = &fixture.runtime;
    let slot = resource_with_selector(runtime, 1);
    let resource = runtime.resource(13, slot as u16).unwrap();
    let original = resource.overlay.bytes().to_vec();
    let revision = runtime.revision();
    // Final code1 has no payload bytes in the allocation.
    let commands = Commands::codes(4, &[0, 1, 0, 1]);
    let mut arena = ObjectArena::new();
    for network_active in [false, true] {
        let id = constructed(construct_world_object(
            &mut arena,
            runtime,
            ObjectConstructionRequest {
                resource_argument: 0xabcd_0000 | slot,
                suffix: Some(commands.request()),
                network_active,
                ..request(slot)
            },
        ));
        let object = arena.object(id).unwrap();
        assert_eq!(object.class, ObjectClass::Base);
        assert_eq!(object.base.flags, 0xa840_01fc | INITIALIZED_FLAG);
        assert_eq!(object.base.resource_word, slot as u16);
        assert_eq!(object.base.placement_word, 0x8000);
        assert_eq!(object.base.position_words, Some([0xfffe, 0x8000, 1]));
        assert_eq!(
            (
                object.base.word_28,
                object.base.word_3c,
                object.base.dirty_word
            ),
            (0, 0, 0)
        );
    }
    assert_eq!(arena.len(), 2);
    assert_eq!(runtime.revision(), revision);
    assert_eq!(resource.overlay.bytes(), original);
}

#[test]
fn rejection_stops_before_missing_or_truncated_tail_and_preserves_neighbors() {
    let fixture = Fixture::new();
    let runtime = &fixture.runtime;
    let mut arena = ObjectArena::new();
    let first = arena.allocate(ObjectClassKind::Base);
    let second = arena.allocate(ObjectClassKind::Base);
    let before = lists(&arena);
    for (slot, selector, commands) in [
        (0, 1, Commands::codes(4, &[0, 2])),
        (1, 19, Commands::new(4, &[(3, &[128]), (8, &[])])),
        (1, 19, Commands::new(3, &[(3, &[128]), (8, &[]), (7, &[])])),
    ] {
        assert_eq!(resource_with_selector(runtime, selector), slot);
        let outcome = construct_world_object(
            &mut arena,
            runtime,
            ObjectConstructionRequest {
                suffix: Some(commands.request()),
                ..request(slot)
            },
        )
        .unwrap();
        assert_eq!(
            outcome,
            ObjectConstructionOutcome::Rejected {
                command_index: 1,
                code: if selector == 1 { 2 } else { 8 }
            }
        );
        assert_eq!(arena.len(), 2);
        assert!(arena.object(first).is_ok());
        assert!(arena.object(second).is_ok());
        assert_eq!(lists(&arena), before);
    }
}

#[test]
fn absent_and_nonpositive_suffixes_skip_commands_without_inventing_position() {
    let fixture = Fixture::new();
    let mut arena = ObjectArena::new();
    let nonpositive = [0, -1, i32::MIN].map(|count| Commands::codes(count, &[]));
    for suffix in
        std::iter::once(None).chain(nonpositive.iter().map(|commands| Some(commands.request())))
    {
        let id = constructed(construct_world_object(
            &mut arena,
            &fixture.runtime,
            ObjectConstructionRequest {
                context: BaseContext {
                    position_words: None,
                    placement_index: u32::MAX,
                },
                suffix,
                ..request(0)
            },
        ));
        let object = &arena.object(id).unwrap().base;
        assert_eq!(object.position_words, None);
        assert_eq!(object.placement_word, 0xffff);
        assert_ne!(object.flags & INITIALIZED_FLAG, 0);
        assert_eq!(object.dirty_word, 0);
    }
    assert_eq!(arena.len(), 4);
}

#[test]
fn resource_failures_do_not_allocate_or_claim_lazy_loading() {
    let fixture = Fixture::new();
    let runtime = &fixture.runtime;
    let mut arena = ObjectArena::new();
    assert!(matches!(
        construct_world_object(&mut arena, runtime, request(0xabcd_ffff)),
        Err(ObjectConstructionError::NegativeResourceArgument {
            resource_argument: 0xabcd_ffff,
            signed_slot: -1
        })
    ));
    assert!(matches!(
        construct_world_object(&mut arena, runtime, request(0xabcd_7fff)),
        Err(ObjectConstructionError::ResourceOutOfRange {
            resource_argument: 0xabcd_7fff,
            ..
        })
    ));
    assert!(matches!(
        construct_world_object(&mut arena, runtime, request(0xabcd_0002)),
        Err(ObjectConstructionError::ResourceOutOfRange {
            resource_argument: 0xabcd_0002,
            capacity: 2
        })
    ));
    let unbooted = ResidentRuntime::new(Installation::open(&fixture.root).unwrap()).unwrap();
    assert!(matches!(
        construct_world_object(&mut arena, &unbooted, request(0xabcd_0000)),
        Err(ObjectConstructionError::NotResident {
            resource_argument: 0xabcd_0000
        })
    ));
    assert!(arena.is_empty());
}

#[test]
fn post_publication_structural_errors_destroy_only_the_new_object() {
    let fixture = Fixture::new();
    let runtime = &fixture.runtime;
    let mut arena = ObjectArena::new();
    arena.allocate(ObjectClassKind::Base);
    arena.allocate(ObjectClassKind::Base);
    let before = lists(&arena);
    let missing = Commands::codes(2, &[0]);
    assert!(matches!(
        construct_world_object(
            &mut arena,
            runtime,
            ObjectConstructionRequest {
                suffix: Some(missing.request()),
                ..request(0)
            }
        ),
        Err(ObjectConstructionError::MissingCommands {
            count: 2,
            available: 1
        })
    ));
    let truncated = Commands::new(2, &[(3, &[128]), (7, &[1])]);
    assert!(matches!(
        construct_world_object(
            &mut arena,
            runtime,
            ObjectConstructionRequest {
                suffix: Some(truncated.request()),
                ..request(1)
            }
        ),
        Err(ObjectConstructionError::Instance(_))
    ));
    // The synthetic fixture's H8 allocation has zero slots: index0 is exactly
    // the native one-past boundary, reported safely by visual metadata lookup.
    let invalid_model = Commands::new(1, &[(2, &[0, 0])]);
    assert!(matches!(
        construct_world_object(
            &mut arena,
            runtime,
            ObjectConstructionRequest {
                suffix: Some(invalid_model.request()),
                ..request(1)
            }
        ),
        Err(ObjectConstructionError::Object(
            ObjectError::ModelSlotBoundary {
                slot: 0,
                capacity: 0
            }
        ))
    ));
    assert_eq!(arena.len(), 2);
    assert_eq!(lists(&arena), before);

    // The same cleanup applies when context initialization fails before the
    // payload or command walk, not only to failures within instance commands.
    let invalid_initial_model = Fixture::with_model_word(0);
    assert!(matches!(
        construct_world_object(&mut arena, &invalid_initial_model.runtime, request(1)),
        Err(ObjectConstructionError::Object(
            ObjectError::ModelSlotBoundary {
                slot: 0,
                capacity: 0
            }
        ))
    ));
    assert_eq!(arena.len(), 2);
    assert_eq!(lists(&arena), before);
}

#[test]
fn terrain_light_construction_executes_scalar_commands_but_not_later_passes() {
    let fixture = Fixture::new();
    let commands = Commands::new(
        8,
        &[
            (0, &[]),
            (1, &[]),
            (2, &[254, 255]),
            (3, &[128]),
            (4, &[255]),
            (5, &[127]),
            (6, &[255]),
            (7, &[255, 255]),
        ],
    );
    let mut arena = ObjectArena::new();
    let id = constructed(construct_world_object(
        &mut arena,
        &fixture.runtime,
        ObjectConstructionRequest {
            suffix: Some(commands.request()),
            command_allocation: None,
            network_active: true,
            actor: None,
            ..request(0xabcd_0001)
        },
    ));
    let object = arena.object(id).unwrap();
    assert_eq!(
        object.class,
        ObjectClass::TerrainLight {
            visual: VisualObject {
                model_word: 0xfffe,
                radius_word: 0
            },
            light: Some(TerrainLight {
                channels: [-128, -1, 127],
                draw_gate: 255,
                radius: 65535
            }),
        }
    );
    assert_eq!(object.base.word_28, 256);
    assert_eq!(object.base.resource_word, 1);
    assert_eq!(object.base.flags, 0x0810_000b);
    assert_eq!(object.base.dirty_word, 0);
    // Construction does not invoke 00405140's update or 00401320's later
    // initialization, and no terrain-light grid or renderer is supplied here.
    assert_eq!(arena.list(ObjectList::Flag1Primary).len(), 1);
    assert_eq!(object.base.flags & 0x0080_0000, 0);
}

#[test]
fn actual_unsupported_actor_is_not_substituted_with_an_implemented_class() {
    let runtime = retail_runtime();
    let slot = resource_with_selector(&runtime, 13);
    let mut arena = ObjectArena::new();
    assert!(matches!(
        construct_world_object(&mut arena, &runtime, request(slot)),
        Err(ObjectConstructionError::UnsupportedSelector { selector: 13, .. })
    ));
    assert!(arena.is_empty());
}

#[test]
fn all_42_first_world_terrain_lights_construct_from_current_registrations() {
    let runtime = retail_runtime();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let slot = resource_with_selector(&runtime, 19);
    assert_eq!(slot, 53);
    let h13 = runtime.resource(13, slot as u16).unwrap();
    let h13_before = h13.overlay.bytes().to_vec();
    let h14 = placements.world_source();
    let h14_before = h14.overlay.bytes().to_vec();
    let revision = runtime.revision();
    let model = runtime.resource(8, 9).unwrap();
    let HandlerResources::Models(models) =
        &model.overlay.handlers()[model.registration.region_index]
    else {
        panic!("H8");
    };
    let model = &models.entries[usize::from(model.registration.local_index)];
    let mut arena = ObjectArena::new();
    let mut count = 0;
    let mut negative_channels = 0;
    for input in placements
        .inputs()
        .filter(|input| input.resource_argument as u16 == slot as u16)
    {
        assert_eq!(input.resource_argument, 0xffff_0035);
        let mut expected = TerrainLight {
            channels: [0; 3],
            draw_gate: 0,
            radius: 1024,
        };
        // Read independent expected scalar bytes at each canonical source span.
        for command in &input.instance.commands {
            let at =
                placements.assets().worlds.main_payload.start + command.payload_offset as usize;
            let bytes = h14.overlay.bytes();
            match command.code.value {
                1 => {}
                3..=5 => expected.channels[(command.code.value - 3) as usize] = bytes[at] as i8,
                7 => expected.radius = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
                code => panic!("unexpected first-world terrain-light command {code}"),
            }
        }
        let id = constructed(construct_world_object(
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
                    source: h14.overlay.bytes(),
                    main_payload: placements.assets().worlds.main_payload,
                    relocations: &placements.assets().worlds.relocations,
                }),
                command_allocation: None,
                network_active: false,
                actor: None,
            },
        ));
        let object = arena.object(id).unwrap();
        assert_eq!(
            object.class,
            ObjectClass::TerrainLight {
                visual: VisualObject {
                    model_word: 9,
                    radius_word: model.header.raw_0e
                },
                light: Some(expected),
            }
        );
        assert_eq!(object.base.flags, 0x0810_000b);
        assert_eq!(object.base.position_words, Some(input.position_words));
        assert_eq!(object.base.placement_word, input.placement_index as u16);
        assert_eq!(object.base.resource_word, 53);
        let expected_extent = model.header.visibility_extent as u16;
        assert_eq!(
            object.base.word_28,
            if expected_extent == 0 {
                256
            } else {
                expected_extent
            }
        );
        assert_eq!(object.base.dirty_word, 0);
        negative_channels += usize::from(expected.channels.iter().any(|&value| value < 0));
        count += 1;
    }
    assert_eq!(count, 42);
    assert_eq!(negative_channels, 13);
    assert_eq!(arena.len(), 42);
    for role in [
        ObjectList::Flag0Primary,
        ObjectList::Flag1Primary,
        ObjectList::Parking,
    ] {
        assert_eq!(arena.list(role).len(), 42);
    }
    assert_eq!(runtime.revision(), revision);
    assert_eq!(h13.overlay.bytes(), h13_before);
    assert_eq!(h14.overlay.bytes(), h14_before);
    // This checks constructor completion only. It does not execute the world
    // loader's later +20 pass, update, light accumulation or manager drawing.
}

#[test]
fn base_resource_and_command_phase_matches_unmodified_native_receipt() {
    #[derive(Deserialize)]
    struct Receipt {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        id: String,
        resource_argument: u32,
        resource_before: u16,
        suffix: Option<Suffix>,
        accepted: bool,
        resource_after: u16,
    }
    #[derive(Deserialize)]
    struct Suffix {
        count: i32,
        commands: Vec<u32>,
        poison_after_commands: bool,
    }
    let fixture = Fixture::new();
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../tests/fixtures/object_base_command_native.json"
    ))
    .unwrap();
    assert_eq!(receipt.cases.len(), 45);
    for case in receipt.cases {
        let codes: Vec<_> = case
            .suffix
            .as_ref()
            .into_iter()
            .flat_map(|suffix| &suffix.commands)
            .map(|&code| code as i32)
            .collect();
        if let Some(suffix) = &case.suffix {
            assert_eq!(
                suffix.count.max(0) as usize,
                codes.len() + usize::from(suffix.poison_after_commands)
            );
        }
        // Keep the native count, including any unmapped tail. Rejection must
        // happen before the portable walk asks for the absent later entry.
        let commands = Commands::codes(case.suffix.as_ref().map_or(0, |s| s.count), &codes);
        let mut arena = ObjectArena::new();
        let id = arena.allocate(ObjectClassKind::Base);
        arena.owned_mut(id).unwrap().state.base.resource_word = case.resource_before;
        let mut expected = arena.object(id).unwrap().clone();
        expected.base.resource_word = case.resource_after;
        let before_lists = lists(&arena);
        let result = run_commands(
            &mut arena,
            &fixture.runtime,
            id,
            CommandPolicy::Constructor(1),
            case.resource_argument,
            case.suffix.as_ref().map(|_| commands.request()),
        )
        .unwrap();
        assert_eq!(result.is_none(), case.accepted, "{}", case.id);
        if let Some(index) = codes.iter().position(|code| !matches!(code, 0 | 1)) {
            assert_eq!(
                result,
                Some(ObjectCommandRejection {
                    index: index as u32,
                    code: codes[index]
                }),
                "{}",
                case.id
            );
        }
        assert_eq!(arena.object(id).unwrap(), &expected, "{}", case.id);
        assert_eq!(lists(&arena), before_lists, "{}", case.id);
    }
}

/// Every family-428650 table binds +38 to 004270E0, which copies the home
/// position +222 when the actor is admitted. Oolbryte's supply truck
/// (selector 9, the family's own class, placement 98 at (37202, 20987))
/// and its supply ship (selector 35) take their homes where they stand, so
/// the home-distance score (00425DB0) no longer meets an unwritten +224;
/// so do world 15's four Nemians (selector 21).
#[test]
fn family_actors_take_their_homes_at_admission() {
    let (_runtime, _assets, world) =
        crate::test_worlds::offline_world_retained(15, TerrainSpacing::Units256);
    let arena = &world.arena;
    let mut nemians = 0;
    for &actor in arena.actor_list() {
        if let Ok(family) = arena.family428650_actor(actor) {
            assert!(family.position_222.is_some(), "world 15 {actor:?}");
            if matches!(
                arena.family4236d0_actor(actor).map(|state| &state.class),
                Ok(crate::object_runtime::Family4236d0Class::Selector10(state)) if state.swing.is_some()
            ) {
                nemians += 1;
            }
        }
    }
    assert_eq!(nemians, 4);

    let (_runtime, _assets, world) =
        crate::test_worlds::offline_world_retained(20, TerrainSpacing::Units256);
    let arena = &world.arena;
    let mut truck = None;
    let mut homes = 0;
    for &actor in arena.actor_list() {
        let Ok(family) = arena.family428650_actor(actor) else {
            continue;
        };
        let position = arena.object(actor).unwrap().base.position_words;
        if position.map(|p| [p[0], p[2]]) == Some([37202, 20987]) {
            truck = Some(actor);
        }
        assert!(family.position_222.is_some(), "{actor:?} at {position:?}");
        homes += 1;
    }
    let truck = truck.expect("Oolbryte's supply truck");
    assert_eq!(
        arena.family428650_actor(truck).unwrap().position_222,
        arena.object(truck).unwrap().base.position_words
    );
    assert!(homes > 1, "{homes}");
}
