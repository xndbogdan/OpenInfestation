//! Port-only event log for the retail scenario lab (`--event-log FILE`,
//! `runtime_re/lab`): one JSON line per world tick
//! and per vehicle update, and event lines, in the lab's own
//! `events.jsonl` schema, so `lab/compare.py` diffs a port run against
//! retail's.
//!
//! - `{"pass":F,"tick":N,"clock":C,"rng":"xxxxxxxx","ev":"tick","a":["delta"]}`
//!   at 00478F20's entry: `pass` is the one-based frame (`--input-replay`'s
//!   numbering), `tick` counts world ticks from 1, `clock` is the animation
//!   clock 005471C0 and `rng` the RNG word 0055F18C.
//! - `{"pass":F,"tick":N,"clock":C,"rng":..,"ev":"vehicle_update","a":["delta"],
//!   "r":[..],"rx":[..]}` around every 00416170 call: at entry (`r`) and
//!   exit (`rx`), the position +32..+36, velocity +8E..+92 and mode +1E8 as
//!   little-endian hex, and the control block's turn, pitch, thrust, fire and
//!   grenade (+14..+27): the driving player's, or the zeroed block of a
//!   vehicle the player left.
//!
//! - Event lines `{"pass":F,"tick":N,"clock":C,"ev":KIND,...}` with the
//!   values the lab's own line for KIND carries, without its pointers (the
//!   tick's animation clock; no RNG word, which moves within a tick):
//!   - `phase` at 004383B0 (the scheduler's per-frame transition poll): `r`
//!     and `rx`, the session's current, requested and pending words
//!     (+64C..+657) at entry and exit;
//!   - `hint` at 0046C760: the player command `id` and its `silent` byte;
//!   - `grant` at 004750C0, every call: the grant id, the low word of its raw
//!     `code` (the setter `set_live_grant`, and `inf_formats::grant_into`,
//!     which the pickup, selector12 and selector15 grants go through);
//!   - `damage` at 0047C6E0: the packet's strength and channel (`packet`),
//!     the target's resource word +30 (`target`) and the `source` handle;
//!   - `particle` at 00464FB0: `category`, `position` (six bytes, `??` where
//!     unknown), `source` handle and `flags`;
//!   - `rng` at 0048F060, every step of the LCG: the port's call site `at`
//!     (`file:line`) and the word after the step `rng`;
//!   - `spawn` at 00460310: the low word of `resource` (the slot it reads
//!     from DI), `position` (six bytes, `null` for a null pointer, `??`
//!     where unwritten) and `placement`;
//!   - `dispose` at 00479FA0: the object's `resource` word +30;
//!   - `object_draw` at 00479570: the caller's `eye` and `far`;
//!   - `intro` at 00443B10, selector 88's stage exit or entry: its `stage`
//!     +148, `entering`, `timer` +144 and `elapsed` +138, little-endian;
//!   - `scientist` at 004679F0, each selector-12 update: `r`, its position
//!     +32, team +B4, state +C4, model word +42 and velocity +8E, as a
//!     scenario's `funcs` reads them.
//!
//! The lab's lines carry native pointers too (`ecx`, `edx`, `ra`, `ret`),
//! which the port has no counterpart for: `compare.py --port` compares each
//! kind's values (README, "Retail against the port").

use std::cell::Cell;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use crate::object_runtime::{ObjectArena, ObjectId, PlayerControlBlock};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOG: Mutex<Option<(File, u64)>> = Mutex::new(None);
/// The animation clock of the last world tick, which event lines carry.
static CLOCK: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// Port option: set while the world draw draws the view shown apart
    /// from the game's draw (`world_scene::draw_world_views`), whose
    /// writes are all put back: its events never happen.
    static SILENT: Cell<bool> = const { Cell::new(false) };
}

/// Run `f` writing no line.
pub(crate) fn silenced<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            SILENT.set(self.0);
        }
    }
    let _restore = Restore(SILENT.replace(true));
    f()
}

/// Whether [`silenced`] holds the lines back now.
pub(crate) fn silent() -> bool {
    SILENT.get()
}

/// Start the log at `path`, its first line a header with this run's
/// command line.
pub fn open(path: &Path, args: &[String]) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "{}", serde_json::json!({ "port": 1, "args": args }))?;
    *LOG.lock().unwrap_or_else(|poison| poison.into_inner()) = Some((file, 0));
    ENABLED.store(true, Ordering::Relaxed);
    let _ = inf_formats::GRANT_OBSERVER.set(grant);
    Ok(())
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed) && !silent()
}

/// Each line is written whole and unbuffered, so a crash keeps the lines
/// before it.
fn line(count_tick: bool, body: impl FnOnce(u64) -> String) {
    if silent() {
        return;
    }
    let mut log = LOG.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some((file, tick)) = log.as_mut() else {
        return;
    };
    if count_tick {
        *tick += 1;
    }
    let line = with_pass(crate::crash_log::frame(), &body(*tick));
    let _ = file.write_all(line.as_bytes());
}

/// The frame goes first, as the lab's pass does (every body starts with "{").
fn with_pass(frame: u64, body: &str) -> String {
    format!("{{\"pass\":{frame},{}\n", &body[1..])
}

fn tick_body(tick: u64, clock: u32, rng: u32, delta: u32) -> String {
    format!(
        r#"{{"tick":{tick},"clock":{clock},"rng":"{rng:08x}","ev":"tick","a":["{delta:08x}"]}}"#
    )
}

fn vehicle_body(
    tick: u64,
    clock: u32,
    rng: u32,
    delta: u32,
    r: &[String; 4],
    rx: &[String; 4],
) -> String {
    format!(
        r#"{{"tick":{tick},"clock":{clock},"rng":"{rng:08x}","ev":"vehicle_update","a":["{delta:08x}"],"r":{},"rx":{}}}"#,
        serde_json::json!(r),
        serde_json::json!(rx)
    )
}

fn event_body(tick: u64, clock: u32, kind: &str, values: serde_json::Value) -> String {
    let mut line = serde_json::json!({ "tick": tick, "clock": clock, "ev": kind });
    if let (Some(line), serde_json::Value::Object(values)) = (line.as_object_mut(), values) {
        line.extend(values);
    }
    line.to_string()
}

fn event(kind: &str, values: serde_json::Value) {
    let clock = CLOCK.load(Ordering::Relaxed);
    line(false, |tick| event_body(tick, clock, kind, values));
}

fn hex(bytes: impl IntoIterator<Item = u8>) -> String {
    bytes.into_iter().map(|b| format!("{b:02x}")).collect()
}

/// The animation clock of the current world tick.
pub fn tick_clock() -> u32 {
    CLOCK.load(Ordering::Relaxed)
}

/// 00478F20's entry: the world tick.
pub fn tick(clock: u32, rng: u32, delta: u32) {
    if enabled() {
        CLOCK.store(clock, Ordering::Relaxed);
        line(true, |tick| tick_body(tick, clock, rng, delta));
    }
}

/// 004383B0, the per-frame transition poll: current, requested and pending
/// (session +64C..+657) at entry and exit.
pub fn phase(r: [u32; 3], rx: [u32; 3]) {
    if !enabled() {
        return;
    }
    let words = |w: [u32; 3]| hex(w.iter().flat_map(|v| v.to_le_bytes()));
    event(
        "phase",
        serde_json::json!({ "r": words(r), "rx": words(rx) }),
    );
}

/// 0046C760: a player command (hint) and its silent byte.
pub fn hint(id: u32, silent: u8) {
    if !enabled() {
        return;
    }
    event(
        "hint",
        serde_json::json!({ "id": format!("{id:08x}"), "silent": format!("{silent:02x}") }),
    );
}

/// 004750C0: a grant, by the low word of its raw DWORD, the one the setter
/// reads (callers leave residue in the high word).
pub fn grant(code: u32) {
    if !enabled() {
        return;
    }
    event(
        "grant",
        serde_json::json!({ "code": format!("{:04x}", code as u16) }),
    );
}

/// 0047C6E0: the packet's strength and channel, the target's resource word
/// and the source handle.
pub fn damage(strength: i16, channel: u8, target_resource: u16, source: u32) {
    if !enabled() {
        return;
    }
    let packet = hex(strength.to_le_bytes().into_iter().chain([channel]));
    event(
        "damage",
        serde_json::json!({
            "packet": packet,
            "target": hex(target_resource.to_le_bytes()),
            "source": format!("{source:08x}"),
        }),
    );
}

/// 0047FFF0: a sound record's position words, gain, pitch and the low byte
/// of its flag (one-shot and persistent records alike), with the H9 slot the
/// port resolved (retail logs its pointer, which `compare.py` skips).
pub fn sound(position: [u16; 3], gain: u32, pitch: u32, flag: u32, slot: i16) {
    if enabled() {
        event(
            "sound",
            serde_json::json!({
                "slot": slot,
                "position": hex(position.iter().flat_map(|w| w.to_le_bytes())),
                "gain": format!("{gain:08x}"),
                "pitch": format!("{pitch:08x}"),
                "flag": format!("{:02x}", flag as u8),
            }),
        );
    }
}

/// 0048F060: one step of the shared RNG, by the port's call site, and the
/// word after it (a step of another seed does not continue the tick's word).
pub fn rng(at: &std::panic::Location<'_>, word: u32) {
    if enabled() {
        event(
            "rng",
            serde_json::json!({
                "at": format!("{}:{}", at.file(), at.line()),
                "rng": format!("{word:08x}"),
            }),
        );
    }
}

/// 00464FB0: a particle's category, position bytes, source handle and flags.
pub fn particle(category: usize, position: [Option<u8>; 6], source: u32, flags: u32) {
    if enabled() {
        let position: String = position
            .iter()
            .map(|b| b.map_or_else(|| "??".to_string(), |b| format!("{b:02x}")))
            .collect();
        event(
            "particle",
            serde_json::json!({
                "category": format!("{category:08x}"),
                "position": position,
                "source": format!("{source:08x}"),
                "flags": format!("{flags:08x}"),
            }),
        );
    }
}

/// 00460310: the resource, the position bytes its pointer holds (`null` for
/// a null pointer, `??` for stack bytes nothing wrote) and the placement.
pub fn spawn(resource: u32, position: Option<[u16; 3]>, unwritten: bool, placement: u32) {
    if enabled() {
        let position = if unwritten {
            Some("??".repeat(6))
        } else {
            position.map(|words| hex(words.iter().flat_map(|w| w.to_le_bytes())))
        };
        event(
            "spawn",
            serde_json::json!({
                "resource": format!("{:04x}", resource as u16),
                "position": position,
                "placement": format!("{placement:08x}"),
            }),
        );
    }
}

/// 00479FA0: the disposed object's resource word +30.
pub fn dispose(resource: u16) {
    if enabled() {
        event(
            "dispose",
            serde_json::json!({ "resource": hex(resource.to_le_bytes()) }),
        );
    }
}

/// 00479570's entry: the caller's eye and far distance (the lab's
/// `object_draw` func reads them from its camera context +4/+8/+C and +68).
pub fn object_draw(eye: [i32; 3], far: i32) {
    if enabled() {
        event("object_draw", serde_json::json!({ "eye": eye, "far": far }));
    }
}

/// The port's ambience: a bed starting, a
/// recording taking over, a sweetener or a pulse beat playing, or the bed
/// stopping. A kind of its own, so the lab's comparisons leave it out.
pub fn ambience(bed: &str, recording: &str, what: &str) {
    if enabled() {
        event(
            "ambience",
            serde_json::json!({ "bed": bed, "recording": recording, "what": what }),
        );
    }
}

/// The port's ambience, each second a world plays: the buses' levels
/// (dBFS RMS) and the ambience's deepest dip under retail's effects.
pub fn ambience_levels(levels: &crate::platform_audio::BusLevels) {
    if enabled() {
        let round = |value: f32| (value * 10.0).round() / 10.0;
        event(
            "ambience",
            serde_json::json!({
                "what": "levels",
                "effects": round(levels.effects),
                "music": round(levels.music),
                "ambience": round(levels.ambience),
                "dip": (levels.dip * 100.0).round() / 100.0,
            }),
        );
    }
}

/// 00443B10: the intro controller's stage exit or entry.
pub fn intro(stage: u32, entering: bool, timer: i32, elapsed: i32) {
    if enabled() {
        event(
            "intro",
            serde_json::json!({
                "stage": hex(stage.to_le_bytes()),
                "entering": format!("{:02x}", u8::from(entering)),
                "timer": hex(timer.to_le_bytes()),
                "elapsed": hex(elapsed.to_le_bytes()),
            }),
        );
    }
}

/// The vehicle fields the lab reads at 00416170 (`r`/`rx`), or None when
/// the actor is not a vehicle with a position.
pub fn vehicle_fields(
    arena: &ObjectArena,
    vehicle: ObjectId,
    controls: &PlayerControlBlock,
) -> Option<[String; 4]> {
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let words = |words: [u16; 3]| {
        hex(&words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>())
    };
    let position = arena.object(vehicle).ok()?.base.position_words?;
    let velocity = arena.actor(vehicle).ok()?.words_8e_92;
    let mode = arena.selector2_actor(vehicle).ok()?.dwords_1e8_1f4[0];
    let held = [
        controls.turn_14,
        controls.pitch_18,
        controls.thrust_1c,
        controls.fire_20,
        controls.grenade_24,
    ];
    Some([
        words(position),
        words(velocity),
        hex(&mode.to_le_bytes()),
        hex(&held
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()),
    ])
}

/// 004679F0's entry, a scenario's funcs `scientist`: the selector-12
/// actor's position +32, team +B4, state +C4, model word +42 and velocity
/// +8E, as the lab reads them (`??` for an unknown position).
pub fn scientist(arena: &ObjectArena, actor: ObjectId) {
    if !enabled() {
        return;
    }
    let (Ok(object), Ok(state)) = (arena.object(actor), arena.actor(actor)) else {
        return;
    };
    let words = |words: [u16; 3]| hex(words.iter().flat_map(|word| word.to_le_bytes()));
    let r = [
        object
            .base
            .position_words
            .map_or_else(|| "?".repeat(12), words),
        hex((state.word_b4 as u16).to_le_bytes()),
        hex((state.word_c4 as u16).to_le_bytes()),
        hex(state.rotating.visual.model_word.to_le_bytes()),
        words(state.words_8e_92),
    ];
    event("scientist", serde_json::json!({ "r": r }));
}

/// 0043E6D0's entry, a scenario's funcs `small_ship`: the selector-35
/// actor's resource +30, position +32, descriptor +184, state +C4,
/// velocity +8E and escort +20C, as the lab reads them.
pub fn small_ship(arena: &ObjectArena, actor: ObjectId) {
    if !enabled() {
        return;
    }
    let (Ok(object), Ok(state), Ok(family)) = (
        arena.object(actor),
        arena.actor(actor),
        arena.family428650_actor(actor),
    ) else {
        return;
    };
    let words = |words: [u16; 3]| hex(words.iter().flat_map(|word| word.to_le_bytes()));
    let r = [
        hex(object.base.resource_word.to_le_bytes()),
        object
            .base
            .position_words
            .map_or_else(|| "?".repeat(12), words),
        hex(family.word_184.to_le_bytes()),
        hex((state.word_c4 as u16).to_le_bytes()),
        words(state.words_8e_92),
        hex(family.word_20c.to_le_bytes()),
    ];
    event("small_ship", serde_json::json!({ "r": r }));
}

/// 00416170, with the vehicle's entry and exit fields.
pub fn vehicle_update(clock: u32, rng: u32, delta: u32, r: &[String; 4], rx: &[String; 4]) {
    if enabled() {
        line(false, |tick| vehicle_body(tick, clock, rng, delta, r, rx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_follow_the_lab_schema() {
        let parse = |line: String| -> serde_json::Value {
            assert!(line.ends_with('\n'));
            serde_json::from_str(&line).unwrap()
        };
        let tick = parse(with_pass(7, &tick_body(1, 1380, 0x9cf4_13a8, 125_000)));
        assert_eq!(tick["pass"], 7);
        assert_eq!(tick["tick"], 1);
        assert_eq!(tick["clock"], 1380);
        assert_eq!(tick["rng"], "9cf413a8");
        assert_eq!(tick["ev"], "tick");
        assert_eq!(tick["a"][0], "0001e848");
        let fields = [
            "d99071002784".to_string(),
            "00004f000000".to_string(),
            "01000000".to_string(),
            "00000000b817feff000000000000000000000000".to_string(),
        ];
        let update = parse(with_pass(
            7,
            &vehicle_body(1, 1380, 0x9cf4_13a8, 125_000, &fields, &fields),
        ));
        assert_eq!(update["ev"], "vehicle_update");
        assert_eq!(update["tick"], 1);
        assert_eq!(update["r"][3], fields[3]);
        assert_eq!(update["rx"][0], fields[0]);
        let damage = parse(with_pass(
            7,
            &event_body(
                1,
                1380,
                "damage",
                serde_json::json!({ "packet": hex([0xb1, 0x65, 0x03]), "target": hex(9u16.to_le_bytes()) }),
            ),
        ));
        assert_eq!(damage["ev"], "damage");
        assert_eq!(damage["clock"], 1380);
        assert_eq!(damage["packet"], "b16503");
        assert_eq!(damage["target"], "0900");
    }

    /// The view shown's draw's events never happen: no line, and no tick counted,
    /// while it draws; the writers skip their work too.
    #[test]
    fn silenced_draws_write_no_line() {
        let path = std::env::temp_dir().join(format!(
            "inf-event-log-silenced-{}.jsonl",
            std::process::id()
        ));
        // The sink alone, without `open`'s ENABLED, which other tests'
        // writers would follow.
        *LOG.lock().unwrap_or_else(|poison| poison.into_inner()) =
            Some((File::create(&path).unwrap(), 0));
        silenced(|| {
            assert!(silent() && !enabled());
            line(true, |tick| tick_body(tick, 1, 2, 3));
            silenced(|| {
                line(false, |tick| {
                    event_body(tick, 1, "dispose", serde_json::json!({}))
                })
            });
            assert!(silent());
        });
        assert!(!silent());
        line(true, |tick| tick_body(tick, 4, 5, 6));
        *LOG.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 1, "{text}");
        assert_eq!(
            (lines[0]["tick"].as_u64(), lines[0]["clock"].as_u64()),
            (Some(1), Some(4))
        );
    }
}
