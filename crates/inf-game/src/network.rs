//! Retail's network layer ([NETWORK_RUNTIME.md](../../../docs/re/NETWORK_RUNTIME.md)).
//! Retail carries it over DirectPlay, which the port does not reproduce: these
//! owners are the transport's rings, framing and built-in protocols, which a
//! port transport drives in DirectPlay's place.

pub mod encoders;
pub mod frontend_host;
pub mod furniture;
pub mod game;
pub mod game_tick;
pub mod lines;
pub mod link;
pub mod notices;
pub mod protocol;
pub mod receivers;
pub mod ring;
pub mod session;
pub mod sync;
pub mod transport;
pub mod udp;

pub use ring::{MessageRing, RingError};

/// A send world code made (`00492500`), stamped with its place among every
/// send of the process: the world's and the particles' queues leave as one,
/// in the order retail made them.
pub type OrderedSend = (u64, u32, protocol::GameSend);

pub(crate) fn next_send_order() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The world's and the particles' sends in the order they were made.
pub fn merge_sends(
    mut world: Vec<OrderedSend>,
    particles: Vec<OrderedSend>,
) -> Vec<(u32, protocol::GameSend)> {
    world.extend(particles);
    world.sort_by_key(|send| send.0);
    world.into_iter().map(|(_, id, send)| (id, send)).collect()
}
pub use transport::{
    NetProtocol, NetTransport, PlayerTableProtocol, ProtocolRow, SequenceProtocol, TransportError,
    PLAYER_TABLE_ENTRIES,
};

#[cfg(test)]
#[path = "network/world_sends_tests.rs"]
mod world_sends_tests;
