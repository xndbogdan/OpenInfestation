//! Periodic target acquisition 00427490/00427510 and input factory 00427640.
//! Player registry, route lists and the shared weighted publishers retain
//! their distinct domains. No scene selects synthetic referents or scores.

use crate::object_runtime::WeightedTargetConstructorNull;
use thiserror::Error;

use super::{FamilyPeriodicProbeRequest, FamilyTargetSlot};
use crate::object_runtime::{
    NativeHeapRef, NativeTargetError, ObjectArena, ObjectError, ObjectId, WeightedTargetReferent,
    WeightedTargetRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyTargetProducerMode {
    FirstPlayerActor,
    RouteNodes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTargetProducerRequest {
    periodic: FamilyPeriodicProbeRequest,
    mode: FamilyTargetProducerMode,
    destination: FamilyTargetSlot,
}

impl FamilyTargetProducerRequest {
    /// Admit only a literal table record; callback identity and actor byte
    /// offset are not interchangeable with a selector or target type number.
    pub fn from_periodic(periodic: FamilyPeriodicProbeRequest) -> Option<Self> {
        if periodic.profile.periodic_record(periodic.index)? != periodic.record {
            return None;
        }
        let (mode, destination) = match (periodic.record.callback, periodic.record.actor_offset) {
            (0x0042_7490, 0x26c) => (
                FamilyTargetProducerMode::FirstPlayerActor,
                FamilyTargetSlot::Target26c,
            ),
            (0x0042_7510, 0x1fc) => (
                FamilyTargetProducerMode::RouteNodes,
                FamilyTargetSlot::Target1fc,
            ),
            _ => return None,
        };
        Some(Self {
            periodic,
            mode,
            destination,
        })
    }

    pub fn mode(self) -> FamilyTargetProducerMode {
        self.mode
    }
    pub fn destination(self) -> FamilyTargetSlot {
        self.destination
    }
    pub fn periodic(self) -> FamilyPeriodicProbeRequest {
        self.periodic
    }
}

/// The entire proven8-byte 00427640 payload: static vtable at+00 and actual
/// actor identity at+04. The enclosing host's PeriodicInput token owns the
/// allocation separately; this data is not a target resource or allocator address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyRadiusScoreInput {
    pub vtable_00: u32,
    pub actor_04: ObjectId,
}

pub trait FamilyTargetProducerHost: crate::object_runtime::TargetNullHandleSource {
    type Error;
    /// One original 0048F060 process draw. This routine has no per-actor RNG.
    fn random(&mut self) -> Result<u32, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyTargetProducerError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error("object {0:?} requires its unwritten native position")]
    MissingPosition(ObjectId),
    #[error("00427490 requires its nonnull score input at the reached virtual callback")]
    MissingInput,
    #[error("unsupported score-input vtable {0:08X}")]
    UnsupportedInput(u32),
    #[error("periodic target host failed at {effect}: {error}")]
    Host { effect: &'static str, error: E },
}

impl ObjectArena {
    /// Actual 00427640 factory. It initializes exactly+00/+04 after successful
    /// allocation and performs no actor read or RNG draw.00425C60 owns release.
    pub fn allocate_family_radius_score_input(
        &mut self,
        actor: ObjectId,
    ) -> Result<Option<NativeHeapRef>, ObjectError> {
        let allocation =
            self.allocate_actor_service(crate::object_runtime::ActorServiceAllocationRequest {
                kind: crate::object_runtime::ActorServiceResourceKind::FamilyRadiusScoreInput,
            })?;
        if let Some(reference) = allocation {
            self.actor_service_heap_mut()
                .construct_radius_score_input(reference, actor)?;
        }
        Ok(allocation)
    }

    /// Called by the existing periodic wrapper's concrete host. Its factory
    /// null branch precedes this callback, even when RouteNodes ignores input.
    pub fn produce_family_periodic_target<H: FamilyTargetProducerHost>(
        &mut self,
        actor: ObjectId,
        request: FamilyTargetProducerRequest,
        input: Option<NativeHeapRef>,
        host: &mut H,
    ) -> Result<bool, FamilyTargetProducerError<H::Error>> {
        let weighted = match request.mode {
            FamilyTargetProducerMode::FirstPlayerActor => {
                let mut weighted = WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::Process(host),
                    referent: WeightedTargetReferent::Actor(None),
                    score: 0,
                };
                if let Some(candidate) = self.first_player_primary_actor()? {
                    let distance = self.producer_distance(candidate, actor)?;
                    let radius = self.derived_actor(actor)?.word_164 as u32;
                    if distance < radius.wrapping_mul(radius) {
                        let input = input.ok_or(FamilyTargetProducerError::MissingInput)?;
                        let input_actor = self
                            .actor_service_heap()
                            .radius_score_actor(input)
                            .map_err(ObjectError::from)?;
                        // The input's virtual 00427660: its own actor's
                        // 00427620 at the supplied distance.
                        weighted = WeightedTargetRequest {
                            constructor_null: WeightedTargetConstructorNull::Process(host),
                            referent: WeightedTargetReferent::Actor(Some(candidate)),
                            score: self.family_radius_score(input_actor, distance)?,
                        };
                    }
                }
                weighted
            }
            FamilyTargetProducerMode::RouteNodes => {
                let radius = self.derived_actor(actor)?.word_164 as u32;
                let mut best = radius.wrapping_mul(radius);
                let mut selected = None;
                // No allocation or virtual callback occurs until publication.
                // Original 0048F060 changes only RNGDWORD, so the existing route
                // owner supplies these ordered linear lists without mutation:
                // 004050C0 the first group, 004050E0 its root, 00404CA0 each
                // next node and 00405120 the next group.
                for group in self.route_groups() {
                    let Some(&root) = group.members.first() else {
                        return Err(ObjectError::UnknownRouteGroup(group.id).into());
                    };
                    if self.object(root)?.base.flags & 0x10 != 0 {
                        continue;
                    }
                    for &candidate in &group.members {
                        // Coordinates are captured BEFORE each RNG draw.
                        let squared = self.producer_distance(actor, candidate)?;
                        let draw =
                            host.random()
                                .map_err(|error| FamilyTargetProducerError::Host {
                                    effect: "0048F060",
                                    error,
                                })?;
                        let metric = squared.wrapping_add(draw & 0xffff);
                        if metric < best {
                            best = metric;
                            // The nativezero branch keeps the previous pointer.
                            if metric != 0 {
                                selected = Some(candidate);
                            }
                        }
                        let flags = group.flags.ok_or(ObjectError::RouteUnfinalized(root))?;
                        if flags & 2 == 0 {
                            break;
                        }
                    }
                }
                WeightedTargetRequest {
                    constructor_null: WeightedTargetConstructorNull::Process(host),
                    referent: WeightedTargetReferent::Object(selected),
                    // Nullreferent bypasses native score inspection entirely.
                    score: if selected.is_some() {
                        self.family_radius_score(actor, best)?
                    } else {
                        0
                    },
                }
            }
        };
        Ok(self.publish_family_target(actor, request.destination, weighted)?)
    }

    fn producer_distance<E>(
        &self,
        first: ObjectId,
        second: ObjectId,
    ) -> Result<u32, FamilyTargetProducerError<E>> {
        let position = |object| {
            self.object(object)?
                .base
                .position_words
                .ok_or(FamilyTargetProducerError::MissingPosition(object))
        };
        Ok(position(first)?
            .into_iter()
            .zip(position(second)?)
            .fold(0u32, |sum, (a, b)| {
                let delta = i32::from(a.wrapping_sub(b) as i16);
                sum.wrapping_add(delta.wrapping_mul(delta) as u32)
            }))
    }
}

mod grant_grid;
pub use grant_grid::{
    terrain_furniture_grant, FamilyGrantGridError, FamilyGrantGridHost,
    FamilyGrantGridPointRequest, FamilyTerrainGrantRequest, Selector12GrantGridRequest,
};
