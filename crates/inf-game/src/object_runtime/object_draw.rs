//! Flag0 object drawing, complete manager 00479570 and represented near policies.
//!
//! The same embedded node moves before fresh fields, terrain and virtual policy;
//! only normal drain exchanges physical header owners and publishes53B890.
//! Mutable terrain services preserve native acquisition/read ordering. Model
//! services borrow the captured source and current caller context, retaining
//! their command/RNG prefix on error. Immutable lighting/executable loans and
//! exclusive arena/context custody exclude arbitrary native alias mutation.
//! Flash consumes actual retained save fields and reached material lookup;
//! offline vehicle drawing shares tint, Common and retained profiling custody.
//! Network vehicle/other custom phases remain explicit required owners. Omission
//! retains each current object's prefix without a successful native return.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::{
    mark_flag_change, ActorExtension, ActorTerrainHost, DerivedActorClass, DerivedActorError,
    ObjectArena, ObjectClass, ObjectError, ObjectId, ObjectList, ObjectModelCallbackError,
    ObjectNode, ObjectNodeKind,
};
use crate::{
    frontend_scene::{SceneCommand, SceneFade, SceneView},
    menu::MenuBasis,
    model_execution::ModelRootMode,
    terrain_runtime::{spacing_pair, TerrainRuntime, TerrainSpacing},
    world_lighting::WorldLightGrid,
};

mod flash;
#[cfg(test)]
mod tests;
mod vehicle;

/// Packet520A44: model geometry and captured object+3C are distinct owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectModelPacket {
    pub source: ObjectId,
    pub model_slot: i16,
    pub position: [i32; 3],
    pub basis: MenuBasis,
}

/// What the name tag over a network vehicle reads (`004205CF..004207C6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleNameTag {
    /// `+32`.
    pub position: [u16; 3],
    /// The owner field of `+84` (bits 27..31), whose initials show.
    pub slot: u8,
    /// `+B4 + 0x25`: the H6 style that colours it.
    pub style: i32,
}

/// Actual caller context. Manager visibility captures entry far once; near
/// callbacks mutate this same context independently of published globals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectDrawContext {
    pub view: SceneView,
    pub near: i32,
    pub far: i32,
    pub far_color: u32,
}

/// Process-global custody, not a new per-world renderer preset. Startup439630
/// initializes547590; later unowned native writers are outside this live owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectDrawProcessState {
    pub completion_53b890: u8,
    pub shared_basis: MenuBasis,
    /// 0046D3E0 startup (the CRT jump 0046D3D0); 0046D940 is the sole
    /// admitted runtime writer.
    pub pickup_basis: MenuBasis,
    pub profiling: super::ProcessProfiling,
}

impl ObjectDrawProcessState {
    pub fn native_startup(
        executable: FrontendExecutable<'_>,
    ) -> Result<Self, FrontendExecutableError> {
        Ok(Self {
            completion_53b890: executable.mapped(0x0053_b890, 1)?.bytes[0],
            //00439630 ->0040D6A0. No per-call identity replacement belongs to
            //the static visual+48 leaf401390, which reads this retained basis.
            shared_basis: MenuBasis::INITIAL,
            pickup_basis: MenuBasis::INITIAL,
            profiling: super::ProcessProfiling::native_startup(executable)?,
        })
    }
    /// 0046D940, reached at the end of 0048E2E0: signed frame input SAR6.
    pub fn advance_pickup_rotation(
        &mut self,
        microseconds: i32,
        math: &inf_formats::native_basis::NativeBasisMath,
    ) {
        let mut basis = inf_formats::native_basis::NativeBasis {
            words: self.pickup_basis.words,
        };
        math.rotate(&mut basis, microseconds >> 6);
        self.pickup_basis.words = basis.words;
    }
}

/// Current process lighting loan. Vehicle publication and47AFC0 share the
/// same mutable tint word. Settings+80 does not gate47AFC0;
/// its fresh object flag80 does. No grid clear or anchor publication occurs.
pub struct ObjectDrawLighting<'a> {
    pub grid: &'a WorldLightGrid,
    pub tint_word: &'a mut u32,
    pub direction: [f32; 3],
    pub spacing: TerrainSpacing,
    pub executable: FrontendExecutable<'a>,
}

pub struct ObjectModelDrawSubmission<'a> {
    /// The current manager lends its real arena for the duration of this model.
    /// Source+10 writes must be visible to subsequent callbacks and objects.
    pub arena: &'a mut ObjectArena,
    pub packet: ObjectModelPacket,
    pub root: ModelRootMode,
    pub context: &'a ObjectDrawContext,
}

/// Complete scene/model phases, not arbitrary successful virtual callbacks.
/// The production model service owns the actual live clock/source/queue/RNG.
pub trait Flag0ObjectDrawHost {
    type Error;

    fn scene(&mut self, command: SceneCommand) -> Result<(), Self::Error>;
    fn attract_demo(&mut self) -> u8;
    /// 004391B0()+64C, the UI state selector 42's draw reads.
    fn ui_state(&mut self) -> Option<u32> {
        None
    }
    /// Live settings +7C, which a cannon shot's draw reads through its
    /// target refresh. None stops that draw.
    fn settings_7c(&mut self) -> Option<u32> {
        None
    }
    /// The live process null 0054E894, which the charge's draw compares
    /// with its +154. None stops that draw.
    fn process_null_handle(&mut self) -> Option<crate::object_runtime::ActorHandle> {
        None
    }
    /// The sine quarter table's basis turns, which the flamethrower's draw
    /// reaches through 0047A720. None stops that draw.
    fn basis_math(&mut self) -> Option<inf_formats::native_basis::NativeBasisMath> {
        None
    }
    /// Settings +00..+4F as the network game reads them (the process
    /// words' copy), which a network vehicle's draw reads. None stops it.
    fn network_settings(&mut self) -> Option<[u32; 20]> {
        None
    }
    /// The resident models a draw's own model change refreshes from
    /// (0047A110), as Powerball's vehicles' does. None stops that draw.
    fn resident_models(&mut self) -> Option<&crate::ResidentRuntime> {
        None
    }
    /// The host's team flag `0054E81C`, which Powerball's vehicles' draw
    /// reads. None stops that draw.
    fn network_team_flag(&mut self) -> Option<u8> {
        None
    }
    /// `004205F7..004207C6`, the name tag: the slot's initials (`0054E71C`,
    /// "%5.5s") in the owner's colour, at the vehicle's point projected
    /// through the view, as a sorted overlay item (`004A42B0` with
    /// `00423370`, which the renderer's `+0C`, `004A3AB0`, draws).
    fn name_tag(
        &mut self,
        _tag: VehicleNameTag,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::VehicleNameTag,
        ))
    }
    /// The H6 style word `argument` (0055EEC8, loaded if absent).
    fn style_word(&mut self, _argument: i32) -> Result<u32, ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::VehicleNetwork,
        ))
    }
    /// The sine quarter table 0053DF00, which the zone's draw reads inline.
    /// None stops that draw.
    fn sine_quarter(&mut self) -> Option<[u16; 4096]> {
        None
    }
    /// 00440640's 00490030: the H12 light (+4/+8/+C, X and Z quartered by
    /// signed division) normalized like 0044FE17, in the style's colour.
    fn gate_light(
        &mut self,
        _style: u16,
    ) -> Result<SceneCommand, ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::GateLight,
        ))
    }
    /// Actual reached H7 lookup. Native checks/indexing consume signed
    /// LOWWORD; the complete supplied argument remains visible to the host.
    fn material_color(&mut self, argument: i32)
        -> Result<u32, ObjectDrawServiceError<Self::Error>>;
    fn model(
        &mut self,
        request: ObjectModelDrawSubmission<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>>;
    ///0048CDB0 consumes the very same actor+140 owner after native draw
    ///callbacks capture weapon positions. The host lends current process RNG.
    fn drain_weapon_fire(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<usize, ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::MissingWeaponFiringServices,
        ))
    }
    /// 00486B30 for a ribbon with two or more nodes.
    fn draw_ribbon(
        &mut self,
        _arena: &mut ObjectArena,
        _ribbon: ObjectId,
        _context: &ObjectDrawContext,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::Selector67Ribbon,
        ))
    }
    /// 004777A0, the Force Field's dome, after its +2C 00477370 has set the
    /// light, while the field is on.
    fn draw_force_field(
        &mut self,
        _arena: &mut ObjectArena,
        _field: ObjectId,
        _context: &ObjectDrawContext,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::Selector18ForceField,
        ))
    }
    /// 00485E60's 0048FE60 and beam 00485520, after the laser's pose and
    /// reach: its quads, which draw the RNG.
    fn draw_laser(
        &mut self,
        _arena: &mut ObjectArena,
        _laser: ObjectId,
        _context: &ObjectDrawContext,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::Selector31Laser,
        ))
    }
    /// 00411570, the barrier's +2C, over its bolts in order: a bolt on the
    /// ground births its spark (004894A0), then each bolt deeper than 0x40
    /// is joined to both posts by beams.
    fn draw_barrier(
        &mut self,
        _arena: &mut ObjectArena,
        _barrier: ObjectId,
        _context: &ObjectDrawContext,
        _bolts: &[super::Selector69Bolt],
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::Selector69Barrier,
        ))
    }
    /// 0046FB50 after its 0047AFC0, a race gate's posts, each a top and a
    /// foot in draw order.
    fn draw_gate(
        &mut self,
        _arena: &mut ObjectArena,
        _gate: ObjectId,
        _context: &ObjectDrawContext,
        _posts: &[([u16; 3], [u16; 3])],
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::Selector72Gate,
        ))
    }
    /// 0042D7D0, the zone's +2C: particle type 2 at each point in order
    /// (00489A90(2, point, 0)).
    fn draw_zone(
        &mut self,
        _arena: &mut ObjectArena,
        _zone: ObjectId,
        _points: &[[u16; 3]],
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::Selector74Zone,
        ))
    }
    /// 0046DAF0 owns collection, oriented particle emission and model policy.
    fn draw_timed_pickup(
        &mut self,
        _arena: &mut ObjectArena,
        _object: ObjectId,
        _context: &mut ObjectDrawContext,
        _process: &mut ObjectDrawProcessState,
        _lighting: &ObjectDrawLighting<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        Err(ObjectDrawServiceError::UnownedCallback(
            ObjectDrawCallbackBoundary::MissingPickupServices,
        ))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObjectDrawServiceError<E> {
    /// Reached required model fields/operations belong here only with
    /// recoverable queue custody. Queue/flush/custody errors remain Backend.
    UnownedCallback(ObjectDrawCallbackBoundary),
    Backend(E),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flag0ObjectDrawFailurePolicy {
    #[default]
    Stop,
    /// Retain the reached node/flags/context/model prefix, omit only this
    /// object's remaining draw callback phases, then read the fresh head.
    UnownedObjectDrawCallbackApproximation,
}

pub struct Flag0ObjectDrawRequest<'a, T, H> {
    pub context: &'a mut ObjectDrawContext,
    /// The near-callback cone: retail's, or the view's own where the
    /// remaster's game or the view shown draws it.
    pub cone: ObjectDrawCone,
    pub process: &'a mut ObjectDrawProcessState,
    /// Fresh full-DWORD55F448; this typed loan excludes arbitrary native aliases.
    pub network_flag: &'a u32,
    pub terrain: &'a mut T,
    pub lighting: ObjectDrawLighting<'a>,
    pub host: &'a mut H,
    pub failure_policy: Flag0ObjectDrawFailurePolicy,
}

/// The cone of manager `00479570`: an object reaches its near callback when its squared
/// distance is below twice the square of its depth along the view (plus its
/// extent), a 45-degree cone. Every original tier's focal is half its width,
/// so the cone matches the display's horizontal field of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectDrawCone {
    /// Retail's cone, with its native DWORD wrapping.
    Retail,
    /// Port option, the view a frame draws: on each axis (across, down)
    /// `half` pixels of it from the centre to its farther edge over that
    /// axis's `focal`. Its corners reach furthest, so the cone is
    /// `1 + (half[0] / focal[0])^2 + (half[1] / focal[1])^2` times the
    /// squared depth.
    View { half: [u32; 2], focal: [u32; 2] },
}

impl ObjectDrawCone {
    /// The cone of the view `projection` draws, out to its corners.
    /// Retail's tiers draw half their width at a focal of half their width
    /// and 3/4 of that down: a view reaching no further on either axis is
    /// retail's, and keeps retail's cone and with it retail's frame (what
    /// its corners show beyond the cone stays undrawn, as in retail). The
    /// Remastered camera's narrower focal (and its speed kick) and a
    /// widescreen frame reach further.
    pub fn for_projection(projection: crate::model_context::ModelProjection) -> Self {
        let half = [0, 1].map(|axis| {
            let [size, centre] = [projection.viewport[axis], projection.center[axis]];
            centre.max(size.wrapping_sub(centre)).max(0).unsigned_abs()
        });
        let focal = projection.focal.map(|focal| focal.max(1).unsigned_abs());
        if half[0] <= focal[0] && u64::from(half[1]) * 4 <= u64::from(focal[1]) * 3 {
            Self::Retail
        } else {
            Self::View { half, focal }
        }
    }

    /// Whether `squared` (the distance squared) lies inside the cone of a
    /// positive `depth`.
    fn admits(self, squared: u32, depth: i32) -> bool {
        match self {
            Self::Retail => squared < depth.wrapping_mul(depth).wrapping_mul(2) as u32,
            Self::View { half, focal } => {
                let [hx, hy] = half.map(u128::from);
                let [fx, fy] = focal.map(|focal| u128::from(focal) * u128::from(focal));
                let depth = depth.unsigned_abs() as u128;
                u128::from(squared) * fx * fy
                    < depth * depth * (fx * fy + hx * hx * fy + hy * hy * fx)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObjectDrawCallbackBoundary {
    Selector66Stopped,
    /// The behavior registry's resource classes (the actors'
    /// 0047D910/00456180/00457620/00485E60/004860C0) without the services
    /// their draws read. 0048D820's objects leave the flag-0 list before any
    /// draw pass.
    BehaviorResource,
    /// The flamethrower's draw (004323B0) turns its basis, which needs the
    /// sine table.
    FlamethrowerBasis,
    /// The ribbon's own +2C 00486B30 with two or more nodes.
    Selector67Ribbon,
    /// A reached ribbon draw that stopped: zero lengths or unwritten bytes.
    Selector67RibbonDraw {
        reason: String,
    },
    /// The Force Field's dome 004777A0.
    Selector18ForceField,
    /// A reached dome that stopped: an unwritten projection byte, a zero
    /// divisor or a missing world input.
    Selector18ForceFieldDraw {
        reason: String,
    },
    /// The laser's beam 00485520.
    Selector31Laser,
    /// A reached beam that stopped: an unwritten aim or projection byte.
    Selector31LaserDraw {
        reason: String,
    },
    /// The barrier's own +2C 00411570.
    Selector69Barrier,
    /// A reached barrier draw that stopped: an unwritten field, a zero
    /// divisor or an unprojected byte.
    Selector69BarrierDraw {
        reason: String,
    },
    /// A race gate's posts, `0046FB50`.
    Selector72Gate,
    /// A reached gate draw that stopped.
    Selector72GateDraw {
        reason: String,
    },
    /// The zone's own +2C 0042D7D0.
    Selector74Zone,
    /// The zone's draw turns its points by its timer +13C, which case 0x4A
    /// leaves to allocator residue: held while that start is unknown.
    Selector74ZoneTimer,
    /// A reached zone draw that stopped: an unwritten field or no sine
    /// table.
    Selector74ZoneDraw {
        reason: String,
    },
    /// The ball's 00403D30 placing a held ball: an unwritten field or no
    /// sine table.
    BallDraw {
        reason: String,
    },
    StoppedConstruction,
    MissingWeaponFiringServices,
    MissingPickupServices,
    Pickup {
        reason: String,
    },
    WeaponFire {
        reason: String,
    },
    /// The charge's draw 00456180: the process null or a field it reads.
    Charge27 {
        reason: String,
    },
    Position,
    TerrainLightStorage,
    /// Selector 42's draw needs the UI state.
    UiState,
    /// The course gate's draw needs the H12 light and its style.
    GateLight,
    ActorFlashField {
        offset: u16,
    },
    /// A network vehicle's draw without the session's settings.
    VehicleNetwork,
    /// Powerball's (mode 4) vehicle model swaps, 00420551..004205A2,
    /// without the resident models or the network game's team flag.
    VehiclePowerball,
    /// The name tag over a network vehicle that moved lately (settings +2C,
    /// vehicle +5A0) without its host's draw: 004A42B0's overlay item,
    /// 004205B2..004207C6.
    VehicleNameTag,
    /// The name tag's draw failed.
    VehicleNameTagDraw {
        reason: String,
    },
    CustomVertex {
        object: ObjectId,
        selector: i32,
        reason: super::ObjectCustomVertexError,
    },
    ModelValue(ObjectModelCallbackError),
    /// Named per-model containment, not a successful native root return.
    /// The host must authenticate recoverable queue custody before choosing
    /// this arm for a required resource, opcode or vertex/value boundary.
    ModelFeature {
        model_slot: i16,
        reason: String,
    },
    MaterialFeature {
        argument: i32,
        reason: String,
    },
    /// Port option: an object's draw in the view shown, drawn apart from
    /// the game's, wrote a runtime allocation it must leave
    /// ([`super::refusing_writes`]).
    ShownDrawWrite,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Flag0ObjectDrawOmission {
    pub object: ObjectId,
    pub reason: ObjectDrawCallbackBoundary,
}

/// Portable invocation diagnostics, not original allocation/callback counters.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Flag0ObjectDrawOutcome {
    pub visited: usize,
    pub near_callbacks: usize,
    pub common_models: usize,
    pub late_models: usize,
    pub empty_behaviors: usize,
    pub firing_drains: usize,
    pub emitted_shots: usize,
    pub omissions: Vec<Flag0ObjectDrawOmission>,
    /// Port option: the objects inside the pass's radius whose near
    /// callback would draw but that its cone turned away (behind the eye,
    /// beside the view), in the order the pass met them: the tracer's
    /// models outside the view ([`ObjectArena::draw_near_callbacks`]). It
    /// decides nothing in the pass.
    pub culled: Vec<CulledObject>,
}

/// An object the pass's cone turned away: where it was, eye-relative in
/// world units (the cone's own differences, Y up), and its extent `+28`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CulledObject {
    pub id: ObjectId,
    pub offset: [i32; 3],
    pub extent: i32,
}

#[derive(Debug, Error)]
pub enum Flag0ObjectDrawError<T, H> {
    #[error(transparent)]
    ActorConstruction(#[from] super::ActorConstructionError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Profiling(#[from] super::ProfilingError),
    #[error("draw pass selected unavailable embedded Flag0 node {0:?}")]
    NodeCustody(ObjectNode),
    #[error("object {object:?} draw callback requires {reason:?}")]
    UnownedCallback {
        object: ObjectId,
        reason: ObjectDrawCallbackBoundary,
    },
    #[error("current object terrain acquisition failed: {0}")]
    Terrain(T),
    #[error("object draw backend failed: {0}")]
    Host(H),
    #[error(transparent)]
    Executable(#[from] FrontendExecutableError),
}

impl ObjectArena {
    pub fn run_flag0_object_draw_pass<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        mut request: Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<Flag0ObjectDrawOutcome, Flag0ObjectDrawError<T::Error, H::Error>> {
        let mut outcome = Flag0ObjectDrawOutcome::default();
        crate::event_log::object_draw(request.context.view.eye, request.context.far);
        //47957B..59E captures entry far and low-WORD eye, independently of
        //the live far later changed by47D910 or consumed by selector86.
        let entry_far = request.context.far;
        let eye = request.context.view.eye.map(|value| value as u16);
        while let Some(&node) = self.list(ObjectList::Flag0Primary).first() {
            let owned = self.owned(node.object)?;
            if node.kind != ObjectNodeKind::Flag0
                || owned.memberships[ObjectNodeKind::Flag0 as usize]
                    != self.list_owners[ObjectList::Flag0Primary as usize]
            {
                return Err(Flag0ObjectDrawError::NodeCustody(node));
            }
            // 0047F7F0 links the node before the secondary list's tail.
            self.move_node(
                node.object,
                ObjectNodeKind::Flag0,
                ObjectList::Flag0Secondary,
            );
            outcome.visited += 1;
            crate::crash_log::step_object(crate::crash_log::Step::ObjectDraw, node.object.slot());
            super::allocation::take_refused();
            let result =
                self.draw_flag0_object(node.object, eye, entry_far, &mut request, &mut outcome);
            match result {
                Err(Flag0ObjectDrawError::UnownedCallback { object, reason })
                    if request.failure_policy
                        == Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation =>
                {
                    outcome
                        .omissions
                        .push(Flag0ObjectDrawOmission { object, reason });
                }
                // Port option: in the view shown, drawn apart from the game's
                // (only there are writes refused), a refused write ends this
                // object's draw, not the pass's.
                Err(_) if super::allocation::take_refused() => {
                    outcome.omissions.push(Flag0ObjectDrawOmission {
                        object: node.object,
                        reason: ObjectDrawCallbackBoundary::ShownDrawWrite,
                    });
                }
                Err(error) => return Err(error),
                Ok(()) => {}
            }
        }
        self.swap_flag0_lists();
        request.process.completion_53b890 = 1; //479827, only normal drain.
        Ok(outcome)
    }

    /// Port option, not retail: the near callbacks (`+2C`) of `objects` (a
    /// pass's [`Flag0ObjectDrawOutcome::culled`]), each admitted as the
    /// manager admits one, for the tracer's feed alone. Those callbacks
    /// write game state, so the world draw runs this only on its copies of
    /// the game (`world_scene`'s `GameAsDrawn`), whose runtime allocations
    /// refuse writes: an object whose draw meets one is left out, as the
    /// view shown leaves it. One a later draw took off the list is passed
    /// over.
    pub fn draw_near_callbacks<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        objects: &[ObjectId],
        mut request: Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<Flag0ObjectDrawOutcome, Flag0ObjectDrawError<T::Error, H::Error>> {
        let mut outcome = Flag0ObjectDrawOutcome::default();
        for &source in objects {
            if !self
                .list(ObjectList::Flag0Primary)
                .iter()
                .any(|node| node.object == source)
            {
                continue;
            }
            crate::crash_log::step_object(crate::crash_log::Step::ObjectDraw, source.slot());
            let flags = self.object(source)?.base.flags | 0x0200_0000;
            mark_flag_change(&mut self.owned_mut(source)?.state.base, flags);
            outcome.near_callbacks += 1;
            super::allocation::take_refused();
            match self.draw_near_callback(source, &mut request, &mut outcome) {
                Err(Flag0ObjectDrawError::UnownedCallback { object, reason })
                    if request.failure_policy
                        == Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation =>
                {
                    outcome
                        .omissions
                        .push(Flag0ObjectDrawOmission { object, reason });
                }
                Err(_) if super::allocation::take_refused() => {
                    outcome.omissions.push(Flag0ObjectDrawOmission {
                        object: source,
                        reason: ObjectDrawCallbackBoundary::ShownDrawWrite,
                    });
                }
                Err(error) => return Err(error),
                Ok(()) => {}
            }
        }
        Ok(outcome)
    }

    /// Port option, not retail: the first player's craft when its view is
    /// first person (its view mode `+D0` above 0, where 00420510 draws no
    /// hull). It changes nothing.
    pub fn first_person_craft(&self) -> Option<ObjectId> {
        let (vehicle, _) = self.first_player_drive()?;
        (self.actor(vehicle).ok()?.byte_d0 as i8 > 0).then_some(vehicle)
    }

    /// Port option, not retail: [`Self::first_person_craft`] with its view
    /// mode cleared, so its draw lays the hull as from behind. Only for a
    /// copy of the game ([`Self::draw_near_callbacks`]): the tracer's feed
    /// takes the hull, which the eye's rays skip.
    pub fn show_first_person_hull(&mut self) -> Option<ObjectId> {
        let vehicle = self.first_person_craft()?;
        self.actor_mut(vehicle).ok()?.byte_d0 = 0;
        Some(vehicle)
    }

    /// Whether `source`'s near callback would draw something
    /// ([`Self::draw_near_callback`]): not the classes it only parks or
    /// lets fire, a ribbon without a second node, a light whose gate is shut.
    fn near_callback_draws(&self, source: ObjectId) -> Result<bool, ObjectError> {
        Ok(match &self.object(source)?.class {
            ObjectClass::Base
            | ObjectClass::Selector81(_)
            | ObjectClass::Selector94(_)
            | ObjectClass::Selector75(_)
            | ObjectClass::MapAnnotation(_)
            | ObjectClass::Selector86(_)
            | ObjectClass::Selector42(_) => false,
            ObjectClass::Selector67(state) => state.nodes.len() >= 2,
            ObjectClass::TerrainLight { light, .. }
            | ObjectClass::RotatingTerrainLight { light, .. } => {
                light.as_ref().is_some_and(|light| light.draw_gate != 0)
            }
            _ => true,
        })
    }

    fn draw_flag0_object<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        eye: [u16; 3],
        entry_far: i32,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        if self.construction_is_stopped(source)? {
            return Err(Flag0ObjectDrawError::UnownedCallback {
                object: source,
                reason: ObjectDrawCallbackBoundary::StoppedConstruction,
            });
        }
        //Whole XYZ containment precedes clear: no native per-axis fault claim.
        let [x, y, z] = self.draw_position(source)?;
        let flags = self.object(source)?.base.flags & !0x0200_0000;
        mark_flag_change(&mut self.owned_mut(source)?.state.base, flags);
        let dx = i32::from(x.wrapping_sub(eye[0]) as i16);
        let dz = i32::from(z.wrapping_sub(eye[2]) as i16);
        let horizontal = dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)) as u32;
        let mut radius = i32::from(self.object(source)?.base.word_28).wrapping_add(entry_far);
        let terrain = request
            .terrain
            .terrain(self, source)
            .map_err(Flag0ObjectDrawError::Terrain)?;
        let excess = ground_excess(y, x, z, terrain);
        if excess > 0 {
            radius = radius.wrapping_add(excess.wrapping_mul(6));
        }
        if horizontal < radius.wrapping_mul(radius) as u32 {
            //4796A8 fresh Y after the acquisition, but saved X/Z differences.
            let current_y = self.draw_position(source)?[1];
            let dy = i32::from(current_y.wrapping_sub(eye[1]) as i16);
            let words = request.context.view.basis.words;
            let coefficient = |axis: usize| f64::from(f32::from_bits(words[7 + axis]));
            let dot = (f64::from(dy) * coefficient(1) + f64::from(dz) * coefficient(2))
                + f64::from(dx) * coefficient(0);
            let dot = super::actor::native_trunc_low32(dot)
                .wrapping_add(i32::from(self.object(source)?.base.word_28));
            let squared = horizontal.wrapping_add(dy.wrapping_mul(dy) as u32);
            if dot > 0 && request.cone.admits(squared, dot) {
                let flags = self.object(source)?.base.flags | 0x0200_0000;
                mark_flag_change(&mut self.owned_mut(source)?.state.base, flags);
                outcome.near_callbacks += 1;
                self.draw_near_callback(source, request, outcome)?;
            } else if self.near_callback_draws(source)? {
                outcome.culled.push(CulledObject {
                    id: source,
                    offset: [dx, dy, dz],
                    extent: i32::from(self.object(source)?.base.word_28),
                });
            }
        }
        //Near callback precedes fresh original-source pending and+18.
        if self.object(source)?.base.flags & 0x0100_0000 == 0 {
            return Ok(());
        }
        let Some(component) = self.draw_component_18(source)? else {
            return Ok(()); //NULL preserves source pending and skips+7C.
        };
        if self.object(component)?.base.flags & 0x0200_0000 == 0 {
            let packet = self.draw_packet(component, request.process)?;
            submit_model(self, packet, ModelRootMode::ManagerLate, request)?;
            outcome.late_models += 1;
        }
        let flags = self.object(source)?.base.flags & !0x0100_0000;
        mark_flag_change(&mut self.owned_mut(source)?.state.base, flags);
        if self.actor_behavior_presence_7c(component)? {
            self.draw_weapon_fire(component, request, outcome)?;
        }
        Ok(())
    }

    fn draw_weapon_fire<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        actor: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        if self
            .actor_behavior_storage(actor)?
            .as_ref()
            .is_some_and(|behavior| behavior.fire_head_00.is_none())
        {
            //The native empty branch does not acquire factory, audio or RNG.
            outcome.empty_behaviors += 1;
            return Ok(());
        }
        outcome.firing_drains += 1;
        let count = request
            .host
            .drain_weapon_fire(self, actor)
            .map_err(|error| match error {
                ObjectDrawServiceError::UnownedCallback(reason) => unowned(actor, reason),
                ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
            })?;
        outcome.emitted_shots += count;
        Ok(())
    }

    fn draw_near_callback<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        match &self.object(source)?.class {
            // 00409990 (00479E90(0)): these tables' draw only parks them.
            ObjectClass::Base
            | ObjectClass::Selector81(_)
            | ObjectClass::Selector94(_)
            | ObjectClass::Selector75(_)
            | ObjectClass::MapAnnotation(_) => {
                self.set_flag0(source, false)?;
                Ok(())
            }
            ObjectClass::Selector86(_) => {
                self.set_selector86_draw_distance(source, request.context.far)?;
                Ok(())
            }
            // 00486B30 returns before any work without a second node.
            ObjectClass::Selector67(state) if state.nodes.len() < 2 => Ok(()),
            ObjectClass::Selector67(_) => request
                .host
                .draw_ribbon(self, source, request.context)
                .map_err(|error| match error {
                    ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                    ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
                }),
            ObjectClass::TimedPickup(_) => request
                .host
                .draw_timed_pickup(
                    self,
                    source,
                    request.context,
                    request.process,
                    &request.lighting,
                )
                .map_err(|error| match error {
                    ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                    ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
                }),
            // 00461C30/00461C60: the gate, then 0047AFC0, which takes the
            // basis from +48: selector 19's the shared one, selector 54's
            // its own.
            ObjectClass::TerrainLight { light, .. }
            | ObjectClass::RotatingTerrainLight { light, .. } => {
                let gate = light
                    .as_ref()
                    .ok_or_else(|| {
                        unowned(source, ObjectDrawCallbackBoundary::TerrainLightStorage)
                    })?
                    .draw_gate;
                if gate == 0 {
                    return Ok(());
                }
                self.draw_common_visual(source, request, outcome)
            }
            // 004012D0 draws nothing; with +4C set it fires outside UI state 3.
            ObjectClass::Selector42(_) => {
                let ui_state = request
                    .host
                    .ui_state()
                    .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::UiState))?;
                self.draw_selector42(source, ui_state)?;
                Ok(())
            }
            // 00440640: the gate's light, then the common visual draw.
            ObjectClass::Selector64(state) => {
                let style = if state.passed_72 != 0 { 0x28 } else { 0x0e };
                let light = request
                    .host
                    .gate_light(style)
                    .map_err(|error| match error {
                        ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                        ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
                    })?;
                request
                    .host
                    .scene(light)
                    .map_err(Flag0ObjectDrawError::Host)?;
                self.draw_common_visual(source, request, outcome)
            }
            // Selectors 22, 50 and 58 share the visual base's +2C 0047AFC0.
            ObjectClass::Visual(_)
            | ObjectClass::Selector22(_)
            | ObjectClass::Selector65(_)
            | ObjectClass::Selector50(_)
            | ObjectClass::Selector48(_)
            | ObjectClass::Selector43(_)
            | ObjectClass::RotatingVisual(_)
            | ObjectClass::Selector45(_)
            | ObjectClass::Selector58(_)
            | ObjectClass::Selector83(_)
            | ObjectClass::RouteNode { .. } => self.draw_common_visual(source, request, outcome),
            // 00475D10: the visual in its holder's colour (H6 style 0x26 and
            // the holder, else 0x0E) as the published tint, restored after.
            ObjectClass::Selector46(territory) => {
                let style = match territory.owner_90 {
                    0 => 0x0e,
                    owner => 0x26 + i32::from(owner),
                };
                let tint = match request.host.style_word(style) {
                    Ok(word) => word,
                    Err(ObjectDrawServiceError::Backend(error)) => {
                        return Err(Flag0ObjectDrawError::Host(error))
                    }
                    Err(ObjectDrawServiceError::UnownedCallback(reason)) => {
                        return Err(unowned(source, reason))
                    }
                };
                let saved = std::mem::replace(&mut *request.lighting.tint_word, tint);
                let drawn = self.draw_common_visual(source, request, outcome);
                *request.lighting.tint_word = saved;
                drawn
            }
            ObjectClass::Selector76(_) => self.draw_selector76(source, request, outcome),
            // 0047AFC0 for the race and selector 71; a gate's 0046FB50 runs
            // it first, then draws its posts.
            ObjectClass::Race(race) => {
                let gate = matches!(race.part, super::RacePart::Gate(_));
                self.draw_common_visual(source, request, outcome)?;
                if gate {
                    self.draw_race_gate(source, request)?;
                }
                Ok(())
            }
            ObjectClass::Actor(actor) => {
                if matches!(&actor.extension, ActorExtension::Selector66(_)) {
                    return self.draw_selector66(source, request, outcome);
                }
                // 0045BB50, the banner's +2C: where its carrier is, then the
                // visual's 0047AFC0.
                if matches!(&actor.extension, ActorExtension::Selector40(_)) {
                    self.place_banner_for_draw(source)?;
                    return self.draw_common_visual(source, request, outcome);
                }
                // 00403D30, the ball's +2C: a held ball at its holder's
                // reach (00403750), then the visual's 0047AFC0.
                if matches!(&actor.extension, ActorExtension::Selector59(state) if state.ball_148.is_some())
                {
                    let stopped = |reason: String| {
                        unowned(source, ObjectDrawCallbackBoundary::BallDraw { reason })
                    };
                    let sine = request
                        .host
                        .sine_quarter()
                        .ok_or_else(|| stopped("the sine table 0053DF00".into()))?;
                    self.place_drawn_ball::<std::convert::Infallible>(source, &sine)
                        .map_err(|error| match error {
                            super::BallError::Object(error) => error.into(),
                            other => stopped(other.to_string()),
                        })?;
                    return self.draw_common_visual(source, request, outcome);
                }
                if matches!(&actor.extension, ActorExtension::Selector69(_)) {
                    return self.draw_selector69(source, request);
                }
                if matches!(&actor.extension, ActorExtension::Selector74(_)) {
                    return self.draw_selector74(source, request);
                }
                if matches!(&actor.extension, ActorExtension::Selector18(_)) {
                    return self.draw_selector18(source, request);
                }
                if matches!(&actor.extension, ActorExtension::Selector44(_)) {
                    return self.draw_selector44(source, request, outcome);
                }
                // 0046E480, the Prison Unit's +2C: the common 0047D910, then
                // its hold's 004126E0, each held actor's own +2C.
                if let ActorExtension::Selector17(prison) = &actor.extension {
                    let held = prison.hold_144.handles.clone();
                    self.draw_common_actor(source, request, outcome)?;
                    for handle in held {
                        if let Some(prisoner) = self
                            .resolve_actor_handle(handle)
                            .map_err(|_| ObjectError::MissingActorRegistry)?
                        {
                            self.draw_near_callback(prisoner, request, outcome)?;
                        }
                    }
                    return Ok(());
                }
                if matches!(&actor.extension, ActorExtension::BehaviorResource(_)) {
                    // 004860C0: the cannon shot's draw only moves and aims it;
                    // the laser's 00485E60 then reaches and draws its beam.
                    let settings_7c = request.host.settings_7c();
                    if let (true, Some(settings_7c)) = (self.is_shot32(source), settings_7c) {
                        let network = *request.network_flag != 0;
                        self.pose_shot32(source, network, settings_7c)
                            .map_err(|error| {
                                unowned(
                                    source,
                                    ObjectDrawCallbackBoundary::WeaponFire {
                                        reason: error.to_string(),
                                    },
                                )
                            })?;
                        if !self.is_laser31(source) {
                            return Ok(());
                        }
                        self.reach_laser31(source, network)?;
                        return request
                            .host
                            .draw_laser(self, source, request.context)
                            .map_err(|error| match error {
                                ObjectDrawServiceError::UnownedCallback(reason) => {
                                    unowned(source, reason)
                                }
                                ObjectDrawServiceError::Backend(error) => {
                                    Flag0ObjectDrawError::Host(error)
                                }
                            });
                    }
                    // 00456180: the charge first returns to the actor it
                    // sticks to, then draws as the grenade does.
                    if self.is_charge27(source) {
                        let charge = |reason: String| {
                            unowned(source, ObjectDrawCallbackBoundary::Charge27 { reason })
                        };
                        let null = request
                            .host
                            .process_null_handle()
                            .ok_or_else(|| charge("the process null 0054E894".into()))?;
                        self.pose_charge27(source, null)
                            .map_err(|error| charge(error.to_string()))?;
                        return self.draw_common_actor(source, request, outcome);
                    }
                    // 0047D910: the grenade's draw is the common one; its
                    // model word 0xFFFF takes it off the list.
                    if self.is_thrown(source) {
                        return self.draw_common_actor(source, request, outcome);
                    }
                    // The Bomb's 00525AB4 keeps the common 0047D910.
                    if self.is_bomb29(source) {
                        return self.draw_common_actor(source, request, outcome);
                    }
                    // 00457620: the missiles' too, unless live and flagged
                    // 0x10 (a player's unit), when the player holding it
                    // (0046B090) looks out of it instead.
                    if self.is_missile(source) {
                        let flags = self.object(source)?.base.flags;
                        if flags & 0x8000_0000 == 0 && flags & 0x10 != 0 {
                            if self.player_holding(self.actor(source)?.handle)?.is_some() {
                                return Ok(());
                            }
                        }
                        return self.draw_common_actor(source, request, outcome);
                    }
                    return Err(unowned(
                        source,
                        ObjectDrawCallbackBoundary::BehaviorResource,
                    ));
                }
                if matches!(&actor.extension, ActorExtension::Derived(state)
                    if matches!(&state.class, DerivedActorClass::Selector2(_)))
                {
                    if request.host.attract_demo() != 0 {
                        self.set_flag0(source, false)?;
                        return Ok(());
                    }
                    return self.draw_ordinary_vehicle(source, request, outcome);
                }
                // The common 0047D910, which the +2C of selectors 8, 9, 10,
                // 21, 34, 35 and 36 (00427E60) and of 12 and 13 (004690E0,
                // through 00427E60) only forward to.
                self.draw_common_actor(source, request, outcome)
            }
        }
    }

    /// `0041D79C..0041D8D5`: the HUD lends a carried actor a position and a
    /// basis, calls its own draw `+2C` with the HUD's view, then gives back
    /// its position and its basis. Port omissions in the draw still give the
    /// pose back.
    pub fn draw_with_lent_pose<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        actor: ObjectId,
        position: [u16; 3],
        basis: MenuBasis,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<Flag0ObjectDrawOutcome, Flag0ObjectDrawError<T::Error, H::Error>> {
        let saved_position = self.object(actor)?.base.position_words;
        let saved_basis = self.actor(actor)?.rotating.basis;
        self.owned_mut(actor)?.state.base.position_words = Some(position);
        self.actor_mut(actor)?.rotating.basis = basis;
        let mut outcome = Flag0ObjectDrawOutcome::default();
        let drawn = self.draw_near_callback(actor, request, &mut outcome);
        self.owned_mut(actor)?.state.base.position_words = saved_position;
        self.actor_mut(actor)?.rotating.basis = saved_basis;
        drawn.map(|()| outcome)
    }

    /// `00477370`, the Force Field's +2C: the H12 light words (+4/+8/+C,
    /// X and Z quartered by signed division) normalized like 0044FE17 in
    /// grey 0x20 (00490030), the context at the field (0048FE60, which the
    /// host's projection takes), then the dome 004777A0 while it is on.
    fn draw_selector18<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let [x, y, z] = request
            .terrain
            .terrain(self, source)
            .map_err(Flag0ObjectDrawError::Terrain)?
            .light_direction_raw();
        let grey = super::actor::FORCE_FIELD_LIGHT_GREY;
        request
            .host
            .scene(SceneCommand::DirectionalLight {
                direction: crate::terrain_runtime::normalize_terrain_light([x / 4, y, z / 4]),
                rgb: [grey; 3],
            })
            .map_err(Flag0ObjectDrawError::Host)?;
        if !self.selector18(source)?.on_176 {
            return Ok(());
        }
        request
            .host
            .draw_force_field(self, source, request.context)
            .map_err(|error| match error {
                ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
            })
    }

    /// `00411570`: the bolts' ground heights read H12/0 at each bolt on the
    /// ground, then the host births the sparks and draws the beams.
    fn draw_selector69<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let stopped = |reason: String| {
            unowned(
                source,
                ObjectDrawCallbackBoundary::Selector69BarrierDraw { reason },
            )
        };
        let geometry = self
            .selector69_geometry(source)
            .map_err(|error| stopped(error.to_string()))?;
        let bolts = geometry
            .bolts(|xz| {
                request
                    .terrain
                    .terrain(self, source)
                    .map(|terrain| terrain.height_at(xz))
            })
            .map_err(|error| match error.split_host() {
                Err(error) => Flag0ObjectDrawError::Terrain(error),
                Ok(super::Selector69Error::Object(error)) => error.into(),
                Ok(other) => stopped(other.to_string()),
            })?;
        request
            .host
            .draw_barrier(self, source, request.context, &bolts)
            .map_err(|error| match error {
                ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
            })
    }

    /// `0046FB50`'s posts: their tops (`0046FC59`), then each foot 50 under
    /// the ground below its top (H12/0's `00450A30`).
    fn draw_race_gate<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let tops = self.gate_post_tops(source).map_err(|error| {
            unowned(
                source,
                ObjectDrawCallbackBoundary::Selector72GateDraw {
                    reason: error.to_string(),
                },
            )
        })?;
        let mut posts = Vec::with_capacity(tops.len());
        for top in tops {
            let ground = request
                .terrain
                .terrain(self, source)
                .map_err(Flag0ObjectDrawError::Terrain)?
                .height_at([top[0], top[2]]);
            let foot = i32::from(ground).wrapping_sub(super::POST_SINK) as u16;
            posts.push((top, [top[0], foot, top[2]]));
        }
        request
            .host
            .draw_gate(self, source, request.context, &posts)
            .map_err(|error| match error {
                ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
            })
    }

    /// `0042D7D0`, the zone's +2C, which draws no model: its points, each on
    /// the ground under it (H12/0) plus 0x32, which the host births.
    fn draw_selector74<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let stopped = |reason: String| {
            unowned(
                source,
                ObjectDrawCallbackBoundary::Selector74ZoneDraw { reason },
            )
        };
        let sine = request
            .host
            .sine_quarter()
            .ok_or_else(|| stopped("the sine table 0053DF00".into()))?;
        let points = self
            .selector74_draw_points(source, &sine)
            .map_err(|error| match error {
                super::Selector74Error::Object(error) => error.into(),
                super::Selector74Error::TimerResidue { .. } => {
                    unowned(source, ObjectDrawCallbackBoundary::Selector74ZoneTimer)
                }
                other => stopped(other.to_string()),
            })?;
        let mut births = Vec::with_capacity(points.len());
        for [x, z] in points {
            let ground = request
                .terrain
                .terrain(self, source)
                .map_err(Flag0ObjectDrawError::Terrain)?
                .height_at([x, z]);
            births.push([x, super::actor::selector74_point_height(ground), z]);
        }
        request
            .host
            .draw_zone(self, source, &births)
            .map_err(|error| match error {
                ObjectDrawServiceError::UnownedCallback(reason) => unowned(source, reason),
                ObjectDrawServiceError::Backend(error) => Flag0ObjectDrawError::Host(error),
            })
    }

    ///4054C0 captures the referent position, temporarily lends helper XYZ and
    ///flag0 to its real+2C, then restores only after normal callback return.
    fn draw_selector66<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        self.draw_selector66_with(source, |arena, owner| {
            arena.draw_near_callback(owner, request, outcome)
        })
        .map_err(|error| match error {
            super::Selector66Error::Service { error, .. } => error,
            super::Selector66Error::Object(error) => error.into(),
            super::Selector66Error::Construction(error) => error.into(),
            super::Selector66Error::PreviouslyStopped(_)
            | super::Selector66Error::Unwritten { .. } => {
                unowned(source, ObjectDrawCallbackBoundary::Selector66Stopped)
            }
        })
    }

    fn draw_common_actor<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let [x, y, z] = self.draw_position(source)?;
        let terrain = request
            .terrain
            .terrain(self, source)
            .map_err(Flag0ObjectDrawError::Terrain)?;
        let excess = ground_excess(y, x, z, terrain);
        let saved_far = request.context.far;
        if excess > 0 {
            request.context.far = saved_far.wrapping_add(excess.wrapping_mul(4));
            request
                .host
                .scene(SceneCommand::PublishFade(SceneFade::new([
                    request.context.near,
                    request.context.far,
                ])))
                .map_err(Flag0ObjectDrawError::Host)?;
        }
        let c8 = self.actor(source)?.word_c8;
        if c8 != 0 {
            let ca = self.actor(source)?.word_ca.ok_or_else(|| {
                unowned(
                    source,
                    ObjectDrawCallbackBoundary::ActorFlashField { offset: 0xca },
                )
            })?;
            //47D9D2..E8's signed quotient. The flash consumer sign-extends
            //its LOWWORD; do not clamp this full positive DWORD first.
            let intensity = i32::from(c8) * 32767 / 31250;
            //Native MOV DX preserves an unspecified high half. Canonical H7
            //checks/indexing use only LOWWORD and its actual NULL callback
            //4EFBF0 ignores the full argument. This host domain excludes a
            //replacement miss callback that observes those unknown bits.
            flash::enter::<T::Error, H>(
                self,
                source,
                request.context,
                intensity,
                i32::from(ca),
                request.host,
            )?;
        }
        let drawn = self.draw_common_visual(source, request, outcome);
        // Native always finishes the model and restores below. A port
        // omission (an unowned model value or callback) stands in for that
        // normal draw, so the flash and far distance still return; otherwise
        // everything drawn after this actor keeps its fog. Host, terrain and
        // object faults keep their reached prefix.
        let drawn = match drawn {
            Err(error @ Flag0ObjectDrawError::UnownedCallback { .. }) => Err(error),
            Err(error) => return Err(error),
            Ok(()) => Ok(()),
        };
        //47D9F9 rereads C8 after model.
        if self.actor(source)?.word_c8 != 0 {
            flash::restore::<T::Error, H>(self, source, request.context, request.host)?;
        }
        if excess > 0 {
            request.context.far = saved_far;
            request
                .host
                .scene(SceneCommand::PublishFade(SceneFade::new([
                    request.context.near,
                    request.context.far,
                ])))
                .map_err(Flag0ObjectDrawError::Host)?;
        }
        drawn
    }

    /// 00454000, the rock's +2C: the context's fade bounds both 0x3FFF,
    /// which 0048FB00 publishes, the common actor draw 0047D910, then the
    /// context's own bounds back and published. As for the common actor
    /// draw, a port omission in the draw stands in for it and the bounds
    /// still return.
    fn draw_selector44<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let saved = [request.context.near, request.context.far];
        let fade = super::actor::ROCK_DRAW_FADE;
        request.context.near = fade;
        request.context.far = fade;
        request
            .host
            .scene(SceneCommand::PublishFade(SceneFade::new([fade, fade])))
            .map_err(Flag0ObjectDrawError::Host)?;
        let drawn = match self.draw_common_actor(source, request, outcome) {
            Err(error @ Flag0ObjectDrawError::UnownedCallback { .. }) => Err(error),
            Err(error) => return Err(error),
            Ok(()) => Ok(()),
        };
        [request.context.near, request.context.far] = saved;
        request
            .host
            .scene(SceneCommand::PublishFade(SceneFade::new(saved)))
            .map_err(Flag0ObjectDrawError::Host)?;
        drawn
    }

    /// 004323B0, the flamethrower's +2C: its basis turned a quarter in its
    /// Y-Z plane (0047A720(0x4000)), the common visual draw at the thrower,
    /// again at the flame's root, then the thrower's position and the saved
    /// basis back. As for the common actor draw, a port omission in the
    /// first draw stands in for it and the second still runs.
    fn draw_selector76<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        let math = request
            .host
            .basis_math()
            .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::FlamethrowerBasis))?;
        let saved = self.selector76_turn_for_draw(source, &math)?;
        let first = self.draw_common_visual(source, request, outcome);
        if let Err(error) = &first {
            if !matches!(error, Flag0ObjectDrawError::UnownedCallback { .. }) {
                self.selector76_draw_restore(source, saved)?;
                return first;
            }
        }
        self.selector76_draw_at_root(source)?;
        let second = self.draw_common_visual(source, request, outcome);
        self.selector76_draw_restore(source, saved)?;
        first.and(second)
    }

    fn draw_common_visual<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
        &mut self,
        source: ObjectId,
        request: &mut Flag0ObjectDrawRequest<'_, T, H>,
        outcome: &mut Flag0ObjectDrawOutcome,
    ) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
        if self.draw_model_word(source)? == u16::MAX {
            self.set_flag0(source, false)?;
            return Ok(()); //Before any packet XYZ/+48/light read.
        }
        let packet = self.draw_packet(source, request.process)?;
        if self.object(source)?.base.flags & 0x80 == 0 {
            //Fresh position at47B062/75 is independent of captured packet.
            let [x, _, z] = self.draw_position(source)?;
            let light = object_grid_light([x, z], &request.lighting)?;
            let tint = request.lighting.tint_word.to_le_bytes();
            let ambient = std::array::from_fn(|i| i32::from(tint[i] >> 2) + i32::from(light[i]));
            request
                .host
                .scene(SceneCommand::AmbientLight(ambient))
                .map_err(Flag0ObjectDrawError::Host)?;
            let rgb = std::array::from_fn(|i| {
                (i32::from(tint[i]) + i32::from(light[i])).clamp(0, 255) as u8
            });
            request
                .host
                .scene(SceneCommand::DirectionalLight {
                    direction: request.lighting.direction,
                    rgb,
                })
                .map_err(Flag0ObjectDrawError::Host)?;
        }
        submit_model(self, packet, ModelRootMode::CommonVisual, request)?;
        outcome.common_models += 1;
        Ok(())
    }

    fn draw_position<T, H>(
        &self,
        source: ObjectId,
    ) -> Result<[u16; 3], Flag0ObjectDrawError<T, H>> {
        self.object(source)?
            .base
            .position_words
            .ok_or_else(|| unowned(source, ObjectDrawCallbackBoundary::Position))
    }

    fn draw_model_word(&self, source: ObjectId) -> Result<u16, ObjectError> {
        Ok(match &self.object(source)?.class {
            ObjectClass::Visual(visual)
            | ObjectClass::TerrainLight { visual, .. }
            | ObjectClass::RouteNode { visual, .. } => visual.model_word,
            ObjectClass::Selector22(state) => state.visual.model_word,
            ObjectClass::Selector65(state) => state.visual.model_word,
            ObjectClass::Selector50(state) => state.visual.model_word,
            ObjectClass::Selector48(state) => state.visual.model_word,
            ObjectClass::Selector43(state) => state.visual.model_word,
            ObjectClass::Selector42(state) => state.visual.model_word,
            ObjectClass::Selector64(gate) => gate.rotating.visual.model_word,
            ObjectClass::RotatingVisual(rotating)
            | ObjectClass::RotatingTerrainLight { rotating, .. } => rotating.visual.model_word,
            ObjectClass::Selector58(pad) => pad.rotating.visual.model_word,
            ObjectClass::Selector45(marker) => marker.rotating.visual.model_word,
            ObjectClass::Selector46(territory) => territory.rotating.visual.model_word,
            ObjectClass::Race(race) => race.rotating.visual.model_word,
            ObjectClass::Selector83(patch) => patch.rotating.visual.model_word,
            ObjectClass::Selector76(flame) => flame.rotating.visual.model_word,
            ObjectClass::TimedPickup(pickup) => pickup.visual.model_word,
            ObjectClass::Actor(actor) => actor.rotating.visual.model_word,
            _ => return Err(ObjectError::NotVisual(source)),
        })
    }

    /// Port setting, not retail: the model, position and basis `source`
    /// draws with, as its draw takes them, for Ultra ground's footings
    /// ([`crate::ultra_footings`]); none for an object that draws no model.
    pub(crate) fn drawn_model(
        &self,
        source: ObjectId,
        process: &ObjectDrawProcessState,
    ) -> Option<ObjectModelPacket> {
        self.draw_packet::<(), ()>(source, process).ok()
    }

    fn draw_packet<T, H>(
        &self,
        source: ObjectId,
        process: &ObjectDrawProcessState,
    ) -> Result<ObjectModelPacket, Flag0ObjectDrawError<T, H>> {
        let model_slot = self.draw_model_word(source)? as i16;
        let position = self
            .draw_position(source)?
            .map(|word| i32::from(word as i16));
        let basis = match &self.object(source)?.class {
            ObjectClass::RotatingVisual(rotating)
            | ObjectClass::RotatingTerrainLight { rotating, .. } => MenuBasis {
                words: rotating.basis.words,
            },
            ObjectClass::Selector58(pad) => MenuBasis {
                words: pad.rotating.basis.words,
            },
            ObjectClass::Selector45(marker) => MenuBasis {
                words: marker.rotating.basis.words,
            },
            ObjectClass::Selector46(territory) => MenuBasis {
                words: territory.rotating.basis.words,
            },
            ObjectClass::Race(race) => MenuBasis {
                words: race.rotating.basis.words,
            },
            ObjectClass::Selector83(patch) => MenuBasis {
                words: patch.rotating.basis.words,
            },
            // 00521A40's +48 is 004026B0, its own basis: 004323B0 turned it.
            ObjectClass::Selector76(flame) => MenuBasis {
                words: flame.rotating.basis.words,
            },
            ObjectClass::Selector64(gate) => MenuBasis {
                words: gate.rotating.basis.words,
            },
            ObjectClass::Actor(actor) => MenuBasis {
                words: actor.rotating.basis.words,
            },
            ObjectClass::Visual(_)
            | ObjectClass::Selector22(_)
            | ObjectClass::Selector65(_)
            | ObjectClass::Selector50(_)
            | ObjectClass::Selector48(_)
            | ObjectClass::Selector43(_)
            | ObjectClass::Selector42(_)
            | ObjectClass::TerrainLight { .. }
            | ObjectClass::RouteNode { .. } => process.shared_basis,
            _ => return Err(ObjectError::NotVisual(source).into()),
        };
        Ok(ObjectModelPacket {
            source,
            model_slot,
            position,
            basis,
        })
    }

    /// Canonical object-wide+18, distinct from actor+58/+64 queries.
    fn draw_component_18(&self, source: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        Ok(match &self.object(source)?.class {
            ObjectClass::Actor(_) => Some(source),
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
            | ObjectClass::RotatingVisual(_)
            | ObjectClass::Selector45(_)
            | ObjectClass::Selector46(_)
            | ObjectClass::Race(_)
            | ObjectClass::Selector58(_)
            | ObjectClass::Selector83(_)
            | ObjectClass::Selector76(_)
            | ObjectClass::TimedPickup(_)
            | ObjectClass::TerrainLight { .. }
            | ObjectClass::RotatingTerrainLight { .. }
            | ObjectClass::RouteNode { .. } => None,
        })
    }
}

fn unowned<T, H>(
    object: ObjectId,
    reason: ObjectDrawCallbackBoundary,
) -> Flag0ObjectDrawError<T, H> {
    Flag0ObjectDrawError::UnownedCallback { object, reason }
}

fn ground_excess(y: u16, x: u16, z: u16, terrain: &TerrainRuntime) -> i32 {
    let height = terrain.raw_height_at([x, z]) as i16;
    let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
    i32::from(y as i16)
        .wrapping_sub(i32::from(height.max(plane)))
        .wrapping_sub(256)
}

fn object_grid_light(
    xz: [u16; 2],
    lighting: &ObjectDrawLighting<'_>,
) -> Result<[i8; 3], FrontendExecutableError> {
    let anchors = lighting.grid.anchors();
    let (_, mask) = spacing_pair(lighting.spacing);
    let align = ((!i32::from(mask)) >> 7) as u32;
    let cells: [u32; 2] = std::array::from_fn(|i| {
        (u32::from(xz[i]).wrapping_sub(u32::from(anchors[i])) >> 7) & 0x1ff & align
    });
    let [x, z] = cells;
    let bytes = if x < 90 && z < 90 {
        let at = ((z + x * 90) * 3) as usize;
        &lighting.grid.cells()[at..at + 3]
    } else {
        &lighting.executable.mapped(0x0052_6068, 4)?.bytes[..3]
    };
    Ok(std::array::from_fn(|i| bytes[i] as i8))
}

fn submit_model<T: ActorTerrainHost, H: Flag0ObjectDrawHost>(
    arena: &mut ObjectArena,
    packet: ObjectModelPacket,
    root: ModelRootMode,
    request: &mut Flag0ObjectDrawRequest<'_, T, H>,
) -> Result<(), Flag0ObjectDrawError<T::Error, H::Error>> {
    match request.host.model(ObjectModelDrawSubmission {
        arena,
        packet,
        root,
        context: request.context,
    }) {
        Ok(()) => Ok(()),
        Err(ObjectDrawServiceError::Backend(error)) => Err(Flag0ObjectDrawError::Host(error)),
        Err(ObjectDrawServiceError::UnownedCallback(reason)) => Err(unowned(packet.source, reason)),
    }
}
