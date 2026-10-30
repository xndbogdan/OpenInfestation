//! Reached entry/scan/pair preparation of 0047C1D0. Genuine zero/lookup/model
//! skips return normally. ACTIVE4F2BE0 delegates to its retained B/table
//! service; required geometry/projection/contact errors preserve reached writes.

use crate::object_runtime::CollisionContact;
use crate::world_furniture_effect::{
    FurnitureQueryRecord, LiteralRetExitRegistrationApproximation, ProcessFurnitureQueryState,
};
use crate::{
    object_runtime::{ObjectArena, ObjectError, ObjectId},
    terrain_runtime::{
        ProcessFurnitureRadius, TerrainRuntime, TerrainSpacing, TerrainSparseRecordId,
    },
};
use inf_formats::{native_basis::NativeBasis, TerrainResourceLookup};
use thiserror::Error;

/// Captured H12 identity and actual H8 loans belong to the host, independently
/// of the intrinsic scan. Every acquisition is lazy at its native call phase.
pub trait FurnitureCollisionPrefixHost {
    type Error;
    type Terrain: Copy;
    type Model;
    fn acquire_terrain(&mut self) -> Result<Option<Self::Terrain>, Self::Error>;
    fn terrain(&self, source: Self::Terrain) -> Result<&TerrainRuntime, Self::Error>;
    /// Current validated5372F0,5372F4 and5372F8, each read separately.
    fn spacing(&mut self) -> TerrainSpacing;
    fn fraction_mask(&mut self) -> TerrainSpacing;
    fn height_shift(&mut self) -> u32;
    fn model(&mut self, slot: u16) -> Result<Option<Self::Model>, Self::Error>;
    fn model_radius(&self, model: &Self::Model) -> u16;
    fn placement_model(&mut self, lookup: TerrainResourceLookup) -> Result<u16, Self::Error>;
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error>;
    fn shared_basis(&mut self) -> NativeBasis;
    fn furniture_basis(&mut self, state: u8) -> NativeBasis;
    /// Qualifies the current sparse allocation without extending its lifetime.
    fn qualify_record(
        &self,
        terrain: Self::Terrain,
        record: TerrainSparseRecordId,
    ) -> FurnitureQueryRecord;
    ///Actual complete auxiliary request. A normal contact is distinct from
    ///a prequery census/typed stop; retained process/projection custody is required.
    fn active_pair(
        &mut self,
        request: FurnitureCollisionPair<'_, Self::Model>,
        process: &mut ProcessFurnitureQueryState,
    ) -> Result<CollisionContact, Self::Error>;
    ///Actual actor+A8 only after a nonzero returned contact. The admitted
    ///family guard may return before reading any continuation operand.
    fn respond_contact(
        &mut self,
        arena: &mut ObjectArena,
        state: FurnitureCollisionPairState,
        contact: CollisionContact,
    ) -> Result<u8, Self::Error>;
}

pub struct FurnitureCollisionPrefixRequest<'a> {
    pub actor: ObjectId,
    pub process: &'a mut ProcessFurnitureQueryState,
    pub radius: &'a ProcessFurnitureRadius,
    /// Explicit portable adapter for this literal RET callback only. Native
    ///511D89 EAX is ignored; this policy has no modeled allocation failure.
    pub exit_registration: LiteralRetExitRegistrationApproximation,
}

/// Stack A's reached values. Coordinates are signed low-WORD differences in
/// world integer units. B remains the actual process context, never origin0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureCollisionPairState {
    pub actor: ObjectId,
    pub actor_model_word: u16,
    pub counterpart_model_word: u16,
    ///The H11 selector was acquired for this cell before the captured H8 loan.
    pub placement_lookup: TerrainResourceLookup,
    pub saved_actor_position: [u16; 3],
    pub actor_basis: NativeBasis,
    pub cell_position: [u16; 3],
    pub relative_actor_origin: [i32; 3],
    pub contact_flag: u32,
}

pub struct FurnitureCollisionPair<'a, M> {
    pub state: FurnitureCollisionPairState,
    /// Request+08 is the actual actor root, +0C the actual counterpart.
    pub actor_root: &'a M,
    pub counterpart: &'a M,
}

#[derive(Debug, Error)]
pub enum FurnitureCollisionPrefixError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("actor {actor:?} requires H8/{model_word} header0E at0047C2AE")]
    RequiredActorModel { actor: ObjectId, model_word: u16 },
    #[error("actor {actor:?} requires position at{address:08X}")]
    RequiredPosition { actor: ObjectId, address: u32 },
    #[error("active callback left contact flag unwritten")]
    RequiredContactFlag,
    #[error("captured H12 is NULL at0047C3A8")]
    RequiredCapturedTerrain,
    #[error("flag04 selected missing sparse row at grid{grid:?}")]
    RequiredSparseRecord { grid: [u16; 2] },
    #[error("captured sparse row disappeared before state read: {record:?}")]
    SparseRecord { record: TerrainSparseRecordId },
    #[error("furniture collision service failed at{effect}: {error}")]
    Host { effect: &'static str, error: E },
}

#[derive(Clone, Copy)]
enum Record {
    Fallback,
    Sparse(TerrainSparseRecordId),
}

/// Source-admitted positive spacing128/256 and sorted canonical H12 buckets.
/// Upper register residue in native MOVAX is immaterial here: endpoints and
/// every count difference are explicitly reduced modulo65536 before use.
pub fn collide_furniture_prefix<H: FurnitureCollisionPrefixHost>(
    arena: &mut ObjectArena,
    request: FurnitureCollisionPrefixRequest<'_>,
    host: &mut H,
) -> Result<(), FurnitureCollisionPrefixError<H::Error>> {
    let actor = request.actor;
    let terrain = host
        .acquire_terrain()
        .map_err(|error| FurnitureCollisionPrefixError::Host {
            effect: "00405540 H12/0",
            error,
        })?;
    // Local basis/context constructors write no process/actor fields. The
    // mask is read before the captured signed process radius, then bit tested.
    let initialize = request.process.query_init_mask() & 1 == 0;
    let process_radius = i32::from(request.radius.word() as i16);
    if initialize {
        request.process.initialize_query(request.exit_registration);
    }
    let actor_model_word = arena.actor(actor)?.rotating.visual.model_word;
    let actor_model = host
        .model(actor_model_word)
        .map_err(|error| FurnitureCollisionPrefixError::Host {
            effect: "actor H8",
            error,
        })?
        .ok_or(FurnitureCollisionPrefixError::RequiredActorModel {
            actor,
            model_word: actor_model_word,
        })?;
    let actor_radius = host.model_radius(&actor_model);
    if actor_radius == 0 || process_radius == 0 {
        return Ok(());
    }
    let x = position(arena, actor, 0x0047_c2c3)?[0];
    let step = i32::from(host.spacing().units());
    let mask = u32::from(host.fraction_mask().units() - 1);
    let radius = process_radius.wrapping_add(i32::from(actor_radius));
    let x_start = lower(x, radius, step, mask);
    let z = position(arena, actor, 0x0047_c2f2)?[2];
    let z_start = lower(z, radius, step, mask);
    let z_end = upper(z, radius, mask);
    let z_count = u32::from(z_end.wrapping_sub(z_start)) / u32::from(host.spacing().units()) + 1;
    let x_end = upper(x, radius, mask);
    let x_count = u32::from(x_end.wrapping_sub(x_start)) / u32::from(host.spacing().units()) + 1;
    //47C448/456: native has_prepared is a stack local for the entire query.
    //Capturing anew after a response would change its later ACTIVE requests.
    let mut prepared_pair: Option<([u16; 3], NativeBasis)> = None;
    let mut x = x_start;
    for _ in 0..x_count {
        let mut z = z_start;
        let mut last_step = 0_u16;
        for _ in 0..z_count {
            let mask = host.fraction_mask().units() - 1;
            let grid = [(x >> 7) & !(mask >> 7), (z >> 7) & !(mask >> 7)];
            let captured = terrain.ok_or(FurnitureCollisionPrefixError::RequiredCapturedTerrain)?;
            let source =
                host.terrain(captured)
                    .map_err(|error| FurnitureCollisionPrefixError::Host {
                        effect: "captured H12 cell",
                        error,
                    })?;
            let record = if source.cell_at_grid(grid).has_sparse_record() {
                Record::Sparse(
                    source
                        .sparse_record_at_grid(grid)
                        .ok_or(FurnitureCollisionPrefixError::RequiredSparseRecord { grid })?
                        .0,
                )
            } else {
                Record::Fallback
            };
            let lookup = payload(host, captured, request.process, record)?.resource_lookup;
            if lookup.raw() != 0 {
                let counterpart_word = host.placement_model(lookup).map_err(|error| {
                    FurnitureCollisionPrefixError::Host {
                        effect: "current H11/0",
                        error,
                    }
                })?;
                let counterpart = host.model(counterpart_word).map_err(|error| {
                    FurnitureCollisionPrefixError::Host {
                        effect: "counterpart H8",
                        error,
                    }
                })?;
                if let Some(counterpart) = counterpart {
                    if host.model_radius(&counterpart) != 0 {
                        //Contactflag0 is fresh for every cell; has_prepared1
                        //gates saved XYZ/virtual48/B-basis/alias to the FIRST ACTIVE.
                        let (saved, basis) = if let Some(prepared) = prepared_pair {
                            prepared
                        } else {
                            let saved = position(arena, actor, 0x0047_c482)?;
                            let basis = host.actor_basis(arena, actor).map_err(|error| {
                                FurnitureCollisionPrefixError::Host {
                                    effect: "actor virtual48",
                                    error,
                                }
                            })?;
                            request.process.publish_pair_basis(host.shared_basis());
                            request.process.publish_pair_alias_546b48();
                            prepared_pair = Some((saved, basis));
                            (saved, basis)
                        };
                        let qualified = match record {
                            Record::Fallback => FurnitureQueryRecord::Fallback,
                            Record::Sparse(record) => host.qualify_record(captured, record),
                        };
                        //546B84 publication precedes a FRESH stateBYTE read.
                        request.process.publish_record(qualified);
                        let state = payload(host, captured, request.process, record)?.state;
                        request
                            .process
                            .publish_pair_basis(host.furniture_basis(state));
                        let mask = host.fraction_mask().units() - 1;
                        let cell_xz = [x & !mask, z & !mask];
                        let grid = [
                            (cell_xz[0] >> 7) & !(mask >> 7),
                            (cell_xz[1] >> 7) & !(mask >> 7),
                        ];
                        let shift = host.height_shift();
                        let height = host
                            .terrain(captured)
                            .map_err(|error| FurnitureCollisionPrefixError::Host {
                                effect: "captured H12 height",
                                error,
                            })?
                            .cell_at_grid(grid)
                            .height_i8();
                        let cell = [
                            cell_xz[0],
                            (i32::from(height).wrapping_shl(shift)) as u16,
                            cell_xz[1],
                        ];
                        let state = FurnitureCollisionPairState {
                            actor,
                            actor_model_word,
                            counterpart_model_word: counterpart_word,
                            placement_lookup: lookup,
                            saved_actor_position: saved,
                            actor_basis: basis,
                            cell_position: cell,
                            relative_actor_origin: std::array::from_fn(|axis| {
                                i32::from(saved[axis].wrapping_sub(cell[axis]) as i16)
                            }),
                            contact_flag: 0,
                        };
                        let contact = host
                            .active_pair(
                                FurnitureCollisionPair {
                                    state,
                                    actor_root: &actor_model,
                                    counterpart: &counterpart,
                                },
                                request.process,
                            )
                            .map_err(|error| FurnitureCollisionPrefixError::Host {
                                effect: "004F2BE0 active pair",
                                error,
                            })?;
                        let flag = contact
                            .flag()
                            .ok_or(FurnitureCollisionPrefixError::RequiredContactFlag)?;
                        if flag != 0 {
                            let al =
                                host.respond_contact(arena, state, contact)
                                    .map_err(|error| FurnitureCollisionPrefixError::Host {
                                        effect: "actor virtualA8",
                                        error,
                                    })?;
                            if al != 0 {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            //FreshF0 for each completed cell; outer increment reuses the
            //last inner value rather than readingF0 a second time.
            last_step = host.spacing().units();
            z = z.wrapping_add(last_step);
        }
        x = x.wrapping_add(last_step);
    }
    Ok(())
}

fn position<E>(
    arena: &ObjectArena,
    actor: ObjectId,
    address: u32,
) -> Result<[u16; 3], FurnitureCollisionPrefixError<E>> {
    arena
        .object(actor)?
        .base
        .position_words
        .ok_or(FurnitureCollisionPrefixError::RequiredPosition { actor, address })
}
fn lower(coordinate: u16, radius: i32, step: i32, mask: u32) -> u16 {
    ((u32::from(coordinate)
        .wrapping_sub(radius as u32)
        .wrapping_add(step as u32)
        .wrapping_sub(1))
        & !mask) as u16
}
fn upper(coordinate: u16, radius: i32, mask: u32) -> u16 {
    (u32::from(coordinate).wrapping_add(radius as u32) & !mask) as u16
}
fn payload<H: FurnitureCollisionPrefixHost>(
    host: &H,
    terrain: H::Terrain,
    process: &ProcessFurnitureQueryState,
    record: Record,
) -> Result<crate::terrain_runtime::TerrainSparsePayload, FurnitureCollisionPrefixError<H::Error>> {
    match record {
        Record::Fallback => Ok(process.fallback_payload()),
        Record::Sparse(record) => {
            let current = host
                .terrain(terrain)
                .map_err(|error| FurnitureCollisionPrefixError::Host {
                    effect: "captured sparse record",
                    error,
                })?
                .buckets()
                .get(usize::from(record.bucket))
                .and_then(|bucket| bucket.records.get(record.index))
                .ok_or(FurnitureCollisionPrefixError::SparseRecord { record })?;
            Ok(crate::terrain_runtime::TerrainSparsePayload {
                state: current.state,
                resource_lookup: current.resource_lookup,
            })
        }
    }
}

#[cfg(test)]
mod native_guards;
#[cfg(test)]
mod tests;
