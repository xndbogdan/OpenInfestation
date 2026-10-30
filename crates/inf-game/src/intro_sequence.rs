//! Selector 88's in-engine intro controller (00524AC0, +24 = 00444930).
//!
//! 00444930 owns the timer/callback order; 00443B10 owns state entry/exit;
//! 00444630 owns elapsed-state work. These functions are transcribed from
//! `bulk_clean/game_logic.c` and checked against `Infest.exe` disassembly.
//! Scheduler microseconds are shifted right ten, not divided by 1000. Timers
//! change in response to actors and route arrival, so no captured duration is
//! used. Required host operations keep selector-90 helper movement, route
//! custody, actor callbacks and native camera pose at explicit boundaries.

use thiserror::Error;

/// The four aliased count/pointer pairs retained at selector88 +13C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroActorList {
    Primary14,
    Sequential1c,
    Secondary24,
    Exit2c,
}

/// Header +00/+04/+08/+0C, passed unchanged to 00404F00.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroRoute {
    Primary00,
    Secondary04,
    Advance08,
    Exit0c,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroWord {
    Helper138,
    Helper13c,
    DerivedDelay144,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroByte {
    Helper140,
    Helper142,
    Camera184,
    Camera187,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroMembership {
    Flag0,
    Flag1,
}

/// A required native effect. No effect has a default successful implementation.
/// Target replacements include 0x20 allocation/004825D0(handle, null) and
/// replacement ownership; a failed allocation installs null as retail does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroEffect<A> {
    Membership {
        actor: A,
        list: IntroMembership,
        enabled: bool,
    },
    /// Write +2C and set dirty bit 2 iff changed bits intersect 0x1FF.
    Flags {
        actor: A,
        and_mask: u32,
        or_mask: u32,
    },
    Dirty {
        actor: A,
        bits: u16,
    },
    Resource42 {
        actor: A,
        value: u16,
    },
    /// 0047A110 after Resource42 and Dirty: visual resource refresh.
    RefreshVisual {
        actor: A,
    },
    PositionFrom {
        actor: A,
        source: A,
    },
    /// 00462CC0: destroy prior helper +150 and construct route-node target.
    HelperRoute {
        helper: A,
        route: Option<A>,
    },
    /// 00462D40 with newly allocated 004825D0 target at helper +154.
    HelperTarget {
        helper: A,
        handle: u32,
    },
    Word {
        actor: A,
        field: IntroWord,
        value: i32,
    },
    Byte {
        actor: A,
        field: IntroByte,
        value: u8,
    },
    /// 00410E70, camera primary target +18C.
    CameraPrimary {
        camera: A,
        handle: u32,
    },
    /// 0040F710, camera secondary target +190.
    CameraSecondary {
        camera: A,
        handle: u32,
    },
    /// Copy primary target with virtual +04, then install through 0040F710.
    CameraSecondaryFromPrimary {
        camera: A,
    },
    /// 00410EE0 calls 00410EC0 on both camera parameter blocks, in order.
    CameraParameterMode {
        camera: A,
        mode: u32,
    },
    DisposeController {
        actor: A,
    },
    /// Resolved state-3 actor's 0047CE90 retirement to state 1.
    RetireSequential {
        actor: A,
    },
    TickHelper {
        helper: A,
        scheduler_microseconds: u32,
    },
    /// State13 tail: helper virtual +48 first, position copy, then ten
    /// returned basis DWORDs when that pointer is nonnull (00444A21..A7F).
    PlayerPoseFromHelper {
        player: A,
        helper: A,
    },
    /// 00443B10 state14, both entry and exit, when DAT_0055F160 != null.
    CompleteMission {
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    },
}

/// Reads preserve the native query order and live aliasing. Array counts are
/// reread per row, rather than copied into a disconnected authored snapshot.
pub trait IntroHost {
    type Actor: Copy + Eq;
    type Error;

    fn first_player_actor(&mut self) -> Result<Option<Self::Actor>, Self::Error>;
    fn route(&mut self, route: IntroRoute) -> Result<Option<Self::Actor>, Self::Error>;
    fn route_successor(&mut self, route: Self::Actor) -> Result<Option<Self::Actor>, Self::Error>;
    /// 00462750 (selector90/00462660), including 004600F0 admission.
    fn create_helper(&mut self) -> Result<Option<Self::Actor>, Self::Error>;
    /// 00478E00, including on-demand selector16 construction.
    fn camera(&mut self) -> Result<Option<Self::Actor>, Self::Error>;
    fn actor_handle(&mut self, actor: Self::Actor) -> Result<u32, Self::Error>;
    fn reference_count(&mut self, list: IntroActorList) -> Result<i32, Self::Error>;
    fn reference_handle(&mut self, list: IntroActorList, index: i32) -> Result<u32, Self::Error>;
    fn main_reference_handle(&mut self) -> Result<u32, Self::Error>;
    fn resolve_actor(&mut self, handle: u32) -> Result<Option<Self::Actor>, Self::Error>;
    /// Actor virtual +58; null is a native skip.
    fn derived_actor(&mut self, actor: Self::Actor) -> Result<Option<Self::Actor>, Self::Error>;
    /// Derived virtual +CC then returned-object byte +22C; null is false.
    fn active_component(&mut self, derived: Self::Actor) -> Result<bool, Self::Error>;
    fn actor_state_c4(&mut self, actor: Self::Actor) -> Result<i32, Self::Error>;
    fn position_words(&mut self, actor: Self::Actor) -> Result<[u16; 3], Self::Error>;
    fn word(&mut self, actor: Self::Actor, field: IntroWord) -> Result<i32, Self::Error>;
    fn byte(&mut self, actor: Self::Actor, field: IntroByte) -> Result<u8, Self::Error>;
    fn world_present(&mut self) -> Result<bool, Self::Error>;
    fn effect(&mut self, effect: IntroEffect<Self::Actor>) -> Result<(), Self::Error>;
}

/// Constructor 00443890 leaves these words unwritten. The caller must provide
/// actual allocation bytes or an explicitly labeled allocation approximation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntroAllocationWords {
    pub elapsed_138: i32,
    pub deadline_140: i32,
    /// Normal entry to stage8 writes this before its first read. Its allocation
    /// contents may remain unknown throughout stages0..7.
    pub sequential_timer_150: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntroSequence<A> {
    pub controller: A,
    pub elapsed_138: i32,
    pub deadline_140: i32,
    pub timer_144: i32,
    pub stage_148: u32,
    pub sequential_index_14c: i32,
    pub sequential_timer_150: Option<i32>,
    pub primary_helper_154: Option<A>,
    pub secondary_helper_158: Option<A>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntroHelper {
    Primary,
    Secondary,
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum IntroError<E> {
    #[error("intro host operation failed")]
    Host(E),
    #[error("intro reached a null {0:?} helper dereference")]
    MissingHelper(IntroHelper),
    #[error("intro reached a null world camera dereference")]
    MissingCamera,
    #[error("intro reached a null route dereference at {0:?}")]
    MissingRoute(IntroRoute),
    #[error("intro stage8 reads unwritten sequential timer +150")]
    UnwrittenSequentialTimer,
}

impl<A: Copy + Eq> IntroSequence<A> {
    pub fn new(controller: A, allocation: IntroAllocationWords) -> Self {
        Self {
            controller,
            elapsed_138: allocation.elapsed_138,
            deadline_140: allocation.deadline_140,
            timer_144: 0,
            stage_148: 0,
            sequential_index_14c: 0,
            sequential_timer_150: allocation.sequential_timer_150,
            primary_helper_154: None,
            secondary_helper_158: None,
        }
    }

    /// 00444930. Errors retain effects already reached, matching other native
    /// actor owners; they do not silently consume an unexecuted cutscene stage.
    pub fn tick<H: IntroHost<Actor = A>>(
        &mut self,
        host: &mut H,
        scheduler_microseconds: u32,
    ) -> Result<(), IntroError<H::Error>> {
        let mut remaining = (scheduler_microseconds >> 10) as i32;
        self.elapsed_138 = self.elapsed_138.wrapping_add(remaining);
        self.deadline_140 = self.deadline_140.wrapping_sub(remaining);
        if self.deadline_140 <= 0 {
            self.timer_144 = remaining;
        }
        while self.timer_144 <= remaining {
            let elapsed = self.timer_144;
            let old_stage = self.stage_148;
            remaining = remaining.wrapping_sub(elapsed);
            self.advance(host, elapsed)?;
            self.transition(host, false)?;
            if self.stage_148 == old_stage {
                self.stage_148 = self.stage_148.wrapping_add(1);
            }
            self.transition(host, true)?;
            if self.timer_144 == 0 {
                self.timer_144 = 5000;
            }
            if self.deadline_140 < self.timer_144 {
                self.deadline_140 = self.timer_144.wrapping_add(250);
            }
        }
        if remaining != 0 {
            self.advance(host, remaining)?;
            self.timer_144 = self.timer_144.wrapping_sub(remaining);
        }
        for helper in [self.secondary_helper_158, self.primary_helper_154]
            .into_iter()
            .flatten()
        {
            self.effect(
                host,
                IntroEffect::TickHelper {
                    helper,
                    scheduler_microseconds,
                },
            )?;
        }
        if self.stage_148 == 13 {
            if let Some(player) = host.first_player_actor().map_err(IntroError::Host)? {
                let helper = self.helper(IntroHelper::Primary)?;
                self.effect(host, IntroEffect::PlayerPoseFromHelper { player, helper })?;
            }
        }
        Ok(())
    }

    /// 00443B10. States12/13 intentionally have no specific entry/exit action.
    pub fn transition<H: IntroHost<Actor = A>>(
        &mut self,
        host: &mut H,
        entering: bool,
    ) -> Result<(), IntroError<H::Error>> {
        crate::event_log::intro(self.stage_148, entering, self.timer_144, self.elapsed_138);
        if entering {
            self.timer_144 = self.timer_144.wrapping_add(1);
        }
        if self.stage_148 == 0 && !entering {
            return self.initialize_helpers(host);
        }
        if self.stage_148 == 14 {
            if host.world_present().map_err(IntroError::Host)? {
                self.effect(
                    host,
                    IntroEffect::CompleteMission {
                        selection: 18,
                        start_ordinal: 1,
                        argument: 0,
                    },
                )?;
            }
            return Ok(());
        }
        if !entering {
            return Ok(());
        }
        match self.stage_148 {
            1 => self.timer_144 = 10000,
            2 => {
                self.timer_144 = 2500;
                if let Some(helper) = self.secondary_helper_158 {
                    self.set_word(host, helper, IntroWord::Helper13c, 1000)?;
                    self.set_word(host, helper, IntroWord::Helper138, 50)?;
                    self.set_byte(host, helper, IntroByte::Helper140, 0)?;
                }
                self.activate_list(host, IntroActorList::Primary14, false)?;
            }
            3 => {
                self.timer_144 = 2000;
                self.activate_list(host, IntroActorList::Secondary24, false)?;
            }
            4 => {
                self.timer_144 = 1500;
                let mut index = 0;
                while index
                    < host
                        .reference_count(IntroActorList::Secondary24)
                        .map_err(IntroError::Host)?
                {
                    if let Some(actor) =
                        self.list_actor(host, IntroActorList::Secondary24, index)?
                    {
                        let camera = self.require_camera(host)?;
                        self.camera_primary(host, camera, actor)?;
                        self.set_byte(host, camera, IntroByte::Camera184, 0)?;
                        self.set_byte(host, camera, IntroByte::Camera187, 1)?;
                        break;
                    }
                    index += 1;
                }
                let helper = self.helper(IntroHelper::Secondary)?;
                if host
                    .byte(helper, IntroByte::Helper140)
                    .map_err(IntroError::Host)?
                    == 0
                {
                    self.set_word(host, helper, IntroWord::Helper13c, 2000)?;
                }
            }
            5 => {
                let camera = self.require_camera(host)?;
                self.timer_144 = 2000;
                self.deadline_140 = 4000;
                self.camera_primary(host, camera, self.helper(IntroHelper::Primary)?)?;
                self.set_byte(host, camera, IntroByte::Camera184, 1)?;
                self.set_byte(host, camera, IntroByte::Camera187, 1)?;
                self.flags_list(host, IntroActorList::Primary14, u32::MAX, 0x20)?;
            }
            6 => {
                self.timer_144 = 5000;
                self.deadline_140 = 6000;
                let mut closest = i32::MAX;
                let mut index = 0;
                while index
                    < host
                        .reference_count(IntroActorList::Secondary24)
                        .map_err(IntroError::Host)?
                {
                    if let Some(actor) =
                        self.list_actor(host, IntroActorList::Secondary24, index)?
                    {
                        if let Some(derived) =
                            host.derived_actor(actor).map_err(IntroError::Host)?
                        {
                            if host.active_component(derived).map_err(IntroError::Host)? {
                                let position = host
                                    .position_words(self.controller)
                                    .map_err(IntroError::Host)?;
                                let other = host.position_words(actor).map_err(IntroError::Host)?;
                                let distance = intro_distance(position, other);
                                if distance < closest {
                                    closest = distance;
                                    let camera = self.require_camera(host)?;
                                    self.camera_primary(host, camera, actor)?;
                                    self.set_byte(host, camera, IntroByte::Camera184, 0)?;
                                    self.set_byte(host, camera, IntroByte::Camera187, 1)?;
                                }
                            }
                        }
                    }
                    index += 1;
                }
            }
            7 => {
                self.deadline_140 = 15000;
                self.timer_144 = 7500;
                let route = host
                    .route(IntroRoute::Advance08)
                    .map_err(IntroError::Host)?
                    .ok_or(IntroError::MissingRoute(IntroRoute::Advance08))?;
                let route = host.route_successor(route).map_err(IntroError::Host)?;
                let helper = self.helper(IntroHelper::Primary)?;
                self.effect(host, IntroEffect::HelperRoute { helper, route })?;
                self.configure_helper(host, helper, 200, 500, true)?;
                let camera = self.require_camera(host)?;
                self.effect(host, IntroEffect::CameraSecondaryFromPrimary { camera })?;
                self.camera_primary(host, camera, helper)?;
                self.set_byte(host, camera, IntroByte::Camera184, 1)?;
                let handle = host.main_reference_handle().map_err(IntroError::Host)?;
                if let Some(actor) = host.resolve_actor(handle).map_err(IntroError::Host)? {
                    self.activate(host, actor, false)?;
                }
            }
            8 => {
                self.sequential_index_14c = 0;
                self.sequential_timer_150 = Some(2000);
                self.timer_144 = 2000;
                self.deadline_140 = 10000;
            }
            9 => {
                let route = host.route(IntroRoute::Exit0c).map_err(IntroError::Host)?;
                self.timer_144 = 3000;
                self.deadline_140 = 10000;
                let handle = host
                    .reference_handle(IntroActorList::Exit2c, 0)
                    .map_err(IntroError::Host)?;
                let helper = self.helper(IntroHelper::Primary)?;
                self.effect(host, IntroEffect::HelperTarget { helper, handle })?;
                self.set_word(host, helper, IntroWord::Helper13c, 0)?;
                self.set_byte(host, helper, IntroByte::Helper140, 0)?;
                self.set_word(host, helper, IntroWord::Helper138, 50)?;
                if route.is_some() {
                    let camera = host.camera().map_err(IntroError::Host)?;
                    self.effect(host, IntroEffect::HelperRoute { helper, route })?;
                    if let Some(camera) = camera {
                        self.set_byte(host, camera, IntroByte::Camera184, 1)?;
                        let handle = host
                            .reference_handle(IntroActorList::Exit2c, 0)
                            .map_err(IntroError::Host)?;
                        self.effect(host, IntroEffect::CameraSecondary { camera, handle })?;
                    }
                }
                self.activate_list(host, IntroActorList::Exit2c, true)?;
            }
            10 => {
                self.timer_144 = 7000;
                self.deadline_140 = 10000;
            }
            11 => {
                self.timer_144 = 7000;
                self.deadline_140 = 12000;
            }
            _ => {}
        }
        Ok(())
    }

    /// 00444630. Division of signed elapsed words truncates toward zero.
    pub fn advance<H: IntroHost<Actor = A>>(
        &mut self,
        host: &mut H,
        elapsed: i32,
    ) -> Result<(), IntroError<H::Error>> {
        match self.stage_148 {
            1 if self.elapsed_138 > 500 => {
                let Some(helper) = self.primary_helper_154 else {
                    return self.effect(
                        host,
                        IntroEffect::DisposeController {
                            actor: self.controller,
                        },
                    );
                };
                self.add_helper_timer(host, helper, elapsed / 2, 2000)?;
            }
            5 => {
                self.timer_144 = 500;
                let mut index = 0;
                while index
                    < host
                        .reference_count(IntroActorList::Secondary24)
                        .map_err(IntroError::Host)?
                {
                    if let Some(actor) =
                        self.list_actor(host, IntroActorList::Secondary24, index)?
                    {
                        if host.actor_state_c4(actor).map_err(IntroError::Host)? == 3 {
                            if let Some(derived) =
                                host.derived_actor(actor).map_err(IntroError::Host)?
                            {
                                if host.active_component(derived).map_err(IntroError::Host)? {
                                    self.timer_144 = 0;
                                    break;
                                }
                            }
                        }
                    }
                    index += 1;
                }
            }
            7 => {
                self.add_helper_timer(host, self.helper(IntroHelper::Primary)?, elapsed, 1500)?;
                let mut index = 0;
                while index
                    < host
                        .reference_count(IntroActorList::Secondary24)
                        .map_err(IntroError::Host)?
                {
                    if let Some(actor) =
                        self.list_actor(host, IntroActorList::Secondary24, index)?
                    {
                        if host.actor_state_c4(actor).map_err(IntroError::Host)? == 3 {
                            self.timer_144 = 2000;
                            break;
                        }
                    }
                    index += 1;
                }
            }
            8 => {
                if self.sequential_index_14c
                    < host
                        .reference_count(IntroActorList::Sequential1c)
                        .map_err(IntroError::Host)?
                {
                    let timer = self
                        .sequential_timer_150
                        .ok_or(IntroError::UnwrittenSequentialTimer)?
                        .wrapping_sub(elapsed);
                    self.sequential_timer_150 = Some(timer);
                    self.timer_144 = 5000;
                    if timer < 0 {
                        while self.sequential_index_14c
                            < host
                                .reference_count(IntroActorList::Sequential1c)
                                .map_err(IntroError::Host)?
                        {
                            if let Some(actor) = self.list_actor(
                                host,
                                IntroActorList::Sequential1c,
                                self.sequential_index_14c,
                            )? {
                                if host.actor_state_c4(actor).map_err(IntroError::Host)? == 3 {
                                    self.sequential_timer_150 = Some(2000);
                                    self.effect(host, IntroEffect::RetireSequential { actor })?;
                                }
                            }
                            self.sequential_index_14c = self.sequential_index_14c.wrapping_add(1);
                            if self.sequential_timer_150.is_some_and(|timer| timer >= 0) {
                                break;
                            }
                        }
                    }
                }
            }
            9 | 10 => {
                if self.stage_148 == 10 {
                    self.timer_144 = 1000;
                }
                let helper = self.helper(IntroHelper::Primary)?;
                self.add_helper_timer(host, helper, elapsed / 4, 500)?;
                if host
                    .byte(helper, IntroByte::Helper140)
                    .map_err(IntroError::Host)?
                    != 0
                {
                    self.timer_144 = 0;
                }
            }
            12 => {
                let helper = self.helper(IntroHelper::Primary)?;
                self.add_helper_timer(host, helper, elapsed, 2000)?;
                if host
                    .byte(helper, IntroByte::Helper140)
                    .map_err(IntroError::Host)?
                    == 0
                {
                    self.timer_144 = 750;
                }
            }
            13 => {
                let primary = self.helper(IntroHelper::Primary)?;
                let value = self.add_helper_timer(host, primary, elapsed.wrapping_mul(2), 2000)?;
                self.set_word(
                    host,
                    self.helper(IntroHelper::Secondary)?,
                    IntroWord::Helper13c,
                    value,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn initialize_helpers<H: IntroHost<Actor = A>>(
        &mut self,
        host: &mut H,
    ) -> Result<(), IntroError<H::Error>> {
        self.elapsed_138 = 0;
        if let Some(player) = host.first_player_actor().map_err(IntroError::Host)? {
            self.membership(host, player, IntroMembership::Flag1, false)?;
            self.membership(host, player, IntroMembership::Flag0, false)?;
            self.effect(
                host,
                IntroEffect::Flags {
                    actor: player,
                    and_mask: 0xffff_fedb,
                    or_mask: 0,
                },
            )?;
            self.effect(
                host,
                IntroEffect::PositionFrom {
                    actor: player,
                    source: self.controller,
                },
            )?;
        }
        let route = host
            .route(IntroRoute::Primary00)
            .map_err(IntroError::Host)?;
        self.primary_helper_154 = host.create_helper().map_err(IntroError::Host)?;
        if let (Some(helper), Some(route)) = (self.primary_helper_154, route) {
            self.membership(host, helper, IntroMembership::Flag1, false)?;
            self.membership(host, helper, IntroMembership::Flag0, false)?;
            self.prepare_helper_visual(host, helper)?;
            self.effect(
                host,
                IntroEffect::PositionFrom {
                    actor: helper,
                    source: route,
                },
            )?;
            self.effect(
                host,
                IntroEffect::HelperRoute {
                    helper,
                    route: Some(route),
                },
            )?;
            self.configure_helper(host, helper, 0, 100, true)?;
        }
        self.secondary_helper_158 = host.create_helper().map_err(IntroError::Host)?;
        let route = host
            .route(IntroRoute::Secondary04)
            .map_err(IntroError::Host)?;
        if let (Some(helper), Some(route)) = (self.secondary_helper_158, route) {
            self.membership(host, helper, IntroMembership::Flag0, false)?;
            self.prepare_helper_visual(host, helper)?;
            self.membership(host, helper, IntroMembership::Flag1, false)?;
            self.effect(
                host,
                IntroEffect::PositionFrom {
                    actor: helper,
                    source: route,
                },
            )?;
            self.effect(
                host,
                IntroEffect::HelperRoute {
                    helper,
                    route: Some(route),
                },
            )?;
            self.set_byte(host, helper, IntroByte::Helper140, 1)?;
        }
        if let (Some(primary), Some(secondary)) =
            (self.primary_helper_154, self.secondary_helper_158)
        {
            let handle = host.actor_handle(secondary).map_err(IntroError::Host)?;
            self.effect(
                host,
                IntroEffect::HelperTarget {
                    helper: primary,
                    handle,
                },
            )?;
            let camera = self.require_camera(host)?;
            self.camera_primary(host, camera, primary)?;
            let handle = host.actor_handle(secondary).map_err(IntroError::Host)?;
            self.effect(host, IntroEffect::CameraSecondary { camera, handle })?;
            self.set_byte(host, camera, IntroByte::Camera184, 1)?;
            self.effect(host, IntroEffect::CameraParameterMode { camera, mode: 1 })?;
        } else {
            self.effect(
                host,
                IntroEffect::DisposeController {
                    actor: self.controller,
                },
            )?;
        }
        self.flags_list(host, IntroActorList::Primary14, !0x20, 0)
    }

    fn prepare_helper_visual<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        helper: A,
    ) -> Result<(), IntroError<H::Error>> {
        self.effect(
            host,
            IntroEffect::Dirty {
                actor: helper,
                bits: 0x100,
            },
        )?;
        self.effect(
            host,
            IntroEffect::Resource42 {
                actor: helper,
                value: 0x2be,
            },
        )?;
        self.effect(host, IntroEffect::RefreshVisual { actor: helper })
    }

    fn configure_helper<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        helper: A,
        word13c: i32,
        word138: i32,
        clear142: bool,
    ) -> Result<(), IntroError<H::Error>> {
        self.set_word(host, helper, IntroWord::Helper13c, word13c)?;
        self.set_word(host, helper, IntroWord::Helper138, word138)?;
        self.set_byte(host, helper, IntroByte::Helper140, 0)?;
        if clear142 {
            self.set_byte(host, helper, IntroByte::Helper142, 0)?;
        }
        Ok(())
    }

    fn flags_list<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        list: IntroActorList,
        and_mask: u32,
        or_mask: u32,
    ) -> Result<(), IntroError<H::Error>> {
        let mut index = 0;
        while index < host.reference_count(list).map_err(IntroError::Host)? {
            if let Some(actor) = self.list_actor(host, list, index)? {
                self.effect(
                    host,
                    IntroEffect::Flags {
                        actor,
                        and_mask,
                        or_mask,
                    },
                )?;
            }
            index += 1;
        }
        Ok(())
    }

    fn activate_list<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        list: IntroActorList,
        enable: bool,
    ) -> Result<(), IntroError<H::Error>> {
        let mut index = 0;
        while index < host.reference_count(list).map_err(IntroError::Host)? {
            if let Some(actor) = self.list_actor(host, list, index)? {
                self.activate(host, actor, enable)?;
            }
            index += 1;
        }
        Ok(())
    }

    fn activate<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        actor: A,
        enable: bool,
    ) -> Result<(), IntroError<H::Error>> {
        if let Some(derived) = host.derived_actor(actor).map_err(IntroError::Host)? {
            if enable {
                self.membership(host, derived, IntroMembership::Flag1, true)?;
            }
            let delay = host
                .word(derived, IntroWord::DerivedDelay144)
                .map_err(IntroError::Host)?;
            if delay < 0 {
                self.set_word(
                    host,
                    derived,
                    IntroWord::DerivedDelay144,
                    delay.wrapping_mul(-1000),
                )?;
            }
            self.effect(
                host,
                IntroEffect::Flags {
                    actor: derived,
                    and_mask: 0xfb9f_ffff,
                    or_mask: 0,
                },
            )?;
        }
        Ok(())
    }

    fn list_actor<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        list: IntroActorList,
        index: i32,
    ) -> Result<Option<A>, IntroError<H::Error>> {
        let handle = host
            .reference_handle(list, index)
            .map_err(IntroError::Host)?;
        host.resolve_actor(handle).map_err(IntroError::Host)
    }

    fn add_helper_timer<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        helper: A,
        elapsed: i32,
        maximum: i32,
    ) -> Result<i32, IntroError<H::Error>> {
        let old = host
            .word(helper, IntroWord::Helper13c)
            .map_err(IntroError::Host)?;
        let value = old.wrapping_add(elapsed).min(maximum);
        self.set_word(host, helper, IntroWord::Helper13c, value)?;
        Ok(value)
    }

    fn helper<E>(&self, helper: IntroHelper) -> Result<A, IntroError<E>> {
        match helper {
            IntroHelper::Primary => self.primary_helper_154,
            IntroHelper::Secondary => self.secondary_helper_158,
        }
        .ok_or(IntroError::MissingHelper(helper))
    }

    fn require_camera<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
    ) -> Result<A, IntroError<H::Error>> {
        host.camera()
            .map_err(IntroError::Host)?
            .ok_or(IntroError::MissingCamera)
    }

    fn camera_primary<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        camera: A,
        actor: A,
    ) -> Result<(), IntroError<H::Error>> {
        let handle = host.actor_handle(actor).map_err(IntroError::Host)?;
        self.effect(host, IntroEffect::CameraPrimary { camera, handle })
    }

    fn membership<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        actor: A,
        list: IntroMembership,
        enabled: bool,
    ) -> Result<(), IntroError<H::Error>> {
        self.effect(
            host,
            IntroEffect::Membership {
                actor,
                list,
                enabled,
            },
        )
    }

    fn set_word<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        actor: A,
        field: IntroWord,
        value: i32,
    ) -> Result<(), IntroError<H::Error>> {
        self.effect(
            host,
            IntroEffect::Word {
                actor,
                field,
                value,
            },
        )
    }

    fn set_byte<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        actor: A,
        field: IntroByte,
        value: u8,
    ) -> Result<(), IntroError<H::Error>> {
        self.effect(
            host,
            IntroEffect::Byte {
                actor,
                field,
                value,
            },
        )
    }

    fn effect<H: IntroHost<Actor = A>>(
        &self,
        host: &mut H,
        effect: IntroEffect<A>,
    ) -> Result<(), IntroError<H::Error>> {
        host.effect(effect).map_err(IntroError::Host)
    }
}

/// State6's DWORD-wrapped square sum and 0048EFC0 integer square root.
/// A high bit uses logical >>2 before sqrt and doubles afterward.
fn intro_distance(left: [u16; 3], right: [u16; 3]) -> i32 {
    let sum = left.into_iter().zip(right).fold(0_u32, |sum, (a, b)| {
        let difference = i32::from(a.wrapping_sub(b) as i16);
        sum.wrapping_add(difference.wrapping_mul(difference) as u32)
    });
    if sum & 0x8000_0000 != 0 {
        integer_sqrt(sum >> 2) as i32 * 2
    } else {
        integer_sqrt(sum) as i32
    }
}

fn integer_sqrt(value: u32) -> u32 {
    // 0048EFC0 returns zero for signed nonpositive operands. Callers above
    // remove the sign bit before invoking it.
    if value as i32 <= 0 {
        return 0;
    }
    let mut root = 0_u32;
    let mut bit = 1_u32 << 15;
    while bit != 0 {
        let trial = root | bit;
        if u64::from(trial) * u64::from(trial) <= u64::from(value) {
            root = trial;
        }
        bit >>= 1;
    }
    root
}

#[cfg(test)]
#[path = "intro_sequence_tests.rs"]
mod tests;
