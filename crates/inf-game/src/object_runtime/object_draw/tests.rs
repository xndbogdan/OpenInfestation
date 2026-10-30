use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ActorTerrainSurface, BaseContext, DerivedActorConstructionRequest, ObjectClassKind,
        TerrainLight,
    },
    terrain_runtime::native_probe_fixture_terrain,
    world_lighting::{TerrainLightColor, TerrainLightPolicy, TerrainLightRequest},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::VecDeque, path::Path, rc::Rc};

mod cone;
mod firing;
mod vehicle;

static UNUSED_SINE: [u16; 4096] = [0; 4096];

fn executable_bytes() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("required original retail executable")
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn allocation() -> ActorConstructionRequest<'static> {
    // Completed shared H12-NULL constructor branch does not consume XYZ/sine.
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &UNUSED_SINE,
        },
    }
}

fn terrain() -> TerrainRuntime {
    native_probe_fixture_terrain(&json!({
        "spacing":128,"height_shift":4,"plane_base":-1000 << 9,"plane_shift":9,
        "xz":[0,0],"heights":[0,0,0,0],"alternate":false,
    }))
}

fn position(arena: &mut ObjectArena, object: ObjectId, xyz: [u16; 3], extent: u16) {
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some(xyz),
                placement_index: 7,
            },
        )
        .unwrap();
    // Explicit supplied current extent, independent of any missing H8 header.
    arena.owned_mut(object).unwrap().state.base.word_28 = extent;
}

fn visual(arena: &mut ObjectArena, class: ObjectClassKind, xyz: [u16; 3], model: u16) -> ObjectId {
    let object = arena.allocate(class);
    position(arena, object, xyz, 0);
    model_word(arena, object, model);
    object
}

fn model_word(arena: &mut ObjectArena, object: ObjectId, model: u16) {
    // Declared current packet fields, not an authored H8 allocation/default.
    match &mut arena.owned_mut(object).unwrap().state.class {
        ObjectClass::Visual(visual)
        | ObjectClass::TerrainLight { visual, .. }
        | ObjectClass::RouteNode { visual, .. } => visual.model_word = model,
        ObjectClass::RotatingVisual(rotating)
        | ObjectClass::RotatingTerrainLight { rotating, .. } => rotating.visual.model_word = model,
        ObjectClass::Actor(actor) => actor.rotating.visual.model_word = model,
        ObjectClass::Selector76(flame) => flame.rotating.visual.model_word = model,
        _ => panic!("actual visual receiver"),
    }
}

fn actor(arena: &mut ObjectArena, xyz: [u16; 3], model: u16) -> ObjectId {
    let object = arena.allocate_actor(allocation()).unwrap();
    position(arena, object, xyz, 0);
    model_word(arena, object, model);
    object
}

fn vehicle(arena: &mut ObjectArena, terrain: &TerrainRuntime) -> ObjectId {
    let mut request = allocation();
    request.allocation_position = Some([768, 600, 768]);
    request.terrain.terrain = Some(ActorTerrainSurface {
        terrain,
        plane_height_fixed: terrain.plane_base(),
    });
    arena
        .allocate_selector2_actor(
            DerivedActorConstructionRequest {
                actor: request,
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap()
}

#[derive(Clone, Debug, PartialEq)]
enum ScenePhase {
    Ambient([i32; 3]),
    Directional { direction: [f32; 3], rgb: [u8; 3] },
    Fade(SceneFade),
}

impl From<SceneCommand> for ScenePhase {
    fn from(command: SceneCommand) -> Self {
        match command {
            SceneCommand::AmbientLight(rgb) => Self::Ambient(rgb),
            SceneCommand::DirectionalLight { direction, rgb } => {
                Self::Directional { direction, rgb }
            }
            SceneCommand::PublishFade(fade) => Self::Fade(fade),
            _ => panic!("only actual object-light/fade publications belong to this owner"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Terrain(ObjectId, Option<[u16; 3]>),
    Scene(ScenePhase),
    Model(ObjectModelPacket, ModelRootMode, ObjectDrawContext),
    Attract(u8),
    Firing(firing::FiringEvent),
}

type Trace = Rc<RefCell<Vec<Event>>>;

enum TerrainEffect {
    None,
    PositionExtent([u16; 3], u16),
    Destroy,
}

struct Terrain<'a> {
    value: &'a TerrainRuntime,
    trace: Trace,
    effects: VecDeque<TerrainEffect>,
    fail_at: Option<usize>,
    calls: usize,
}

impl ActorTerrainHost for Terrain<'_> {
    type Error = &'static str;

    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.trace.borrow_mut().push(Event::Terrain(
            object,
            arena.object(object).unwrap().base.position_words,
        ));
        let call = self.calls;
        self.calls += 1;
        if self.fail_at == Some(call) {
            return Err("current H12 source unavailable");
        }
        match self.effects.pop_front().unwrap_or(TerrainEffect::None) {
            TerrainEffect::None => {}
            TerrainEffect::PositionExtent(xyz, extent) => {
                let base = &mut arena.owned_mut(object).unwrap().state.base;
                base.position_words = Some(xyz);
                base.word_28 = extent;
            }
            TerrainEffect::Destroy => {
                arena
                    .destroy(crate::object_runtime::ObjectDestructionRequest {
                        object: object,
                        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    })
                    .unwrap();
            }
        }
        Ok(self.value)
    }
}

/// Records the required backend boundary; it does not claim model execution or
/// native rendering acceptance. Production supplies the real typed executor.
struct Host {
    trace: Trace,
    attract: u8,
    fail: Option<(ObjectId, ObjectDrawServiceError<&'static str>)>,
    scene_fail_at: Option<usize>,
    scene_calls: usize,
    firing: Option<firing::FiringHost>,
    /// A supplied H7 colour for a reached flash lookup.
    material: Option<u32>,
    /// The sine table's turns, for the flamethrower's draw.
    math: Option<inf_formats::native_basis::NativeBasisMath>,
    /// Whether the event log was silent at each model.
    silent: Vec<bool>,
    /// An object whose model writes this allocation's first word.
    write_at: Option<(
        ObjectId,
        std::sync::Arc<crate::object_runtime::RuntimeAllocation>,
    )>,
}

impl Flag0ObjectDrawHost for Host {
    type Error = &'static str;

    fn scene(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        self.trace.borrow_mut().push(Event::Scene(command.into()));
        let call = self.scene_calls;
        self.scene_calls += 1;
        if self.scene_fail_at == Some(call) {
            Err("scene publication stopped")
        } else {
            Ok(())
        }
    }

    fn attract_demo(&mut self) -> u8 {
        self.trace.borrow_mut().push(Event::Attract(self.attract));
        self.attract
    }

    fn material_color(&mut self, _: i32) -> Result<u32, ObjectDrawServiceError<Self::Error>> {
        //Complete-constructor C8-zero cases never reach H7. The owning flash
        //tests separately supply and verify its genuine lookup phases.
        self.material
            .take()
            .ok_or(ObjectDrawServiceError::Backend("unexpected H7 acquisition"))
    }

    fn basis_math(&mut self) -> Option<inf_formats::native_basis::NativeBasisMath> {
        self.math.clone()
    }

    fn model(
        &mut self,
        request: ObjectModelDrawSubmission<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        assert!(request.arena.object(request.packet.source).is_ok());
        self.trace
            .borrow_mut()
            .push(Event::Model(request.packet, request.root, *request.context));
        self.silent.push(crate::event_log::silent());
        if let Some((_, allocation)) = self
            .write_at
            .as_ref()
            .filter(|(object, _)| *object == request.packet.source)
        {
            allocation
                .write_word(0, 0x1234)
                .map_err(|_| ObjectDrawServiceError::Backend("model's write refused"))?;
        }
        if self
            .fail
            .as_ref()
            .is_some_and(|(object, _)| *object == request.packet.source)
        {
            Err(self.fail.take().unwrap().1)
        } else {
            if let Some(firing) = self.firing.as_mut() {
                firing.model_return(request.arena, request.packet.source);
            }
            Ok(())
        }
    }

    fn drain_weapon_fire(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<usize, ObjectDrawServiceError<Self::Error>> {
        self.firing
            .as_mut()
            .ok_or(ObjectDrawServiceError::UnownedCallback(
                ObjectDrawCallbackBoundary::MissingWeaponFiringServices,
            ))?
            .drain(arena, actor)
    }
}

struct Fixture<'a> {
    context: ObjectDrawContext,
    process: ObjectDrawProcessState,
    grid: WorldLightGrid,
    executable: FrontendExecutable<'a>,
    tint: u32,
    network_flag: u32,
    spacing: TerrainSpacing,
}

impl<'a> Fixture<'a> {
    fn new(raw: &'a [u8]) -> Self {
        let executable = FrontendExecutable::parse(raw).unwrap();
        Self {
            context: ObjectDrawContext {
                view: SceneView {
                    eye: [0; 3],
                    basis: MenuBasis::INITIAL,
                },
                near: 7,
                far: 100,
                far_color: 0x7654_3210,
            },
            process: ObjectDrawProcessState::native_startup(executable).unwrap(),
            grid: WorldLightGrid::default(),
            executable,
            tint: 0x00ff_0464,
            network_flag: 0,
            spacing: TerrainSpacing::Units128,
        }
    }

    fn run(
        &mut self,
        arena: &mut ObjectArena,
        terrain: &mut Terrain<'_>,
        host: &mut Host,
        failure_policy: Flag0ObjectDrawFailurePolicy,
    ) -> Result<Flag0ObjectDrawOutcome, Flag0ObjectDrawError<&'static str, &'static str>> {
        arena.run_flag0_object_draw_pass(Flag0ObjectDrawRequest {
            context: &mut self.context,
            cone: crate::object_runtime::ObjectDrawCone::Retail,
            process: &mut self.process,
            network_flag: &self.network_flag,
            terrain,
            lighting: ObjectDrawLighting {
                grid: &self.grid,
                tint_word: &mut self.tint,
                direction: [0.25, -0.5, 0.75],
                spacing: self.spacing,
                executable: self.executable,
            },
            host,
            failure_policy,
        })
    }
}

fn services(value: &TerrainRuntime) -> (Terrain<'_>, Host, Trace) {
    let trace = Rc::new(RefCell::new(Vec::new()));
    (
        Terrain {
            value,
            trace: Rc::clone(&trace),
            effects: VecDeque::new(),
            fail_at: None,
            calls: 0,
        },
        Host {
            trace: Rc::clone(&trace),
            attract: 0,
            fail: None,
            scene_fail_at: None,
            scene_calls: 0,
            firing: None,
            material: None,
            math: None,
            silent: Vec::new(),
            write_at: None,
        },
        trace,
    )
}

fn node(object: ObjectId, kind: ObjectNodeKind) -> ObjectNode {
    ObjectNode { object, kind }
}

fn flag0(objects: &[ObjectId]) -> Vec<ObjectNode> {
    objects
        .iter()
        .map(|&object| node(object, ObjectNodeKind::Flag0))
        .collect()
}

#[test]
fn stopped_construction_draw_boundary_precedes_native_writes_and_all_services() {
    let raw = executable_bytes();
    for policy in [
        Flag0ObjectDrawFailurePolicy::Stop,
        Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
    ] {
        let mut fixture = Fixture::new(&raw);
        let mut arena = arena();
        let stopped = arena.allocate(ObjectClassKind::VisualBase);
        arena.apply_payload_flags(stopped, 0x0300_0083).unwrap();
        arena.owned_mut(stopped).unwrap().state.base.dirty_word = 0xa55a;
        // Unknown XYZ must not be read, and pending/visible flags stay intact.
        assert!(arena.object(stopped).unwrap().base.position_words.is_none());
        let native = arena.object(stopped).unwrap().clone();
        arena.stop_object_construction(stopped).unwrap();
        let healthy = visual(&mut arena, ObjectClassKind::VisualBase, [0, 0, 10], 23);
        arena.apply_payload_flags(healthy, 0x83).unwrap();
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        let result = fixture.run(&mut arena, &mut terrain, &mut host, policy);
        assert_eq!(arena.object(stopped).unwrap(), &native);
        if policy == Flag0ObjectDrawFailurePolicy::Stop {
            assert!(matches!(result, Err(Flag0ObjectDrawError::UnownedCallback {
                object, reason: ObjectDrawCallbackBoundary::StoppedConstruction,
            }) if object == stopped));
            assert!(trace.borrow().is_empty());
            assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[healthy]));
            continue;
        }
        let outcome = result.unwrap();
        assert_eq!(outcome.visited, 2);
        assert_eq!(outcome.common_models, 1);
        assert_eq!(
            outcome.omissions,
            [Flag0ObjectDrawOmission {
                object: stopped,
                reason: ObjectDrawCallbackBoundary::StoppedConstruction,
            }]
        );
        assert_eq!(terrain.calls, 1);
        assert!(
            matches!(&trace.borrow()[..], [Event::Terrain(object, _), Event::Model(packet, _, _)]
            if *object == healthy && packet.source == healthy)
        );
        trace.borrow_mut().clear();
        let outcome = fixture
            .run(&mut arena, &mut terrain, &mut host, policy)
            .unwrap();
        assert_eq!(outcome.common_models, 1);
        assert_eq!(outcome.omissions[0].object, stopped);
        assert_eq!(terrain.calls, 2);
        assert_eq!(arena.object(stopped).unwrap(), &native);
    }
}

fn other_custody(arena: &ObjectArena) -> String {
    let ObjectArena {
        identity,
        slots,
        free,
        live_count,
        lists: _,
        list_owners: _,
        routes,
        actors,
        map_annotations,
        spawn_markers,
        territories,
        race,
        race_gates,
        race_clock,
        powerball,
        powerball_goals,
        powerball_seen,
        network_sends,
        network_game,
        actor_handles,
        actor_allocation,
        actor_services,
        actor_payloads,
        constructor_allocations,
        selector60_rows,
        players,
        world_load,
        pickup_kind7_5b4,
        force_field_bits,
        capture_nodes,
        alien_brain,
        cleanup_exit_pending,
    } = arena;
    let allocations: Vec<_> = slots
        .iter()
        .map(|slot| (slot.generation, slot.value.is_some()))
        .collect();
    format!(
        "{:?}",
        (
            (identity, allocations, free, live_count),
            (
                routes,
                actors,
                map_annotations,
                actor_handles,
                actor_allocation,
                actor_services,
                actor_payloads,
                constructor_allocations,
                selector60_rows,
                players,
                world_load,
                (
                    pickup_kind7_5b4,
                    force_field_bits,
                    capture_nodes,
                    alien_brain,
                    cleanup_exit_pending,
                    spawn_markers,
                    territories,
                    (race, race_gates, race_clock),
                    (powerball, powerball_goals, powerball_seen),
                    network_sends,
                    network_game
                )
            )
        )
    )
}

#[test]
fn complete_original_manager_and_near_bodies_remain_authenticated() {
    let raw = executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    for (start, end, digest) in [
        (
            0x0047_9570,
            0x0047_9835,
            "95d984e514ccc0f002f5cbad8dedf625ae65df6fd15535ad3ac3c33f93be8ebb",
        ),
        (
            0x0047_afc0,
            0x0047_b1da,
            "c3df72afa7cf498f73c5f0e45c932d82ceb4ad62414ac52db5891c5ced7b6f46",
        ),
        (
            0x0047_d910,
            0x0047_da23,
            "f630ca72987d836d20faa7a2efed8812e84eb71ab358b64028ca559878796689",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&raw[start - 0x0040_0000..end - 0x0040_0000])
            ),
            digest
        );
    }
    assert_eq!(
        &raw[0x0007_9827..0x0007_982e],
        &[0xc6, 0x05, 0x90, 0xb8, 0x53, 0, 1]
    );
    let mut fixture = Fixture::new(&raw);
    assert_eq!(fixture.process.completion_53b890, 1);
    assert_eq!(fixture.process.shared_basis, MenuBasis::INITIAL);
    // Strict executable mapping consumes the supplied current byte, not a
    // hardcoded fallback. This is not an original startup writer claim.
    let mut supplied = raw.clone();
    supplied[0x0013_b890] = 0x5a;
    assert_eq!(
        ObjectDrawProcessState::native_startup(FrontendExecutable::parse(&supplied).unwrap())
            .unwrap()
            .completion_53b890,
        0x5a
    );
    fixture.process.completion_53b890 = 0x7d;
    let mut arena = arena();
    let before = format!("{arena:?}");
    let owners = arena.list_owners;
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    assert_eq!(
        fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop
            )
            .unwrap(),
        Flag0ObjectDrawOutcome::default()
    );
    assert!(trace.borrow().is_empty());
    assert_eq!(fixture.process.completion_53b890, 1);
    assert_eq!(
        arena.list_owners[ObjectList::Flag0Primary as usize],
        owners[ObjectList::Flag0Secondary as usize]
    );
    arena.swap_flag0_lists();
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn physical_flag0_nodes_move_in_order_and_class_only_near_callbacks_keep_other_owners() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = -101;
    let mut arena = arena();
    let objects: Vec<_> = [
        ObjectClassKind::Base,
        ObjectClassKind::Selector86,
        ObjectClassKind::MapAnnotation,
    ]
    .into_iter()
    .map(|class| {
        let object = arena.allocate(class);
        position(&mut arena, object, [0, 0, 10], 0);
        object
    })
    .collect();
    let before = other_custody(&arena);
    let headers = arena.list_owners;
    let flag1 = arena.list(ObjectList::Flag1Primary).to_vec();
    let mut parking = arena.list(ObjectList::Parking).to_vec();
    parking.extend([
        node(objects[0], ObjectNodeKind::Flag0),
        node(objects[2], ObjectNodeKind::Flag0),
    ]);
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(
        (
            outcome.visited,
            outcome.near_callbacks,
            outcome.common_models,
            outcome.late_models
        ),
        (3, 3, 0, 0)
    );
    assert_eq!(
        arena.selector86(objects[1]).unwrap().distance_squared_48,
        2500
    );
    assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[objects[1]]));
    assert!(arena.list(ObjectList::Flag0Secondary).is_empty());
    assert_eq!(arena.list(ObjectList::Flag1Primary), flag1);
    assert_eq!(arena.list(ObjectList::Parking), parking);
    assert_eq!(
        arena.list_owners[ObjectList::Flag0Primary as usize],
        headers[ObjectList::Flag0Secondary as usize]
    );
    assert_eq!(other_custody(&arena), before);
    assert_eq!(trace.borrow().len(), 3); // H12 for each manager admission; no geometry/light host.
}

#[test]
fn visibility_uses_strict_unsigned_squared_bounds_and_signed_wrapped_words() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    let mut arena = arena();
    let positions = [
        [0, 0, 100],
        [0, 0, 99],
        [40, 0, 40],
        [39, 0, 40],
        [0xffff, 0, 10],
        [0, 0, 0xfffc],
    ];
    let objects: Vec<_> = positions
        .into_iter()
        .map(|xyz| {
            let object = arena.allocate(ObjectClassKind::Base);
            position(&mut arena, object, xyz, 0);
            object
        })
        .collect();
    let value = terrain();
    let (mut terrain, mut host, _) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!((outcome.visited, outcome.near_callbacks), (6, 3));
    for (i, &object) in objects.iter().enumerate() {
        assert_eq!(
            arena.object(object).unwrap().base.flags & 0x0200_0000 != 0,
            [false, true, false, true, true, false][i]
        );
    }
    assert_eq!(
        arena.list(ObjectList::Flag0Primary),
        flag0(&[objects[0], objects[2], objects[5]])
    );
}

#[test]
fn mutable_terrain_reads_keep_entry_xz_but_reread_y_extent_packet_and_second_acquisition() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = 1000;
    fixture.context.view.basis.words[7..10].copy_from_slice(&[0, 1_f32.to_bits(), 0]);
    let mut arena = arena();
    let object = actor(&mut arena, [10, 1000, 30], 23);
    arena.owned_mut(object).unwrap().state.base.word_28 = 7;
    arena.apply_payload_flags(object, 0x0100_0083).unwrap(); // current pending, lighting bypass
    let before = other_custody(&arena);
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    terrain.effects.extend([
        TerrainEffect::PositionExtent([500, 800, 900], 9),
        TerrainEffect::PositionExtent([64000, 2, 65000], 11),
    ]);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(
        (
            outcome.near_callbacks,
            outcome.common_models,
            outcome.late_models
        ),
        (1, 1, 0)
    );
    let events = trace.borrow();
    assert_eq!(events[0], Event::Terrain(object, Some([10, 1000, 30])));
    assert_eq!(events[1], Event::Terrain(object, Some([500, 800, 900])));
    assert_eq!(
        events[2],
        Event::Scene(ScenePhase::Fade(SceneFade::new([7, 3176])))
    );
    let Event::Model(packet, root, context) = events[3] else {
        panic!("actual model phase")
    };
    assert_eq!(
        (packet.source, packet.model_slot, packet.position, root),
        (object, 23, [-1536, 2, -536], ModelRootMode::CommonVisual)
    );
    assert_eq!(context.far, 3176);
    assert_eq!(context.far_color, 0x7654_3210);
    assert_eq!(
        events[4],
        Event::Scene(ScenePhase::Fade(SceneFade::new([7, 1000])))
    );
    assert_eq!(events.len(), 5);
    assert_eq!(fixture.context.far, 1000);
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        Some([64000, 2, 65000])
    );
    assert_eq!(
        arena.object(object).unwrap().base.flags & 0x0300_0000,
        0x0200_0000
    );
    assert_eq!(other_custody(&arena), before);
}

#[test]
fn terrain_errors_stop_at_the_reached_acquisition_and_do_not_exchange_headers() {
    let raw = executable_bytes();
    for fail_at in [0, 1] {
        let mut fixture = Fixture::new(&raw);
        fixture.process.completion_53b890 = 0x6d;
        let mut arena = arena();
        let object = actor(&mut arena, [0, 0, 10], 23);
        arena.apply_payload_flags(object, 0x0300_0083).unwrap();
        let before = other_custody(&arena);
        let headers = arena.list_owners;
        let context = fixture.context;
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        terrain.fail_at = Some(fail_at);
        assert!(matches!(
            fixture.run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation
            ),
            Err(Flag0ObjectDrawError::Terrain(
                "current H12 source unavailable"
            ))
        ));
        assert_eq!(terrain.calls, fail_at + 1);
        assert_eq!(trace.borrow().len(), fail_at + 1);
        assert_eq!(arena.list_owners, headers);
        assert!(arena.list(ObjectList::Flag0Primary).is_empty());
        assert_eq!(arena.list(ObjectList::Flag0Secondary), flag0(&[object]));
        assert_eq!(
            arena.object(object).unwrap().base.flags & 0x0300_0000,
            if fail_at == 0 {
                0x0100_0000
            } else {
                0x0300_0000
            }
        );
        assert_eq!(fixture.context, context);
        assert_eq!(fixture.process.completion_53b890, 0x6d);
        assert_eq!(other_custody(&arena), before);
    }
}

#[test]
fn unowned_model_omission_restores_far_and_keeps_pending_prefix_then_uses_fresh_current_far() {
    let raw = executable_bytes();
    for policy in [
        Flag0ObjectDrawFailurePolicy::Stop,
        Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
    ] {
        let mut fixture = Fixture::new(&raw);
        fixture.context.view.basis.words[7..10].copy_from_slice(&[0, 1_f32.to_bits(), 0]);
        fixture.process.completion_53b890 = 0x71;
        let mut arena = arena();
        let object = actor(&mut arena, [0, 1000, 0], 23);
        arena.apply_payload_flags(object, 0x0100_0083).unwrap();
        let emitter = arena.allocate(ObjectClassKind::Selector86);
        position(&mut arena, emitter, [0, 0, 10], 32);
        let emitter_before = arena.selector86(emitter).unwrap().clone();
        let before = other_custody(&arena);
        let headers = arena.list_owners;
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        host.fail = Some((
            object,
            ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::ModelFeature {
                model_slot: 23,
                reason: "required supplied model backing".into(),
            }),
        ));
        let result = fixture.run(&mut arena, &mut terrain, &mut host, policy);
        if policy == Flag0ObjectDrawFailurePolicy::Stop {
            assert!(
                matches!(result, Err(Flag0ObjectDrawError::UnownedCallback { object: reached,
                reason: ObjectDrawCallbackBoundary::ModelFeature { model_slot: 23, .. } }) if reached == object)
            );
            assert_eq!(arena.list_owners, headers);
            assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[emitter]));
            assert_eq!(fixture.process.completion_53b890, 0x71);
            assert_eq!(*arena.selector86(emitter).unwrap(), emitter_before);
        } else {
            let outcome = result.unwrap();
            assert_eq!((outcome.visited, outcome.near_callbacks), (2, 2));
            assert_eq!(
                outcome.omissions,
                vec![Flag0ObjectDrawOmission {
                    object,
                    reason: ObjectDrawCallbackBoundary::ModelFeature {
                        model_slot: 23,
                        reason: "required supplied model backing".into()
                    }
                }]
            );
            // The emitter sees the restored far (100), half of it squared.
            assert_eq!(
                arena.selector86(emitter).unwrap().distance_squared_48,
                50 * 50
            );
            assert_eq!(
                arena.list(ObjectList::Flag0Primary),
                flag0(&[object, emitter])
            );
            assert_eq!(fixture.process.completion_53b890, 1);
        }
        // A port omission stands in for the draw native completes, so the
        // raised far returns (and is published again) as on a normal return.
        assert_eq!(fixture.context.far, 100);
        assert_eq!(fixture.context.far_color, 0x7654_3210);
        assert_eq!(
            arena.object(object).unwrap().base.flags & 0x0300_0000,
            0x0300_0000
        );
        assert_eq!(other_custody(&arena), before);
        assert_eq!(
            trace
                .borrow()
                .iter()
                .filter(|event| matches!(event, Event::Scene(ScenePhase::Fade(_))))
                .count(),
            2
        );
    }
}

#[test]
fn fade_publication_errors_retain_the_reached_far_store_and_stop_normal_suffixes() {
    let raw = executable_bytes();
    for failure in [0, 1] {
        let mut fixture = Fixture::new(&raw);
        fixture.context.view.basis.words[7..10].copy_from_slice(&[0, 1_f32.to_bits(), 0]);
        fixture.process.completion_53b890 = 0x71;
        let mut arena = arena();
        let object = actor(&mut arena, [0, 1000, 0], 23);
        arena.apply_payload_flags(object, 0x0100_0083).unwrap();
        let headers = arena.list_owners;
        let custody = other_custody(&arena);
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        host.scene_fail_at = Some(failure);
        assert!(matches!(
            fixture.run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation
            ),
            Err(Flag0ObjectDrawError::Host("scene publication stopped"))
        ));
        assert_eq!(fixture.context.far, if failure == 0 { 3076 } else { 100 });
        assert_eq!(fixture.context.far_color, 0x7654_3210);
        assert_eq!(fixture.process.completion_53b890, 0x71);
        assert_eq!(arena.list_owners, headers);
        assert_eq!(
            arena.object(object).unwrap().base.flags & 0x0300_0000,
            0x0300_0000
        );
        assert_eq!(other_custody(&arena), custody);
        assert_eq!(trace.borrow().len(), if failure == 0 { 3 } else { 5 });
    }
}

/// A flashing actor whose model a port omission stops (the drowning ACAM,
/// whose +1FC read stopped its model) still restores its flash fog and the
/// raised far: native always finishes the model and restores both, so the
/// next object, and the terrain after the pass, keep the world's fog.
#[test]
fn flashing_actor_omission_restores_the_fog_for_the_next_object() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.view.basis.words[7..10].copy_from_slice(&[0, 1_f32.to_bits(), 0]);
    let mut arena = arena();
    let object = actor(&mut arena, [0, 50, 0], 23);
    arena.apply_payload_flags(object, 0x0100_0083).unwrap();
    arena.actor_mut(object).unwrap().word_c8 = 31_250;
    arena.actor_mut(object).unwrap().word_ca = Some(0x55);
    let next = actor(&mut arena, [0, 0, 10], 24);
    arena.apply_payload_flags(next, 0x0100_0083).unwrap();
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    host.material = Some(0x00ff_0000);
    host.fail = Some((
        object,
        ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::ModelFeature {
            model_slot: 23,
            reason: "the drowning vehicle's unknown track phase".into(),
        }),
    ));
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        )
        .unwrap();
    assert_eq!(outcome.omissions.len(), 1);
    assert_eq!(outcome.omissions[0].object, object);
    assert_eq!(
        (
            fixture.context.near,
            fixture.context.far,
            fixture.context.far_color
        ),
        (7, 100, 0x7654_3210)
    );
    let models: Vec<_> = trace
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Model(packet, _, context) => Some((packet.source, *context)),
            _ => None,
        })
        .collect();
    assert_eq!(models.len(), 2);
    // The flashing actor drew inside its narrowed red fog...
    assert_eq!(models[0].0, object);
    assert_eq!(models[0].1.far_color, 0x00ff_0000);
    assert_ne!((models[0].1.near, models[0].1.far), (7, 100));
    // ...and the next object in the world's.
    assert_eq!(models[1].0, next);
    assert_eq!(
        (models[1].1.near, models[1].1.far, models[1].1.far_color),
        (7, 100, 0x7654_3210)
    );
}

#[test]
fn flash_ca_is_lazy_on_nonzero_c8_after_the_actual_far_publication() {
    let raw = executable_bytes();
    for c8 in [0, 1] {
        let mut fixture = Fixture::new(&raw);
        fixture.context.view.basis.words[7..10].copy_from_slice(&[0, 1_f32.to_bits(), 0]);
        let mut arena = arena();
        let object = actor(&mut arena, [0, 1000, 0], 23);
        arena.apply_payload_flags(object, 0x0100_0083).unwrap();
        arena.actor_mut(object).unwrap().word_c8 = c8; // explicit supplied current flash activity
        assert_eq!(arena.actor(object).unwrap().word_ca, None);
        let actor_before = arena.actor(object).unwrap().clone();
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        let result = fixture.run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        );
        if c8 == 0 {
            assert!(result.is_ok());
            assert_eq!(fixture.context.far, 100);
            assert_eq!(trace.borrow().len(), 5);
        } else {
            assert!(matches!(result, Err(Flag0ObjectDrawError::UnownedCallback {
                object: reached, reason: ObjectDrawCallbackBoundary::ActorFlashField { offset: 0xca }
            }) if reached == object));
            assert_eq!(fixture.context.far, 3076);
            assert_eq!(trace.borrow().len(), 3);
            assert_eq!(
                trace.borrow()[2],
                Event::Scene(ScenePhase::Fade(SceneFade::new([7, 3076])))
            );
        }
        assert_eq!(*arena.actor(object).unwrap(), actor_before);
    }
}

#[test]
fn null_component_preserves_pending_while_actual_empty_behavior_follows_normal_late_return() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = 1;
    let mut arena = arena();
    let plain = visual(
        &mut arena,
        ObjectClassKind::VisualBase,
        [0, 0, 100],
        u16::MAX,
    );
    let object = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3, 3],
            },
            &mut 17,
        )
        .unwrap();
    position(&mut arena, object, [0xfff0, 0xff00, 10], 0);
    model_word(&mut arena, object, 23);
    let ObjectClass::Actor(actor) = &mut arena.owned_mut(object).unwrap().state.class else {
        unreachable!()
    };
    let ActorExtension::Derived(derived) = &mut actor.extension else {
        unreachable!()
    };
    derived.byte_13d = Some(1); // supplied current guard mode through real owner
    arena.admit_derived_actor(object).unwrap();
    arena.apply_payload_flags(plain, 0x0100_0003).unwrap();
    arena.apply_payload_flags(object, 0x0100_0003).unwrap();
    let before = other_custody(&arena);
    let behavior = arena.derived_actor(object).unwrap().behavior_140.clone();
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(
        (
            outcome.near_callbacks,
            outcome.common_models,
            outcome.late_models,
            outcome.empty_behaviors
        ),
        (0, 0, 1, 1)
    );
    assert_eq!(
        arena.object(plain).unwrap().base.flags & 0x0100_0001,
        0x0100_0001
    );
    assert_eq!(arena.object(object).unwrap().base.flags & 0x0100_0000, 0);
    assert_eq!(arena.derived_actor(object).unwrap().behavior_140, behavior);
    assert_eq!(other_custody(&arena), before);
    let events = trace.borrow();
    let Event::Model(packet, root, context) = events[2] else {
        panic!("late model follows both admissions")
    };
    assert_eq!(
        (packet.source, packet.position, root),
        (object, [-16, -256, 10], ModelRootMode::ManagerLate)
    );
    assert_eq!(context, fixture.context);
    assert_eq!(events.len(), 3);
}

#[test]
fn common_lighting_uses_x_major_signed_rgb_and_packet_basis_without_state_restore() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.view.eye = [256, 0, 300];
    fixture
        .grid
        .add(
            TerrainLightRequest {
                world_xz: [256, 384],
                radius: 1024,
                rgb: [-16, 8, -127],
            },
            TerrainLightPolicy {
                enabled: true,
                spacing: TerrainSpacing::Units128,
                color: TerrainLightColor::Unmodified,
            },
        )
        .unwrap();
    assert_eq!(
        &fixture.grid.cells()[(2 * 90 + 3) * 3..(2 * 90 + 3) * 3 + 3],
        &[240, 8, 129]
    );
    let mut shared = MenuBasis::INITIAL;
    shared.words[0] = 0x1234_5678;
    shared.words[1] = (-1_f32).to_bits();
    fixture.process.shared_basis = shared;
    let grid = fixture.grid.cells().to_vec();
    let mut arena = arena();
    let object = visual(&mut arena, ObjectClassKind::VisualBase, [256, 0, 384], 23);
    let before = arena.object(object).unwrap().clone();
    let custody = other_custody(&arena);
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(outcome.common_models, 1);
    let events = trace.borrow();
    assert_eq!(events[1], Event::Scene(ScenePhase::Ambient([9, 9, -64])));
    assert_eq!(
        events[2],
        Event::Scene(ScenePhase::Directional {
            direction: [0.25, -0.5, 0.75],
            rgb: [84, 12, 128]
        })
    );
    let Event::Model(packet, root, _) = events[3] else {
        panic!("ambient/directional precede model")
    };
    assert_eq!((packet.basis, root), (shared, ModelRootMode::CommonVisual));
    assert_eq!(events.len(), 4);
    let mut expected = before;
    expected.base.flags |= 0x0200_0000;
    assert_eq!(*arena.object(object).unwrap(), expected);
    assert_eq!(other_custody(&arena), custody);
    assert_eq!(fixture.grid.cells(), grid);
}

#[test]
fn outside_light_uses_authored_word_and_fresh_flag80_bypasses_both_light_publications() {
    let mut raw = executable_bytes();
    // Supplied same-size executable data domain, not original asset mutation.
    raw[0x0012_6068..0x0012_606c].copy_from_slice(&[0x80, 0x7f, 0xff, 0x42]);
    for bypass in [false, true] {
        let mut fixture = Fixture::new(&raw);
        fixture.context.view.eye = [0xf000, 0, 0xeff0];
        let mut arena = arena();
        let object = visual(
            &mut arena,
            ObjectClassKind::RotatingVisualBase,
            [0xf000, 0, 0xf000],
            23,
        );
        if bypass {
            arena.apply_payload_flags(object, 0x83).unwrap();
        }
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        fixture
            .run(
                &mut arena,
                &mut terrain,
                &mut host,
                Flag0ObjectDrawFailurePolicy::Stop,
            )
            .unwrap();
        let events = trace.borrow();
        if bypass {
            assert_eq!(events.len(), 2);
            assert!(matches!(
                events[1],
                Event::Model(_, ModelRootMode::CommonVisual, _)
            ));
        } else {
            assert_eq!(
                events[1],
                Event::Scene(ScenePhase::Ambient([-103, 128, 62]))
            );
            assert_eq!(
                events[2],
                Event::Scene(ScenePhase::Directional {
                    direction: [0.25, -0.5, 0.75],
                    rgb: [0, 131, 254]
                })
            );
        }
    }
}

#[test]
fn terrain_light_gate_and_common_ffff_disable_preserve_their_distinct_prefixes() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    let mut arena = arena();
    let objects: Vec<_> = [0, 1]
        .into_iter()
        .map(|gate| {
            let object = visual(
                &mut arena,
                ObjectClassKind::TerrainLight,
                [0, 0, 10],
                u16::MAX,
            );
            arena
                .initialize_terrain_light_payload(
                    object,
                    3,
                    TerrainLight {
                        channels: [-5, 7, 11],
                        radius: 123,
                        draw_gate: gate,
                    },
                )
                .unwrap();
            object
        })
        .collect();
    let before: Vec<_> = objects
        .iter()
        .map(|&id| arena.object(id).unwrap().clone())
        .collect();
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!((outcome.near_callbacks, outcome.common_models), (2, 0));
    assert_eq!(trace.borrow().len(), 2);
    assert_eq!(arena.list(ObjectList::Flag0Primary), flag0(&[objects[0]]));
    for (i, &object) in objects.iter().enumerate() {
        let mut expected = before[i].clone();
        expected.base.flags |= 0x0200_0000;
        if i == 1 {
            expected.base.flags &= !1;
            expected.base.dirty_word |= 2;
        }
        assert_eq!(*arena.object(object).unwrap(), expected);
    }
}

/// Selector 54's 00461C60 is selector 19's gate before 0047AFC0, whose basis
/// comes from +48: 004026B0 answers the light's own, not the shared one.
#[test]
fn rotating_terrain_light_draws_behind_its_gate_with_its_own_basis() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    let mut shared = MenuBasis::INITIAL;
    shared.words[1] = (-1_f32).to_bits();
    fixture.process.shared_basis = shared;
    let mut arena = arena();
    let objects: Vec<_> = [0, 1]
        .into_iter()
        .map(|gate| {
            let object = visual(
                &mut arena,
                ObjectClassKind::RotatingTerrainLight,
                [0, 0, 10],
                23,
            );
            arena
                .initialize_terrain_light_payload(
                    object,
                    3,
                    TerrainLight {
                        channels: [-5, 7, 11],
                        radius: 123,
                        draw_gate: gate,
                    },
                )
                .unwrap();
            let ObjectClass::RotatingTerrainLight { rotating, .. } =
                &mut arena.owned_mut(object).unwrap().state.class
            else {
                unreachable!()
            };
            rotating.basis.words[2] = 0.5_f32.to_bits();
            object
        })
        .collect();
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!((outcome.near_callbacks, outcome.common_models), (2, 1));
    let models: Vec<_> = trace
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Model(packet, root, _) => Some((*packet, *root)),
            _ => None,
        })
        .collect();
    assert_eq!(models.len(), 1);
    let (packet, root) = models[0];
    assert_eq!(
        (packet.source, root),
        (objects[1], ModelRootMode::CommonVisual)
    );
    let mut own = MenuBasis::INITIAL;
    own.words[2] = 0.5_f32.to_bits();
    assert_eq!(packet.basis, own);
}

/// 004323B0, the flamethrower's +2C: its own basis (00521A40's +48 is
/// 004026B0) turned a quarter in Y-Z (0047A720(0x4000)), then 0047AFC0 with
/// its +42 model (H8 110, `exwave2`, the shockwave) at the thrower's
/// position and again at the flame's root, then +78 and the saved basis
/// back. The draw stopped the whole object pass (NotVisual) before both arms.
#[test]
fn the_flamethrower_draws_its_wave_at_the_thrower_and_at_the_root() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    let mut shared = MenuBasis::INITIAL;
    shared.words[1] = (-1_f32).to_bits();
    fixture.process.shared_basis = shared;
    let mut arena = arena();
    let flame = visual(&mut arena, ObjectClassKind::Selector76, [0, 0, 10], 110);
    let saved = {
        let state = arena.selector76_mut(flame).unwrap();
        state.owner_position_78 = Some([0, 0, 10]);
        state.root_7e = Some([3, 0, 118]);
        state.rotating.basis.words[2] = 0.5_f32.to_bits();
        state.rotating.basis
    };
    let math = inf_formats::native_basis::NativeBasisMath::new(
        inf_formats::parse_frontend_sine_table(
            fixture.executable.data(),
            inf_formats::FRONTEND_SINE_TABLE_ADDRESS,
        )
        .unwrap()
        .values,
    );
    let mut turned = saved;
    math.rotate_in_plane(
        &mut turned,
        0x4000,
        inf_formats::native_basis::RotationPlane::Yz,
    );
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    host.math = Some(math);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        )
        .unwrap();
    assert_eq!(outcome.common_models, 2);
    let models: Vec<_> = trace
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Model(packet, root, _) => Some((*packet, *root)),
            _ => None,
        })
        .collect();
    let at = |position| ObjectModelPacket {
        source: flame,
        model_slot: 110,
        position,
        basis: MenuBasis {
            words: turned.words,
        },
    };
    assert_eq!(
        models,
        [
            (at([0, 0, 10]), ModelRootMode::CommonVisual),
            (at([3, 0, 118]), ModelRootMode::CommonVisual),
        ]
    );
    assert_eq!(
        arena.object(flame).unwrap().base.position_words,
        Some([0, 0, 10])
    );
    assert_eq!(arena.selector76(flame).unwrap().rotating.basis, saved);
}

#[test]
fn unavailable_callback_fields_are_per_object_omissions_without_eager_model_or_process_reads() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    let mut arena = arena();
    let unknown = arena.allocate(ObjectClassKind::VisualBase);
    arena.owned_mut(unknown).unwrap().state.base.flags |= 0x0200_0000;
    let light = visual(&mut arena, ObjectClassKind::TerrainLight, [0, 0, 10], 23);
    let last = arena.allocate(ObjectClassKind::Base);
    position(&mut arena, last, [0, 0, 10], 0);
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        )
        .unwrap();
    assert_eq!(
        outcome.omissions,
        vec![
            Flag0ObjectDrawOmission {
                object: unknown,
                reason: ObjectDrawCallbackBoundary::Position
            },
            Flag0ObjectDrawOmission {
                object: light,
                reason: ObjectDrawCallbackBoundary::TerrainLightStorage
            },
        ]
    );
    assert_eq!(
        arena.object(unknown).unwrap().base.flags & 0x0200_0000,
        0x0200_0000
    );
    assert_eq!(
        arena.object(light).unwrap().base.flags & 0x0200_0000,
        0x0200_0000
    );
    assert_eq!(
        trace.borrow().as_slice(),
        &[
            Event::Terrain(light, Some([0, 0, 10])),
            Event::Terrain(last, Some([0, 0, 10]))
        ]
    );
    assert_eq!(
        arena.list(ObjectList::Flag0Primary),
        flag0(&[unknown, light])
    );
}

#[test]
fn vehicle_attract_branch_reads_current_byte_only_after_admission_and_skips_ordinary_tail() {
    let raw = executable_bytes();
    for attract in [0, 0x80] {
        let mut fixture = Fixture::new(&raw);
        fixture.network_flag = 0x100; // Genuine ordinary required-network boundary.
        let value = terrain();
        let mut arena = arena();
        let object = vehicle(&mut arena, &value);
        position(&mut arena, object, [0, 0, 10], 0);
        let before = arena.object(object).unwrap().clone();
        let (mut terrain, mut host, trace) = services(&value);
        host.attract = attract;
        let result = fixture.run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::Stop,
        );
        let mut expected = before;
        expected.base.flags |= 0x0200_0000;
        if attract == 0 {
            assert!(matches!(
                result,
                Err(Flag0ObjectDrawError::UnownedCallback {
                    reason: ObjectDrawCallbackBoundary::VehicleNetwork,
                    ..
                })
            ));
        } else {
            assert!(result.is_ok());
            expected.base.flags &= !1;
            expected.base.dirty_word |= 2;
        }
        assert_eq!(*arena.object(object).unwrap(), expected);
        assert_eq!(
            trace.borrow().as_slice(),
            &[
                Event::Terrain(object, Some([0, 0, 10])),
                Event::Attract(attract)
            ]
        );
    }
}

#[test]
fn late_ffff_is_a_required_model_boundary_and_keeps_pending_on_omission() {
    let raw = executable_bytes();
    let mut fixture = Fixture::new(&raw);
    fixture.context.far = 1;
    let mut arena = arena();
    let object = actor(&mut arena, [0, 0, 100], u16::MAX);
    arena.apply_payload_flags(object, 0x0100_0003).unwrap();
    let value = terrain();
    let (mut terrain, mut host, trace) = services(&value);
    host.fail = Some((
        object,
        ObjectDrawServiceError::UnownedCallback(ObjectDrawCallbackBoundary::ModelFeature {
            model_slot: -1,
            reason: "required late H8 source".into(),
        }),
    ));
    let outcome = fixture
        .run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        )
        .unwrap();
    assert_eq!(outcome.late_models, 0);
    assert_eq!(outcome.omissions.len(), 1);
    assert_eq!(
        arena.object(object).unwrap().base.flags & 0x0100_0001,
        0x0100_0001
    );
    assert!(matches!(
        trace.borrow()[1],
        Event::Model(
            ObjectModelPacket { model_slot: -1, .. },
            ModelRootMode::ManagerLate,
            _
        )
    ));
}

#[test]
fn hard_backend_and_actual_arena_custody_errors_stop_even_with_omission_policy() {
    let raw = executable_bytes();
    let value = terrain();
    // Required host hard-stop retains ambient/directional and moved-node prefix.
    let mut fixture = Fixture::new(&raw);
    fixture.process.completion_53b890 = 0x71;
    let mut arena = arena();
    let object = visual(&mut arena, ObjectClassKind::VisualBase, [0, 0, 10], 23);
    let headers = arena.list_owners;
    let (mut terrain, mut host, trace) = services(&value);
    host.fail = Some((object, ObjectDrawServiceError::Backend("queue stopped")));
    assert!(matches!(
        fixture.run(
            &mut arena,
            &mut terrain,
            &mut host,
            Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation
        ),
        Err(Flag0ObjectDrawError::Host("queue stopped"))
    ));
    assert_eq!(trace.borrow().len(), 4);
    assert_eq!(arena.list_owners, headers);
    assert_eq!(fixture.process.completion_53b890, 0x71);

    // Actual fresh receiver error after a service destroys it is not an unowned
    // scalar omission and never silently reinstates the allocation/registry.
    let mut arena = self::arena();
    let object = actor(&mut arena, [0, 0, 10], 23);
    let (mut terrain, mut host, _) = services(&value);
    terrain.effects.push_back(TerrainEffect::Destroy);
    let headers = arena.list_owners;
    assert!(matches!(fixture.run(&mut arena, &mut terrain, &mut host,
        Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation),
        Err(Flag0ObjectDrawError::Object(ObjectError::UnknownObject(reached))) if reached == object));
    assert!(arena.object(object).is_err());
    assert_eq!(arena.list_owners, headers);

    // Supplied typed-list corruption, no raw native-fault equivalence claim.
    let mut arena = self::arena();
    let object = arena.allocate(ObjectClassKind::Base);
    let head = arena.list_owners[ObjectList::Flag0Primary as usize];
    arena.lists[head][0].kind = ObjectNodeKind::Flag1;
    let before = format!("{arena:?}");
    let (mut terrain, mut host, trace) = services(&value);
    assert!(matches!(fixture.run(&mut arena, &mut terrain, &mut host,
        Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation),
        Err(Flag0ObjectDrawError::NodeCustody(ObjectNode { object: reached, kind: ObjectNodeKind::Flag1 })) if reached == object));
    assert_eq!(format!("{arena:?}"), before);
    assert!(trace.borrow().is_empty());
}

#[test]
fn retails_cone_is_45_degrees_with_its_native_wrapping() {
    use crate::object_runtime::ObjectDrawCone;
    // Depth 1000: retail admits under 2 000 000 squared, so 45 degrees.
    assert!(ObjectDrawCone::Retail.admits(1_999_999, 1000));
    assert!(!ObjectDrawCone::Retail.admits(2_000_000, 1000));
    // 2 * 50000^2 wraps to 705032704.
    assert!(ObjectDrawCone::Retail.admits(705_032_703, 50_000));
    assert!(!ObjectDrawCone::Retail.admits(705_032_704, 50_000));
}

/// The rock's +2C 00454000 against the original under Unicorn
/// (`selector44_native.json`'s draws, `check_selector44.py`):
/// the context's fade bounds both 0x3FFF and published (0048FB00), the
/// common draw with them, then the context's own back and published. The
/// harness records 0047D910 as one call; here it runs, and the rock sits
/// below its ground excess so the common draw publishes no fade of its own.
#[test]
fn the_rock_draws_unfogged_as_the_original() {
    let raw = executable_bytes();
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector44_native.json"
    ))
    .unwrap();
    let mut draws = 0;
    for case in receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["input"]["mode"] == "draw")
    {
        let fade = &case["input"]["fade"];
        let bounds = [0, 1].map(|n| fade[n].as_u64().unwrap() as u32 as i32);
        let mut fixture = Fixture::new(&raw);
        [fixture.context.near, fixture.context.far] = bounds;
        let mut arena = arena();
        let rock = arena.allocate_selector44(allocation()).unwrap();
        position(&mut arena, rock, [0, 0, 10], 0);
        model_word(&mut arena, rock, 23);
        let value = terrain();
        let (mut terrain, mut host, trace) = services(&value);
        let mut request = Flag0ObjectDrawRequest {
            context: &mut fixture.context,
            cone: crate::object_runtime::ObjectDrawCone::Retail,
            process: &mut fixture.process,
            network_flag: &fixture.network_flag,
            terrain: &mut terrain,
            lighting: ObjectDrawLighting {
                grid: &fixture.grid,
                tint_word: &mut fixture.tint,
                direction: [0.25, -0.5, 0.75],
                spacing: fixture.spacing,
                executable: fixture.executable,
            },
            host: &mut host,
            failure_policy: Flag0ObjectDrawFailurePolicy::Stop,
        };
        arena
            .draw_selector44(rock, &mut request, &mut Flag0ObjectDrawOutcome::default())
            .unwrap();
        let events: Vec<serde_json::Value> = trace
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Scene(ScenePhase::Fade(fade)) => Some(json!({"kind": "fade",
                    "ecx": fade.bounds[0] as u32, "edx": fade.bounds[1] as u32})),
                Event::Model(_, _, context) => Some(json!({"kind": "draw", "context": true,
                    "fade": [context.near as u32, context.far as u32]})),
                _ => None,
            })
            .collect();
        assert_eq!(json!(events), case["events"], "{fade}");
        assert_eq!(
            json!([fixture.context.near as u32, fixture.context.far as u32]),
            case["output"]["fade"]
        );
        draws += 1;
    }
    assert_eq!(draws, 4);
}
