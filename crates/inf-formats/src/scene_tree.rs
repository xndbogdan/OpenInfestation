//! Native scene-node activation and unloading, independent of resource I/O.
//!
//! The flat node arena preserves the authored sibling order and parent links.
//! `FUN_004EE1E0` requests a leaf through its parent's virtual `+0x18` before
//! checking the leaf's active flag. Group and Select implement different
//! ancestor policies; computing only a difference between final leaf sets
//! would lose the registration order. Clone the tree before planning a resource
//! transaction, and publish that clone only after all operations succeed.

use serde::Serialize;
use thiserror::Error;

use crate::{BranchKind, Preload, SceneNode};

/// A scene tree that cannot name or hold a requested leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SceneTreeError {
    #[error("scene {level} is absent from PreLoad.dat")]
    UnknownScene { level: u8 },
    #[error("PreLoad.dat contains duplicate scene {level}")]
    DuplicateScene { level: u8 },
}

type Result<T> = std::result::Result<T, SceneTreeError>;

type NodeId = usize;

/// A leaf lifetime boundary, including leaves whose request tables are empty.
///
/// Each load expands to `FUN_004EE680`'s four overlay phases; each unload
/// expands to `FUN_004EE6D0`'s handler release and global-slot clearing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum LeafOperation {
    Load { level: u8 },
    Unload { level: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NodeKind {
    Leaf(u8),
    Branch {
        policy: BranchKind,
        children: Vec<NodeId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    parent: Option<NodeId>,
    kind: NodeKind,
    active: bool,
}

/// Scene-node lifetime state, with no decoded resources or platform pointers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneTree {
    nodes: Vec<Node>,
    leaves: [Option<NodeId>; 256],
}

impl SceneTree {
    /// Build inactive nodes from the authored tree without loading any leaf.
    pub fn new(preload: &Preload) -> Result<Self> {
        let mut tree = Self {
            nodes: Vec::new(),
            leaves: [None; 256],
        };
        tree.append_node(&preload.root, None)?;
        Ok(tree)
    }

    /// Activate the root, as `FUN_004EDEA0` does after parsing PreLoad.dat.
    /// Calling this after the root is active preserves its current selection.
    pub fn boot(&mut self) -> Vec<LeafOperation> {
        let mut operations = Vec::new();
        self.activate(0, &mut operations);
        operations
    }

    /// Request one leaf through the retail ancestor-selection policy.
    /// An unknown ID returns an error without changing any active flag.
    pub fn request(&mut self, level: u8) -> Result<Vec<LeafOperation>> {
        let node = self.leaves[usize::from(level)].ok_or(SceneTreeError::UnknownScene { level })?;
        let mut operations = Vec::new();
        // FUN_004EE1E0, 004EE1E9..004EE1F3: notify the parent even when the
        // requested leaf is active. Do not turn this into an early return.
        if let Some(parent) = self.nodes[node].parent {
            self.select_descendant(parent, node, &mut operations);
        }
        self.activate(node, &mut operations);
        Ok(operations)
    }

    /// Resident leaves in authored tree order, including zero-request leaves.
    /// This is an inventory, not load order or global registration precedence.
    pub fn active_leaves(&self) -> Vec<u8> {
        self.nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Leaf(level) if node.active => Some(*level),
                _ => None,
            })
            .collect()
    }

    fn append_node(&mut self, source: &SceneNode, parent: Option<NodeId>) -> Result<NodeId> {
        let id = self.nodes.len();
        let kind = match source {
            SceneNode::Leaf(leaf) => {
                let level = leaf.overlay_id;
                if self.leaves[usize::from(level)].replace(id).is_some() {
                    return Err(SceneTreeError::DuplicateScene { level });
                }
                NodeKind::Leaf(level)
            }
            SceneNode::Branch { branch_kind, .. } => NodeKind::Branch {
                policy: *branch_kind,
                children: Vec::new(),
            },
        };
        // The node base (004EE210): inactive, under its parent.
        self.nodes.push(Node {
            parent,
            kind,
            active: false,
        });
        if let SceneNode::Branch { children, .. } = source {
            let children = children
                .iter()
                .map(|child| self.append_node(child, Some(id)))
                .collect::<Result<Vec<_>>>()?;
            let NodeKind::Branch {
                children: target, ..
            } = &mut self.nodes[id].kind
            else {
                unreachable!("source and constructed node have the same kind");
            };
            *target = children;
        }
        Ok(id)
    }

    /// Invoke virtual load `+0x04` and set the active byte after it returns.
    fn activate(&mut self, id: NodeId, operations: &mut Vec<LeafOperation>) {
        if self.nodes[id].active {
            return;
        }
        match self.nodes[id].kind.clone() {
            NodeKind::Leaf(level) => operations.push(LeafOperation::Load { level }),
            NodeKind::Branch { policy, children } => match policy {
                // FUN_004EEE70: every child, in authored order.
                BranchKind::Group => {
                    for child in children {
                        self.activate(child, operations);
                    }
                }
                // FUN_004EED50: only the first child, even when it is empty.
                BranchKind::Select => {
                    if let Some(child) = children.first() {
                        self.activate(*child, operations);
                    }
                }
            },
        }
        self.nodes[id].active = true;
    }

    /// Invoke virtual unload `+0x08` and clear the active byte afterwards.
    fn deactivate(&mut self, id: NodeId, operations: &mut Vec<LeafOperation>) {
        if !self.nodes[id].active {
            return;
        }
        match self.nodes[id].kind.clone() {
            NodeKind::Leaf(level) => operations.push(LeafOperation::Unload { level }),
            NodeKind::Branch { children, .. } => {
                // FUN_004EEBF0 is shared by both branch kinds. It visits
                // active children forward, not in reverse allocation order.
                for child in children {
                    self.deactivate(child, operations);
                }
            }
        }
        self.nodes[id].active = false;
    }

    /// Virtual `+0x18`: prepare this parent for the requested direct child.
    fn select_descendant(
        &mut self,
        id: NodeId,
        requested_child: NodeId,
        operations: &mut Vec<LeafOperation>,
    ) {
        let NodeKind::Branch { policy, children } = self.nodes[id].kind.clone() else {
            unreachable!("a leaf cannot be another node's parent");
        };
        match policy {
            BranchKind::Group => {
                // FUN_004EEEB0 notifies its parent unconditionally, including
                // when this group is already active. Siblings load only after
                // that call, and this group's active byte is set last.
                if let Some(parent) = self.nodes[id].parent {
                    self.select_descendant(parent, id, operations);
                }
                for child in children {
                    if child != requested_child {
                        self.activate(child, operations);
                    }
                }
                self.nodes[id].active = true;
            }
            BranchKind::Select if self.nodes[id].active => {
                // FUN_004EED80's active branch does not notify its parent.
                for child in children {
                    if child != requested_child {
                        self.deactivate(child, operations);
                    }
                }
            }
            BranchKind::Select => {
                // Inactive Select: notify parent and mark self active, but
                // never run the ordinary first-child load method here.
                if let Some(parent) = self.nodes[id].parent {
                    self.select_descendant(parent, id, operations);
                }
                self.nodes[id].active = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{HandlerRequest, OverlayLeaf, PreloadHeader, RequestTable, HANDLER_COUNT};

    use super::*;
    use LeafOperation::{Load, Unload};

    fn leaf(level: u8) -> SceneNode {
        let requests = RequestTable(std::array::from_fn(|_| HandlerRequest {
            count: 0,
            segments: Vec::new(),
        }));
        SceneNode::Leaf(Box::new(OverlayLeaf {
            overlay_id: level,
            base: requests.clone(),
            difficulty: requests.clone(),
            language: requests.clone(),
            difficulty_language: requests,
        }))
    }

    fn group(children: Vec<SceneNode>) -> SceneNode {
        SceneNode::Branch {
            branch_kind: BranchKind::Group,
            children,
        }
    }

    fn select(children: Vec<SceneNode>) -> SceneNode {
        SceneNode::Branch {
            branch_kind: BranchKind::Select,
            children,
        }
    }

    fn preload(root: SceneNode) -> Preload {
        Preload {
            header: PreloadHeader {
                difficulty_flag: 0,
                language_flag: 0,
                handler_capacities: [0; HANDLER_COUNT],
                handler_3_allocation: 0,
                node_allocation: 0,
            },
            root,
        }
    }

    fn retail_tree() -> SceneTree {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/PreLoad.dat");
        let bytes = std::fs::read(path).expect("retail PreLoad.dat is required");
        SceneTree::new(&Preload::parse(&bytes).unwrap()).unwrap()
    }

    #[test]
    fn boot_keeps_authored_order_and_only_the_first_select_child() {
        let mut tree = SceneTree::new(&preload(group(vec![
            group(vec![leaf(4), leaf(3)]),
            select(vec![group(vec![leaf(8), leaf(9)]), leaf(7)]),
            leaf(0),
        ])))
        .unwrap();
        assert_eq!(
            tree.boot(),
            [
                Load { level: 4 },
                Load { level: 3 },
                Load { level: 8 },
                Load { level: 9 },
                Load { level: 0 },
            ]
        );
        assert_eq!(tree.active_leaves(), [4, 3, 8, 9, 0]);
        let before = tree.clone();
        assert!(tree.boot().is_empty());
        assert!(tree.request(9).unwrap().is_empty());
        assert_eq!(tree, before);
    }

    #[test]
    fn cold_descendant_request_loads_ancestor_siblings_before_requested_leaf() {
        let mut tree = SceneTree::new(&preload(group(vec![
            leaf(90),
            select(vec![
                leaf(80),
                group(vec![select(vec![leaf(10), leaf(11)]), leaf(20)]),
            ]),
            leaf(91),
        ])))
        .unwrap();
        assert_eq!(
            tree.request(11).unwrap(),
            [
                Load { level: 90 },
                Load { level: 91 },
                Load { level: 20 },
                Load { level: 11 },
            ]
        );
        // Inventory order intentionally differs from activation order.
        assert_eq!(tree.active_leaves(), [90, 11, 20, 91]);
        assert!(tree.boot().is_empty());
        assert_eq!(
            tree.request(10).unwrap(),
            [Unload { level: 11 }, Load { level: 10 }]
        );
        assert_eq!(tree.active_leaves(), [90, 10, 20, 91]);
    }

    #[test]
    fn retail_boot_frontend_empty_loading_world_and_return_preserve_exact_order() {
        let mut tree = retail_tree();
        assert_eq!(tree.boot(), [Load { level: 0 }, Load { level: 2 }]);
        assert_eq!(
            tree.request(47).unwrap(),
            [Unload { level: 2 }, Load { level: 47 }]
        );
        assert_eq!(
            tree.request(48).unwrap(),
            [Unload { level: 47 }, Load { level: 1 }, Load { level: 48 },]
        );
        assert_eq!(tree.active_leaves(), [0, 1, 48]);
        assert_eq!(
            tree.request(11).unwrap(),
            [Unload { level: 48 }, Load { level: 6 }, Load { level: 11 },]
        );
        assert_eq!(tree.active_leaves(), [0, 1, 11, 6]);
        assert_eq!(
            tree.request(19).unwrap(),
            [Unload { level: 11 }, Load { level: 19 }]
        );
        assert_eq!(
            tree.request(12).unwrap(),
            [
                Unload { level: 19 },
                Unload { level: 6 },
                Load { level: 9 },
                Load { level: 12 },
            ]
        );
        assert_eq!(tree.active_leaves(), [0, 1, 12, 9]);
        assert_eq!(
            tree.request(47).unwrap(),
            [
                Unload { level: 1 },
                Unload { level: 12 },
                Unload { level: 9 },
                Load { level: 47 },
            ]
        );
        assert_eq!(tree.active_leaves(), [0, 47]);
        assert!(tree.request(47).unwrap().is_empty());
        assert!(tree.request(0).unwrap().is_empty());
        assert_eq!(
            tree.request(2).unwrap(),
            [Unload { level: 47 }, Load { level: 2 }]
        );
        assert_eq!(tree.active_leaves(), [0, 2]);
    }

    #[test]
    fn requesting_shared_world_bank_activates_default_siblings_before_the_bank() {
        let mut tree = retail_tree();
        tree.boot();
        assert_eq!(
            tree.request(1).unwrap(),
            [
                Unload { level: 2 },
                Load { level: 11 },
                Load { level: 6 },
                Load { level: 1 },
            ]
        );
        assert_eq!(tree.active_leaves(), [0, 1, 11, 6]);
        assert!(tree.request(1).unwrap().is_empty());
    }

    #[test]
    fn empty_first_branch_is_selected_and_does_not_fall_through_to_a_leaf() {
        let mut tree = SceneTree::new(&preload(group(vec![
            select(vec![group(Vec::new()), leaf(7)]),
            group(Vec::new()),
            select(Vec::new()),
            leaf(48),
        ])))
        .unwrap();
        assert_eq!(tree.boot(), [Load { level: 48 }]);
        assert_eq!(tree.active_leaves(), [48]);
        assert_eq!(tree.nodes.iter().filter(|node| node.active).count(), 6);
        assert_eq!(tree.request(7).unwrap(), [Load { level: 7 }]);
        assert_eq!(tree.active_leaves(), [7, 48]);
        assert!(!tree.nodes[2].active); // Empty former Select child was unloaded.
        assert!(tree.request(48).unwrap().is_empty());
    }

    #[test]
    fn cloned_plans_and_unknown_requests_preserve_the_original_state() {
        let mut tree = retail_tree();
        tree.boot();
        let before = tree.clone();
        assert!(matches!(
            tree.request(44),
            Err(SceneTreeError::UnknownScene { level: 44 })
        ));
        assert_eq!(tree, before);

        let mut staged = tree.clone();
        assert_eq!(
            staged.request(47).unwrap(),
            [Unload { level: 2 }, Load { level: 47 }]
        );
        assert_eq!(tree, before);
        assert_eq!(tree.active_leaves(), [0, 2]);
        assert_eq!(staged.active_leaves(), [0, 47]);
    }

    #[test]
    fn duplicate_leaf_ids_are_rejected_without_an_ambiguous_lookup() {
        assert!(matches!(
            SceneTree::new(&preload(group(vec![leaf(3), leaf(3)]))),
            Err(SceneTreeError::DuplicateScene { level: 3 })
        ));
    }
}
