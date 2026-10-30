//! Selector 78, the Alien Brain's ending director: case 0x4E of 004603B0,
//! 0x174 bytes, 00406640 over the shared actor. Its table 0051FA9C is the
//! shared actor's 00527C00 but for +00 (00406730), +04 (the commands
//! 00406800), +24 (the update 004072A0), +44 (00407340, which runs +24; the
//! player's update calls it once the player has taken the director) and +88
//! (00444AB0, which lets a player take it). Its +138 is a creation listener
//! (0051FA98, 00407350) on 0054E878, and command 16 keeps the address of a
//! list of three one-based route-node numbers at +154.
//!
//! The update runs the ending 00406830 as timed states (+160), each with a
//! clock +158 in milliseconds that entering sets to 1000; a state that runs
//! its clock out is left and the next entered. It then ticks its two
//! cinematic helpers (selector 90), +168 then +164. The phase 00546910
//! steps the director and the brain (selector 85) through each other:
//!
//! - 1 waits for phase 1 (the player's lock on the brain). Leaving it, the
//!   director builds the helpers, the first player takes it, their unit
//!   heals by 1000, helper +168 starts at the unit and flies route 2, helper
//!   +164 starts at route 0, flies it and looks at +168, and the camera
//!   follows +164.
//! - 2 speeds the helpers up; once +164 has arrived the phase becomes 2 (the
//!   brain's attack); the state ends when the brain makes it 3.
//! - 3 kills the aliens' remaining units one by one in view, five sparks
//!   round each, retiring each after 500 ms; those whose +58's +CC answers
//!   (the creature families) are disposed unseen.
//! - 4 has the camera follow +164 looking at +168 and sets phase 4 (the
//!   brain opens); the state ends when the brain makes it 5.
//! - 5 sends +168 (the bug, now drawn) along route 1, +164 two nodes ahead,
//!   each looking at the other's end; near the route's end +164 and the
//!   camera wait at its last node. The state ends 600 ms after +168 arrives,
//!   or after the listener sees a gate's pass announce it (0048C2E9).
//! - 6 ends the world 750 ms in (00487560(1, 1, 0)) and sets the completion
//!   bonus.

use thiserror::Error;

use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorConstructorNull, ActorExtension, ArenaIntroInputs, ArenaIntroServices, DerivedActorClass,
    InstanceCommandAllocation, InstanceCommandAllocationError, RetainedInstanceCommand,
};
use crate::{
    name_entry::name_entry_rng_next,
    object_runtime::{
        mark_flag_change, ActorHandle, CreationListenerInterface, CreationListeners,
        FollowTargetRequest, NativeTargetError, ObjectArena, ObjectError, ObjectId, PlayerId,
        Selector90Byte, Selector90Error, Selector90TickRequest, Selector90Word,
        WeightedTargetConstructorNull, WorldLoadError,
    },
};
use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};
use std::sync::Arc;

/// 00406859: a state's clock as entering sets it.
const STATE_CLOCK: i32 = 1000;
/// 00406E41: state 3's victim clock as entering sets it.
const VICTIM_CLOCK: i32 = 500;
/// 00406D26: the victim clock from which the next victim is sought.
const NEXT_VICTIM: i32 = 1200;
/// 00406E32: how long the camera stays on a victim.
const VICTIM_VIEW: i32 = 2000;
/// 00406D51..00406D62: the victims' flags, team and lifecycle.
const VICTIM_FLAGS: u32 = 0x24;
const ALIEN_TEAM: u32 = 0x12;
/// 00406B70: the sparks round each victim.
const VICTIM_SPARKS: usize = 5;
/// 00406F48: state 5's clock while the bug has not arrived.
const BUG_WAIT: i32 = 600;
/// 00407216: state 6's clock as entering sets it; the world ends once it
/// falls below 1000, 750 ms in.
const END_CLOCK: i32 = 0x6d6;
/// 0040724B: the completion bonus, settings +84.
pub const COMPLETION_BONUS: u32 = 0x40;
/// 00406934: what the take heals the player's unit by.
const HEAL: u16 = 1000;
/// 004070F5: the flag state 5 gives the bug.
const BUG_FLAG: u32 = 0x800;

/// 00406640's fields after the shared 0047A7A0; +13C..+150 hold the
/// listener's binding list, which [`CreationListeners`] owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector78State {
    /// +154: command 16's operand address, three one-based route-node
    /// numbers (00404F00 finds node n - 1); null until a command writes it.
    pub routes_154: Option<RetainedInstanceCommand>,
    /// +158: the state's clock, in milliseconds.
    pub clock_158: i32,
    /// +15C: state 3's victim clock, which entering state 3 writes before
    /// any read; the constructor leaves it unwritten.
    pub victim_clock_15c: Option<i32>,
    /// +160: the ending's state.
    pub state_160: u32,
    /// +164 and +168: the two cinematic helpers (selector 90).
    pub helper_164: Option<ObjectId>,
    pub helper_168: Option<ObjectId>,
    /// +16C: state 3's victim, the process null at construction.
    pub victim_16c: ActorHandle,
}

/// What the ending reaches beyond the intro's helper, camera, retirement
/// and world-end services.
pub trait Selector78Host: ArenaIntroServices {
    /// 0048F060's seed, 0055F18C.
    fn rng(&mut self) -> &mut u32;
    /// 0046AE90: `player` takes the actor whose handle this is.
    fn take_player(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        handle: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// 004665D0(1, point, velocity, owner, 0), a spark.
    fn spark(
        &mut self,
        arena: &mut ObjectArena,
        point: [u16; 3],
        velocity: [u16; 3],
        owner: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// 0040724B: settings +84 |= [`COMPLETION_BONUS`]. The world's +5C6,
    /// set beside it, only has 004875E3 repeat the same OR every frame,
    /// which nothing clears.
    fn completion_bonus(&mut self);
}

#[derive(Debug, Error)]
pub enum Selector78Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Target(#[from] NativeTargetError),
    #[error(transparent)]
    Helper(#[from] Selector90Error),
    #[error(transparent)]
    Routes(#[from] WorldLoadError),
    #[error(transparent)]
    Player(#[from] crate::object_runtime::PlayerAttachError),
    /// 00406898: no command 16 wrote the route list +154.
    #[error("director {0:?} reads its route list +154, which no command wrote")]
    NoRoutes(ObjectId),
    /// A state reads a helper +164/+168 that state 1 never built.
    #[error("director {director:?} reaches a helper it never built at {address:08X}")]
    NoHelper { director: ObjectId, address: u32 },
    /// 004070A8: route 1 names no node, which 00404C50 would read.
    #[error("director {director:?} walks from a missing route node at {address:08X}")]
    NoNode { director: ObjectId, address: u32 },
    /// 00406B5F: state 3's victim clock, which only entering it writes.
    #[error("director {0:?} reads its victim clock +15C before state 3 wrote it")]
    NoVictimClock(ObjectId),
    /// 0040693D: the heal's cap, the unit's +A2, is unwritten.
    #[error("director's heal of {0:?} reads its unwritten maximum health +A2")]
    NoMaximumHealth(ObjectId),
    /// 00478E00 built no camera, which the next call dereferences.
    #[error("director {director:?} has no camera at {address:08X}")]
    NoCamera { director: ObjectId, address: u32 },
    #[error("director {director:?} service at {address:08X}: {error}")]
    Host {
        director: ObjectId,
        address: u32,
        error: E,
    },
}

type Result78<T, E> = Result<T, Selector78Error<E>>;

/// The mode 00406830 runs a state in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Enter,
    Leave,
    Tick(i32),
}

impl ObjectArena {
    /// Case 0x4E: 0x174 bytes, the shared 0047A7A0, then 00406640: the
    /// fields zeroed, +16C the process null, and the +138 binding at the
    /// tail of 0054E878.
    pub fn allocate_selector78(
        &mut self,
        request: ActorConstructionRequest<'_>,
        listeners: &mut CreationListeners,
    ) -> Result<ObjectId, ActorConstructionError> {
        let constructor_null = request.constructor_null;
        let object = self.allocate_actor(request)?;
        // 00406692 reads 0054E894, the value 0047A7A0 already used.
        let null = match constructor_null {
            ActorConstructorNull::Process(process) => process
                .process_null_handle
                .ok_or(ActorConstructionError::MissingProcessNull)?,
            ActorConstructorNull::RegistrySnapshotApproximation => self
                .actor_handles()
                .ok_or(ActorConstructionError::MissingRegistry)?
                .null_handle(),
        };
        self.actor_mut(object)?.extension = ActorExtension::Selector78(Box::new(Selector78State {
            routes_154: None,
            clock_158: 0,
            victim_clock_15c: None,
            state_160: 0,
            helper_164: None,
            helper_168: None,
            victim_16c: null,
        }));
        listeners.register(object, CreationListenerInterface::Selector78);
        Ok(object)
    }

    pub fn selector78(&self, object: ObjectId) -> Result<&Selector78State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector78(state) => Ok(state),
            _ => Err(ObjectError::NotSelector78(object)),
        }
    }

    fn selector78_mut(&mut self, object: ObjectId) -> Result<&mut Selector78State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector78(state) => Ok(state),
            _ => Err(ObjectError::NotSelector78(object)),
        }
    }

    /// 00406800, the director's +04: codes through 15 are the shared
    /// actor's (0047ADA0), 16 keeps its operand's address at +154, and any
    /// other refuses.
    pub(in crate::object_runtime) fn apply_selector78_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
        allocation: Option<&Arc<InstanceCommandAllocation>>,
    ) -> Result<bool, ActorCommandError> {
        self.selector78(object)?;
        if command.target != InstanceCommandTarget::Constructor(78) {
            return Err(ActorCommandError::WrongTarget);
        }
        if command.code.value <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        if command.code.value != 16 {
            return Ok(false);
        }
        let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } = command.payload
        else {
            return Err(ActorCommandError::Payload(16));
        };
        let allocation = allocation.ok_or(InstanceCommandAllocationError::MissingOwner)?;
        self.selector78_mut(object)?.routes_154 = Some(RetainedInstanceCommand::new(
            Arc::clone(allocation),
            allocation_offset,
        ));
        Ok(true)
    }

    /// 00407350, the director's creation listener: from state 5, an actor
    /// announced with helper +168's handle (a gate's pass announces the
    /// actor that crossed it, 0048C2E9) marks both helpers arrived and
    /// clears +164's look-at target (00462D40(0)).
    pub fn selector78_listener(
        &mut self,
        director: ObjectId,
        actor: ObjectId,
    ) -> Result<(), ObjectError> {
        let state = self.selector78(director)?;
        let (Some(bug), other) = (state.helper_168, state.helper_164) else {
            return Ok(());
        };
        if (state.state_160 as i32) < 5 || self.actor(bug)?.handle != self.actor(actor)?.handle {
            return Ok(());
        }
        let other = other.ok_or(ObjectError::Selector78Helper(director))?;
        self.write_selector90_byte(bug, Selector90Byte::Arrival140, 1)?;
        self.write_selector90_byte(other, Selector90Byte::Arrival140, 1)?;
        self.replace_selector90_target(other, None)
    }

    /// 004072A0, the director's +24 (and, through 00407340, its +44): the
    /// delta in milliseconds runs the states, then the helpers tick.
    pub fn tick_selector78<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        delta: u32,
        inputs: ArenaIntroInputs,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        let mut ms = (delta >> 10) as i32;
        let mut clock = self.selector78(director)?.clock_158;
        while clock <= ms {
            if clock > 0 {
                ms -= clock;
                self.selector78_state(director, Mode::Tick(clock), inputs, host)?;
            }
            self.selector78_state(director, Mode::Leave, inputs, host)?;
            let state = self.selector78_mut(director)?;
            state.state_160 = state.state_160.wrapping_add(1);
            self.selector78_state(director, Mode::Enter, inputs, host)?;
            clock = self.selector78(director)?.clock_158;
        }
        if ms != 0 && self.selector78(director)?.clock_158 > 0 {
            self.selector78_state(director, Mode::Tick(ms), inputs, host)?;
            let state = self.selector78_mut(director)?;
            state.clock_158 = state.clock_158.wrapping_sub(ms);
        }
        let request = Selector90TickRequest {
            microseconds: delta,
            motion_mode: inputs.motion_mode,
            settings_7c: inputs.settings_7c,
            up: inputs.up,
        };
        let state = self.selector78(director)?;
        let helpers = [state.helper_168, state.helper_164];
        for helper in helpers.into_iter().flatten() {
            self.tick_selector90(helper, request)?;
        }
        Ok(())
    }

    /// 00406830(mode, ms), one state's entry, exit or tick.
    fn selector78_state<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        mode: Mode,
        inputs: ArenaIntroInputs,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        if mode == Mode::Enter {
            self.selector78_mut(director)?.clock_158 = STATE_CLOCK;
        }
        let phase = self.alien_brain.phase_546910;
        // 00406881 and its kind: hold the clock until the phase is reached.
        let wait_for = |phase_wanted: u32| {
            if phase != phase_wanted {
                STATE_CLOCK
            } else {
                0
            }
        };
        match (self.selector78(director)?.state_160, mode) {
            (1, Mode::Leave) => self.selector78_begin(director, inputs, host)?,
            (1, Mode::Tick(_)) => self.selector78_mut(director)?.clock_158 = wait_for(1),
            (2, Mode::Tick(ms)) => {
                self.selector78_mut(director)?.clock_158 = STATE_CLOCK;
                let guide = self.selector78_helper(director, Helper::Guide, 0x0040_6aa8)?;
                let bug = self.selector78_helper(director, Helper::Bug, 0x0040_6acf)?;
                self.speed_helper(guide, ms / 2, 1000)?;
                self.speed_helper(bug, ms / 4, 1000)?;
                if self.selector90(guide)?.byte_140 != 0 && phase == 1 {
                    self.alien_brain.phase_546910 = 2;
                    self.selector78_mut(director)?.clock_158 = STATE_CLOCK;
                } else {
                    self.selector78_mut(director)?.clock_158 = wait_for(3);
                }
            }
            (3, Mode::Enter) => {
                self.selector78_mut(director)?.victim_clock_15c = Some(VICTIM_CLOCK)
            }
            (3, Mode::Tick(ms)) => self.selector78_victims(director, ms, inputs, host)?,
            (4, Mode::Enter) => {
                let camera = self.selector78_camera(director, 0x0040_6e80, host)?;
                let guide = self.selector78_helper(director, Helper::Guide, 0x0040_6ea1)?;
                let bug = self.selector78_helper(director, Helper::Bug, 0x0040_6ee5)?;
                self.selector78_follow(camera, guide, inputs)?;
                let target = self.follow_actor_target(bug)?;
                self.replace_camera_secondary_target(camera, target)?;
                self.selector16_mut(camera)?.byte_184 = 1;
                self.set_camera_parameter_mode(camera, 1)?;
                self.alien_brain.phase_546910 = 4;
            }
            (4, Mode::Tick(_)) => self.selector78_mut(director)?.clock_158 = wait_for(5),
            (5, Mode::Enter) => self.selector78_flight(director, host)?,
            (5, Mode::Tick(ms)) => self.selector78_flight_tick(director, ms, host)?,
            (6, Mode::Enter) => self.selector78_mut(director)?.clock_158 = END_CLOCK,
            (6, Mode::Tick(_)) => {
                if self.selector78(director)?.clock_158 < STATE_CLOCK
                    && host
                        .world_present()
                        .map_err(host_error(director, 0x0040_7235))?
                {
                    host.complete_mission(self, 1, 1, 0)
                        .map_err(host_error(director, 0x0040_7241))?;
                    host.completion_bonus();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// 00406898: leaving state 1, the director builds the helpers and the
    /// first player takes it; with any piece missing it disposes itself.
    fn selector78_begin<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        inputs: ArenaIntroInputs,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        let start = self.selector78_route(director, 0)?;
        let bug_route = self.selector78_route(director, 8)?;
        let player = self.first_player();
        let camera = host
            .camera(self)
            .map_err(host_error(director, 0x0040_68ca))?;
        let guide = host
            .create_helper(self)
            .map_err(host_error(director, 0x0040_68d3))?;
        let bug = host
            .create_helper(self)
            .map_err(host_error(director, 0x0040_68de))?;
        let state = self.selector78_mut(director)?;
        state.helper_164 = guide;
        state.helper_168 = bug;
        let (Some(player), Some(guide), Some(bug), Some(start), Some(bug_route)) =
            (player, guide, bug, start, bug_route)
        else {
            return Ok(self.dispose(director)?);
        };
        let unit = self.first_player_primary_actor()?;
        let handle = self.actor(director)?.handle;
        host.take_player(self, player, handle)
            .map_err(host_error(director, 0x0040_6927))?;
        let Some(unit) = unit else {
            return Ok(());
        };
        let actor = self.actor_mut(unit)?;
        actor.health = actor.health.wrapping_add(HEAL);
        let maximum = actor
            .maximum_health
            .ok_or(Selector78Error::NoMaximumHealth(unit))?;
        if actor.health > maximum {
            actor.health = maximum;
        }
        let at = self.selector78_position(unit)?;
        self.copy_actor_position(bug, at)?;
        self.selector78_route_helper(bug, Some(bug_route), host)?;
        self.write_selector90_word(bug, Selector90Word::Speed13c, 0)?;
        self.write_selector90_byte(bug, Selector90Byte::Arrival140, 0)?;
        self.write_selector90_word(bug, Selector90Word::Approach138, 0x4b)?;
        let at = self.selector78_position(start)?;
        self.copy_actor_position(guide, at)?;
        self.selector78_route_helper(guide, Some(start), host)?;
        self.write_selector90_word(guide, Selector90Word::Speed13c, 0x14)?;
        self.write_selector90_word(guide, Selector90Word::Approach138, 0x96)?;
        self.write_selector90_byte(guide, Selector90Byte::Arrival140, 0)?;
        self.write_selector90_byte(guide, Selector90Byte::Mode142, 0)?;
        let target = self.follow_actor_target(bug)?;
        self.replace_selector90_target(guide, target)?;
        let camera = camera.ok_or(Selector78Error::NoCamera {
            director,
            address: 0x0040_6a76,
        })?;
        self.selector78_follow(camera, guide, inputs)?;
        self.selector16_mut(camera)?.byte_184 = 1;
        self.set_camera_parameter_mode(camera, 0)?;
        Ok(())
    }

    /// 00406B49: state 3's tick. The victim gets five sparks on the first
    /// tick and is retired once alive past 500 ms; once it is gone, from
    /// 1200 ms the next of the aliens' units in the actor list is sought.
    fn selector78_victims<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        ms: i32,
        inputs: ArenaIntroInputs,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        let handle = self.selector78(director)?.victim_16c;
        let victim = self
            .resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let clock = self
            .selector78(director)?
            .victim_clock_15c
            .ok_or(Selector78Error::NoVictimClock(director))?;
        if let (Some(victim), 0) = (victim, clock) {
            for _ in 0..VICTIM_SPARKS {
                self.selector78_spark(director, victim, host)?;
            }
        }
        let clock = clock.wrapping_add(ms);
        self.selector78_mut(director)?.victim_clock_15c = Some(clock);
        if let Some(victim) = victim {
            if clock > VICTIM_CLOCK && self.actor(victim)?.word_c4 == 3 {
                host.retire_actor(self, victim)
                    .map_err(host_error(director, 0x0040_6d1c))?;
            }
            return Ok(());
        }
        if clock < NEXT_VICTIM {
            return Ok(());
        }
        let own = self.actor(director)?.handle;
        for actor in self.actor_list().to_vec() {
            let object = self.object(actor)?;
            let (flags, placement) = (object.base.flags, object.base.placement_word as i16);
            let state = self.actor(actor)?;
            if flags & VICTIM_FLAGS != VICTIM_FLAGS
                || state.word_b4 != ALIEN_TEAM
                || state.word_c4 != 3
                || placement < 0
                || state.handle == own
            {
                continue;
            }
            let Some(view) = self.actor_derived_component(actor)? else {
                continue;
            };
            // 00406D98: the view's +CC answers (00404610) in the creature
            // families alone; every other table returns null (00401300).
            if matches!(
                self.derived_actor(view)?.class,
                DerivedActorClass::Family428650(_)
            ) {
                self.dispose(actor)?;
                continue;
            }
            let camera = self.selector78_camera(director, 0x0040_6dc7, host)?;
            let handle = self.actor(actor)?.handle;
            let state = self.selector78_mut(director)?;
            state.victim_16c = handle;
            state.victim_clock_15c = Some(0);
            self.selector78_follow(camera, actor, inputs)?;
            self.selector16_mut(camera)?.byte_184 = 0;
            self.set_camera_parameter_mode(camera, 0)?;
            self.selector78_mut(director)?.clock_158 = VICTIM_VIEW;
            return Ok(());
        }
        Ok(())
    }

    /// 00406B78: a spark within 50 of the victim on X and Z. An odd first
    /// draw throws it at three draws of -600..599; an even one at -500..499,
    /// 600..799 and -500..499, the point moving by the X and Z over 16.
    /// Both move it up by the Y over 16. Its owner is the victim.
    fn selector78_spark<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        victim: ObjectId,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        let rng = host.rng();
        let first = name_entry_rng_next(rng) & 0xffff;
        let mut point = self.selector78_position(victim)?;
        let mut offset = || (((name_entry_rng_next(rng) & 0xffff) * 50) as i32 >> 15) - 50;
        point[0] = point[0].wrapping_add(offset() as u16);
        point[2] = point[2].wrapping_add(offset() as u16);
        let mut draw = |modulus: u32| ((name_entry_rng_next(rng) & 0xffff) % modulus) as i16;
        let velocity = if first & 1 != 0 {
            [draw(1200) - 600, draw(1200) - 600, draw(1200) - 600]
        } else {
            let velocity = [draw(1000) - 500, draw(200) + 600, draw(1000) - 500];
            point[0] = point[0].wrapping_add((velocity[0] / 16) as u16);
            point[2] = point[2].wrapping_add((velocity[2] / 16) as u16);
            velocity
        };
        point[1] = point[1].wrapping_add((velocity[1] / 16) as u16);
        let owner = self.actor(victim)?.handle;
        host.spark(self, point, velocity.map(|v| v as u16), owner)
            .map_err(host_error(director, 0x0040_6cd6))
    }

    /// 0040709F: entering state 5, the bug (+168) is drawn and flies route
    /// 1 to its end, looking at the last node; +164 flies from two nodes on,
    /// looking at the bug.
    fn selector78_flight<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        let route = self.selector78_route(director, 4)?;
        let bug = self.selector78_helper(director, Helper::Bug, 0x0040_70ad)?;
        self.selector78_route_helper(bug, route, host)?;
        self.write_selector90_word(bug, Selector90Word::Speed13c, 0x32)?;
        self.write_selector90_byte(bug, Selector90Byte::Arrival140, 0)?;
        self.set_flag0(bug, true)?;
        let base = &mut self.owned_mut(bug)?.state.base;
        let flags = base.flags | BUG_FLAG;
        mark_flag_change(base, flags);
        let missing = Selector78Error::NoNode {
            director,
            address: 0x0040_710a,
        };
        let mut node = route.ok_or(missing)?;
        for _ in 0..2 {
            if let Some(next) = self.route_next(node)? {
                node = next;
            }
        }
        let guide = self.selector78_helper(director, Helper::Guide, 0x0040_711a)?;
        self.selector78_route_helper(guide, Some(node), host)?;
        self.write_selector90_word(guide, Selector90Word::Speed13c, 0)?;
        self.write_selector90_word(guide, Selector90Word::Approach138, 0x32)?;
        self.write_selector90_byte(guide, Selector90Byte::Arrival140, 0)?;
        self.write_selector90_byte(guide, Selector90Byte::Mode142, 0)?;
        let target = self.follow_actor_target(bug)?;
        self.replace_selector90_target(guide, target)?;
        while let Some(next) = self.route_next(node)? {
            node = next;
        }
        let end = self.object_position_target_resource(
            node,
            WeightedTargetConstructorNull::Process(&*host),
        )?;
        self.replace_selector90_target(bug, end)?;
        Ok(())
    }

    /// 00406F38: state 5's tick. The bug speeds up to 1500, and +164 to
    /// 1600 once the bug passes 600 (else it stops); while the bug has not
    /// arrived the clock holds at 600. With one or two nodes left after the
    /// bug's, +164 waits at the last and the camera looks at the one
    /// before it.
    fn selector78_flight_tick<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        ms: i32,
        host: &mut H,
    ) -> Result78<(), H::Error> {
        let bug = self.selector78_helper(director, Helper::Bug, 0x0040_6f38)?;
        if self.selector90(bug)?.byte_140 == 0 {
            self.selector78_mut(director)?.clock_158 = BUG_WAIT;
        }
        let speed = self.speed_helper(bug, ms / 2, 1500)?;
        // 00406F84: 00462D80, the node the bug's route target holds.
        let current = match self.selector90(bug)?.route_150 {
            Some(target) => Some(self.object_position_target_data(target)?.object_14),
            None => None,
        };
        if let Some(current) = current {
            let mut nodes = vec![current];
            while nodes.len() < 5 {
                let next = self.route_next(*nodes.last().unwrap())?;
                match next {
                    Some(next) => nodes.push(next),
                    None => break,
                }
            }
            // 00406FBE: the native count is one past the last node read,
            // the missing successor included.
            let count = if nodes.len() < 5 { nodes.len() + 1 } else { 5 };
            if count > 2 && count < 5 {
                let last = nodes[count - 2];
                let guide = self.selector78_helper(director, Helper::Guide, 0x0040_6fd3)?;
                let at = self.selector78_position(last)?;
                self.copy_actor_position(guide, at)?;
                self.write_selector90_byte(guide, Selector90Byte::Arrival140, 1)?;
                let camera = self.selector78_camera(director, 0x0040_7000, host)?;
                let look = self.object_position_target_resource(
                    nodes[count - 3],
                    WeightedTargetConstructorNull::Process(&*host),
                )?;
                self.replace_camera_secondary_target(camera, look)?;
            }
        }
        let guide = self.selector78_helper(director, Helper::Guide, 0x0040_705c)?;
        if speed > 600 {
            self.speed_helper(guide, ms / 4, 1600)?;
        } else {
            self.write_selector90_word(guide, Selector90Word::Speed13c, 0)?;
        }
        Ok(())
    }

    /// A helper's speed +13C gains `gain`, at most `cap`; the new speed.
    fn speed_helper(&mut self, helper: ObjectId, gain: i32, cap: i32) -> Result<i32, ObjectError> {
        let speed = self
            .selector90(helper)?
            .word_13c
            .wrapping_add(gain)
            .min(cap);
        self.write_selector90_word(helper, Selector90Word::Speed13c, speed)?;
        Ok(speed)
    }

    /// 00404F00 of the route number at +154 + `offset`.
    fn selector78_route<E>(
        &self,
        director: ObjectId,
        offset: u32,
    ) -> Result78<Option<ObjectId>, E> {
        let routes = self
            .selector78(director)?
            .routes_154
            .as_ref()
            .ok_or(Selector78Error::NoRoutes(director))?;
        let at = routes
            .allocation_offset
            .checked_add(offset)
            .ok_or(WorldLoadError::OffsetOverflow)?;
        let number = routes
            .allocation()
            .runtime_allocation()
            .dword(at)
            .map_err(WorldLoadError::from)?;
        Ok(self.find_route_node(number))
    }

    fn selector78_helper<E>(
        &self,
        director: ObjectId,
        helper: Helper,
        address: u32,
    ) -> Result78<ObjectId, E> {
        let state = self.selector78(director)?;
        match helper {
            Helper::Guide => state.helper_164,
            Helper::Bug => state.helper_168,
        }
        .ok_or(Selector78Error::NoHelper { director, address })
    }

    /// 00478E00(G+8), which the next call dereferences.
    fn selector78_camera<H: Selector78Host>(
        &mut self,
        director: ObjectId,
        address: u32,
        host: &mut H,
    ) -> Result78<ObjectId, H::Error> {
        host.camera(self)
            .map_err(host_error(director, address))?
            .ok_or(Selector78Error::NoCamera { director, address })
    }

    /// 00462CC0: the helper's route target, a node's position.
    fn selector78_route_helper<H: Selector78Host>(
        &mut self,
        helper: ObjectId,
        node: Option<ObjectId>,
        host: &mut H,
    ) -> Result<(), NativeTargetError> {
        self.replace_selector90_route(helper, node, WeightedTargetConstructorNull::Process(&*host))
    }

    /// 004825D0(handle, 0), a target following the actor.
    fn follow_actor_target(
        &mut self,
        actor: ObjectId,
    ) -> Result<Option<crate::object_runtime::NativeHeapRef>, ObjectError> {
        let handle = self.actor(actor)?.handle;
        self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))
    }

    /// 00410E70 with a target following `actor`.
    fn selector78_follow(
        &mut self,
        camera: ObjectId,
        actor: ObjectId,
        inputs: ArenaIntroInputs,
    ) -> Result<(), ObjectError> {
        let target = self.follow_actor_target(actor)?;
        self.replace_camera_target(camera, target, inputs.camera_settings)
    }

    fn selector78_position(&self, object: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(object)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(object))
    }
}

#[derive(Clone, Copy)]
enum Helper {
    /// +164, which the camera rides.
    Guide,
    /// +168, the brain's bug.
    Bug,
}

fn host_error<E>(director: ObjectId, address: u32) -> impl FnOnce(E) -> Selector78Error<E> {
    move |error| Selector78Error::Host {
        director,
        address,
        error,
    }
}

#[cfg(test)]
#[path = "selector78_tests.rs"]
mod tests;
