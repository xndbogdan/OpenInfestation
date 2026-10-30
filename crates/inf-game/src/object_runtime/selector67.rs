//! Selector67 ribbon 005286D8, the Selector34 engine trail: the base class
//! with its own destructor 00486570/00486590 (which frees the nodes), update
//! +24 004866A0 and draw +2C 00486B30. Selector34 motion creates it through
//! 00486600 (constructor 00486530, then 004600F0 admission with H13 resource
//! 4) and drives it each frame through 004869B0.

use inf_formats::{native_basis::isqrt_48efc0, FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{
    ActorHandle, ObjectArena, ObjectClass, ObjectConstructionError, ObjectConstructionOutcome,
    ObjectError, ObjectId, ObjectList, ObjectNodeKind,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::ResidentRuntime;

mod draw;
pub use draw::{
    ribbon_sort_key, ribbon_sort_word, RibbonCorner, RibbonDrawError, RibbonDrawInput, RibbonQuad,
    RibbonSortError, RIBBON_MODE_0053C568,
};

#[cfg(test)]
mod tests;

/// .data DWORDs that only these functions read: the segment length (200)
/// and the jump that splits a ribbon (750).
const SEGMENT_0053C558: u32 = 0x0053_c558;
const SPLIT_0053C55C: u32 = 0x0053_c55c;

/// Selector67PositionApproximation: the position +32 of a ribbon
/// `00486600(process NULL)` creates. Its constructor `00486530` never writes
/// it; a ship's ribbon takes its head point at its first `004869B0`, a
/// split's never, yet the ribbon's own +24 reads it for the radius +28 and
/// the world draw's distance test for visibility, a new ship's ribbon from
/// the first frame. Infest03 and Infest04 split 23 times
/// (`ribbon-split.js`): retail's +32 held allocator residue, halves of heap
/// pointers from the block's earlier use such as `(0, F030, 37D)` and
/// `(392, 1474, 392)`, and zero in three. The port starts it at zero, a
/// fresh heap's value. Before this, a split ribbon's update stopped there
/// and its trail stayed in the world, and a new ship's ribbon was left out
/// of the draw until its first point.
pub const SELECTOR67_POSITION_APPROXIMATION: [u16; 3] = [0; 3];

fn log_position_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("Selector67PositionApproximation: 00486600 leaves a ribbon's position +32 to allocator residue; the port starts it at 0 (SELECTOR67_RUNTIME.md)");
    }
}

/// One 0xC-byte node: +0 next, +4 point, +A length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RibbonNode {
    pub point: [u16; 3],
    /// The length credited to the segment from this node to the next.
    pub length: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector67State {
    /// +40: the floor of +48.
    pub word_40: u16,
    /// +42: subtracted from the length +48 eases toward; cleared once.
    pub word_42: u16,
    /// +44: the draw's texture selector; nonzero prevents a split.
    pub word_44: u16,
    /// +46: countdown, reset to 2 by every ship update.
    pub word_46: u16,
    /// +48: the eased length the tail recedes by.
    pub dword_48: u32,
    /// +4C: the node list, head (newest) first; empty is NULL.
    pub nodes: Vec<RibbonNode>,
    /// +50: the owner; the process NULL until admission stores the source.
    pub owner_50: ActorHandle,
    /// +54: the total length, then the length the cut keeps.
    pub dword_54: u32,
}

impl Selector67State {
    /// 00486530's fields.
    pub(super) fn new(process_null: ActorHandle) -> Self {
        Self {
            word_40: 0x96,
            word_42: 0xdac,
            word_44: 0,
            word_46: 2,
            dword_48: 0xfa,
            nodes: Vec::new(),
            owner_50: process_null,
            dword_54: 0,
        }
    }
}

pub trait Selector67Host {
    type Error;
    /// 00486600 with the current process NULL as the source: a new admitted
    /// ribbon, or NULL.
    fn split_ribbon(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error>;
    /// 0047AD90: whether the owner handle still resolves.
    fn owner_resolves(
        &mut self,
        arena: &mut ObjectArena,
        owner: ActorHandle,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector67Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    /// A split ribbon keeps 00486600's unwritten +32, which its +24 reads.
    #[error("selector67 ribbon {0:?} reads its unwritten position +32")]
    UnwrittenPosition(ObjectId),
    #[error("selector67 ribbon service at {address:08X} failed: {error}")]
    Service { address: u32, error: E },
}

/// H13 resource 00486600 admits the ribbon with.
const RIBBON_RESOURCE: u32 = 4;

pub struct Selector67FactoryRequest<'a> {
    pub runtime: &'a ResidentRuntime,
    pub process: &'a ParticleFactoryProcessState,
    /// 0055F448: a nonzero flag also runs the base +40 0047E320.
    pub network_active: bool,
}

#[derive(Debug, Error)]
pub enum Selector67FactoryError {
    #[error("selector67 ribbon construction requires the current process NULL")]
    MissingProcessNull,
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("selector67 ribbon {object:?} admission stopped after publication: {source}")]
    Admission {
        object: ObjectId,
        #[source]
        source: ObjectConstructionError,
    },
}

/// 00486600: new 0x5C and 00486530 (the ribbon's fields, +50 the process
/// NULL), then 004600F0 mode 0 with H13 resource 4. A rejected admission
/// deletes the ribbon (virtual +00 with 1) and returns NULL; otherwise +50
/// takes the source. Allocation itself never fails here.
pub fn construct_selector67_ribbon(
    arena: &mut ObjectArena,
    request: &Selector67FactoryRequest<'_>,
    source: ActorHandle,
) -> Result<Option<ObjectId>, Selector67FactoryError> {
    let process_null = request
        .process
        .process_null_handle
        .ok_or(Selector67FactoryError::MissingProcessNull)?;
    let ribbon = arena.allocate_selector67(process_null);
    let outcome = super::construction::admit_constructed_selector67(
        arena,
        request.runtime,
        ribbon,
        RIBBON_RESOURCE,
        request.network_active,
    )
    .map_err(|source| Selector67FactoryError::Admission {
        object: ribbon,
        source,
    })?;
    match outcome {
        ObjectConstructionOutcome::Constructed(_) => {
            arena.set_selector67_owner(ribbon, source)?;
            log_position_approximation();
            arena.owned_mut(ribbon)?.state.base.position_words =
                Some(SELECTOR67_POSITION_APPROXIMATION);
            Ok(Some(ribbon))
        }
        ObjectConstructionOutcome::Rejected { .. } => {
            arena.destroy(super::ObjectDestructionRequest {
                object: ribbon,
                sound: super::SoundTeardownService::RequiredAtReach,
            })?;
            Ok(None)
        }
    }
}

/// Wrapped DWORD sum of the squared signed WORD differences.
fn squared_distance(from: [u16; 3], to: [u16; 3]) -> i32 {
    (0..3).fold(0_i32, |sum, axis| {
        let difference = i32::from(from[axis].wrapping_sub(to[axis]) as i16);
        sum.wrapping_add(difference.wrapping_mul(difference))
    })
}

impl ObjectArena {
    /// 00479C60 and 00486530; admission belongs to the caller.
    pub(crate) fn allocate_selector67(&mut self, process_null: ActorHandle) -> ObjectId {
        let object = self.allocate(super::ObjectClassKind::Base);
        self.owned_mut(object)
            .expect("fresh allocation")
            .state
            .class = ObjectClass::Selector67(Selector67State::new(process_null));
        object
    }

    pub fn selector67(&self, object: ObjectId) -> Result<&Selector67State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector67(state) => Ok(state),
            _ => Err(ObjectError::NotSelector67(object)),
        }
    }

    /// WORD +40 of a ribbon, which selector 62's state 2 clears (004643ED).
    pub(in crate::object_runtime) fn set_selector67_word_40(
        &mut self,
        ribbon: ObjectId,
        value: u16,
    ) -> Result<(), ObjectError> {
        self.selector67_mut(ribbon)?.word_40 = value;
        Ok(())
    }

    /// WORD +44 of a ribbon, which selector 62 sets on each it raises
    /// (004643C3).
    pub(in crate::object_runtime) fn set_selector67_word_44(
        &mut self,
        ribbon: ObjectId,
        value: u16,
    ) -> Result<(), ObjectError> {
        self.selector67_mut(ribbon)?.word_44 = value;
        Ok(())
    }

    fn selector67_mut(&mut self, object: ObjectId) -> Result<&mut Selector67State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector67(state) => Ok(state),
            _ => Err(ObjectError::NotSelector67(object)),
        }
    }

    pub(crate) fn set_selector67_owner(
        &mut self,
        object: ObjectId,
        owner: ActorHandle,
    ) -> Result<(), ObjectError> {
        self.selector67_mut(object)?.owner_50 = owner;
        Ok(())
    }

    /// 00479040: a flag-1 object moves to the end of the alternate list,
    /// so the current pass no longer reaches it.
    pub fn append_flag1_alternate(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        if self.object(object)?.base.flags & 2 != 0 {
            self.move_node(object, ObjectNodeKind::Flag1, ObjectList::Flag1Secondary);
        }
        Ok(())
    }

    /// 004868F0: the list becomes two nodes at the point, and so does +32.
    fn selector67_first_points(
        &mut self,
        ribbon: ObjectId,
        point: [u16; 3],
    ) -> Result<(), ObjectError> {
        self.selector67_mut(ribbon)?.nodes = vec![RibbonNode { point, length: 0 }; 2];
        self.owned_mut(ribbon)?.state.base.position_words = Some(point);
        Ok(())
    }

    fn selector67_position<E>(&self, ribbon: ObjectId) -> Result<[u16; 3], Selector67Error<E>> {
        self.object(ribbon)?
            .base
            .position_words
            .ok_or(Selector67Error::UnwrittenPosition(ribbon))
    }

    /// 004869B0, Selector34 motion's call with the ship's position. Within
    /// 200 the head follows the ship; beyond, +32 takes the head's point and
    /// a new head is pushed. Beyond 750 with +44 clear, a new ribbon takes
    /// the old nodes and this one restarts at the ship. Then the own +24.
    pub fn update_selector67_from_ship<H: Selector67Host>(
        &mut self,
        ribbon: ObjectId,
        microseconds: u32,
        point: [u16; 3],
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> Result<(), Selector67Error<H::Error>> {
        self.selector67(ribbon)?;
        self.append_flag1_alternate(ribbon)?;
        self.selector67_mut(ribbon)?.word_46 = 2;
        if self.selector67(ribbon)?.nodes.is_empty() {
            self.selector67_first_points(ribbon, point)?;
        }
        let squared = squared_distance(self.selector67_position(ribbon)?, point);
        // A sum with bit 31 set is quartered before the root, then doubled.
        let distance = if squared < 0 {
            isqrt_48efc0(((squared as u32) >> 2) as i32) << 1
        } else {
            isqrt_48efc0(squared)
        } as i32;
        if distance <= executable.word(SEGMENT_0053C558)? as i32 {
            let head = &mut self.selector67_mut(ribbon)?.nodes[0];
            head.point = point;
            head.length = distance as u16;
        } else {
            if distance > executable.word(SPLIT_0053C55C)? as i32
                && self.selector67(ribbon)?.word_44 == 0
            {
                let created =
                    host.split_ribbon(self)
                        .map_err(|error| Selector67Error::Service {
                            address: 0x0048_6a72,
                            error,
                        })?;
                if let Some(created) = created {
                    let old = self.selector67_mut(ribbon)?;
                    let nodes = std::mem::take(&mut old.nodes);
                    let (word_44, dword_48, word_40) = (old.word_44, old.dword_48, old.word_40);
                    let new = self.selector67_mut(created)?;
                    new.nodes = nodes;
                    new.word_44 = word_44;
                    new.dword_48 = dword_48;
                    new.word_40 = word_40;
                    log_position_approximation();
                    self.owned_mut(created)?.state.base.position_words =
                        Some(SELECTOR67_POSITION_APPROXIMATION);
                }
                // Without a new ribbon the old nodes are freed here.
                self.selector67_first_points(ribbon, point)?;
            }
            let head = self.selector67(ribbon)?.nodes[0].point;
            self.owned_mut(ribbon)?.state.base.position_words = Some(head);
            self.selector67_mut(ribbon)?.nodes.insert(
                0,
                RibbonNode {
                    point,
                    length: distance as u16,
                },
            );
        }
        self.tick_selector67(ribbon, microseconds, executable, host)
    }

    /// 004866A0, virtual +24. Fewer than two nodes only reach the owner
    /// check. Otherwise the total length and the largest squared distance
    /// from +32 (at least 64) give +54 and the radius +28; the countdown
    /// +46, then the one-shot +42 (raising +40 to 200), then a total under
    /// 400 (disposal) are tested in turn. +48 eases by (total - +42) and is
    /// clamped to [+40, total]; the tail recedes by +48 scaled by the
    /// interval, moving the node at the cut and freeing the rest (00486500).
    /// Only a cut that walks off the end reaches the owner check.
    pub fn tick_selector67<H: Selector67Host>(
        &mut self,
        ribbon: ObjectId,
        microseconds: u32,
        executable: FrontendExecutable<'_>,
        host: &mut H,
    ) -> Result<(), Selector67Error<H::Error>> {
        self.selector67_mut(ribbon)?.dword_54 = 0;
        match self.selector67(ribbon)?.nodes.len() {
            0 => {
                self.owned_mut(ribbon)?.state.base.word_28 = 0;
                return self.selector67_owner_check(ribbon, host);
            }
            1 => return self.selector67_owner_check(ribbon, host),
            _ => {}
        }
        let position = self.selector67_position(ribbon)?;
        let mut total = 0_u32;
        let mut spread = 0x40_u32;
        for node in &self.selector67(ribbon)?.nodes {
            total = total.wrapping_add(u32::from(node.length));
            spread = spread.max(squared_distance(position, node.point) as u32);
        }
        let segment = executable.word(SEGMENT_0053C558)? as i32;
        let state = self.selector67_mut(ribbon)?;
        state.dword_54 = total;
        if state.word_46 != 0 {
            state.word_46 -= 1;
        } else if state.word_42 != 0 {
            state.word_42 = 0;
            if segment > i32::from(state.word_40) {
                state.word_40 = segment as u16;
            }
        } else if (total as i32 >> 1) < segment {
            // 00479FA0 queues disposal; the update continues.
            self.dispose(ribbon)?;
        }
        let step = (microseconds >> 5) as i32;
        let state = self.selector67_mut(ribbon)?;
        let ease = (total as i32)
            .wrapping_sub(i32::from(state.word_42 as i16))
            .wrapping_mul(step)
            >> 15;
        let eased = (state.dword_48 as i32).wrapping_add(i32::from(ease as i16));
        state.dword_48 = if eased < i32::from(state.word_40) {
            u32::from(state.word_40)
        } else if eased > total as i32 {
            total
        } else {
            eased as u32
        };
        let shrink = i32::from((step.wrapping_mul(state.dword_48 as i32) >> 15) as i16);
        let cut = (total as i32).wrapping_sub(shrink);
        let radius = isqrt_48efc0(spread as i32).wrapping_add(16);
        self.owned_mut(ribbon)?.state.base.word_28 = radius as u16;
        let state = self.selector67_mut(ribbon)?;
        let mut kept = 0_i32;
        let mut index = 0;
        let stopped = loop {
            if index >= state.nodes.len() {
                break None;
            }
            if kept >= cut {
                break Some(index);
            }
            let length = i32::from(state.nodes[index].length);
            kept = kept.wrapping_add(length);
            if kept < cut {
                index += 1;
                continue;
            }
            let overshoot = kept.wrapping_sub(cut);
            if overshoot >= length || length == 0 || index + 1 == state.nodes.len() {
                continue;
            }
            // The next node moves to the cut along this segment.
            let keep = length - overshoot;
            let from = state.nodes[index].point;
            let to = state.nodes[index + 1].point;
            state.nodes[index + 1].point = std::array::from_fn(|axis| {
                let offset = i32::from(to[axis].wrapping_sub(from[axis]) as i16);
                from[axis].wrapping_add((offset.wrapping_mul(keep) / length) as u16)
            });
            state.nodes[index].length = keep as u16;
            index += 1;
        };
        state.dword_54 = cut as u32;
        if let Some(index) = stopped {
            // 00486500: the stopping node ends the list with length 0.
            state.nodes[index].length = 0;
            state.nodes.truncate(index + 1);
            return Ok(());
        }
        self.selector67_owner_check(ribbon, host)
    }

    fn selector67_owner_check<H: Selector67Host>(
        &mut self,
        ribbon: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector67Error<H::Error>> {
        let owner = self.selector67(ribbon)?.owner_50;
        let resolves =
            host.owner_resolves(self, owner)
                .map_err(|error| Selector67Error::Service {
                    address: 0x0048_68cd,
                    error,
                })?;
        if !resolves {
            self.dispose(ribbon)?;
        }
        Ok(())
    }
}
