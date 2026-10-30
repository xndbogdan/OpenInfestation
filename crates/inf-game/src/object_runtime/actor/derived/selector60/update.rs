//! Native 00480910 update, 00480510 row census, and the normal-return
//! suffix of 00480610. The live host owns H14/H12 and 00460310 placement
//! construction; row storage remains the original shared allocation.

use super::{add, RetainedSpawnList, Selector60Error};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorConstructionError, DerivedActorError, ObjectArena, ObjectError, ObjectId,
        RuntimePointer, Selector60RowId, TargetNullHandleSource,
    },
};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Selector60SpawnRow {
    retained: RetainedSpawnList,
    offset: u32,
}

impl Selector60SpawnRow {
    pub fn reference(&self) -> Result<u32, Selector60Error> {
        Ok(self
            .retained
            .allocation()
            .runtime_allocation()
            .dword(self.offset)?)
    }

    fn word(&self, offset: u32) -> Result<u16, Selector60Error> {
        Ok(self
            .retained
            .allocation()
            .runtime_allocation()
            .word(add(self.offset, offset)?)?)
    }

    fn flags(&self) -> Result<u8, Selector60Error> {
        Ok(self
            .retained
            .allocation()
            .runtime_allocation()
            .byte(add(self.offset, 0x10)?)?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector60SpawnResult {
    NoPlacement,
    Rejected,
    Created(ObjectId),
}

pub trait Selector60UpdateHost: TargetNullHandleSource {
    type Error;
    fn delayed_physics(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
    ) -> Result<(), Self::Error>;
    /// Resolve H14/0 before reading row.reference(). Capture resource,
    /// suffix and XYZ before H12 height, then reread placement rotation.
    /// 00460310 receives placement FFFFFFFF, mode zero and current NULL.
    fn spawn_placement(
        &mut self,
        arena: &mut ObjectArena,
        row: &Selector60SpawnRow,
        rng: &mut u32,
    ) -> Result<Selector60SpawnResult, Self::Error>;
    fn actor_component(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn initialize_spawned(
        &mut self,
        arena: &mut ObjectArena,
        raw: ObjectId,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// Actual virtual +48 returns a basis pointer. Retain its arena owner,
    /// not a snapshot: a later getter may mutate the same basis storage.
    fn source_basis(
        &mut self,
        arena: &mut ObjectArena,
        parent: ObjectId,
    ) -> Result<Option<ObjectId>, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector60UpdateError<E> {
    #[error("selector60 {0:?} update previously stopped after reached writes")]
    PreviouslyStopped(ObjectId),
    #[error(transparent)]
    Actor(#[from] ActorConstructionError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Row(#[from] Selector60Error),
    #[error("selector60 requires current process NULL at {0:08X}")]
    MissingProcessNull(u32),
    #[error("selector60 requires actor {actor:?} field +{offset:X} at {address:08X}")]
    RequiredField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("selector60 row {row:?} handle index {index} is outside its owned allocation")]
    HandleRange { row: Selector60RowId, index: usize },
    #[error("selector60 row pointer is NULL at {0:#X}")]
    MissingRow(u32),
    #[error("selector60 service failed: {0}")]
    Service(E),
}

impl ObjectArena {
    fn selector60_row_mut(
        &mut self,
        row: Selector60RowId,
    ) -> Result<&mut super::Selector60Row, Selector60Error> {
        self.selector60_row(row)?;
        Ok(self.selector60_rows.0[row.index]
            .as_mut()
            .expect("validated row"))
    }

    fn spawn_row<E>(
        &self,
        actor: ObjectId,
        index: u32,
    ) -> Result<(Selector60SpawnRow, Selector60RowId), Selector60UpdateError<E>> {
        let retained = self.selector60_actor(actor)?.retained_190.clone().ok_or(
            Selector60UpdateError::RequiredField {
                actor,
                offset: 0x190,
                address: 0x0048_09cd,
            },
        )?;
        let offset = retained.row_offset(index)?;
        let pointer = add(offset, 0x14)?;
        let Some(RuntimePointer::Selector60Row(row)) = retained
            .allocation()
            .runtime_allocation()
            .runtime_pointer(pointer)
            .map_err(Selector60Error::from)?
        else {
            return Err(Selector60UpdateError::MissingRow(pointer));
        };
        self.selector60_row(row)?;
        Ok((Selector60SpawnRow { retained, offset }, row))
    }

    fn census_spawn_row<H: Selector60UpdateHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        row: Selector60RowId,
        source: &Selector60SpawnRow,
        delta: u32,
        host: &H,
    ) -> Result<u32, Selector60UpdateError<H::Error>> {
        if self.selector60_row(row)?.byte_00 != 0 {
            return Ok(self.selector60_row(row)?.word_10);
        }
        let mut live = 0_i32;
        let mut index = 0_usize;
        while (index as i32) < i32::from(source.word(6)? as i16) {
            let null = host
                .target_null_handle()
                .ok_or(Selector60UpdateError::MissingProcessNull(0x0048_0530))?;
            let handle = *self
                .selector60_row(row)?
                .handles_08
                .get(index)
                .ok_or(Selector60UpdateError::HandleRange { row, index })?;
            if handle != null {
                let resolved = self.resolve_actor_handle(handle)?;
                if resolved
                    .map(|target| self.actor(target).map(|a| a.word_c4 as i32 > 1))
                    .transpose()?
                    .unwrap_or(false)
                {
                    live = live.wrapping_add(1);
                } else {
                    let null = host
                        .target_null_handle()
                        .ok_or(Selector60UpdateError::MissingProcessNull(0x0048_055b))?;
                    self.selector60_row_mut(row)?.handles_08[index] = null;
                }
            }
            index += 1;
        }
        if source.flags()? & 1 != 0 {
            let state = self.selector60_actor_mut(actor)?;
            state.word_18a = state.word_18a.wrapping_add(live as u16);
        }
        let limit = i32::from(source.word(8)? as i16);
        if limit != 0 && self.selector60_row(row)?.word_0c as i32 >= limit {
            if live == 0 {
                self.selector60_row_mut(row)?.byte_00 = 1;
            }
            return Ok(self.selector60_row(row)?.word_10);
        }
        if source.flags()? & 1 != 0 {
            self.selector60_actor_mut(actor)?.byte_189 = 0;
        }
        self.selector60_actor_mut(actor)?.byte_188 = 0;
        if live < i32::from(source.word(4)? as i16) {
            self.selector60_row_mut(row)?.byte_01 = 1;
        }
        if live >= i32::from(source.word(6)? as i16) {
            self.selector60_row_mut(row)?.byte_01 = 0;
        }
        if self.selector60_row(row)?.byte_01 != 0 {
            let remaining = self.selector60_row(row)?.word_04;
            if remaining > delta {
                self.selector60_row_mut(row)?.word_04 = remaining.wrapping_sub(delta);
            } else {
                let row = self.selector60_row_mut(row)?;
                row.word_04 = 0;
                row.word_10 = row.word_10.wrapping_add(1);
            }
            let remaining = self.selector60_row(row)?.word_04;
            if remaining < self.selector60_actor(actor)?.word_18c {
                self.selector60_actor_mut(actor)?.word_18c = remaining;
            }
        }
        Ok(self.selector60_row(row)?.word_10)
    }

    /// 00480EA0: the kept +198 triple once BYTE +184 is set; else the
    /// negated radius along the source basis's Z row, truncated, added to
    /// the parent's position.
    fn selector60_spawn_position<H: Selector60UpdateHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<[u16; 3], Selector60UpdateError<H::Error>> {
        if self.selector60_actor(actor)?.byte_184 == 0 {
            let radius = self.object(actor)?.base.word_28 as u16;
            let radius = f64::from(radius.wrapping_neg() as i16);
            let basis_owner = host
                .source_basis(self, actor)
                .map_err(Selector60UpdateError::Service)?
                .ok_or(Selector60UpdateError::RequiredField {
                    actor,
                    offset: 0x44,
                    address: 0x0048_0ecf,
                })?;
            let basis = self.actor(basis_owner)?.rotating.basis;
            let offset = [7, 8, 9].map(|i| {
                super::super::kinematics::ftol(radius * f64::from(f32::from_bits(basis.words[i])))
                    as u16
            });
            self.selector60_actor_mut(actor)?.position_198 = Some(offset);
            let position = self.object(actor)?.base.position_words.ok_or(
                Selector60UpdateError::RequiredField {
                    actor,
                    offset: 0x32,
                    address: 0x0048_0efd,
                },
            )?;
            self.selector60_actor_mut(actor)?.position_198 =
                Some(std::array::from_fn(|i| offset[i].wrapping_add(position[i])));
        }
        self.selector60_actor(actor)?
            .position_198
            .ok_or(Selector60UpdateError::RequiredField {
                actor,
                offset: 0x198,
                address: 0x0048_0762,
            })
    }

    fn execute_spawn_row<H: Selector60UpdateHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        row: Selector60RowId,
        source: &Selector60SpawnRow,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector60UpdateError<H::Error>> {
        let raw = match host
            .spawn_placement(self, source, rng)
            .map_err(Selector60UpdateError::Service)?
        {
            Selector60SpawnResult::NoPlacement => {
                self.selector60_row_mut(row)?.byte_00 = 1;
                return Ok(());
            }
            Selector60SpawnResult::Rejected => return Ok(()),
            Selector60SpawnResult::Created(raw) => raw,
        };
        self.selector60_row_mut(row)?.word_10 = 0;
        let child = host
            .actor_component(self, raw)
            .map_err(Selector60UpdateError::Service)?;
        host.initialize_spawned(self, raw, rng)
            .map_err(Selector60UpdateError::Service)?;
        let Some(child) = child else {
            self.selector60_row_mut(row)?.byte_00 = 1;
            return Ok(());
        };
        self.selector60_row_mut(row)?.word_0c = self.selector60_row(row)?.word_0c.wrapping_add(1);
        let random = name_entry_rng_next(rng) & 0xffff;
        let upper = i32::from(source.word(0x0e)? as i16);
        let lower = i32::from(source.word(0x0c)? as i16);
        let interpolated = (random as i32).wrapping_mul(upper.wrapping_sub(lower)) >> 16;
        self.selector60_row_mut(row)?.word_04 = interpolated as u32;
        let lower = i32::from(source.word(0x0c)? as i16);
        self.selector60_row_mut(row)?.word_04 =
            lower.wrapping_add(interpolated).wrapping_mul(100_000) as u32;
        if source.flags()? & 4 == 0 {
            let basis_owner = host
                .source_basis(self, actor)
                .map_err(Selector60UpdateError::Service)?;
            let position = self.selector60_spawn_position(actor, host)?;
            self.owned_mut(child)?.state.base.position_words = Some(position);
            if let Some(owner) = basis_owner {
                let basis = self.actor(owner)?.rotating.basis;
                self.actor_mut(child)?.rotating.basis = basis;
            }
        }
        let capacity = i32::from(source.word(6)? as i16);
        if capacity > 0 {
            let null = host
                .target_null_handle()
                .ok_or(Selector60UpdateError::MissingProcessNull(0x0048_07be))?;
            for index in 0..capacity as usize {
                let handle = *self
                    .selector60_row(row)?
                    .handles_08
                    .get(index)
                    .ok_or(Selector60UpdateError::HandleRange { row, index })?;
                if handle == null {
                    let handle = self.actor(child)?.handle;
                    self.selector60_row_mut(row)?.handles_08[index] = handle;
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn run_selector60_update<H: Selector60UpdateHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector60UpdateError<H::Error>> {
        if self.selector60_actor(actor)?.update_stopped {
            return Err(Selector60UpdateError::PreviouslyStopped(actor));
        }
        self.selector60_actor_mut(actor)?.update_stopped = true;
        let result = self.run_selector60_update_inner(actor, microseconds, host, rng);
        if result.is_ok() {
            self.selector60_actor_mut(actor)?.update_stopped = false;
        }
        result
    }

    fn run_selector60_update_inner<H: Selector60UpdateHost + ?Sized>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), Selector60UpdateError<H::Error>> {
        self.selector60_actor(actor)?;
        let delay = self.derived_actor(actor)?.word_144;
        if delay != 0 {
            if delay > 0 {
                self.derived_actor_mut(actor)?.word_144 = if delay > microseconds as i32 {
                    delay.wrapping_sub(microseconds as i32)
                } else {
                    0
                };
                host.delayed_physics(self, actor, microseconds)
                    .map_err(Selector60UpdateError::Service)?;
            }
            return Ok(());
        }
        let state = self.selector60_actor(actor)?;
        if (state.word_1a4 as i32 <= 0 && state.byte_1a8 != 0) || state.retained_190.is_none() {
            return Ok(());
        }
        let state = self.selector60_actor_mut(actor)?;
        state.word_18a = 0;
        state.word_18c = u32::MAX;
        state.byte_188 = 1;
        state.byte_189 = 1;
        state.word_194 = state.word_194.saturating_sub(microseconds);
        let mut best = 0_i32;
        let mut selected = 0_u32;
        let mut index = 0_u32;
        loop {
            let count = self
                .selector60_actor(actor)?
                .retained_190
                .as_ref()
                .ok_or(Selector60UpdateError::RequiredField {
                    actor,
                    offset: 0x190,
                    address: 0x0048_09f1,
                })?
                .count()?;
            if index as i32 >= count {
                break;
            }
            let (source, row) = self.spawn_row(actor, index)?;
            let priority = self.census_spawn_row(actor, row, &source, microseconds, host)? as i32;
            if priority > best {
                best = priority;
                selected = index;
            }
            index = index.wrapping_add(1);
        }
        let state = self.selector60_actor_mut(actor)?;
        let mut target = 0_i32;
        if best == 0 && state.word_18c == u32::MAX {
            state.word_1a0 = 0;
        } else {
            state.word_1a0 = 1;
            let timer = state.word_194.max(state.word_18c);
            if timer < 1_500_000 {
                target = 0xffff - ((timer as i32) >> 6);
            }
        }
        let current = i32::from(state.word_19e);
        let product = (microseconds >> 5).wrapping_mul(0xffff);
        if target < current {
            let high = ((u64::from(product) * 0x65eb_ec79) >> 32) as u32;
            let decrement = ((product.wrapping_sub(high) >> 1).wrapping_add(high)) >> 14;
            state.word_19e = if (decrement as i32) < current {
                state.word_19e.wrapping_sub(decrement as u16)
            } else {
                0
            };
        } else if target > current {
            let increment = ((u64::from(product) * 0x165e_9f81) >> 32) as u32 >> 12;
            let next = increment.wrapping_add(current as u32);
            state.word_19e = if next as i32 > 0xffff {
                0xffff
            } else {
                next as u16
            };
        }
        if best != 0 && state.word_194 == 0 && state.word_19e == 0xffff {
            let (source, row) = self.spawn_row(actor, selected)?;
            self.execute_spawn_row(actor, row, &source, host, rng)?;
            let retained = self.selector60_actor(actor)?.retained_190.as_ref().ok_or(
                Selector60UpdateError::RequiredField {
                    actor,
                    offset: 0x190,
                    address: 0x0048_0afd,
                },
            )?;
            let delay = retained
                .allocation()
                .runtime_allocation()
                .word(add(retained.allocation_offset, 2)?)
                .map_err(Selector60Error::from)? as i16;
            self.selector60_actor_mut(actor)?.word_194 =
                (i32::from(delay).wrapping_mul(100_000)) as u32;
        }
        Ok(())
    }
}

#[cfg(test)]
mod native_receipts;
