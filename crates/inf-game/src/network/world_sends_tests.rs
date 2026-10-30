//! The sends world code makes in a network game, and their order.

use super::protocol::GameSend;
use super::*;
use crate::object_runtime::ActorHandle;
use crate::process_particles::factories::{
    queue_creation_notice, ParticleCreationNotice, ParticleFactorySources, ParticleVelocitySource,
};
use crate::process_particles::{ParticleVectorBytes, ProcessParticleRuntime};
use crate::world_radius_effect::{radius_effect_send, RadiusEffectBlock};

/// The world's and the particles' sends leave in the order they were made.
#[test]
fn world_and_particle_sends_leave_in_the_order_made() {
    let mut arena = crate::object_runtime::ObjectArena::new();
    let mut particles = ProcessParticleRuntime::default();
    arena.queue_network_send(26, GameSend::State);
    particles.queue_network_send(23, GameSend::Dword(1));
    arena.queue_network_send(18, GameSend::Dword(2));
    particles.queue_network_send(24, GameSend::Dword(3));
    let sends = merge_sends(
        arena.take_ordered_network_sends(),
        particles.take_ordered_network_sends(),
    );
    let ids: Vec<_> = sends.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [26, 23, 18, 24]);
}

/// 004887D0's message 19 (0045DFF0): the source, the position's words and
/// the block's ten bytes; a block whose byte 3 nothing wrote stops.
#[test]
fn a_blast_carries_its_source_position_and_block() {
    let bytes = [0x10, 0x27, 2, 7, 0x64, 0, 0x2c, 1, 0x20, 0];
    let block = RadiusEffectBlock::from_authored_bytes(bytes);
    assert_eq!(block.bytes(), Some(bytes));
    let send = radius_effect_send(
        ActorHandle::from_raw(0x0801_0005),
        [1, 0x203, 0xfffe],
        block,
    )
    .unwrap();
    let GameSend::Effect {
        value,
        point,
        record,
    } = send
    else {
        panic!("{send:?}");
    };
    let mut out = [0; 20];
    assert_eq!(
        encoders::encode_effect(&mut out, 20, value, point, record),
        20
    );
    assert_eq!(&out[..4], &0x0801_0005_u32.to_le_bytes());
    assert_eq!(&out[4..10], &[1, 0, 3, 2, 0xfe, 0xff]);
    assert_eq!(&out[10..], &bytes);
    let unwritten = RadiusEffectBlock {
        byte_03: None,
        ..block
    };
    assert!(radius_effect_send(ActorHandle::from_raw(1), [0; 3], unwritten).is_err());
}

/// The factories' notice (0x17 without a source, 0x18 with one): the
/// position, the velocity (the position itself, or its own six bytes), the
/// factory and selector; a null velocity stops where 0045E960 reads it.
#[test]
fn a_particle_notice_carries_its_position_velocity_and_source() {
    let mut particles = ProcessParticleRuntime::default();
    let sources = ParticleFactorySources {
        position: ParticleVectorBytes::from_words([1, 2, 3]),
        separate_velocity: Some(ParticleVectorBytes::from_words([4, 5, 6])),
    };
    let notice = |event, source, velocity| ParticleCreationNotice {
        event,
        code: 1 << 16 | 0x22,
        source_handle: source,
        velocity_source: velocity,
    };
    queue_creation_notice(
        &mut particles,
        notice(
            0x18,
            Some(ActorHandle::from_raw(7)),
            ParticleVelocitySource::Separate,
        ),
        &sources,
    )
    .unwrap();
    queue_creation_notice(
        &mut particles,
        notice(0x17, None, ParticleVelocitySource::SamePosition),
        &sources,
    )
    .unwrap();
    assert!(queue_creation_notice(
        &mut particles,
        notice(0x17, None, ParticleVelocitySource::Null),
        &sources,
    )
    .is_err());
    let sends = merge_sends(Vec::new(), particles.take_ordered_network_sends());
    assert_eq!(
        sends,
        [
            (
                0x18,
                GameSend::Points {
                    points: [[1, 0, 2, 0, 3, 0], [4, 0, 5, 0, 6, 0]],
                    kind: 1 << 16 | 0x22,
                    extra: Some(7),
                }
            ),
            (
                0x17,
                GameSend::Points {
                    points: [[1, 0, 2, 0, 3, 0]; 2],
                    kind: 1 << 16 | 0x22,
                    extra: None,
                }
            ),
        ]
    );
}
