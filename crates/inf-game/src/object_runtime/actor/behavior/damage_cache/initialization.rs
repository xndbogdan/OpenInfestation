//! Unready-cache phases in 0048D820. The captured row stays fixed while
//! creation, component queries and deletion retain the live mutable owners.

use super::{
    packets::{self, PacketCopyPhase},
    storage, BehaviorComponentQuery, BehaviorCreationRequest, BehaviorPacketSlot,
    BehaviorPacketSourceRequest, BehaviorPredicateContext, BehaviorPredicateError,
    BehaviorPredicateHost, BehaviorRowSelection, BehaviorTemporary,
};
use crate::object_runtime::ObjectArena;

pub(super) struct CacheInitialization<'a, 'state, H> {
    pub(super) arena: &'a mut ObjectArena,
    pub(super) context: &'a mut BehaviorPredicateContext<'state>,
    pub(super) host: &'a mut H,
    pub(super) row: BehaviorRowSelection,
}

impl<H: BehaviorPredicateHost> CacheInitialization<'_, '_, H> {
    pub(super) fn initialize(&mut self) -> Result<(), BehaviorPredicateError<H::Error>> {
        let kind = storage::read_row_word(self.arena, self.context.tables, self.row, 0)? as i16;
        let first = self.context.tables.default_bytes()?;
        storage::write_row(self.arena, self.context.tables, self.row, 0x24, &first)?;
        let second = self.context.tables.default_bytes()?;
        storage::write_row(self.arena, self.context.tables, self.row, 0x28, &second)?;
        let registry = 0x0053_dba8_u32.wrapping_add(i32::from(kind).wrapping_mul(16) as u32);
        let factory = self.context.tables.read_dword(registry)?;
        if factory != 0 {
            self.initialize_particle(registry, factory)
        } else if self.context.tables.read_dword(registry.wrapping_add(8))? == 0 {
            storage::write_row(self.arena, self.context.tables, self.row, 0x22, &[Some(1)])?;
            Ok(())
        } else {
            self.initialize_actor(registry)
        }
    }

    /// 0048D87C..0048D8B0: null/context reads, create, ready, two packet
    /// sources and required deleting virtual +00(1), in that order.
    fn initialize_particle(
        &mut self,
        registry: u32,
        factory: u32,
    ) -> Result<(), BehaviorPredicateError<H::Error>> {
        let null_handle = self.context.process.process_null_handle.ok_or(
            BehaviorPredicateError::MissingProcessNull {
                address: 0x0048_d87c,
            },
        )?;
        let context_word = self.context.tables.read_dword(registry.wrapping_add(4))?;
        let created = self
            .host
            .create(
                self.arena,
                self.context,
                BehaviorCreationRequest::ParticleFactory {
                    callback: factory,
                    context_word,
                    null_handle,
                    scratch_bytes: [None; 8],
                },
            )
            .map_err(|error| BehaviorPredicateError::Host {
                address: 0x0048_d891,
                error,
            })?;
        let Some(created) = created else {
            return Ok(());
        };
        storage::write_row(self.arena, self.context.tables, self.row, 0x22, &[Some(1)])?;
        let BehaviorTemporary::Particle(particle) = created else {
            return Err(BehaviorPredicateError::TemporaryClass {
                address: 0x0048_d893,
            });
        };
        for slot in [BehaviorPacketSlot::First, BehaviorPacketSlot::Second] {
            let source = self
                .host
                .packet_source(
                    self.arena,
                    self.context,
                    BehaviorPacketSourceRequest::ParticlePointer { particle, slot },
                )
                .map_err(|error| BehaviorPredicateError::Host {
                    address: 0x0046_6120,
                    error,
                })?;
            if let Some(source) = source {
                packets::copy(
                    self.arena,
                    self.context.tables,
                    self.row,
                    PacketCopyPhase::Particle466120(slot),
                    source,
                )?;
            }
        }
        self.host
            .delete_particle(self.arena, self.context, particle, 1)
            .map_err(|error| BehaviorPredicateError::Host {
                address: 0x0048_d8b0,
                error,
            })
    }

    /// 0048D8BB..0048D924: retain source-null/slot read ordering and mark
    /// ready before any query. A failed query/copy skips later disposal.
    fn initialize_actor(&mut self, registry: u32) -> Result<(), BehaviorPredicateError<H::Error>> {
        let null_handle = self.context.process.process_null_handle.ok_or(
            BehaviorPredicateError::MissingProcessNull {
                address: 0x0048_d8bb,
            },
        )?;
        // Native +08 is reread as a WORD only after the process-null read.
        let resource = self.context.tables.read_word(registry.wrapping_add(8))? as i16;
        let created = self
            .host
            .create(
                self.arena,
                self.context,
                BehaviorCreationRequest::ActorResource {
                    resource,
                    null_handle,
                    scratch_bytes: [None; 8],
                },
            )
            .map_err(|error| BehaviorPredicateError::Host {
                address: 0x0048_d8d3,
                error,
            })?;
        let Some(created) = created else {
            return Ok(());
        };
        storage::write_row(self.arena, self.context.tables, self.row, 0x22, &[Some(1)])?;
        let BehaviorTemporary::Actor(actor) = created else {
            return Err(BehaviorPredicateError::TemporaryClass {
                address: 0x0048_d8d8,
            });
        };
        let component = self
            .host
            .query_component(
                self.arena,
                self.context,
                BehaviorComponentQuery::Actor18(actor),
            )
            .map_err(|error| BehaviorPredicateError::Host {
                address: 0x0048_d8e6,
                error,
            })?;
        if let Some(component) = component {
            self.copy_component_packet(component)?;
        }
        // Actual 00479FA0 queues disposal and keeps the allocation live.
        self.arena.dispose(actor)?;
        Ok(())
    }

    /// Actual +6C wins over +68. Its channel store precedes the strength
    /// source read; both copy policies leave the default second packet intact.
    fn copy_component_packet(
        &mut self,
        component: H::Component,
    ) -> Result<(), BehaviorPredicateError<H::Error>> {
        let first = self
            .host
            .query_component(
                self.arena,
                self.context,
                BehaviorComponentQuery::Component6c(component),
            )
            .map_err(|error| BehaviorPredicateError::Host {
                address: 0x0048_d8f3,
                error,
            })?;
        if let Some(first) = first {
            for (request, phase, address) in [
                (
                    BehaviorPacketSourceRequest::Channel148(first),
                    PacketCopyPhase::Channel4863da,
                    0x0048_63d4,
                ),
                (
                    BehaviorPacketSourceRequest::Strength146(first),
                    PacketCopyPhase::Strength4863e4,
                    0x0048_63dd,
                ),
            ] {
                let source = self
                    .host
                    .packet_source(self.arena, self.context, request)
                    .map_err(|error| BehaviorPredicateError::Host { address, error })?
                    .ok_or(BehaviorPredicateError::PacketSource { address })?;
                packets::copy(self.arena, self.context.tables, self.row, phase, source)?;
            }
        } else {
            let second = self
                .host
                .query_component(
                    self.arena,
                    self.context,
                    BehaviorComponentQuery::Component68(component),
                )
                .map_err(|error| BehaviorPredicateError::Host {
                    address: 0x0048_d90f,
                    error,
                })?;
            if let Some(second) = second {
                let packet = self
                    .host
                    .packet_source(
                        self.arena,
                        self.context,
                        BehaviorPacketSourceRequest::Complete13e(second),
                    )
                    .map_err(|error| BehaviorPredicateError::Host {
                        address: 0x0045_8f50,
                        error,
                    })?
                    .ok_or(BehaviorPredicateError::PacketSource {
                        address: 0x0045_8f50,
                    })?;
                packets::copy(
                    self.arena,
                    self.context.tables,
                    self.row,
                    PacketCopyPhase::Complete458f50,
                    packet,
                )?;
            }
        }
        Ok(())
    }
}
