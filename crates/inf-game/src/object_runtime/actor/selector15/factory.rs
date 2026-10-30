//! The factory's update `0042EE30` (selector 15's +24): the pad test
//! `0042EC60` and docking `0042F490` while `+188` holds the process NULL,
//! the pull toward the dock point and the camera while a vehicle is docked,
//! then production `0042E570` over the shop state `+160`. Undocking
//! `0042F2A0` runs when the shop menu is destroyed, the state store
//! `00431CE0` rebuilds a bound menu, and `00431E50` starts a product.

use inf_formats::{native_basis::isqrt_48efc0, FactoryProduct};
use thiserror::Error;

use super::broadcast::{UnitBroadcast, UnitBroadcastCode};
use super::Selector15State;
use crate::factory_shop::{FactoryShopError, FactoryShopTables, ShopMenuPlan, ShopPlayer};
use crate::menu::{MenuBasis, MenuState};
use crate::object_runtime::{
    ActorChildResource, ActorGrantDifferenceDelivery, ActorGrantDifferenceNotificationError,
    ActorGrantDifferenceNotificationRequest, ActorHandle, CameraSettings, FollowTargetRequest,
    GrantOwner, LiveGrantContext, LiveGrantError, LiveGrantOperation, LiveGrantRequest,
    ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext, PlayerActorLookupError,
    PlayerActorLookupRequest, PlayerAttachError, PlayerId, PlayerMessageError, ShownMenuOwner,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::process_sound::ProcessSoundRef;

/// The camera mode and offset a docked factory sets every tick.
const DOCK_CAMERA_MODE: u32 = 1;
const DOCK_CAMERA_OFFSET: [i16; 3] = [0x12c, 0x1f4, 0xfa24_u16 as i16];
/// The pull's per-axis cap toward the dock point (positive steps only).
const PULL_LIMIT: i32 = 0x10;
/// `0042EC60`'s docking speed limit.
const DOCK_SPEED: u32 = 0x2bc;
/// The dock cooldown's cap and scale: 50 µs per unit of distance.
const COOLDOWN_LIMIT: i32 = 2_000_000;
const COOLDOWN_SCALE: i32 = 50;
/// The silent player commands docking sets.
const DOCK_COMMANDS: [u32; 3] = [3, 4, 0x26];
/// "Access denied"; team 0x12 also gets command 0x1D.
const ACCESS_DENIED_TEXT: u16 = 0x2fb;
const ENEMY_TEAM: u32 = 0x12;
const ENEMY_COMMAND: u32 = 0x1d;
/// Production's two-second phases, in microseconds.
const PHASE_MICROS: u32 = 2_000_000;
/// "There is insufficient space in your hold".
const HOLD_FULL_TEXT: u16 = 0x143;
/// The repair `0046B2B0` kind and the upgrade product kind.
const REPAIR_ID: u16 = 4;
const UPGRADE_KIND: u32 = 9;
/// The Remote Beamer's first item (resource 7) and the receiver after it.
const BEAMER_RESOURCE: u32 = 7;
const RECEIVER_RESOURCE: u32 = 0x25;
/// The sound of a new upgrade.
const NEW_UPGRADE_SOUND: u32 = 0x30;
/// The looped sound: busy in states 3 to 6, idle otherwise, heard within
/// 3000 units of the player's unit.
const BUSY_SOUND: u32 = 0x1c;
const IDLE_SOUND: u32 = 0x4b;
const SOUND_RANGE: i32 = 3000;

/// What the factory's staff reaches from the shop and from the pair pass:
/// the grants a scientist brings, the slot flash, the mission's placement
/// bit and, to rebuild a bound menu, the shop's tables.
pub trait FactoryStaffHost {
    type Error: std::fmt::Display;

    fn process(&self) -> &ParticleFactoryProcessState;
    /// The context of the grants docking and staffing merge, whose notices
    /// reach the player's message.
    fn grants(&self) -> LiveGrantContext<'_>;
    /// The shop's executable tables.
    fn shop_tables(&self) -> Result<&FactoryShopTables, Self::Error>;
    /// `00489A90(2, position, 0)`, the slot flash.
    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), Self::Error>;
    /// `0048EAD0(index, 0)`: the current world's placement-completion bit.
    fn complete_placement(&mut self, index: i8) -> Result<(), Self::Error>;
}

/// What the staff release `0040DCB0` reaches: the world factory, the slot
/// flash, and for a docked vehicle its free slot and its take. A factory
/// lends its [`FactoryHost`]; the terrain bridge, which releases to no
/// vehicle, lends its own.
pub trait StaffReleaseHost {
    type ReleaseError;
    /// `00460310` with H13 resource 2 (a scientist) at `position`.
    fn release_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, Self::ReleaseError>;
    /// `00489A90(2, position, 0)`, the slot flash.
    fn release_flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<(), Self::ReleaseError>;
    /// The process NULL a vehicle's free slot holds (`004120E0`).
    fn release_null(&self) -> Option<crate::object_runtime::ActorHandle>;
    /// `00421810`: the vehicle takes `scientist` into its hold.
    fn release_carry(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        scientist: ObjectId,
    ) -> Result<bool, Self::ReleaseError>;
}

impl<H: FactoryHost + ?Sized> StaffReleaseHost for H {
    type ReleaseError = H::Error;
    fn release_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, H::Error> {
        self.create_scientist(arena, position)
    }
    fn release_flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<(), H::Error> {
        self.flash(arena, position)
    }
    fn release_null(&self) -> Option<crate::object_runtime::ActorHandle> {
        self.process().process_null_handle
    }
    fn release_carry(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        scientist: ObjectId,
    ) -> Result<bool, H::Error> {
        self.carry_scientist(arena, vehicle, scientist)
    }
}

/// The services the factory reaches.
pub trait FactoryHost: FactoryStaffHost {
    fn camera_settings(&self) -> CameraSettings;
    /// `00478E00`, the world camera.
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<ObjectId, Self::Error>;
    /// `00487F10` then `004923C0`: a one-shot sound.
    fn sound(
        &mut self,
        arena: &ObjectArena,
        sound: u32,
        position: [u16; 3],
        volume: u32,
        pitch: u32,
    ) -> Result<(), Self::Error>;
    /// `00487F60` at the factory, at full volume and pitch with flag 0.
    /// None when `0055EE9C` holds no resource and loading it fails.
    fn create_loop(
        &mut self,
        arena: &ObjectArena,
        factory: ObjectId,
        sound: u32,
    ) -> Result<Option<ProcessSoundRef>, Self::Error>;
    /// `00487FC0` at the factory with full pitch.
    fn update_loop(
        &mut self,
        arena: &ObjectArena,
        factory: ObjectId,
        reference: ProcessSoundRef,
        volume: u32,
    ) -> Result<(), Self::Error>;
    /// `00487FE0`.
    fn release_loop(&mut self, reference: ProcessSoundRef) -> Result<(), Self::Error>;
    /// `004135D0(text, 0)` on the player's message.
    fn text(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        text: u16,
    ) -> Result<(), Self::Error>;
    /// `0046C760(raw_id, silent)`.
    fn command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        raw_id: u32,
        silent: u32,
    ) -> Result<(), Self::Error>;
    /// `0046B2B0(id, amount)` with the notices and messages it sends.
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
        amount: i16,
    ) -> Result<(), Self::Error>;
    /// `00460310` with H13 resource 2 at `position` (an evacuated
    /// scientist) through the world factory. None when nothing was made.
    fn create_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// `00421810`: the vehicle takes `scientist` (or a bought item) into
    /// its hold.
    fn carry_scientist(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        scientist: ObjectId,
    ) -> Result<bool, Self::Error>;
    /// `0048F060`, the world RNG's next WORD.
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// The factory's `+4C` (`0047AAF0`): `resource` built at `position`.
    /// The built actor, or None.
    fn create_item(
        &mut self,
        arena: &mut ObjectArena,
        factory: ObjectId,
        resource: ActorChildResource,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// The shop's `004137D0` broadcast skipped these receivers, whose `+98`
    /// the port does not build.
    fn broadcast_skipped(&mut self, receivers: &[String]);
}

#[derive(Debug, Error)]
pub enum FactoryError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Construction(#[from] crate::object_runtime::ActorConstructionError),
    #[error(transparent)]
    Derived(#[from] crate::object_runtime::DerivedActorError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Lookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Grants(#[from] LiveGrantError),
    #[error(transparent)]
    Event22(#[from] ActorGrantDifferenceNotificationError),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
    #[error(transparent)]
    Shop(#[from] FactoryShopError),
    #[error("0046D310 reaches {0:?}'s +64 view, which is ported only for the vehicle")]
    WeaponView(ObjectId),
    #[error("player {0:?} has no shown menu (+0C) for its command")]
    MissingShownMenu(PlayerId),
    #[error("004537E0 reaches menu code {0}")]
    MenuCode(u32),
    #[error("the shop menu's command: {0}")]
    Menu(String),
    #[error("0042F655 reaches 00488AA0 without a player")]
    MissingFirstPlayer,
    #[error("0043BEE6 grants a filled course target's +7A without a player (00488AA0)")]
    CourseTargetFirstPlayer,
    #[error("the shop menu reaches row action {0:08X}")]
    ShopAction(u32),
    #[error("0042F800 reaches token level {0}, past the player's +13C bytes")]
    TokenLevel(u32),
    #[error("factory {factory:?} reads its unwritten {field}")]
    Unwritten {
        factory: ObjectId,
        field: &'static str,
    },
    #[error("factory {0:?} needs the process NULL 0054E894")]
    MissingProcessNull(ObjectId),
    #[error("the unit's broadcast: {0}")]
    UnitBroadcast(crate::object_runtime::ActorMessageError),
    #[error("the unit's broadcast leaves {actor:?}'s {reply:?}, which it does not apply")]
    UnitBroadcastEffect {
        actor: ObjectId,
        reply: crate::object_runtime::ActorMessageReply,
    },
    #[error("factory host at {address:08X}: {error}")]
    Host { address: u32, error: E },
}

fn at<E>(address: u32) -> impl FnOnce(E) -> FactoryError<E> {
    move |error| FactoryError::Host { address, error }
}

/// The signed WORD difference the factory code forms with 16-bit subtraction.
fn delta(to: u16, from: u16) -> i32 {
    i32::from(to.wrapping_sub(from) as i16)
}

/// `0048EFC0` over a sum of squares: a sum with bit 31 set is quartered
/// and its root doubled.
fn distance(squares: u32) -> u32 {
    if squares & 0x8000_0000 != 0 {
        isqrt_48efc0((squares >> 2) as i32) << 1
    } else {
        isqrt_48efc0(squares as i32)
    }
}

/// The WORD progress `(value * 0xFFFF) / limit`, both shifted right 12
/// (or 16 above 0x0FFFFFFF) to stay in 32 bits.
fn progress(value: u32, limit: u32) -> u16 {
    let shift = if value > 0x0fff_ffff { 16 } else { 12 };
    let value = value >> shift;
    let scaled = (value << 16).wrapping_sub(value);
    scaled.checked_div(limit >> shift).unwrap_or(0) as u16
}

impl ObjectArena {
    fn factory_null<E>(
        &self,
        factory: ObjectId,
        process: &ParticleFactoryProcessState,
    ) -> Result<ActorHandle, FactoryError<E>> {
        process
            .process_null_handle
            .ok_or(FactoryError::MissingProcessNull(factory))
    }

    /// `0046B090` of the factory's `+188`.
    pub fn factory_player<E>(
        &self,
        factory: ObjectId,
        process: &ParticleFactoryProcessState,
    ) -> Result<Option<PlayerId>, FactoryError<E>> {
        let handle = self.selector15(factory)?.handle_188;
        self.lookup_player(handle, process)
    }

    fn lookup_player<E>(
        &self,
        handle: ActorHandle,
        process: &ParticleFactoryProcessState,
    ) -> Result<Option<PlayerId>, FactoryError<E>> {
        Ok(self.find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: Some(process),
            },
        )?)
    }

    fn factory_position_words<E>(&self, object: ObjectId) -> Result<[u16; 3], FactoryError<E>> {
        Ok(self
            .object(object)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(object))?)
    }

    fn dock_point<E>(&self, factory: ObjectId) -> Result<[u16; 3], FactoryError<E>> {
        self.selector15(factory)?
            .dock_190
            .ok_or(FactoryError::Unwritten {
                factory,
                field: "dock point +190",
            })
    }

    /// `0042EE30`, the factory's +24.
    pub fn tick_selector15<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        microseconds: i32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let null = self.factory_null(factory, host.process())?;
        if self.selector15(factory)?.dock_190.is_none() {
            // FactoryDockPointApproximation: the model's custom vertex writes
            // the dock point when the factory is drawn. Infest01's briefing
            // draws had written it before gameplay's first pad test; until
            // the port's first draw the pad test waits (see
            // FAMILY40D800_RUNTIME.md).
        } else if self.selector15(factory)?.handle_188 == null {
            let state = self.selector15_mut(factory)?;
            if microseconds < state.word_1c4 as i32 {
                state.word_1c4 = (state.word_1c4 as i32).wrapping_sub(microseconds) as u32;
            } else {
                match self.first_player_primary_actor()? {
                    None => self.selector15_mut(factory)?.byte_1bd = 1,
                    Some(actor) => {
                        if self.factory_dock_test(factory, actor)? {
                            self.dock_factory(factory, actor, host)?;
                            let handle = self.actor(actor)?.handle;
                            if let Some(player) = self.lookup_player(handle, host.process())? {
                                for command in DOCK_COMMANDS {
                                    host.command(self, player, command, 1)
                                        .map_err(at(0x0042_eebd))?;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let handle = self.selector15(factory)?.handle_188;
            let vehicle = self
                .resolve_actor_handle(handle)
                .map_err(|_| ObjectError::MissingActorRegistry)?;
            if let Some(vehicle) = vehicle {
                self.docked_factory(factory, vehicle, microseconds, host)?;
            }
        }
        self.run_factory_production(factory, microseconds, host)
    }

    /// `00431A90`, the `+3C` of a factory another machine owns: with no
    /// vehicle docked, the first player's unit on its pad (`0042EC60`)
    /// docks (`0042F490`). No cooldown, pull or production runs here.
    pub fn tick_remote_selector15<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let null = self.factory_null(factory, host.process())?;
        let state = self.selector15(factory)?;
        // FactoryDockPointApproximation, as the local update's: the pad
        // test waits for the first draw's dock point.
        if state.handle_188 != null || state.dock_190.is_none() {
            return Ok(());
        }
        if let Some(unit) = self.first_player_primary_actor()? {
            if self.factory_dock_test(factory, unit)? {
                self.dock_factory(factory, unit, host)?;
            }
        }
        Ok(())
    }

    /// `0042EC60`: a vehicle docks when it is within the pad (squared
    /// distance at most twice its radius squared, the height difference
    /// halved), slower than 700 and the factory is armed (`+1BD`). Farther
    /// than twice the radius arms the factory and sets the cooldown `+1C4`
    /// from the distance.
    fn factory_dock_test<E>(
        &mut self,
        factory: ObjectId,
        vehicle: ObjectId,
    ) -> Result<bool, FactoryError<E>> {
        let [x, y, z] = self.factory_position_words(vehicle)?;
        let [dock_x, dock_y, dock_z] = self.dock_point(factory)?;
        // 0042EC8F..0042ECAF: the vehicle's height moves halfway to the
        // dock's (signed, truncated), WORD-wrapped.
        let half = (i32::from(y as i16) - i32::from(dock_y as i16)) / 2;
        let height = (half as u16).wrapping_add(dock_y);
        let dx = delta(x, dock_x);
        let dy = delta(height, dock_y);
        let dz = delta(z, dock_z);
        let squares = (dx * dx).wrapping_add(dy * dy).wrapping_add(dz * dz) as u32;
        let radius = u32::from(self.object(vehicle)?.base.word_28);
        let radius = radius.wrapping_mul(radius);
        if squares > radius.wrapping_mul(2) {
            if squares > radius.wrapping_mul(4) {
                let state = self.selector15_mut(factory)?;
                state.byte_1bd = 1;
                let root = isqrt_48efc0(squares.wrapping_sub(radius.wrapping_mul(3)) as i32);
                let cooldown = (root as i32).wrapping_mul(COOLDOWN_SCALE);
                state.word_1c4 = cooldown.min(COOLDOWN_LIMIT) as u32;
            }
            return Ok(false);
        }
        if self.selector15(factory)?.byte_1bd == 0 {
            return Ok(false);
        }
        let [vx, _, vz] = self.actor(vehicle)?.words_8e_92;
        let (vx, vz) = (i32::from(vx as i16), i32::from(vz as i16));
        let speed = (vx * vx).wrapping_add(vz * vz) as u32;
        Ok(distance(speed) <= DOCK_SPEED)
    }

    /// `0042F490`. A vehicle whose player is on the factory's team (or the
    /// factory is neutral) docks: `+188` names it, the player's `+DC` the
    /// factory, the player gains every ID the factory knows (with their
    /// notices) and the factory's mask becomes the player's, event 22 goes
    /// to every actor, and the shop is shown. Otherwise "Access denied".
    fn dock_factory<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        vehicle: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let handle = self.actor(vehicle)?.handle;
        let Some(player) = self.lookup_player(handle, host.process())? else {
            return Ok(());
        };
        let team = self.actor(factory)?.word_b4;
        if team == 1 || team == 0 || team == self.actor(vehicle)?.word_b4 {
            let factory_handle = self.actor(factory)?.handle;
            self.selector15_mut(factory)?.handle_188 = handle;
            self.world_player_mut(player)?.handle_dc = factory_handle;
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::Merge {
                        source: GrantOwner::Selector15Factory198(factory),
                    },
                },
                &host.grants(),
            )?;
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Selector15Factory198(factory),
                    operation: LiveGrantOperation::Copy {
                        source: GrantOwner::Player9c(player),
                    },
                },
                &host.grants(),
            )?;
            // 0047DAF0(0x16, 0, factory +32) with a null sender and radius 0.
            self.notify_actor_grant_difference(
                ActorGrantDifferenceNotificationRequest {
                    delivery: ActorGrantDifferenceDelivery::ActorList,
                },
                PlayerActorLookupContext {
                    process: Some(host.process()),
                },
            )?;
            let menu = self.factory_shop_menu(factory, host)?;
            self.selector15_mut(factory)?.word_1b8 = 1;
            // 004137D0(0, menu): the unit (0046AE60) before the show, then
            // the broadcast from it.
            let unit = self.player_actor(player)?;
            self.show_player_menu(player, 0, menu, ShownMenuOwner::Shop { factory })?;
            if let Some(unit) = unit {
                let outcome = self.run_unit_broadcast(
                    UnitBroadcast {
                        player,
                        unit,
                        code: UnitBroadcastCode::MenuShown,
                    },
                    host,
                )?;
                if !outcome.skipped.is_empty() {
                    host.broadcast_skipped(&outcome.skipped);
                }
            }
        } else {
            host.text(self, player, ACCESS_DENIED_TEXT)
                .map_err(at(0x0042_f522))?;
            if team == ENEMY_TEAM {
                host.command(self, player, ENEMY_COMMAND, 0)
                    .map_err(at(0x0042_f531))?;
            }
            self.selector15_mut(factory)?.byte_1bd = 0;
        }
        Ok(())
    }

    /// `0042EEE9..0042F0D7`: a docked vehicle that strays beyond half the
    /// factory's radius or stops living hides the shop; otherwise it is
    /// pulled toward the dock point and the camera watches the factory.
    fn docked_factory<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        vehicle: ObjectId,
        microseconds: i32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let dock = self.dock_point(factory)?;
        let position = self.factory_position_words(vehicle)?;
        let offsets: [i32; 3] = std::array::from_fn(|axis| delta(dock[axis], position[axis]));
        let squares = offsets.iter().fold(0_i32, |sum, d| sum.wrapping_add(d * d)) as u32;
        let radius = u32::from(self.object(factory)?.base.word_28);
        if squares > radius.wrapping_mul(radius) >> 2 || (self.actor(vehicle)?.word_c4 as i32) < 3 {
            let handle = self.selector15(factory)?.handle_188;
            if let Some(player) = self.lookup_player(handle, host.process())? {
                self.hide_player_message(player)?;
            }
            return Ok(());
        }
        // 0042EF86..0042F034: each axis steps by (delta << 12) * offset >> 31
        // (the high DWORD of the product shifted left once), capped at 16.
        let scale = i64::from(microseconds.wrapping_shl(12));
        let pulled: [u16; 3] = std::array::from_fn(|axis| {
            let step = ((scale * i64::from(offsets[axis])) >> 31) as i32;
            position[axis].wrapping_add(step.min(PULL_LIMIT) as u16)
        });
        // 0047DDF0 writes the vehicle's +32..+36 and leaves its velocity.
        self.owned_mut(vehicle)?.state.base.position_words = Some(pulled);
        let camera = host.camera(self).map_err(at(0x0042_f054))?;
        let factory_handle = self.actor(factory)?.handle;
        let target =
            self.follow_target_resource(FollowTargetRequest::resolve_actor(factory_handle))?;
        self.replace_camera_target(camera, target, host.camera_settings())?;
        let state = self.selector16_mut(camera)?;
        state.mode_188 = DOCK_CAMERA_MODE;
        state.offset_194 = Some(DOCK_CAMERA_OFFSET);
        Ok(())
    }

    /// `004315E0`, a destroyed shop menu that no rebuild replaced: the
    /// factory's `+1B8` clears, then `0042F2A0` undocks it.
    pub fn destroy_shop_menu<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        self.selector15_mut(factory)?.word_1b8 = 0;
        self.undock_factory(factory, host)
    }

    /// `0042F2A0`, from the shop menu's destruction (`004315E0`, which has
    /// already cleared `+1B8`): the camera follows the vehicle again with
    /// the First Person setting, the dock and the player's `+DC` clear, and
    /// Facilities or Energy Distribution return to the Options.
    pub fn undock_factory<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let null = self.factory_null(factory, host.process())?;
        let camera = host.camera(self).map_err(at(0x0042_f2c2))?;
        let handle = self.selector15(factory)?.handle_188;
        let target = self.follow_target_resource(FollowTargetRequest::resolve_actor(handle))?;
        let settings = host.camera_settings();
        self.replace_camera_target(camera, target, settings)?;
        self.set_camera_view_mode(camera, u8::from(settings.first_person != 0))?;
        self.selector15_mut(factory)?.byte_1bd = 0;
        if let Some(player) = self.lookup_player(handle, host.process())? {
            self.world_player_mut(player)?.handle_dc = null;
        }
        let state = self.selector15_mut(factory)?;
        state.handle_188 = null;
        state.byte_1be = 0;
        if matches!(state.word_160, 1 | 2) {
            self.set_factory_state(factory, 0, false, host)?;
        }
        Ok(())
    }

    /// `00431CE0`: store the state; a bound menu (`+1B8`) is rebuilt by
    /// `00431D10` unless `force` is clear and the menu keeps its shape (the
    /// same state, or 4 and 5 into each other).
    pub fn set_factory_state<H: FactoryStaffHost>(
        &mut self,
        factory: ObjectId,
        state: u32,
        force: bool,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let factory_state = self.selector15_mut(factory)?;
        let old = factory_state.word_160;
        factory_state.word_160 = state;
        if factory_state.word_1b8 == 0 {
            return Ok(());
        }
        if !force {
            let same = if (4..=5).contains(&(state as i32)) {
                old == 4 || old == 5
            } else {
                state == old
            };
            if same {
                return Ok(());
            }
        }
        self.rebuild_factory_shop(factory, host)
    }

    /// `00431D10`'s rebuild: a new `00431000` menu replaces the bound one
    /// on the docked player's controller (`00453480`).
    fn rebuild_factory_shop<H: FactoryStaffHost>(
        &mut self,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let menu = self.factory_shop_menu(factory, host)?;
        let handle = self.selector15(factory)?.handle_188;
        let player =
            self.lookup_player(handle, host.process())?
                .ok_or(FactoryError::Unwritten {
                    factory,
                    field: "a docked player for the bound menu",
                })?;
        self.replace_player_menu(player, menu, ShownMenuOwner::Shop { factory })?;
        Ok(())
    }

    /// A new `00431000` menu over the factory's state.
    fn factory_shop_menu<H: FactoryStaffHost>(
        &mut self,
        factory: ObjectId,
        host: &H,
    ) -> Result<MenuState, FactoryError<H::Error>> {
        let tables = host.shop_tables().map_err(at(0x0043_1d74))?;
        let plan = self.plan_factory_shop(factory, tables, host.process())?;
        Ok(MenuState::new(plan.menu_data(tables), MenuBasis::INITIAL))
    }

    /// `0046D310`'s source: the first player's unit (`0046AE60`), whose
    /// `+64` view is the vehicle itself; its behavior `+140` holds the
    /// weapon entries. None when there is no unit.
    pub(super) fn first_unit_weapon_entries<E>(
        &mut self,
    ) -> Result<Option<Vec<[u32; 8]>>, FactoryError<E>> {
        let Some(player) = self.first_player() else {
            return Ok(None);
        };
        let Some(unit) = self.player_actor(player)? else {
            return Ok(None);
        };
        self.selector2_actor(unit)
            .map_err(|_| FactoryError::WeaponView(unit))?;
        let behavior = self
            .derived_actor(unit)?
            .behavior_140
            .as_ref()
            .ok_or(FactoryError::WeaponView(unit))?;
        Ok(Some(behavior.entries_18.clone()))
    }

    /// `00431000`'s inputs: the factory and, through `0046B090` of `+188`,
    /// the docked player.
    pub fn plan_factory_shop<E>(
        &mut self,
        factory: ObjectId,
        tables: &FactoryShopTables,
        process: &ParticleFactoryProcessState,
    ) -> Result<ShopMenuPlan, FactoryError<E>> {
        let player = self.factory_player(factory, process)?;
        let entries = match player {
            Some(_) => self.first_unit_weapon_entries()?,
            None => None,
        };
        let state = self.selector15(factory)?;
        let shop_factory = state.shop_factory();
        let shop_player = match player {
            Some(player) => {
                let owner = self.world_player(player)?;
                Some(ShopPlayer {
                    crystals_118: owner.dword_118 as i32,
                    tokens_120: owner.dwords_120_128.map(|count| count as i32),
                    upgrades_100: &owner.bits_100,
                    weapon_entries: entries.as_deref(),
                })
            }
            None => None,
        };
        Ok(crate::factory_shop::plan_shop_menu(
            tables,
            &shop_factory,
            shop_player.as_ref(),
        )?)
    }

    /// What the shop's row draws read: the factory and, through
    /// `0046B090` of `+188`, the docked player, with the first player's
    /// weapon stocks (`00431950`) and the clock `005471C0`.
    pub fn shop_draw_state<E>(
        &mut self,
        factory: ObjectId,
        process: &ParticleFactoryProcessState,
        clock: u32,
    ) -> Result<crate::factory_shop_draw::ShopDrawState, FactoryError<E>> {
        let player = self.factory_player(factory, process)?;
        let state = self.selector15(factory)?;
        let shop_factory = crate::factory_shop_draw::ShopDrawFactory {
            staff_13c: state.intermediate.word_13c,
            staff_140: state.intermediate.word_140,
            build_158: state.word_158,
            progress_16c: state.words_168_174[1],
            product_18c: state.product_18c,
        };
        let player = match player {
            Some(id) => {
                let weapon_entries = self.first_unit_weapon_entries()?;
                let owner = self.world_player(id)?;
                Some(crate::factory_shop_draw::ShopDrawPlayer {
                    id,
                    crystals_118: owner.dword_118 as i32,
                    tokens_120: owner.dwords_120_128.map(|count| count as i32),
                    levels_13c: owner.bytes_13c_143,
                    upgrades_100: owner.bits_100,
                    weapon_entries,
                })
            }
            None => None,
        };
        Ok(crate::factory_shop_draw::ShopDrawState {
            factory: shop_factory,
            player,
            clock,
        })
    }

    /// `0042F1D0`: the listed products, 0 without a docked player.
    pub fn factory_listed_products<E>(
        &self,
        factory: ObjectId,
        tables: &FactoryShopTables,
        process: &ParticleFactoryProcessState,
    ) -> Result<i32, FactoryError<E>> {
        let Some(player) = self.factory_player(factory, process)? else {
            return Ok(0);
        };
        let owner = self.world_player(player)?;
        let shop_player = ShopPlayer {
            crystals_118: owner.dword_118 as i32,
            tokens_120: owner.dwords_120_128.map(|count| count as i32),
            upgrades_100: &owner.bits_100,
            weapon_entries: None,
        };
        let shop_factory = self.selector15(factory)?.shop_factory();
        Ok(crate::factory_shop::listed_product_count(
            tables,
            &shop_factory,
            &shop_player,
        )?)
    }

    /// `00431E50`: the build time (seconds to microseconds), kind, ID and
    /// amount of the product; `reset` clears the progress `+16C`.
    pub fn start_factory_production(
        &mut self,
        factory: ObjectId,
        product: &FactoryProduct,
        reset: bool,
    ) -> Result<(), ObjectError> {
        let state = self.selector15_mut(factory)?;
        state.word_158 = Some(u32::from(product.build_seconds).wrapping_mul(1_000_000));
        state.word_184 = product.kind;
        state.product_18c = Some(product.id);
        state.amount_18e = Some(u16::from(product.amount));
        if reset {
            state.words_168_174[1] = 0;
        }
        Ok(())
    }

    fn production_words<E>(&mut self, factory: ObjectId) -> Result<&mut [u16; 6], FactoryError<E>> {
        self.selector15_mut(factory)?
            .words_178_182
            .as_mut()
            .ok_or(FactoryError::Unwritten {
                factory,
                field: "model words +178",
            })
    }

    /// `0042E570`.
    fn run_factory_production<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        microseconds: i32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let delta_micros = microseconds as u32;
        let state = self.selector15(factory)?;
        let short_staffed = state.intermediate.word_13c != state.intermediate.word_140;
        let handle = state.handle_188;
        let player = self.lookup_player(handle, host.process())?;
        // A full factory repairs its docked player's unit: 0046B2B0(4,
        // delta / 2000) while the unit's damage fraction is nonzero.
        if let (false, Some(player)) = (short_staffed, player) {
            let unit = self
                .player_actor(player)?
                .ok_or(ObjectError::MissingActorRegistry)?;
            let actor = self.actor(unit)?;
            let maximum = u32::from(actor.maximum_health.ok_or(FactoryError::Unwritten {
                factory,
                field: "unit maximum health +A2",
            })?);
            let damage = (maximum.wrapping_sub(u32::from(actor.health)) << 16) as i32;
            if (damage / (maximum as i32 + 1)) as u16 != 0 {
                host.grant(self, player, REPAIR_ID, (delta_micros / 2000) as i16)
                    .map_err(at(0x0042_e5dc))?;
            }
        }
        let mut sound = BUSY_SOUND;
        match self.selector15(factory)?.word_160 {
            3 => {
                if short_staffed {
                    self.set_factory_state(factory, 0, false, host)?;
                }
                let state = self.selector15(factory)?;
                let limit = state.word_158.ok_or(FactoryError::Unwritten {
                    factory,
                    field: "build time +158",
                })?;
                let progress_16c = state.words_168_174[1];
                if (progress_16c as i32) < limit as i32 || limit == 0 {
                    let advanced = progress_16c.wrapping_add(delta_micros);
                    self.selector15_mut(factory)?.words_168_174[1] = advanced;
                    if advanced as i32 >= limit as i32 {
                        self.selector15_mut(factory)?.words_168_174[1] = limit;
                        self.set_factory_state(factory, 4, false, host)?;
                    }
                    let state = self.selector15(factory)?;
                    let (value, limit) = (state.words_168_174[1], state.word_158.unwrap_or(0));
                    self.production_words(factory)?[3] = if value >= limit {
                        0xffff
                    } else {
                        progress(value, limit)
                    };
                }
            }
            4 => {
                let fitting = self.selector15(factory)?.words_168_174[2];
                if (fitting as i32) < PHASE_MICROS as i32 {
                    let null = self.factory_null(factory, host.process())?;
                    if handle == null {
                        self.selector15_mut(factory)?.words_168_174[2] =
                            if (fitting as i32) > microseconds {
                                fitting.wrapping_sub(delta_micros)
                            } else {
                                0
                            };
                    } else {
                        self.factory_fitting(factory, player, delta_micros, host)?;
                    }
                    let fitting = self.selector15(factory)?.words_168_174[2];
                    self.production_words(factory)?[4] = if fitting >= PHASE_MICROS {
                        0xffff
                    } else {
                        progress(fitting, PHASE_MICROS)
                    };
                }
            }
            5 => self.factory_grant_phase(factory, player, delta_micros, host)?,
            6 => {
                let recovery = self.selector15(factory)?.words_168_174[0];
                if microseconds < recovery as i32 {
                    let remaining = recovery.wrapping_sub(delta_micros);
                    self.selector15_mut(factory)?.words_168_174[0] = remaining;
                    let word = if remaining >= PHASE_MICROS {
                        0xffff
                    } else {
                        progress(remaining, PHASE_MICROS)
                    };
                    let words = self.production_words(factory)?;
                    words[3] = word;
                    words[4] = word;
                } else {
                    self.selector15_mut(factory)?.words_168_174[0] = 0;
                    let tables = host.shop_tables().map_err(at(0x0042_e570))?;
                    let listed = self.factory_listed_products(factory, tables, host.process())?;
                    self.set_factory_state(factory, u32::from(listed > 0), false, host)?;
                    let words = self.production_words(factory)?;
                    words[3] = 0;
                    words[4] = 0;
                }
            }
            _ => sound = IDLE_SOUND,
        }
        self.factory_loop_sound(factory, sound, host)
    }

    /// State 4 while docked: the fitting `+170` charges for 2 s. When it is
    /// full, `+174` restarts, a non-upgrade product whose hold has too few
    /// free slots (`004120E0`) is announced, and the state becomes 5.
    fn factory_fitting<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        player: Option<PlayerId>,
        delta_micros: u32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        // 0042E6E8..0042E6F1: the vehicle's virtual +64, its own view.
        let handle = self.selector15(factory)?.handle_188;
        let vehicle = self
            .resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?
            .ok_or(ObjectError::MissingActorRegistry)?;
        let state = self.selector15_mut(factory)?;
        let fitting = state.words_168_174[2].wrapping_add(delta_micros);
        state.words_168_174[2] = fitting;
        if (fitting as i32) < PHASE_MICROS as i32 {
            return Ok(());
        }
        state.words_168_174[2] = PHASE_MICROS;
        state.words_168_174[3] = 0;
        let kind = state.word_184;
        if let (true, Some(player)) = (kind != UPGRADE_KIND, player) {
            let amount = self
                .selector15(factory)?
                .amount_18e
                .ok_or(FactoryError::Unwritten {
                    factory,
                    field: "amount +18E",
                })?;
            let null = self.factory_null(factory, host.process())?;
            let free = self.vehicle_free_carry_slots(vehicle, null)?;
            if (free as i32) < i32::from(amount as i16) {
                host.text(self, player, HOLD_FULL_TEXT)
                    .map_err(at(0x0042_e783))?;
            }
        }
        self.set_factory_state(factory, 5, false, host)?;
        self.selector15_mut(factory)?.byte_1bc = 0;
        Ok(())
    }

    /// `004120E0` on the vehicle's carry slots `+1A4`: the slots holding
    /// the process NULL.
    fn vehicle_free_carry_slots<E>(
        &self,
        vehicle: ObjectId,
        null: ActorHandle,
    ) -> Result<usize, FactoryError<E>> {
        let state = self.selector2_actor(vehicle)?;
        Ok(state
            .handles_1a4
            .handles
            .iter()
            .filter(|&&handle| handle == null)
            .count())
    }

    /// State 5: without a vehicle the fitting returns to state 4 (its
    /// timer one short); `+174` counts 2 s and then grants the product,
    /// an upgrade through `0046B2B0` (sound 0x30 when new), else items
    /// ([`Self::factory_items`]). Past that, the
    /// recovery starts: state 6 with `+168` at 2 s, and the shop is hidden
    /// when nothing is left to list.
    fn factory_grant_phase<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        player: Option<PlayerId>,
        delta_micros: u32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let null = self.factory_null(factory, host.process())?;
        if self.selector15(factory)?.handle_188 == null {
            self.set_factory_state(factory, 4, false, host)?;
            let state = self.selector15_mut(factory)?;
            state.words_168_174[2] = state.words_168_174[2].wrapping_sub(1);
        }
        let grant = self.selector15(factory)?.words_168_174[3];
        if grant as i32 <= PHASE_MICROS as i32 {
            let advanced = grant.wrapping_add(delta_micros);
            self.selector15_mut(factory)?.words_168_174[3] = advanced;
            if advanced as i32 <= PHASE_MICROS as i32 {
                return Ok(());
            }
            let Some(player) = player else {
                self.selector15_mut(factory)?.word_184 = 0;
                return Ok(());
            };
            let state = self.selector15(factory)?;
            if state.word_184 != UPGRADE_KIND {
                self.factory_items(factory, player, host)?;
                self.selector15_mut(factory)?.word_184 = 0;
                return Ok(());
            }
            let unwritten = |field| FactoryError::Unwritten { factory, field };
            let id = state.product_18c.ok_or_else(|| unwritten("product +18C"))?;
            let amount = state.amount_18e.ok_or_else(|| unwritten("amount +18E"))?;
            if !self.player_upgrade_owned(player, u32::from(id))? {
                let position = self.factory_position_words(factory)?;
                host.sound(self, NEW_UPGRADE_SOUND, position, 0x10000, 0x10000)
                    .map_err(at(0x0042_e86c))?;
            }
            host.grant(self, player, id, amount as i16)
                .map_err(at(0x0042_e88a))?;
            self.selector15_mut(factory)?.word_184 = 0;
            return Ok(());
        }
        if self.selector15(factory)?.byte_1bc != 0 {
            self.selector15_mut(factory)?.words_168_174[1] = 0;
            if let Some(player) = player {
                self.hide_player_message(player)?;
            }
        }
        let state = self.selector15_mut(factory)?;
        state.words_168_174 = [PHASE_MICROS, 0, 0, state.words_168_174[3]];
        self.set_factory_state(factory, 6, false, host)?;
        self.selector15_mut(factory)?.words_168_174[3] = 0;
        let tables = host.shop_tables().map_err(at(0x0042_e570))?;
        let listed = self.factory_listed_products(factory, tables, host.process())?;
        if let (0, Some(player)) = (listed, player) {
            self.hide_player_message(player)?;
        }
        Ok(())
    }

    /// `0042E89E`: `+18E` items of resource `+184`, reread for each, from
    /// the factory's `+4C` at the dock point, for the player's unit
    /// (`0046AE60`). When its vehicle view (`+64`) has fewer free slots
    /// than that, each point moves back along the factory's forward row
    /// `+60..+68` by 200 to 327. A built item is dirty, joins team 0 and
    /// goes into the vehicle's hold; the Remote Beamer's next item is its
    /// receiver.
    fn factory_items<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        player: PlayerId,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let Some(unit) = self.player_actor(player)? else {
            return Ok(());
        };
        let amount = self
            .selector15(factory)?
            .amount_18e
            .ok_or(FactoryError::Unwritten {
                factory,
                field: "amount +18E",
            })? as i16;
        let vehicle = self.actor_is_vehicle(unit)?.then_some(unit);
        let null = self.factory_null(factory, host.process())?;
        for _ in 0..amount {
            let mut position = self.dock_point(factory)?;
            if let Some(vehicle) = vehicle {
                if (self.vehicle_free_carry_slots(vehicle, null)? as i32) < i32::from(amount) {
                    let draw = host.random().map_err(at(0x0042_e8fa))?;
                    let distance = f64::from(((draw & 0xffff) >> 9) + 200);
                    let forward = &self.actor(factory)?.rotating.basis.words[7..];
                    for (word, &row) in position.iter_mut().zip(forward) {
                        // 00511BA8 truncates; the WORD subtraction keeps the low half.
                        let step = (distance * f64::from(f32::from_bits(row))) as i64;
                        *word = word.wrapping_sub(step as u16);
                    }
                }
            }
            // 0042E93F loads the WORD into CX over ECX's earlier value.
            let resource =
                ActorChildResource::RegisterLowWord(self.selector15(factory)?.word_184 as u16);
            let Some(item) = host
                .create_item(self, factory, resource, position)
                .map_err(at(0x0042_e955))?
            else {
                continue;
            };
            let state = self.selector15_mut(factory)?;
            if state.word_184 == BEAMER_RESOURCE {
                state.word_184 = RECEIVER_RESOURCE;
            }
            self.owned_mut(item)?.state.base.dirty_word |= 2;
            self.actor_mut(item)?.word_b4 = 0;
            if let Some(vehicle) = vehicle {
                host.carry_scientist(self, vehicle, item)
                    .map_err(at(0x0042_e996))?;
            }
        }
        Ok(())
    }

    /// `0042EAD8..0042EC46`: within 3000 units of the first player's unit
    /// the looped sound `+154` plays `sound` (restarted when it changes,
    /// remembered at `+1C0`) at a volume falling to zero at the range.
    fn factory_loop_sound<H: FactoryHost>(
        &mut self,
        factory: ObjectId,
        sound: u32,
        host: &mut H,
    ) -> Result<(), FactoryError<H::Error>> {
        let unit = match self.first_player() {
            Some(player) => self.player_actor(player)?,
            None => None,
        };
        let audible = match unit {
            Some(unit) => {
                let [ux, uy, uz] = self.factory_position_words(unit)?;
                let [fx, fy, fz] = self.factory_position_words(factory)?;
                let (dx, dy, dz) = (delta(ux, fx), delta(uy, fy), delta(uz, fz));
                let squares = (dx * dx).wrapping_add(dy * dy).wrapping_add(dz * dz) as u32;
                let range = distance(squares) as i32;
                (range < SOUND_RANGE).then_some(range)
            }
            None => None,
        };
        let Some(range) = audible else {
            if let Some(reference) = self.factory_sound_mut(factory)?.take() {
                host.release_loop(reference).map_err(at(0x0042_ec3b))?;
            }
            return Ok(());
        };
        let volume = ((SOUND_RANGE - range) * 0xc000) / SOUND_RANGE;
        let live = self.selector15(factory)?.intermediate.sound_154;
        if live.is_none() || self.selector15(factory)?.word_1c0 != sound {
            if let Some(reference) = self.factory_sound_mut(factory)?.take() {
                host.release_loop(reference).map_err(at(0x0042_eb8e))?;
            }
            let created = host
                .create_loop(self, factory, sound)
                .map_err(at(0x0042_ebe9))?;
            let state = self.selector15_mut(factory)?;
            state.intermediate.sound_154 = created;
            state.word_1c0 = sound;
        }
        if let Some(reference) = self.selector15(factory)?.intermediate.sound_154 {
            host.update_loop(self, factory, reference, volume as u32)
                .map_err(at(0x0042_ec08))?;
        }
        Ok(())
    }

    fn factory_sound_mut(
        &mut self,
        factory: ObjectId,
    ) -> Result<&mut Option<ProcessSoundRef>, ObjectError> {
        Ok(&mut self.selector15_mut(factory)?.intermediate.sound_154)
    }
}

impl Selector15State {
    /// `0042F1D0` reads the factory's listing inputs.
    pub fn shop_factory(&self) -> crate::factory_shop::ShopFactory<'_> {
        crate::factory_shop::ShopFactory {
            state_160: self.word_160,
            kind_15c: self.word_15c,
            staff_13c: self.intermediate.word_13c,
            staff_140: self.intermediate.word_140,
            grants_198: &self.grants_198.mask_04,
        }
    }
}

#[cfg(test)]
#[path = "factory_tests.rs"]
mod tests;
