//! Shared Flag0 lighting pass, complete 00479500..0047956B.
//!
//! The supplied current settings/spacing/tint owner is read at entry and again
//! only when a callback reaches 0045B1A0. The same embedded node moves before
//! fresh flags and canonical +28 dispatch; only drained completion exchanges
//! physical header owners. This pass neither clears nor reanchors the grid.
//! Raw replaced/NULL node storage and arbitrary virtual overrides are outside
//! the typed live-owner domain. Active vehicle callbacks remain explicit
//! boundaries; interactive omission preserves their reached prefix and is
//! named separately from strict source completion.

use thiserror::Error;

use super::{
    mark_flag_change, ActorExtension, DerivedActorClass, ObjectArena, ObjectClass, ObjectError,
    ObjectId, ObjectList, ObjectNode, ObjectNodeKind,
};
use crate::world_lighting::{
    TerrainLightError, TerrainLightOutcome, TerrainLightPolicy, TerrainLightRequest, WorldLightGrid,
};

#[cfg(test)]
mod tests;
mod vehicle;
pub use vehicle::{RearLamps, VehicleLamp};

/// Borrow the actual current policy owner. The two native settings reads are
/// distinct phases; a caller must not turn the entry result into deposit policy.
/// A supplied immutable settings loan excludes intervening alias mutation.
pub trait ObjectLightPolicySource {
    fn dynamic_lighting(&mut self) -> bool;
    fn terrain_light_policy(&mut self) -> TerrainLightPolicy;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag0LightFailurePolicy {
    Stop,
    /// Skip only the unowned current callback remainder, collect its reason,
    /// and continue the fresh-head pass. Native grid/custody errors still stop.
    UnownedObjectLightCallbackApproximation,
}

pub struct Flag0LightPassRequest<'a> {
    pub grid: &'a mut WorldLightGrid,
    pub policy_source: &'a mut dyn ObjectLightPolicySource,
    pub failure_policy: Flag0LightFailurePolicy,
    /// H12/0, which the grenade's +28 samples beneath it.
    pub terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
    /// The process RNG `0048F060`, which a course target's +28 draws from.
    pub rng: Option<&'a mut u32>,
    /// The world ticks' delta since the last pass: `0055F0E0`, which
    /// `00478F20` stores every tick and the ECM Unit's glow and the
    /// vehicle's pulse add to their phases. At one tick a frame, as retail
    /// ran, it is that tick's delta.
    pub world_delta: u32,
    /// `0053DF00`, the quarter wave their colours read.
    pub sine_quarter: &'a [u16; 4096],
    /// A Vulkan world's lamps (non-native): a vehicle with its headlights
    /// on or its rear glow lit is kept here. None deposits them as retail.
    pub lamps: Option<&'a mut Vec<vehicle::VehicleLamp>>,
    /// The kept lamps light what the headlamp's pool and the rear glow lit
    /// (a traced frame's), which are left out of the grid; else (the
    /// raster's, whose lamps light only its water) the grid keeps them as
    /// retail.
    pub lamps_light: bool,
}

/// `0043B680`'s constants: the target light's glow `0054761C` and shade
/// `00547618`, signed RGB.
const TARGET_GLOW: [i8; 3] = [8, 8, 0x18];
const TARGET_SHADE: [i8; 3] = [-8, -8, -0x18];
/// `0043B8F9`: the shade's radius.
const TARGET_SHADE_RADIUS: i32 = 0x100;
/// `0042E200`'s constants, a startup initializer: the factory's grant
/// flicker `00546B1C` and its chimney's glow `00546B18`, signed RGB.
const FACTORY_GRANT: [i8; 3] = [0x20, 0x20, 0x60];
const FACTORY_CHIMNEY: [i8; 3] = [0x40, 0x40, 0];
/// `00431E35` and `00431E15`: the chimney's radius and model.
const FACTORY_CHIMNEY_RADIUS: i32 = 300;
const FACTORY_CHIMNEY_MODEL: u16 = 0x9d;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectLightCallbackBoundary {
    StoppedConstruction,
    TerrainLightStorage,
    Position,
    /// The vehicle's headlamp (`00420097`) samples H12, which this pass was
    /// not lent.
    HeadlampTerrain,
    /// The vehicle's rear glow (mode 1 or 2) finds no M+74 camera, where
    /// 00478E00 would build one before its +188 read.
    VehicleCamera,
    /// Selector 39's 0043B8A0 draws two RNG-scaled terrain lights from
    /// globals 0054761C/00547618; this pass was lent no RNG.
    TargetFlicker,
    /// Selector 15's 00431DC0 lights production states 4..6 at the dock
    /// point +190/+194, state 5 from the RNG: this pass was lent no RNG, no
    /// draw has written the dock point, or the chimney's postinit WORD +180
    /// is unwritten.
    FactoryProduction,
    /// The grenade's 00456560 samples H12, which this pass was not lent.
    GrenadeTerrain,
    /// The rock's 00454040 samples H12, which this pass was not lent.
    RockTerrain,
    /// 00454040's IDIV faults: the rock's height above the ground squares
    /// to zero.
    RockGlowFault,
    /// The behavior registry's resource classes without a ported light;
    /// 0048D820's objects leave the flag-0 list before any light pass.
    BehaviorResource,
    /// Selector 79's 00408790 colours by its phase +19C, which this
    /// allocation left unwritten.
    EyePhase,
    /// The barrier's 00411C50 lights two pulses, which the pass deposits
    /// itself; it stops here when its ground fit wrote no half length or
    /// posts, or the half length is zero.
    BarrierPulses,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flag0LightPassOmission {
    pub object: ObjectId,
    pub reason: ObjectLightCallbackBoundary,
}

/// Diagnostics of this invocation, not original call/allocation counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flag0LightPassOutcome {
    pub visited: usize,
    pub callbacks: usize,
    pub deposits: usize,
    pub light: TerrainLightOutcome,
    pub omissions: Vec<Flag0LightPassOmission>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum Flag0LightPassError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("light pass selected unavailable embedded Flag0 node {0:?}")]
    NodeCustody(ObjectNode),
    #[error("object {object:?} light callback requires {reason:?}")]
    UnownedCallback {
        object: ObjectId,
        reason: ObjectLightCallbackBoundary,
    },
    #[error(transparent)]
    Terrain(#[from] TerrainLightError),
}

impl ObjectArena {
    pub fn run_flag0_light_pass(
        &mut self,
        mut request: Flag0LightPassRequest<'_>,
    ) -> Result<Flag0LightPassOutcome, Flag0LightPassError> {
        let mut outcome = Flag0LightPassOutcome::default();
        if !request.policy_source.dynamic_lighting() {
            return Ok(outcome); // 00479508..510, before the first node read.
        }
        while let Some(&node) = self.list(ObjectList::Flag0Primary).first() {
            // Authenticate the actual typed embedded owner before private
            // move_node's unlink. This adds no native physical-fault claim.
            let owned = self.owned(node.object)?;
            if node.kind != ObjectNodeKind::Flag0
                || owned.memberships[ObjectNodeKind::Flag0 as usize]
                    != self.list_owners[ObjectList::Flag0Primary as usize]
            {
                return Err(Flag0LightPassError::NodeCustody(node));
            }
            self.move_node(
                node.object,
                ObjectNodeKind::Flag0,
                ObjectList::Flag0Secondary,
            );
            outcome.visited += 1;
            if self.object(node.object)?.base.flags & 0x0800_0000 == 0 {
                continue;
            }
            outcome.callbacks += 1;
            match self.barrier_lights(node.object) {
                Ok(None) => {}
                Ok(Some(lights)) => {
                    for packet in lights {
                        let policy = request.policy_source.terrain_light_policy();
                        let light = request.grid.add(packet, policy)?;
                        outcome.deposits += 1;
                        outcome.light.samples += light.samples;
                        outcome.light.cell_writes += light.cell_writes;
                    }
                    continue;
                }
                Err(Flag0LightPassError::UnownedCallback { object, reason })
                    if request.failure_policy
                        == Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation =>
                {
                    outcome
                        .omissions
                        .push(Flag0LightPassOmission { object, reason });
                    continue;
                }
                Err(error) => return Err(error),
            }
            if let Some(rng) = request.rng.as_deref_mut() {
                let lights = match self.target_lights(node.object, rng)? {
                    None => self.factory_lights(node.object, rng)?,
                    lights => lights,
                };
                if let Some(lights) = lights {
                    for packet in lights {
                        let policy = request.policy_source.terrain_light_policy();
                        let light = request.grid.add(packet, policy)?;
                        outcome.deposits += 1;
                        outcome.light.samples += light.samples;
                        outcome.light.cell_writes += light.cell_writes;
                    }
                    continue;
                }
            }
            match self.vehicle_lights(
                node.object,
                request.terrain,
                request.world_delta,
                request.sine_quarter,
            )? {
                None => {}
                Some(vehicle::VehicleLights {
                    headlamp,
                    rear,
                    lights,
                    lamp,
                    boundary,
                    hull,
                }) => {
                    // A traced frame's lamps light what the pool and the rear
                    // glow lit.
                    let (headlamp, rear) = match (lamp, request.lamps.as_deref_mut()) {
                        (Some(lamp), Some(lamps)) => {
                            lamps.push(lamp);
                            if request.lamps_light {
                                (None, None)
                            } else {
                                (headlamp, rear)
                            }
                        }
                        _ => (headlamp, rear),
                    };
                    for packet in headlamp.into_iter().chain(rear).chain(lights) {
                        let policy = request.policy_source.terrain_light_policy();
                        let light = match hull {
                            Some(hull) => request.grid.add_from(packet, policy, hull, false)?,
                            None => request.grid.add(packet, policy)?,
                        };
                        outcome.deposits += 1;
                        outcome.light.samples += light.samples;
                        outcome.light.cell_writes += light.cell_writes;
                    }
                    if let Some(reason) = boundary {
                        if request.failure_policy
                            != Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation
                        {
                            return Err(Flag0LightPassError::UnownedCallback {
                                object: node.object,
                                reason,
                            });
                        }
                        outcome.omissions.push(Flag0LightPassOmission {
                            object: node.object,
                            reason,
                        });
                    }
                    continue;
                }
            }
            let packet = match self.object_light_callback(
                node.object,
                request.terrain,
                request.world_delta,
                request.sine_quarter,
            ) {
                Ok(packet) => packet,
                Err(Flag0LightPassError::UnownedCallback { object, reason })
                    if request.failure_policy
                        == Flag0LightFailurePolicy::UnownedObjectLightCallbackApproximation =>
                {
                    outcome
                        .omissions
                        .push(Flag0LightPassOmission { object, reason });
                    continue;
                }
                Err(error) => return Err(error),
            };
            if let Some(packet) = packet {
                // 19/30 have already consumed all callback fields. 45B1A0
                // samples settings again even if this deposit is disabled.
                let policy = request.policy_source.terrain_light_policy();
                let light = request.grid.add(packet, policy)?;
                outcome.deposits += 1;
                outcome.light.samples += light.samples;
                outcome.light.cell_writes += light.cell_writes;
            }
        }
        self.swap_flag0_lists(); // 0047955C..565; never on a hard/strict stop.
        Ok(outcome)
    }

    /// `0043B8A0`, a course target's or generator's +28, which keeps flag
    /// 08000000. With its radiation on (+165 clear) and alive (state 3), it
    /// draws twice from the process RNG and lights the ground at its X/Z
    /// twice (`0045B1A0`): a glow of radius `((draw & 0x3F) + 0x60) * 4` in
    /// (8, 8, 24), then a shade of radius 256 in (-8, -8, -24). The second
    /// draw, `| 0xF`, is never zero and decides nothing. None for another
    /// class or a stopped construction, which the single-light callback
    /// reports.
    fn target_lights(
        &mut self,
        object: ObjectId,
        rng: &mut u32,
    ) -> Result<Option<Vec<TerrainLightRequest>>, Flag0LightPassError> {
        if self.construction_is_stopped(object)? {
            return Ok(None);
        }
        let actor = match &self.owned(object)?.state.class {
            ObjectClass::Actor(actor) => actor,
            _ => return Ok(None),
        };
        let ActorExtension::Selector39(target) = &actor.extension else {
            return Ok(None);
        };
        if target.byte_165 != 0 || (actor.word_c4 as i32) < 3 {
            return Ok(Some(Vec::new()));
        }
        let [x, _, z] = self.object(object)?.base.position_words.ok_or(
            Flag0LightPassError::UnownedCallback {
                object,
                reason: ObjectLightCallbackBoundary::Position,
            },
        )?;
        let radius = (((crate::name_entry::name_entry_rng_next(rng) & 0x3f) + 0x60) << 2) as i32;
        crate::name_entry::name_entry_rng_next(rng);
        let world_xz = [i32::from(x as i16), i32::from(z as i16)];
        Ok(Some(vec![
            TerrainLightRequest {
                world_xz,
                radius,
                rgb: TARGET_GLOW,
            },
            TerrainLightRequest {
                world_xz,
                radius: TARGET_SHADE_RADIUS,
                rgb: TARGET_SHADE,
            },
        ]))
    }

    /// `00431DC0`, the factory's +28, which keeps flag 08000000. In the
    /// grant state (5) it draws twice from the process RNG and lights the
    /// dock point (`0045B1A0`): radius `((draw & 0x3F) + 0x60) * 2` in
    /// (32, 32, 96); the second draw, `| 0xF`, is never zero and decides
    /// nothing. Then in states 4..6 the chimney model with WORD +180 set
    /// lights the dock point again, radius 300 in (64, 64, 0). Other
    /// states light nothing. None for another class, a stopped construction
    /// or an unwritten dock point or +180, which the single-light callback
    /// reports.
    fn factory_lights(
        &mut self,
        object: ObjectId,
        rng: &mut u32,
    ) -> Result<Option<Vec<TerrainLightRequest>>, Flag0LightPassError> {
        if self.construction_is_stopped(object)? {
            return Ok(None);
        }
        let ObjectClass::Actor(actor) = &self.owned(object)?.state.class else {
            return Ok(None);
        };
        let ActorExtension::Selector15(factory) = &actor.extension else {
            return Ok(None);
        };
        if super::actor::selector15_light_is_idle(factory) {
            return Ok(Some(Vec::new()));
        }
        let Some([x, _, z]) = factory.dock_190 else {
            return Ok(None);
        };
        let chimney = match (actor.rotating.visual.model_word, factory.words_178_182) {
            (FACTORY_CHIMNEY_MODEL, None) => return Ok(None),
            (FACTORY_CHIMNEY_MODEL, Some(words)) => words[4] != 0,
            _ => false,
        };
        let grant = factory.word_160 == 5;
        let world_xz = [i32::from(x as i16), i32::from(z as i16)];
        let mut lights = Vec::new();
        if grant {
            let radius =
                (((crate::name_entry::name_entry_rng_next(rng) & 0x3f) + 0x60) << 1) as i32;
            crate::name_entry::name_entry_rng_next(rng);
            lights.push(TerrainLightRequest {
                world_xz,
                radius,
                rgb: FACTORY_GRANT,
            });
        }
        if chimney {
            lights.push(TerrainLightRequest {
                world_xz,
                radius: FACTORY_CHIMNEY_RADIUS,
                rgb: FACTORY_CHIMNEY,
            });
        }
        Ok(Some(lights))
    }

    /// `00411C50`, the barrier's +28, which keeps flag 08000000: a pulse at
    /// each post. None for another class or a stopped construction, which
    /// the single-light callback reports.
    fn barrier_lights(
        &mut self,
        object: ObjectId,
    ) -> Result<Option<[TerrainLightRequest; 2]>, Flag0LightPassError> {
        if self.construction_is_stopped(object)? {
            return Ok(None);
        }
        if !matches!(
            &self.owned(object)?.state.class,
            ObjectClass::Actor(actor) if matches!(actor.extension, ActorExtension::Selector69(_))
        ) {
            return Ok(None);
        }
        match self.selector69_lights(object) {
            Ok(lights) => Ok(Some(lights)),
            Err(super::actor::Selector69Error::Object(error)) => Err(error.into()),
            Err(_) => Err(Flag0LightPassError::UnownedCallback {
                object,
                reason: ObjectLightCallbackBoundary::BarrierPulses,
            }),
        }
    }

    /// Complete admitted +28 policy, producing a packet only at a reached
    /// 45B1A0 invocation. Whole XYZ Option is a known-vector supplied domain;
    /// no per-axis read-fault or raw callback-pointer equivalence is implied.
    fn object_light_callback(
        &mut self,
        object: ObjectId,
        terrain: Option<&crate::terrain_runtime::TerrainRuntime>,
        world_delta: u32,
        sine_quarter: &[u16; 4096],
    ) -> Result<Option<TerrainLightRequest>, Flag0LightPassError> {
        if self.construction_is_stopped(object)? {
            return Err(Flag0LightPassError::UnownedCallback {
                object,
                reason: ObjectLightCallbackBoundary::StoppedConstruction,
            });
        }
        if self.is_thrown(object) {
            // 00456560: within 1500 of the ground, a grey light beneath it.
            let terrain = terrain.ok_or(Flag0LightPassError::UnownedCallback {
                object,
                reason: ObjectLightCallbackBoundary::GrenadeTerrain,
            })?;
            let light = self
                .grenade26_light(object, terrain)
                .map_err(|error| match error {
                    super::actor::Grenade26Error::Object(error) => Flag0LightPassError::from(error),
                    _ => Flag0LightPassError::UnownedCallback {
                        object,
                        reason: ObjectLightCallbackBoundary::Position,
                    },
                })?;
            return Ok(light.map(|light| TerrainLightRequest {
                world_xz: light.position_xz,
                radius: light.radius,
                rgb: light.rgb.map(|channel| channel as i8),
            }));
        }
        if self.is_shot32(object) {
            // The cannon shot's 0048C850 and the laser's 00401310 are RETs.
            return Ok(None);
        }
        if self.is_ball(object) {
            // 00404450 keeps flag 08000000: a light of 1000 at the ball's X
            // and Z, in its holder's colours or cycling while free.
            let rgb = self.ball_light(object, world_delta, sine_quarter)?;
            let [x, _, z] = self.object(object)?.base.position_words.ok_or(
                Flag0LightPassError::UnownedCallback {
                    object,
                    reason: ObjectLightCallbackBoundary::Position,
                },
            )?;
            return Ok(Some(TerrainLightRequest {
                world_xz: [i32::from(x as i16), i32::from(z as i16)],
                radius: 1000,
                rgb: [rgb[0] as i8, rgb[1] as i8, rgb[2] as i8],
            }));
        }
        if self.is_bomb29(object) {
            // The Bomb's 00525AB4 keeps the shared +28, 00401350.
            let base = &mut self.owned_mut(object)?.state.base;
            let flags = base.flags & !0x0800_0000;
            mark_flag_change(base, flags);
            return Ok(None);
        }
        if self.is_missile(object) {
            // 00457EE0: within 1500 of the ground, red fading with the height.
            let terrain = terrain.ok_or(Flag0LightPassError::UnownedCallback {
                object,
                reason: ObjectLightCallbackBoundary::GrenadeTerrain,
            })?;
            let light = self
                .missile_light(object, terrain)
                .map_err(|error| match error {
                    super::actor::MissileError::Object(error) => Flag0LightPassError::from(error),
                    _ => Flag0LightPassError::UnownedCallback {
                        object,
                        reason: ObjectLightCallbackBoundary::Position,
                    },
                })?;
            return Ok(light.map(|light| TerrainLightRequest {
                world_xz: light.position_xz,
                radius: light.radius,
                rgb: light.rgb.map(|channel| channel as i8),
            }));
        }
        let state = &mut self.owned_mut(object)?.state;
        let unowned = |reason| Flag0LightPassError::UnownedCallback { object, reason };
        let (radius, rgb) = match &mut state.class {
            // 00432490: the flamethrower glows red with its growth.
            ObjectClass::Selector76(_) => {
                let light = self.selector76_light(object)?;
                return Ok(Some(TerrainLightRequest {
                    world_xz: light.position_xz,
                    radius: light.radius,
                    rgb: light.rgb.map(|channel| channel as i8),
                }));
            }
            // 4481A0: packed+44 then zero-extended WORD+48, before XYZ;
            // selector 54's 4649C0 reads the same at +6C and +70.
            ObjectClass::TerrainLight { light, .. }
            | ObjectClass::RotatingTerrainLight { light, .. } => {
                let light = light
                    .as_ref()
                    .ok_or_else(|| unowned(ObjectLightCallbackBoundary::TerrainLightStorage))?;
                (i32::from(light.radius), light.channels)
            }
            ObjectClass::Actor(actor) => match &mut actor.extension {
                // 0042DAE0, the ECM Unit's glow.
                ActorExtension::Selector73(_) => {
                    return Ok(self.selector73_glow(object, world_delta, sine_quarter)?);
                }
                // 004647E0, the attack's light at its target point.
                ActorExtension::Selector62(_) => {
                    return Ok(self.selector62_light(object)?);
                }
                // 00454040 keeps flag 08000000: the rock's glow beneath it.
                ActorExtension::Selector44(_) => {
                    let terrain =
                        terrain.ok_or_else(|| unowned(ObjectLightCallbackBoundary::RockTerrain))?;
                    return self
                        .selector44_glow::<std::convert::Infallible>(
                            object,
                            terrain,
                            super::actor::ROCK_GLOW,
                        )
                        .map_err(|error| match error {
                            super::actor::Selector44Error::Object(error) => error.into(),
                            super::actor::Selector44Error::GlowDivide(_) => {
                                unowned(ObjectLightCallbackBoundary::RockGlowFault)
                            }
                            _ => unowned(ObjectLightCallbackBoundary::Position),
                        });
                }
                ActorExtension::Selector30(light) => {
                    // 48BD40: unsigned clamp store precedes both fresh gates.
                    if light.word_178 > 256 {
                        light.word_178 = 256;
                    }
                    if light.word_178 == 0 || light.byte_18e != 0 {
                        return Ok(None);
                    }
                    let color = light.word_174.to_le_bytes();
                    (
                        (light.word_178 << 3) as i32,
                        [color[0] as i8, color[1] as i8, color[2] as i8],
                    )
                }
                ActorExtension::Derived(derived) => match &derived.class {
                    // 00420080: the pass lights the vehicle first.
                    DerivedActorClass::Selector2(_) => return Ok(None),
                    // 00408790, the eye's light, keeps flag 08000000.
                    DerivedActorClass::Selector77(eye) if eye.pair.is_some() => {
                        let phase = eye
                            .word_19c
                            .ok_or_else(|| unowned(ObjectLightCallbackBoundary::EyePhase))?;
                        return Ok(Some(self.selector79_light(object, phase, sine_quarter)?));
                    }
                    DerivedActorClass::Base
                    | DerivedActorClass::Family428650(_)
                    | DerivedActorClass::Selector11(_)
                    | DerivedActorClass::Selector60(_)
                    | DerivedActorClass::Selector77(_) => {
                        let flags = state.base.flags & !0x0800_0000;
                        mark_flag_change(&mut state.base, flags);
                        return Ok(None);
                    }
                },
                // 0043B8A0 keeps flag 08000000: a target with its radiation
                // off (+165), or below state 3, lights nothing.
                ActorExtension::Selector39(target) => {
                    if target.byte_165 != 0 || (actor.word_c4 as i32) < 3 {
                        return Ok(None);
                    }
                    return Err(unowned(ObjectLightCallbackBoundary::TargetFlicker));
                }
                // 00431DC0 returns at once outside production states 4..6;
                // the pass lights those itself when it was lent the RNG.
                ActorExtension::Selector15(factory) => {
                    if super::actor::selector15_light_is_idle(factory) {
                        return Ok(None);
                    }
                    return Err(unowned(ObjectLightCallbackBoundary::FactoryProduction));
                }
                ActorExtension::BehaviorResource(_) => {
                    return Err(unowned(ObjectLightCallbackBoundary::BehaviorResource));
                }
                // The pass deposits the barrier's two pulses before this.
                ActorExtension::Selector69(_) => {
                    return Err(unowned(ObjectLightCallbackBoundary::BarrierPulses));
                }
                // 00478910 keeps flag 08000000: a field that is on lights
                // the ground green, radius (+154 * 5) >> 2 (unsigned).
                ActorExtension::Selector18(field) => {
                    if !field.on_176 {
                        return Ok(None);
                    }
                    (
                        (field.radius_154.wrapping_mul(5) >> 2) as i32,
                        super::actor::FORCE_FIELD_LIGHT_RGB,
                    )
                }
                // 0051FF54 keeps the shared +28 = 00401350.
                ActorExtension::Shared
                | ActorExtension::Selector55(_)
                | ActorExtension::Selector17(_)
                | ActorExtension::Selector56
                | ActorExtension::Selector78(_)
                | ActorExtension::Selector61(_)
                | ActorExtension::Selector16(_)
                | ActorExtension::Selector37(_)
                | ActorExtension::Selector47(_)
                | ActorExtension::Selector49(_)
                | ActorExtension::Selector88(_)
                | ActorExtension::Selector66(_)
                | ActorExtension::Selector90(_)
                | ActorExtension::Selector53(_)
                | ActorExtension::Selector52(_)
                | ActorExtension::Selector74(_)
                | ActorExtension::Selector84(_)
                | ActorExtension::Selector57
                | ActorExtension::Selector40(_)
                | ActorExtension::Selector41
                | ActorExtension::Selector59(_)
                | ActorExtension::Selector93(_) => {
                    let flags = state.base.flags & !0x0800_0000;
                    mark_flag_change(&mut state.base, flags);
                    return Ok(None);
                }
            },
            ObjectClass::Base
            | ObjectClass::Selector81(_)
            | ObjectClass::Selector94(_)
            | ObjectClass::Selector75(_)
            | ObjectClass::Selector86(_)
            | ObjectClass::Selector67(_)
            | ObjectClass::MapAnnotation(_)
            | ObjectClass::Visual(_)
            | ObjectClass::Selector22(_)
            | ObjectClass::Selector65(_)
            | ObjectClass::Selector50(_)
            | ObjectClass::Selector48(_)
            | ObjectClass::Selector43(_)
            | ObjectClass::Selector42(_)
            | ObjectClass::Selector64(_)
            | ObjectClass::TimedPickup(_)
            | ObjectClass::RotatingVisual(_)
            | ObjectClass::Selector45(_)
            | ObjectClass::Selector46(_)
            | ObjectClass::Race(_)
            | ObjectClass::Selector58(_)
            | ObjectClass::Selector83(_)
            | ObjectClass::RouteNode { .. } => {
                let flags = state.base.flags & !0x0800_0000;
                mark_flag_change(&mut state.base, flags);
                return Ok(None);
            }
        };
        let [x, _, z] = state
            .base
            .position_words
            .ok_or_else(|| unowned(ObjectLightCallbackBoundary::Position))?;
        Ok(Some(TerrainLightRequest {
            world_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius,
            rgb,
        }))
    }
}
