//! The Force Field against its disassembly (00476750, 0047DE30, 00477210,
//! 004772B0, 00478860, 004787E0, 004771D0, 004769C0, 004769F0 and
//! 00476AE0) and the fields of Infest03 and Infest04 (`force-field.js`).

use inf_formats::{ByteSpan, InstanceField};

use super::*;
use crate::object_runtime::{ActorHandle, ActorRegistryConfig, ActorTerrainInputs};

const SINE: [u16; 4096] = [0; 4096];

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn request(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

/// A field built at the zero-filled pre-context position, then placed.
fn field(
    arena: &mut ObjectArena,
    listeners: &mut CreationListeners,
    position: [u16; 3],
    radius: u32,
    height: u32,
) -> ObjectId {
    let field = arena
        .allocate_selector18(request(Some([0; 3])), listeners)
        .unwrap();
    arena.owned_mut(field).unwrap().state.base.position_words = Some(position);
    let state = arena.selector18_mut(field).unwrap();
    state.radius_154 = radius;
    state.height_158 = height;
    arena.owned_mut(field).unwrap().state.base.word_28 = radius as u16;
    field
}

/// A shared actor at `position`, collidable (flag 4) unless `flags` say.
fn actor(arena: &mut ObjectArena, position: [u16; 3], flags: u32) -> ObjectId {
    let actor = arena.allocate_actor(request(Some(position))).unwrap();
    arena.owned_mut(actor).unwrap().state.base.flags = flags;
    actor
}

/// 0047DE30: the cursor starts at 1 and shifts left before each test, so
/// the first field takes bit 2; past the top bit it wraps to 1. 0047DE80
/// gives a bit back, and with all 32 taken the 32nd shift gives up with 0,
/// the cursor left where it stopped.
#[test]
fn bits_rotate_from_one_and_wrap() {
    let mut bits = ForceFieldBits::default();
    let first: Vec<u32> = (0..31).map(|_| bits.allocate()).collect();
    assert_eq!(first, (1..32).map(|shift| 1 << shift).collect::<Vec<_>>());
    assert_eq!(bits.allocate(), 1);
    assert_eq!(bits.used, u32::MAX);
    assert_eq!(bits.allocate(), 0);
    assert_eq!(bits.cursor, 1);
    bits.release(0x10);
    assert_eq!(bits.allocate(), 0x10);
    assert_eq!(bits.cursor, 0x10);
}

/// 00476750: radius 0x400 and height 0x300 until the commands, +28 the
/// radius's WORD, a bit of the pool, then 004772B0(1, 0) at the
/// pre-context position, which marks the field itself (its flags 0x124
/// admit it) and nothing farther than the radius, and the listener binding.
/// Infest03 and Infest04's fifteen fields each marked only themselves.
#[test]
fn construction_marks_the_field_and_binds_its_listener() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let near = actor(&mut arena, [0x3ff, 0, 0], 4);
    let far = actor(&mut arena, [0x400, 0, 0], 4);
    let below = actor(&mut arena, [0, (-100_i16) as u16, 0x3ff], 4);
    let field = arena
        .allocate_selector18(request(Some([0; 3])), &mut listeners)
        .unwrap();
    let state = arena.selector18(field).unwrap();
    assert_eq!(state.bit_16c, 2);
    assert_eq!((state.radius_154, state.height_158), (0x400, 0x300));
    assert!(state.on_176 && state.shown_177);
    assert_eq!(arena.object(field).unwrap().base.word_28, 0x400);
    assert_eq!(arena.actor(field).unwrap().word_6c, 2);
    assert_eq!(arena.actor(near).unwrap().word_6c, 2);
    assert_eq!(arena.actor(far).unwrap().word_6c, 0);
    // Below the centre the height counts for nothing.
    assert_eq!(arena.actor(below).unwrap().word_6c, 2);
    let bound: Vec<_> = listeners.registrations().collect();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].owner, field);
    assert_eq!(bound[0].interface, CreationListenerInterface::Selector18);
}

fn context() -> ActorCommandContext<'static> {
    static RUNTIME: std::sync::OnceLock<crate::ResidentRuntime> = std::sync::OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap()
    });
    ActorCommandContext {
        runtime,
        first_player_grants: None,
        terrain: None,
    }
}

fn view(code: i32, payload: InstanceCommandPayload) -> InstanceCommandView {
    InstanceCommandView {
        target: InstanceCommandTarget::Constructor(18),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0047_8860),
        payload_consumer: Some(0x0047_8860),
        payload,
    }
}

fn field_value<T>(value: T) -> InstanceField<T> {
    InstanceField {
        span: ByteSpan { start: 4, end: 6 },
        value,
    }
}

/// 00478860: 0x10 the radius (and +28), 0x11 the height, 0x12 the key
/// grant, 0x13 BYTE != 0, 0x14 the hint; past 0x14 it refuses.
#[test]
fn commands_store_the_shape_the_key_and_the_hint() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0; 3], 0x400, 0x300);
    let apply = |arena: &mut ObjectArena, code, payload| {
        arena
            .apply_selector18_command(field, &view(code, payload), context())
            .unwrap()
    };
    assert!(apply(
        &mut arena,
        0x10,
        InstanceCommandPayload::Selector18Radius(field_value(0x5dc))
    ));
    assert!(apply(
        &mut arena,
        0x11,
        InstanceCommandPayload::Selector18Height(field_value(0x2ee))
    ));
    assert!(apply(
        &mut arena,
        0x12,
        InstanceCommandPayload::Selector18KeyGrant(field_value(0x31))
    ));
    assert!(apply(
        &mut arena,
        0x13,
        InstanceCommandPayload::Selector18TeamPasses(field_value(2))
    ));
    assert!(apply(
        &mut arena,
        0x14,
        InstanceCommandPayload::Selector18Hint(field_value(0x8001_0002))
    ));
    assert!(!apply(
        &mut arena,
        0x15,
        InstanceCommandPayload::NoPayloadRead
    ));
    let state = arena.selector18(field).unwrap();
    assert_eq!((state.radius_154, state.height_158), (0x5dc, 0x2ee));
    assert_eq!(state.key_grant_17a, 0x31);
    assert!(state.team_passes_178);
    assert_eq!(state.hint_180, 0x8001_0002);
    assert_eq!(arena.object(field).unwrap().base.word_28, 0x5dc);
    assert!(apply(
        &mut arena,
        0x13,
        InstanceCommandPayload::Selector18TeamPasses(field_value(0))
    ));
    assert!(!arena.selector18(field).unwrap().team_passes_178);
}

/// 004787E0: 7 links the field and switches it on, re-marking at its own
/// radius; 8 unlinks; 0xB and 0xC switch only a field below radius 1000
/// off, for 5 s. Tabuchi's generator links its field at the after-load,
/// as do Artoria's two generators theirs.
#[test]
fn links_switch_it_on_and_blasts_drop_a_small_field() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0x1000, 0, 0x1000], 999, 0x300);
    let inside = actor(&mut arena, [0x1000 + 998, 0, 0x1000], 4);
    arena.switch_selector18(field, false, 0).unwrap();
    assert_eq!(arena.selector18_message(field, 7).unwrap(), 0);
    let state = arena.selector18(field).unwrap();
    assert!(state.linked_175 && state.on_176 && state.shown_177);
    assert_eq!(state.links_17c, 1);
    assert_eq!(arena.actor(inside).unwrap().word_6c, 2);
    arena.selector18_message(field, 8).unwrap();
    assert_eq!(arena.selector18(field).unwrap().links_17c, 0);
    arena.selector18_message(field, 0xb).unwrap();
    let state = arena.selector18(field).unwrap();
    assert!(!state.on_176);
    assert_eq!(state.off_timer_15c, FORCE_FIELD_ALIEN_PASS_OFF);
    assert_eq!(arena.actor(inside).unwrap().word_6c, 0);
    arena.switch_selector18(field, true, 0).unwrap();
    arena.selector18_mut(field).unwrap().radius_154 = 1000;
    arena.selector18_message(field, 0xc).unwrap();
    assert!(arena.selector18(field).unwrap().on_176);
}

/// 00477350, the listener: Infest04's three new actors inside Tabuchi's
/// field, and Artoria's spawned actor 3609800 inside both of its
/// concentric fields, each from 0 to the field's bit (6C 0->2, 4->C).
#[test]
fn the_listener_marks_new_actors_as_recorded() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let tabuchi = field(
        &mut arena,
        &mut listeners,
        [0xaa66, 0xfd6a, 0x8d97],
        0x5dc,
        0x2ee,
    );
    for position in [
        [0xaf23, 0xfdc3, 0x8e2b],
        [0xaeb3, 0xfdc2, 0x8dd3],
        [0xafe0, 0xfdc3, 0x8de1],
    ] {
        let new = actor(&mut arena, position, 4);
        arena.selector18_listener(tabuchi, new).unwrap();
        assert_eq!(arena.actor(new).unwrap().word_6c, 2, "{position:x?}");
    }
    let mut artoria = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let outer = field(
        &mut artoria,
        &mut listeners,
        [0xf299, 0, 0x7500],
        0x5dc,
        0x2bc,
    );
    let inner = field(
        &mut artoria,
        &mut listeners,
        [0xf299, 0, 0x7500],
        0x515,
        0x28a,
    );
    assert_eq!(artoria.selector18(outer).unwrap().bit_16c, 2);
    assert_eq!(artoria.selector18(inner).unwrap().bit_16c, 4);
    let new = actor(&mut artoria, [0xf204, 0, 0x751c], 4);
    artoria.selector18_listener(outer, new).unwrap();
    artoria.selector18_listener(inner, new).unwrap();
    assert_eq!(artoria.actor(new).unwrap().word_6c, 6);
    // Flag 0x40 admits it but for 0x80; otherwise 8 or no 4 keeps it out.
    for (flags, marked) in [
        (0x40, true),
        (0xc0, false),
        (0x0c, false),
        (0, false),
        (0x44, true),
    ] {
        let other = actor(&mut artoria, [0xf204, 0, 0x751c], flags);
        artoria.selector18_listener(outer, other).unwrap();
        assert_eq!(
            artoria.actor(other).unwrap().word_6c == 2,
            marked,
            "{flags:#x}"
        );
    }
}

/// What the update reaches outside the arena, recorded.
#[derive(Default)]
struct Recorder {
    player_actors: Vec<ObjectId>,
    first_player: Option<PlayerId>,
    vehicle: Option<ObjectId>,
    owned_grants: Vec<u16>,
    bounces: Vec<(ObjectId, Option<[u16; 3]>)>,
    told: Vec<u16>,
    commands: Vec<u32>,
    grants: Vec<u16>,
}

const PLAYER: PlayerId = PlayerId::unissued(7);

impl Selector18Host for Recorder {
    type Error = String;
    fn find_player(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, String> {
        Ok(self.player_actors.contains(&actor).then_some(PLAYER))
    }
    fn first_player(&mut self, _: &mut ObjectArena) -> Result<Option<PlayerId>, String> {
        Ok(self.first_player)
    }
    fn player_vehicle(&mut self, _: &mut ObjectArena) -> Result<Option<ObjectId>, String> {
        Ok(self.vehicle)
    }
    fn owns_grant(&mut self, _: &mut ObjectArena, _: PlayerId, grant: u16) -> Result<bool, String> {
        Ok(self.owned_grants.contains(&grant))
    }
    fn grant(&mut self, _: &mut ObjectArena, _: PlayerId, grant: u16) -> Result<(), String> {
        self.grants.push(grant);
        Ok(())
    }
    fn player_command(
        &mut self,
        _: &mut ObjectArena,
        _: PlayerId,
        command: u32,
    ) -> Result<(), String> {
        self.commands.push(command);
        Ok(())
    }
    fn bounce(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        actor: ObjectId,
        correction: Option<[u16; 3]>,
    ) -> Result<(), String> {
        self.bounces.push((actor, correction));
        Ok(())
    }
    fn tell(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
        text: u16,
    ) -> Result<(), String> {
        self.told.push(text);
        Ok(())
    }
}

/// One of `pushes04.txt`'s records: the actor's position, +28, +8C and
/// speed before 0047B510, whether it is the player's, and the push.
struct Push {
    flags: u32,
    position: [u16; 3],
    extent: u16,
    word_8c: u16,
    speed: [u16; 3],
    player: bool,
    push: [u16; 3],
}

/// All ten of Infest04's pushes, by Tabuchi's field (radius 0x5DC, height
/// 0x2EE, bit 2): the vehicle crossing out twice (600 more than its speed
/// over the edge), and a creature held inside eight times (200), five of
/// those pushes truncating to almost nothing.
#[test]
fn the_recorded_pushes_replay() {
    let pushes = [
        Push {
            flags: 0x4a18_6937,
            position: [0xafec, 0xfdc3, 0x8e3f],
            extent: 0x6d,
            word_8c: 0xc8,
            speed: [0xdc, 0xfffd, 0x48],
            player: true,
            push: [0xfcd5, 0xff9a, 0xffa0],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa472, 0xfdb3, 0x8ba8],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0xfcdf, 0x3c, 0xff33],
            player: false,
            push: [0x3cc, 0xffa3, 0x13b],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa472, 0xfdb3, 0x8ba8],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0xab, 0xffdf, 0x6e],
            player: false,
            push: [1, 0, 0],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa49a, 0xfd7f, 0x8bc0],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0xa8, 0xfef1, 0x6b],
            player: false,
            push: [0, 0, 0],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa49a, 0xfd7f, 0x8bc0],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0xa8, 0xfef1, 0x6b],
            player: false,
            push: [0, 0, 0],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa4bb, 0xfd80, 0x8bd5],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0x7d, 0x77, 0x4e],
            player: false,
            push: [0x3b, 0xffff, 0x12],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa4bb, 0xfd80, 0x8bd5],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0xb8, 0x76, 0x60],
            player: false,
            push: [0, 0, 0],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa4e5, 0xfd72, 0x8beb],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0x89, 0x4d, 0x47],
            player: false,
            push: [0x2f, 0, 0xe],
        },
        Push {
            flags: 0x4a10_0807,
            position: [0xa4e5, 0xfd72, 0x8beb],
            extent: 0x24,
            word_8c: 0xa,
            speed: [0xb8, 0x4d, 0x55],
            player: false,
            push: [0, 0, 0],
        },
        Push {
            flags: 0x4a18_6937,
            position: [0xafe0, 0xfdc3, 0x8de1],
            extent: 0x6d,
            word_8c: 0xc8,
            speed: [0x2de, 3, 0x2d],
            player: true,
            push: [0xfadc, 0xff59, 0xffbb],
        },
    ];
    for (index, record) in pushes.iter().enumerate() {
        let mut arena = arena();
        let mut listeners = CreationListeners::new();
        let field = field(
            &mut arena,
            &mut listeners,
            [0xaa66, 0xfd6a, 0x8d97],
            0x5dc,
            0x2ee,
        );
        arena.actor_mut(field).unwrap().word_b4 = 0x12;
        // Fields leave their own update by flag 8.
        arena.owned_mut(field).unwrap().state.base.flags = 0x4890_004b;
        let pushed = actor(&mut arena, record.position, record.flags);
        arena.owned_mut(pushed).unwrap().state.base.word_28 = record.extent;
        let state = arena.actor_mut(pushed).unwrap();
        state.word_6c = 2;
        state.word_8c = record.word_8c;
        state.words_8e_92 = record.speed;
        let mut host = Recorder::default();
        if record.player {
            host.player_actors.push(pushed);
        }
        arena.selector18_keep(field, pushed, &mut host).unwrap();
        let expected: [u16; 3] =
            std::array::from_fn(|axis| record.speed[axis].wrapping_add(record.push[axis]));
        assert_eq!(
            arena.actor(pushed).unwrap().words_8e_92,
            expected,
            "push {index}"
        );
        assert_eq!(host.bounces, vec![(pushed, None)], "push {index}");
    }
}

/// 00476DC6..00476E3E: a +8C of 1 hears where the edge is, the gap to it
/// (inside: one short of the edge) along the normal.
#[test]
fn a_word_8c_of_one_hears_the_correction() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    arena.owned_mut(field).unwrap().state.base.flags = 0x4890_004b;
    let held = actor(&mut arena, [1100, 0, 0], 4);
    let state = arena.actor_mut(held).unwrap();
    state.word_6c = 2;
    state.word_8c = 1;
    let mut host = Recorder::default();
    arena.selector18_keep(field, held, &mut host).unwrap();
    // Edge 1000 - 0, distance 1100: 1000 - 1100 - 1 = -101 along +X; the
    // push is -(0 + 200).
    assert_eq!(host.bounces, vec![(held, Some([(-101_i16) as u16, 0, 0]))]);
    assert_eq!(
        arena.actor(held).unwrap().words_8e_92,
        [(-200_i16) as u16, 0, 0]
    );
    // Outside, approaching: one beyond the edge.
    let field_outside = actor(&mut arena, [990, 0, 0], 4);
    arena.actor_mut(field_outside).unwrap().word_8c = 1;
    let mut host = Recorder::default();
    arena
        .selector18_keep(field, field_outside, &mut host)
        .unwrap();
    assert_eq!(host.bounces, vec![(field_outside, Some([11, 0, 0]))]);
    assert_eq!(arena.actor(field_outside).unwrap().words_8e_92, [200, 0, 0]);
}

/// 004769F0: with +178, its own team (or 1 against 2) passes a non-alien
/// field once no alien stands near (off for 2 s), an alien field at once
/// (5 s); the key grant's owner passes for 2 s.
#[test]
fn its_own_side_and_the_key_holder_pass() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    arena.owned_mut(field).unwrap().state.base.flags = 0x4890_004b;
    arena.actor_mut(field).unwrap().word_b4 = 2;
    arena.selector18_mut(field).unwrap().team_passes_178 = true;
    let friend = actor(&mut arena, [1000, 0, 0], 4);
    arena.actor_mut(friend).unwrap().word_b4 = 1;
    let mut host = Recorder::default();
    arena.selector18_passage(field, friend, &mut host).unwrap();
    let state = arena.selector18(field).unwrap();
    assert!(!state.on_176);
    assert_eq!(state.off_timer_15c, FORCE_FIELD_PASS_OFF);
    // An alien in state 3 within twice (the radius + 0x500) holds it.
    arena.switch_selector18(field, true, 0).unwrap();
    let alien = actor(&mut arena, [0, 0, 2 * (1000 + 0x500) - 1], 0x24);
    let state = arena.actor_mut(alien).unwrap();
    state.word_b4 = 0x12;
    state.word_c4 = 3;
    arena.selector18_passage(field, friend, &mut host).unwrap();
    assert!(arena.selector18(field).unwrap().on_176);
    arena.owned_mut(alien).unwrap().state.base.position_words = Some([0, 0, 2 * (1000 + 0x500)]);
    arena.selector18_passage(field, friend, &mut host).unwrap();
    assert!(!arena.selector18(field).unwrap().on_176);
    // An alien field lets aliens through for 5 s whatever stands near.
    arena.switch_selector18(field, true, 0).unwrap();
    arena.actor_mut(field).unwrap().word_b4 = 0x12;
    arena.actor_mut(friend).unwrap().word_b4 = 0x12;
    arena.selector18_passage(field, friend, &mut host).unwrap();
    assert_eq!(
        arena.selector18(field).unwrap().off_timer_15c,
        FORCE_FIELD_ALIEN_PASS_OFF
    );
    // The key: a player's actor owning the grant.
    arena.switch_selector18(field, true, 0).unwrap();
    let state = arena.selector18_mut(field).unwrap();
    state.team_passes_178 = false;
    state.key_grant_17a = 0x40;
    arena.selector18_passage(field, friend, &mut host).unwrap();
    assert!(arena.selector18(field).unwrap().on_176);
    host.player_actors.push(friend);
    arena.selector18_passage(field, friend, &mut host).unwrap();
    assert!(arena.selector18(field).unwrap().on_176);
    host.owned_grants.push(0x40);
    arena.selector18_passage(field, friend, &mut host).unwrap();
    let state = arena.selector18(field).unwrap();
    assert!(!state.on_176);
    assert_eq!(state.off_timer_15c, FORCE_FIELD_PASS_OFF);
}

/// 00476AF0..00476BE6: the fade climbs while shown and falls after; the
/// off timer counts down and switches the field back on at zero; a linked
/// field whose links are gone grants +7A and sends +180, then goes off
/// (with a hint) or away (without).
#[test]
fn the_update_fades_times_and_releases() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    arena.owned_mut(field).unwrap().state.base.flags = 0x4890_004b;
    let mut host = Recorder::default();
    arena.tick_selector18(field, 400_000, &mut host).unwrap();
    assert_eq!(arena.selector18(field).unwrap().fade_160, 400_000);
    arena.tick_selector18(field, 400_000, &mut host).unwrap();
    assert_eq!(arena.selector18(field).unwrap().fade_160, 800_000);
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(arena.selector18(field).unwrap().fade_160, FORCE_FIELD_FADE);
    arena.switch_selector18(field, false, 300_000).unwrap();
    arena.tick_selector18(field, 200_000, &mut host).unwrap();
    let state = arena.selector18(field).unwrap();
    assert_eq!((state.fade_160, state.off_timer_15c), (300_000, 100_000));
    assert!(!state.on_176);
    arena.tick_selector18(field, 100_000, &mut host).unwrap();
    let state = arena.selector18(field).unwrap();
    assert_eq!((state.fade_160, state.off_timer_15c), (200_000, 0));
    assert!(state.on_176);
    assert_eq!(state.clock_164, 1_100_001);
    // Released with a hint: the grant, the hint, then off for good.
    arena.actor_mut(field).unwrap().word_7a = 0x33;
    let state = arena.selector18_mut(field).unwrap();
    state.linked_175 = true;
    state.hint_180 = 0x1f;
    host.first_player = Some(PLAYER);
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(
        (host.grants.clone(), host.commands.clone()),
        (vec![0x33], vec![0x1f])
    );
    let state = arena.selector18(field).unwrap();
    assert!(!state.on_176 && !state.shown_177);
    assert_eq!(state.off_timer_15c, 0);
    // Without a hint it disposes itself.
    arena.selector18_mut(field).unwrap().hint_180 = 0;
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(host.grants, vec![0x33, 0x33]);
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == field));
}

/// 00476FE5..004771B8: near another team's field the player's vehicle
/// hears once that the area is secured; at its edge a secured field
/// switches off for 2 s, and an unsecured one warns and hints 0xF at most
/// every 10 s. An alien field says nothing.
#[test]
fn the_vehicle_hears_whether_the_area_is_secured() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    arena.owned_mut(field).unwrap().state.base.flags = 0x4890_004b;
    // Outside the edge band, not collidable: only the notices reach it.
    let vehicle = actor(&mut arena, [1000 + 0xa00 - 1, 0, 0], 0);
    let mut host = Recorder {
        vehicle: Some(vehicle),
        first_player: Some(PLAYER),
        ..Recorder::default()
    };
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(host.told, vec![FORCE_FIELD_SECURED_TEXT]);
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(host.told, vec![FORCE_FIELD_SECURED_TEXT]);
    // At the edge, secured: off for 2 s.
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([1000 + 0x2ff, 0, 0]);
    arena.tick_selector18(field, 1, &mut host).unwrap();
    let state = arena.selector18(field).unwrap();
    assert!(!state.on_176);
    assert_eq!(state.off_timer_15c, FORCE_FIELD_PASS_OFF);
    // Not secured: the warning and hint, again only after 10 s.
    arena.switch_selector18(field, true, 0).unwrap();
    let alien = actor(&mut arena, [0, 0, 0x100], 0x20);
    let state = arena.actor_mut(alien).unwrap();
    state.word_b4 = 0x12;
    state.word_c4 = 3;
    arena.selector18_mut(field).unwrap().clock_164 = 0;
    arena.tick_selector18(field, 10_000_001, &mut host).unwrap();
    assert_eq!(
        host.told,
        vec![FORCE_FIELD_SECURED_TEXT, FORCE_FIELD_NOT_SECURED_TEXT]
    );
    assert_eq!(host.commands, vec![FORCE_FIELD_NOT_SECURED_HINT]);
    assert_eq!(arena.selector18(field).unwrap().warned_170, 10_000_001);
    arena.tick_selector18(field, 10_000_000, &mut host).unwrap();
    assert_eq!(host.told.len(), 2);
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(host.told.len(), 3);
    assert!(arena.selector18(field).unwrap().on_176);
    // Inside the radius it switches off even unsecured.
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some([1000 - 1, 0, 0]);
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert!(!arena.selector18(field).unwrap().on_176);
    // An alien field tells nothing.
    arena.switch_selector18(field, true, 0).unwrap();
    arena.actor_mut(field).unwrap().word_b4 = 0x12;
    arena.selector18_mut(field).unwrap().secured_told_174 = false;
    arena.tick_selector18(field, 1, &mut host).unwrap();
    assert_eq!(host.told.len(), 3);
}

/// 004771D0's own part: a capture switches the field off; a release sets
/// it to come back on in 2 s. 004769C0 scores only a state-3 alien.
#[test]
fn capture_release_and_the_scan_score() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let field = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    arena.selector18_capture(field, true).unwrap();
    assert!(!arena.selector18(field).unwrap().on_176);
    arena.selector18_capture(field, false).unwrap();
    assert_eq!(arena.selector18(field).unwrap().off_timer_15c, 2_000_000);
    let alien = actor(&mut arena, [0; 3], 0x20);
    assert_eq!(arena.selector18_score(alien, 5).unwrap(), 0);
    let state = arena.actor_mut(alien).unwrap();
    state.word_b4 = 0x12;
    state.word_c4 = 3;
    assert_eq!(arena.selector18_score(alien, 5).unwrap(), 0x7fff_fffa);
}

/// 004768D0: off, clearing the bit everywhere, and the bit back to the
/// pool, which the next field takes.
#[test]
fn retirement_gives_the_bit_back() {
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let first = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    let marked = actor(&mut arena, [10, 0, 0], 4);
    arena.switch_selector18(first, true, 0).unwrap();
    assert_eq!(arena.actor(marked).unwrap().word_6c, 2);
    arena.retire_selector18(first).unwrap();
    assert_eq!(arena.actor(marked).unwrap().word_6c, 0);
    assert_eq!(arena.force_field_bits.used, 0);
    let second = field(&mut arena, &mut listeners, [0; 3], 1000, 1000);
    assert_eq!(arena.selector18(second).unwrap().bit_16c, 4);
}
