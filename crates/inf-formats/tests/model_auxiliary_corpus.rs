use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use inf_formats::{
    parse_model_bank, parse_overlay, ByteSpan, ModelAuxiliaryInstructionKind, ModelEntry,
    OverlayKey, Preload,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

fn visit_models(mut visit: impl FnMut(&[u8], &ModelEntry, &OverlayKey)) {
    let root = root();
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let mut paths = fs::read_dir(root.join("Overlay"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let key = OverlayKey::parse_filename(path.file_name().unwrap().to_str().unwrap()).unwrap();
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let request = requests.get(8).unwrap();
        if request.count == 0 {
            continue;
        }
        let bytes = fs::read(path).unwrap();
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 8)
            .unwrap();
        let bank = parse_model_bank(&bytes, region, request, &key).unwrap();
        for entry in &bank.entries {
            visit(&bytes, entry, &key);
        }
    }
}

#[test]
fn all_retail_auxiliary_streams_fit_their_declared_region_and_native_grammar() {
    let mut identities = BTreeSet::new();
    let mut raw_patterns = BTreeSet::new();
    let mut physical_entries = 0;
    let mut physical_streams = 0;
    let mut unique_streams = 0;
    let mut physical_bytes = 0;
    let mut unique_bytes = 0;
    let mut physical_opcodes = BTreeMap::new();
    let mut unique_opcodes = BTreeMap::new();
    let mut physical_padding = BTreeMap::new();
    let mut unique_padding = BTreeMap::new();
    let mut children = BTreeMap::new();
    visit_models(|bytes, entry, key| {
        physical_entries += 1;
        let fresh = identities.insert((
            entry.identity.global_index,
            entry.content_span.get(bytes).unwrap().to_vec(),
        ));
        let auxiliary = entry.auxiliary_span.get(bytes).unwrap();
        raw_patterns.insert(auxiliary.to_vec());
        assert_eq!(
            auxiliary.len(),
            usize::from(entry.header.auxiliary_count) * 4
        );
        let preserved: Vec<_> = entry
            .auxiliary
            .iter()
            .flat_map(|record| record.words.iter().flat_map(|word| word.to_le_bytes()))
            .collect();
        assert_eq!(preserved, auxiliary, "raw auxiliary bytes must survive");
        if entry.header.raw_0e == 0 {
            assert!(entry.auxiliary_program.is_none());
            assert!(auxiliary.is_empty(), "retail disabled stream: {key:?}");
            return;
        }
        let program = entry.auxiliary_program.as_ref().unwrap();
        physical_streams += 1;
        physical_bytes += auxiliary.len();
        unique_streams += usize::from(fresh);
        unique_bytes += if fresh { auxiliary.len() } else { 0 };
        assert_eq!(program.span.start, entry.auxiliary_span.start);
        assert_eq!(program.span.end, program.terminator.end);
        assert_eq!(program.terminator.get(bytes).unwrap(), [0x88]);
        assert_eq!(program.padding.start, program.terminator.end);
        assert_eq!(program.padding.end, entry.auxiliary_span.end);
        let padding = program.padding.get(bytes).unwrap();
        assert!(padding.len() <= 3 && padding.iter().all(|&byte| byte == 0));
        *physical_padding.entry(padding.len()).or_insert(0) += 1;
        if fresh {
            *unique_padding.entry(padding.len()).or_insert(0) += 1;
        }
        let starts: BTreeSet<_> = program
            .instructions
            .iter()
            .map(|instruction| instruction.span.start)
            .chain([program.terminator.start])
            .collect();
        let mut cursor = program.span.start;
        for instruction in &program.instructions {
            assert_eq!(instruction.span.start, cursor);
            assert!(instruction.span.end > cursor);
            assert!(instruction.span.end <= program.terminator.start);
            assert_eq!(bytes[cursor], instruction.opcode);
            assert!(instruction.arguments_span.start > cursor);
            assert!(instruction.arguments_span.end <= instruction.span.end);
            *physical_opcodes.entry(instruction.opcode).or_insert(0) += 1;
            if fresh {
                *unique_opcodes.entry(instruction.opcode).or_insert(0) += 1;
            }
            match &instruction.kind {
                ModelAuxiliaryInstructionKind::BranchUnlessHit {
                    displacement_bytes,
                    target,
                }
                | ModelAuxiliaryInstructionKind::GroupBegin {
                    displacement_bytes,
                    target,
                } => {
                    assert!(*displacement_bytes > 0, "retail forward branch");
                    assert!(starts.contains(target), "branch into operands/padding");
                    assert_eq!(
                        *target,
                        instruction.arguments_span.start + *displacement_bytes as usize
                    );
                }
                ModelAuxiliaryInstructionKind::ChildModel {
                    model,
                    orientation,
                    argument_bytes,
                    state_words,
                } => {
                    assert!(*argument_bytes >= 8);
                    assert_eq!(orientation.raw(), 6);
                    assert_eq!(
                        instruction.span.end,
                        instruction.arguments_span.start + *argument_bytes as usize
                    );
                    if fresh {
                        assert!(children
                            .insert(
                                entry.identity.global_index,
                                (model.raw(), *argument_bytes, state_words.clone())
                            )
                            .is_none());
                    }
                }
                _ => {}
            }
            cursor = instruction.span.end;
        }
        assert_eq!(cursor, program.terminator.start);
    });
    assert_eq!(physical_entries, 5142);
    assert_eq!(identities.len(), 849);
    assert_eq!(raw_patterns.len(), 229); // Includes the empty pattern.
    assert_eq!((physical_streams, unique_streams), (2526, 354));
    assert_eq!((physical_bytes, unique_bytes), (128628, 18180));
    assert_eq!(
        physical_opcodes,
        BTreeMap::from([
            (0x05, 6),
            (0x07, 9250),
            (0x18, 242),
            (0x2f, 10),
            (0x30, 10),
            (0x89, 277),
            (0x8c, 50),
            (0x8d, 50),
            (0x8e, 7404),
            (0x8f, 436),
            (0x95, 2518),
        ])
    );
    assert_eq!(
        unique_opcodes,
        BTreeMap::from([
            (0x05, 3),
            (0x07, 1365),
            (0x18, 18),
            (0x2f, 5),
            (0x30, 5),
            (0x89, 75),
            (0x8c, 11),
            (0x8d, 11),
            (0x8e, 932),
            (0x8f, 85),
            (0x95, 349),
        ])
    );
    assert_eq!(
        physical_padding,
        BTreeMap::from([(0, 31), (1, 2484), (2, 9), (3, 2)])
    );
    assert_eq!(
        unique_padding,
        BTreeMap::from([(0, 6), (1, 344), (2, 3), (3, 1)])
    );
    assert_eq!(
        children,
        BTreeMap::from([
            (698, (699, 8, vec![14])),
            (699, (700, 12, vec![14, 16, 18])),
            (700, (701, 26, vec![52, 44, 40, 36, 46, 50, 48, 38, 42, 34])),
        ])
    );
}

#[test]
fn gunbar_parent_auxiliary_stream_preserves_its_collision_branch_and_separate_render_program() {
    let mut found = false;
    visit_models(|bytes, entry, key| {
        if key.level != 32 || entry.identity.global_index != 553 {
            return;
        }
        assert!(!found);
        found = true;
        assert_eq!(entry.identity.local_index, 23);
        assert_eq!(entry.span.start, 0x5b16c);
        assert_eq!(entry.header.raw_0e, 0x01a2);
        assert_eq!(
            entry.auxiliary_span,
            ByteSpan {
                start: 0x5b18c,
                end: 0x5b1ac
            }
        );
        assert_eq!(
            entry.auxiliary_span.get(bytes).unwrap(),
            [
                0x07, 0x01, 0, 0, 0x8e, 0, 0, 0, 0xa2, 0x01, 0, 0, 0, 0, 0x95, 0, 0x0e, 0, 0x07,
                0x01, 0x22, 0, 0x8e, 0, 0x16, 0, 0, 0, 0x22, 0, 0x88, 0,
            ]
        );
        let program = entry.auxiliary_program.as_ref().unwrap();
        assert_eq!(
            program
                .instructions
                .iter()
                .map(|i| i.opcode)
                .collect::<Vec<_>>(),
            [0x07, 0x8e, 0x95, 0x07, 0x8e]
        );
        assert_eq!(
            program
                .instructions
                .iter()
                .map(|i| (
                    i.span.start - entry.auxiliary_span.start,
                    i.span.end - entry.auxiliary_span.start
                ))
                .collect::<Vec<_>>(),
            [(0, 4), (4, 14), (14, 18), (18, 22), (22, 30)]
        );
        assert!(matches!(
            program.instructions[2].kind,
            ModelAuxiliaryInstructionKind::BranchUnlessHit {
                displacement_bytes: 14,
                target: 0x5b1aa
            }
        ));
        assert_eq!(
            program.terminator,
            ByteSpan {
                start: 0x5b1aa,
                end: 0x5b1ab
            }
        );
        assert_eq!(
            program.padding,
            ByteSpan {
                start: 0x5b1ab,
                end: 0x5b1ac
            }
        );
        assert_eq!(entry.vertices_span.start, program.padding.end);
        assert_eq!(entry.command_span.start - entry.span.start, 0x142);
        assert!(entry.command_span.start > program.padding.end);
    });
    assert!(found, "retail leaf32 parent553 fixture is required");
}
