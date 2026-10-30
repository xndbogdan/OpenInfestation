use std::collections::BTreeMap;

use super::*;

#[derive(Clone, Copy, Debug, Default)]
struct Actor {
    position: [u16; 3],
    helper138: i32,
    helper13c: i32,
    delay144: i32,
    helper140: u8,
    helper142: u8,
    camera184: u8,
    camera187: u8,
    state_c4: i32,
    derived: Option<u32>,
    active: bool,
}

#[derive(Default)]
struct Host {
    actors: BTreeMap<u32, Actor>,
    player: Option<u32>,
    camera: Option<u32>,
    routes: [Option<u32>; 4],
    lists: [Vec<u32>; 4],
    helpers: Vec<Option<u32>>,
    world: bool,
    effects: Vec<IntroEffect<u32>>,
    fail_refresh: bool,
    shorten_list_on_retirement: bool,
}

fn list_index(list: IntroActorList) -> usize {
    match list {
        IntroActorList::Primary14 => 0,
        IntroActorList::Sequential1c => 1,
        IntroActorList::Secondary24 => 2,
        IntroActorList::Exit2c => 3,
    }
}

impl IntroHost for Host {
    type Actor = u32;
    type Error = &'static str;

    fn first_player_actor(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.player)
    }
    fn route(&mut self, route: IntroRoute) -> Result<Option<u32>, Self::Error> {
        Ok(self.routes[match route {
            IntroRoute::Primary00 => 0,
            IntroRoute::Secondary04 => 1,
            IntroRoute::Advance08 => 2,
            IntroRoute::Exit0c => 3,
        }])
    }
    fn route_successor(&mut self, route: u32) -> Result<Option<u32>, Self::Error> {
        Ok(Some(route + 1))
    }
    fn create_helper(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.helpers.remove(0))
    }
    fn camera(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.camera)
    }
    fn actor_handle(&mut self, actor: u32) -> Result<u32, Self::Error> {
        Ok(actor)
    }
    fn reference_count(&mut self, list: IntroActorList) -> Result<i32, Self::Error> {
        Ok(self.lists[list_index(list)].len() as i32)
    }
    fn reference_handle(&mut self, list: IntroActorList, index: i32) -> Result<u32, Self::Error> {
        self.lists[list_index(list)]
            .get(index as usize)
            .copied()
            .ok_or("missing row")
    }
    fn main_reference_handle(&mut self) -> Result<u32, Self::Error> {
        Ok(100)
    }
    fn resolve_actor(&mut self, handle: u32) -> Result<Option<u32>, Self::Error> {
        Ok(self.actors.contains_key(&handle).then_some(handle))
    }
    fn derived_actor(&mut self, actor: u32) -> Result<Option<u32>, Self::Error> {
        Ok(self.actors[&actor].derived)
    }
    fn active_component(&mut self, derived: u32) -> Result<bool, Self::Error> {
        Ok(self.actors[&derived].active)
    }
    fn actor_state_c4(&mut self, actor: u32) -> Result<i32, Self::Error> {
        Ok(self.actors[&actor].state_c4)
    }
    fn position_words(&mut self, actor: u32) -> Result<[u16; 3], Self::Error> {
        Ok(self.actors[&actor].position)
    }
    fn word(&mut self, actor: u32, field: IntroWord) -> Result<i32, Self::Error> {
        let actor = self.actors[&actor];
        Ok(match field {
            IntroWord::Helper138 => actor.helper138,
            IntroWord::Helper13c => actor.helper13c,
            IntroWord::DerivedDelay144 => actor.delay144,
        })
    }
    fn byte(&mut self, actor: u32, field: IntroByte) -> Result<u8, Self::Error> {
        let actor = self.actors[&actor];
        Ok(match field {
            IntroByte::Helper140 => actor.helper140,
            IntroByte::Helper142 => actor.helper142,
            IntroByte::Camera184 => actor.camera184,
            IntroByte::Camera187 => actor.camera187,
        })
    }
    fn world_present(&mut self) -> Result<bool, Self::Error> {
        Ok(self.world)
    }
    fn effect(&mut self, effect: IntroEffect<u32>) -> Result<(), Self::Error> {
        self.effects.push(effect);
        match effect {
            IntroEffect::Word {
                actor,
                field,
                value,
            } => {
                let actor = self.actors.get_mut(&actor).unwrap();
                *match field {
                    IntroWord::Helper138 => &mut actor.helper138,
                    IntroWord::Helper13c => &mut actor.helper13c,
                    IntroWord::DerivedDelay144 => &mut actor.delay144,
                } = value;
            }
            IntroEffect::Byte {
                actor,
                field,
                value,
            } => {
                let actor = self.actors.get_mut(&actor).unwrap();
                *match field {
                    IntroByte::Helper140 => &mut actor.helper140,
                    IntroByte::Helper142 => &mut actor.helper142,
                    IntroByte::Camera184 => &mut actor.camera184,
                    IntroByte::Camera187 => &mut actor.camera187,
                } = value;
            }
            IntroEffect::PositionFrom { actor, source } => {
                let position = self.actors[&source].position;
                self.actors.get_mut(&actor).unwrap().position = position;
            }
            IntroEffect::RetireSequential { .. } if self.shorten_list_on_retirement => {
                self.lists[1].truncate(1)
            }
            IntroEffect::RefreshVisual { .. } if self.fail_refresh => {
                return Err("refresh unavailable")
            }
            _ => {}
        }
        Ok(())
    }
}

fn fixture(stage: u32) -> (IntroSequence<u32>, Host) {
    let mut state = IntroSequence::new(
        1,
        IntroAllocationWords {
            elapsed_138: 0,
            deadline_140: 0,
            sequential_timer_150: None,
        },
    );
    state.stage_148 = stage;
    state.primary_helper_154 = Some(10);
    state.secondary_helper_158 = Some(11);
    let mut host = Host {
        player: Some(2),
        camera: Some(3),
        world: true,
        routes: [Some(20), Some(21), Some(22), Some(23)],
        helpers: vec![Some(10), Some(11)],
        ..Host::default()
    };
    for id in [1, 2, 3, 10, 11, 20, 21, 22, 23] {
        host.actors.insert(id, Actor::default());
    }
    (state, host)
}

#[test]
fn allocation_words_are_explicit_and_constructor_writes_stay_separate() {
    let state = IntroSequence::new(
        7_u32,
        IntroAllocationWords {
            elapsed_138: -1,
            deadline_140: i32::MIN,
            sequential_timer_150: Some(87),
        },
    );
    assert_eq!(
        (
            state.elapsed_138,
            state.deadline_140,
            state.sequential_timer_150
        ),
        (-1, i32::MIN, Some(87))
    );
    assert_eq!(
        (state.timer_144, state.stage_148, state.sequential_index_14c),
        (0, 0, 0)
    );
    assert_eq!(
        (state.primary_helper_154, state.secondary_helper_158),
        (None, None)
    );
}

#[test]
fn first_callback_constructs_helpers_then_enters_stage_one_in_native_order() {
    let (mut state, mut host) = fixture(0);
    state.primary_helper_154 = None;
    state.secondary_helper_158 = None;
    state.tick(&mut host, 1023).unwrap();
    assert_eq!(
        (
            state.stage_148,
            state.elapsed_138,
            state.timer_144,
            state.deadline_140
        ),
        (1, 0, 10000, 10250)
    );
    assert_eq!(
        &host.effects[..4],
        &[
            IntroEffect::Membership {
                actor: 2,
                list: IntroMembership::Flag1,
                enabled: false
            },
            IntroEffect::Membership {
                actor: 2,
                list: IntroMembership::Flag0,
                enabled: false
            },
            IntroEffect::Flags {
                actor: 2,
                and_mask: 0xffff_fedb,
                or_mask: 0
            },
            IntroEffect::PositionFrom {
                actor: 2,
                source: 1
            },
        ]
    );
    assert_eq!(
        &host.effects[host.effects.len() - 2..],
        &[
            IntroEffect::TickHelper {
                helper: 11,
                scheduler_microseconds: 1023
            },
            IntroEffect::TickHelper {
                helper: 10,
                scheduler_microseconds: 1023
            },
        ]
    );
    assert_eq!(host.actors[&10].helper138, 100);
    assert_eq!(host.actors[&11].helper140, 1);
    assert_eq!(host.actors[&3].camera184, 1);
    assert_eq!(state.sequential_timer_150, None);
}

#[test]
fn stage_eight_initializes_its_timer_before_read_but_arbitrary_resume_fails_closed() {
    let (mut state, mut host) = fixture(8);
    host.lists[1] = vec![999];
    assert_eq!(
        state.advance(&mut host, 1),
        Err(IntroError::UnwrittenSequentialTimer)
    );
    state.transition(&mut host, true).unwrap();
    state.advance(&mut host, 1).unwrap();
    assert_eq!(state.sequential_timer_150, Some(1999));
}

#[test]
fn scheduler_shift_and_elapsed_threshold_are_exact_without_fractional_carry() {
    let (mut state, mut host) = fixture(1);
    state.elapsed_138 = 498;
    state.timer_144 = 10000;
    state.deadline_140 = 20000;
    state.tick(&mut host, 3071).unwrap(); // >>10 = 2; exactly 500 does not blend.
    assert_eq!(state.elapsed_138, 500);
    assert_eq!(host.actors[&10].helper13c, 0);
    state.tick(&mut host, 3071).unwrap();
    assert_eq!(state.elapsed_138, 502);
    assert_eq!(host.actors[&10].helper13c, 1);
    assert_eq!(state.timer_144, 9996);
}

#[test]
fn inherited_stage_timers_and_deadline_repair_preserve_native_asymmetries() {
    let (mut state, mut host) = fixture(11);
    state.timer_144 = 3;
    state.deadline_140 = 100;
    state.tick(&mut host, 5 << 10).unwrap();
    assert_eq!(state.stage_148, 12);
    // Exit11 retains timer3; enter12 only increments it to4; elapsed12 then
    // rewrites it to750 because helper140 is clear, before the remaining2.
    assert_eq!(state.timer_144, 748);
    assert_eq!(state.deadline_140, 95);
    assert_eq!(host.actors[&10].helper13c, 2);
}

#[test]
fn stage_four_selects_first_resolved_reference_and_stage_six_retains_ties() {
    let (mut state, mut host) = fixture(4);
    host.lists[2] = vec![999, 40, 41];
    host.actors.insert(
        40,
        Actor {
            position: [10, 0, 0],
            derived: Some(50),
            ..Actor::default()
        },
    );
    host.actors.insert(
        41,
        Actor {
            position: [0, 10, 0],
            derived: Some(51),
            ..Actor::default()
        },
    );
    for id in [50, 51] {
        host.actors.insert(
            id,
            Actor {
                active: true,
                ..Actor::default()
            },
        );
    }
    state.transition(&mut host, true).unwrap();
    assert!(host.effects.contains(&IntroEffect::CameraPrimary {
        camera: 3,
        handle: 40
    }));
    assert!(!host.effects.contains(&IntroEffect::CameraPrimary {
        camera: 3,
        handle: 41
    }));
    host.effects.clear();
    state.stage_148 = 6;
    state.transition(&mut host, true).unwrap();
    assert_eq!(
        host.effects
            .iter()
            .filter(|effect| matches!(effect, IntroEffect::CameraPrimary { .. }))
            .copied()
            .collect::<Vec<_>>(),
        vec![IntroEffect::CameraPrimary {
            camera: 3,
            handle: 40
        }]
    );
}

#[test]
fn stage_five_and_seven_use_distinct_actor_gates() {
    let (mut state, mut host) = fixture(5);
    host.lists[2] = vec![40];
    host.actors.insert(
        40,
        Actor {
            state_c4: 3,
            derived: Some(50),
            ..Actor::default()
        },
    );
    host.actors.insert(50, Actor::default());
    state.advance(&mut host, 5).unwrap();
    assert_eq!(state.timer_144, 500);
    host.actors.get_mut(&50).unwrap().active = true;
    state.advance(&mut host, 5).unwrap();
    assert_eq!(state.timer_144, 0);
    host.actors.get_mut(&40).unwrap().derived = None;
    state.stage_148 = 7;
    state.advance(&mut host, 5).unwrap();
    assert_eq!(
        state.timer_144, 2000,
        "stage7 checks state3 without derived/activity queries"
    );
}

#[test]
fn sequential_retirement_is_strictly_negative_skips_non_state_three_and_rereads_count() {
    let (mut state, mut host) = fixture(8);
    host.lists[1] = vec![999, 40, 41];
    host.actors.insert(
        40,
        Actor {
            state_c4: 2,
            ..Actor::default()
        },
    );
    host.actors.insert(
        41,
        Actor {
            state_c4: 3,
            ..Actor::default()
        },
    );
    state.sequential_timer_150 = Some(2);
    state.advance(&mut host, 2).unwrap();
    assert_eq!(state.sequential_index_14c, 0);
    state.advance(&mut host, 1).unwrap();
    assert_eq!(
        (state.sequential_index_14c, state.sequential_timer_150),
        (3, Some(2000))
    );
    assert!(host
        .effects
        .contains(&IntroEffect::RetireSequential { actor: 41 }));
    host.lists[1] = vec![41, 41];
    host.shorten_list_on_retirement = true;
    state.sequential_index_14c = 0;
    state.sequential_timer_150 = Some(-1);
    state.advance(&mut host, 1).unwrap();
    assert_eq!(state.sequential_index_14c, 1);
}

#[test]
fn activation_preserves_signed_delay_conversion_and_flag_policy() {
    let (mut state, mut host) = fixture(2);
    host.lists[0] = vec![40, 41];
    for (actor, derived, delay) in [(40, 50, -3), (41, 51, 7)] {
        host.actors.insert(
            actor,
            Actor {
                derived: Some(derived),
                ..Actor::default()
            },
        );
        host.actors.insert(
            derived,
            Actor {
                delay144: delay,
                ..Actor::default()
            },
        );
    }
    state.transition(&mut host, true).unwrap();
    assert_eq!(host.actors[&50].delay144, 3000);
    assert_eq!(host.actors[&51].delay144, 7);
    assert!(host.effects.contains(&IntroEffect::Flags {
        actor: 50,
        and_mask: 0xfb9f_ffff,
        or_mask: 0
    }));
    assert!(!host
        .effects
        .iter()
        .any(|effect| matches!(effect, IntroEffect::Membership { enabled: true, .. })));
}

#[test]
fn completion_fires_on_entry_and_exit_and_pose_tail_follows_helper_ticks() {
    let (mut state, mut host) = fixture(13);
    state.timer_144 = 100;
    state.deadline_140 = 1000;
    state.tick(&mut host, 1 << 10).unwrap();
    assert_eq!(
        &host.effects[host.effects.len() - 3..],
        &[
            IntroEffect::TickHelper {
                helper: 11,
                scheduler_microseconds: 1024
            },
            IntroEffect::TickHelper {
                helper: 10,
                scheduler_microseconds: 1024
            },
            IntroEffect::PlayerPoseFromHelper {
                player: 2,
                helper: 10
            },
        ]
    );
    state.stage_148 = 14;
    host.effects.clear();
    state.transition(&mut host, true).unwrap();
    state.transition(&mut host, false).unwrap();
    assert_eq!(
        host.effects,
        vec![
            IntroEffect::CompleteMission {
                selection: 18,
                start_ordinal: 1,
                argument: 0
            };
            2
        ]
    );
    host.world = false;
    state.transition(&mut host, false).unwrap();
    assert_eq!(host.effects.len(), 2);
}

#[test]
fn host_failure_stops_before_later_actions_and_keeps_reached_state() {
    let (mut state, mut host) = fixture(0);
    host.fail_refresh = true;
    state.elapsed_138 = 42;
    assert_eq!(
        state.transition(&mut host, false),
        Err(IntroError::Host("refresh unavailable"))
    );
    assert_eq!(state.elapsed_138, 0);
    assert_eq!(state.primary_helper_154, Some(10));
    assert!(matches!(
        host.effects.last(),
        Some(IntroEffect::RefreshVisual { actor: 10 })
    ));
    assert!(!host
        .effects
        .iter()
        .any(|effect| matches!(effect, IntroEffect::HelperRoute { .. })));
    let (mut state, mut host) = fixture(4);
    state.secondary_helper_158 = None;
    assert_eq!(
        state.transition(&mut host, true),
        Err(IntroError::MissingHelper(IntroHelper::Secondary))
    );
}

#[test]
fn wrapped_signed_short_distance_and_overflow_branch_match_native_consumer() {
    assert_eq!(intro_distance([0; 3], [3, 4, 0]), 5);
    assert_eq!(intro_distance([0; 3], [65535, 0, 0]), 1);
    assert_eq!(intro_distance([0; 3], [32768; 3]), 56754);
    for (value, root) in [
        (0, 0),
        (1, 1),
        (3, 1),
        (4, 2),
        (9, 3),
        (i32::MAX as u32, 46340),
        (u32::MAX, 0),
    ] {
        assert_eq!(integer_sqrt(value), root);
    }
}
