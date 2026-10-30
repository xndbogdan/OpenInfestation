use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectClassKind, ObjectList,
};
use inf_formats::{ByteSpan, InstanceField};

const SINE: [u16; 4096] = [0; 4096];

fn field<T>(value: T) -> InstanceField<T> {
    InstanceField {
        span: ByteSpan::new(0, 0),
        value,
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0x1234),
    })
    .unwrap()
}

fn actor(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    let actor = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some(position),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [0, 0],
            },
            &mut 0,
        )
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
    actor
}

/// Actor command 18 with a nonzero DWORD: +144 negated, flags 04200000.
fn put_to_sleep(arena: &mut ObjectArena, actor: ObjectId, seconds: i32) {
    arena.derived_actor_mut(actor).unwrap().word_144 = -seconds;
    arena.owned_mut(actor).unwrap().state.base.flags |= 0x0420_0000;
}

fn trigger(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    let object = arena.allocate(ObjectClassKind::Selector42);
    assert_eq!(*arena.selector42(object).unwrap(), Selector42State::new());
    arena.owned_mut(object).unwrap().state.base.position_words = Some(position);
    object
}

fn disposed(arena: &ObjectArena, object: ObjectId) -> bool {
    arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == object)
}

/// 00401080: codes 3/4 square WORD radii, 5/6 keep BYTE != 0.
#[test]
fn commands_store_squared_radii_and_flags() {
    let mut arena = arena();
    let object = trigger(&mut arena, [0; 3]);
    for (payload, expected) in [
        (
            InstanceCommandPayload::Selector42Radius(field(0xffff)),
            Selector42State {
                radius_squared_44: 0xfffe_0001,
                ..Selector42State::new()
            },
        ),
        (
            InstanceCommandPayload::Selector42WakeRadius(field(300)),
            Selector42State {
                radius_squared_44: 0xfffe_0001,
                wake_squared_48: 90_000,
                ..Selector42State::new()
            },
        ),
        (
            InstanceCommandPayload::Selector42OnDraw(field(7)),
            Selector42State {
                radius_squared_44: 0xfffe_0001,
                wake_squared_48: 90_000,
                on_draw_4c: 1,
                ..Selector42State::new()
            },
        ),
        (
            InstanceCommandPayload::Selector42Disabled(field(0x80)),
            Selector42State {
                radius_squared_44: 0xfffe_0001,
                wake_squared_48: 90_000,
                on_draw_4c: 1,
                disabled_4d: 1,
                ..Selector42State::new()
            },
        ),
    ] {
        assert!(arena.apply_selector42_command(object, &payload).unwrap());
        assert_eq!(*arena.selector42(object).unwrap(), expected);
    }
    assert!(!arena
        .apply_selector42_command(object, &InstanceCommandPayload::NoPayloadRead)
        .unwrap());
}

/// 00401110: only dormant actors within the wake radius on the ground
/// (inclusive, Y ignored) wake: +144 times -1000 and flags 04600000
/// cleared. The trigger then disposes itself.
#[test]
fn firing_wakes_dormant_actors_in_reach_and_disposes_the_trigger() {
    let mut arena = arena();
    let near = actor(&mut arena, [100, 0x7000, 0]);
    let far = actor(&mut arena, [0, 0, 101]);
    let awake = actor(&mut arena, [10, 0, 10]);
    put_to_sleep(&mut arena, near, 3);
    put_to_sleep(&mut arena, far, 4);
    let object = trigger(&mut arena, [0; 3]);
    arena
        .apply_selector42_command(
            object,
            &InstanceCommandPayload::Selector42WakeRadius(field(100)),
        )
        .unwrap();
    arena
        .apply_selector42_command(object, &InstanceCommandPayload::Selector42OnDraw(field(1)))
        .unwrap();
    let awake_flags = arena.object(awake).unwrap().base.flags;
    // The briefing (UI state 3) never fires a drawn trigger.
    arena.draw_selector42(object, 3).unwrap();
    assert!(!disposed(&arena, object));
    arena.draw_selector42(object, 4).unwrap();
    assert!(disposed(&arena, object));
    assert_eq!(arena.derived_actor(near).unwrap().word_144, 3000);
    assert_eq!(arena.object(near).unwrap().base.flags & 0x0460_0000, 0);
    assert_eq!(arena.derived_actor(far).unwrap().word_144, -4);
    assert_eq!(
        arena.object(far).unwrap().base.flags & 0x0460_0000,
        0x0420_0000
    );
    assert_eq!(arena.object(awake).unwrap().base.flags, awake_flags);
}

/// 004011F0: the first player's primary actor fires the trigger only
/// strictly inside +44; +4D disables the update.
#[test]
fn the_update_fires_strictly_inside_the_trigger_radius() {
    let mut arena = arena();
    let vehicle = actor(&mut arena, [30, 0, 40]);
    let sleeper = actor(&mut arena, [0, 0, 0]);
    put_to_sleep(&mut arena, sleeper, 1);
    let player = arena.add_world_player([0; 6]);
    let handle = arena.actor(vehicle).unwrap().handle;
    let null = ActorHandle::from_raw(0x1234);
    {
        let state = arena.world_player_mut(player).unwrap();
        state.handle_d4 = handle;
        state.handle_d8 = null;
    }
    let object = trigger(&mut arena, [0; 3]);
    // Exactly on the radius (50 squared) does not fire.
    arena
        .apply_selector42_command(object, &InstanceCommandPayload::Selector42Radius(field(50)))
        .unwrap();
    arena.tick_selector42(object).unwrap();
    assert!(!disposed(&arena, object));
    arena
        .apply_selector42_command(
            object,
            &InstanceCommandPayload::Selector42Disabled(field(1)),
        )
        .unwrap();
    arena
        .apply_selector42_command(object, &InstanceCommandPayload::Selector42Radius(field(51)))
        .unwrap();
    arena.tick_selector42(object).unwrap();
    assert!(!disposed(&arena, object));
    arena
        .apply_selector42_command(
            object,
            &InstanceCommandPayload::Selector42Disabled(field(0)),
        )
        .unwrap();
    arena.tick_selector42(object).unwrap();
    assert!(disposed(&arena, object));
    assert_eq!(arena.derived_actor(sleeper).unwrap().word_144, 1000);
}

/// Infest02's two firings (bundle 20261008-152553-576-wake02): 1E4848:132F from
/// the update (trigger radius 3000, wake radius 1000) and 1EF4DF:13B9 from
/// the draw (+4C and +4D set, default wake radius 2500). Every G+48 actor
/// flagged 0x200000 at entry, with its flags and +144 before and after.
#[test]
fn firing_matches_infest02() {
    type Sleeper = ([u16; 3], u32, u32, i32, i32);
    let firings: [([u16; 3], u32, u32, &[Sleeper]); 2] = [
        (
            [0x5b9c, 0xffd0, 0x95bc],
            0x0089_5440,
            0x000f_4240,
            &[
                ([0x3938, 0x110, 0x9259], 0x44b0_002f, 0x44b0_002f, -1, -1),
                ([0x871f, 0x230, 0xaa56], 0x44b0_002f, 0x44b0_002f, -1, -1),
                ([0x4b55, 0xffd0, 0x87f8], 0x44b0_0867, 0x44b0_0867, -1, -1),
                ([0x49a4, 0xffd0, 0x890a], 0x44b0_0867, 0x44b0_0867, -1, -1),
                ([0x4ee9, 0xffd5, 0x8758], 0x44b0_0867, 0x44b0_0867, -1, -1),
                ([0x48cc, 0xffd0, 0x86e5], 0x44b0_0867, 0x44b0_0867, -1, -1),
                ([0x3901, 0x110, 0x8cf5], 0x44b0_002f, 0x44b0_002f, -1, -1),
                ([0x5f3f, 0xffd9, 0x94fe], 0x44b0_002f, 0x4090_002f, -1, 1000),
            ],
        ),
        (
            [0x4e22, 0xfff3, 0x86b2],
            0,
            0x005f_5e10,
            &[
                ([0x3938, 0x110, 0x9259], 0x44b0_002f, 0x44b0_002f, -1, -1),
                ([0x871f, 0x230, 0xaa56], 0x44b0_002f, 0x44b0_002f, -1, -1),
                ([0x4b55, 0xffd0, 0x87f8], 0x46b0_0867, 0x4290_0867, -1, 1000),
                ([0x49a4, 0xffd0, 0x890a], 0x46b0_0867, 0x4290_0867, -1, 1000),
                ([0x4ee9, 0xffd5, 0x8758], 0x46b0_0867, 0x4290_0867, -1, 1000),
                ([0x48cc, 0xffd0, 0x86e5], 0x44b0_0867, 0x4090_0867, -1, 1000),
                ([0x3901, 0x110, 0x8cf5], 0x44b0_002f, 0x44b0_002f, -1, -1),
            ],
        ),
    ];
    for (index, (position, radius, wake, sleepers)) in firings.into_iter().enumerate() {
        let mut arena = arena();
        let actors: Vec<ObjectId> = sleepers
            .iter()
            .map(|&(at, flags, _, delay, _)| {
                let actor = actor(&mut arena, at);
                arena.owned_mut(actor).unwrap().state.base.flags = flags;
                arena.derived_actor_mut(actor).unwrap().word_144 = delay;
                actor
            })
            .collect();
        let object = trigger(&mut arena, position);
        match &mut arena.owned_mut(object).unwrap().state.class {
            ObjectClass::Selector42(state) => {
                state.radius_squared_44 = radius;
                state.wake_squared_48 = wake;
                state.on_draw_4c = u8::from(index == 1);
                state.disabled_4d = u8::from(index == 1);
            }
            _ => unreachable!(),
        }
        if index == 0 {
            arena.fire_selector42(object).unwrap();
        } else {
            arena.draw_selector42(object, 4).unwrap();
        }
        assert!(disposed(&arena, object));
        for (actor, &(_, _, flags, _, delay)) in actors.iter().zip(sleepers) {
            assert_eq!(arena.object(*actor).unwrap().base.flags, flags, "{index}");
            assert_eq!(
                arena.derived_actor(*actor).unwrap().word_144,
                delay,
                "{index}"
            );
        }
    }
}
