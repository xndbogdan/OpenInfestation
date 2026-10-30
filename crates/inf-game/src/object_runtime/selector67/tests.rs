use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectList, ObjectNode};
use serde::Deserialize;
use serde_json::{json, Value};

fn executable_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("required original retail executable")
}

/// check_selector67_ribbon.py's admitted flags and source handle.
const FLAGS: u32 = 0x0810_0003;
const SOURCE: u32 = 0x0003_0005;

#[derive(Deserialize)]
struct Receipt {
    constants: Constants,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Constants {
    segment_53c558: u32,
    split_53c55c: u32,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    steps: Vec<Value>,
    flags: Option<u32>,
    list: Option<String>,
    position: Option<[u16; 3]>,
    #[serde(default)]
    words: Vec<(usize, u16)>,
    #[serde(default)]
    dwords: Vec<(usize, u32)>,
    #[serde(default)]
    nodes: Vec<[u16; 4]>,
    #[serde(default)]
    split_ready: bool,
    owner_alive: Option<bool>,
    process_null: Option<u32>,
    steps_after: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    events: Vec<Value>,
    ribbon: State,
    primary: Vec<String>,
    alternate: Vec<String>,
    split: Option<State>,
}
#[derive(Clone, Deserialize)]
struct State {
    word_28: u16,
    position: [u16; 3],
    words_40: [u16; 4],
    dword_48: u32,
    owner_50: u32,
    dword_54: u32,
    nodes: Vec<[u16; 4]>,
}

struct Host {
    split_ready: bool,
    process_null: ActorHandle,
    owner_alive: bool,
    events: Vec<Value>,
    split: Option<ObjectId>,
}

impl Selector67Host for Host {
    type Error = std::convert::Infallible;

    fn split_ribbon(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        self.events.push(json!(["create", self.process_null.raw()]));
        if !std::mem::take(&mut self.split_ready) {
            return Ok(None);
        }
        // 00486600 stores its source after a successful admission.
        let created = arena.allocate_selector67(self.process_null);
        arena
            .set_selector67_owner(created, self.process_null)
            .unwrap();
        self.split = Some(created);
        Ok(Some(created))
    }

    fn owner_resolves(
        &mut self,
        _: &mut ObjectArena,
        owner: ActorHandle,
    ) -> Result<bool, Self::Error> {
        self.events.push(json!(["resolve", owner.raw()]));
        Ok(self.owner_alive)
    }
}

fn nodes(rows: &[[u16; 4]]) -> Vec<RibbonNode> {
    rows.iter()
        .map(|&[x, y, z, length]| RibbonNode {
            point: [x, y, z],
            length,
        })
        .collect()
}

fn assert_state(arena: &ObjectArena, object: ObjectId, expected: &State, label: &str) {
    let base = &arena.object(object).unwrap().base;
    let state = arena.selector67(object).unwrap();
    assert_eq!(base.word_28, expected.word_28, "{label} +28");
    // An unwritten native position keeps the supplied A5 fill.
    assert_eq!(
        base.position_words.unwrap_or([0xa5a5; 3]),
        expected.position,
        "{label} +32"
    );
    assert_eq!(
        [state.word_40, state.word_42, state.word_44, state.word_46],
        expected.words_40,
        "{label} +40..+46"
    );
    assert_eq!(state.dword_48, expected.dword_48, "{label} +48");
    assert_eq!(state.owner_50.raw(), expected.owner_50, "{label} +50");
    assert_eq!(state.dword_54, expected.dword_54, "{label} +54");
    assert_eq!(state.nodes, nodes(&expected.nodes), "{label} +4C nodes");
}

/// 004869B0, 004868F0, 004866A0, 00486500 and 00479040 against the
/// unchanged executable: every field, node, list position, split and
/// owner check after each step. Allocation is the bump heap's success.
#[test]
fn selector67_ribbon_matches_native_ship_updates_ticks_and_splits() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector67_ribbon_native.json"
    ))
    .unwrap();
    assert_eq!(
        executable.word(SEGMENT_0053C558).unwrap(),
        receipt.constants.segment_53c558
    );
    assert_eq!(
        executable.word(SPLIT_0053C55C).unwrap(),
        receipt.constants.split_53c55c
    );
    let mut steps = 0;
    for case in &receipt.cases {
        let mut arena = ObjectArena::new();
        let primary = arena.allocate(ObjectClassKind::Base);
        let ribbon = arena.allocate_selector67(ActorHandle::from_raw(0x0001_0001));
        arena
            .set_selector67_owner(ribbon, ActorHandle::from_raw(SOURCE))
            .unwrap();
        let alternate = arena.allocate(ObjectClassKind::Base);
        arena.move_node(alternate, ObjectNodeKind::Flag1, ObjectList::Flag1Secondary);
        let flags = case.flags.unwrap_or(FLAGS);
        arena.owned_mut(ribbon).unwrap().state.base.flags = flags;
        match case.list.as_deref() {
            Some("alternate") => {
                arena.move_node(ribbon, ObjectNodeKind::Flag1, ObjectList::Flag1Secondary)
            }
            None if flags & 2 == 0 => {
                arena.move_node(ribbon, ObjectNodeKind::Flag1, ObjectList::Parking)
            }
            None => {}
            Some(other) => panic!("{other}"),
        }
        arena.owned_mut(ribbon).unwrap().state.base.position_words = case.position;
        for &(offset, value) in &case.words {
            if offset == 0x28 {
                arena.owned_mut(ribbon).unwrap().state.base.word_28 = value;
            }
        }
        {
            let state = arena.selector67_mut(ribbon).unwrap();
            for &(offset, value) in &case.words {
                match offset {
                    0x28 => {}
                    0x40 => state.word_40 = value,
                    0x42 => state.word_42 = value,
                    0x44 => state.word_44 = value,
                    0x46 => state.word_46 = value,
                    _ => panic!("{offset:X}"),
                }
            }
            for &(offset, value) in &case.dwords {
                assert_eq!(offset, 0x48);
                state.dword_48 = value;
            }
            state.nodes = nodes(&case.nodes);
        }
        let mut host = Host {
            split_ready: case.split_ready,
            process_null: ActorHandle::from_raw(case.process_null.unwrap_or(0x0001_0001)),
            owner_alive: case.owner_alive.unwrap_or(true),
            events: Vec::new(),
            split: None,
        };
        let names = |arena: &ObjectArena, role| {
            arena
                .list(role)
                .iter()
                .map(|node: &ObjectNode| {
                    if node.object == ribbon {
                        "ribbon"
                    } else if node.object == primary {
                        "primary"
                    } else if node.object == alternate {
                        "alternate"
                    } else {
                        "split"
                    }
                })
                .filter(|name| *name != "split")
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        for (index, (step, expected)) in case.steps.iter().zip(&case.steps_after).enumerate() {
            let label = format!("{} step {index}", case.name);
            host.events.clear();
            let microseconds = step[1].as_u64().unwrap() as u32;
            match step[0].as_str().unwrap() {
                "ship" => {
                    let point: [u16; 3] = serde_json::from_value(step[2].clone()).unwrap();
                    arena
                        .update_selector67_from_ship(
                            ribbon,
                            microseconds,
                            point,
                            executable,
                            &mut host,
                        )
                        .unwrap();
                }
                "tick" => arena
                    .tick_selector67(ribbon, microseconds, executable, &mut host)
                    .unwrap(),
                other => panic!("{other}"),
            }
            let native_events = expected
                .events
                .iter()
                .filter(|event| matches!(event[0].as_str(), Some("create" | "resolve")))
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(host.events, native_events, "{label} services");
            let disposed = expected.events.iter().any(|event| event[0] == "dispose");
            assert_eq!(
                arena.list(ObjectList::Disposal).contains(&ObjectNode {
                    object: ribbon,
                    kind: ObjectNodeKind::Lifecycle,
                }),
                disposed,
                "{label} disposal"
            );
            assert_state(&arena, ribbon, &expected.ribbon, &label);
            // The harness records 00479FA0; the real disposal's 00479F20(false)
            // parks the +10 node.
            let listed = |names: &[String]| {
                names
                    .iter()
                    .filter(|name| !disposed || *name != "ribbon")
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                names(&arena, ObjectList::Flag1Primary),
                listed(&expected.primary),
                "{label}"
            );
            assert_eq!(
                names(&arena, ObjectList::Flag1Secondary),
                listed(&expected.alternate),
                "{label}"
            );
            match (&expected.split, host.split) {
                // Native leaves the split ribbon's +32 to the oracle's A5
                // fill; the port's is the labelled approximation.
                (Some(state), Some(split)) if state.position == [0xa5a5; 3] => {
                    let approximated = State {
                        position: SELECTOR67_POSITION_APPROXIMATION,
                        ..state.clone()
                    };
                    assert_state(&arena, split, &approximated, &label)
                }
                (Some(state), Some(split)) => assert_state(&arena, split, state, &label),
                (None, None) => {}
                _ => panic!("{label} split ribbon"),
            }
            steps += 1;
        }
    }
    assert_eq!((receipt.cases.len(), steps), (44, 120));
}

/// A split ribbon starts its +32 at the approximation, so its own +24 takes
/// the radius from there and recedes as any ribbon does.
#[test]
fn a_split_ribbon_ticks_from_the_approximated_position() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut arena = ObjectArena::new();
    let ribbon = arena.allocate_selector67(ActorHandle::from_raw(SOURCE));
    // A trail of 300 that ends at the ship's last position.
    arena.selector67_mut(ribbon).unwrap().nodes =
        nodes(&[[1300, 50, 1000, 300], [1000, 50, 1000, 0]]);
    arena.owned_mut(ribbon).unwrap().state.base.position_words = Some([1300, 50, 1000]);
    let mut host = Host {
        split_ready: true,
        process_null: ActorHandle::from_raw(7),
        owner_alive: true,
        events: Vec::new(),
        split: None,
    };
    // A jump of 1200 along X splits the trail off.
    arena
        .update_selector67_from_ship(ribbon, 33_333, [2500, 50, 1000], executable, &mut host)
        .unwrap();
    let split = host.split.expect("the jump splits the ribbon");
    assert_eq!(
        arena.selector67(split).unwrap().nodes,
        nodes(&[[1300, 50, 1000, 300], [1000, 50, 1000, 0]])
    );
    // A fresh heap's zero, as three of retail's 23 split ribbons held.
    assert_eq!(
        arena.object(split).unwrap().base.position_words,
        Some([0, 0, 0])
    );
    arena
        .tick_selector67(split, 33_333, executable, &mut host)
        .unwrap();
    // The radius: the root of the farthest node's squared distance from 0,
    // plus 16. Then the tail recedes as any ribbon's does: +54 keeps the
    // trail's 300 less (33333 >> 5) * 150 >> 15.
    let spread = 1300 * 1300 + 50 * 50 + 1000 * 1000;
    assert_eq!(
        arena.object(split).unwrap().base.word_28,
        (isqrt_48efc0(spread) + 16) as u16
    );
    assert_eq!(arena.selector67(split).unwrap().dword_54, 296);
}

/// A ribbon without a position, which no ship update or split produces,
/// still fails closed in its own +24 after the native +54 store.
#[test]
fn split_ribbon_tick_stops_at_its_unwritten_position() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut arena = ObjectArena::new();
    let ribbon = arena.allocate_selector67(ActorHandle::from_raw(7));
    arena.selector67_mut(ribbon).unwrap().nodes = nodes(&[[1, 2, 3, 40], [1, 2, 43, 0]]);
    arena.selector67_mut(ribbon).unwrap().dword_54 = 99;
    let mut host = Host {
        split_ready: false,
        process_null: ActorHandle::from_raw(7),
        owner_alive: true,
        events: Vec::new(),
        split: None,
    };
    assert!(matches!(
        arena.tick_selector67(ribbon, 33_333, executable, &mut host),
        Err(Selector67Error::UnwrittenPosition(id)) if id == ribbon
    ));
    assert_eq!(arena.selector67(ribbon).unwrap().dword_54, 0);
    assert!(host.events.is_empty());
}
