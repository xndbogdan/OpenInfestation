use super::*;
use crate::{
    factory_shop::FactoryShopTables,
    frontend_assets::FrontendAssets,
    grant_text::{GrantTextData, GrantTextPairClassification},
    object_runtime::{ActorExtension, CameraSettings, ShownMenuOwner},
    process_particles::factories::ParticleFactoryProcessState,
    process_sound::{
        ProcessSoundRecordError, ProcessSoundRuntime, ProcessSoundVoiceRef,
        ProcessSoundVoiceTeardown,
    },
    terrain_runtime::TerrainSpacing,
    world_backend::{OfflineWorldLoadRequest, WorldSession},
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::FrontendExecutable;

fn loaded() -> (ResidentRuntime, FrontendAssets, WorldSession) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let offline = crate::offline_prepare::offline_game_with_selection(&grants, Some(18)).unwrap();
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut owner = crate::world_backend::WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
        mission: offline.mission,
        player: offline.player,
        previous_world: 1,
        visibility: None,
    })
    .unwrap();
    let mut listener_effects = crate::world_backend::EmptyListenerEffects;
    let mut furniture_radius =
        crate::terrain_runtime::ProcessFurnitureRadius::native_startup(executable).unwrap();
    let metadata = crate::world_backend::load_offline_world(
        &mut runtime,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                TerrainSpacing::Units128,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut owner,
            listener_effects: &mut listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            process: Some(&ParticleFactoryProcessState::native_startup()),
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    (runtime, assets, WorldSession::from_loaded(owner, metadata))
}

/// Level 1's truck factory, Infest01's `0384F080`, and its captured dock.
const TRUCK_FACTORY_SLOT: usize = 1;
const TRUCK_DOCK: [u16; 3] = [0x90dd, 0x50, 0x842b];

struct NoVoices;
impl ProcessSoundVoiceTeardown for NoVoices {
    fn delete_voice(
        &mut self,
        _: ProcessSoundRef,
        _: ProcessSoundVoiceRef,
    ) -> Result<(), ProcessSoundRecordError> {
        Ok(())
    }
}

struct Host<'a> {
    process: ParticleFactoryProcessState,
    resident: &'a ResidentRuntime,
    executable: FrontendExecutable<'a>,
    text: GrantTextData<'static>,
    sounds: ProcessSoundRuntime,
    tables: FactoryShopTables,
    events: Vec<String>,
    /// Level 1's loose scientists, standing in for 00460310's creations
    /// and, without an item world, for the factory's +4C items.
    evacuees: Vec<ObjectId>,
    /// The world a bought item is really built in (0047AAF0).
    items: Option<ItemWorld<'a>>,
    rng: u32,
}

struct ItemWorld<'a> {
    terrain: &'a crate::terrain_runtime::TerrainRuntime,
    listeners: &'a mut crate::object_runtime::CreationListeners,
    world_flags: u32,
    sine: &'a [u16; 4096],
}

/// Records the take's flash and sounds.
struct CarryRecorder<'a>(&'a mut Vec<String>);

impl crate::object_runtime::VehicleCarryHost for CarryRecorder<'_> {
    type Error = String;
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: crate::object_runtime::VehicleCarryEffect,
        _: &mut u32,
    ) -> Result<(), String> {
        self.0.push(format!("{effect:?}"));
        Ok(())
    }
}

impl FactoryStaffHost for Host<'_> {
    type Error = String;
    fn process(&self) -> &ParticleFactoryProcessState {
        &self.process
    }
    fn grants(&self) -> LiveGrantContext<'_> {
        LiveGrantContext {
            process: Some(&self.process),
            executable: Some(self.executable),
            resident: Some(self.resident),
            text_data: Some(&self.text),
        }
    }
    fn shop_tables(&self) -> Result<&FactoryShopTables, String> {
        Ok(&self.tables)
    }
    fn flash(&mut self, _: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        self.events.push(format!("flash {position:?}"));
        Ok(())
    }
    fn complete_placement(&mut self, index: i8) -> Result<(), String> {
        self.events.push(format!("placement {index}"));
        Ok(())
    }
}

impl FactoryHost for Host<'_> {
    fn broadcast_skipped(&mut self, receivers: &[String]) {
        for receiver in receivers {
            self.events.push(format!("skipped {receiver}"));
        }
    }
    fn camera_settings(&self) -> CameraSettings {
        CameraSettings {
            first_person: 0,
            camera_style: 0,
        }
    }
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<ObjectId, String> {
        let handle = arena.camera_handle().ok_or("no camera")?;
        arena
            .resolve_actor_handle(handle)
            .map_err(|error| format!("{error:?}"))?
            .ok_or_else(|| "dead camera".into())
    }
    fn sound(
        &mut self,
        _: &ObjectArena,
        sound: u32,
        position: [u16; 3],
        volume: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.events
            .push(format!("sound {sound:x} {position:?} {volume:x} {pitch:x}"));
        Ok(())
    }
    fn create_loop(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        sound: u32,
    ) -> Result<Option<ProcessSoundRef>, String> {
        self.events.push(format!("loop {sound:x}"));
        self.sounds
            .create_persistent(self.resident, sound as i16, 0x10000, 0x10000, 0, || {
                Ok::<_, std::convert::Infallible>([0, 0, 0])
            })
            .map(Some)
            .map_err(|error| format!("{error:?}"))
    }
    fn update_loop(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        volume: u32,
    ) -> Result<(), String> {
        self.events.push(format!("volume {volume:x}"));
        Ok(())
    }
    fn release_loop(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.events.push("release".into());
        self.sounds
            .delete_record(reference, &mut NoVoices)
            .map_err(|error| format!("{error:?}"))
    }
    fn text(&mut self, _: &mut ObjectArena, _: PlayerId, text: u16) -> Result<(), String> {
        self.events.push(format!("text {text:x}"));
        Ok(())
    }
    fn command(
        &mut self,
        _: &mut ObjectArena,
        _: PlayerId,
        raw_id: u32,
        silent: u32,
    ) -> Result<(), String> {
        self.events.push(format!("command {raw_id:x} {silent}"));
        Ok(())
    }
    /// Records the grant and sets its upgrade bit, as 0046B2B0 does.
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
        amount: i16,
    ) -> Result<(), String> {
        self.events.push(format!("grant {id:x} {amount}"));
        if id != 4 {
            let bits = &mut arena.world_player_mut(player).unwrap().bits_100;
            bits[usize::from(id >> 5)] |= 1 << (id & 31);
        }
        Ok(())
    }
    /// As Infest01's: on the ground, team 1, flags 0x48100867.
    fn create_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        self.events.push(format!("create {position:?}"));
        if self.evacuees.is_empty() {
            return Ok(None);
        }
        let scientist = self.evacuees.remove(0);
        let base = &mut arena.owned_mut(scientist).unwrap().state.base;
        base.position_words = Some([position[0], 0, position[2]]);
        base.flags = 0x4810_0867;
        base.dirty_word = 0;
        arena.actor_mut(scientist).unwrap().word_b4 = 1;
        Ok(Some(scientist))
    }
    fn carry_scientist(
        &mut self,
        arena: &mut ObjectArena,
        vehicle: ObjectId,
        scientist: ObjectId,
    ) -> Result<bool, String> {
        let base = &arena.object(scientist).unwrap().base;
        self.events.push(format!(
            "carry at {:?} team {} +38 {:x}",
            base.position_words.unwrap(),
            arena.actor(scientist).unwrap().word_b4,
            base.dirty_word
        ));
        let context = crate::object_runtime::VehicleCarryContext {
            process: &self.process,
            executable: self.executable,
            resident: self.resident,
            text_data: Some(&self.text),
        };
        let mut rng = 0;
        arena
            .attach_vehicle_item(
                vehicle,
                scientist,
                context,
                &mut CarryRecorder(&mut self.events),
                &mut rng,
            )
            .map_err(|error| error.to_string())
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn create_item(
        &mut self,
        arena: &mut ObjectArena,
        factory: ObjectId,
        resource: ActorChildResource,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, String> {
        self.events.push(format!("item {resource:?} {position:?}"));
        let Some(world) = self.items.as_mut() else {
            // A stand-in placed as built, still on its own team 1.
            if self.evacuees.is_empty() {
                return Ok(None);
            }
            let item = self.evacuees.remove(0);
            let base = &mut arena.owned_mut(item).unwrap().state.base;
            base.position_words = Some(position);
            base.dirty_word = 0;
            arena.actor_mut(item).unwrap().word_b4 = 1;
            return Ok(Some(item));
        };
        let math = inf_formats::native_basis::NativeBasisMath::new(*world.sine);
        let mut effects = crate::session_platform::world_actor_factory::RequiredListenerEffects;
        let mut host = crate::session_platform::world_actor_factory::WorldActorChildHost {
            residency: self.resident,
            executable: self.executable,
            process: &self.process,
            terrain: world.terrain,
            world_flags: world.world_flags,
            clock: 0,
            sine: world.sine,
            math: &math,
            camera_settings: None,
            world_start: None,
            listeners: &mut *world.listeners,
            listener_effects: &mut effects,
        };
        let mut item = None;
        let built = arena
            .create_actor_child(
                factory,
                crate::object_runtime::ActorChildRequest {
                    resource,
                    position: Some(position),
                },
                Some(&mut item),
                &mut host,
                &mut self.rng,
            )
            .map_err(|error| error.to_string())?;
        Ok(built.and(item))
    }
}

/// The shown menu's current owner and title row (draw callback and text).
fn shop_title(arena: &ObjectArena, player: PlayerId) -> Option<(ShownMenuOwner, u32, u32)> {
    let menu = arena.player_shown_menu(player)?;
    let owner = menu.current_owner()?;
    let records = menu.stack.current()?.data().records();
    let title = records.items[1];
    Some((owner, title.draw_callback, title.draw_arguments[0]))
}

/// The resident runtime and assets the host borrows.
struct Fixture {
    resident: ResidentRuntime,
    assets: FrontendAssets,
}

struct Level1 {
    world: WorldSession,
    factory: ObjectId,
    vehicle: ObjectId,
    player: PlayerId,
}

fn level1() -> (Fixture, Level1) {
    let (resident, assets, mut world) = loaded();
    let factory = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            matches!(
                world.arena.actor(actor).unwrap().extension,
                ActorExtension::Selector15(_)
            )
        })
        .find(|actor| actor.slot == TRUCK_FACTORY_SLOT)
        .unwrap();
    world.arena.selector15_mut(factory).unwrap().dock_190 = Some(TRUCK_DOCK);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    (
        Fixture { resident, assets },
        Level1 {
            world,
            factory,
            vehicle,
            player,
        },
    )
}

impl Fixture {
    fn host(&self, null: ActorHandle) -> Host<'_> {
        let executable = FrontendExecutable::parse(self.assets.executable_bytes()).unwrap();
        let mut process = ParticleFactoryProcessState::native_startup();
        process.process_null_handle = Some(null);
        Host {
            process,
            resident: &self.resident,
            executable,
            text: GrantTextData {
                null_substitute: None,
                pair_classification: Some(GrantTextPairClassification {
                    colon_high: Some(0),
                    space_high: Some(0),
                }),
            },
            sounds: ProcessSoundRuntime::new(executable).unwrap(),
            tables: FactoryShopTables::parse(executable).unwrap(),
            events: Vec::new(),
            evacuees: Vec::new(),
            items: None,
            rng: 0,
        }
    }
}

impl Level1 {
    fn null(&self) -> ActorHandle {
        self.world.arena.actor_handles().unwrap().null_handle()
    }

    fn place(&mut self, position: [u16; 3], velocity: [u16; 3]) {
        let arena = &mut self.world.arena;
        arena
            .owned_mut(self.vehicle)
            .unwrap()
            .state
            .base
            .position_words = Some(position);
        arena.actor_mut(self.vehicle).unwrap().words_8e_92 = velocity;
    }
}

/// `0042EC60` over the truck factory's pad: the vehicle's radius is 109.
#[test]
fn the_pad_test_follows_0042ec60() {
    let (_fixture, mut level) = level1();
    let (factory, vehicle) = (level.factory, level.vehicle);
    // Inside twice its radius squared, still and armed: it docks.
    level.place(TRUCK_DOCK, [0; 3]);
    assert!(level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    // Too fast (speed 701), or disarmed: no.
    level.place(TRUCK_DOCK, [701, 0, 0]);
    assert!(!level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    level.place(TRUCK_DOCK, [700, 0, 0]);
    assert!(level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    level.world.arena.selector15_mut(factory).unwrap().byte_1bd = 0;
    assert!(!level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    // Between two and four radii squared: nothing changes.
    let [x, y, z] = TRUCK_DOCK;
    level.place([x + 200, y, z], [0; 3]);
    assert!(!level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    let state = level.world.arena.selector15(factory).unwrap();
    assert_eq!((state.byte_1bd, state.word_1c4), (0, 0));
    // Height counts half: 2 * 109 = 218 units up is still on the pad.
    level.place([x, y + 300, z], [0; 3]);
    level.world.arena.selector15_mut(factory).unwrap().byte_1bd = 1;
    assert!(level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    // Farther than twice the radius: re-armed, and the cooldown is 50 µs
    // per unit of sqrt(d² - 3r²), at most 2 s.
    level.world.arena.selector15_mut(factory).unwrap().byte_1bd = 0;
    level.place([x + 1000, y, z], [0; 3]);
    assert!(!level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap());
    let state = level.world.arena.selector15(factory).unwrap();
    let root = isqrt_48efc0(1000 * 1000 - 3 * 109 * 109);
    assert_eq!((state.byte_1bd, state.word_1c4), (1, root * 50));
    level.place([x - 32000, y, z - 32000], [0; 3]);
    level
        .world
        .arena
        .factory_dock_test::<()>(factory, vehicle)
        .unwrap();
    assert_eq!(
        level.world.arena.selector15(factory).unwrap().word_1c4,
        2_000_000
    );
}

/// `0042EE30` with the cooldown spent and the vehicle on the pad: docking
/// merges the factory's IDs into the player's (the truck, 2), copies the
/// player's mask back, binds the shop and shows it on the shop's viewport,
/// then sets commands 3, 4 and 0x26 silently. The broadcast re-arms the
/// factory and sets the message's +34.
#[test]
fn docking_binds_the_shop() {
    let (fixture, mut level) = level1();
    level.place(TRUCK_DOCK, [0; 3]);
    let mut host = fixture.host(level.null());
    let (factory, player) = (level.factory, level.player);
    assert!(!level.world.arena.player_upgrade_owned(player, 2).unwrap());
    let before = level.world.arena.world_player(player).unwrap().grants_9c;
    level
        .world
        .arena
        .tick_selector15(factory, 125_000, &mut host)
        .unwrap();
    let arena = &level.world.arena;
    let state = arena.selector15(factory).unwrap();
    let vehicle_handle = arena.actor(level.vehicle).unwrap().handle;
    assert_eq!(state.handle_188, vehicle_handle);
    assert_eq!(state.word_1b8, 1);
    assert_eq!(state.byte_1bd, 1);
    let owner = arena.world_player(player).unwrap();
    assert_eq!(owner.handle_dc, arena.actor(factory).unwrap().handle);
    assert_eq!(owner.grants_9c[0], before[0] | 4);
    assert_eq!(state.grants_198.mask_04, owner.grants_9c);
    let message = arena.player_message(player).unwrap();
    assert_eq!((message.shown_00(), message.byte_34()), (1, 1));
    let menu = arena.player_shown_menu(player).unwrap();
    assert_eq!(menu.stack.spec().viewport, [470, 0, 980, 700]);
    assert_eq!(
        menu.stack.current().unwrap().data().records().items.len(),
        6
    );
    assert_eq!(
        shop_title(arena, player),
        Some((ShownMenuOwner::Shop { factory }, 0x453a80, 0x308))
    );
    assert_eq!(
        &host.events[..3],
        ["command 3 1", "command 4 1", "command 26 1"]
    );
}

/// A factory of another team turns the vehicle away; team 0x12 also sets
/// command 0x1D.
#[test]
fn other_teams_deny_access() {
    for (team, expected) in [
        (
            5,
            vec!["text 2fb", "command 3 1", "command 4 1", "command 26 1"],
        ),
        (
            0x12,
            vec![
                "text 2fb",
                "command 1d 0",
                "command 3 1",
                "command 4 1",
                "command 26 1",
            ],
        ),
    ] {
        let (fixture, mut level) = level1();
        level.place(TRUCK_DOCK, [0; 3]);
        level.world.arena.actor_mut(level.factory).unwrap().word_b4 = team;
        let mut host = fixture.host(level.null());
        level
            .world
            .arena
            .tick_selector15(level.factory, 125_000, &mut host)
            .unwrap();
        let state = level.world.arena.selector15(level.factory).unwrap();
        assert_eq!(state.handle_188, level.null());
        assert_eq!((state.byte_1bd, state.word_1b8), (0, 0));
        let events: Vec<_> = host
            .events
            .iter()
            .filter(|event| !event.starts_with("loop") && !event.starts_with("volume"))
            .cloned()
            .collect();
        assert_eq!(events, expected);
    }
}

/// `0042F3E7..0042F44B`: a smoke grenade's alert (0xD) takes a mutant
/// factory's whole staff, tells the first player "Factory neutralised" (H0
/// 0x30C) and turns the factory team 0, so the vehicle on its pad docks. A
/// remote factory ignores it; an empty one turns team 0 without the text.
#[test]
fn smoke_neutralises_a_mutant_factory() {
    use crate::object_runtime::{ActorMessage, ActorMessageReply};
    let smoke = ActorMessage {
        sender: None,
        distance_squared: 0,
        code: 0xd,
        argument: 0,
    };
    let (fixture, mut level) = level1();
    let (factory, player) = (level.factory, level.player);
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerActorLookupContext {
        process: Some(&process),
    };
    let arena = &mut level.world.arena;
    let held = |arena: &ObjectArena| {
        (
            arena.actor(factory).unwrap().word_b4,
            arena.selector15(factory).unwrap().intermediate.word_140,
        )
    };
    assert!(arena.selector15(factory).unwrap().intermediate.word_13c >= 2);
    arena.actor_mut(factory).unwrap().word_b4 = 0x12;
    arena.selector15_mut(factory).unwrap().intermediate.word_140 = 2;
    arena.owned_mut(factory).unwrap().state.base.flags |= 0x8000_0000;
    let reply = arena.receive_actor_message(factory, smoke, context, false);
    assert_eq!(reply.unwrap(), ActorMessageReply::Done(0));
    assert_eq!(held(arena), (0x12, 2));

    arena.owned_mut(factory).unwrap().state.base.flags &= !0x8000_0000;
    let dirty = arena.object(factory).unwrap().base.dirty_word;
    let reply = arena.receive_actor_message(factory, smoke, context, false);
    assert_eq!(
        reply.unwrap(),
        ActorMessageReply::PlayerText {
            player,
            string: 0x30c
        }
    );
    assert_eq!(held(arena), (0, 0));
    assert_eq!(
        arena.object(factory).unwrap().base.dirty_word,
        dirty | 0x202
    );

    arena.actor_mut(factory).unwrap().word_b4 = 0x12;
    let reply = arena.receive_actor_message(factory, smoke, context, false);
    assert_eq!(reply.unwrap(), ActorMessageReply::Done(0));
    assert_eq!(held(arena), (0, 0));

    level.place(TRUCK_DOCK, [0; 3]);
    let mut host = fixture.host(level.null());
    level
        .world
        .arena
        .tick_selector15(factory, 125_000, &mut host)
        .unwrap();
    let arena = &level.world.arena;
    assert_eq!(
        arena.selector15(factory).unwrap().handle_188,
        arena.actor(level.vehicle).unwrap().handle
    );
}

fn docked<'a>(fixture: &'a Fixture, level: &mut Level1) -> Host<'a> {
    level.place(TRUCK_DOCK, [0; 3]);
    let mut host = fixture.host(level.null());
    level
        .world
        .arena
        .tick_selector15(level.factory, 125_000, &mut host)
        .unwrap();
    host.events.clear();
    host
}

/// While docked the vehicle is pulled toward the dock point: each axis
/// moves by `(delta << 12) * offset >> 31`, at most 16 toward the positive
/// side. The camera follows the factory in mode 1 at (300, 500, -1500).
#[test]
fn the_dock_pulls_the_vehicle_and_turns_the_camera() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let [x, y, z] = TRUCK_DOCK;
    level.place([x - 100, y + 100, z - 10], [0; 3]);
    let (factory, vehicle) = (level.factory, level.vehicle);
    let arena = &mut level.world.arena;
    arena.tick_selector15(factory, 125_000, &mut host).unwrap();
    // +100: 23.8 caps at 16; -100: -24 (floored); +10: 2.
    assert_eq!(
        arena.object(vehicle).unwrap().base.position_words,
        Some([x - 100 + 16, y + 100 - 24, z - 10 + 2])
    );
    let camera = host.camera(arena).unwrap();
    let state = arena.selector16(camera).unwrap();
    assert_eq!(state.mode_188, 1);
    assert_eq!(state.offset_194, Some([300, 500, -1500]));
    let target = arena
        .actor_follow_target(state.target_18c.unwrap())
        .unwrap();
    assert_eq!(target.handle_04, arena.actor(factory).unwrap().handle);
}

/// Infest01's first dock (factory `0384F080`, 125 ms ticks): each docked
/// tick moves the vehicle from its position to the one `0047DDF0` wrote
/// (the `dockpull` query, distinct rows).
#[test]
fn the_pull_matches_infest01() {
    const PULLS: [([u16; 3], [u16; 3]); 13] = [
        ([0x9080, 0x90, 0x8400], [0x9090, 0x80, 0x840a]),
        ([0x9096, 0x82, 0x840b], [0x90a6, 0x76, 0x8412]),
        ([0x90a6, 0x7b, 0x8412], [0x90b3, 0x70, 0x8417]),
        ([0x90b3, 0x7a, 0x8417], [0x90bd, 0x6f, 0x841b]),
        ([0x90bd, 0x7b, 0x841b], [0x90c4, 0x70, 0x841e]),
        ([0x90c4, 0x7b, 0x841e], [0x90c9, 0x70, 0x8421]),
        ([0x90c9, 0x7b, 0x8421], [0x90cd, 0x70, 0x8423]),
        ([0x90cd, 0x7c, 0x8423], [0x90d0, 0x71, 0x8424]),
        ([0x90d0, 0x7c, 0x8424], [0x90d3, 0x71, 0x8425]),
        ([0x90d3, 0x7c, 0x8425], [0x90d5, 0x71, 0x8426]),
        ([0x90d5, 0x7c, 0x8426], [0x90d6, 0x71, 0x8427]),
        ([0x90d6, 0x7c, 0x8427], [0x90d7, 0x71, 0x8427]),
        ([0x90d9, 0x7c, 0x8427], [0x90d9, 0x71, 0x8427]),
    ];
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    for (before, after) in PULLS {
        level.place(before, [0; 3]);
        level
            .world
            .arena
            .tick_selector15(level.factory, 125_000, &mut host)
            .unwrap();
        let position = level
            .world
            .arena
            .object(level.vehicle)
            .unwrap()
            .base
            .position_words;
        assert_eq!(position, Some(after), "from {before:x?}");
    }
}

/// Beyond half the factory's radius (641² / 4) the shop is hidden
/// (004133C0): the shown byte clears and the controller fades out.
#[test]
fn a_straying_vehicle_hides_the_shop() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let [x, y, z] = TRUCK_DOCK;
    level.place([x + 321, y, z], [0; 3]);
    let arena = &mut level.world.arena;
    arena
        .tick_selector15(level.factory, 125_000, &mut host)
        .unwrap();
    assert_eq!(arena.player_message(level.player).unwrap().shown_00(), 0);
    assert!(!arena
        .player_shown_menu(level.player)
        .unwrap()
        .stack
        .active());
    assert_eq!(
        arena.object(level.vehicle).unwrap().base.position_words,
        Some([x + 321, y, z])
    );
}

/// `0042F2A0`: the camera follows the vehicle again, the dock and the
/// player's +DC clear and Facilities returns to the Options.
#[test]
fn undocking_returns_the_camera() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, vehicle, player) = (level.factory, level.vehicle, level.player);
    let null = level.null();
    let arena = &mut level.world.arena;
    arena.selector15_mut(factory).unwrap().word_160 = 1;
    arena.selector15_mut(factory).unwrap().word_1b8 = 0;
    arena.undock_factory(factory, &mut host).unwrap();
    let state = arena.selector15(factory).unwrap();
    assert_eq!(
        (state.handle_188, state.byte_1bd, state.byte_1be),
        (null, 0, 0)
    );
    assert_eq!(state.word_160, 0);
    assert_eq!(arena.world_player(player).unwrap().handle_dc, null);
    let camera = host.camera(arena).unwrap();
    let state = arena.selector16(camera).unwrap();
    let target = arena
        .actor_follow_target(state.target_18c.unwrap())
        .unwrap();
    assert_eq!(target.handle_04, arena.actor(vehicle).unwrap().handle);
    // First Person off: view mode 0, so the camera's +188 is 3.
    assert_eq!(state.mode_188, 3);
}

/// Leaving the world while docked: 0048E890's player prefix (`0048E8FA`)
/// deletes the shown shop menu, whose destructor (`004315E0`, `0042F2A0`)
/// undocks the factory before the manager drain (`0048E901`) takes it.
/// Left queued past the drain, it reached the next world's first drain
/// against the old factory, expired, and stopped every menu there: the
/// user's 2026-10-21 12:21 crash log, docked on Artoria after a reload.
#[test]
fn leaving_the_world_docked_undocks_before_the_drain() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    let null = level.null();
    let arena = &mut level.world.arena;
    let factory_handle = arena.actor(factory).unwrap().handle;
    assert_eq!(
        arena.world_player(player).unwrap().handle_dc,
        factory_handle
    );
    arena.retire_world_player().unwrap();
    assert!(arena.player_shown_menu(player).is_none());
    let destroyed = arena.take_destroyed_menus();
    assert_eq!(destroyed, [(player, ShownMenuOwner::Shop { factory })]);
    arena.destroy_shop_menu(factory, &mut host).unwrap();
    let state = arena.selector15(factory).unwrap();
    assert_eq!((state.word_1b8, state.handle_188), (0, null));
    assert_eq!(arena.world_player(player).unwrap().handle_dc, null);
}

/// `00431CE0`: a bound menu is rebuilt when the state changes, except
/// between 4 and 5; force always rebuilds. A kept menu keeps its row ages.
#[test]
fn the_state_store_rebuilds_on_shape_changes() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    let arena = &mut level.world.arena;
    let mark = |arena: &mut ObjectArena| {
        let menu = arena.player_shown_menu_mut(player).unwrap();
        menu.stack.current().unwrap().data().records_mut().items[0].age = 77;
    };
    let kept = |arena: &ObjectArena| {
        let menu = arena.player_shown_menu(player).unwrap();
        menu.stack.current().unwrap().data().records().items[0].age == 77
    };
    let mut rebuilt = Vec::new();
    for (state, force) in [
        (0, false),
        (3, false),
        (4, false),
        (5, false),
        (4, false),
        (6, false),
        (6, true),
    ] {
        mark(arena);
        arena
            .set_factory_state(factory, state, force, &mut host)
            .unwrap();
        rebuilt.push(!kept(arena));
    }
    assert_eq!(rebuilt, [false, true, true, false, false, true, true]);
    assert_eq!(
        shop_title(arena, player),
        Some((ShownMenuOwner::Shop { factory }, 0x453a80, 0x302))
    );
    // Without a bound menu only the state changes.
    arena.selector15_mut(factory).unwrap().word_1b8 = 0;
    mark(arena);
    arena
        .set_factory_state(factory, 1, false, &mut host)
        .unwrap();
    assert!(kept(arena));
    assert_eq!(arena.selector15(factory).unwrap().word_160, 1);
}

/// Infest01's truck, a zero-second build: state 3 passes to 4 at once,
/// the fitting charges for 2 s, the grant phase counts 2 s and grants the
/// truck (sound 0x30, it is new), then the recovery starts and, with
/// nothing left to list, hides the shop; 2 s later the state returns to 0.
#[test]
fn production_runs_the_truck() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let truck = *host.tables.product(0, 0x13).unwrap();
    let (factory, player) = (level.factory, level.player);
    let arena = &mut level.world.arena;
    arena
        .start_factory_production(factory, &truck, false)
        .unwrap();
    arena
        .set_factory_state(factory, 3, false, &mut host)
        .unwrap();
    let state = arena.selector15(factory).unwrap();
    assert_eq!(
        (
            state.word_158,
            state.word_184,
            state.product_18c,
            state.amount_18e
        ),
        (Some(0), 9, Some(2), Some(0))
    );
    let mut states = Vec::new();
    let mut titles = Vec::new();
    for _ in 0..60 {
        arena.tick_selector15(factory, 125_000, &mut host).unwrap();
        states.push(arena.selector15(factory).unwrap().word_160);
        let title = shop_title(arena, player).unwrap();
        if titles.last() != Some(&title) {
            titles.push(title);
        }
    }
    // 1 tick of build, 16 of fitting, 17 of grant (the 17th exceeds 2 s),
    // the recovery's first tick, then 16 more.
    let changes: Vec<_> = states
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[0] != pair[1])
        .map(|(tick, pair)| (tick + 1, pair[1]))
        .collect();
    assert_eq!(states[0], 4);
    assert_eq!(changes, [(16, 5), (34, 6), (50, 0)]);
    let shop = ShownMenuOwner::Shop { factory };
    assert_eq!(
        titles,
        [
            (shop, 0x430d70, 0x2f6),
            (shop, 0x453a80, 0x302),
            (shop, 0x453a80, 0x308)
        ]
    );
    let events: Vec<_> = host
        .events
        .iter()
        .filter(|event| {
            !event.starts_with("volume") && !event.starts_with("loop") && *event != "release"
        })
        .cloned()
        .collect();
    let position = arena.object(factory).unwrap().base.position_words.unwrap();
    assert_eq!(
        events,
        [
            format!("sound 30 {position:?} 10000 10000"),
            "grant 2 0".into()
        ]
    );
    // Nothing is left to list, so the recovery hid the shop; the Options
    // replaced its menu on the fading controller.
    assert_eq!(arena.player_message(player).unwrap().shown_00(), 0);
    let words = arena.selector15(factory).unwrap().words_178_182.unwrap();
    assert_eq!((words[3], words[4]), (0, 0));
}

/// The busy hum (0x1C) plays in production, the idle one (0x4B) otherwise,
/// at a volume falling to zero at 3000 units from the player's unit.
#[test]
fn the_factory_hums_near_the_player() {
    let (fixture, mut level) = level1();
    let mut host = fixture.host(level.null());
    let factory = level.factory;
    let position = level
        .world
        .arena
        .object(factory)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let [x, y, z] = position;
    level.place([x + 1000, y, z], [0; 3]);
    let arena = &mut level.world.arena;
    arena.selector15_mut(factory).unwrap().word_1c4 = 10_000_000;
    arena.tick_selector15(factory, 125_000, &mut host).unwrap();
    assert_eq!(
        host.events,
        ["loop 4b", &format!("volume {:x}", (2000 * 0xc000) / 3000)]
    );
    host.events.clear();
    arena.selector15_mut(factory).unwrap().word_160 = 6;
    arena.selector15_mut(factory).unwrap().words_168_174[0] = 10_000_000;
    arena.tick_selector15(factory, 125_000, &mut host).unwrap();
    assert_eq!(
        host.events,
        [
            "release",
            "loop 1c",
            &format!("volume {:x}", (2000 * 0xc000) / 3000)
        ]
    );
    host.events.clear();
    level.place([x + 3000, y, z], [0; 3]);
    let arena = &mut level.world.arena;
    arena.tick_selector15(factory, 125_000, &mut host).unwrap();
    assert_eq!(host.events, ["release"]);
    assert_eq!(
        arena.selector15(factory).unwrap().intermediate.sound_154,
        None
    );
}

fn command(level: &mut Level1, host: &mut Host<'_>, code: u32) {
    level
        .world
        .arena
        .run_shown_menu_command(
            level.player,
            code,
            crate::menu::MenuVisibility::OwnerFlagSet,
            host,
        )
        .unwrap();
}

fn selection(level: &Level1) -> i32 {
    let menu = level.world.arena.player_shown_menu(level.player).unwrap();
    menu.stack.current().unwrap().selection()
}

/// `00431660`: the selection moves with sound 2 at the factory; Activate
/// on Facilities (`0042F630`, row 2) shows the products, and Back
/// (`0042F620`) returns to the Options.
#[test]
fn the_options_open_facilities_and_back() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let factory = level.factory;
    let position = level
        .world
        .arena
        .object(factory)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!(selection(&level), 2);
    command(&mut level, &mut host, 1);
    assert_eq!(selection(&level), 3);
    command(&mut level, &mut host, 0);
    assert_eq!(selection(&level), 2);
    let sounds: Vec<_> = host
        .events
        .iter()
        .filter(|event| event.starts_with("sound"))
        .cloned()
        .collect();
    let expected = format!("sound 2 {position:?} 10000 10000");
    assert_eq!(sounds, [expected.clone(), expected]);
    command(&mut level, &mut host, 4);
    let arena = &level.world.arena;
    assert_eq!(arena.selector15(factory).unwrap().word_160, 1);
    let menu = arena.player_shown_menu(level.player).unwrap();
    let records = menu.stack.current().unwrap().data().records();
    // The truck is listed (index 0x13), then Back.
    assert_eq!(records.items.len(), 4);
    assert_eq!(records.items[2].action_arguments[0], 0x13);
    drop(records);
    // Next to Back, then Activate.
    command(&mut level, &mut host, 1);
    assert_eq!(selection(&level), 3);
    command(&mut level, &mut host, 4);
    let arena = &level.world.arena;
    assert_eq!(arena.selector15(factory).unwrap().word_160, 0);
    assert_eq!(
        shop_title(arena, level.player),
        Some((ShownMenuOwner::Shop { factory }, 0x453a80, 0x308))
    );
}

/// `00431740` for the truck (no costs): sound 5, production from state 3,
/// then sound 0x17 at a quarter pitch.
#[test]
fn buying_the_truck_starts_production() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let factory = level.factory;
    let position = level
        .world
        .arena
        .object(factory)
        .unwrap()
        .base
        .position_words
        .unwrap();
    command(&mut level, &mut host, 4);
    host.events.clear();
    command(&mut level, &mut host, 4);
    let state = level.world.arena.selector15(factory).unwrap();
    assert_eq!(
        (state.word_160, state.product_18c, state.word_184),
        (3, Some(2), 9)
    );
    assert_eq!(
        host.events,
        [
            format!("sound 5 {position:?} 10000 10000"),
            format!("sound 17 {position:?} 10000 4000"),
        ]
    );
    assert_eq!(
        shop_title(&level.world.arena, level.player),
        Some((ShownMenuOwner::Shop { factory }, 0x430e30, 0x2f6))
    );
}

/// A product the player cannot pay for (turbo, five of the second token,
/// listed once the factory knows ID 3): sound 4, the "not enough
/// resources" text and command 7; the state stays.
#[test]
fn an_unaffordable_product_is_refused() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let factory = level.factory;
    level
        .world
        .arena
        .selector15_mut(factory)
        .unwrap()
        .grants_198
        .mask_04[0] |= 8;
    command(&mut level, &mut host, 4);
    host.events.clear();
    // Turbo (index 0x12) comes first, disabled.
    let menu = level.world.arena.player_shown_menu(level.player).unwrap();
    let row = menu.stack.current().unwrap().data().records().items[2];
    assert_eq!((row.flags, row.action_arguments[0]), (0x41, 0x12));
    command(&mut level, &mut host, 4);
    let position = level
        .world
        .arena
        .object(factory)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!(
        host.events,
        [
            format!("sound 4 {position:?} 10000 10000"),
            "text 303".into(),
            "command 7 0".into(),
        ]
    );
    assert_eq!(level.world.arena.selector15(factory).unwrap().word_160, 1);
}

/// A factory short of scientists refuses before anything else: sound 4
/// and "Insufficient Staff".
#[test]
fn a_short_staffed_factory_refuses() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let factory = level.factory;
    command(&mut level, &mut host, 4);
    level
        .world
        .arena
        .selector15_mut(factory)
        .unwrap()
        .intermediate
        .word_140 = 1;
    host.events.clear();
    command(&mut level, &mut host, 4);
    let position = level
        .world
        .arena
        .object(factory)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!(
        host.events,
        [
            format!("sound 4 {position:?} 10000 10000"),
            "text 300".into()
        ]
    );
}

/// Energy Distribution (`0042F630` with 2 sets the first player's command
/// 0x27): AdjustNext raises a level for one crystal, AdjustPrevious sets
/// command 0x28.
#[test]
fn energy_distribution_raises_levels_for_crystals() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    level
        .world
        .arena
        .world_player_mut(player)
        .unwrap()
        .dword_118 = 2;
    command(&mut level, &mut host, 1);
    command(&mut level, &mut host, 4);
    assert_eq!(level.world.arena.selector15(factory).unwrap().word_160, 2);
    assert!(host.events.contains(&"command 27 0".to_string()));
    host.events.clear();
    // Row 2 is level 0 (upgrade 0x32, owned since the spawn).
    assert_eq!(selection(&level), 2);
    for _ in 0..3 {
        command(&mut level, &mut host, 3);
    }
    command(&mut level, &mut host, 2);
    let owner = level.world.arena.world_player(player).unwrap();
    assert_eq!((owner.dword_118, owner.bytes_13c_143[0]), (0, 2));
    assert!(host.events.contains(&"command 28 0".to_string()));
}

/// Disconnect (`0042F620` outside Facilities and Energy Distribution)
/// fades the controller out; the message stays shown until it has faded,
/// when the update destroys it and the menu's destructor undocks.
#[test]
fn disconnect_fades_out_and_undocks() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    let assets = &fixture.assets;
    level.world.arena.set_player_message_environment(
        crate::object_runtime::PlayerMessageEnvironment {
            menus: crate::object_runtime::PlayerMessageMenus::parse(assets).unwrap(),
            math: crate::menu_basis::MenuBasisMath::new(assets.sine_table().values),
            network: 0,
            visibility: crate::menu::MenuVisibility::OwnerFlagSet,
            pause_on_hints: false,
            attract: false,
            ui_state: 4,
            cheats: Default::default(),
        },
    );
    for _ in 0..3 {
        command(&mut level, &mut host, 1);
    }
    assert_eq!(selection(&level), 5);
    command(&mut level, &mut host, 4);
    let null = level.null();
    let arena = &mut level.world.arena;
    assert!(!arena.player_shown_menu(player).unwrap().stack.active());
    assert_eq!(arena.player_message(player).unwrap().shown_00(), 1);
    let mut ticks = 0;
    while arena.player_shown_menu(player).is_some() {
        arena.update_player_message(player, 125_000, false).unwrap();
        ticks += 1;
        assert!(ticks < 10);
    }
    assert_eq!(arena.player_message(player).unwrap().shown_00(), 0);
    let destroyed = arena.take_destroyed_menus();
    assert_eq!(destroyed, [(player, ShownMenuOwner::Shop { factory })]);
    arena.destroy_shop_menu(factory, &mut host).unwrap();
    let state = arena.selector15(factory).unwrap();
    assert_eq!((state.word_1b8, state.handle_188), (0, null));
}

fn message_environment(assets: &FrontendAssets) -> crate::object_runtime::PlayerMessageEnvironment {
    crate::object_runtime::PlayerMessageEnvironment {
        menus: crate::object_runtime::PlayerMessageMenus::parse(assets).unwrap(),
        math: crate::menu_basis::MenuBasisMath::new(assets.sine_table().values),
        network: 0,
        visibility: crate::menu::MenuVisibility::OwnerFlagSet,
        pause_on_hints: false,
        attract: false,
        ui_state: 4,
        cheats: Default::default(),
    }
}

fn key(level: &mut Level1, address: u32) {
    let player = level.player;
    level
        .world
        .arena
        .player_key_callback(
            player,
            crate::object_runtime::PlayerKeyEvent {
                address,
                value: 1,
                timestamp_micros: 0,
            },
            crate::object_runtime::PlayerKeyInputs {
                network: 0,
                double_tap_window: 0,
                cheat_word_84: 0,
            },
        )
        .unwrap();
}

/// Tab (`00469450`) and Escape (`00469480`).
const TAB: u32 = 0x0046_9450;
const ESCAPE: u32 = 0x0046_9480;

/// The broadcasts the keys queued, run as the gameplay router runs them.
fn broadcasts(level: &mut Level1, host: &mut Host<'_>) -> Vec<u32> {
    let arena = &mut level.world.arena;
    arena
        .take_unit_broadcasts()
        .into_iter()
        .map(|broadcast| arena.run_unit_broadcast(broadcast, host).unwrap().sum)
        .collect()
}

/// The message's controller fades in until it is opaque (0xFFFF).
fn fade_in(level: &mut Level1) {
    for _ in 0..20 {
        level
            .world
            .arena
            .update_player_message(level.player, 16_384, false)
            .unwrap();
    }
}

/// The shown menus' owners, oldest first.
fn owners(level: &Level1) -> Vec<ShownMenuOwner> {
    level
        .world
        .arena
        .player_shown_menu(level.player)
        .map(|menu| menu.owners().to_vec())
        .unwrap_or_default()
}

/// Tab hides the shown shop (`00413410`: `+0` clears and the controller
/// fades out at `+20 = 0x20` per 128 µs of the delta, so 0xFFFF lasts about
/// 262 ms). When the fade ends the controller is destroyed with its menus,
/// and the shop's destructor (`004315E0`) undocks the factory (`0042F2A0`),
/// clearing `+1BD`: on the pad the factory stays undocked. A later Tab
/// opens the computer, and its `004137D0` broadcast re-arms the factory
/// (code 6, `0042F472`), which answers 1 (`+34` set). The factory's next
/// update docks the vehicle again and shows its shop over the computer.
#[test]
fn a_slow_second_tab_brings_the_shop_back() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    level
        .world
        .arena
        .set_player_message_environment(message_environment(&fixture.assets));
    fade_in(&mut level);
    key(&mut level, TAB);
    assert!(broadcasts(&mut level, &mut host).is_empty());
    let arena = &mut level.world.arena;
    assert_eq!(arena.player_message(player).unwrap().shown_00(), 0);
    let mut ticks = 0;
    while arena.player_shown_menu(player).is_some() {
        arena.update_player_message(player, 16_384, false).unwrap();
        ticks += 1;
    }
    // 0xFFFF in steps of (16384 / 128) * 0x20.
    assert_eq!(ticks, 16);
    let destroyed = arena.take_destroyed_menus();
    assert_eq!(destroyed, [(player, ShownMenuOwner::Shop { factory })]);
    arena.destroy_shop_menu(factory, &mut host).unwrap();
    let null = level.null();
    let state = level.world.arena.selector15(factory).unwrap();
    assert_eq!((state.handle_188, state.byte_1bd), (null, 0));
    // Disarmed, the factory leaves the vehicle on its pad.
    level
        .world
        .arena
        .tick_selector15(factory, 16_384, &mut host)
        .unwrap();
    assert_eq!(
        level.world.arena.selector15(factory).unwrap().handle_188,
        null
    );

    key(&mut level, TAB);
    assert_eq!(owners(&level), [ShownMenuOwner::Plain]);
    assert_eq!(broadcasts(&mut level, &mut host), [1]);
    let arena = &level.world.arena;
    assert_eq!(arena.selector15(factory).unwrap().byte_1bd, 1);
    assert_eq!(arena.player_message(player).unwrap().byte_34(), 1);
    level
        .world
        .arena
        .tick_selector15(factory, 16_384, &mut host)
        .unwrap();
    let arena = &level.world.arena;
    let vehicle = arena.actor(level.vehicle).unwrap().handle;
    assert_eq!(arena.selector15(factory).unwrap().handle_188, vehicle);
    assert_eq!(
        owners(&level),
        [ShownMenuOwner::Plain, ShownMenuOwner::Shop { factory }]
    );
    assert_eq!(
        shop_title(&level.world.arena, player),
        Some((ShownMenuOwner::Shop { factory }, 0x453a80, 0x308))
    );
}

/// A second Tab while the hidden shop still fades: the controller lives,
/// so `00413230` reactivates it (`+8`) and pushes the computer over the
/// shop (`00453440`). Nothing is destroyed and the vehicle stays docked.
#[test]
fn a_quick_second_tab_opens_the_computer_over_the_shop() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    level
        .world
        .arena
        .set_player_message_environment(message_environment(&fixture.assets));
    fade_in(&mut level);
    key(&mut level, TAB);
    for _ in 0..8 {
        level
            .world
            .arena
            .update_player_message(player, 16_384, false)
            .unwrap();
    }
    assert!(!level
        .world
        .arena
        .player_shown_menu(player)
        .unwrap()
        .stack
        .active());
    key(&mut level, TAB);
    assert_eq!(broadcasts(&mut level, &mut host), [1]);
    let arena = &mut level.world.arena;
    assert_eq!(arena.player_message(player).unwrap().shown_00(), 1);
    assert!(arena.player_shown_menu(player).unwrap().stack.active());
    for _ in 0..32 {
        arena.update_player_message(player, 16_384, false).unwrap();
    }
    assert!(arena.take_destroyed_menus().is_empty());
    assert_eq!(
        owners(&level),
        [ShownMenuOwner::Shop { factory }, ShownMenuOwner::Plain]
    );
    let vehicle = level.world.arena.actor(level.vehicle).unwrap().handle;
    assert_eq!(
        level.world.arena.selector15(factory).unwrap().handle_188,
        vehicle
    );
}

/// Escape's code 0x18 (`0042F3A7..0042F3D7`): Facilities returns to the
/// Options (`00431CE0(0, 0)` rebuilds the shop); in the Options the first
/// player's message hides (`004133C0`). Both answer 1.
#[test]
fn escape_backs_out_of_the_shop() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let (factory, player) = (level.factory, level.player);
    level
        .world
        .arena
        .set_player_message_environment(message_environment(&fixture.assets));
    // Activate on Facilities.
    command(&mut level, &mut host, 4);
    assert_eq!(level.world.arena.selector15(factory).unwrap().word_160, 1);
    key(&mut level, ESCAPE);
    assert_eq!(broadcasts(&mut level, &mut host), [1]);
    let arena = &level.world.arena;
    assert_eq!(arena.selector15(factory).unwrap().word_160, 0);
    assert_eq!(arena.player_message(player).unwrap().shown_00(), 1);
    assert_eq!(
        shop_title(arena, player),
        Some((ShownMenuOwner::Shop { factory }, 0x453a80, 0x308))
    );
    key(&mut level, ESCAPE);
    assert_eq!(broadcasts(&mut level, &mut host), [1]);
    assert_eq!(
        level.world.arena.player_message(player).unwrap().shown_00(),
        0
    );
}

/// `00413950` under the shown shop: a cursor key queues its code for the
/// menu and the interface cue (2 for Next), and the queued command moves
/// the selection.
#[test]
fn shown_message_keys_queue_menu_commands_and_cues() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let player = level.player;
    let arena = &mut level.world.arena;
    arena.set_player_message_environment(message_environment(&fixture.assets));
    arena.world_player_mut(player).unwrap().dword_50 = Some(0);
    // 004697C0, the Next key's press.
    arena
        .player_key_callback(
            player,
            crate::object_runtime::PlayerKeyEvent {
                address: 0x0046_97c0,
                value: 1,
                timestamp_micros: 0,
            },
            crate::object_runtime::PlayerKeyInputs {
                network: 0,
                double_tap_window: 0,
                cheat_word_84: 0,
            },
        )
        .unwrap();
    assert_eq!(arena.take_shown_menu_commands(), [(player, 1)]);
    assert_eq!(arena.take_ui_cues(), [2]);
    arena
        .run_shown_menu_command(
            player,
            1,
            crate::menu::MenuVisibility::OwnerFlagSet,
            &mut host,
        )
        .unwrap();
    assert_eq!(selection(&level), 3);
}

/// `0046A260` under a shown message: the controls brake. In buggy or truck
/// mode the thrust is -100 times the velocity along the basis row
/// +60..+68; in another mode it is zero. Turn, pitch, Fire and Grenade are
/// zero and +3E is set.
#[test]
fn a_shown_message_brakes_the_vehicle() {
    let (fixture, mut level) = level1();
    let _host = docked(&fixture, &mut level);
    let (player, vehicle) = (level.player, level.vehicle);
    let arena = &mut level.world.arena;
    arena.world_player_mut(player).unwrap().dword_50 = Some(0);
    let mut input = crate::input::InputRuntime::new(0);
    let analog = [0x0054_ecf8, 0x0054_ece8, 0x0054_ed08]
        .map(|identity| crate::object_runtime::player_analog_fixture(&mut input, identity));
    let mut acquire = |identity| {
        input
            .acquire_consumer(
                crate::input::ActionDescriptor {
                    identity,
                    callback_address: None,
                    flags: 6,
                },
                0x77,
            )
            .unwrap()
    };
    let consumers = crate::object_runtime::PlayerInputConsumers {
        analog_5c: analog[0],
        analog_60: analog[1],
        analog_64: analog[2],
        left_68: acquire(0x0053_a368),
        right_6c: acquire(0x0053_a370),
        dive_70: acquire(0x0053_a358),
        climb_74: acquire(0x0053_a360),
        thrust_78: acquire(0x0053_a378),
        reverse_7c: acquire(0x0053_a380),
        fire_80: acquire(0x0053_a388),
        grenade_84: acquire(0x0053_a390),
    };
    let mut words = arena.actor(vehicle).unwrap().rotating.basis.words;
    words[7] = 0.5f32.to_bits();
    words[8] = 0.0f32.to_bits();
    words[9] = 0.75f32.to_bits();
    arena.actor_mut(vehicle).unwrap().rotating.basis.words = words;
    arena.actor_mut(vehicle).unwrap().words_8e_92 = [100, 7, 0xffd8];
    let mut update = |arena: &mut ObjectArena, mode: u32| {
        arena.selector2_actor_mut(vehicle).unwrap().dwords_1e8_1f4[0] = mode;
        arena
            .update_player_controls(
                player,
                crate::object_runtime::PlayerUpdateInputs {
                    delta_micros: 125_000,
                    slow_input_5c5: false,
                    input_mode_08: 0,
                    network: 0,
                },
                &mut input,
                &consumers,
                Some((2, 3)),
                Some(vehicle),
                None,
            )
            .unwrap();
        arena.world_player(player).unwrap().controls
    };
    // (-40 * 0.75) + 100 * 0.5 = 20, so the thrust is -2000.
    for (mode, thrust) in [(1, -2000), (2, -2000), (3, 0)] {
        let controls = update(arena, mode);
        assert_eq!(controls.thrust_1c, thrust, "mode {mode}");
        assert_eq!(
            (
                controls.turn_14,
                controls.pitch_18,
                controls.fire_20,
                controls.grenade_24
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(controls.byte(0x3e), 1);
    }
}

/// The shop's draws on Level 1's resources: Infest01's colours for the
/// styles the rows read, the vehicle model's extent 109 (its preview at
/// 1154) and the texts; then what the draw reads from the docked world.
#[test]
fn the_shop_draws_with_level_1s_resources() {
    use crate::menu_text::MenuTextResources;
    use crate::render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    };
    let (fixture, mut level) = level1();
    let host = docked(&fixture, &mut level);
    let resources = RenderResources::new(&fixture.resident);
    for (slot, rgb) in [
        (0, 0x486c21),
        (1, 0x304816),
        (5, 0x3d5c1c),
        (7, 0),
        (8, 0x121212),
        (0x22, 0x602000),
        (0x23, 0x003800),
        (0x24, 0x08084e),
        (0x27, 0xffb45a),
        (0x28, 0x4040ff),
        (0x29, 0x40ff40),
    ] {
        assert_eq!(
            resources.style(slot).unwrap() & 0xff_ffff,
            rgb,
            "style {slot:#x}"
        );
    }
    let model = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 0x75,
        })
        .unwrap();
    let RenderResourceData::Model(entry) = model.data else {
        panic!("model 0x75");
    };
    assert_eq!(entry.header.visibility_extent, 109);
    for (id, text) in [
        (0x14c, "Cannons"),
        (0x17c, "Truck mode"),
        (0x2fd, "Manufacturing %s"),
        (0x300, "Insufficient Staff"),
        (0x301, "Fitting %s... Please wait"),
        (0x309, "Spare Tokens"),
        (0x30b, "Duration: %d secs "),
        (0x30d, "Fully Stocked"),
        (0x30e, "Insufficient Crystals"),
    ] {
        let bytes = resources.string(id).unwrap();
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        assert_eq!(&bytes[..end], text.as_bytes(), "text {id:#x}");
    }
    let state = level
        .world
        .arena
        .shop_draw_state::<String>(level.factory, &host.process, 0x46e)
        .unwrap();
    let owner = level.world.arena.world_player(level.player).unwrap();
    let player = state.player.as_ref().unwrap();
    assert_eq!(player.id, level.player);
    assert_eq!(player.crystals_118, owner.dword_118 as i32);
    assert_eq!(player.levels_13c, owner.bytes_13c_143);
    // The vehicle's weapon stocks feed 00431950.
    assert!(player.weapon_entries.is_some());
    let factory = level.world.arena.selector15(level.factory).unwrap();
    assert_eq!(
        (state.factory.staff_13c, state.factory.staff_140),
        (factory.intermediate.word_13c, factory.intermediate.word_140)
    );
    assert_eq!(state.factory.product_18c, None);
    assert_eq!(state.clock, 0x46e);
}

/// Infest01's Evacuate (the `staff` and `evacuate` queries): the factory at
/// (47223, 16, 26207), turned half a turn about Y, held 2 of 2 scientists
/// and spawned each at its +146 offset (0, 0, -1000) turned to
/// (47223, 16, 27207). Each left the staff, took the vehicle's team 2 and
/// position (47219, 53, 26734), and boarded. The emptied factory turned
/// team 0 with +38 0x202.
#[test]
fn evacuate_sends_infest01s_scientists_into_the_hold() {
    let (fixture, mut level) = level1();
    let null = level.null();
    let arena = &mut level.world.arena;
    let factory = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena.selector15(actor).is_ok()
                && arena.object(actor).unwrap().base.position_words == Some([47223, 16, 26207])
        })
        .expect("Infest01's evacuated factory");
    let state = &arena.selector15(factory).unwrap().intermediate;
    assert_eq!((state.word_140, state.word_13c), (2, 2));
    assert_eq!(state.words_146_14a, Some([0, 0, 64536]));
    assert_eq!(
        arena.actor(factory).unwrap().rotating.basis.words[1..],
        [0xbf80_0000, 0, 0, 0, 0x3f80_0000, 0, 0, 0, 0xbf80_0000]
    );
    let vehicle_position = [47219, 53, 26734];
    arena
        .owned_mut(level.vehicle)
        .unwrap()
        .state
        .base
        .position_words = Some(vehicle_position);
    let evacuees: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector12_actor(actor).is_ok())
        .take(2)
        .collect();
    let mut host = fixture.host(null);
    host.evacuees = evacuees.clone();
    assert_eq!(
        arena
            .evacuate_factory(factory, 2, Some(level.vehicle), &mut host)
            .unwrap(),
        2
    );
    let carry = format!("carry at {vehicle_position:?} team 2 +38 2");
    let creates = host
        .events
        .iter()
        .filter(|event| *event == "create [47223, 16, 27207]")
        .count();
    let carries = host.events.iter().filter(|event| **event == carry).count();
    assert_eq!((creates, carries), (2, 2), "{:#?}", host.events);
    assert!(!host.events.iter().any(|event| event.starts_with("flash")));

    let state = &arena.selector15(factory).unwrap().intermediate;
    assert_eq!(state.word_140, 0);
    assert_eq!(arena.actor(factory).unwrap().word_b4, 0);
    assert_eq!(
        arena.object(factory).unwrap().base.dirty_word & 0x202,
        0x202
    );
    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    let handles: Vec<ActorHandle> = evacuees
        .iter()
        .map(|&actor| arena.actor(actor).unwrap().handle)
        .collect();
    assert_eq!(slots.handles[..2], handles[..]);
    assert_eq!(slots.states, [Some(2), Some(2), None]);
}

/// Without a vehicle the evacuees stay where they spawn, on the factory's
/// team, each under the slot flash; the factory still empties.
#[test]
fn evacuees_without_a_vehicle_stay_by_the_factory() {
    let (fixture, mut level) = level1();
    let null = level.null();
    let arena = &mut level.world.arena;
    let factory = level.factory;
    {
        let state = &mut arena.selector15_mut(factory).unwrap().intermediate;
        state.word_140 = 1;
        state.word_13c = 2;
        state.words_146_14a = Some([0, 0, 0]);
    }
    arena.actor_mut(factory).unwrap().word_b4 = 3;
    let origin = arena.object(factory).unwrap().base.position_words.unwrap();
    let evacuee = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector12_actor(actor).is_ok())
        .unwrap();
    let mut host = fixture.host(null);
    host.evacuees = vec![evacuee];
    assert_eq!(
        arena.evacuate_factory(factory, 1, None, &mut host).unwrap(),
        1
    );
    assert_eq!(
        host.events,
        [format!("create {origin:?}"), format!("flash {origin:?}")]
    );
    assert_eq!(arena.actor(evacuee).unwrap().word_b4, 3);
    assert_eq!(arena.object(evacuee).unwrap().base.dirty_word & 2, 2);
    assert_eq!(arena.actor(factory).unwrap().word_b4, 0);
}

/// `0042F780` from the docked shop: Activate on Evacuate factory sends the
/// staff into the vehicle's hold (`0040DCB0`), then the Options back out
/// (`00431710`), which fades the controller's menu.
#[test]
fn evacuate_from_the_shop_fills_the_hold() {
    let (fixture, mut level) = level1();
    let factory = level.factory;
    {
        let state = &mut level
            .world
            .arena
            .selector15_mut(factory)
            .unwrap()
            .intermediate;
        state.word_140 = 1;
        state.word_13c = 2;
        state.words_146_14a = Some([0, 0, 0]);
    }
    let mut host = docked(&fixture, &mut level);
    let row = {
        let menu = level.world.arena.player_shown_menu(level.player).unwrap();
        let records = menu.stack.current().unwrap().data().records();
        records
            .items
            .iter()
            .position(|item| item.action_callback == super::super::shop::SHOP_EVACUATE_ACTION)
            .expect("a staffed factory lists Evacuate") as i32
    };
    while selection(&level) < row {
        command(&mut level, &mut host, 1);
    }
    let evacuee = level
        .world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| level.world.arena.selector12_actor(actor).is_ok())
        .unwrap();
    host.evacuees = vec![evacuee];
    command(&mut level, &mut host, 4);
    let arena = &level.world.arena;
    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    assert_eq!(slots.handles[0], arena.actor(evacuee).unwrap().handle);
    assert_eq!(arena.actor(evacuee).unwrap().word_b4, 2);
    let state = &arena.selector15(factory).unwrap().intermediate;
    assert_eq!(state.word_140, 0);
    assert_eq!(arena.actor(factory).unwrap().word_b4, 0);
    assert!(!arena
        .player_shown_menu(level.player)
        .unwrap()
        .stack
        .active());
}

fn loose_scientists(level: &Level1) -> Vec<ObjectId> {
    let arena = &level.world.arena;
    arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector12_actor(actor).is_ok())
        .collect()
}

fn disposed(arena: &ObjectArena, actor: ObjectId) -> bool {
    arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == actor)
}

/// `0042F170` with free staff: the scientist's grants join the factory's,
/// and `0040DAB0` gives the factory its team, flashes at it, disposes of it
/// and adds one staff. The scientist that fills the factory records its
/// placement `+78` (`0048EAD0`); a full factory refuses the next.
#[test]
fn scientists_staff_the_factory() {
    let (fixture, mut level) = level1();
    let factory = level.factory;
    let scientists = loose_scientists(&level);
    let mut host = fixture.host(level.null());
    let arena = &mut level.world.arena;
    {
        let state = &mut arena.selector15_mut(factory).unwrap().intermediate;
        state.word_140 = 0;
        state.word_13c = 2;
    }
    arena.actor_mut(factory).unwrap().word_b4 = 0;
    arena.owned_mut(factory).unwrap().state.base.dirty_word = 0;
    for &scientist in &scientists[..3] {
        arena.actor_mut(scientist).unwrap().word_b4 = 2;
    }
    let grants = |arena: &ObjectArena, scientist| {
        let crate::object_runtime::Family429e20Class::Selector12(state) =
            &arena.family429e20_actor(scientist).unwrap().class
        else {
            panic!("not selector 12");
        };
        state.grants_24c.mask_04
    };
    let wanted = grants(arena, scientists[0]);
    let position = arena
        .object(scientists[0])
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert!(arena
        .factory_take_scientist(factory, scientists[0], &mut host)
        .unwrap());
    assert_eq!(host.events, [format!("flash {position:?}")]);
    assert!(disposed(arena, scientists[0]));
    assert_eq!(arena.actor(factory).unwrap().word_b4, 2);
    assert_eq!(
        arena.object(factory).unwrap().base.dirty_word & 0x202,
        0x202
    );
    assert_eq!(arena.selector15(factory).unwrap().intermediate.word_140, 1);
    let held = arena.selector15(factory).unwrap().grants_198.mask_04;
    assert!(wanted
        .iter()
        .zip(&held)
        .all(|(&wanted, &held)| wanted & held == wanted));

    host.events.clear();
    assert!(arena
        .factory_take_scientist(factory, scientists[1], &mut host)
        .unwrap());
    let index = arena.actor(factory).unwrap().byte_78 as i8;
    assert_eq!(host.events.len(), 2);
    assert_eq!(host.events[1], format!("placement {index}"));
    assert_eq!(arena.selector15(factory).unwrap().intermediate.word_140, 2);

    host.events.clear();
    assert!(!arena
        .factory_take_scientist(factory, scientists[2], &mut host)
        .unwrap());
    assert!(host.events.is_empty());
    assert!(!disposed(arena, scientists[2]));
}

/// `0043BEA0`, the course target's `+D0`, as the Defence Satellite's
/// hangar takes a scientist: the family's take (the flash, the scientist's
/// disposal, one staff and, full, the placement's record), then the
/// target's `+7A` granted to the first player, "Defence satellite
/// captured" for 0x35. A second scientist finds it full: refused, `+22E`
/// bit 0, and nothing granted. One of Level 1's targets stands in.
#[test]
fn a_course_target_that_a_scientist_fills_grants_its_7a() {
    let (fixture, mut level) = level1();
    let scientists = loose_scientists(&level);
    let mut host = fixture.host(level.null());
    let player = level.player;
    let arena = &mut level.world.arena;
    let target = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector39(actor).is_ok())
        .expect("Level 1's course targets");
    {
        let state = &mut arena.selector39_mut(target).unwrap().intermediate;
        state.word_140 = 0;
        state.word_13c = 1;
    }
    arena.actor_mut(target).unwrap().word_7a = 0x35;
    let owned = |arena: &ObjectArena| {
        arena.world_player(player).unwrap().grants_9c[0x35 / 32] & 1 << (0x35 % 32) != 0
    };
    assert!(!owned(arena));
    let position = arena
        .object(scientists[0])
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert!(arena
        .scientist_reaches_course_target(scientists[0], target, &mut host)
        .unwrap());
    let index = arena.actor(target).unwrap().byte_78 as i8;
    assert_eq!(
        host.events,
        [format!("flash {position:?}"), format!("placement {index}")]
    );
    assert!(disposed(arena, scientists[0]));
    assert_eq!(arena.selector39(target).unwrap().intermediate.word_140, 1);
    assert!(owned(arena));

    host.events.clear();
    arena.world_player_mut(player).unwrap().grants_9c[0x35 / 32] &= !(1 << (0x35 % 32));
    assert!(!arena
        .scientist_reaches_course_target(scientists[1], target, &mut host)
        .unwrap());
    assert!(host.events.is_empty());
    assert!(!disposed(arena, scientists[1]));
    assert_eq!(
        arena.family428650_actor(scientists[1]).unwrap().word_22e & 1,
        1
    );
    assert!(!owned(arena));
}

/// `0040E090` as the interaction relation a scientist's search divides
/// by: the free staff, but none for another team's scientist once the
/// factory has a team.
#[test]
fn a_factory_with_room_draws_scientists() {
    let (_, mut level) = level1();
    let factory = level.factory;
    let scientist = loose_scientists(&level)[0];
    let arena = &mut level.world.arena;
    {
        let state = &mut arena.selector15_mut(factory).unwrap().intermediate;
        state.word_140 = 1;
        state.word_13c = 3;
    }
    let relation = |arena: &ObjectArena| {
        arena
            .query_actor_interaction_relation(
                crate::object_runtime::ActorInteractionRelationRequest {
                    component: factory,
                    source: Some(scientist),
                },
            )
            .unwrap()
    };
    arena.actor_mut(factory).unwrap().word_b4 = 0;
    assert_eq!(relation(arena), 2);
    arena.actor_mut(factory).unwrap().word_b4 = 2;
    arena.actor_mut(scientist).unwrap().word_b4 = 1;
    assert_eq!(relation(arena), 0);
    arena.actor_mut(scientist).unwrap().word_b4 = 2;
    assert_eq!(relation(arena), 2);
}

/// `0042F670` from the docked shop: the hold's scientists join the factory
/// from the last occupied slot while it has room; the full factory's menu
/// is rebuilt without Beam down. The hold keeps their handles until its
/// next tick.
#[test]
fn beam_down_staffs_the_factory_from_the_hold() {
    let (fixture, mut level) = level1();
    let factory = level.factory;
    let scientists = loose_scientists(&level);
    {
        let arena = &mut level.world.arena;
        let state = &mut arena.selector15_mut(factory).unwrap().intermediate;
        state.word_140 = 0;
        state.word_13c = 2;
    }
    let mut host = docked(&fixture, &mut level);
    // Two scientists aboard, taken as the vehicle's team.
    for &scientist in &scientists[..2] {
        let arena = &mut level.world.arena;
        arena.actor_mut(scientist).unwrap().word_b4 = 2;
        assert!(host
            .carry_scientist(arena, level.vehicle, scientist)
            .unwrap());
    }
    let beam_down = |level: &Level1| {
        let menu = level.world.arena.player_shown_menu(level.player).unwrap();
        let records = menu.stack.current().unwrap().data().records();
        records
            .items
            .iter()
            .position(|item| item.action_callback == super::super::shop::SHOP_BEAM_DOWN_ACTION)
            .map(|row| row as i32)
    };
    let row = beam_down(&level).expect("a factory short of staff lists Beam down");
    while selection(&level) < row {
        command(&mut level, &mut host, 1);
    }
    while selection(&level) > row {
        command(&mut level, &mut host, 0);
    }
    host.events.clear();
    command(&mut level, &mut host, 4);
    let arena = &level.world.arena;
    assert_eq!(arena.selector15(factory).unwrap().intermediate.word_140, 2);
    assert!(scientists[..2]
        .iter()
        .all(|&scientist| disposed(arena, scientist)));
    assert_eq!(arena.actor(factory).unwrap().word_b4, 2);
    let flashes = host
        .events
        .iter()
        .filter(|event| event.starts_with("flash"))
        .count();
    assert_eq!(flashes, 2);
    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    let null = level.null();
    assert_eq!(
        slots
            .handles
            .iter()
            .filter(|&&handle| handle != null)
            .count(),
        2
    );
    assert_eq!(beam_down(&level), None);
}

/// Runs production to its grant: build, fitting, then the grant phase.
fn produce(arena: &mut ObjectArena, factory: ObjectId, host: &mut Host<'_>) {
    for _ in 0..80 {
        arena.tick_selector15(factory, 125_000, host).unwrap();
        if arena.selector15(factory).unwrap().word_160 == 6 {
            return;
        }
    }
    panic!("production never reached its recovery");
}

/// `0042E89E` with a bought Bomb (resource 0x6F): the factory's +4C builds
/// it at the dock point, dirty on team 0, and the hold takes it.
#[test]
fn a_bought_bomb_goes_into_the_hold() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let bomb = *(0..0x12)
        .find_map(|index| host.tables.product(0, index).ok().filter(|p| p.id == 0x1f))
        .unwrap();
    assert_eq!((bomb.kind, bomb.amount), (0x6f, 1));
    let sine = fixture.assets.sine_table();
    host.items = Some(ItemWorld {
        terrain: &level.world.terrain,
        listeners: &mut level.world.listeners,
        world_flags: level.world.world_flags,
        sine: &sine.values,
    });
    let factory = level.factory;
    let arena = &mut level.world.arena;
    arena
        .start_factory_production(factory, &bomb, false)
        .unwrap();
    arena
        .set_factory_state(factory, 3, false, &mut host)
        .unwrap();
    host.events.clear();
    produce(arena, factory, &mut host);
    // The factory's final table binds +4C to 0047AAF0 and +48 (the child's
    // source basis) to 004026B0.
    let exe = fixture.assets.executable_bytes();
    let slot = |offset: usize| {
        let at = 0x0052_1940 - 0x0040_0000 + offset;
        u32::from_le_bytes(exe[at..at + 4].try_into().unwrap())
    };
    assert_eq!((slot(0x4c), slot(0x48)), (0x0047_aaf0, 0x0040_26b0));
    let items: Vec<_> = host
        .events
        .iter()
        .filter(|event| event.starts_with("item") || event.starts_with("carry"))
        .cloned()
        .collect();
    assert_eq!(
        items,
        [
            format!("item RegisterLowWord(111) {TRUCK_DOCK:?}"),
            format!("carry at {TRUCK_DOCK:?} team 0 +38 6"),
        ]
    );
    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    let bomb = arena
        .resolve_actor_handle(slots.handles[0])
        .unwrap()
        .unwrap();
    assert!(arena.bomb29(bomb).is_ok());
    // Built as the factory's child; the Bomb's capture makes the vehicle
    // its owner.
    assert_eq!(
        arena.actor(bomb).unwrap().owner_handle,
        arena.actor(level.vehicle).unwrap().handle
    );
    assert_eq!(arena.selector15(factory).unwrap().word_184, 0);
}

/// With fewer free slots than items, each point moves back along the
/// factory's forward row by `((draw & 0xFFFF) >> 9) + 200`; after the
/// Remote Beamer (resource 7) the rest are its receivers (0x25).
#[test]
fn a_short_hold_scatters_the_items() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    host.evacuees = loose_scientists(&level);
    let factory = level.factory;
    // The truck factory faces +Z.
    assert_eq!(
        level
            .world
            .arena
            .actor(factory)
            .unwrap()
            .rotating
            .basis
            .words[7..],
        [0, 0, 0x3f80_0000]
    );
    let state = level.world.arena.selector15_mut(factory).unwrap();
    state.word_184 = 7;
    state.amount_18e = Some(4);
    state.product_18c = Some(0x6d);
    state.word_160 = 4;
    host.events.clear();
    produce(&mut level.world.arena, factory, &mut host);
    let items: Vec<_> = host
        .events
        .iter()
        .filter(|event| {
            ["item", "carry", "text"]
                .iter()
                .any(|kind| event.starts_with(kind))
        })
        .cloned()
        .collect();
    // The seed's first draw is 0x13 (distance 200); the hold has three
    // free slots, so all four move, and the fourth stays outside.
    let [x, y, z] = TRUCK_DOCK;
    let built = |resource, back: u16| {
        let position = [x, y, z - back];
        [
            format!("item RegisterLowWord({resource}) {position:?}"),
            format!("carry at {position:?} team 0 +38 2"),
        ]
    };
    let mut expected = vec!["text 143".to_string()];
    expected.extend(built(7, 200));
    expected.extend(built(37, 248));
    expected.extend(built(37, 204));
    expected.extend(built(37, 217));
    assert_eq!(items, expected);
    let flashes = host
        .events
        .iter()
        .filter(|event| event.starts_with("Particle"))
        .count();
    assert_eq!(flashes, 3);
    assert_eq!(level.world.arena.selector15(factory).unwrap().word_184, 0);
}

/// A bought Remote Beamer (resource 7, then its receiver 0x25) with a full
/// hold: both are built where they stand, and a full charge beams the
/// scientist beside the beamer to the receiver (00473D70): the scientist
/// joins team 2, learns grants 0x6D and 0x6E, and the charge empties.
#[test]
fn a_bought_beamer_beams_a_scientist_to_its_receiver() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let scientists = loose_scientists(&level);
    assert!(scientists.len() >= 4, "{}", scientists.len());
    let fillers: Vec<ActorHandle> = scientists[..3]
        .iter()
        .map(|&actor| level.world.arena.actor(actor).unwrap().handle)
        .collect();
    level
        .world
        .arena
        .selector2_actor_mut(level.vehicle)
        .unwrap()
        .handles_1a4
        .handles
        .copy_from_slice(&fillers);
    let sine = fixture.assets.sine_table();
    host.items = Some(ItemWorld {
        terrain: &level.world.terrain,
        listeners: &mut level.world.listeners,
        world_flags: level.world.world_flags,
        sine: &sine.values,
    });
    let factory = level.factory;
    let state = level.world.arena.selector15_mut(factory).unwrap();
    state.word_184 = 7;
    state.amount_18e = Some(2);
    state.product_18c = Some(0x6d);
    state.word_160 = 4;
    produce(&mut level.world.arena, factory, &mut host);
    let arena = &mut level.world.arena;
    let beamer = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector55(actor).is_ok())
        .expect("the bought beamer");
    let receiver = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.is_selector56(actor))
        .expect("its receiver");
    let [x, y, z] = arena.object(beamer).unwrap().base.position_words.unwrap();
    let person = scientists[3];
    arena.owned_mut(person).unwrap().state.base.position_words = Some([x + 100, y, z]);
    assert_eq!(
        arena.actor(person).unwrap().word_6c,
        arena.actor(beamer).unwrap().word_6c
    );
    let arrival = arena.object(receiver).unwrap().base.position_words.unwrap();
    // Charging: 125 ms adds 0x257.
    let context = crate::object_runtime::VehicleCarryContext {
        process: &host.process,
        executable: host.executable,
        resident: host.resident,
        text_data: Some(&host.text),
    };
    let mut events = Vec::new();
    let mut rng = 0;
    arena
        .tick_selector55(
            beamer,
            125_000,
            context,
            &mut CarryRecorder(&mut events),
            &mut rng,
        )
        .unwrap();
    assert_eq!(arena.selector55(beamer).unwrap().charge_138, 0x257);
    arena.selector55_mut(beamer).unwrap().charge_138 = 0x6000;
    arena
        .tick_selector55(
            beamer,
            125_000,
            context,
            &mut CarryRecorder(&mut events),
            &mut rng,
        )
        .unwrap();
    let state = arena.selector55(beamer).unwrap();
    assert_eq!(state.charge_138, 0);
    assert_eq!(
        state.receiver_13c,
        Some(arena.actor(receiver).unwrap().handle)
    );
    assert_eq!(
        arena.object(person).unwrap().base.position_words,
        Some(arrival)
    );
    assert_eq!(arena.actor(person).unwrap().word_b4, 2);
    let mask = &arena.selector12_actor(person).unwrap().grants_24c.mask_04;
    for id in [0x6d_usize, 0x6e] {
        assert_ne!(mask[id / 32] & (1 << (id % 32)), 0, "grant {id:#x}");
    }
    assert_eq!(
        events,
        [
            format!(
                "Particle {{ selector: 2, position: {:?}, flags: 0 }}",
                [x + 100, y, z]
            ),
            format!("Particle {{ selector: 2, position: {arrival:?}, flags: 0 }}"),
        ]
    );
}

/// A Prison Unit's world: its takes go through the hold as the session's
/// host sends them, and 00482C60's slot is one team's source.
struct PrisonRecorder<'a> {
    context: crate::object_runtime::VehicleCarryContext<'a>,
    events: Vec<String>,
    rng: u32,
    null: ActorHandle,
    task: Option<(u32, ActorHandle, bool)>,
}

impl crate::object_runtime::FamilyUpdateHost for PrisonRecorder<'_> {
    type Error = String;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: crate::object_runtime::FamilyUpdateEffect,
    ) -> Result<(), String> {
        Err(format!("{effect:?}"))
    }
}

impl crate::object_runtime::PrisonHost for PrisonRecorder<'_> {
    fn take_prisoner(
        &mut self,
        arena: &mut ObjectArena,
        prison: ObjectId,
        person: ObjectId,
    ) -> Result<bool, String> {
        let mut rng = self.rng;
        let taken = arena
            .take_into_hold(
                prison,
                person,
                self.context,
                &mut CarryRecorder(&mut self.events),
                &mut rng,
            )
            .map_err(|error| error.to_string());
        self.rng = rng;
        taken
    }
    fn task_source(&mut self, team: u32, source: ActorHandle, enable: bool) -> Result<(), String> {
        self.task = Some((team, source, enable));
        Ok(())
    }
}

/// A bought Prison Unit (resource 13) with room in the hold goes in: its
/// attach (0046E530) takes the vehicle's team, so 00421810 asks its +5C,
/// which binds 00401300 and answers NULL, and the team change is skipped.
/// The port answered only a scientist's view and stopped the factory (the
/// user's 2026-10-21 session on Kankutan).
#[test]
fn a_bought_prison_unit_goes_into_a_hold_with_room() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let sine = fixture.assets.sine_table();
    host.items = Some(ItemWorld {
        terrain: &level.world.terrain,
        listeners: &mut level.world.listeners,
        world_flags: level.world.world_flags,
        sine: &sine.values,
    });
    let factory = level.factory;
    let state = level.world.arena.selector15_mut(factory).unwrap();
    state.word_184 = 13;
    state.amount_18e = Some(1);
    state.product_18c = Some(0x71);
    state.word_160 = 4;
    produce(&mut level.world.arena, factory, &mut host);
    let arena = &level.world.arena;
    let prison = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector17(actor).is_ok())
        .expect("the bought Prison Unit");
    let handle = arena.actor(prison).unwrap().handle;
    let hold = &arena
        .selector2_actor(level.vehicle)
        .unwrap()
        .handles_1a4
        .handles;
    assert!(hold.contains(&handle), "{hold:?}");
    assert_eq!(
        arena.actor(prison).unwrap().word_b4,
        arena.actor(level.vehicle).unwrap().word_b4
    );
}

/// Level 1's factory builds a bought Prison Unit (resource 13) beside it
/// with the hold full. A second arms it, and the same update's scan takes
/// the scientist of team 0x12 strolling within 0x300 into its hold: the
/// hold puts it at the unit and gives it the unit's basis.
#[test]
fn a_bought_prison_unit_takes_a_person_of_team_0x12() {
    let (fixture, mut level) = level1();
    let mut host = docked(&fixture, &mut level);
    let scientists = loose_scientists(&level);
    assert!(scientists.len() >= 4, "{}", scientists.len());
    let fillers: Vec<ActorHandle> = scientists[..3]
        .iter()
        .map(|&actor| level.world.arena.actor(actor).unwrap().handle)
        .collect();
    level
        .world
        .arena
        .selector2_actor_mut(level.vehicle)
        .unwrap()
        .handles_1a4
        .handles
        .copy_from_slice(&fillers);
    let sine = fixture.assets.sine_table();
    host.items = Some(ItemWorld {
        terrain: &level.world.terrain,
        listeners: &mut level.world.listeners,
        world_flags: level.world.world_flags,
        sine: &sine.values,
    });
    let factory = level.factory;
    let state = level.world.arena.selector15_mut(factory).unwrap();
    state.word_184 = 13;
    state.amount_18e = Some(1);
    state.product_18c = Some(0x71);
    state.word_160 = 4;
    produce(&mut level.world.arena, factory, &mut host);
    let arena = &mut level.world.arena;
    let prison = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector17(actor).is_ok())
        .expect("the bought Prison Unit");
    assert_eq!(
        arena.selector17(prison).unwrap().radius_138,
        crate::object_runtime::actor::PRISON_RADIUS
    );
    let [x, y, z] = arena.object(prison).unwrap().base.position_words.unwrap();
    let person = scientists[3];
    arena.owned_mut(person).unwrap().state.base.position_words = Some([x + 0x200, y, z]);
    arena.actor_mut(person).unwrap().word_b4 = 0x12;
    let null = arena.actor_handles().unwrap().null_handle();
    let mut world = PrisonRecorder {
        context: crate::object_runtime::VehicleCarryContext {
            process: &host.process,
            executable: host.executable,
            resident: host.resident,
            text_data: Some(&host.text),
        },
        events: Vec::new(),
        rng: 0,
        null,
        task: None,
    };
    arena
        .tick_selector17(prison, 500_000, None, &mut world)
        .unwrap();
    assert_eq!(arena.selector17(prison).unwrap().arming_13c, 500_000);
    arena
        .tick_selector17(prison, 500_000, None, &mut world)
        .unwrap();
    let state = arena.selector17(prison).unwrap();
    assert_eq!(state.armed_154, 1);
    assert_eq!(
        state.hold_144.handles,
        [arena.actor(person).unwrap().handle]
    );
    assert_eq!(state.team_158, Some(arena.actor(person).unwrap().word_b4));
    assert_eq!(state.kind_15c, arena.actor(person).unwrap().word_7a);
    assert_eq!(
        arena.object(person).unwrap().base.position_words,
        Some([x, y, z])
    );
    // 004122A0 flashes where the person stood; without grant 0x15 on its
    // view the unit names no task source.
    assert_eq!(
        world.events,
        [format!(
            "Particle {{ selector: 2, position: {:?}, flags: 0 }}",
            [x + 0x200, y, z]
        )]
    );
    assert_eq!((state.holding_15e, world.task), (0, None));
    // The next scan from the RNG's first draw past the zero seed.
    let mut rng = 0;
    let draw = crate::name_entry::name_entry_rng_next(&mut rng);
    assert_eq!(state.scan_140 as u32, (draw & 0xffff) * 8 + 1_000_000);
}

/// `00431A90`, another machine's factory: the first player's unit on its
/// pad docks at once, past a cooldown the local update would wait out and
/// without the silent commands; once docked, nothing more runs.
#[test]
fn a_remote_factory_docks_as_00431a90() {
    let (fixture, mut level) = level1();
    level.place(TRUCK_DOCK, [0; 3]);
    let mut host = fixture.host(level.null());
    let factory = level.factory;
    level.world.arena.selector15_mut(factory).unwrap().word_1c4 = 1_000_000;
    level
        .world
        .arena
        .tick_remote_selector15(factory, &mut host)
        .unwrap();
    let arena = &level.world.arena;
    let state = arena.selector15(factory).unwrap().clone();
    assert_eq!(state.handle_188, arena.actor(level.vehicle).unwrap().handle);
    assert_eq!(state.word_1c4, 1_000_000);
    assert!(!host.events.iter().any(|event| event.starts_with("command")));
    level
        .world
        .arena
        .tick_remote_selector15(factory, &mut host)
        .unwrap();
    assert_eq!(*level.world.arena.selector15(factory).unwrap(), state);
}

/// `0040D870`, which the factory's destructor `0042E320` reaches, stops its
/// hum: a factory deleted with a live `+154` (as its world ends, before the
/// briefing) releases the record, and the deletion needs the sound service
/// to do so.
#[test]
fn a_deleted_factory_stops_its_hum() {
    use crate::object_runtime::{ObjectDestructionRequest, ObjectError, SoundTeardownService};
    use crate::process_sound::{
        ProcessSoundRecordError, ProcessSoundRecordState, ProcessSoundRuntime,
        RequiredProcessSoundVoiceTeardown,
    };
    let (fixture, mut level) = level1();
    let executable = FrontendExecutable::parse(fixture.assets.executable_bytes()).unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let hum = sounds
        .create_persistent(&fixture.resident, 0x4b, 1, 1, 0, || {
            Ok::<_, std::convert::Infallible>([0; 3])
        })
        .unwrap();
    let factory = level.factory;
    let arena = &mut level.world.arena;
    arena
        .selector15_mut(factory)
        .unwrap()
        .intermediate
        .sound_154 = Some(hum);
    assert_eq!(
        arena
            .destroy(ObjectDestructionRequest {
                object: factory,
                sound: SoundTeardownService::RequiredAtReach,
            })
            .err(),
        Some(ObjectError::Sound(
            ProcessSoundRecordError::RequiredTeardown(hum)
        ))
    );
    assert_eq!(
        arena.selector15(factory).unwrap().intermediate.sound_154,
        Some(hum)
    );
    arena
        .destroy(ObjectDestructionRequest {
            object: factory,
            sound: SoundTeardownService::Available {
                sounds: &mut sounds,
                voices: &mut RequiredProcessSoundVoiceTeardown,
            },
        })
        .unwrap();
    assert!(arena.object(factory).is_err());
    assert_eq!(
        sounds.records().snapshot(hum).unwrap().state,
        ProcessSoundRecordState::Released
    );
}
