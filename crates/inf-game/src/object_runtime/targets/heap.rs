//! Retained native actor-service allocations. The portable reservation policy
//! owns real fallible backing; it does not reproduce511708 new-handler cadence,
//! Windows heap address/reuse history or unknown payload bytes.
use super::{NativeTarget, NativeTargetClass};
use crate::object_runtime::ObjectId;
use std::mem::MaybeUninit;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeHeapRef {
    arena: u64,
    serial: u64,
}

/// The eight DWORDs of a 0048D0D0 allocation. Float bits remain exact across
/// queue custody and the later 0048CDB0 direction adjustment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeWeaponFireNode {
    pub direction_bits: [u32; 3],
    pub compensation: i32,
    pub group: u32,
    pub selection: u32,
    pub speed_scale: u32,
    pub next: Option<NativeHeapRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorServiceResourceKind {
    Target(NativeTargetClass),
    ///427640: vtable520F2C and the actual actor pointer, eight bytes.
    FamilyRadiusScoreInput,
    WeaponFireNode,
}
impl ActorServiceResourceKind {
    pub fn allocation_bytes(self) -> usize {
        match self {
            Self::Target(class) => class.allocation_bytes() as usize,
            Self::FamilyRadiusScoreInput => 8,
            Self::WeaponFireNode => 32,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorServiceAllocationRequest {
    pub kind: ActorServiceResourceKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorServiceAllocationPolicy {
    PortableFallibleActorHeapApproximation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorServiceResourceState {
    Pending,
    Target(NativeTarget),
    FamilyRadiusScoreInput { actor: ObjectId },
    WeaponFireNode(NativeWeaponFireNode),
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ActorServiceHeapError {
    #[error("actor-service allocation belongs to another arena: {0:?}")]
    Foreign(NativeHeapRef),
    #[error("actor-service allocation identity is absent: {0:?}")]
    Unknown(NativeHeapRef),
    #[error("actor-service allocation was released: {0:?}")]
    Released(NativeHeapRef),
    #[error("actor-service allocation constructor is incomplete: {0:?}")]
    Pending(NativeHeapRef),
    #[error("actor-service allocation has another resource kind: {0:?}")]
    Kind(NativeHeapRef),
    #[error("actor-service allocation was already constructed: {0:?}")]
    Constructed(NativeHeapRef),
    #[error("actor-service identity counter exhausted")]
    IdentityExhausted,
    #[cfg(test)]
    #[error("supplied actor-service allocation failure {0}")]
    FixtureFailure(u32),
}
#[derive(Clone, Debug)]
struct Entry {
    kind: ActorServiceResourceKind,
    state: ActorServiceResourceState,
    ///Kept until explicit release. Never read or used as a zero initializer.
    backing: Option<Vec<MaybeUninit<u8>>>,
}
#[derive(Clone, Debug)]
pub struct ActorServiceHeap {
    arena: u64,
    policy: ActorServiceAllocationPolicy,
    entries: Vec<Entry>,
    #[cfg(test)]
    script: std::collections::VecDeque<FixtureActorServiceAllocation>,
    #[cfg(test)]
    attempts: Vec<ActorServiceResourceKind>,
    #[cfg(test)]
    release_failure: Option<u32>,
    #[cfg(test)]
    releases: Vec<NativeHeapRef>,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureActorServiceAllocation {
    Allocate,
    Null,
    Error(u32),
}
impl ActorServiceHeap {
    pub(crate) fn new(arena: u64, policy: ActorServiceAllocationPolicy) -> Self {
        Self {
            arena,
            policy,
            entries: Vec::new(),
            #[cfg(test)]
            script: Default::default(),
            #[cfg(test)]
            attempts: Vec::new(),
            #[cfg(test)]
            release_failure: None,
            #[cfg(test)]
            releases: Vec::new(),
        }
    }
    pub fn allocate(
        &mut self,
        request: ActorServiceAllocationRequest,
    ) -> Result<Option<NativeHeapRef>, ActorServiceHeapError> {
        #[cfg(test)]
        {
            self.attempts.push(request.kind);
            match self.script.pop_front() {
                Some(FixtureActorServiceAllocation::Null) => return Ok(None),
                Some(FixtureActorServiceAllocation::Error(code)) => {
                    return Err(ActorServiceHeapError::FixtureFailure(code))
                }
                Some(FixtureActorServiceAllocation::Allocate) | None => {}
            }
        }
        match self.policy {
            ActorServiceAllocationPolicy::PortableFallibleActorHeapApproximation => {}
        }
        let serial = u64::try_from(self.entries.len())
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(ActorServiceHeapError::IdentityExhausted)?;
        //Metadata is secured before any payload reservation can be lost.
        if self.entries.try_reserve(1).is_err() {
            return Ok(None);
        }
        let mut backing = Vec::<MaybeUninit<u8>>::new();
        let bytes = request.kind.allocation_bytes();
        if backing.try_reserve_exact(bytes).is_err() {
            return Ok(None);
        }
        backing.resize_with(bytes, MaybeUninit::uninit);
        self.entries.push(Entry {
            kind: request.kind,
            state: ActorServiceResourceState::Pending,
            backing: Some(backing),
        });
        Ok(Some(NativeHeapRef {
            arena: self.arena,
            serial,
        }))
    }
    fn entry(&self, reference: NativeHeapRef) -> Result<&Entry, ActorServiceHeapError> {
        if reference.arena != self.arena {
            return Err(ActorServiceHeapError::Foreign(reference));
        }
        let index = reference
            .serial
            .checked_sub(1)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(ActorServiceHeapError::Unknown(reference))?;
        self.entries
            .get(index)
            .ok_or(ActorServiceHeapError::Unknown(reference))
    }
    fn entry_mut(&mut self, reference: NativeHeapRef) -> Result<&mut Entry, ActorServiceHeapError> {
        self.entry(reference)?;
        Ok(&mut self.entries[(reference.serial - 1) as usize])
    }
    pub fn state(
        &self,
        reference: NativeHeapRef,
    ) -> Result<ActorServiceResourceState, ActorServiceHeapError> {
        Ok(self.entry(reference)?.state)
    }
    pub fn requested_bytes(
        &self,
        reference: NativeHeapRef,
    ) -> Result<usize, ActorServiceHeapError> {
        Ok(self.entry(reference)?.kind.allocation_bytes())
    }
    pub fn retained_bytes(&self, reference: NativeHeapRef) -> Result<usize, ActorServiceHeapError> {
        Ok(self.entry(reference)?.backing.as_ref().map_or(0, Vec::len))
    }
    pub fn construct_target(
        &mut self,
        reference: NativeHeapRef,
        data: NativeTarget,
    ) -> Result<(), ActorServiceHeapError> {
        let entry = self.entry_mut(reference)?;
        if entry.state == ActorServiceResourceState::Released {
            return Err(ActorServiceHeapError::Released(reference));
        }
        if entry.kind != ActorServiceResourceKind::Target(data.class()) {
            return Err(ActorServiceHeapError::Kind(reference));
        }
        if entry.state != ActorServiceResourceState::Pending {
            return Err(ActorServiceHeapError::Constructed(reference));
        }
        entry.state = ActorServiceResourceState::Target(data);
        Ok(())
    }
    pub fn target(&self, reference: NativeHeapRef) -> Result<NativeTarget, ActorServiceHeapError> {
        match self.entry(reference)?.state {
            ActorServiceResourceState::Target(data) => Ok(data),
            ActorServiceResourceState::Pending => Err(ActorServiceHeapError::Pending(reference)),
            ActorServiceResourceState::Released => Err(ActorServiceHeapError::Released(reference)),
            _ => Err(ActorServiceHeapError::Kind(reference)),
        }
    }
    pub fn write_target(
        &mut self,
        reference: NativeHeapRef,
        data: NativeTarget,
    ) -> Result<(), ActorServiceHeapError> {
        if self.target(reference)?.class() != data.class() {
            return Err(ActorServiceHeapError::Kind(reference));
        }
        self.entry_mut(reference)?.state = ActorServiceResourceState::Target(data);
        Ok(())
    }
    pub fn construct_radius_score_input(
        &mut self,
        reference: NativeHeapRef,
        actor: ObjectId,
    ) -> Result<(), ActorServiceHeapError> {
        let entry = self.entry_mut(reference)?;
        if entry.state == ActorServiceResourceState::Released {
            return Err(ActorServiceHeapError::Released(reference));
        }
        if entry.kind != ActorServiceResourceKind::FamilyRadiusScoreInput {
            return Err(ActorServiceHeapError::Kind(reference));
        }
        if entry.state != ActorServiceResourceState::Pending {
            return Err(ActorServiceHeapError::Constructed(reference));
        }
        entry.state = ActorServiceResourceState::FamilyRadiusScoreInput { actor };
        Ok(())
    }
    pub fn radius_score_actor(
        &self,
        reference: NativeHeapRef,
    ) -> Result<ObjectId, ActorServiceHeapError> {
        match self.entry(reference)?.state {
            ActorServiceResourceState::FamilyRadiusScoreInput { actor } => Ok(actor),
            ActorServiceResourceState::Pending => Err(ActorServiceHeapError::Pending(reference)),
            ActorServiceResourceState::Released => Err(ActorServiceHeapError::Released(reference)),
            _ => Err(ActorServiceHeapError::Kind(reference)),
        }
    }
    pub fn construct_weapon_fire_node(
        &mut self,
        reference: NativeHeapRef,
        node: NativeWeaponFireNode,
    ) -> Result<(), ActorServiceHeapError> {
        let entry = self.entry_mut(reference)?;
        if entry.state == ActorServiceResourceState::Released {
            return Err(ActorServiceHeapError::Released(reference));
        }
        if entry.kind != ActorServiceResourceKind::WeaponFireNode {
            return Err(ActorServiceHeapError::Kind(reference));
        }
        if entry.state != ActorServiceResourceState::Pending {
            return Err(ActorServiceHeapError::Constructed(reference));
        }
        entry.state = ActorServiceResourceState::WeaponFireNode(node);
        Ok(())
    }

    pub fn weapon_fire_node(
        &self,
        reference: NativeHeapRef,
    ) -> Result<NativeWeaponFireNode, ActorServiceHeapError> {
        match self.entry(reference)?.state {
            ActorServiceResourceState::WeaponFireNode(node) => Ok(node),
            ActorServiceResourceState::Pending => Err(ActorServiceHeapError::Pending(reference)),
            ActorServiceResourceState::Released => Err(ActorServiceHeapError::Released(reference)),
            _ => Err(ActorServiceHeapError::Kind(reference)),
        }
    }

    pub fn write_weapon_fire_node(
        &mut self,
        reference: NativeHeapRef,
        node: NativeWeaponFireNode,
    ) -> Result<(), ActorServiceHeapError> {
        self.weapon_fire_node(reference)?;
        self.entry_mut(reference)?.state = ActorServiceResourceState::WeaponFireNode(node);
        Ok(())
    }

    ///Explicit5116F4/target+00 release; pointer fields are a caller operation.
    pub fn release(&mut self, reference: NativeHeapRef) -> Result<(), ActorServiceHeapError> {
        if self.entry(reference)?.state == ActorServiceResourceState::Released {
            return Err(ActorServiceHeapError::Released(reference));
        }
        #[cfg(test)]
        {
            self.releases.push(reference);
            if let Some(code) = self.release_failure.take() {
                return Err(ActorServiceHeapError::FixtureFailure(code));
            }
        }
        let entry = self.entry_mut(reference)?;
        entry.backing = None;
        entry.state = ActorServiceResourceState::Released;
        Ok(())
    }
    #[cfg(test)]
    pub fn script(&mut self, script: impl IntoIterator<Item = FixtureActorServiceAllocation>) {
        self.script = script.into_iter().collect();
    }
    #[cfg(test)]
    pub fn attempts(&self) -> &[ActorServiceResourceKind] {
        &self.attempts
    }
    #[cfg(test)]
    pub fn fixture_references(&self) -> Vec<NativeHeapRef> {
        (0..self.entries.len())
            .map(|index| NativeHeapRef {
                arena: self.arena,
                serial: index as u64 + 1,
            })
            .collect()
    }
    #[cfg(test)]
    pub fn fixture_fail_next_release(&mut self, code: u32) {
        self.release_failure = Some(code);
    }
    #[cfg(test)]
    pub fn fixture_release_attempts(&self) -> &[NativeHeapRef] {
        &self.releases
    }
}
#[cfg(test)]
mod tests;
