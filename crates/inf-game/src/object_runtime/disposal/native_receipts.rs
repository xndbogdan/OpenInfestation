//! Original 00479840 traversal with supplied lower virtual-call effects.
//! Objects use real common allocation/membership; native vtable identities
//! are supplied service metadata, not fabricated derived actor construction.
//! Class callback/destructor bodies and frame scheduling are outside scope.

use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectDestructionRequest, ObjectNodeKind};
use serde_json::{json, Map, Value};

const LISTS: [(&str, ObjectList); 6] = [
    ("flag0", ObjectList::Flag0Primary),
    ("flag1", ObjectList::Flag1Primary),
    ("flag0_secondary", ObjectList::Flag0Secondary),
    ("flag1_secondary", ObjectList::Flag1Secondary),
    ("parking", ObjectList::Parking),
    ("disposal", ObjectList::Disposal),
];

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn role(name: &str) -> ObjectList {
    LISTS.iter().find(|(label, _)| *label == name).unwrap().1
}

struct Host<'a> {
    input: &'a Value,
    objects: Vec<ObjectId>,
    vtables: Vec<u32>,
    events: Vec<Value>,
    destroyed: Vec<usize>,
}

impl Host<'_> {
    fn index(&self, object: ObjectId) -> usize {
        self.objects.iter().position(|id| *id == object).unwrap()
    }

    fn snapshot(&self, arena: &ObjectArena) -> Value {
        let lists: Map<String, Value> = LISTS
            .iter()
            .map(|(name, role)| {
                let nodes: Vec<_> = arena
                    .list(*role)
                    .iter()
                    .map(|node| {
                        let offset = match node.kind {
                            ObjectNodeKind::Flag0 => 4,
                            ObjectNodeKind::Flag1 => 16,
                            ObjectNodeKind::Lifecycle => 28,
                        };
                        json!([self.index(node.object), offset])
                    })
                    .collect();
                ((*name).to_owned(), Value::Array(nodes))
            })
            .collect();
        let objects: Vec<_> = self
            .objects
            .iter()
            .enumerate()
            .filter_map(|(index, object)| {
                let state = arena.object(*object).ok()?;
                Some(json!({
                    "id":index, "vtable":self.vtables[index],
                    "flags":state.base.flags,"dirty":state.base.dirty_word
                }))
            })
            .collect();
        json!({"lists":lists,"objects":objects})
    }

    fn actions(&mut self, arena: &mut ObjectArena, kind: &str, index: usize) {
        let Some(actions) = self.input[format!("{kind}_actions")][index.to_string()].as_array()
        else {
            return;
        };
        for action in actions {
            let target = number(&action["object"]) as usize;
            let object = self.objects[target];
            match action["op"].as_str().unwrap() {
                "move" => {
                    let role = role(action["list"].as_str().unwrap());
                    arena.move_node(object, ObjectNodeKind::Lifecycle, role);
                    if action["front"].as_bool().unwrap_or(false) {
                        // Explicit supplied lower-call mutation. Ordinary
                        // native disposal appends; this receipt tests a host
                        // callback installing a different current head.
                        let physical = arena.list_owners[role as usize];
                        let moved = arena.lists[physical].pop().unwrap();
                        assert_eq!(moved.object, object);
                        assert_eq!(moved.kind, ObjectNodeKind::Lifecycle);
                        arena.lists[physical].insert(0, moved);
                    }
                }
                "vtable" => self.vtables[target] = number(&action["value"]),
                "flags" => {
                    arena.owned_mut(object).unwrap().state.base.flags = number(&action["value"])
                }
                other => panic!("unowned supplied mutation {other}"),
            }
        }
    }

    fn boundary(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        kind: &'static str,
    ) -> Result<(), &'static str> {
        let index = self.index(object);
        // Lower services select the current class independently at each call.
        // The native fixture executes these exact vtable loads; this host
        // supplies only their target metadata and effects to the drain kernel.
        let address = match (kind, self.vtables[index]) {
            ("callback", 0x0052_7b24) => 0x0040_1310u32,
            ("callback", 0x0052_7058 | 0x0052_7ee0) => 0x0047_ec50,
            ("destructor", 0x0052_7b24) => 0x0047_9ce0,
            ("destructor", 0x0052_7058) => 0x0046_71c0,
            ("destructor", 0x0052_7ee0) => 0x0048_1150,
            other => panic!("unowned native virtual target {other:?}"),
        };
        let mut event = json!({
            "kind":kind,"object":index,"address":address,"state":self.snapshot(arena)
        });
        if kind == "destructor" {
            // This service is specifically the deleting +00(1) boundary.
            event["delete_argument"] = json!(1);
        }
        self.events.push(event);
        let stop = self.input["stop"].as_str() == Some(kind)
            && self.input["stop_object"].as_u64() == Some(index as u64);
        if stop && !self.input["stop_after_actions"].as_bool().unwrap() {
            return Err(kind);
        }
        self.actions(arena, kind, index);
        if stop {
            return Err(kind);
        }
        Ok(())
    }
}

impl DisposalDispatch for Host<'_> {
    type Error = &'static str;

    fn callback(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        // The native +30 return register is ignored. Result carries only the
        // portable stop, including the receipt with supplied EAX=FFFFFFFF.
        self.boundary(arena, object, "callback")
    }

    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), Self::Error> {
        self.boundary(arena, object, "destructor")?;
        arena
            .destroy(ObjectDestructionRequest {
                object,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        self.destroyed.push(self.index(object));
        Ok(())
    }
}

#[test]
fn object_disposal_drain_matches_21_native_traversal_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/object_disposal_drain_native.json"
    )))
    .unwrap();
    assert_eq!(number(&fixture["provenance"]["fpcw"]), 0x027f);
    assert_eq!(fixture["provenance"]["code_patched"], false);
    assert_eq!(fixture["provenance"]["hooks"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 21);
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut arena = ObjectArena::default();
        let objects: Vec<_> = (0..number(&input["count"]))
            .map(|_| arena.allocate(ObjectClassKind::Base))
            .collect();
        for entry in input["disposal"].as_array().unwrap() {
            arena.dispose(objects[number(entry) as usize]).unwrap();
        }
        let mut host = Host {
            input,
            vtables: vec![0x0052_7b24; objects.len()],
            objects,
            events: Vec::new(),
            destroyed: Vec::new(),
        };
        for initial in case["initial"]["objects"].as_array().unwrap() {
            let target = number(&initial["id"]) as usize;
            host.vtables[target] = number(&initial["vtable"]);
            let base = &mut arena.owned_mut(host.objects[target]).unwrap().state.base;
            base.flags = number(&initial["flags"]);
            base.dirty_word = number(&initial["dirty"]) as u16;
        }
        assert_eq!(
            host.snapshot(&arena),
            case["initial"],
            "case {index} initial membership"
        );
        let mut state = ObjectDisposalState::default();
        let result = arena.drain_disposal_with(
            &mut host,
            &mut state,
            ObjectDisposalFailurePolicy::ReturnFirstError,
        );
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "case {index} result"
        );
        match result {
            Ok(report) => {
                assert!(report.newly_stopped.is_empty());
                assert!(state.stopped().is_empty());
                let destroyed: Vec<_> = report.destroyed.iter().map(|id| host.index(*id)).collect();
                assert_eq!(json!(destroyed), case["destroyed"], "case {index} report");
            }
            Err(ObjectDisposalError::Failed {
                object,
                phase,
                error,
            }) => {
                assert_eq!(host.index(object), number(&input["stop_object"]) as usize);
                assert_eq!(Some(error), input["stop"].as_str());
                let expected_phase = if error == "callback" {
                    ObjectDisposalPhase::CleanupCallback
                } else {
                    ObjectDisposalPhase::DeletingDestructor
                };
                assert_eq!(phase, expected_phase);
                assert_eq!(state.stopped().len(), 1);
                assert_eq!(state.stopped()[0].object, object);
                assert_eq!(state.stopped()[0].phase, phase);
                assert_eq!(state.stopped()[0].reason, error);
            }
            Err(error) => panic!("case {index} unexpected error {error:?}"),
        }
        assert_eq!(
            json!(host.events),
            case["events"],
            "case {index} virtual call order and prefixes"
        );
        assert_eq!(
            json!(host.destroyed),
            case["destroyed"],
            "case {index} successful deletions"
        );
        assert_eq!(
            host.snapshot(&arena),
            case["final"],
            "case {index} retained membership"
        );
    }
}

#[test]
fn object_disposal_drain_complete_native_body_is_guarded() {
    use sha2::{Digest, Sha256};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    // Includes the capture before +30, fresh vtable load for +00(1),
    // restart at the manager head, empty-tail check and final RET.
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes[0x0007_9840..0x0007_986b])),
        "ab790356b9df43cce1631f4758d790fcb25ebedfe2ea8ad8c3c99a4996c2aa8a"
    );
}
