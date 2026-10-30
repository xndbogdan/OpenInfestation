//! Port-only control replay for the retail scenario lab (`--replay-controls
//! FILE`, `runtime_re/lab`): a retail recording's
//! first-player controls, turn, pitch, thrust, fire and grenade (+14..+27),
//! for each world tick, written over the block 0046A260 computed before the
//! vehicle's update reads it. A key replay has whole frames only; retail's
//! held times come from key times inside a tick, which this carries over.
//!
//! The file holds one JSON line per world tick, from the first:
//! `{"controls": "<five little-endian i32, as 40 hex digits>"}` (the lab's
//! `vehicle_update` `r[3]`).
//!
//! `--replay-rng FILE` likewise writes a retail run's RNG word 0055F18C over
//! the port's at each world tick's start, `{"rng": "xxxxxxxx"}` per tick, so
//! a tick that draws more or fewer numbers does not shift every later one.

use std::path::Path;
use std::sync::Mutex;

/// The ticks' controls, and how many world ticks have begun.
static REPLAY: Mutex<Option<(Vec<[i32; 5]>, usize)>> = Mutex::new(None);
/// The ticks' RNG words, and how many world ticks have begun.
static RNG: Mutex<Option<(Vec<u32>, usize)>> = Mutex::new(None);

pub fn load(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut ticks = Vec::new();
    for (index, line) in text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
    {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        ticks.push(
            value["controls"]
                .as_str()
                .and_then(parse)
                .ok_or_else(|| format!("line {}: controls is not 40 hex digits", index + 1))?,
        );
    }
    *REPLAY.lock().unwrap_or_else(|poison| poison.into_inner()) = Some((ticks, 0));
    Ok(())
}

pub fn load_rng(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut words = Vec::new();
    for (index, line) in text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
    {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        words.push(
            value["rng"]
                .as_str()
                .filter(|hex| hex.len() == 8)
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .ok_or_else(|| format!("line {}: rng is not 8 hex digits", index + 1))?,
        );
    }
    *RNG.lock().unwrap_or_else(|poison| poison.into_inner()) = Some((words, 0));
    Ok(())
}

fn parse(hex: &str) -> Option<[i32; 5]> {
    if hex.len() != 40 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0; 5];
    for (slot, value) in out.iter_mut().enumerate() {
        let mut bytes = [0u8; 4];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let at = slot * 8 + i * 2;
            *byte = u8::from_str_radix(&hex[at..at + 2], 16).ok()?;
        }
        *value = i32::from_le_bytes(bytes);
    }
    Some(out)
}

/// A world tick begins (00478F20).
pub fn tick() {
    if let Some((_, ticks)) = REPLAY
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_mut()
    {
        *ticks += 1;
    }
    if let Some((_, ticks)) = RNG
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_mut()
    {
        *ticks += 1;
    }
}

/// The recorded RNG word for this world tick's start, while the replay
/// covers it.
pub fn rng_word() -> Option<u32> {
    let replay = RNG.lock().unwrap_or_else(|poison| poison.into_inner());
    let (words, ticks) = replay.as_ref()?;
    words.get(ticks.checked_sub(1)?).copied()
}

/// This world tick's recorded turn, pitch, thrust, fire and grenade, while
/// the replay covers it.
pub fn current() -> Option<[i32; 5]> {
    let replay = REPLAY.lock().unwrap_or_else(|poison| poison.into_inner());
    let (controls, ticks) = replay.as_ref()?;
    controls.get(ticks.checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn controls_parse_as_the_lab_logs_them() {
        assert_eq!(
            parse("00000000b817feff000000000000000000000000"),
            Some([0, -125_000, 0, 0, 0])
        );
        assert_eq!(parse("00"), None);
    }
}
