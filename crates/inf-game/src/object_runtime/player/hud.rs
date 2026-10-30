//! The players' HUD pass that ends the gameplay draw (`00440F00`,
//! `00441456..004414E5`). Each player's controlled unit (`0046AE60`) draws
//! through its virtual +84. The vehicle's (`0041ED40`) draws its own HUD and
//! then its player's (`0046BE00`), which begins with the message draw
//! (`00413820`).
//!
//! Native walks the session's player list (screen +E4, list +C4); the port
//! walks the arena's players, the one local player of every admitted world.

use thiserror::Error;

use super::{
    PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest, PlayerAttachError,
    PlayerId,
};
use crate::free_look_view::{FreeLookActor, FreeLookTarget};
use crate::object_runtime::{NativeTarget, ObjectArena, ObjectError, ObjectId, VehicleDriveError};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::vehicle_hud::{
    HudAnnotation, HudPlayer, HudTarget, HudTargetActor, HudVehicle, HudWeapons,
};

/// The settings and view the reticle reads (`0041C92E..0041D065`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudReticleSettings {
    /// Settings +60, "HUD".
    pub hud: u32,
    /// Settings +7C, which the target's refresh reads.
    pub settings_7c: u32,
    /// World-manager byte G+5C5, the special view.
    pub special_view: bool,
}

/// What one player's controlled unit draws through virtual +84.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerHudUnit {
    /// `0041ED40`. Only when the vehicle's +84 handle names a player
    /// (`0046B090`) and the vehicle is in the draw list (+2C bit 0) does it
    /// draw its own HUD (`0041B230` when +1C0 is set, else `0041C320`) and
    /// then that player's `0046BE00`.
    Vehicle {
        vehicle: ObjectId,
        owner: Option<PlayerId>,
        drawable: bool,
        alternate_hud: bool,
    },
    /// The shared actor view `0047F690` of the camera (the player holds it
    /// after the vehicle's death) or of a creature the Cranial Implanter
    /// took (the family `004236D0`'s classes and selector 12): the unit's
    /// player (`0046B090` of +84), if any, gets its `0046BE00` and no
    /// vehicle HUD.
    Camera {
        camera: ObjectId,
        owner: Option<PlayerId>,
    },
    /// The missiles' `00456CF0` (selector 68's `00458F30` calls it): the
    /// guided view, [`crate::missile_view`]. `owner` is the player holding
    /// it (`0046B090` of +84).
    Missile {
        missile: ObjectId,
        owner: Option<PlayerId>,
    },
    /// Another class's virtual +84.
    Other(ObjectId),
}

/// What the guided view `00456CF0` reads of the missile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissileViewSource {
    /// BYTE +150.
    pub disrupted: bool,
    /// +94, +98 and +9C.
    pub rates: [i32; 3],
    pub position: [u16; 3],
    /// +8E..+92.
    pub velocity: [i16; 3],
    /// WORD +13A.
    pub fuse: u16,
}

#[derive(Debug, Error)]
pub enum PlayerHudError {
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Lookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Vehicle(#[from] crate::object_runtime::DerivedActorError),
    #[error("vehicle {0:?} has no written maximum health +A2 for the HUD")]
    MaximumHealth(ObjectId),
    #[error("vehicle {0:?} has no behavior +140 for the HUD")]
    Behavior(ObjectId),
    #[error("vehicle {vehicle:?}'s behavior has no entry {index} (+18) for the HUD")]
    WeaponEntry { vehicle: ObjectId, index: i8 },
    #[error("vehicle {vehicle:?}'s behavior has no +26C table record {index} for 0048D7D0")]
    WeaponRecord { vehicle: ObjectId, index: i8 },
    #[error("vehicle {0:?}'s carry slot handle does not resolve")]
    CarrySlot(ObjectId),
    #[error("map annotation {0:?} has no position for the radar")]
    AnnotationPosition(ObjectId),
    #[error("the sight's fire direction (0041FF20) stopped: {0}")]
    Sight(#[from] VehicleDriveError<std::convert::Infallible>),
    #[error("free look's target actor {0:?} has no written name +76")]
    TargetName(ObjectId),
    #[error("free look's target {0:?} has resource 0xF but no gate's destination +15C")]
    GateDestination(ObjectId),
    #[error("free look's furniture target never wrote its cell +08")]
    FurnitureCell,
}

/// What free look's view (`0041B230`) reads of the vehicle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeLookSource {
    /// +B4 and +5C0.
    pub side: u32,
    pub dword_5c0: u32,
    pub target: FreeLookPick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeLookPick {
    Target(FreeLookTarget),
    /// Type 4's WORDs +08 and +0C, which `00436BC0` looks up.
    FurnitureCell([u16; 2]),
}

impl ObjectArena {
    /// What the reticle reads once no menu is shown and the HUD setting is
    /// on (`0041C92E`): the target +148, refreshed (virtual +18) before its
    /// type +10 is read, its focus (virtual +1C) and type 2's actor (virtual
    /// +10, `004827C0`; the other types return NULL) unless that actor is
    /// the vehicle; then, when no box is drawn, `0041FF20`'s fire direction,
    /// which writes the behavior's lead +20 and under the special view frees
    /// the target.
    fn vehicle_hud_reticle(
        &mut self,
        vehicle: ObjectId,
        player: PlayerId,
        reticle: HudReticleSettings,
    ) -> Result<(Option<HudTarget>, Option<[i16; 3]>), PlayerHudError> {
        if self.world_player(player)?.message_byte_00() != 0 || reticle.hud == 0 {
            return Ok((None, None));
        }
        let mut target = None;
        if let Some(reference) = self.derived_actor(vehicle)?.target_148 {
            let mut native = self.native_target(reference)?;
            let refreshed = self.refresh_native_target(&mut native, reticle.settings_7c);
            self.write_native_target(reference, native)?;
            refreshed?;
            let kind = native.kind_word();
            let mut actor = None;
            if let (2, NativeTarget::ActorFollow(follow)) = (kind, native) {
                let resolved = self
                    .resolve_actor_handle(follow.handle_04)
                    .map_err(|_| ObjectError::MissingActorRegistry)?;
                if let Some(object) = resolved {
                    let framed = self.actor(object)?;
                    if framed.handle != self.actor(vehicle)?.handle {
                        actor = Some(HudTargetActor {
                            health: framed.health,
                            maximum_health: framed.maximum_health,
                            team: framed.word_b4,
                        });
                    }
                }
            }
            target = Some(HudTarget {
                kind,
                focus: native.point_words(),
                actor,
            });
        }
        if matches!(target, Some(HudTarget { kind: 2 | 4, .. })) {
            return Ok((target, None));
        }
        let direction =
            self.vehicle_sight_direction(vehicle, reticle.special_view, reticle.settings_7c)?;
        Ok((target, direction))
    }

    /// What free look reads (`0041B56B..0041B5C5`, `0041BA99`): the target
    /// +148 refreshed (virtual +18), then by its type +10 the actor (virtual
    /// +10) or the furniture cell. Unlike the reticle it reads the target
    /// whatever the HUD setting and the message.
    pub fn free_look_source(
        &mut self,
        vehicle: ObjectId,
        settings_7c: u32,
    ) -> Result<FreeLookSource, PlayerHudError> {
        let target = match self.derived_actor(vehicle)?.target_148 {
            None => FreeLookPick::Target(FreeLookTarget::Absent),
            Some(reference) => {
                let mut native = self.native_target(reference)?;
                let refreshed = self.refresh_native_target(&mut native, settings_7c);
                self.write_native_target(reference, native)?;
                refreshed?;
                match (native.kind_word(), native) {
                    (2, NativeTarget::ActorFollow(follow)) => {
                        FreeLookPick::Target(self.free_look_actor(vehicle, follow.handle_04)?)
                    }
                    (2, _) => FreeLookPick::Target(FreeLookTarget::Hidden),
                    (4, _) => {
                        let [x, _, z] = native
                            .raw_focus_08_words()
                            .ok_or(PlayerHudError::FurnitureCell)?;
                        FreeLookPick::FurnitureCell([x, z])
                    }
                    _ => FreeLookPick::Target(FreeLookTarget::Absent),
                }
            }
        };
        let actor = self.actor(vehicle)?;
        Ok(FreeLookSource {
            side: actor.word_b4,
            dword_5c0: self.selector2_actor(vehicle)?.dword_5c0,
            target,
        })
    }

    fn free_look_actor(
        &mut self,
        vehicle: ObjectId,
        handle: crate::object_runtime::ActorHandle,
    ) -> Result<FreeLookTarget, PlayerHudError> {
        let resolved = self
            .resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        let Some(object) = resolved else {
            return Ok(FreeLookTarget::Hidden);
        };
        let framed = self.actor(object)?;
        if framed.handle == self.actor(vehicle)?.handle {
            return Ok(FreeLookTarget::Hidden);
        }
        let name = framed.word_76.ok_or(PlayerHudError::TargetName(object))?;
        if name == crate::free_look_view::HIDDEN_NAME {
            return Ok(FreeLookTarget::Hidden);
        }
        // 0041B95B: a gate (resource 0xF) names its destination +15C.
        let gate = match self.object(object)?.base.resource_word {
            0xf => match &framed.extension {
                crate::object_runtime::ActorExtension::Selector30(gate) => Some(gate.word_15c),
                _ => return Err(PlayerHudError::GateDestination(object)),
            },
            _ => None,
        };
        Ok(FreeLookTarget::Actor(FreeLookActor {
            name,
            health: framed.health,
            maximum_health: framed.maximum_health,
            side: framed.word_b4,
            gate,
        }))
    }

    /// The pass's units in player order. A player without a unit draws
    /// nothing.
    pub fn player_hud_units(
        &mut self,
        process: Option<&ParticleFactoryProcessState>,
    ) -> Result<Vec<(PlayerId, PlayerHudUnit)>, PlayerHudError> {
        let players: Vec<PlayerId> = self.players.list.iter().map(|player| player.id).collect();
        let mut units = Vec::new();
        for player in players {
            let Some(unit) = self.player_actor(player)? else {
                continue;
            };
            let entry = match self.selector2_actor(unit) {
                Ok(vehicle) => {
                    let alternate_hud = vehicle.byte_1c0 != 0;
                    let handle = self.actor(unit)?.handle;
                    let owner = self.find_player_for_actor_handle(
                        PlayerActorLookupRequest { handle },
                        PlayerActorLookupContext { process },
                    )?;
                    PlayerHudUnit::Vehicle {
                        vehicle: unit,
                        owner,
                        drawable: self.object(unit)?.base.flags & 1 != 0,
                        alternate_hud,
                    }
                }
                Err(_) if self.is_missile(unit) => {
                    let handle = self.actor(unit)?.handle;
                    let owner = self.find_player_for_actor_handle(
                        PlayerActorLookupRequest { handle },
                        PlayerActorLookupContext { process },
                    )?;
                    PlayerHudUnit::Missile {
                        missile: unit,
                        owner,
                    }
                }
                Err(_)
                    if self.selector16(unit).is_ok()
                        || self.family4236d0_actor(unit).is_ok()
                        || self.selector12_actor(unit).is_ok() =>
                {
                    let handle = self.actor(unit)?.handle;
                    let owner = self.find_player_for_actor_handle(
                        PlayerActorLookupRequest { handle },
                        PlayerActorLookupContext { process },
                    )?;
                    PlayerHudUnit::Camera {
                        camera: unit,
                        owner,
                    }
                }
                Err(_) => PlayerHudUnit::Other(unit),
            };
            units.push((player, entry));
        }
        Ok(units)
    }
}

impl ObjectArena {
    /// The missile's fields `00456CF0` reads.
    pub fn missile_view_source(&self, missile: ObjectId) -> Result<MissileViewSource, ObjectError> {
        let (disrupted, fuse) = self.missile_view_state(missile)?;
        let actor = self.actor(missile)?;
        Ok(MissileViewSource {
            disrupted,
            rates: actor.words_94_9c.map(|word| word as i32),
            position: self
                .object(missile)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(missile))?,
            velocity: actor.words_8e_92.map(|word| word as i16),
            fuse,
        })
    }

    /// What `0041C320` reads from the vehicle, its behavior +140 and its
    /// player (`0046B090` of +84).
    pub fn vehicle_hud_sources(
        &mut self,
        vehicle: ObjectId,
        player: PlayerId,
        reticle: HudReticleSettings,
    ) -> Result<(HudVehicle, HudPlayer), PlayerHudError> {
        let (target, sight_direction) = self.vehicle_hud_reticle(vehicle, player, reticle)?;
        let base = &self.object(vehicle)?.base;
        let drawn = base.flags & 2 != 0;
        let position = base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
        let actor = self.actor(vehicle)?;
        let health = actor.health;
        let team = actor.word_b4;
        let maximum_health = actor
            .maximum_health
            .ok_or(PlayerHudError::MaximumHealth(vehicle))?;
        let derived = self.derived_actor(vehicle)?;
        let behavior = derived
            .behavior_140
            .as_ref()
            .ok_or(PlayerHudError::Behavior(vehicle))?;
        let entry = |index: i8| {
            usize::try_from(index)
                .ok()
                .and_then(|slot| behavior.entries_18.get(slot))
                .ok_or(PlayerHudError::WeaponEntry { vehicle, index })
        };
        let (primary, secondary) = (behavior.byte_10 as i8, behavior.byte_11 as i8);
        let primary_entry = *entry(primary)?;
        let secondary_ammo = entry(secondary)?[0] as i32;
        let state = self.selector2_actor(vehicle)?;
        // 0048D7D0: the selected record's +1C in the +26C copy, then the
        // entry's +1C timer as a percent of six seconds.
        let record = usize::try_from(primary)
            .ok()
            .and_then(|slot| {
                state
                    .table_26c
                    .as_ref()?
                    .get(slot * inf_formats::VEHICLE_BEHAVIOR_RECORD_DWORDS + 7)
            })
            .copied()
            .ok_or(PlayerHudError::WeaponRecord {
                vehicle,
                index: primary,
            })?;
        let primary_charge = if record == 0 {
            -1
        } else {
            let timer = primary_entry[7] as i32;
            if timer > 6_000_000 {
                100
            } else {
                timer / 60_000
            }
        };
        let handles = state.handles_1a4.handles.clone();
        // 0041AA38: the switch panels count each weapon's entry +0.
        let mut ammo = [0; 16];
        for (count, entry) in ammo.iter_mut().zip(&behavior.entries_18) {
            *count = entry[0] as i32;
        }
        let basis_words = self.actor(vehicle)?.rotating.basis.words;
        let hud_vehicle = HudVehicle {
            drawn,
            health,
            maximum_health,
            mode: state.dwords_1e8_1f4[0] as u16,
            target,
            corners: state.corners_5b4,
            position,
            basis_bits: std::array::from_fn(|row| {
                std::array::from_fn(|i| basis_words[1 + row * 3 + i])
            }),
            team,
            sight_direction,
            carried: [None; 3],
            dword_218: state.dwords_210_218[2],
            switching: state.bytes_534_535,
            switch_timers: state.dwords_52c_530,
            switch_slides: [state.dwords_538_554[2], state.dwords_538_554[3]],
            weapons: HudWeapons {
                primary,
                secondary,
                primary_ammo: primary_entry[0] as i32,
                secondary_ammo,
                primary_charge,
                ammo,
            },
            notices: [state.bytes_20a_20c[0], state.bytes_20a_20c[1]],
            dword_5c0: state.dword_5c0,
        };
        // 00412780 resolves each of the +1A8 slots.
        let mut carried = [None; 3];
        if handles.len() > carried.len() {
            return Err(PlayerHudError::CarrySlot(vehicle));
        }
        for (slot, handle) in carried.iter_mut().zip(handles) {
            if let Some(actor) = self
                .resolve_actor_handle(handle)
                .map_err(|_| PlayerHudError::CarrySlot(vehicle))?
            {
                *slot = Some(crate::vehicle_hud::HudCarriedActor {
                    actor,
                    size_28: self.object(actor)?.base.word_28,
                });
            }
        }
        let mut weapons_owned = [false; 16];
        for (owned, &id) in weapons_owned
            .iter_mut()
            .zip(&crate::vehicle_hud::WEAPON_UPGRADES)
        {
            *owned = self.player_upgrade_owned(player, u32::from(id))?;
        }
        let owner = self.world_player(player)?;
        Ok((
            HudVehicle {
                carried,
                ..hud_vehicle
            },
            HudPlayer {
                message_shown: owner.message_byte_00(),
                milestones: owner.word_11c,
                counters: owner.dwords_120_128,
                turbo: owner.dword_12c,
                radar_upgrades: [
                    self.player_upgrade_owned(player, 0x11)?,
                    self.player_upgrade_owned(player, 0x12)?,
                    self.player_upgrade_owned(player, 0x13)?,
                ],
                weapons_owned,
            },
        ))
    }

    /// The `0055F170` list's annotations (`0048C6F0`) with their positions
    /// and labels.
    pub fn hud_annotations(&self) -> Result<Vec<HudAnnotation>, PlayerHudError> {
        let mut annotations = Vec::new();
        for &id in self.map_annotation_list() {
            let words = self
                .object(id)?
                .base
                .position_words
                .ok_or(PlayerHudError::AnnotationPosition(id))?;
            annotations.push(HudAnnotation {
                position_words: [words[0], words[2]],
                label: self.map_annotation(id)?.label,
            });
        }
        Ok(annotations)
    }

    /// The actor list `G+48` in order, for the radar's walk: each actor's
    /// flags, X/Z, team and the class answering `+64` (the vehicle) or
    /// `+70` (a pickup spawner). The walk reads a position only for an
    /// actor it admits, and fails on an admitted actor without one.
    pub fn hud_radar_actors(&self) -> Result<Vec<crate::radar_blips::RadarActor>, PlayerHudError> {
        use crate::radar_blips::{RadarActor, RadarActorKind};
        let mut actors = Vec::new();
        for &id in self.actor_list() {
            let map_actor = crate::briefing_markers::BriefingMapActor::from_arena(self, id)?;
            let kind = if let Ok(spawner) = self.selector53(id) {
                RadarActorKind::Spawner {
                    kind: spawner.kind_138 as i8,
                    count: spawner.count_139 as i8,
                }
            } else if self.selector2_actor(id).is_ok() {
                RadarActorKind::Vehicle
            } else {
                RadarActorKind::Other
            };
            actors.push(RadarActor {
                flags: map_actor.flags,
                position_words: map_actor.position_words.map(|words| [words[0], words[2]]),
                team: map_actor.material_word_b4,
                kind,
            });
        }
        Ok(actors)
    }

    /// `0041CDCC`/`0041CE49`: the reticle countdown the HUD raises.
    pub fn set_vehicle_hud_5c0(
        &mut self,
        vehicle: ObjectId,
        value: u32,
    ) -> Result<(), PlayerHudError> {
        self.selector2_actor_mut(vehicle)?.dword_5c0 = value;
        Ok(())
    }

    /// The reticle's eased corners +5B4..+5BF.
    pub fn set_vehicle_hud_corners(
        &mut self,
        vehicle: ObjectId,
        corners: [[u16; 2]; 3],
    ) -> Result<(), PlayerHudError> {
        self.selector2_actor_mut(vehicle)?.corners_5b4 = corners;
        Ok(())
    }

    /// `0041EC87`/`0041ED19`: a drawn notice clears +20A or +20B.
    pub fn clear_vehicle_hud_notice(
        &mut self,
        vehicle: ObjectId,
        index: usize,
    ) -> Result<(), PlayerHudError> {
        self.selector2_actor_mut(vehicle)?.bytes_20a_20c[index] = 0;
        Ok(())
    }

    /// The camera's +188 for `00441499..004414BE`: whether the HUD takes
    /// the "First Person" layout.
    pub fn camera_first_person(&mut self) -> Result<bool, PlayerHudError> {
        let Some(handle) = self.camera_handle() else {
            return Ok(false);
        };
        let Some(camera) = self
            .resolve_actor_handle(handle)
            .map_err(|_| ObjectError::MissingActorRegistry)?
        else {
            return Ok(false);
        };
        Ok(self.selector16(camera)?.mode_188 == 2)
    }

    /// [`Self::camera_first_person`] without the handles' lookup cache, for
    /// a reader that leaves the game's state alone (the port's ambience).
    pub fn camera_first_person_peek(&self) -> bool {
        let camera = self
            .camera_handle()
            .and_then(|handle| self.actor_handles()?.peek(handle));
        camera.is_some_and(|camera| {
            self.selector16(camera)
                .is_ok_and(|camera| camera.mode_188 == 2)
        })
    }
}

#[cfg(test)]
#[path = "hud_tests.rs"]
mod tests;
