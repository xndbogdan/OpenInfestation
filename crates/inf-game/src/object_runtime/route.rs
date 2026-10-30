//! Selector-51 route-node custody and 00404D40's post-placement finalizer.
//! Pending order, group assignment and physical membership are distinct state.

use super::{BaseContext, ObjectArena, ObjectClass, ObjectError, ObjectId};
use crate::ResidentRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteGroupId {
    arena: u64,
    serial: u64,
}

#[cfg(test)]
#[path = "route_retail_tests.rs"]
mod retail_tests;

#[cfg(test)]
#[path = "route_native_tests.rs"]
mod native_tests;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteNode {
    /// Object +44: DWORD-wrapped squared segment length, not a distance.
    pub distance_squared: u32,
    /// Object +48. Bit0 wraps endpoints; meanings of bits1/4 remain open.
    pub flags: u8,
    /// Object +4C can already be assigned while node +50 is still pending.
    pub group: Option<RouteGroupId>,
    /// Object +5C, set separately from common placement WORD +2A.
    pub identity_word: u16,
    /// Object +5E: command3's one-based WORD minus one with WORD wrapping.
    pub predecessor_word: u16,
}

impl RouteNode {
    pub(super) fn new() -> Self {
        Self {
            distance_squared: 0,
            flags: 0,
            group: None,
            identity_word: u16::MAX,
            predecessor_word: u16::MAX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteGroup {
    pub id: RouteGroupId,
    /// 00404F90 leaves this byte untouched until the root is appended.
    pub flags: Option<u8>,
    pub members: Vec<ObjectId>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Routes {
    /// The pending list 005468F8, empty at startup (00404AF0, the CRT's jump
    /// 00404AE0).
    pub(super) pending: Vec<ObjectId>,
    /// The group list 005468E0, empty at startup (00404AB0, the CRT's jump
    /// 00404AA0).
    groups: Vec<RouteGroup>,
    next_serial: u64,
}

impl ObjectArena {
    pub fn route_node(&self, object: ObjectId) -> Result<&RouteNode, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::RouteNode { route, .. } => Ok(route),
            _ => Err(ObjectError::NotRouteNode(object)),
        }
    }

    pub(super) fn route_mut(&mut self, object: ObjectId) -> Result<&mut RouteNode, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::RouteNode { route, .. } => Ok(route),
            _ => Err(ObjectError::NotRouteNode(object)),
        }
    }

    pub fn route_pending(&self) -> &[ObjectId] {
        &self.routes.pending
    }

    /// Global group order follows 00404F90's tail insertion.
    pub fn route_groups(&self) -> &[RouteGroup] {
        &self.routes.groups
    }

    pub fn route_group(&self, id: RouteGroupId) -> Result<&RouteGroup, ObjectError> {
        self.routes
            .groups
            .iter()
            .find(|group| group.id == id)
            .ok_or(ObjectError::UnknownRouteGroup(id))
    }

    /// 00404C30 runs visual initialization before copying the placement WORD.
    pub fn initialize_route_context(
        &mut self,
        object: ObjectId,
        context: BaseContext,
        model_argument: Option<u32>,
        runtime: &ResidentRuntime,
    ) -> Result<(), ObjectError> {
        self.route_node(object)?;
        self.initialize_visual_context(object, context, model_argument, runtime)?;
        self.route_mut(object)?.identity_word = context.placement_index as u16;
        Ok(())
    }

    /// 00404D40, invoked by the loader after every placement is constructed.
    /// Errors stop at the reached frontier, retaining earlier native effects.
    /// In particular a predecessor cycle is not a native rejection/deletion.
    /// After failure, inspect and discard the whole arena; retries and further
    /// per-node route cleanup are not supported for that partial frontier.
    pub fn finalize_routes(&mut self) -> Result<(), ObjectError> {
        // Each native traversal saves its next pointer before moving/deleting
        // the current node. Snapshotting this pass's IDs preserves that order.
        for object in self.routes.pending.clone() {
            let route = self.route_node(object)?;
            if route.identity_word == u16::MAX {
                self.destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })?;
            } else if route.identity_word == route.predecessor_word {
                let group = self.allocate_route_group();
                self.route_mut(object)?.group = Some(group);
            }
        }
        for object in self.routes.pending.clone() {
            if self.resolve_route_group(object)?.is_none() {
                self.destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })?;
            }
        }
        loop {
            let mut moved = false;
            for object in self.routes.pending.clone() {
                moved |= self.append_route_node(object)?;
            }
            if !moved {
                break;
            }
        }
        while let Some(&object) = self.routes.pending.first() {
            self.destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })?;
        }
        self.calculate_route_segments()
    }

    /// 00404C50: ordinary next node, then optional group-head wrapping.
    pub fn route_next(&self, object: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        self.route_neighbor(object, true)
    }

    /// 00404C70: ordinary previous node, then optional group-tail wrapping.
    pub fn route_previous(&self, object: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        self.route_neighbor(object, false)
    }

    /// 00404F00 compares (reference - 1) as a full DWORD against the signed
    /// common placement WORD. This is not the raw-WORD predecessor search.
    pub fn find_route_node(&self, reference: u32) -> Option<ObjectId> {
        let wanted = reference.wrapping_sub(1) as i32;
        self.routes
            .groups
            .iter()
            .flat_map(|group| group.members.iter())
            .copied()
            .find(|&object| {
                self.object(object)
                    .expect("live route member")
                    .base
                    .placement_word as i16 as i32
                    == wanted
            })
    }

    fn route_neighbor(
        &self,
        object: ObjectId,
        forward: bool,
    ) -> Result<Option<ObjectId>, ObjectError> {
        let route = self.route_node(object)?;
        // Before finalization native ordinary next/previous can still traverse
        // pending nodes. Only an endpoint needs the possibly-unset group pointer.
        let members = if self.routes.pending.contains(&object) {
            &self.routes.pending
        } else {
            &self
                .route_group(route.group.ok_or(ObjectError::RouteUnfinalized(object))?)?
                .members
        };
        let index = members
            .iter()
            .position(|&id| id == object)
            .expect("live route membership");
        if forward && index + 1 < members.len() {
            return Ok(Some(members[index + 1]));
        }
        if !forward && index > 0 {
            return Ok(Some(members[index - 1]));
        }
        let group = self.route_group(route.group.ok_or(ObjectError::RouteUnfinalized(object))?)?;
        let flags = group.flags.ok_or(ObjectError::RouteUnfinalized(object))?;
        Ok(if flags & 1 == 0 {
            None
        } else if forward {
            group.members.first().copied()
        } else {
            group.members.last().copied()
        })
    }

    /// 00404F90 publishes an empty group without initializing its policy byte.
    fn allocate_route_group(&mut self) -> RouteGroupId {
        let id = RouteGroupId {
            arena: self.identity,
            serial: self.routes.next_serial,
        };
        self.routes.next_serial = self
            .routes
            .next_serial
            .checked_add(1)
            .expect("route group identity exhausted");
        self.routes.groups.push(RouteGroup {
            id,
            flags: None,
            members: Vec::new(),
        });
        id
    }

    /// 00404F40 follows the first matching identity in CURRENT pending order.
    /// An iterative path preserves recursive unwind stores without risking the
    /// host stack. Only ObjectId repetition is a cycle; duplicate WORDs are legal.
    fn resolve_route_group(
        &mut self,
        object: ObjectId,
    ) -> Result<Option<RouteGroupId>, ObjectError> {
        let mut path = Vec::new();
        let mut current = object;
        let resolved = loop {
            let route = self.route_node(current)?;
            if let Some(group) = route.group {
                self.route_group(group)?;
                break Some(group);
            }
            if path.contains(&current) {
                return Err(ObjectError::RouteCycle(current));
            }
            path.push(current);
            let predecessor = route.predecessor_word;
            let next = self.routes.pending.iter().copied().find(|&candidate| {
                self.route_node(candidate)
                    .expect("live pending route")
                    .identity_word
                    == predecessor
            });
            let Some(next) = next else { break None };
            current = next;
        };
        for object in path.into_iter().rev() {
            self.route_mut(object)?.group = resolved;
        }
        Ok(resolved)
    }

    /// 00405010 either moves one node to the group tail or leaves it pending.
    fn append_route_node(&mut self, object: ObjectId) -> Result<bool, ObjectError> {
        let route = self.route_node(object)?;
        let id = route.group.ok_or(ObjectError::RouteUnfinalized(object))?;
        let group = self.route_group(id)?;
        let is_root = route.identity_word == route.predecessor_word;
        let can_append = if is_root {
            group.members.is_empty()
        } else if let Some(&tail) = group.members.last() {
            self.route_node(tail)?.identity_word == route.predecessor_word
        } else {
            false
        };
        if !can_append {
            return Ok(false);
        }
        let flags = if is_root {
            route.flags
        } else {
            group.flags.ok_or(ObjectError::RouteUnfinalized(object))?
        };
        let index = self
            .routes
            .pending
            .iter()
            .position(|&id| id == object)
            .expect("pending route node");
        self.routes.pending.remove(index);
        let group = self
            .routes
            .groups
            .iter_mut()
            .find(|group| group.id == id)
            .unwrap();
        group.members.push(object);
        if is_root {
            group.flags = Some(flags);
        } else {
            self.route_mut(object)?.flags = flags;
        }
        Ok(true)
    }

    fn calculate_route_segments(&mut self) -> Result<(), ObjectError> {
        let objects: Vec<_> = self
            .routes
            .groups
            .iter()
            .flat_map(|group| &group.members)
            .copied()
            .collect();
        for object in objects {
            let Some(next) = self.route_next(object)? else {
                continue;
            };
            let position = |id| {
                self.object(id)?
                    .base
                    .position_words
                    .ok_or(ObjectError::RoutePositionUnknown(id))
            };
            let from = position(object)?;
            let to = position(next)?;
            let squared = from.into_iter().zip(to).fold(0_u32, |sum, (a, b)| {
                let delta = i32::from(a.wrapping_sub(b) as i16);
                sum.wrapping_add((delta * delta) as u32)
            });
            self.route_mut(object)?.distance_squared = if squared == 0 { 1 } else { squared };
        }
        Ok(())
    }

    /// 00404B90 unlinks node +50 first and deletes its group only if empty,
    /// before the common destructor unlinks the other three memberships.
    pub(super) fn unlink_route(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let group_id = self.route_node(object)?.group;
        if let Some(id) = group_id {
            self.route_group(id)?;
        }
        if let Some(index) = self.routes.pending.iter().position(|&id| id == object) {
            self.routes.pending.remove(index);
        } else if let Some(id) = group_id {
            let group = self
                .routes
                .groups
                .iter_mut()
                .find(|group| group.id == id)
                .unwrap();
            let index = group
                .members
                .iter()
                .position(|&id| id == object)
                .expect("route group member");
            group.members.remove(index);
        } else {
            unreachable!("live route node has a physical list")
        }
        if let Some(id) = group_id {
            if self.route_group(id)?.members.is_empty() {
                // 00404FD0 sees an empty list here, then unlinks the group.
                self.routes.groups.retain(|group| group.id != id);
            }
        }
        Ok(())
    }
}
