//! Complete per-item 004359A0 orchestration. Native 00435690's basis phase
//! and 00435870's packed query share the same process/record custody.
//! Resource residency, renderer publication, real trail births and H1 execution
//! are required services; failures retain each reached prefix.

use inf_formats::{
    native_basis::{NativeBasis, NativeBasisMath},
    TerrainRecord, TerrainResourceLookup,
};
use thiserror::Error;

use crate::{
    frontend_scene::SceneView,
    terrain_runtime::{
        TerrainRuntime, TerrainSparsePayload, TerrainSparseRecordId, TerrainSparseWriteError,
    },
    world_furniture_effect::FurnitureEffectQueue,
};

/// 00546B58 and the sixteen 40-byte bases at00546B88. The CRT's initializers
/// build them as identities (004353A0, which also gives 00546B48 the base
/// vtable 00520384, and 004354F0). The service's first
/// acquisition starts with BSS counter0. Reacquisition preserves its current
/// counter; world changes do not independently rebuild these process bases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FurnitureDrawState {
    pub current_basis: NativeBasis,
    bases: [NativeBasis; 16],
}

impl FurnitureDrawState {
    pub fn new(math: &NativeBasisMath) -> Self {
        let mut state = Self {
            current_basis: NativeBasis::INITIAL,
            bases: [NativeBasis::INITIAL; 16],
        };
        state.rebuild(math);
        state
    }

    /// Basis phase 00435690: copy the nine read-only CRT identity axes without
    /// writing00546B58, then copy all ten words and rotate each byindex*4096.
    /// The enclosing service acquisition separately clears/registers the queue.
    pub fn rebuild(&mut self, math: &NativeBasisMath) {
        self.current_basis.words[1..].copy_from_slice(&NativeBasis::INITIAL.words[1..]);
        for (index, basis) in self.bases.iter_mut().enumerate() {
            *basis = self.current_basis;
            math.rotate(basis, (index as i32) << 12);
        }
    }

    pub fn basis(&self, state: u8) -> NativeBasis {
        self.bases[usize::from(state >> 4)]
    }
}

/// The context fields read or temporarily replaced by 004359A0. Other renderer
/// projection fields stay with the host. Eye differences use wrapping WORDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureDrawContext {
    pub view: SceneView,
    pub fade: [i32; 2],
    pub far_color: u32,
}

pub struct FurnitureDrawRequest<'a> {
    pub terrain: &'a mut TerrainRuntime,
    pub record: TerrainSparseRecordId,
    /// Actual 0044CEF0 position, not a model-local vector.
    pub position: [i32; 3],
    pub context: &'a mut FurnitureDrawContext,
    pub pending: &'a FurnitureEffectQueue,
    /// Actual process0055F154 input to signed truncating division by2048.
    pub load_factor: i32,
    pub clock: u32,
    pub state: &'a mut FurnitureDrawState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureTrailRequest {
    pub particle_type: u32,
    pub position: [u16; 3],
    pub velocity: [u16; 3],
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureModelRequest {
    pub record: TerrainSparseRecordId,
    pub h1_slot: u16,
    pub position: [i32; 3],
    pub basis: NativeBasis,
    pub clock: u32,
}

/// A proven native null H1 is a normal no-draw return, unlike a missing header
/// dereference in the earlier pending fade branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnitureModelOutcome {
    Submitted,
    NoDraw,
}

pub trait FurnitureDrawHost {
    type Error;
    /// H11/0 first WORD at unsigned byteindex*4. Only zero skips this service.
    fn bind_h1(&mut self, lookup: TerrainResourceLookup) -> Result<u16, Self::Error>;
    /// H8 model first DWORD. This reached native dereference cannot use a
    /// null-model no-draw fallback and must preserve the full32-bit extent.
    fn model_extent(&mut self, h1_slot: u16) -> Result<u32, Self::Error>;
    /// First runtime DWORD of H7 Material slot32, reached after H8 succeeds.
    /// Its loader conversion belongs to the resource host, not this kernel.
    fn far_color32(&mut self) -> Result<u32, Self::Error>;
    /// 0048FB00 after each actual context fade write/normal-return restoration.
    fn publish_fade(&mut self, context: &FurnitureDrawContext) -> Result<(), Self::Error>;
    fn birth_trail(
        &mut self,
        request: FurnitureTrailRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error>;
    /// 0049FEA0 after all pre-H1 effects. Packed queries and model callbacks
    /// receive the current sparse-record identity, never a copied state byte.
    fn draw_h1(
        &mut self,
        terrain: &TerrainRuntime,
        context: &mut FurnitureDrawContext,
        request: FurnitureModelRequest,
        rng: &mut u32,
    ) -> Result<FurnitureModelOutcome, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnitureDrawOutcome {
    LookupZero,
    Drawn(FurnitureModelOutcome),
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum FurnitureDrawError<E> {
    #[error(transparent)]
    Record(#[from] TerrainSparseWriteError),
    #[error("furniture draw host failed at {effect}")]
    Host { effect: &'static str, error: E },
}

/// Native packed callback 00435870. The selector is an authored callback
/// argument:0 reads process clock lowWORD,1 rereads sparse lowtwo bits.
pub fn furniture_packed_value(selector: i32, clock: u32, record: TerrainRecord) -> u16 {
    match selector {
        0 => clock as u16,
        1 => u16::from(record.state & 3) << 14,
        _ => 0,
    }
}

pub fn draw_furniture_item<H: FurnitureDrawHost>(
    request: FurnitureDrawRequest<'_>,
    host: &mut H,
    rng: &mut u32,
) -> Result<FurnitureDrawOutcome, FurnitureDrawError<H::Error>> {
    let FurnitureDrawRequest {
        terrain,
        record: id,
        position,
        context,
        pending,
        load_factor,
        clock,
        state,
    } = request;
    let record = terrain
        .buckets()
        .get(usize::from(id.bucket))
        .and_then(|b| b.records.get(id.index))
        .copied()
        .ok_or(TerrainSparseWriteError { record: id })?;
    if record.resource_lookup.raw() == 0 {
        return Ok(FurnitureDrawOutcome::LookupZero);
    }
    let slot = host
        .bind_h1(record.resource_lookup)
        .map_err(|error| FurnitureDrawError::Host {
            effect: "H11 bind",
            error,
        })?;
    state.current_basis = state.basis(record.state);
    let low_bits = record.state & 3;
    let mut saved = None;
    if record.state & 8 != 0 {
        let mask = !(terrain.sampling().spacing.units() - 1);
        let key =
            u32::from(position[0] as u16 & mask) | (u32::from(position[2] as u16 & mask) << 16);
        if let Some(effect) = pending
            .records()
            .iter()
            .find(|effect| effect.aligned_key == key)
        {
            if effect.remaining_microseconds != 0 {
                let depth = signed_word_depth(position, context.view);
                let threshold = context.fade[1]
                    .wrapping_mul(3)
                    .wrapping_sub(context.fade[0])
                    / 2;
                if depth < threshold {
                    let extent = host
                        .model_extent(slot)
                        .map_err(|error| FurnitureDrawError::Host {
                            effect: "H8 extent",
                            error,
                        })?
                        .wrapping_mul(2) as i32;
                    // Original resource reads both precede the first context write.
                    let color = host
                        .far_color32()
                        .map_err(|error| FurnitureDrawError::Host {
                            effect: "H7/32 color",
                            error,
                        })?;
                    saved = Some((context.fade, context.far_color));
                    context.far_color = color;
                    let decay =
                        (0x004c_4b40_i32.wrapping_sub(effect.remaining_microseconds as i32) >> 2)
                            .wrapping_mul(extent)
                            .wrapping_shl(1)
                            >> 15;
                    context.fade = [
                        extent
                            .wrapping_sub(i32::from(decay as i16))
                            .wrapping_add(depth),
                        extent.wrapping_add(depth),
                    ];
                    host.publish_fade(context)
                        .map_err(|error| FurnitureDrawError::Host {
                            effect: "fade publication",
                            error,
                        })?;
                }
            }
        } else {
            terrain.write_sparse_payload(
                id,
                TerrainSparsePayload {
                    state: record.state & !8,
                    resource_lookup: record.resource_lookup,
                },
            )?;
        }
    }
    // The cached lowtwo bits precede queue work, and the velocity draw precedes
    // the separate load-factor probability draw. No rollback on later failures.
    if low_bits != 0 {
        let velocity_y = crate::name_entry::name_entry_rng_next(rng) as u16 & 63;
        let factor = 0xc000_i32.wrapping_sub(load_factor) / 2048;
        if factor < 2
            || (crate::name_entry::name_entry_rng_next(rng) & 0xffff) % (factor as u32) == 0
        {
            host.birth_trail(
                FurnitureTrailRequest {
                    particle_type: 6,
                    position: position.map(|v| v as u16),
                    velocity: [0, velocity_y, 0],
                    flags: 0,
                },
                rng,
            )
            .map_err(|error| FurnitureDrawError::Host {
                effect: "trail birth",
                error,
            })?;
        }
    }
    let outcome = host
        .draw_h1(
            terrain,
            context,
            FurnitureModelRequest {
                record: id,
                h1_slot: slot,
                position,
                basis: state.current_basis,
                clock,
            },
            rng,
        )
        .map_err(|error| FurnitureDrawError::Host {
            effect: "H1 draw",
            error,
        })?;
    if let Some((fade, color)) = saved {
        context.far_color = color;
        context.fade = fade;
        host.publish_fade(context)
            .map_err(|error| FurnitureDrawError::Host {
                effect: "fade restoration",
                error,
            })?;
    }
    Ok(FurnitureDrawOutcome::Drawn(outcome))
}

///00435B84: wrap each lowWORD subtraction before sign-extension. Products
/// accumulate Z thenY thenX at retained precision; CRT__ftol truncates tozero.
fn signed_word_depth(position: [i32; 3], view: SceneView) -> i32 {
    let delta: [i16; 3] = std::array::from_fn(|axis| {
        (position[axis] as u16).wrapping_sub(view.eye[axis] as u16) as i16
    });
    let v = |axis: usize| f64::from(f32::from_bits(view.basis.words[7 + axis]));
    let depth = ((f64::from(delta[2]) * v(2) + f64::from(delta[1]) * v(1))
        + f64::from(delta[0]) * v(0))
    .trunc();
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if depth.is_finite() && (-LIMIT..LIMIT).contains(&depth) {
        depth as i64 as i32
    } else {
        0
    }
}

#[cfg(test)]
mod tests;
