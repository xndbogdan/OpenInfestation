//! Production services for the world contact pass 004791F0. Terrain contact
//! runs the actor's own H8 model through terrain table 00537090; furniture
//! contact reuses the 0047C1D0 prefix owner and impact damage the live
//! 0047C6E0 composition. Each phase stops per actor.

use std::sync::Arc;

use thiserror::Error;

use crate::{
    model_auxiliary::{
        query_auxiliary_actor_pair, query_auxiliary_terrain, AuxiliaryActorPairRequest,
        AuxiliaryModelError, AuxiliaryModelSource, AuxiliaryTerrain, AuxiliaryTerrainRequest,
        AuxiliaryValueSource,
    },
    model_process::ModelProcessOwners,
    model_scratch::{ModelScratchDrawState, ModelScratchError, RegisteredMarkUnwindApproximation},
    object_runtime::{
        collide_furniture_prefix, resident_model_resource, ActorDamagePacket, ActorDamageRequest,
        ActorExtension, ActorHandle, ActorPairError, ActorPairHost, ActorTerrainInputs,
        ActorTerrainSurface, CameraSettings, CollisionContact, DerivedActorClass,
        FamilyCollisionGuardError, FamilyCollisionGuardHost, FamilyCollisionGuardRequest,
        FamilyPairReactionError, FamilyPairReactionRequest, FamilyRescueAccept, FamilyUpdateClass,
        FurnitureCollisionPair, FurnitureCollisionPairState, FurnitureCollisionPrefixError,
        FurnitureCollisionPrefixHost, FurnitureCollisionPrefixRequest, FurnitureContactResponse,
        FurnitureResponseError, FurnitureResponseHost, NetworkImpactGate, ObjectArena, ObjectError,
        ObjectId, ObjectModelCallbacks, PickupBirthRequest, ResidentModelBinding,
        TerrainDestructionEffect, TerrainSphereGrid, WeightedTargetConstructorNull,
        WorldContactHost, WorldContactPhases, WorldPairHost,
    },
    process_particles::births::{
        ParticleBirthInputs, ParticleBirthRequest, ParticleImpactSparksRequest,
    },
    process_sound::ProcessParticleSoundQueue,
    session_platform::{
        particles::{
            damage::{
                apply_world_actor_damage, birth_world_burst, world_retirement_host,
                WorldDamageError, WorldDamageOwners,
            },
            radius::{apply_radius_effect, ProjectileFurnitureResources, WorldRadiusOrigin},
        },
        world_actor_behavior::DyingStateZero,
        world_actor_factory::{RequiredListenerEffects, WorldActorChildHost, WorldPickupHost},
        world_actor_update::{gate_actor_player, gate_grant, GateCamera},
    },
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::{
        FurnitureDamageRequest, ProcessFurnitureRadius, TerrainRuntime, TerrainSpacing,
        TerrainSparseRecordId,
    },
    world_actor_collision::{
        resident_active_pair, resident_placement_slot, ResidentFurnitureCollisionError,
    },
    world_actor_terrain::{WorldActorTerrainError, WorldTerrainSource},
    world_backend::WorldActorUpdateState,
    world_furniture_effect::{
        AuxiliaryRawSource, FurnitureDamageRuntime, FurnitureQueryRecord,
        LiteralRetExitRegistrationApproximation, ProcessFurnitureQueryState,
    },
    world_radius_effect::{ScaledRadiusEffect, WorldRadiusEffectRequest},
    world_start::WorldStartState,
};
use inf_formats::{
    native_basis::{NativeBasis, NativeBasisMath},
    TerrainResourceLookup,
};

#[derive(Debug, Error)]
pub(crate) enum WorldContactPassError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Auxiliary(#[from] AuxiliaryModelError),
    #[error(transparent)]
    Scratch(#[from] ModelScratchError),
    #[error(transparent)]
    Terrain(#[from] WorldActorTerrainError),
    #[error("0044EC50 reached NULL H8 model slot {slot} for actor {actor:?}")]
    MissingModel { actor: ObjectId, slot: u16 },
    #[error("reached furniture collision prefix failed: {0}")]
    Furniture(FurnitureCollisionPrefixError<PassFurnitureError>),
    #[error(transparent)]
    Damage(#[from] WorldDamageError),
    #[error("actor {actor:?} ({class}) has no admitted pair class for 0047B610")]
    PairClass { actor: ObjectId, class: String },
    #[error("actor {actor:?} pair virtual {address:08X} reaches an unowned arm")]
    RequiredPairVirtual { actor: ObjectId, address: u32 },
    #[error("0048AB10 impact sparks failed: {0}")]
    ImpactEffect(String),
    #[error("the Boulders' contact particle failed: {0}")]
    BoulderParticle(String),
    #[error("+94 destruction service failed: {0}")]
    Destruction(String),
    #[error(transparent)]
    FamilyPair(#[from] FamilyPairReactionError),
    #[error("00468160 boarding failed: {0}")]
    Boarding(String),
    #[error("0042F170 staffing failed: {0}")]
    Staffing(String),
    #[error("00455640 missile contact failed: {0}")]
    Missile(String),
    #[error("00456010 charge contact failed: {0}")]
    Charge27(String),
    #[error("00403660 ball contact failed: {0}")]
    Ball(String),
    #[error("00458DC0 implant contact failed: {0}")]
    Implant(String),
    #[error("00469030's 0047CE90 failed: {0}")]
    Retirement(String),
    #[error("0048BD90 gate entry failed: {0}")]
    Gate(String),
    #[error("0043F790 transport pickup failed: {0}")]
    Pickup(String),
    #[error("0043C150 capture node contact failed: {0}")]
    CaptureNode(String),
    #[error("00407BB0 eye contact failed: {0}")]
    Eye(String),
}

/// Final tables reached by 004790C0 whose +A0/+A4 are owned. Their +48 is
/// 004026B0, and their +5C/+54/+64/+68 are NULL (00401300) except Selector12
/// +5C, Selector2 +64, Selector30 +54 and the grenade's +68, which return
/// themselves (00404610).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::session_platform) enum PairClass {
    Family(FamilyUpdateClass),
    Selector2,
    Selector11,
    Selector60,
    Selector66,
    Selector77,
    /// The gate 005289C0: +A0 00402720, +A4 00405570.
    Selector30,
    /// The factory 00521940, whose +48/+54/+5C/+64/+68/+A0/+A4 are the
    /// gate's; its +D0 is 0042F170.
    Selector15,
    /// The bridge 0051FF54, the gate's too; its +D0 is the family's
    /// 0040DAB0.
    Selector37,
    /// The course target or generator 00522398, the gate's too; its +D0 is
    /// 0043BEA0.
    Selector39,
    /// The Boulders 0051F900: the shared prefix's +A0 00402720 and +48/+54/
    /// +5C/+64/+68, and their own +A4 00405E30, which crushes the partner.
    Selector59,
    /// Powerball's ball 0051F1E0, the Boulders' table but for its +A0
    /// 00403630, which passes over its holder, and +A4 00403660.
    Ball,
    /// The shared actor table 00527C00 and its near copies (selectors 6, 16,
    /// 52, 53, 57, 69, 88, 90 and 93): +A0 00402720 and +A4 00405570, with
    /// +54/+5C/+64/+68 all 00401300, so a partner's queries find nothing
    /// either.
    SharedPrefix,
    /// The grenade 005268AC and the mine 00525914: +A0 00455610, +A4
    /// 00455830, and +68 00404610, which answers itself.
    Grenade26,
    /// The charge 00525848 (selector 27): the grenade's +A4 and +68, and
    /// its own +A0 00456010, which sticks it to an actor of another team.
    Charge27,
    /// The missiles 00525B88 and 005259E8 (selectors 25 and 28): +A0
    /// 00455610 as the grenade's, +A4 00455640, and +68 00404610.
    Missile,
    /// The rock 00525648 (selector 44): the shared prefix's +A0 00402720
    /// and +48/+54/+5C/+64/+68, and its own +A4 00454180, which retires it.
    Rock,
    /// The Bomb 00525AB4 (selector 29): the shared +A0 00402720, the
    /// missiles' +A4 00455640, and +68 00404610, which answers itself.
    Bomb,
    /// The capture node 00522488 (selector 84): the shared +A0 00402720
    /// and +48/+54/+5C/+64/+68, and its own +A4 0043C150, which takes the
    /// team of a partner whose +5C answers.
    Selector84,
}

pub(in crate::session_platform) fn pair_class(
    arena: &ObjectArena,
    actor: ObjectId,
) -> Option<PairClass> {
    if let Ok(class) = arena.family_update_class(actor) {
        return Some(PairClass::Family(class));
    }
    if arena.is_charge27(actor) {
        return Some(PairClass::Charge27);
    }
    if arena.is_thrown(actor) {
        return Some(PairClass::Grenade26);
    }
    if arena.is_missile(actor) {
        return Some(PairClass::Missile);
    }
    if arena.is_bomb29(actor) {
        return Some(PairClass::Bomb);
    }
    if arena.is_ball(actor) {
        return Some(PairClass::Ball);
    }
    match &arena.actor(actor).ok()?.extension {
        ActorExtension::Selector66(_) => Some(PairClass::Selector66),
        ActorExtension::Selector30(_) => Some(PairClass::Selector30),
        ActorExtension::Selector15(_) => Some(PairClass::Selector15),
        // The stabiliser 00527908 has the bridge's contact slots and +D0.
        ActorExtension::Selector37(_)
        | ActorExtension::Selector49(_)
        | ActorExtension::Selector47(_) => Some(PairClass::Selector37),
        ActorExtension::Selector39(_) => Some(PairClass::Selector39),
        ActorExtension::Selector59(_) => Some(PairClass::Selector59),
        ActorExtension::Selector44(_) => Some(PairClass::Rock),
        ActorExtension::Selector84(_) => Some(PairClass::Selector84),
        ActorExtension::Shared
        | ActorExtension::Selector78(_)
        | ActorExtension::Selector16(_)
        | ActorExtension::Selector53(_)
        | ActorExtension::Selector52(_)
        | ActorExtension::Selector73(_)
        | ActorExtension::Selector55(_)
        | ActorExtension::Selector17(_)
        | ActorExtension::Selector56
        | ActorExtension::Selector74(_)
        | ActorExtension::Selector61(_)
        | ActorExtension::Selector62(_)
        | ActorExtension::Selector57
        | ActorExtension::Selector40(_)
        | ActorExtension::Selector41
        | ActorExtension::Selector18(_)
        | ActorExtension::Selector69(_)
        | ActorExtension::Selector88(_)
        | ActorExtension::Selector90(_)
        | ActorExtension::Selector93(_) => Some(PairClass::SharedPrefix),
        ActorExtension::Derived(derived) => match derived.class {
            DerivedActorClass::Selector2(_) => Some(PairClass::Selector2),
            DerivedActorClass::Selector11(_) => Some(PairClass::Selector11),
            DerivedActorClass::Selector60(_) => Some(PairClass::Selector60),
            DerivedActorClass::Selector77(_) => Some(PairClass::Selector77),
            _ => None,
        },
        _ => None,
    }
}

/// A pair partner's +68: the grenade's 00404610 answers itself; every other
/// admitted table holds the null 00401300.
fn answers_68(class: PairClass) -> bool {
    matches!(
        class,
        PairClass::Grenade26 | PairClass::Charge27 | PairClass::Missile | PairClass::Bomb
    )
}

fn unadmitted(arena: &ObjectArena, actor: ObjectId) -> WorldContactPassError {
    let class = match arena.actor(actor).map(|state| &state.extension) {
        Ok(ActorExtension::Derived(derived)) => match &derived.class {
            DerivedActorClass::Base => "derived base",
            DerivedActorClass::Family428650(_) => "unadmitted 00428650 family",
            DerivedActorClass::Selector11(_) => "selector 11",
            DerivedActorClass::Selector2(_) => "selector 2",
            DerivedActorClass::Selector60(_) => "selector 60",
            DerivedActorClass::Selector77(_) => "selector 77",
        },
        Ok(ActorExtension::Shared) => "shared actor",
        Ok(ActorExtension::Selector15(_)) => "selector 15",
        Ok(ActorExtension::Selector16(_)) => "selector 16",
        Ok(ActorExtension::Selector30(_)) => "selector 30",
        Ok(ActorExtension::Selector37(_)) => "selector 37",
        Ok(ActorExtension::Selector47(_)) => "selector 47",
        Ok(ActorExtension::Selector39(_)) => "selector 39",
        Ok(ActorExtension::Selector53(_)) => "selector 53",
        Ok(ActorExtension::Selector52(_)) => "selector 52",
        Ok(ActorExtension::Selector44(_)) => "selector 44",
        Ok(ActorExtension::Selector57) => "selector 57",
        Ok(ActorExtension::Selector40(_)) => "selector 40",
        Ok(ActorExtension::Selector41) => "selector 41",
        Ok(ActorExtension::Selector73(_)) => "selector 73",
        Ok(ActorExtension::Selector55(_)) => "selector 55",
        Ok(ActorExtension::Selector17(_)) => "selector 17",
        Ok(ActorExtension::Selector56) => "selector 56",
        Ok(ActorExtension::Selector74(_)) => "selector 74",
        Ok(ActorExtension::Selector61(_)) => "selector 61",
        Ok(ActorExtension::Selector62(_)) => "selector 62",
        Ok(ActorExtension::Selector84(_)) => "selector 84",
        Ok(ActorExtension::Selector78(_)) => "selector 78",
        Ok(ActorExtension::Selector59(_)) => "selector 59",
        Ok(ActorExtension::Selector18(_)) => "selector 18",
        Ok(ActorExtension::Selector69(_)) => "selector 69",
        Ok(ActorExtension::Selector49(_)) => "selector 49",
        Ok(ActorExtension::Selector66(_)) => "selector 66",
        Ok(ActorExtension::Selector88(_)) => "selector 88",
        Ok(ActorExtension::Selector90(_)) => "selector 90",
        Ok(ActorExtension::Selector93(_)) => "selector 93",
        Ok(ActorExtension::BehaviorResource(_)) => "behavior resource",
        Err(_) => "not an actor",
    }
    .to_owned();
    WorldContactPassError::PairClass { actor, class }
}

#[derive(Debug, Error)]
pub(crate) enum PassFurnitureError {
    #[error(transparent)]
    Collision(#[from] ResidentFurnitureCollisionError),
    #[error("actor {actor:?} has no owned virtual +A8 furniture response")]
    ResponseClass { actor: ObjectId },
    #[error(transparent)]
    Response(#[from] FurnitureResponseError<String>),
    #[error("00455660 missile furniture contact failed: {0}")]
    Missile(String),
    #[error("00454190's 0047CE90 failed: {0}")]
    Retirement(String),
}

/// Short loans of the retained world and process owners for one pass. The
/// damage owners also lend the prepared terrain read by both queries.
pub(crate) struct WorldContactPassHost<'a, 'exe> {
    pub damage: WorldDamageOwners<'a, 'exe>,
    pub rng: &'a mut u32,
    /// A second residency for models the world's own does not register
    /// (Pelagia's ships borrow the Command Centre's 665, as their draw
    /// does); None keeps retail's NULL model stop.
    pub model_fallback: Option<&'a crate::ResidentRuntime>,
    pub model_process: &'a ModelProcessOwners,
    pub furniture_query: &'a mut ProcessFurnitureQueryState,
    pub furniture_radius: &'a ProcessFurnitureRadius,
    pub furniture_draw: &'a FurnitureDrawState,
    pub shared_basis: &'a NativeBasis,
    pub terrain_source: &'a WorldTerrainSource,
    /// Retained H14/0 +4; only bit 0x40 reaches 00410F60.
    pub world_flags: u32,
    pub sine_quarter: &'a [u16; 4096],
    pub network_active: bool,
    /// Actor factory loans for the Selector34 crash pickup 0046D870; the
    /// creation listeners come through the damage owners' factory loan.
    pub world_start: &'a mut WorldStartState,
    pub camera_settings: CameraSettings,
    /// The paced actors skip this tick (`PortFixes::paced_actors`): a
    /// contact bump then leaves their request for a behaviour choice as
    /// it was, so they call as often as at retail's 30 fps.
    pub paced_skip: bool,
    /// `PortFixes::mission_completion`, off in a network game: the rescue's
    /// grant as its last scientist boards.
    pub mission_completion: bool,
    /// A boarding scientist's capture announces its grants.
    pub grant_text: Option<&'a crate::grant_text::GrantTextData<'a>>,
    /// The shop's tables, parsed when a staffed factory's bound menu is
    /// first rebuilt.
    pub factory_tables: std::cell::OnceCell<Result<crate::factory_shop::FactoryShopTables, String>>,
    /// The basis math of a scientist's gate pass; its world flash is the
    /// damage owners'.
    pub math: &'a inf_formats::native_basis::NativeBasisMath,
}

/// The factory's staff from the pair pass: a scientist that walks in.
impl crate::object_runtime::FactoryStaffHost for WorldContactPassHost<'_, '_> {
    type Error = String;

    fn process(&self) -> &crate::process_particles::factories::ParticleFactoryProcessState {
        self.damage.factory_process
    }

    fn grants(&self) -> crate::object_runtime::LiveGrantContext<'_> {
        crate::object_runtime::LiveGrantContext {
            process: Some(self.damage.factory_process),
            executable: Some(self.damage.executable),
            resident: Some(self.damage.residency),
            text_data: self.grant_text,
        }
    }

    fn shop_tables(&self) -> Result<&crate::factory_shop::FactoryShopTables, String> {
        self.factory_tables
            .get_or_init(|| {
                crate::factory_shop::FactoryShopTables::parse(self.damage.executable)
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        let process = *self.damage.factory_process;
        use crate::object_runtime::VehicleCarryHost;
        crate::session_platform::world_actor_update::CarryEffects {
            particles: &mut *self.damage.particles,
            sounds: &mut *self.damage.sounds,
            residency: self.damage.residency,
            executable: self.damage.executable,
            process: &process,
            phase: u32::from(self.damage.session.current_phase().id()),
        }
        .effect(
            arena,
            crate::object_runtime::VehicleCarryEffect::Particle {
                selector: 2,
                position,
                flags: 0,
            },
            self.rng,
        )
    }

    fn complete_placement(&mut self, index: i8) -> Result<(), String> {
        let inhibit_byte_08 = self.damage.mission.inhibit_byte_08;
        self.damage
            .mission
            .record_placement_death(crate::world_mission::PlacementDeathRequest {
                index: i32::from(index),
                inhibit_byte_08,
                force: 0,
            })
            .map(|_| ())
            .map_err(|error| format!("0048EAD0 placement {index}: {error}"))
    }
}

/// `0048BE70` for a scientist the pair pass offered to a `+18E` gate. A
/// scientist has no `+64` view, so the hold's drop and a world's end, which
/// only a player's vehicle reaches, stay out of it.
struct ContactGatePass<'h, 'a, 'exe> {
    host: &'h mut WorldContactPassHost<'a, 'exe>,
}

impl crate::object_runtime::Selector30PassHost for ContactGatePass<'_, '_, '_> {
    type Error = String;

    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<crate::object_runtime::PlayerId>, String> {
        gate_actor_player(arena, actor, self.host.damage.factory_process)
    }

    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: crate::object_runtime::PlayerId,
        id: u16,
    ) -> Result<(), String> {
        let context = crate::object_runtime::FactoryStaffHost::grants(&*self.host);
        gate_grant(arena, player, id, &context)
    }

    fn drop_item(&mut self, _: &mut ObjectArena, actor: ObjectId, _: usize) -> Result<(), String> {
        Err(format!(
            "0048BF00: {actor:?} is a vehicle in the pair pass's scientist arm"
        ))
    }

    fn world_status(&mut self) -> Result<(u32, u32), String> {
        Err("0048C3AA: only a player's vehicle leaves the world".into())
    }

    fn set_world_status(&mut self, _: u32) -> Result<(), String> {
        Err("0048C3FA: only a player's vehicle leaves the world".into())
    }

    fn grant_cheat_flag(&mut self, _: u32) -> Result<(), String> {
        Err("0048C41A: only a player's vehicle leaves the world".into())
    }

    fn enter_pelagia(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), String> {
        Err("0048C446: only a player's vehicle leaves the world".into())
    }

    fn request_world(
        &mut self,
        _: &mut ObjectArena,
        _: u32,
        _: u16,
        _: ObjectId,
    ) -> Result<(), String> {
        Err("0048C446: only a player's vehicle leaves the world".into())
    }

    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        let (camera, flash, rng) = self.host.gate_services()?;
        camera.flash(flash, arena, position, rng)
    }

    fn camera(&mut self, arena: &mut ObjectArena) -> Result<ObjectId, String> {
        let (camera, _, rng) = self.host.gate_services()?;
        camera.camera(arena, rng)
    }

    fn notify_listeners(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        let loan = self
            .host
            .damage
            .behavior_factory
            .as_mut()
            .ok_or("0048C2E9 needs the world's creation listeners")?;
        loan.listeners
            .notify(actor, arena, &mut RequiredListenerEffects)
            .map_err(|error| error.to_string())
    }
}

impl WorldContactPassHost<'_, '_> {
    /// The gate pass's camera loans, the flash and the RNG, borrowed apart.
    fn gate_services(
        &mut self,
    ) -> Result<
        (
            GateCamera<'_>,
            &mut crate::world_flash::WorldFlashState,
            &mut u32,
        ),
        String,
    > {
        let flash = self
            .damage
            .flash
            .as_deref_mut()
            .ok_or("the gate pass's flash needs the world flash")?;
        let loan = self
            .damage
            .behavior_factory
            .as_mut()
            .ok_or("the gate pass's camera needs the world's creation listeners")?;
        Ok((
            GateCamera {
                residency: self.damage.residency,
                sine: self.sine_quarter,
                camera_settings: self.camera_settings,
                factory_process: self.damage.factory_process,
                animation_clock: self.damage.animation_clock.animation_value(),
                terrain: self.damage.terrain,
                world_flags: self.world_flags,
                listeners: &mut *loan.listeners,
            },
            flash,
            &mut *self.rng,
        ))
    }

    /// `0048BDE7..0048BE0D`: a gate that changed sides marks its placement
    /// (`0048EAD0(+78, 0)`) or clears it (`0048EB30(+78, 0)`).
    fn gate_record(
        &mut self,
        record: crate::object_runtime::GateRecord,
    ) -> Result<(), WorldContactPassError> {
        self.damage
            .mission
            .apply_gate_record(record)
            .map(|_| ())
            .map_err(|error| WorldContactPassError::Gate(error.to_string()))
    }

    /// A scientist reached Pelagia's locked gate (the port's,
    /// [`crate::pelagia`]): the gate wakes, lights its bulb over the
    /// scientist, and the scientist reports on the first player's message
    /// line.
    fn wake_pelagia(
        &mut self,
        arena: &mut ObjectArena,
        gate: ObjectId,
        scientist: ObjectId,
    ) -> Result<(), WorldContactPassError> {
        arena
            .wake_pelagia_gate(
                gate,
                scientist,
                &crate::object_runtime::PlayerCommandContext {
                    process: Some(self.damage.factory_process),
                    executable: Some(self.damage.executable),
                    resident: Some(self.damage.residency),
                },
            )
            .map_err(WorldContactPassError::Gate)?;
        if let Some(player) = arena.first_player() {
            let language = self.damage.residency.summary().selection.language;
            arena
                .deliver_player_port_text(player, crate::pelagia::report(u32::from(language)))
                .map_err(|error| WorldContactPassError::Gate(error.to_string()))?;
        }
        eprintln!("Pelagia: a scientist woke the Command Centre's gate");
        Ok(())
    }

    /// 00468160's vehicle arm: the scientist takes itself into the hold.
    fn board_vehicle(
        &mut self,
        arena: &mut ObjectArena,
        scientist: ObjectId,
        vehicle: ObjectId,
    ) -> Result<bool, String> {
        // Only native-receipt harnesses write the process state; the take
        // borrows a scratch copy of the pass's owner.
        let process = *self.damage.factory_process;
        let context = crate::object_runtime::VehicleCarryContext {
            process: &process,
            executable: self.damage.executable,
            resident: self.damage.residency,
            text_data: self.grant_text,
        };
        let mut effects = crate::session_platform::world_actor_update::CarryEffects {
            particles: &mut *self.damage.particles,
            sounds: &mut *self.damage.sounds,
            residency: self.damage.residency,
            executable: self.damage.executable,
            process: &process,
            phase: u32::from(self.damage.session.current_phase().id()),
        };
        let boarded = arena
            .board_vehicle(scientist, vehicle, context, &mut effects, self.rng)
            .map_err(|error| error.to_string())?;
        // One of Pelagia's colonists tells its line of the colony's story
        // on the player's message line, once ([`crate::pelagia`]).
        if boarded {
            if let Some(line) = arena.take_pelagia_story(scientist) {
                if let Some(player) = arena.first_player() {
                    let language = self.damage.residency.summary().selection.language;
                    arena
                        .deliver_player_port_text(
                            player,
                            crate::pelagia::colonist_line(line, u32::from(language)),
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(boarded)
    }
}

/// An actor's model for the contact queries: the world's own, else the
/// borrowed `fallback`'s (Pelagia's ships), else none, where retail's NULL
/// stops the pass.
pub(crate) fn contact_model<'r>(
    residency: &'r crate::ResidentRuntime,
    fallback: Option<&'r crate::ResidentRuntime>,
    slot: u16,
) -> Result<Option<ResidentModelBinding<'r>>, WorldContactPassError> {
    if let Some(binding) = resident_model_resource(residency, slot)? {
        return Ok(Some(binding));
    }
    match fallback {
        Some(fallback) => Ok(resident_model_resource(fallback, slot)?),
        None => Ok(None),
    }
}

impl WorldContactHost for WorldContactPassHost<'_, '_> {
    type Error = WorldContactPassError;

    fn terrain_contact(
        &mut self,
        arena: &ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<CollisionContact>, Self::Error> {
        let residency = self.damage.residency;
        // 0047923D: H8 word +42 through 00405540. NULL faults in 004F2C50.
        let slot = arena.actor(actor)?.rotating.visual.model_word;
        let binding = contact_model(residency, self.model_fallback, slot)?
            .ok_or(WorldContactPassError::MissingModel { actor, slot })?;
        // Every constructed actor table (selectors 2, 6, 7, 10, 11, 12, 30,
        // 33, 34, 35, 36, 60, 66, 77, 88, 90, 93) holds +48 = 004026B0.
        let basis = arena.actor(actor)?.rotating.basis;
        let position = arena
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?;
        let current = residency
            .resource(12, 0)
            .ok_or(WorldActorTerrainError::RequiredTerrain)?;
        if !self.terrain_source.is_current(&current) {
            return Err(WorldActorTerrainError::PreparedTerrainReplaced.into());
        }
        let terrain = &*self.damage.terrain;
        let sampling = terrain.sampling();
        let qualification = AuxiliaryRawSource {
            registration: binding.registration.clone(),
            allocation: Arc::downgrade(binding.allocation),
            vertices_span: binding.model.vertices_span,
        };
        let mut contact = CollisionContact::unwritten();
        let result = query_auxiliary_terrain(
            AuxiliaryTerrainRequest {
                model: AuxiliaryModelSource {
                    model: binding.model,
                    qualification: &qualification,
                },
                basis,
                // 0047927E..9C: request +10/+14/+18 sign-extend +32/+34/+36.
                origin: position.map(|word| i32::from(word as i16)),
                terrain: AuxiliaryTerrain {
                    cells: terrain.cells(),
                    grid: TerrainSphereGrid {
                        spacing: sampling.spacing,
                        mask: sampling.spacing,
                        height_shift: sampling.height_scale.shift(),
                    },
                },
                // 004792AE: wrapper 00527B18 captures this actor.
                values: AuxiliaryValueSource::Object(ObjectModelCallbacks {
                    arena,
                    source: actor,
                    animation: self.damage.animation_clock,
                    executable: self.damage.executable,
                }),
                residency: Some(residency),
            },
            self.furniture_query,
            self.model_process,
            &mut contact,
        );
        // Contain a stopped feature like the projectile query: retire marks,
        // keep pool bytes and publication, and still return the error.
        if result
            .as_ref()
            .is_err_and(AuxiliaryModelError::is_required_feature_boundary)
        {
            if let ModelScratchDrawState::Stopped(scope) = self.model_process.scratch.state()? {
                self.model_process
                    .scratch
                    .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)?;
            }
        }
        result?;
        // 0044EC67..6C: SETNE over the whole flag DWORD.
        Ok(contact
            .flag()
            .is_some_and(|flag| flag != 0)
            .then_some(contact))
    }

    fn water_inputs(&self) -> ActorTerrainInputs<'_> {
        let terrain = &*self.damage.terrain;
        ActorTerrainInputs {
            terrain: Some(ActorTerrainSurface {
                terrain,
                plane_height_fixed: terrain.plane_base(),
            }),
            coordinate_shift: terrain.plane_shift(),
            clock: self.damage.animation_clock.animation_value(),
            world_flags: Some(self.world_flags),
            sine_quarter: self.sine_quarter,
        }
    }

    /// Settings `+34` (`00463AB0`), Landscape Damaging.
    fn network_impact_gate(&self) -> NetworkImpactGate {
        if self.network_active {
            NetworkImpactGate::Session(self.damage.factory_process.settings[0x34 / 4])
        } else {
            NetworkImpactGate::Offline
        }
    }

    fn impact_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        Ok(apply_world_actor_damage(
            arena,
            actor,
            request.packet,
            request.source,
            &mut self.damage,
            self.rng,
        )?)
    }

    fn furniture_contact(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error> {
        let Self {
            damage,
            rng,
            model_process,
            furniture_query,
            furniture_radius,
            furniture_draw,
            shared_basis,
            terrain_source,
            network_active,
            model_fallback,
            ..
        } = self;
        let mut host = PassFurnitureHost {
            damage,
            rng,
            model_fallback: *model_fallback,
            model_process,
            terrain_source,
            furniture_draw,
            shared_basis,
            network_active: *network_active,
        };
        collide_furniture_prefix(
            arena,
            FurnitureCollisionPrefixRequest {
                actor,
                process: furniture_query,
                radius: furniture_radius,
                exit_registration: LiteralRetExitRegistrationApproximation,
            },
            &mut host,
        )
        .map_err(WorldContactPassError::Furniture)
    }

    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.damage.factory_process.process_null_handle
    }

    #[track_caller]
    fn random(&mut self) -> Result<u32, Self::Error> {
        Ok(crate::name_entry::name_entry_rng_next(self.rng))
    }

    fn boulder_particle(
        &mut self,
        kind: crate::object_runtime::BoulderParticle,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), Self::Error> {
        let damage = &mut self.damage;
        let request = ParticleBirthRequest {
            particle_type: 0,
            position,
            velocity: Some(velocity),
            flags: 0,
            inputs: ParticleBirthInputs {
                network_active: damage.factory_process.network_flag != 0,
                phase: Some(u32::from(damage.session.current_phase().id())),
                process: damage.factory_process,
            },
        };
        match kind {
            crate::object_runtime::BoulderParticle::Splash => damage
                .particles
                .birth_impact(request, damage.executable, self.rng)
                .map(|_| ()),
            crate::object_runtime::BoulderParticle::Dust => damage
                .particles
                .birth_material_sprite(request, false, damage.executable, self.rng)
                .map(|_| ()),
        }
        .map_err(|error| WorldContactPassError::BoulderParticle(format!("{error:?}")))
    }

    fn destruction_effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: TerrainDestructionEffect,
    ) -> Result<(), Self::Error> {
        let damage = &mut self.damage;
        match effect {
            TerrainDestructionEffect::Burst(burst) => birth_world_burst(damage, burst, self.rng)
                .map_err(WorldContactPassError::Destruction),
            TerrainDestructionEffect::Radius {
                block_address,
                source,
            } => apply_radius_effect(
                arena,
                WorldRadiusEffectRequest {
                    origin: WorldRadiusOrigin::Actor {
                        actor,
                        block_address,
                    },
                    damage_source: source,
                    replicate: true,
                },
                damage,
                self.rng,
            )
            .map_err(WorldContactPassError::Destruction),
            TerrainDestructionEffect::Pickup { kind, value } => {
                // 00434F6E passes the live +32 words of the crashing ship.
                let mut position = arena
                    .object(actor)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(actor))?;
                let math = NativeBasisMath::new(*self.sine_quarter);
                let listeners = &mut *damage
                    .behavior_factory
                    .as_mut()
                    .ok_or(WorldContactPassError::Destruction(
                        "crash pickup 0046D870 requires the world's creation listeners".into(),
                    ))?
                    .listeners;
                let mut host = WorldPickupHost {
                    factory: WorldActorChildHost {
                        residency: damage.residency,
                        executable: damage.executable,
                        process: damage.factory_process,
                        terrain: damage.terrain,
                        world_flags: self.world_flags,
                        clock: damage.animation_clock.animation_value(),
                        sine: self.sine_quarter,
                        math: &math,
                        camera_settings: Some(self.camera_settings),
                        world_start: Some(&mut *self.world_start),
                        listeners,
                        listener_effects: &mut RequiredListenerEffects,
                    },
                    terrain_source: self.terrain_source,
                };
                arena
                    .birth_timed_pickup(
                        PickupBirthRequest {
                            position: &mut position,
                            kind,
                            value,
                        },
                        &mut host,
                        self.rng,
                    )
                    .map(|_| ())
                    .map_err(|error| WorldContactPassError::Destruction(error.to_string()))
            }
            TerrainDestructionEffect::Retire => {
                let null = damage.factory_process.process_null_handle;
                arena
                    .retire_actor(actor, null, &mut world_retirement_host(damage, self.rng))
                    .map(|_| ())
                    .map_err(|error| {
                        WorldContactPassError::Destruction(format!("0047CE90: {error:?}"))
                    })
            }
            TerrainDestructionEffect::ChangeState(request) => arena
                .change_actor_state(actor, request, &mut DyingStateZero)
                .map_err(|error| WorldContactPassError::Destruction(error.to_string())),
        }
    }
}

/// 0047C1D0 for 004791F0's flag-4 phase. Terrain reads go through `&self`, so
/// a reached response can lend the same terrain to 0047C6E0's owners.
struct PassFurnitureHost<'h, 'a, 'exe> {
    damage: &'h mut WorldDamageOwners<'a, 'exe>,
    rng: &'h mut u32,
    /// [`WorldContactPassHost::model_fallback`].
    model_fallback: Option<&'a crate::ResidentRuntime>,
    model_process: &'a ModelProcessOwners,
    terrain_source: &'a WorldTerrainSource,
    furniture_draw: &'a FurnitureDrawState,
    shared_basis: &'a NativeBasis,
    network_active: bool,
}

/// The H12 captured at 0047C1E7; its cells are reread through the host.
#[derive(Clone, Copy)]
struct CapturedPassTerrain;

impl<'a> FurnitureCollisionPrefixHost for PassFurnitureHost<'_, 'a, '_> {
    type Error = PassFurnitureError;
    type Terrain = CapturedPassTerrain;
    type Model = ResidentModelBinding<'a>;
    fn acquire_terrain(&mut self) -> Result<Option<Self::Terrain>, Self::Error> {
        let Some(current) = self.damage.residency.resource(12, 0) else {
            return Ok(None);
        };
        if !self.terrain_source.is_current(&current) {
            return Err(ResidentFurnitureCollisionError::from(
                WorldActorTerrainError::PreparedTerrainReplaced,
            )
            .into());
        }
        Ok(Some(CapturedPassTerrain))
    }
    fn terrain(&self, _: Self::Terrain) -> Result<&TerrainRuntime, Self::Error> {
        Ok(&*self.damage.terrain)
    }
    fn spacing(&mut self) -> TerrainSpacing {
        self.damage.terrain.sampling().spacing
    }
    fn fraction_mask(&mut self) -> TerrainSpacing {
        self.damage.terrain.sampling().spacing
    }
    fn height_shift(&mut self) -> u32 {
        self.damage.terrain.sampling().height_scale.shift()
    }
    fn model(&mut self, slot: u16) -> Result<Option<Self::Model>, Self::Error> {
        let residency: &'a crate::ResidentRuntime = self.damage.residency;
        let own = resident_model_resource(residency, slot)
            .map_err(ResidentFurnitureCollisionError::Object)?;
        match (own, self.model_fallback) {
            (None, Some(fallback)) => resident_model_resource(fallback, slot)
                .map_err(|error| ResidentFurnitureCollisionError::Object(error).into()),
            (own, _) => Ok(own),
        }
    }
    fn model_radius(&self, model: &Self::Model) -> u16 {
        model.model.header.raw_0e
    }
    fn placement_model(&mut self, lookup: TerrainResourceLookup) -> Result<u16, Self::Error> {
        Ok(resident_placement_slot(self.damage.residency, lookup)?.resource_word)
    }
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, Self::Error> {
        // 0047C493: every constructed actor table holds +48 = 004026B0.
        Ok(arena
            .actor(actor)
            .map_err(ResidentFurnitureCollisionError::Object)?
            .rotating
            .basis)
    }
    fn shared_basis(&mut self) -> NativeBasis {
        *self.shared_basis
    }
    fn furniture_basis(&mut self, state: u8) -> NativeBasis {
        self.furniture_draw.basis(state)
    }
    fn qualify_record(
        &self,
        _: Self::Terrain,
        record: TerrainSparseRecordId,
    ) -> FurnitureQueryRecord {
        self.terrain_source.query_record(record)
    }
    fn active_pair(
        &mut self,
        request: FurnitureCollisionPair<'_, Self::Model>,
        process: &mut ProcessFurnitureQueryState,
    ) -> Result<CollisionContact, Self::Error> {
        Ok(resident_active_pair(
            self.damage.residency,
            self.model_process,
            request,
            process,
        )?)
    }
    fn respond_contact(
        &mut self,
        arena: &mut ObjectArena,
        state: FurnitureCollisionPairState,
        contact: CollisionContact,
    ) -> Result<u8, Self::Error> {
        // Selector34/35 tables hold 0047BFD0 at +A8; 10/12/36 hold guard
        // 0042A760, which reaches the same response. The vehicle's table
        // (00520860), the Powerball's (00526464), the Boulders' (0051F900),
        // the ECM Unit's (005262CC) and the Remote Beamer's and its
        // receiver's (005274C4, 00526530) hold 0047BFD0 itself.
        if arena.selector2_actor(state.actor).is_ok()
            || arena.selector57(state.actor).is_ok()
            || arena.selector59(state.actor).is_ok()
            || arena.selector73(state.actor).is_ok()
            || arena.selector55(state.actor).is_ok()
            || arena.is_selector56(state.actor)
            || arena.selector17(state.actor).is_ok()
        {
            return self.respond(arena, state, contact);
        }
        // The rock's 00454190 retires it (0047CE90) and answers AL 1, which
        // ends the pass's furniture walk.
        if arena.selector44(state.actor).is_ok() {
            let null = self.damage.factory_process.process_null_handle;
            arena
                .retire_actor(
                    state.actor,
                    null,
                    &mut world_retirement_host(self.damage, self.rng),
                )
                .map_err(|error| PassFurnitureError::Retirement(format!("{error:?}")))?;
            return Ok(1);
        }
        // The missiles' 00455660, the Bomb's too, detonates an armed one and
        // answers AL 0 without the shared response.
        if arena.is_bomb29(state.actor) {
            arena
                .bomb29_touch(
                    state.actor,
                    self.rng,
                    &mut crate::session_platform::world_actor_update::WorldMissileLifecycle {
                        owners: self.damage,
                    },
                )
                .map_err(|error| PassFurnitureError::Missile(error.to_string()))?;
            return Ok(0);
        }
        if arena.is_missile(state.actor) {
            arena
                .missile_touch(
                    state.actor,
                    self.rng,
                    &mut crate::session_platform::world_actor_update::WorldMissileLifecycle {
                        owners: self.damage,
                    },
                )
                .map_err(|error| PassFurnitureError::Missile(error.to_string()))?;
            return Ok(0);
        }
        // The grenade's 00455880 marks an impact first and answers AL 0.
        if arena.is_thrown(state.actor) {
            arena
                .grenade26_impact(state.actor)
                .map_err(ResidentFurnitureCollisionError::Object)?;
            self.respond(arena, state, contact)?;
            return Ok(0);
        }
        // Selector 66's table (0051F6C0) holds the same 0047BFD0 at +A8.
        if matches!(
            arena.actor(state.actor).map(|actor| &actor.extension),
            Ok(ActorExtension::Selector66(_))
        ) {
            return self.respond(arena, state, contact);
        }
        match arena.family_update_class(state.actor) {
            Ok(FamilyUpdateClass::Selector34 | FamilyUpdateClass::Selector35) => {
                self.respond(arena, state, contact)
            }
            Ok(_) => {
                let mut request = FamilyCollisionGuardRequest {
                    actor: state.actor,
                    continuation_operands: (state, contact),
                };
                match arena.dispatch_family_collision_guard(&mut request, self) {
                    Ok(al) => Ok(al),
                    Err(FamilyCollisionGuardError::Object(error)) => {
                        Err(ResidentFurnitureCollisionError::Object(error).into())
                    }
                    Err(FamilyCollisionGuardError::Host(error)) => Err(error),
                }
            }
            Err(_) => Err(PassFurnitureError::ResponseClass { actor: state.actor }),
        }
    }
}

impl PassFurnitureHost<'_, '_, '_> {
    /// 0047BFD0 with the contact, H11 entry kind and stack cell position.
    fn respond(
        &mut self,
        arena: &mut ObjectArena,
        state: FurnitureCollisionPairState,
        contact: CollisionContact,
    ) -> Result<u8, PassFurnitureError> {
        let slot = resident_placement_slot(self.damage.residency, state.placement_lookup)?;
        let response = FurnitureContactResponse {
            contact,
            kind: slot.parameter_word as u8,
            cell_xz: [state.cell_position[0], state.cell_position[2]],
        };
        let mut services = PassFurnitureDamage {
            damage: self.damage,
            rng: self.rng,
            network_active: self.network_active,
        };
        Ok(arena.respond_furniture_contact(state.actor, response, &mut services)?)
    }
}

impl FamilyCollisionGuardHost<(FurnitureCollisionPairState, CollisionContact)>
    for PassFurnitureHost<'_, '_, '_>
{
    type Error = PassFurnitureError;
    fn continue_collision(
        &mut self,
        arena: &mut ObjectArena,
        request: &mut FamilyCollisionGuardRequest<(FurnitureCollisionPairState, CollisionContact)>,
    ) -> Result<u32, Self::Error> {
        let (state, contact) = request.continuation_operands;
        // 0047BFD0 clears only AL; the guard's callers test that byte.
        self.respond(arena, state, contact).map(u32::from)
    }
}

/// 0047C6E0 and 00436020 as 0047BFD0 reaches them.
struct PassFurnitureDamage<'h, 'a, 'exe> {
    damage: &'h mut WorldDamageOwners<'a, 'exe>,
    rng: &'h mut u32,
    network_active: bool,
}

impl FurnitureResponseHost for PassFurnitureDamage<'_, '_, '_> {
    type Error = String;
    /// Settings `+34` (`00463AB0`), Landscape Damaging.
    fn network_impact_gate(&self) -> NetworkImpactGate {
        if self.network_active {
            NetworkImpactGate::Session(self.damage.factory_process.settings[0x34 / 4])
        } else {
            NetworkImpactGate::Offline
        }
    }
    fn process_null_handle(&self) -> Option<ActorHandle> {
        self.damage.factory_process.process_null_handle
    }
    fn actor_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), String> {
        apply_world_actor_damage(
            arena,
            actor,
            request.packet,
            request.source,
            self.damage,
            self.rng,
        )
        .map_err(|error| error.to_string())
    }
    fn furniture_damage(&mut self, xz: [u16; 2], packet: ActorDamagePacket) -> Result<(), String> {
        FurnitureDamageRuntime {
            network_active: self.network_active,
            resources: &mut ProjectileFurnitureResources::new(
                self.damage.residency,
                self.damage.executable,
            ),
            queue: self.damage.furniture,
        }
        .apply(
            self.damage.terrain,
            FurnitureDamageRequest {
                xz,
                effect: ScaledRadiusEffect {
                    strength: packet.strength,
                    channel: packet.channel,
                    byte_03: packet.byte_03,
                    impulse_magnitude: 0,
                    impulse_vector: None,
                },
            },
            self.rng,
        )
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
    }
}

impl ActorPairHost for WorldContactPassHost<'_, '_> {
    type Error = WorldContactPassError;
    fn accept_pair(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        other: ObjectId,
    ) -> Result<bool, Self::Error> {
        let class = pair_class(arena, actor).ok_or_else(|| unadmitted(arena, actor))?;
        let other_class = pair_class(arena, other).ok_or_else(|| unadmitted(arena, other))?;
        let state = arena.actor(actor)?.word_c4 as i32;
        let required = |address| Err(WorldContactPassError::RequiredPairVirtual { actor, address });
        match class {
            // 00425B90: a ship that meets a soldier with both at +C4 3 loses
            // flag 10000000; a team-0 one takes the soldier aboard, which
            // flashes where it stood and goes (00425BE0..00425C1E).
            PairClass::Family(
                FamilyUpdateClass::Family4236d0
                | FamilyUpdateClass::Selector10
                | FamilyUpdateClass::Selector36,
            ) => {
                if arena.accept_family_rescue(actor, other)? == FamilyRescueAccept::Boarding {
                    arena.board_family_unit(actor, other)?;
                    let position = arena.object(other)?.base.position_words.ok_or(
                        crate::object_runtime::ObjectError::ActorPositionUnknown(other),
                    )?;
                    crate::object_runtime::FactoryStaffHost::flash(self, arena, position)
                        .map_err(WorldContactPassError::Staffing)?;
                    arena.dispose(other)?;
                    return Ok(true);
                }
            }
            // 00468160: a partner holding the current target's handle runs
            // its +54. Only the gate, the factory, the bridge and the
            // course target answer (themselves), reaching their +D0: the
            // gate's 0048BD90, the factory's 0042F170, where a scientist
            // enters it, the bridge's 0040DAB0 and the target's 0043BEA0.
            // Partner +64 is Selector2's.
            PairClass::Family(FamilyUpdateClass::Selector12) => {
                let partner_d0 = match other_class {
                    PairClass::Selector30 => Some(0x0048_bd90),
                    PairClass::Selector15 => Some(0x0042_f170),
                    PairClass::Selector37 => Some(0x0040_dab0),
                    PairClass::Selector39 => Some(0x0043_bea0),
                    _ => None,
                };
                if let Some(address) = partner_d0 {
                    if let Some(reference) = arena.derived_actor(actor)?.target_148 {
                        if arena.native_target(reference)?.handle_04() == arena.actor(other)?.handle
                        {
                            match other_class {
                                // The factory's +D0, where a scientist walks
                                // in; a refusal sets +22E bit 0.
                                PairClass::Selector15 => {
                                    if arena
                                        .scientist_reaches_factory(actor, other, self)
                                        .map_err(|error| {
                                            WorldContactPassError::Staffing(error.to_string())
                                        })?
                                    {
                                        return Ok(true);
                                    }
                                }
                                // The gate's 0048BD90 takes the scientist's
                                // team, and a +18E gate passes it
                                // (0048BE70); it answers 0 either way.
                                PairClass::Selector30 => {
                                    if arena.pelagia_gate_locked(other) {
                                        self.wake_pelagia(arena, other, actor)?;
                                    }
                                    let entry = arena.gate_takes_scientist(other, actor).map_err(
                                        |error| WorldContactPassError::Gate(error.to_string()),
                                    )?;
                                    if let Some(record) = entry.record {
                                        self.gate_record(record)?;
                                    }
                                    if entry.pass {
                                        let (network_active, math) =
                                            (self.network_active, self.math);
                                        arena
                                            .pass_selector30(
                                                other,
                                                actor,
                                                network_active,
                                                math,
                                                &mut ContactGatePass { host: self },
                                            )
                                            .map_err(|error| {
                                                WorldContactPassError::Gate(error.to_string())
                                            })?;
                                    }
                                    arena.scientist_refused(actor)?;
                                }
                                // The bridge's +D0 is 0040DAB0 itself.
                                PairClass::Selector37 => {
                                    if arena.scientist_reaches_bridge(actor, other, self).map_err(
                                        |error| WorldContactPassError::Staffing(error.to_string()),
                                    )? {
                                        return Ok(true);
                                    }
                                }
                                // The course target's 0043BEA0: the same
                                // take, and a full one grants its +7A.
                                PairClass::Selector39 => {
                                    if arena
                                        .scientist_reaches_course_target(actor, other, self)
                                        .map_err(|error| {
                                            WorldContactPassError::Staffing(error.to_string())
                                        })?
                                    {
                                        return Ok(true);
                                    }
                                }
                                _ => return required(address),
                            }
                        }
                    }
                }
                // 004681A7: alive and outside team 0x12, a scientist that
                // touches a vehicle (partner +64) boards it.
                if state > 2
                    && other_class == PairClass::Selector2
                    && arena.actor(actor)?.word_b4 != 0x12
                {
                    return self
                        .board_vehicle(arena, actor, other)
                        .map_err(WorldContactPassError::Boarding);
                }
            }
            // 0043F790: a transport on a rescue takes a scientist aboard.
            PairClass::Family(FamilyUpdateClass::Selector35) => {
                let took = arena
                    .selector35_picks_up(actor, other, self)
                    .map_err(|error| WorldContactPassError::Pickup(error.to_string()))?;
                // Port fix: the rescue completes as its last scientist
                // boards, not when the player later comes near (+240).
                if took && self.mission_completion {
                    let state = arena
                        .selector35_actor(actor)
                        .map_err(|error| WorldContactPassError::Pickup(error.to_string()))?;
                    if let [_, _, Some(count), Some(aboard)] = state.bytes_240 {
                        if count != 0 && aboard == count {
                            let grants = crate::object_runtime::FactoryStaffHost::grants(self);
                            arena
                                .selector35_grant_rescue(actor, &grants)
                                .map_err(|error| {
                                    WorldContactPassError::Pickup(error.to_string())
                                })?;
                        }
                    }
                }
                return Ok(took);
            }
            // 00455610: the owner, for the grenade's first 500 ms.
            PairClass::Grenade26 => return Ok(arena.grenade26_ignores(actor, other)?),
            // 00456010: its thrower while young; a teammate is a contact;
            // anyone else takes the charge.
            PairClass::Charge27 => {
                return arena
                    .charge27_accepts(actor, other)
                    .map_err(|error| WorldContactPassError::Charge27(error.to_string()))
            }
            // 00458DC0: the implant's player takes what it touches.
            PairClass::Missile if arena.is_implant(actor) => {
                return arena
                    .implant_accepts(
                        actor,
                        other,
                        &mut crate::session_platform::world_actor_update::WorldImplantTake {
                            owners: &mut self.damage,
                        },
                    )
                    .map_err(|error| WorldContactPassError::Implant(error.to_string()))
            }
            PairClass::Missile => return Ok(arena.missile_ignores(actor, other)?),
            // 00403630: the ball passes over the vehicle holding it.
            PairClass::Ball => return Ok(arena.ball_held_by(actor, other)?),
            // 00402720 returns AL 0.
            PairClass::Family(FamilyUpdateClass::Selector34)
            | PairClass::Selector2
            | PairClass::Selector11
            | PairClass::Selector60
            | PairClass::Selector66
            | PairClass::Selector77
            | PairClass::Selector30
            | PairClass::Selector15
            | PairClass::Selector37
            | PairClass::Selector39
            | PairClass::Selector59
            | PairClass::Rock
            | PairClass::Bomb
            | PairClass::Selector84
            | PairClass::SharedPrefix => {}
        }
        Ok(false)
    }
    fn react_pair(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        other: ObjectId,
        own_saved: [u16; 3],
        _other_saved: [u16; 3],
    ) -> Result<(), Self::Error> {
        let class = pair_class(arena, actor).ok_or_else(|| unadmitted(arena, actor))?;
        let other_class = pair_class(arena, other).ok_or_else(|| unadmitted(arena, other))?;
        let family = |arena: &mut ObjectArena, host: &mut Self| {
            arena.react_family_pair(
                actor,
                other,
                FamilyPairReactionRequest {
                    rng: &mut *host.rng,
                    sine_quarter: host.sine_quarter,
                    constructor_null: WeightedTargetConstructorNull::Process(
                        &*host.damage.factory_process,
                    ),
                },
            )
        };
        match class {
            PairClass::Family(FamilyUpdateClass::Selector12) => {
                // 00469030: a living mutant scientist (team 0x12, +C4 3)
                // that the vehicle (partner +64, Selector2 alone) knocks
                // faster than 500 dies (0047CE90) before the shared
                // reaction, as a shot one does.
                if arena.knock_kills_scientist(
                    actor,
                    other_class == PairClass::Selector2,
                    own_saved,
                )? {
                    let null = self.damage.factory_process.process_null_handle;
                    let mut host = world_retirement_host(&mut self.damage, self.rng);
                    arena
                        .retire_actor(actor, null, &mut host)
                        .map_err(|error| WorldContactPassError::Retirement(format!("{error:?}")))?;
                }
                family(arena, self)?;
            }
            // 00427C20.
            PairClass::Family(_) => family(arena, self)?,
            // 00427BE0.
            PairClass::Selector2 | PairClass::Selector11 | PairClass::Selector60 => {
                arena.settle_pair_contact(actor)?
            }
            // 00455830: touching an actor whose +68 is null detonates it.
            PairClass::Grenade26 | PairClass::Charge27 => {
                arena.grenade26_touch(actor, other, answers_68(other_class))?;
            }
            // 00455640: an armed missile detonates on any contact.
            PairClass::Missile => arena
                .missile_touch(
                    actor,
                    &mut *self.rng,
                    &mut crate::session_platform::world_actor_update::WorldMissileLifecycle {
                        owners: &mut self.damage,
                    },
                )
                .map_err(|error| WorldContactPassError::Missile(error.to_string()))?,
            // 00407BB0: from state 2 on, a partner whose +68 answers (a
            // grenade, charge, mine, missile, the implant or the Bomb)
            // striking the stalk's tip (+D8, 00408310) marks the eye with its
            // thrower +1B0, which the eye's next update turns into 10,000
            // damage.
            PairClass::Selector77 => {
                if answers_68(other_class) && arena.actor(actor)?.word_c4 as i32 >= 2 {
                    let residency = self.damage.residency;
                    let math = self.math;
                    (|| -> Result<(), String> {
                        let root = arena
                            .selector77_actor(actor)
                            .map_err(|error| error.to_string())?
                            .position_192
                            .ok_or("the eye's +1C has not written its root +192")?;
                        let length = crate::object_runtime::resident_model_metadata(
                            residency,
                            crate::object_runtime::EYE_STALK_MODEL,
                        )
                        .map_err(|error| error.to_string())?
                        .map(|metadata| metadata.radius_word);
                        let tip = arena
                            .selector77_tip(actor, root, length, math)
                            .map_err(|error| error.to_string())?;
                        arena
                            .selector77_struck(actor, other, tip)
                            .map_err(|error| error.to_string())
                    })()
                    .map_err(WorldContactPassError::Eye)?;
                }
            }
            // 00405E30: the boulder's change of speed crushes the partner.
            PairClass::Selector59 => {
                if let Some(packet) = arena.selector59_crush(actor, own_saved)? {
                    let source = arena.actor(actor)?.handle;
                    self.actor_damage(arena, other, ActorDamageRequest { packet, source })?;
                }
            }
            // 00403660: a vehicle takes a free ball; any other contact
            // knocks a held one loose.
            PairClass::Ball => {
                let bytes = self
                    .damage
                    .executable
                    .mapped(0x0053_ff00, 2048)
                    .map_err(|error| WorldContactPassError::Ball(format!("{error:?}")))?
                    .bytes;
                let inverse_sine: [i16; 1024] = std::array::from_fn(|index| {
                    i16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])
                });
                let sine_quarter = self.sine_quarter;
                let mut host = crate::session_platform::world_actor_update::WorldFamilyUpdateHost {
                    owners: self.damage.reborrow(),
                    rng: &mut *self.rng,
                    sine_quarter,
                };
                arena
                    .ball_contact(actor, other, &inverse_sine, &mut host)
                    .map_err(|error| WorldContactPassError::Ball(error.to_string()))?;
            }
            // 00455640: an armed Bomb goes off on any contact.
            PairClass::Bomb => arena
                .bomb29_touch(
                    actor,
                    &mut *self.rng,
                    &mut crate::session_platform::world_actor_update::WorldMissileLifecycle {
                        owners: &mut self.damage,
                    },
                )
                .map_err(|error| WorldContactPassError::Missile(error.to_string()))?,
            // 0043C150: the partner's +5C, which selector 12 alone answers
            // (with itself), hands the node its team.
            PairClass::Selector84 => {
                let capturer = (other_class == PairClass::Family(FamilyUpdateClass::Selector12))
                    .then_some(other);
                arena
                    .touch_selector84(actor, capturer)
                    .map_err(|error| WorldContactPassError::CaptureNode(error.to_string()))?;
            }
            // 00454180: the rock retires (0047CE90).
            PairClass::Rock => {
                let null = self.damage.factory_process.process_null_handle;
                let mut host = world_retirement_host(&mut self.damage, self.rng);
                arena
                    .retire_actor(actor, null, &mut host)
                    .map_err(|error| WorldContactPassError::Retirement(format!("{error:?}")))?;
            }
            // 00405570 returns.
            PairClass::Selector66
            | PairClass::Selector30
            | PairClass::Selector15
            | PairClass::Selector37
            | PairClass::Selector39
            | PairClass::SharedPrefix => {}
        }
        Ok(())
    }
    fn impact_effect(&mut self, position: [u16; 3], strength: i32) -> Result<(), Self::Error> {
        let damage = &mut self.damage;
        damage
            .particles
            .birth_impact_sparks(
                ParticleImpactSparksRequest {
                    position,
                    strength,
                    inputs: ParticleBirthInputs {
                        network_active: damage.factory_process.network_flag != 0,
                        phase: Some(u32::from(damage.session.current_phase().id())),
                        process: damage.factory_process,
                    },
                },
                damage.executable,
                self.rng,
                &mut ProcessParticleSoundQueue {
                    sounds: damage.sounds,
                    residency: damage.residency,
                },
            )
            .map_err(|error| WorldContactPassError::ImpactEffect(format!("{error:?}")))
    }
    fn actor_damage(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        let paced = self.paced_skip
            && matches!(
                arena.family_update_class(actor),
                Ok(crate::object_runtime::FamilyUpdateClass::Selector10
                    | crate::object_runtime::FamilyUpdateClass::Selector12
                    | crate::object_runtime::FamilyUpdateClass::Selector35
                    | crate::object_runtime::FamilyUpdateClass::Selector36)
            );
        let request_was = paced
            .then(|| {
                arena
                    .family428650_actor(actor)
                    .ok()
                    .map(|state| state.word_22e & 1 != 0)
            })
            .flatten();
        apply_world_actor_damage(
            arena,
            actor,
            request.packet,
            request.source,
            &mut self.damage,
            self.rng,
        )?;
        if let Some(was) = request_was {
            if arena.object(actor).is_ok() {
                let _ = arena.keep_choice_request(actor, was);
            }
        }
        Ok(())
    }
}

impl WorldPairHost for WorldContactPassHost<'_, '_> {
    fn pair_contact(
        &mut self,
        arena: &ObjectArena,
        a: ObjectId,
        b: ObjectId,
        delta: [i16; 3],
    ) -> Result<Option<CollisionContact>, Self::Error> {
        let (residency, fallback) = (self.damage.residency, self.model_fallback);
        // 0047B66F..9C: both lookups precede both header tests.
        let model = |actor: ObjectId| -> Result<_, WorldContactPassError> {
            let slot = arena.actor(actor)?.rotating.visual.model_word;
            contact_model(residency, fallback, slot)?
                .ok_or(WorldContactPassError::MissingModel { actor, slot })
        };
        let (model_a, model_b) = (model(a)?, model(b)?);
        if model_a.model.header.raw_0e == 0 || model_b.model.header.raw_0e == 0 {
            return Ok(None);
        }
        for actor in [a, b] {
            pair_class(arena, actor).ok_or_else(|| unadmitted(arena, actor))?;
        }
        let qualify =
            |binding: &crate::object_runtime::ResidentModelBinding<'_>| AuxiliaryRawSource {
                registration: binding.registration.clone(),
                allocation: Arc::downgrade(binding.allocation),
                vertices_span: binding.model.vertices_span,
            };
        let (qualified_a, qualified_b) = (qualify(&model_a), qualify(&model_b));
        let values = |source| {
            AuxiliaryValueSource::Object(ObjectModelCallbacks {
                arena,
                source,
                animation: self.damage.animation_clock,
                executable: self.damage.executable,
            })
        };
        let mut contact = CollisionContact::unwritten();
        let result = query_auxiliary_actor_pair(
            AuxiliaryActorPairRequest {
                actor_origin: delta.map(i32::from),
                actor_basis: arena.actor(a)?.rotating.basis,
                actor_model: AuxiliaryModelSource {
                    model: model_a.model,
                    qualification: &qualified_a,
                },
                actor_values: values(a),
                counterpart_basis: arena.actor(b)?.rotating.basis,
                counterpart_model: AuxiliaryModelSource {
                    model: model_b.model,
                    qualification: &qualified_b,
                },
                counterpart_values: values(b),
                residency: Some(residency),
            },
            self.furniture_query,
            self.model_process,
            &mut contact,
        );
        if result
            .as_ref()
            .is_err_and(AuxiliaryModelError::is_required_feature_boundary)
        {
            if let ModelScratchDrawState::Stopped(scope) = self.model_process.scratch.state()? {
                self.model_process
                    .scratch
                    .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)?;
            }
        }
        result?;
        // 0047B7E8: any nonzero flag DWORD is a contact.
        Ok(contact
            .flag()
            .is_some_and(|flag| flag != 0)
            .then_some(contact))
    }
}

/// 004790C0 after 004791F0. A failed pair is logged once and skipped until
/// world replacement; its reached writes stay.
pub(crate) fn run_world_pair_pass(
    arena: &mut ObjectArena,
    stops: &mut WorldActorUpdateState,
    host: &mut WorldContactPassHost<'_, '_>,
) {
    let stopped = std::mem::take(&mut stops.pair_contact_stops);
    let mut fresh = Vec::new();
    arena.run_world_pair_pass(
        host,
        |a, b| !stopped.iter().any(|(x, y, _)| (*x, *y) == (a, b)),
        |a, b, error: ActorPairError<WorldContactPassError>| {
            let reason = error.to_string();
            eprintln!("World pair contact stopped after reached writes: {a:?}/{b:?}: {reason}");
            fresh.push((a, b, reason));
        },
    );
    stops.pair_contact_stops = stopped;
    stops.pair_contact_stops.extend(fresh);
}

/// 004791F0 over the G+48 actor list. Each phase failure is logged once and
/// stops only that phase of that actor until world replacement.
pub(crate) fn run_world_contact_pass(
    arena: &mut ObjectArena,
    stops: &mut WorldActorUpdateState,
    host: &mut WorldContactPassHost<'_, '_>,
) {
    let actors = arena.actor_list().to_vec();
    for actor in actors {
        // A response may retire an actor; later list entries still run.
        if arena.actor(actor).is_err() {
            continue;
        }
        let phases = WorldContactPhases {
            terrain: !stops
                .terrain_contact_stops
                .iter()
                .any(|(stopped, _)| *stopped == actor),
            furniture: !stops
                .furniture_contact_stops
                .iter()
                .any(|(stopped, _)| *stopped == actor),
        };
        if let Err(error) = arena.run_world_contact(actor, phases, host) {
            let reason = error.to_string();
            if error.is_furniture() {
                eprintln!(
                    "World furniture contact stopped after reached writes: {actor:?}: {reason}"
                );
                stops.furniture_contact_stops.push((actor, reason));
            } else {
                eprintln!(
                    "World terrain contact stopped after reached writes: {actor:?}: {reason}"
                );
                stops.terrain_contact_stops.push((actor, reason));
            }
        }
    }
}
