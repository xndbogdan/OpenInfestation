//! Shared 004665D0/00466710/00466850 factory and creation-notice owners.
//! Source bytes retain their captured pointer identity through live services.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{
    ParticleAllocationRequest, ParticleConstruction, ParticleError, ParticleId,
    ParticleVectorBytes, ProcessParticleRuntime,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{ActorHandle, BehaviorPacketSlot},
};

mod deletion;
mod emitter;
pub use emitter::ParticleEmitterFactoryRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleFactoryKind {
    Factory4665D0,
    Factory466710,
    Factory466850,
}

impl ParticleFactoryKind {
    fn notice_family(self) -> u32 {
        match self {
            Self::Factory4665D0 => 0,
            Self::Factory466710 => 1,
            Self::Factory466850 => 2,
        }
    }
}

/// Captured pointer identity; equal vector values do not imply aliasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleVelocitySource {
    Null,
    SamePosition,
    Separate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleFactoryRequest {
    pub kind: ParticleFactoryKind,
    /// Full native ECX DWORD, including unsupported high bits.
    pub selector: u32,
    pub source_handle: ActorHandle,
    pub flags: u32,
    pub velocity_source: ParticleVelocitySource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticleFactorySources {
    pub position: ParticleVectorBytes,
    pub separate_velocity: Option<ParticleVectorBytes>,
}

/// Explicit live process operands; the particle pool does not retain these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleFactoryProcessState {
    pub network_flag: u32,
    pub process_null_handle: Option<ActorHandle>,
    /// `0055F2F4`, this machine's slot: 1 offline (`00493150`), the
    /// session's slot in a network game. New actors' handles carry it.
    pub local_slot: u8,
    /// `0055F3A0`, set on the network game's host.
    pub host_flag: u32,
    /// Settings `+00..+4F` (`00463AB0`) as the network game reads them:
    /// the mode `+10` (`0053C570`'s `+10` of the host's arena, kept in step
    /// on a guest by the host's header), the team `+14`, the vehicle `+18`
    /// and the arena `+1C`. Synced with the flags above.
    pub settings: [u32; 20],
}

impl ParticleFactoryProcessState {
    /// Supported native process startup: 0054E894 and 0055F448 lie wholly
    /// in the PE loader's zero-filled .data tail, without overlapping base
    /// relocations. This initializes retained state once, never a read fallback.
    /// Session+64C has its own live owner; no factory phase is supplied here.
    pub const fn native_startup() -> Self {
        Self {
            network_flag: 0,
            process_null_handle: Some(ActorHandle::from_raw(0)),
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
        }
    }
}

/// The pool is the method receiver and is passed to services exactly once.
/// Stable EXE backing is read-only; notifier mutations of it are not admitted.
pub struct ParticleFactoryContext<'a, 'exe> {
    pub process: &'a mut ParticleFactoryProcessState,
    pub rng: &'a mut u32,
    pub executable: FrontendExecutable<'exe>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleCreationNotice {
    pub event: u32,
    pub code: u32,
    /// Event17 leaves the fourth native DWORD unwritten.
    pub source_handle: Option<ActorHandle>,
    pub velocity_source: ParticleVelocitySource,
}

/// `00492500(1, 0x17 or 0x18, &notice)`, which `0045E960` and `0045EA60`
/// encode: the particle's position and velocity, its factory and selector,
/// and for 0x18 its source. The particle runtime keeps it for the next
/// network pass.
pub fn queue_creation_notice(
    particles: &mut ProcessParticleRuntime,
    notice: ParticleCreationNotice,
    sources: &ParticleFactorySources,
) -> Result<(), &'static str> {
    #[cfg(test)]
    if particles.refuse_network_sends {
        return Err("a receipt's 00492500 boundary");
    }
    let bytes = |vector: ParticleVectorBytes| -> Option<[u8; 6]> {
        let mut out = [0; 6];
        for (out, byte) in out.iter_mut().zip(vector.0) {
            *out = byte?;
        }
        Some(out)
    };
    let position = bytes(sources.position)
        .ok_or("the particle's position is unwritten where 0045E960 copies it")?;
    let velocity = match notice.velocity_source {
        ParticleVelocitySource::Null => {
            return Err("the particle's velocity pointer is null where 0045E960 reads it")
        }
        ParticleVelocitySource::SamePosition => position,
        ParticleVelocitySource::Separate => sources
            .separate_velocity
            .and_then(bytes)
            .ok_or("the particle's velocity is unwritten where 0045E960 copies it")?,
    };
    particles.queue_network_send(
        notice.event,
        crate::network::protocol::GameSend::Points {
            points: [position, velocity],
            kind: notice.code,
            extra: notice.source_handle.map(|handle| handle.raw()),
        },
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleDeleteRequest {
    pub particle: ParticleId,
    /// Captured DWORD; the deleting wrapper consumes only low BYTE bit0.
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParticleFactoryImpactRequest {
    pub particle: ParticleId,
    pub selector: u32,
    pub flags: u32,
}

pub trait ParticleFactoryHost {
    type Error;
    /// Fresh manager+64C after the synchronous notice, required only when
    /// the subsequent actual network DWORD is zero. No retained phase copy.
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error>;
    /// Required only after actual 00492500's network gate. Position and
    /// velocity sources stay live and unknown data is not interpreted here.
    fn network_notice(
        &mut self,
        request: ParticleCreationNotice,
        particles: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error>;

    /// Actual 00489600 argument identities: live position +04, velocity +0A.
    /// The normalizer's step is retained by its caller across this service.
    fn impact_489600(
        &mut self,
        request: ParticleFactoryImpactRequest,
        particles: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum ParticleFactoryError<E> {
    #[error(transparent)]
    Particle(#[from] ParticleError),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
    #[error("required process null 0054E894 is unavailable at 00466030")]
    MissingProcessNull,
    #[error("required emitter process null0054E894 is unavailable at{address:08X}")]
    MissingEmitterProcessNull { address: u32 },
    #[error("captured separate velocity pointer requires six available source bytes")]
    MissingVelocitySource,
    #[error("required network creation service failed at 00492500: {0}")]
    Network(E),
    #[error("required current manager phase service failed at 004391B0: {0}")]
    Phase(E),
    #[error("required impact service failed at 00466394: {0}")]
    Impact(E),
}

enum Parameters {
    Record(u32),
    Local850 { packet: u32, scale: u32 },
}

fn select(request: ParticleFactoryRequest) -> Option<Parameters> {
    let address = match (request.kind, request.selector) {
        (ParticleFactoryKind::Factory4665D0, 1) => 0x539ec8,
        (ParticleFactoryKind::Factory4665D0, 11) => 0x539ed8,
        (ParticleFactoryKind::Factory4665D0, 12) => 0x539ee8,
        (ParticleFactoryKind::Factory466710, 2) => 0x539f28,
        (ParticleFactoryKind::Factory466710, 3) => 0x539f38,
        (ParticleFactoryKind::Factory466710, 4) => 0x539f48,
        (ParticleFactoryKind::Factory466710, 5) => 0x539ef8,
        (ParticleFactoryKind::Factory466710, 6) => 0x539f08,
        (ParticleFactoryKind::Factory466710, 7) => 0x539f18,
        (ParticleFactoryKind::Factory466850, 8) => {
            return Some(Parameters::Local850 {
                packet: 0x539ea0,
                scale: 0x400,
            })
        }
        (ParticleFactoryKind::Factory466850, 9) => {
            return Some(Parameters::Local850 {
                packet: 0x539e9c,
                scale: 0x200,
            })
        }
        _ => return None,
    };
    Some(Parameters::Record(address))
}

pub(super) fn dword(
    executable: FrontendExecutable<'_>,
    address: u32,
) -> Result<u32, FrontendExecutableError> {
    Ok(u32::from_le_bytes(
        executable
            .mapped(address, 4)?
            .bytes
            .try_into()
            .expect("DWORD"),
    ))
}

fn word(executable: FrontendExecutable<'_>, address: u32) -> Result<u16, FrontendExecutableError> {
    Ok(u16::from_le_bytes(
        executable
            .mapped(address, 2)?
            .bytes
            .try_into()
            .expect("WORD"),
    ))
}

impl ProcessParticleRuntime {
    /// Reached 00466120 source read. Transfer order/destination custody belongs
    /// to the shared behavior cache; each slot is resolved independently.
    pub fn factory_packet_source(
        &self,
        particle: ParticleId,
        slot: BehaviorPacketSlot,
        executable: FrontendExecutable<'_>,
    ) -> Result<Option<[Option<u8>; 4]>, ParticleFactoryError<std::convert::Infallible>> {
        let offset = match slot {
            BehaviorPacketSlot::First => 0x34,
            BehaviorPacketSlot::Second => 0x38,
        };
        let pointer = self.record(particle)?.read_dword(offset)?;
        if pointer == 0 {
            return Ok(None);
        }
        let source = executable.mapped(pointer, 4)?.bytes;
        Ok(Some(std::array::from_fn(|i| Some(source[i]))))
    }

    pub fn create_factory_particle<H: ParticleFactoryHost>(
        &mut self,
        request: ParticleFactoryRequest,
        context: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
        host: &mut H,
    ) -> Result<Option<ParticleId>, ParticleFactoryError<H::Error>> {
        let Some(parameters) = select(request) else {
            return Ok(None);
        };
        if request.flags & 1 == 0 {
            let null = context
                .process
                .process_null_handle
                .ok_or(ParticleFactoryError::MissingProcessNull)?;
            let notice = ParticleCreationNotice {
                event: if request.source_handle == null {
                    0x17
                } else {
                    0x18
                },
                code: (request.kind.notice_family() << 16) | request.selector,
                source_handle: (request.source_handle != null).then_some(request.source_handle),
                velocity_source: request.velocity_source,
            };
            if context.process.network_flag != 0 {
                host.network_notice(notice, self, context, sources)
                    .map_err(ParticleFactoryError::Network)?;
            }
        }
        // Allocation reads these operands again after the synchronous notice.
        let network_active = context.process.network_flag != 0;
        let phase = if network_active {
            None // Native online path never calls the manager getter.
        } else {
            host.current_phase().map_err(ParticleFactoryError::Phase)?
        };
        let allocation = ParticleAllocationRequest {
            payload_bytes: 0x40,
            last_reclaim_category: 3,
            network_active,
            phase,
        };
        let id = self.allocate_reclaiming(allocation, |particles, particle| {
            particles.delete_factory_particle(
                ParticleDeleteRequest { particle, flags: 1 },
                context,
                host,
            )
        })?;
        // Original +08,+04,+0C,+00 accesses follow the successful allocation.
        let (packet0, scale, packet1, animation) = match parameters {
            Parameters::Record(address) => (
                dword(context.executable, address + 8)?,
                dword(context.executable, address + 4)?,
                dword(context.executable, address + 12)?,
                dword(context.executable, address)?,
            ),
            Parameters::Local850 { packet, scale } => (packet, scale, 0, 0x539e70),
        };
        self.construct_resolving_velocity(
            id,
            ParticleConstruction {
                category: 3,
                position: sources.position,
                velocity: None,
                source_handle: request.source_handle.raw(),
                flags: request.flags | 2,
            },
            || match request.velocity_source {
                ParticleVelocitySource::Null => Ok(None),
                ParticleVelocitySource::SamePosition => Ok(Some(sources.position)),
                ParticleVelocitySource::Separate => sources
                    .separate_velocity
                    .map(Some)
                    .ok_or(ParticleFactoryError::MissingVelocitySource),
            },
        )?;
        let record = self.record_mut(id)?;
        record.write_dword(0x28, animation)?;
        record.write_dword(0, 0x526f40)?;
        let lifetime = if request.kind == ParticleFactoryKind::Factory466850 {
            dword(context.executable, 0x539e7c)?.wrapping_sub(1) as u16
        } else {
            word(context.executable, animation.wrapping_add(12))?.wrapping_sub(1)
        };
        record.write_word(0x10, lifetime)?;
        if request.kind == ParticleFactoryKind::Factory466710 {
            record.write_dword(0x38, packet1)?;
            record.write_dword(0x2c, scale)?;
            record.write_dword(0x34, packet0)?;
        } else {
            record.write_dword(0x34, packet0)?;
            record.write_dword(0x38, packet1)?;
            record.write_dword(0x2c, scale)?;
        }
        let vtable = match request.kind {
            ParticleFactoryKind::Factory4665D0 => 0x526f20,
            ParticleFactoryKind::Factory466710 => 0x526efc,
            ParticleFactoryKind::Factory466850 => 0x526f60,
        };
        record.write_dword(0, vtable)?;
        record.write_dword(0x3c, request.selector)?;
        if request.kind == ParticleFactoryKind::Factory4665D0 {
            let random = name_entry_rng_next(context.rng);
            record.write_word(0x10, (random as u16 & 0x3ff) + 0x100)?;
        }
        Ok(Some(id))
    }
}

#[cfg(test)]
#[path = "../object_runtime/actor/behavior/damage_cache/particle_adapter/native_receipts.rs"]
mod behavior_particle_adapter_native_receipts;
#[cfg(test)]
mod native_receipts;
#[cfg(test)]
pub(crate) mod phase_fixture;
#[cfg(test)]
mod phase_tests;
#[cfg(test)]
mod startup_native_receipts;
#[cfg(test)]
mod tests;
