use serde::Serialize;

use crate::reader::Reader;
use crate::{Error, Result};

pub const HANDLER_COUNT: usize = 16;
const MAX_SCENE_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Preload {
    pub header: PreloadHeader,
    pub root: SceneNode,
}

impl Preload {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(data);
        let difficulty_flag = reader.u8("difficulty flag")?;
        let language_flag = reader.u8("language flag")?;
        let mut handler_capacities = [0; HANDLER_COUNT];
        for capacity in &mut handler_capacities {
            *capacity = reader.u16("handler capacity")?;
        }
        let header = PreloadHeader {
            difficulty_flag,
            language_flag,
            handler_capacities,
            handler_3_allocation: reader.u32("handler 3 allocation size")?,
            node_allocation: reader.u8("scene-node allocation count")?,
        };

        // FUN_004EDEA0 calls FUN_004EF490 directly: vtable 0x00529290,
        // whose load method FUN_004EEE70 activates every child in order.
        let root = parse_branch(&mut reader, BranchKind::Group, 0)?;
        if reader.remaining() != 0 {
            return Err(Error::TrailingPreloadBytes {
                offset: reader.offset(),
                remaining: reader.remaining(),
            });
        }
        Ok(Self { header, root })
    }

    pub fn leaves(&self) -> impl Iterator<Item = &OverlayLeaf> {
        self.root.leaves()
    }

    pub fn leaf(&self, overlay_id: u8) -> Option<&OverlayLeaf> {
        self.leaves().find(|leaf| leaf.overlay_id == overlay_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreloadHeader {
    pub difficulty_flag: u8,
    pub language_flag: u8,
    pub handler_capacities: [u16; HANDLER_COUNT],
    pub handler_3_allocation: u32,
    pub node_allocation: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchKind {
    /// All children coexist (tag 2, plus the implicit root). Retail vtable
    /// 0x00529290 uses FUN_004EEE70 to load every child and FUN_004EEEB0 to
    /// activate the siblings of a specifically requested descendant.
    Group,
    /// One child is active (tag 1). Retail vtable 0x00529274 uses
    /// FUN_004EED50 to load the first child and FUN_004EED80 to unload
    /// active siblings when another descendant is requested.
    Select,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SceneNode {
    Branch {
        branch_kind: BranchKind,
        children: Vec<SceneNode>,
    },
    Leaf(Box<OverlayLeaf>),
}

impl SceneNode {
    pub fn leaves(&self) -> Leaves<'_> {
        Leaves { stack: vec![self] }
    }
}

pub struct Leaves<'a> {
    stack: Vec<&'a SceneNode>,
}

impl<'a> Iterator for Leaves<'a> {
    type Item = &'a OverlayLeaf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            match node {
                SceneNode::Leaf(leaf) => return Some(leaf.as_ref()),
                SceneNode::Branch { children, .. } => {
                    self.stack.extend(children.iter().rev());
                }
            }
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OverlayLeaf {
    pub overlay_id: u8,
    pub base: RequestTable,
    pub difficulty: RequestTable,
    pub language: RequestTable,
    pub difficulty_language: RequestTable,
}

impl OverlayLeaf {
    pub const fn requests(&self, kind: RequestKind) -> &RequestTable {
        match kind {
            RequestKind::Base => &self.base,
            RequestKind::Difficulty => &self.difficulty,
            RequestKind::Language => &self.language,
            RequestKind::DifficultyLanguage => &self.difficulty_language,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Base,
    Difficulty,
    Language,
    DifficultyLanguage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RequestTable(pub [HandlerRequest; HANDLER_COUNT]);

impl RequestTable {
    pub fn get(&self, handler: usize) -> Option<&HandlerRequest> {
        self.0.get(handler)
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &HandlerRequest)> {
        self.0.iter().enumerate()
    }

    pub fn has_active_handlers(&self) -> bool {
        self.0.iter().any(|request| request.count != 0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HandlerRequest {
    pub count: i16,
    pub segments: Vec<ResourceSegment>,
}

impl HandlerRequest {
    /// Resolve one handler-local index exactly as `FUN_004EFC00` registers it.
    ///
    /// The executable selects the final segment whose local threshold does not
    /// exceed the index, then adds that segment's signed high-word delta. This
    /// is not an absolute global base: later segments can deliberately map
    /// local entries backwards onto an existing global resource.
    pub fn global_index(&self, local_index: u16) -> Option<u16> {
        let segment = self
            .segments
            .iter()
            .rev()
            .find(|segment| segment.local_start <= local_index)?;
        let global_index = i32::from(local_index) + i32::from(segment.global_delta);
        u16::try_from(global_index).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceSegment {
    pub local_start: u16,
    /// Signed value added to a matching local resource index by
    /// `FUN_004EFC00`. The old Python extractor mislabeled this as an absolute
    /// global base, which is only accidentally true when `local_start == 0`.
    pub global_delta: i16,
}

impl ResourceSegment {
    pub const fn packed(self) -> u32 {
        self.local_start as u32 | ((self.global_delta as u16 as u32) << 16)
    }
}

/// A branch as retail reads it: `FUN_004EF490` (Group) and `FUN_004EF7E0`
/// (Select) read the child count, then a kind byte per child; under a Select,
/// `FUN_004EFB30` dispatches that byte (0 a leaf, 1 a Select, 2 a Group).
fn parse_branch(reader: &mut Reader<'_>, kind: BranchKind, depth: usize) -> Result<SceneNode> {
    if depth >= MAX_SCENE_DEPTH {
        return Err(Error::SceneTreeTooDeep {
            max_depth: MAX_SCENE_DEPTH,
        });
    }

    let child_count = reader.u8("scene-node child count")?;
    let mut children = Vec::with_capacity(child_count as usize);
    for _ in 0..child_count {
        let kind_offset = reader.offset();
        let child_kind = reader.u8("scene-node child kind")?;
        let child = match child_kind {
            0 => parse_leaf(reader)?,
            1 => parse_branch(reader, BranchKind::Select, depth + 1)?,
            2 => parse_branch(reader, BranchKind::Group, depth + 1)?,
            kind => {
                return Err(Error::InvalidNodeKind {
                    offset: kind_offset,
                    kind,
                })
            }
        };
        children.push(child);
    }
    Ok(SceneNode::Branch {
        branch_kind: kind,
        children,
    })
}

/// `FUN_004EF0E0` (and the Group reader inline): the overlay byte, then the
/// base, difficulty, language and combined request tables, which the leaf
/// node `004EE260` takes (with an output array for each).
fn parse_leaf(reader: &mut Reader<'_>) -> Result<SceneNode> {
    Ok(SceneNode::Leaf(Box::new(OverlayLeaf {
        overlay_id: reader.u8("leaf overlay id")?,
        base: parse_request_table(reader)?,
        difficulty: parse_request_table(reader)?,
        language: parse_request_table(reader)?,
        difficulty_language: parse_request_table(reader)?,
    })))
}

/// `FUN_004EEFC0`: sixteen handler entries, each as `FUN_004EEF30` reads it
/// (a signed WORD count, a WORD segment count and that many DWORD segments).
fn parse_request_table(reader: &mut Reader<'_>) -> Result<RequestTable> {
    let mut requests = Vec::with_capacity(HANDLER_COUNT);
    for _ in 0..HANDLER_COUNT {
        let count = reader.i16("handler request count")?;
        let segment_count = reader.u16("handler segment count")? as usize;
        let mut segments = Vec::with_capacity(segment_count);
        for _ in 0..segment_count {
            let packed = reader.u32("handler resource segment")?;
            segments.push(ResourceSegment {
                local_start: packed as u16,
                global_delta: (packed >> 16) as u16 as i16,
            });
        }
        requests.push(HandlerRequest { count, segments });
    }
    Ok(RequestTable(
        requests
            .try_into()
            .expect("request parser always emits exactly 16 handlers"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_preload_and_preserves_zero_segments() {
        let mut data = vec![0, 0];
        data.extend_from_slice(&[0; HANDLER_COUNT * 2]);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.push(1);
        data.push(1); // root child count
        data.push(0); // leaf
        data.push(7); // overlay id
        for table in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let count = if table == 0 && handler == 3 { 2i16 } else { 0 };
                data.extend_from_slice(&count.to_le_bytes());
                let segment_count = usize::from(table == 0 && handler == 3);
                data.extend_from_slice(&(segment_count as u16).to_le_bytes());
                if segment_count == 1 {
                    data.extend_from_slice(&0u32.to_le_bytes());
                }
            }
        }

        let preload = Preload::parse(&data).unwrap();
        let leaf = preload.leaf(7).unwrap();
        let request = leaf.base.get(3).unwrap();
        assert_eq!(request.count, 2);
        assert_eq!(request.segments.len(), 1);
        assert_eq!(request.segments[0].packed(), 0);
    }

    #[test]
    fn rejects_trailing_preload_data() {
        let mut data = vec![0, 0];
        data.extend_from_slice(&[0; HANDLER_COUNT * 2]);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.push(0);
        data.push(0); // empty root group node
        data.push(0xaa);

        assert_eq!(
            Preload::parse(&data).unwrap_err(),
            Error::TrailingPreloadBytes {
                offset: data.len() - 1,
                remaining: 1,
            }
        );
    }

    #[test]
    fn resource_segments_apply_the_executables_signed_delta_mapping() {
        let request = HandlerRequest {
            count: 47,
            // Retail 1.ovl handler 9: local 0..37 append at global 30, local
            // 38 aliases global 19, and local 39..46 append at global 68.
            segments: vec![
                ResourceSegment {
                    local_start: 0,
                    global_delta: 30,
                },
                ResourceSegment {
                    local_start: 38,
                    global_delta: -19,
                },
                ResourceSegment {
                    local_start: 39,
                    global_delta: 29,
                },
            ],
        };

        assert_eq!(request.global_index(0), Some(30));
        assert_eq!(request.global_index(37), Some(67));
        assert_eq!(request.global_index(38), Some(19));
        assert_eq!(request.global_index(39), Some(68));
        assert_eq!(request.global_index(46), Some(75));
        assert_eq!(request.segments[1].packed(), 0xFFED_0026);
    }
}
