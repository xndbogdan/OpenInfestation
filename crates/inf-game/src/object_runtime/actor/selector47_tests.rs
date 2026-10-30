//! The Siren against its disassembly (0047FA40, 0047FDC0, 0047FE10,
//! 0047FAA0, 0047FAB0 and 0047FAC0). Infest03's and Infest04's ten sirens
//! (`siren.js`) never sounded; the world tests match their construction.

use std::path::Path;

use inf_formats::{ByteSpan, InstanceField};

use super::*;
use crate::{
    object_runtime::{ActorRegistryConfig, ActorTerrainInputs, FamilyUpdateEffect},
    process_sound::{ProcessSoundRuntime, ProcessSoundVoiceRef, ProcessSoundVoiceTeardown},
    Installation, ResidencyRequest, ResidentRuntime,
};

const SINE: [u16; 4096] = [0; 4096];
const NULL: ActorHandle = ActorHandle::from_raw(0x1234);
const AT: [u16; 3] = [1000, 0, 1000];

fn resident() -> ResidentRuntime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    for level in [47, 48, 21] {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    }
    runtime
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: NULL,
    })
    .unwrap()
}

fn construction() -> ActorConstructionRequest<'static> {
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
            sine_quarter: &SINE,
        },
    }
}

/// A mutant-side Siren (+B4 = 0x12) at `AT` with 10 s of alarm, as the
/// recorded ones are after their commands and first update.
fn siren(arena: &mut ObjectArena) -> ObjectId {
    let siren = arena.allocate_selector47(construction()).unwrap();
    let base = &mut arena.owned_mut(siren).unwrap().state.base;
    base.position_words = Some(AT);
    base.word_28 = 220;
    arena.actor_mut(siren).unwrap().word_b4 = 0x12;
    let state = arena.selector47_mut(siren).unwrap();
    state.seconds_16c = Some(10);
    state.silenced_16d = Some(0);
    siren
}

/// The first player's vehicle (0046B030) on side `side` at `position`.
fn vehicle(arena: &mut ObjectArena, side: u32, position: [u16; 3]) -> ObjectId {
    let vehicle = arena.allocate_actor(construction()).unwrap();
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some(position);
    arena.actor_mut(vehicle).unwrap().word_b4 = side;
    let player = arena.add_world_player([0; 6]);
    let handle = arena.actor(vehicle).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    vehicle
}

struct NoVoices;
impl ProcessSoundVoiceTeardown for NoVoices {
    fn delete_voice(
        &mut self,
        _: ProcessSoundRef,
        _: ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        Ok(())
    }
}

/// Records every service the update reaches, in order.
struct Host<'r> {
    resident: &'r ResidentRuntime,
    sounds: ProcessSoundRuntime,
    attract: bool,
    rng: u32,
    events: Vec<String>,
}

impl<'r> Host<'r> {
    fn new(resident: &'r ResidentRuntime) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        Self {
            resident,
            sounds: ProcessSoundRuntime::new(executable).unwrap(),
            attract: false,
            rng: 0x1234_5678,
            events: Vec::new(),
        }
    }
}

impl FamilyUpdateHost for Host<'_> {
    type Error = String;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(NULL)
    }
    fn random(&mut self) -> Result<u32, String> {
        Ok(crate::name_entry::name_entry_rng_next(&mut self.rng))
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), String> {
        match effect {
            FamilyUpdateEffect::StatusTrail { .. } => {
                self.events.push("trail".into());
                Ok(())
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

impl Selector47Host for Host<'_> {
    fn status_load_factor(&self) -> Option<i32> {
        Some(100)
    }
    fn attract(&self) -> bool {
        self.attract
    }
    fn create_alarm(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
    ) -> Result<Option<ProcessSoundRef>, String> {
        self.events.push("create".into());
        self.sounds
            .create_persistent(self.resident, SIREN_SOUND as i16, FULL, FULL, 0, || {
                Ok::<_, std::convert::Infallible>(AT)
            })
            .map(Some)
            .map_err(|error| format!("{error:?}"))
    }
    fn update_alarm(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        volume: u32,
        pitch: u32,
    ) -> Result<(), String> {
        self.events.push(format!("update {volume:#x} {pitch:#x}"));
        Ok(())
    }
    fn release_alarm(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.events.push("release".into());
        self.sounds
            .delete_record(reference, &mut NoVoices)
            .map_err(|error| format!("{error:?}"))
    }
    fn alert(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        source: ActorHandle,
    ) -> Result<(), String> {
        self.events.push(format!("alert {:#x}", source.raw()));
        Ok(())
    }
    fn flash(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
    ) -> Result<(), String> {
        self.events.push(format!("flash {position:?} {radius:#x}"));
        Ok(())
    }
}

fn view(code: i32, payload: InstanceCommandPayload) -> InstanceCommandView {
    InstanceCommandView {
        target: InstanceCommandTarget::Constructor(47),
        command_offset: 0,
        code: InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: code,
        },
        proven_method: Some(0x0047_fdc0),
        payload_consumer: Some(0x0047_fdc0),
        payload,
    }
}

#[test]
fn case_2f_builds_a_quiet_siren_with_a_0xa00_radius() {
    let mut arena = arena();
    let siren = arena.allocate_selector47(construction()).unwrap();
    let state = arena.selector47(siren).unwrap();
    assert_eq!(
        (
            state.word_158,
            state.spin_15a,
            state.turn_15c,
            state.word_15e,
            state.alarm_160,
            state.radius_164,
            state.dying_168,
            state.seconds_16c,
            state.silenced_16d,
        ),
        (0, 0, 0, None, 0, 0xa00, 0, None, None)
    );
    // The world load's FreshHeapZeroExtendedCarryApproximation starts the
    // silence byte at 0, as world 8's retail Siren read it
    // (siren-world8.json).
    let mut loaded = self::arena();
    loaded
        .select_actor_allocation_policy(
            crate::object_runtime::ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
        )
        .unwrap();
    let siren = loaded.allocate_selector47(construction()).unwrap();
    assert_eq!(loaded.selector47(siren).unwrap().silenced_16d, Some(0));
}

/// 0047FDC0: code 21 accepts without a read, 22 copies the seconds BYTE, 23
/// sign-extends the radius WORD and 24 refuses.
#[test]
fn commands_set_the_seconds_and_a_signed_radius() {
    let runtime = resident();
    let context = || ActorCommandContext {
        runtime: &runtime,
        first_player_grants: None,
        terrain: None,
    };
    let mut arena = arena();
    let siren = arena.allocate_selector47(construction()).unwrap();
    let before = arena.selector47(siren).unwrap().clone();
    assert!(arena
        .apply_selector47_command(
            siren,
            &view(21, InstanceCommandPayload::NoPayloadRead),
            context()
        )
        .unwrap());
    assert_eq!(arena.selector47(siren).unwrap(), &before);
    let seconds = InstanceCommandPayload::Selector47Seconds16c(InstanceField {
        span: ByteSpan { start: 4, end: 5 },
        value: 0xf6,
    });
    assert!(arena
        .apply_selector47_command(siren, &view(22, seconds), context())
        .unwrap());
    let radius = InstanceCommandPayload::Selector47Radius164(InstanceField {
        span: ByteSpan { start: 4, end: 6 },
        value: -4096,
    });
    assert!(arena
        .apply_selector47_command(siren, &view(23, radius), context())
        .unwrap());
    let state = arena.selector47(siren).unwrap();
    assert_eq!(
        (state.seconds_16c, state.radius_164),
        (Some(0xf6), 0xffff_f000)
    );
    assert!(!arena
        .apply_selector47_command(siren, &view(24, InstanceCommandPayload::Opaque), context())
        .unwrap());
}

/// 0047FE10: 0xB and 0xC silence it for 10 s; 0xE takes the argument or the
/// signed seconds; links reach 0040DE70.
#[test]
fn messages_silence_set_the_alarm_or_link() {
    let mut arena = arena();
    let siren = siren(&mut arena);
    for code in [0xb, 0xc] {
        arena.selector47_mut(siren).unwrap().silenced_16d = Some(0);
        arena.selector47_message(siren, code, 0).unwrap();
        let state = arena.selector47(siren).unwrap();
        assert_eq!((state.alarm_160, state.silenced_16d), (10_000_000, Some(1)));
    }
    arena.selector47_message(siren, 0xe, 123).unwrap();
    assert_eq!(arena.selector47(siren).unwrap().alarm_160, 123);
    arena.selector47_message(siren, 0xe, 0).unwrap();
    assert_eq!(arena.selector47(siren).unwrap().alarm_160, 10_000_000);
    arena.selector47_mut(siren).unwrap().seconds_16c = Some(0xf6);
    arena.selector47_message(siren, 0xe, 0).unwrap();
    assert_eq!(
        arena.selector47(siren).unwrap().alarm_160,
        (-10_000_000_i32) as u32
    );
    arena.selector47_mut(siren).unwrap().seconds_16c = None;
    assert!(matches!(
        arena.selector47_message(siren, 0xe, 0),
        Err(Selector47Error::UnwrittenSeconds(found)) if found == siren
    ));
    // 0xD and 0x15 reach the family, which ignores them; 7 links.
    let before = arena.selector47(siren).unwrap().clone();
    for code in [0xd, 0x15, 0xa] {
        arena.selector47_message(siren, code, 5).unwrap();
    }
    assert_eq!(arena.selector47(siren).unwrap(), &before);
    arena.selector47_message(siren, 7, 0).unwrap();
    let links = &arena.selector47(siren).unwrap().intermediate;
    assert_eq!((links.byte_14c, links.byte_14d), (1, 1));
}

/// 0047FAA0 reads the WORD at +158 plus twice the selector.
#[test]
fn model_values_read_the_words_from_158() {
    let mut arena = arena();
    let siren = siren(&mut arena);
    let state = arena.selector47_mut(siren).unwrap();
    state.spin_15a = 0x1234;
    state.turn_15c = 0x5678;
    state.alarm_160 = 0x0102_0304;
    state.radius_164 = 0x0a00;
    state.dying_168 = 0x1e_8480;
    let state = arena.selector47(siren).unwrap().clone();
    let values: Vec<_> = (0..12)
        .map(|selector| state.model_value(selector))
        .collect();
    assert_eq!(
        values,
        [
            Some(0),
            Some(0x1234),
            Some(0x5678),
            None,
            Some(0x0304),
            Some(0x0102),
            Some(0x0a00),
            Some(0),
            Some(0x8480),
            Some(0x1e),
            Some(10),
            None,
        ]
    );
}

/// 0047FAB0 starts 2 s of dying; the update counts it down with the status
/// visual and lowers the sound's pitch, then flashes, releases the sound and
/// disposes the Siren.
#[test]
fn its_death_winds_the_sound_down_then_flashes_and_disposes() {
    let resident = resident();
    let mut host = Host::new(&resident);
    let mut arena = arena();
    let siren = siren(&mut arena);
    let sound = host.create_alarm(&arena, siren).unwrap();
    arena.selector47_mut(siren).unwrap().intermediate.sound_154 = sound;
    arena.retire_selector47(siren).unwrap();
    assert_eq!(arena.selector47(siren).unwrap().dying_168, 2_000_000);
    host.events.clear();
    arena.tick_selector47(siren, 500_000, &mut host).unwrap();
    assert_eq!(arena.selector47(siren).unwrap().dying_168, 1_500_000);
    // ((1_500_000 / 1000) << 16) / 2000
    let pitch = host
        .events
        .iter()
        .find(|event| event.starts_with("update"))
        .unwrap();
    assert_eq!(pitch, "update 0x10000 0xc000");
    host.events.clear();
    arena.tick_selector47(siren, 1_500_000, &mut host).unwrap();
    assert_eq!(host.events, ["flash [1000, 0, 1000] 0xf00", "release"]);
    assert_eq!(
        arena.selector47(siren).unwrap().intermediate.sound_154,
        None
    );
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|node| node.object == siren));
}

/// 0047FB8C: a silenced Siren releases its sound and counts the silence
/// down; at the end the silence byte clears.
#[test]
fn silence_releases_the_sound_and_counts_down() {
    let resident = resident();
    let mut host = Host::new(&resident);
    let mut arena = arena();
    let siren = siren(&mut arena);
    let sound = host.create_alarm(&arena, siren).unwrap();
    let state = arena.selector47_mut(siren).unwrap();
    state.intermediate.sound_154 = sound;
    state.silenced_16d = Some(1);
    state.alarm_160 = 1_000_000;
    host.events.clear();
    arena.tick_selector47(siren, 400_000, &mut host).unwrap();
    assert_eq!(host.events, ["release"]);
    let state = arena.selector47(siren).unwrap();
    assert_eq!((state.alarm_160, state.silenced_16d), (600_000, Some(1)));
    arena.tick_selector47(siren, 600_000, &mut host).unwrap();
    let state = arena.selector47(siren).unwrap();
    assert_eq!((state.alarm_160, state.silenced_16d), (0, Some(0)));
}

/// 0047FBF7..0047FCC4: a running alarm counts down, spins up to 0xFFFF and
/// sounds (none in attract playback); a spent one spins down and releases.
/// The spin then turns the Siren.
#[test]
fn the_alarm_spins_up_and_sounds_then_spins_down() {
    let resident = resident();
    let mut host = Host::new(&resident);
    let mut arena = arena();
    let siren = siren(&mut arena);
    let state = arena.selector47_mut(siren).unwrap();
    state.alarm_160 = 20_000_000;
    state.spin_15a = 0xfff0;
    arena.tick_selector47(siren, 1000, &mut host).unwrap();
    let state = arena.selector47(siren).unwrap();
    // 1000 >> 5 = 31 more spin, capped; ((1000 >> 4) * 0xFFFF) >> 16 turn.
    assert_eq!(
        (state.alarm_160, state.spin_15a, state.turn_15c),
        (19_999_000, 0xffff, 61)
    );
    assert_eq!(host.events, ["create"]);
    assert!(state.intermediate.sound_154.is_some());
    arena.tick_selector47(siren, 1000, &mut host).unwrap();
    assert_eq!(host.events, ["create", "update 0x10000 0x10000"]);
    // Spent: the spin falls by delta >> 5, never below zero.
    arena.selector47_mut(siren).unwrap().alarm_160 = 0;
    arena.selector47_mut(siren).unwrap().spin_15a = 40;
    host.events.clear();
    arena.tick_selector47(siren, 1600, &mut host).unwrap();
    let state = arena.selector47(siren).unwrap();
    assert_eq!((state.spin_15a, state.intermediate.sound_154), (0, None));
    assert_eq!(host.events, ["release"]);
    // Attract playback starts no sound.
    let mut quiet = Host::new(&resident);
    quiet.attract = true;
    arena.selector47_mut(siren).unwrap().alarm_160 = 20_000_000;
    arena.tick_selector47(siren, 1000, &mut quiet).unwrap();
    assert!(quiet.events.is_empty());
}

/// 0047FCC7..0047FDB3: below its seconds, the first player's vehicle of
/// another side within the radius sets the alarm to its seconds plus
/// 0x7A121 and alerts the Siren's side to it.
#[test]
fn an_enemy_vehicle_within_the_radius_sets_off_the_alarm() {
    let resident = resident();
    for (side, offset, triggers) in [
        (0, 2559, true),
        (0, 2560, true),
        (0, 2561, false),
        (0x12, 100, false),
    ] {
        let mut host = Host::new(&resident);
        let mut arena = arena();
        let siren = siren(&mut arena);
        let vehicle = vehicle(&mut arena, side, [AT[0] + offset, 0, AT[2]]);
        arena.tick_selector47(siren, 1000, &mut host).unwrap();
        let alarm = arena.selector47(siren).unwrap().alarm_160;
        if triggers {
            let handle = arena.actor(vehicle).unwrap().handle;
            assert_eq!(alarm, 10_000_000 + 0x7_a121, "{side} {offset}");
            assert_eq!(host.events, [format!("alert {:#x}", handle.raw())]);
        } else {
            assert_eq!(alarm, 0, "{side} {offset}");
            assert!(host.events.is_empty());
        }
    }
    // An alarm at or above its seconds is not checked.
    let mut host = Host::new(&resident);
    let mut arena = arena();
    let siren = siren(&mut arena);
    vehicle(&mut arena, 0, AT);
    arena.selector47_mut(siren).unwrap().alarm_160 = 10_002_000;
    arena.tick_selector47(siren, 1000, &mut host).unwrap();
    assert!(!host.events.iter().any(|event| event.starts_with("alert")));
}

/// A linked Siren (+14D) whose last link is gone (+14C zero) triggers
/// wherever the vehicle is.
#[test]
fn a_lost_last_link_sets_off_the_alarm_anywhere() {
    let resident = resident();
    for (linked, links, triggers) in [(1, 0, true), (1, 1, false), (0, 0, false)] {
        let mut host = Host::new(&resident);
        let mut arena = arena();
        let siren = siren(&mut arena);
        vehicle(&mut arena, 0, [AT[0] + 30_000, 0, AT[2]]);
        let family = &mut arena.selector47_mut(siren).unwrap().intermediate;
        family.byte_14d = linked;
        family.byte_14c = links;
        arena.tick_selector47(siren, 1000, &mut host).unwrap();
        assert_eq!(
            arena.selector47(siren).unwrap().alarm_160 != 0,
            triggers,
            "{linked} {links}"
        );
    }
}

/// 0047FBD2: counts that differ stop the update; equal nonzero counts
/// alert with the process NULL handle.
#[test]
fn the_family_counts_gate_the_update() {
    let resident = resident();
    let mut host = Host::new(&resident);
    let mut arena = arena();
    let siren = siren(&mut arena);
    vehicle(&mut arena, 0, AT);
    let state = arena.selector47_mut(siren).unwrap();
    state.intermediate.word_13c = 2;
    state.intermediate.word_140 = 1;
    state.spin_15a = 0x8000;
    let before = arena.selector47(siren).unwrap().clone();
    arena.tick_selector47(siren, 1000, &mut host).unwrap();
    assert_eq!(arena.selector47(siren).unwrap(), &before);
    assert!(host.events.is_empty());
    arena.selector47_mut(siren).unwrap().intermediate.word_140 = 2;
    arena.tick_selector47(siren, 1000, &mut host).unwrap();
    assert_eq!(host.events[0], format!("alert {:#x}", NULL.raw()));
}

/// The first update reads the unwritten silence byte. A quiet Siren that
/// would not alert leaves it zero on both arms; any other first update
/// depends on the residue and stops.
#[test]
fn the_first_update_resolves_the_unwritten_silence_only_when_both_arms_agree() {
    let resident = resident();
    let mut host = Host::new(&resident);
    let mut arena = arena();
    let siren = siren(&mut arena);
    arena.selector47_mut(siren).unwrap().silenced_16d = None;
    let mut expected = arena.selector47(siren).unwrap().clone();
    arena.tick_selector47(siren, 1000, &mut host).unwrap();
    expected.silenced_16d = Some(0);
    assert_eq!(arena.selector47(siren).unwrap(), &expected);
    assert!(host.events.is_empty());
    // A vehicle in range: the alarm arm would alert, the silenced one not.
    let mut arena = self::arena();
    let siren = self::siren(&mut arena);
    arena.selector47_mut(siren).unwrap().silenced_16d = None;
    vehicle(&mut arena, 0, AT);
    assert!(matches!(
        arena.tick_selector47(siren, 1000, &mut host),
        Err(Selector47Error::UnwrittenSilence(found)) if found == siren
    ));
    // A running alarm counts down on either arm, but differently.
    let mut arena = self::arena();
    let siren = self::siren(&mut arena);
    let state = arena.selector47_mut(siren).unwrap();
    state.silenced_16d = None;
    state.alarm_160 = 5;
    assert!(matches!(
        arena.tick_selector47(siren, 1000, &mut host),
        Err(Selector47Error::UnwrittenSilence(_))
    ));
}
