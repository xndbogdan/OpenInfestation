//! Selector 79, world 19's eyes, against 00408740, 00408880, 004088C0,
//! 00408A70, 00408AB0, 00408DA0 and 00408790, with values worked by hand.

use inf_formats::{
    ByteSpan, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView, InstanceField,
};

use super::*;
use crate::object_runtime::{
    ActorAllocationPolicy, ActorCommandContext, ActorMessage, ActorMessageError,
    FollowTargetRequest, ObjectList, PlayerActorLookupContext, Selector79Message,
};

const A: [u16; 3] = [1000, 0, 1000];
const B: [u16; 3] = [1600, 0, 1800];
/// 100 either side of the pair's midpoint (1300, 0, 1400), along the
/// leader's forward (-0.8, 0, 0.6).
const NEAR_SIDE: [u16; 3] = [1220, 0, 1460];
const FAR_SIDE: [u16; 3] = [1380, 0, 1340];

fn arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap();
    arena
        .select_actor_allocation_policy(
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
        )
        .unwrap();
    arena
}

fn eye(arena: &mut ObjectArena, position: [u16; 3], placement: u16, partner: u32) -> ObjectId {
    let eye = arena
        .allocate_selector79_actor(construction(Some(position)), &mut 0)
        .unwrap();
    arena.owned_mut(eye).unwrap().state.base.placement_word = placement;
    arena.place_for_tests(eye, position);
    arena.selector79_mut(eye).unwrap().partner_1c4 = partner;
    let state = arena.selector77_update_state_mut(eye).unwrap();
    state.position_192 = Some(position);
    state.word_198 = 100_000_000;
    arena.actor_mut(eye).unwrap().word_c4 = 3;
    eye
}

/// The leader at A, its follower at B, both after-loads run.
fn pair(arena: &mut ObjectArena) -> (ObjectId, ObjectId) {
    let leader = eye(arena, A, 0, 2);
    let follower = eye(arena, B, 1, 1);
    arena.initialize_selector79_after_load(leader).unwrap();
    arena.initialize_selector79_after_load(follower).unwrap();
    (leader, follower)
}

/// Records the strike's 0047C6E0.
#[derive(Default)]
struct PairHost {
    host: Host,
    damage: Vec<(ObjectId, ActorDamagePacket, ActorHandle)>,
}

impl FamilyUpdateHost for PairHost {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.host.process_null_handle()
    }
    fn random(&mut self) -> Result<u32, Self::Error> {
        self.host.random()
    }
    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        self.host.effect(arena, actor, effect)
    }
}

impl Selector77UpdateHost for PairHost {
    fn apply_damage(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        packet: ActorDamagePacket,
        source: ActorHandle,
    ) -> Result<(), Selector77UpdateError<Self::Error>> {
        self.damage.push((actor, packet, source));
        Ok(())
    }
}

/// 00408740 after 00407410.
#[test]
fn construction_names_no_partner_and_leads() {
    let mut arena = arena();
    let eye = arena
        .allocate_selector79_actor(construction(Some(A)), &mut 0)
        .unwrap();
    let pair = arena.selector79(eye).unwrap();
    assert_eq!(pair.partner_1c4, NULL.raw());
    assert_eq!((pair.target_1c8, pair.struck_1cc), (NULL, NULL));
    assert_eq!((pair.strike_1d0, pair.leader_1d4), (0, 1));
    assert_eq!((pair.side_1bc, pair.aimed_1c0), (Some(0), Some(0)));
    assert_eq!((pair.midpoint_1b4, pair.reach_1ba), (None, None));
    // A selector 77 is not an eye.
    let plain = arena
        .allocate_selector77_actor(construction(Some(A)), &mut 0)
        .unwrap();
    assert!(arena.selector79(plain).is_err());
}

/// 00408880: 19 names the partner, through 18 is selector 77's, above
/// refuses.
#[test]
fn command_19_names_the_partner() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let context = || ActorCommandContext {
        runtime: &runtime,
        first_player_grants: None,
        terrain: None,
    };
    let view = |code, payload| InstanceCommandView {
        target: InstanceCommandTarget::Constructor(79),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0040_8880),
        payload_consumer: Some(0x0040_8880),
        payload,
    };
    let mut arena = arena();
    let eye = eye(&mut arena, A, 0, 0);
    let link = InstanceCommandPayload::Selector79Link(InstanceField {
        span: ByteSpan { start: 4, end: 8 },
        value: 0x5b,
    });
    assert!(arena
        .apply_selector79_command(eye, &view(19, link), context())
        .unwrap());
    assert_eq!(arena.selector79(eye).unwrap().partner_1c4, 0x5b);
    let range = InstanceCommandPayload::ActorWord164(InstanceField {
        span: ByteSpan { start: 4, end: 6 },
        value: 900,
    });
    assert!(arena
        .apply_selector79_command(eye, &view(17, range), context())
        .unwrap());
    assert_eq!(arena.derived_actor(eye).unwrap().word_164, 900);
    assert!(!arena
        .apply_selector79_command(
            eye,
            &view(20, InstanceCommandPayload::NoPayloadRead),
            context()
        )
        .unwrap());
}

/// 004088C0: the first after-load leads. Its partner 1000 away along
/// (0.6, 0, 0.8) gives the midpoint (1300, 0, 1400), a reach of 750 and a
/// forward of (-0.8, 0, 0.6); the partner follows and writes none of them.
#[test]
fn the_first_after_load_leads_and_faces_across_the_line() {
    let mut arena = arena();
    let (leader, follower) = pair(&mut arena);
    let state = arena.selector79(leader).unwrap();
    assert_eq!(state.leader_1d4, 1);
    assert_eq!(state.midpoint_1b4, Some([1300, 0, 1400]));
    assert_eq!(state.reach_1ba, Some(750));
    assert_eq!(
        state.partner_1c4,
        arena.actor(follower).unwrap().handle.raw()
    );
    let basis = arena.actor(leader).unwrap().rotating.basis;
    let forward: [f32; 3] = std::array::from_fn(|axis| f32::from_bits(basis.words[7 + axis]));
    assert_eq!(forward, [-0.8, 0.0, 0.6]);
    let state = arena.selector79(follower).unwrap();
    assert_eq!(state.leader_1d4, 0);
    assert_eq!(state.partner_1c4, arena.actor(leader).unwrap().handle.raw());
    assert_eq!((state.midpoint_1b4, state.reach_1ba), (None, None));

    // An eye whose partner never resolves disposes itself.
    let lone = eye(&mut arena, A, 5, 40);
    arena.initialize_selector79_after_load(lone).unwrap();
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == lone));
}

/// 00408C0C..00408D00 and 00408DA0: the target crosses between the pair,
/// the strike arms, and past 1 s both eyes draw their lines (509 apart:
/// ten steps, eleven particles each) and the leader deals 20000 on channel
/// 2, the packet's last byte the Z sum's top (-340 * 11).
#[test]
fn a_target_crossing_between_the_pair_is_struck_a_second_later() {
    let mut arena = arena();
    let (leader, follower) = pair(&mut arena);
    let target = arena
        .allocate_derived_actor(construction(Some(NEAR_SIDE)), &mut 0)
        .unwrap();
    arena.place_for_tests(target, NEAR_SIDE);
    arena.actor_mut(target).unwrap().word_c4 = 3;
    let handle = arena.actor(target).unwrap().handle;
    let follow = arena
        .follow_target_resource(FollowTargetRequest::resolve_actor(handle))
        .unwrap();
    arena.derived_actor_mut(leader).unwrap().target_148 = follow;
    let mut host = PairHost::default();
    arena
        .tick_selector79(leader, request(33_333), &mut host)
        .unwrap();
    let state = arena.selector79(leader).unwrap();
    assert_eq!((state.target_1c8, state.side_1bc), (handle, Some(100)));
    assert_eq!(state.strike_1d0, 0);

    // The follower copies the leader's target each update.
    arena
        .tick_selector79(follower, request(33_333), &mut host)
        .unwrap();
    let copy = arena.derived_actor(follower).unwrap().target_148.unwrap();
    assert_ne!(Some(copy), follow);
    assert!(matches!(
        arena.native_target(copy).unwrap(),
        crate::object_runtime::NativeTarget::ActorFollow(data) if data.handle_04 == handle
    ));

    arena.place_for_tests(target, FAR_SIDE);
    arena
        .tick_selector79(leader, request(33_333), &mut host)
        .unwrap();
    let state = arena.selector79(leader).unwrap();
    assert_eq!((state.strike_1d0, state.struck_1cc), (33_334, handle));
    assert_eq!(state.side_1bc, Some((-100_i16) as u16));
    assert!(host.damage.is_empty());

    let trails = |host: &PairHost| {
        host.host
            .effects
            .iter()
            .filter_map(|effect| match *effect {
                FamilyUpdateEffect::StatusTrail {
                    particle_type: 12,
                    position,
                    velocity,
                    flags: 0,
                } => Some((position, velocity)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    while arena.selector79(leader).unwrap().strike_1d0 <= 1_000_000 {
        assert!(trails(&host).is_empty());
        arena
            .tick_selector79(leader, request(33_333), &mut host)
            .unwrap();
    }
    let lines = trails(&host);
    assert_eq!(lines.len(), 22);
    let d = [(-380_i16) as u16, 0, (-340_i16) as u16];
    assert_eq!(lines[0], (A, d));
    assert_eq!(lines[10], (FAR_SIDE, d));
    assert_eq!(lines[11], (B, [220, 0, 460]));
    assert_eq!(lines[21], (FAR_SIDE, [220, 0, 460]));
    assert_eq!(
        host.damage,
        [(
            target,
            ActorDamagePacket {
                strength: 20_000,
                channel: 2,
                byte_03: Some(0xff),
            },
            arena.actor(leader).unwrap().handle,
        )]
    );
    assert_eq!(arena.selector79(follower).unwrap().struck_1cc, handle);
    assert_eq!(arena.selector79(follower).unwrap().strike_1d0, 1_000_001);
    assert!(arena.selector79(leader).unwrap().strike_1d0 > 1_000_000);
}

/// 00408A70: 3 makes a follower; 2 strikes only a follower with a sender,
/// a strike the update runs, so the general receiver stops at it.
#[test]
fn message_3_follows_and_message_2_strikes_from_a_follower() {
    let mut arena = arena();
    let (leader, follower) = pair(&mut arena);
    assert_eq!(
        arena.selector79_message(leader, Some(follower), 2).unwrap(),
        Selector79Message::Done
    );
    assert_eq!(
        arena.selector79_message(follower, None, 2).unwrap(),
        Selector79Message::Done
    );
    assert_eq!(
        arena.selector79_message(follower, Some(leader), 2).unwrap(),
        Selector79Message::Strike
    );
    let message = |code| ActorMessage {
        sender: Some(leader),
        distance_squared: 0,
        code,
        argument: 0,
    };
    let context = PlayerActorLookupContext { process: None };
    assert!(matches!(
        arena.receive_actor_message(follower, message(0xd), context, false),
        Ok(crate::object_runtime::ActorMessageReply::Done(0))
    ));
    assert!(matches!(
        arena.receive_actor_message(follower, message(2), context, false),
        Err(ActorMessageError::Unported {
            function: 0x0040_8a70,
            ..
        })
    ));
    arena
        .receive_actor_message(leader, message(3), context, false)
        .unwrap();
    assert_eq!(arena.selector79(leader).unwrap().leader_1d4, 0);
}

/// 00408790: sines of the phase and a third and two thirds of a turn on,
/// each >> 12, at the eye's X and Z.
#[test]
fn the_light_turns_through_three_sines() {
    let mut arena = arena();
    let eye = eye(&mut arena, A, 0, 0);
    let sine: [u16; 4096] = std::array::from_fn(|index| (index * 4) as u16);
    let light = arena.selector79_light(eye, 0, &sine).unwrap();
    assert_eq!(light.world_xz, [1000, 1000]);
    assert_eq!(light.radius, 0x200);
    // 0x5555 reads index 2730 (10920 >> 12 = 2); 0xAAAA the same, negated.
    assert_eq!(light.rgb, [0, 2, -3]);
}
