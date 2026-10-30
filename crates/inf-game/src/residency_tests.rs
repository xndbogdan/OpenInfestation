//! Synthetic lifecycle cases pin effects that a fresh corpus load cannot show.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use inf_formats::{
    BranchKind, HandlerRequest, OverlayKey, OverlayLeaf, RequestKind, RequestTable,
    ResourceSegment, SceneNode, HANDLER_COUNT,
};

use crate::{
    Error, HandlerResources, Installation, ResidencyEvent, ResidencyRequest, ResidentRuntime,
    ResourceScope, ResourceSelection, SceneContext,
};

use RequestKind::{Base, Difficulty, DifficultyLanguage, Language};
use ResidencyEvent::{LeafActivated, LeafReleased, OverlayLoaded, OverlayReleased};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(children: Vec<SceneNode>) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-game-residency-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut bytes = vec![0, 0]; // initial difficulty and language
        for _ in 0..HANDLER_COUNT {
            bytes.extend_from_slice(&16u16.to_le_bytes());
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.push(64); // enough ID slots for every synthetic leaf
        bytes.push(children.len() as u8); // implicit additive root
        for child in children {
            encode_node(&child, &mut bytes);
        }
        fs::write(root.join("PreLoad.dat"), bytes).unwrap();
        Self { root }
    }

    fn runtime(&self) -> ResidentRuntime {
        ResidentRuntime::new(Installation::open(&self.root).unwrap()).unwrap()
    }

    fn bank(&self, level: u8, selection: ResourceSelection, phase: RequestKind, values: &[&str]) {
        let payload: Vec<_> = values
            .iter()
            .flat_map(|value| value.bytes().chain(std::iter::once(0)))
            .collect();
        let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
        bytes.extend(payload);
        let context = SceneContext::new(level, selection.difficulty, selection.language);
        fs::write(
            self.root
                .join("Overlay")
                .join(&context.overlay_filenames()[phase_index(phase)]),
            bytes,
        )
        .unwrap();
    }

    fn base(&self, level: u8, values: &[&str]) {
        self.bank(level, selection(0, 0), Base, values);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn phase_index(phase: RequestKind) -> usize {
    match phase {
        Base => 0,
        Difficulty => 1,
        Language => 2,
        DifficultyLanguage => 3,
    }
}

fn encode_node(node: &SceneNode, bytes: &mut Vec<u8>) {
    match node {
        SceneNode::Leaf(leaf) => {
            bytes.extend_from_slice(&[0, leaf.overlay_id]);
            for table in [
                &leaf.base,
                &leaf.difficulty,
                &leaf.language,
                &leaf.difficulty_language,
            ] {
                for (_, request) in table.iter() {
                    bytes.extend_from_slice(&request.count.to_le_bytes());
                    bytes.extend_from_slice(&(request.segments.len() as u16).to_le_bytes());
                    for segment in &request.segments {
                        bytes.extend_from_slice(&segment.packed().to_le_bytes());
                    }
                }
            }
        }
        SceneNode::Branch {
            branch_kind,
            children,
        } => {
            bytes.push(match branch_kind {
                BranchKind::Select => 1,
                BranchKind::Group => 2,
            });
            bytes.push(children.len() as u8);
            for child in children {
                encode_node(child, bytes);
            }
        }
    }
}

fn leaf(level: u8, phases: &[(RequestKind, &[u16])]) -> SceneNode {
    let mut tables: [RequestTable; 4] = std::array::from_fn(|_| {
        RequestTable(std::array::from_fn(|_| HandlerRequest {
            count: 0,
            segments: Vec::new(),
        }))
    });
    for &(phase, slots) in phases {
        tables[phase_index(phase)].0[0] = HandlerRequest {
            count: slots.len() as i16,
            segments: slots
                .iter()
                .enumerate()
                .map(|(local, global)| ResourceSegment {
                    local_start: local as u16,
                    global_delta: *global as i16 - local as i16,
                })
                .collect(),
        };
    }
    let [base, difficulty, language, difficulty_language] = tables;
    SceneNode::Leaf(Box::new(OverlayLeaf {
        overlay_id: level,
        base,
        difficulty,
        language,
        difficulty_language,
    }))
}

fn branch(branch_kind: BranchKind, children: Vec<SceneNode>) -> SceneNode {
    SceneNode::Branch {
        branch_kind,
        children,
    }
}

fn selection(difficulty: u8, language: u8) -> ResourceSelection {
    ResourceSelection {
        difficulty,
        language,
    }
}

fn key(level: u8, selection: ResourceSelection, phase: RequestKind) -> OverlayKey {
    SceneContext::new(level, selection.difficulty, selection.language).overlay_keys()
        [phase_index(phase)]
    .clone()
}

fn bytes_pointer(runtime: &ResidentRuntime, slot: u16) -> *const u8 {
    runtime.resource(0, slot).unwrap().overlay.bytes().as_ptr()
}

fn text(runtime: &ResidentRuntime, slot: u16) -> String {
    let resource = runtime.resource(0, slot).unwrap();
    let (_, HandlerResources::Strings(bank)) = resource.overlay.handler(0).unwrap() else {
        panic!("synthetic fixture contains a string bank");
    };
    bank.entries[resource.registration.local_index as usize]
        .byte_preserving_text(resource.overlay.bytes())
        .unwrap()
}

#[test]
fn boot_activates_the_authored_root_and_replacement_retains_other_allocations() {
    let fixture = Fixture::new(vec![
        leaf(0, &[(Base, &[0])]),
        branch(
            BranchKind::Select,
            vec![leaf(2, &[(Base, &[1])]), leaf(47, &[(Base, &[2])])],
        ),
        leaf(9, &[(Base, &[3])]),
    ]);
    fixture.base(0, &["root"]);
    fixture.base(2, &["intro"]);
    fixture.base(47, &["menu"]);
    fixture.base(9, &["persistent"]);
    let mut runtime = fixture.runtime();
    assert!(runtime.summary().active_leaves.is_empty());
    let boot = runtime.apply(ResidencyRequest::Boot).unwrap();
    let initial = selection(0, 0);
    assert_eq!(
        boot.events,
        [
            OverlayLoaded {
                key: key(0, initial, Base)
            },
            LeafActivated { level: 0 },
            OverlayLoaded {
                key: key(2, initial, Base)
            },
            LeafActivated { level: 2 },
            OverlayLoaded {
                key: key(9, initial, Base)
            },
            LeafActivated { level: 9 },
        ]
    );
    assert_eq!(runtime.summary().scope, ResourceScope::ResidentResources);
    assert_eq!(runtime.summary().active_leaves, [0, 2, 9]);
    let retained = [bytes_pointer(&runtime, 0), bytes_pointer(&runtime, 3)];
    let change = runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    assert_eq!(
        change.events,
        [
            OverlayReleased {
                key: key(2, initial, Base)
            },
            LeafReleased { level: 2 },
            OverlayLoaded {
                key: key(47, initial, Base)
            },
            LeafActivated { level: 47 },
        ]
    );
    assert_eq!(runtime.summary().active_leaves, [0, 47, 9]);
    assert!(runtime.resource(0, 1).is_none());
    assert_eq!(text(&runtime, 2), "menu");
    assert_eq!(
        [bytes_pointer(&runtime, 0), bytes_pointer(&runtime, 3)],
        retained
    );
}

#[test]
fn empty_leaf_48_releases_all_four_phases_in_forward_order() {
    let fixture = Fixture::new(vec![
        leaf(0, &[(Base, &[0])]),
        branch(
            BranchKind::Select,
            vec![
                leaf(
                    2,
                    &[
                        (Base, &[1]),
                        (Difficulty, &[2]),
                        (Language, &[3]),
                        (DifficultyLanguage, &[4]),
                    ],
                ),
                leaf(48, &[]),
            ],
        ),
    ]);
    fixture.base(0, &["retained"]);
    let initial = selection(0, 0);
    for phase in [Base, Difficulty, Language, DifficultyLanguage] {
        fixture.bank(2, initial, phase, &["departing"]);
    }
    let mut runtime = fixture.runtime();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let retained = bytes_pointer(&runtime, 0);
    let change = runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    assert_eq!(
        change.events,
        [
            OverlayReleased {
                key: key(2, initial, Base)
            },
            OverlayReleased {
                key: key(2, initial, Difficulty)
            },
            OverlayReleased {
                key: key(2, initial, Language)
            },
            OverlayReleased {
                key: key(2, initial, DifficultyLanguage)
            },
            LeafReleased { level: 2 },
            LeafActivated { level: 48 },
        ]
    );
    assert_eq!(runtime.summary().active_leaves, [0, 48]);
    assert_eq!(runtime.summary().overlays.len(), 1);
    for slot in 1..=4 {
        assert!(runtime.resource(0, slot).is_none());
    }
    assert_eq!(bytes_pointer(&runtime, 0), retained);
}

#[test]
fn unload_clears_retained_writers_and_never_restores_shadowed_predecessors() {
    let fixture = Fixture::new(vec![
        leaf(0, &[(Base, &[1, 3])]),
        branch(
            BranchKind::Select,
            vec![leaf(2, &[(Base, &[0, 0, 1, 2])]), leaf(48, &[])],
        ),
        leaf(3, &[(Base, &[0, 4])]),
    ]);
    fixture.base(0, &["shadowed predecessor", "early anchor"]);
    fixture.base(
        2,
        &["aliased first", "aliased last", "replacement", "exclusive"],
    );
    fixture.base(3, &["retained current writer", "late anchor"]);
    let mut runtime = fixture.runtime();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    assert_eq!(runtime.resource(0, 0).unwrap().registration.level, 3);
    assert_eq!(runtime.resource(0, 1).unwrap().registration.level, 2);
    let retained = [bytes_pointer(&runtime, 3), bytes_pointer(&runtime, 4)];
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    assert_eq!(runtime.summary().active_leaves, [0, 48, 3]);
    // 004EE6D0 clears the departing leaf's mappings unconditionally, even
    // where leaf 3 was the latest writer or leaf 0 remains allocated below it.
    for slot in 0..=2 {
        assert!(runtime.resource(0, slot).is_none());
    }
    assert_eq!(
        [bytes_pointer(&runtime, 3), bytes_pointer(&runtime, 4)],
        retained
    );
    assert_eq!(runtime.summary().active_slots[0], 2);
}

fn reload_fixture() -> Fixture {
    Fixture::new(vec![
        leaf(
            10,
            &[
                (Difficulty, &[0, 4]),
                (Language, &[1, 5]),
                (DifficultyLanguage, &[2, 6]),
            ],
        ),
        leaf(
            20,
            &[
                (Difficulty, &[1, 7]),
                (Language, &[2, 8]),
                (DifficultyLanguage, &[3, 9]),
            ],
        ),
    ])
}

fn write_reload_banks(fixture: &Fixture, selected: ResourceSelection) {
    for level in [10, 20] {
        for phase in [Difficulty, Language, DifficultyLanguage] {
            fixture.bank(
                level,
                selected,
                phase,
                &["overlapping", "allocation anchor"],
            );
        }
    }
}

#[test]
fn selector_reload_matrix_is_phase_major_and_preserves_unselected_allocations() {
    let initial = selection(0, 0);
    for (selected, phases, slot_one_winner) in [
        (
            selection(1, 0),
            vec![Difficulty, DifficultyLanguage],
            (20, Difficulty),
        ),
        (
            selection(0, 1),
            vec![Language, DifficultyLanguage],
            (10, Language),
        ),
        (
            selection(1, 1),
            vec![Difficulty, Language, DifficultyLanguage],
            (10, Language),
        ),
    ] {
        let fixture = reload_fixture();
        write_reload_banks(&fixture, initial);
        write_reload_banks(&fixture, selected);
        let mut runtime = fixture.runtime();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        let pointers: Vec<_> = (4..=9).map(|slot| bytes_pointer(&runtime, slot)).collect();
        let change = runtime
            .apply(ResidencyRequest::Select {
                selection: selected,
            })
            .unwrap();
        let expected: Vec<_> = phases
            .iter()
            .flat_map(|&phase| {
                [10, 20].into_iter().flat_map(move |level| {
                    [
                        OverlayReleased {
                            key: key(level, initial, phase),
                        },
                        OverlayLoaded {
                            key: key(level, selected, phase),
                        },
                    ]
                })
            })
            .collect();
        assert_eq!(change.events, expected);
        assert_eq!(runtime.summary().selection, selected);
        assert_eq!(runtime.summary().active_leaves, [10, 20]);
        let winner = runtime.resource(0, 1).unwrap().registration;
        assert_eq!((winner.level, winner.phase), slot_one_winner);
        let winner = runtime.resource(0, 2).unwrap().registration;
        assert_eq!((winner.level, winner.phase), (10, DifficultyLanguage));
        // A per-leaf reconstruction would instead leave leaf 20's difficulty
        // at slot 1 and its language at slot 2 after a both-selectors change.
        for (index, phase) in [
            Difficulty,
            Language,
            DifficultyLanguage,
            Difficulty,
            Language,
            DifficultyLanguage,
        ]
        .into_iter()
        .enumerate()
        {
            let current = bytes_pointer(&runtime, index as u16 + 4);
            if phases.contains(&phase) {
                assert_ne!(current, pointers[index]);
            } else {
                assert_eq!(current, pointers[index]);
            }
        }
    }
}

#[test]
fn unchanged_selection_and_repeated_boot_do_not_read_any_overlay() {
    let fixture = reload_fixture();
    write_reload_banks(&fixture, selection(0, 0));
    let mut runtime = fixture.runtime();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let before = runtime.summary();
    let pointer = bytes_pointer(&runtime, 4);
    fs::remove_dir_all(fixture.root.join("Overlay")).unwrap();
    let unchanged = runtime
        .apply(ResidencyRequest::Select {
            selection: selection(0, 0),
        })
        .unwrap();
    assert!(unchanged.events.is_empty());
    assert!(runtime
        .apply(ResidencyRequest::Boot)
        .unwrap()
        .events
        .is_empty());
    assert_eq!(runtime.summary(), before);
    assert_eq!(bytes_pointer(&runtime, 4), pointer);
}

#[test]
fn inactive_selector_change_does_no_io_and_failed_boot_retains_inactive_tree() {
    let fixture = Fixture::new(vec![leaf(0, &[(Base, &[0])]), leaf(1, &[(Base, &[1])])]);
    let mut runtime = fixture.runtime();
    let selected = selection(3, 2);
    assert!(runtime
        .apply(ResidencyRequest::Select {
            selection: selected
        })
        .unwrap()
        .events
        .is_empty());
    assert_eq!(runtime.summary().selection, selected);
    assert!(runtime.summary().active_leaves.is_empty());
    fixture.base(0, &["staged first bank"]);
    let before = runtime.summary();
    assert!(matches!(
        runtime.apply(ResidencyRequest::Boot),
        Err(Error::Read { .. })
    ));
    assert_eq!(runtime.summary(), before);
    assert!(runtime.resource(0, 0).is_none());
    fixture.base(1, &["second bank"]);
    assert_eq!(
        runtime.apply(ResidencyRequest::Boot).unwrap().events.len(),
        4
    );
    assert_eq!(runtime.summary().active_leaves, [0, 1]);
    assert_eq!(runtime.summary().selection, selected);
}

#[test]
fn failed_multibank_transition_and_unknown_leaf_preserve_tree_slots_and_bytes() {
    let fixture = Fixture::new(vec![
        leaf(0, &[(Base, &[0])]),
        branch(
            BranchKind::Select,
            vec![
                leaf(2, &[(Base, &[1])]),
                branch(
                    BranchKind::Group,
                    vec![leaf(10, &[(Base, &[2])]), leaf(11, &[(Base, &[3])])],
                ),
            ],
        ),
        leaf(12, &[(Base, &[4])]),
    ]);
    for (level, value) in [
        (0, "root"),
        (2, "original"),
        (10, "staged"),
        (12, "persistent"),
    ] {
        fixture.base(level, &[value]);
    }
    let mut runtime = fixture.runtime();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let before = runtime.summary();
    let pointers = [
        bytes_pointer(&runtime, 0),
        bytes_pointer(&runtime, 1),
        bytes_pointer(&runtime, 4),
    ];
    fixture.base(2, &["changed on disk"]);
    // Activating 11 first unloads 2 and loads its sibling 10; missing 11 must
    // roll back all three ownership steps, not just preserve a leaf number.
    assert!(matches!(
        runtime.apply(ResidencyRequest::Activate { level: 11 }),
        Err(Error::Read { .. })
    ));
    assert_eq!(runtime.summary(), before);
    assert_eq!(
        [
            bytes_pointer(&runtime, 0),
            bytes_pointer(&runtime, 1),
            bytes_pointer(&runtime, 4)
        ],
        pointers
    );
    assert_eq!(text(&runtime, 1), "original");
    assert!(runtime.resource(0, 2).is_none());
    assert!(runtime.resource(0, 3).is_none());
    assert!(matches!(
        runtime.apply(ResidencyRequest::Activate { level: 44 }),
        Err(Error::UnknownScene { level: 44 })
    ));
    assert_eq!(runtime.summary(), before);
    assert_eq!(bytes_pointer(&runtime, 1), pointers[1]);
    fixture.base(11, &["destination"]);
    let committed = runtime
        .apply(ResidencyRequest::Activate { level: 11 })
        .unwrap();
    assert_eq!(
        committed
            .events
            .iter()
            .filter_map(|event| match event {
                LeafReleased { level } => Some((false, *level)),
                LeafActivated { level } => Some((true, *level)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [(false, 2), (true, 10), (true, 11)]
    );
    assert_eq!(runtime.summary().active_leaves, [0, 10, 11, 12]);
    assert_eq!(
        [bytes_pointer(&runtime, 0), bytes_pointer(&runtime, 4)],
        [pointers[0], pointers[2]]
    );
}

#[test]
fn failed_reload_restores_both_selectors_and_all_original_allocations() {
    let fixture = reload_fixture();
    let initial = selection(0, 0);
    let selected = selection(1, 1);
    write_reload_banks(&fixture, initial);
    write_reload_banks(&fixture, selected);
    let mut runtime = fixture.runtime();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let before = runtime.summary();
    let pointers: Vec<_> = (4..=9).map(|slot| bytes_pointer(&runtime, slot)).collect();
    // Two difficulty replacements and leaf 10's language replacement have
    // already staged when leaf 20's new language file fails to open.
    fs::remove_file(fixture.root.join("Overlay/20l1.ovl")).unwrap();
    assert!(matches!(
        runtime.apply(ResidencyRequest::Select {
            selection: selected
        }),
        Err(Error::Read { .. })
    ));
    assert_eq!(runtime.summary(), before);
    assert_eq!(
        (4..=9)
            .map(|slot| bytes_pointer(&runtime, slot))
            .collect::<Vec<_>>(),
        pointers
    );
    assert_eq!(
        runtime.resource(0, 1).unwrap().registration.phase,
        Difficulty
    );
    assert_eq!(runtime.resource(0, 2).unwrap().registration.phase, Language);
    fixture.bank(20, selected, Language, &["replacement", "new anchor"]);
    runtime
        .apply(ResidencyRequest::Select {
            selection: selected,
        })
        .unwrap();
    assert_eq!(runtime.summary().selection, selected);
    assert_eq!(text(&runtime, 8), "new anchor");
}
