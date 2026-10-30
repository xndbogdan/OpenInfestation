use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::object_runtime::{ObjectConstructionError, RuntimeAllocationError};
use crate::{Installation, ResidencyRequest, ResidentRuntime, ResourceSelection};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    runtime: ResidentRuntime,
}
impl Fixture {
    /// A selector-86 header sits inside an actor payload; its interval write
    /// lands beyond that nested header, outside the actor's conservative range.
    fn with_specialized_overlap(selector: u16, write_offset: usize) -> Self {
        let mut fixture = Self::new(false);
        let payload_len = match selector {
            30 => 0x58,
            35 => 0x74,
            _ => panic!("expected specialized actor"),
        };
        let mut data = vec![0; 4 + payload_len];
        data[..2].copy_from_slice(&(selector - 1).to_le_bytes());
        data[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
        let nested = write_offset - 14;
        data[nested..nested + 2].copy_from_slice(&85_u16.to_le_bytes());
        data[nested + 4..nested + 8].copy_from_slice(&3_u32.to_le_bytes());
        data[nested + 10..nested + 12].copy_from_slice(&3_i16.to_le_bytes());
        data[write_offset..write_offset + 2].copy_from_slice(&0xabcd_u16.to_le_bytes());
        let mut overlay = (data.len() as u32).to_le_bytes().to_vec();
        overlay.extend_from_slice(&data);
        overlay.extend_from_slice(&0_u32.to_le_bytes());
        for offset in [0, nested as u32] {
            overlay.extend_from_slice(&offset.to_le_bytes());
        }
        for variant in 0..2 {
            fs::write(
                fixture.root.join(format!("Overlay/0d{variant}.ovl")),
                &overlay,
            )
            .unwrap();
        }
        fixture.runtime = ResidentRuntime::new(Installation::open(&fixture.root).unwrap()).unwrap();
        fixture.runtime.apply(ResidencyRequest::Boot).unwrap();
        fixture
    }

    fn new(nested_actor: bool) -> Self {
        Self::with_layout(nested_actor, false)
    }

    fn with_layout(nested_actor: bool, header_overlap: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-selector86-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..inf_formats::HANDLER_COUNT {
            preload.extend_from_slice(&(if handler == 13 { 2_u16 } else { 0 }).to_le_bytes());
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for table in 0..4 {
            for handler in 0..inf_formats::HANDLER_COUNT {
                let populated = table == 1 && handler == 13;
                preload.extend_from_slice(&(if populated { 2_i16 } else { 0 }).to_le_bytes());
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        for variant in 0..2 {
            let mut data = vec![
                0;
                if nested_actor {
                    0x46
                } else if header_overlap {
                    30
                } else {
                    16
                }
            ];
            let at = if nested_actor {
                0x20
            } else if header_overlap {
                14
            } else {
                0
            };
            if header_overlap {
                data[..2].copy_from_slice(&85_u16.to_le_bytes());
                data[4..8].copy_from_slice(&3_u32.to_le_bytes());
                // Native write 60000 / 10000 = 6 would change the next
                // stored tag from 85 (selector 86) to 6 (selector 7).
                data[10..12].copy_from_slice(&10000_i16.to_le_bytes());
            }
            if nested_actor {
                data[..2].copy_from_slice(&87_u16.to_le_bytes());
                data[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
            }
            data[at..at + 2].copy_from_slice(&85_u16.to_le_bytes());
            data[at + 2..at + 4].copy_from_slice(&0x1234_u16.to_le_bytes());
            data[at + 4..at + 8].copy_from_slice(&3_u32.to_le_bytes());
            data[at + 8..at + 10].copy_from_slice(&500_u16.to_le_bytes());
            data[at + 10..at + 12].copy_from_slice(&(3_i16 + variant).to_le_bytes());
            data[at + 12..at + 14].copy_from_slice(&2_u16.to_le_bytes());
            data[at + 14..at + 16].copy_from_slice(&0xabcd_u16.to_le_bytes());
            let mut overlay = (data.len() as u32).to_le_bytes().to_vec();
            overlay.extend_from_slice(&data);
            overlay.extend_from_slice(&0_u32.to_le_bytes());
            for offset in [0, at as u32] {
                overlay.extend_from_slice(&offset.to_le_bytes());
            }
            fs::write(root.join(format!("Overlay/0d{variant}.ovl")), overlay).unwrap();
        }
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
            .starts_with("inf-selector86-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn request<'a>(slot: u32) -> ObjectConstructionRequest<'a> {
    ObjectConstructionRequest {
        resource_argument: slot,
        context: BaseContext {
            position_words: None,
            placement_index: 0,
        },
        suffix: None,
        command_allocation: None,
        network_active: false,
        actor: None,
    }
}
fn constructed(arena: &mut ObjectArena, runtime: &ResidentRuntime, slot: u32) -> ObjectId {
    let ObjectConstructionOutcome::Constructed(id) =
        construct_world_object(arena, runtime, request(slot)).unwrap()
    else {
        panic!("expected emitter")
    };
    id
}

#[test]
fn constructor_aliases_share_mutations_across_last_object_deletion_and_reload_replaces_identity() {
    let mut fixture = Fixture::new(false);
    let mut arena = ObjectArena::new();
    let first = constructed(&mut arena, &fixture.runtime, 0);
    let second = constructed(&mut arena, &fixture.runtime, 1);
    let first_data = arena
        .selector86(first)
        .unwrap()
        .data_40
        .as_ref()
        .unwrap()
        .clone();
    let second_data = arena.selector86(second).unwrap().data_40.as_ref().unwrap();
    assert!(Arc::ptr_eq(
        first_data.allocation(),
        second_data.allocation()
    ));
    assert_eq!(first_data.words(), [500, 3, 2, 20000]);
    let weak = Arc::downgrade(first_data.allocation());
    let original = fixture
        .runtime
        .resource(13, 0)
        .unwrap()
        .overlay
        .bytes()
        .to_vec();
    assert_eq!(
        u16::from_le_bytes(original[18..20].try_into().unwrap()),
        0xabcd
    );
    drop(first_data);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(
        weak.upgrade().is_some(),
        "resident allocation owns mutation even without objects"
    );
    let third = constructed(&mut arena, &fixture.runtime, 1);
    assert!(Arc::ptr_eq(
        &weak.upgrade().unwrap(),
        arena
            .selector86(third)
            .unwrap()
            .data_40
            .as_ref()
            .unwrap()
            .allocation()
    ));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: third,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    fixture
        .runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 1,
                language: 0,
            },
        })
        .unwrap();
    let replacement = constructed(&mut arena, &fixture.runtime, 0);
    assert!(
        weak.upgrade().is_none(),
        "unloaded unretained allocation evicted on next lookup"
    );
    assert_eq!(
        arena
            .selector86(replacement)
            .unwrap()
            .data_40
            .as_ref()
            .unwrap()
            .words()[3],
        15000
    );
}

#[test]
fn nested_selector86_cannot_silently_modify_a_retained_actor_damage_table() {
    let fixture = Fixture::new(true);
    let mut arena = ObjectArena::new();
    let source = fixture
        .runtime
        .resource(13, 1)
        .unwrap()
        .overlay
        .bytes()
        .to_vec();
    assert!(matches!(
        construct_world_object(&mut arena, &fixture.runtime, request(1)),
        Err(ObjectConstructionError::RuntimeAllocation(
            RuntimeAllocationError::ImmutableConstructorWrite { offset: 0x2e }
        ))
    ));
    assert!(arena.is_empty());
    assert_eq!(
        fixture.runtime.resource(13, 1).unwrap().overlay.bytes(),
        source
    );
}

#[test]
fn nested_selector86_respects_specialized_actor_reads_but_can_write_unread_gaps() {
    let cases = [
        (
            30,
            vec![0x48, 0x4a, 0x4c, 0x4e, 0x50, 0x54, 0x56, 0x58, 0x5a],
            vec![0x52],
        ),
        (35, vec![0x70, 0x72, 0x74, 0x76], vec![0x5c, 0x62, 0x6e]),
    ];
    for (selector, protected, gaps) in cases {
        for (write_offset, rejected) in protected
            .into_iter()
            .map(|at| (at, true))
            .chain(gaps.into_iter().map(|at| (at, false)))
        {
            let fixture = Fixture::with_specialized_overlap(selector, write_offset);
            let resource = fixture.runtime.resource(13, 1).unwrap();
            let region = &resource.overlay.layout().regions[resource.registration.region_index];
            let bank = inf_formats::parse_constructor_asset_bank(resource.overlay.bytes(), region)
                .unwrap();
            let mut arena = ObjectArena::new();
            let allocation = arena
                .constructor_allocations
                .retain(resource.overlay, resource.registration.region_index, &bank)
                .unwrap();
            let before = allocation.bytes();
            let outcome = construct_world_object(&mut arena, &fixture.runtime, request(1));
            if rejected {
                assert!(
                    matches!(outcome, Err(ObjectConstructionError::RuntimeAllocation(
                    RuntimeAllocationError::ImmutableConstructorWrite { offset }
                )) if offset == write_offset as u32),
                    "selector {selector}, offset {write_offset:#x}: {outcome:?}"
                );
                assert!(arena.is_empty());
                assert_eq!(allocation.bytes(), before);
            } else {
                let ObjectConstructionOutcome::Constructed(object) = outcome.unwrap() else {
                    panic!("unread bytes must remain mutable")
                };
                let mut expected = before;
                expected[write_offset..write_offset + 2].copy_from_slice(&20000_u16.to_le_bytes());
                assert_eq!(
                    allocation.bytes(),
                    expected,
                    "selector {selector}, offset {write_offset:#x}"
                );
                arena
                    .destroy(crate::object_runtime::ObjectDestructionRequest {
                        object: object,
                        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    })
                    .unwrap();
            }
        }
    }
}

#[test]
fn selector86_cannot_change_another_registered_dispatch_header() {
    let fixture = Fixture::with_layout(false, true);
    let mut arena = ObjectArena::new();
    assert!(matches!(
        construct_world_object(&mut arena, &fixture.runtime, request(0)),
        Err(ObjectConstructionError::RuntimeAllocation(
            RuntimeAllocationError::ConstructorHeaderWrite {
                offset: 14,
                header_offset: 14
            }
        ))
    ));
    assert!(arena.is_empty());
    let second = constructed(&mut arena, &fixture.runtime, 1);
    assert_eq!(
        arena
            .selector86(second)
            .unwrap()
            .data_40
            .as_ref()
            .unwrap()
            .words()[3],
        20000
    );
}

#[test]
fn invalid_write_ranges_and_relocated_pointer_overlaps_leave_existing_state_unchanged() {
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Selector86);
    let before = arena.object(object).unwrap().clone();
    let short =
        Arc::new(RuntimeAllocation::new(Arc::from([0_u8; 7]), ByteSpan::new(0, 7)).unwrap());
    assert!(arena.set_selector86_data(object, &short, 0).is_err());
    let relocation = AssetRelocation {
        table_span: ByteSpan::new(0, 0),
        field_offset: 6,
        field_span: ByteSpan::new(6, 10),
        target_offset: 0,
    };
    let owned = InstanceCommandAllocation::new(
        Arc::from([0_u8; 12]),
        ByteSpan::new(0, 12),
        Arc::from([relocation]),
    )
    .unwrap();
    assert!(matches!(
        arena.set_selector86_data(object, owned.runtime_allocation(), 0),
        Err(ObjectConstructionError::RuntimeAllocation(
            RuntimeAllocationError::RelocatedWrite {
                offset: 6,
                field_offset: 6
            }
        ))
    ));
    assert_eq!(arena.object(object).unwrap(), &before);
    assert_eq!(owned.runtime_allocation().bytes(), [0; 12]);
}

#[test]
fn owned_walk_rereads_shortened_count_before_unmapped_later_command() {
    let runtime = retail_runtime();
    let mut source = vec![0; 42];
    source[16..20].copy_from_slice(&2_i32.to_le_bytes());
    source[22..24].copy_from_slice(&32767_i16.to_le_bytes());
    source[26..30].copy_from_slice(&2_i32.to_le_bytes());
    source[30..34].copy_from_slice(&16_u32.to_le_bytes());
    source[34..38].copy_from_slice(&0xffff_ffff_u32.to_le_bytes());
    let relocations: Vec<_> = [(30, 16), (34, u32::MAX)]
        .into_iter()
        .map(|(field, target)| AssetRelocation {
            table_span: ByteSpan::new(0, 0),
            field_offset: field,
            field_span: ByteSpan::new(field as usize, field as usize + 4),
            target_offset: target,
        })
        .collect();
    let owner = Arc::new(
        InstanceCommandAllocation::new(source.into(), ByteSpan::new(0, 42), relocations.into())
            .unwrap(),
    );
    let placement = PlacementInstanceAssets {
        placement_index: 0,
        suffix_offset: 26,
        count: InstanceField {
            span: ByteSpan::new(26, 30),
            value: 2,
        },
        command_pointer_span: ByteSpan::new(30, 38),
        commands: vec![],
    };
    let mut arena = ObjectArena::new();
    let object = arena.allocate(ObjectClassKind::Selector86);
    assert_eq!(
        crate::object_runtime::construction::run_commands(
            &mut arena,
            &runtime,
            object,
            crate::object_runtime::construction::CommandPolicy::OwnedConstructor {
                selector: 86,
                allocation: &owner
            },
            0,
            Some(owner.commands(&placement))
        )
        .unwrap(),
        None
    );
    assert_eq!(owner.runtime_allocation().dword(26).unwrap(), 1);
}

#[test]
fn rejected_factory_destroys_object_but_keeps_reached_shared_data_write() {
    let fixture = Fixture::new(false);
    let mut source = vec![0; 28];
    source[0..4].copy_from_slice(&2_i32.to_le_bytes());
    source[4..8].copy_from_slice(&12_u32.to_le_bytes());
    source[8..12].copy_from_slice(&24_u32.to_le_bytes());
    source[12..16].copy_from_slice(&2_i32.to_le_bytes());
    source[18..20].copy_from_slice(&3_i16.to_le_bytes());
    source[24..28].copy_from_slice(&3_i32.to_le_bytes());
    let relocations: Vec<_> = [(4, 12), (8, 24)]
        .into_iter()
        .map(|(field, target)| AssetRelocation {
            table_span: ByteSpan::new(0, 0),
            field_offset: field,
            field_span: ByteSpan::new(field as usize, field as usize + 4),
            target_offset: target,
        })
        .collect();
    let owner = Arc::new(
        InstanceCommandAllocation::new(source.into(), ByteSpan::new(0, 28), relocations.into())
            .unwrap(),
    );
    let placement = PlacementInstanceAssets {
        placement_index: 0,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(0, 4),
            value: 2,
        },
        command_pointer_span: ByteSpan::new(4, 12),
        commands: vec![],
    };
    let mut arena = ObjectArena::new();
    let outcome = construct_world_object(
        &mut arena,
        &fixture.runtime,
        ObjectConstructionRequest {
            suffix: Some(owner.commands(&placement)),
            command_allocation: Some(&owner),
            ..request(0)
        },
    )
    .unwrap();
    assert_eq!(
        outcome,
        ObjectConstructionOutcome::Rejected {
            command_index: 1,
            code: 3
        }
    );
    assert!(arena.is_empty());
    assert_eq!(owner.runtime_allocation().word(22).unwrap(), 20000);
    assert_eq!(&owner.source()[22..24], &[0, 0]);
}
