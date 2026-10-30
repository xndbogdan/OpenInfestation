//! A settings file the game cannot load, for the launcher (2026-10-28: a field a
//! newer build wrote stopped the game, and the launcher asked for the
//! game's folder). [`diagnose`] says whether the file
//! loads, can be repaired or must be reset; [`repair`] keeps what this build
//! reads and drops or resets the rest; [`reset`] sets the file aside, so the
//! game starts from the original settings. Both keep the old file beside it
//! ([`broken_copy`]). The game's own load stays exact: it never changes a
//! file by itself.
//!
//! A repair reads the file field by field over a fresh install's file: each
//! field this build knows keeps the saved value where the exact reader takes
//! it there, else its default; inside the fixes and the Vulkan settings each
//! key does the same. Fields this build does not know are dropped, a schema
//! from a newer build is read as this one's, the native words are made 34,
//! a word past its limit takes its authored default, and key bindings this
//! build cannot use reset to the originals. The result must then load
//! exactly.

use std::path::Path;

use serde_json::{Map, Value};

use super::*;

/// What the launcher can do about the settings file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsHealth {
    /// It loads, or there is none (the game starts from the originals).
    Fine,
    /// It does not load, and a repair would make it: what the repair changes.
    Repairable { problems: Vec<String> },
    /// Nothing can be read from it: only a reset helps.
    Unreadable { error: String },
}

/// Where a repair or reset keeps the file it replaces.
pub fn broken_copy(path: &Path) -> PathBuf {
    path.with_extension("json.broken")
}

/// Whether the settings file at `path` loads, can be repaired, or cannot be
/// read. Reads only.
pub fn diagnose(path: &Path, assets: &FrontendAssets) -> SettingsHealth {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return SettingsHealth::Fine,
        Err(error) => {
            return SettingsHealth::Unreadable {
                error: error.to_string(),
            }
        }
    };
    if loads(path, &bytes, assets).is_ok() {
        return SettingsHealth::Fine;
    }
    match repaired(path, &bytes, assets) {
        Ok((_, problems)) => SettingsHealth::Repairable { problems },
        Err(error) => SettingsHealth::Unreadable { error },
    }
}

/// Repairs the settings file at `path` in place, keeping the old one as its
/// [`broken_copy`]: what changed.
pub fn repair(path: &Path, assets: &FrontendAssets) -> Result<Vec<String>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let (file, problems) = repaired(path, &bytes, assets)?;
    fs::copy(path, broken_copy(path)).map_err(|error| error.to_string())?;
    FrontendSettings::defaults(path.to_path_buf(), assets)
        .and_then(|settings| settings.write_file(&file))
        .map_err(|error| error.to_string())?;
    Ok(problems)
}

/// Sets the settings file at `path` aside as its [`broken_copy`]: the game
/// starts from the original settings.
pub fn reset(path: &Path) -> Result<(), String> {
    fs::rename(path, broken_copy(path)).map_err(|error| error.to_string())
}

/// The game's own load of `bytes` as the file at `path`.
fn loads(
    path: &Path,
    bytes: &[u8],
    assets: &FrontendAssets,
) -> Result<FrontendSettings, FrontendSettingsError> {
    let mut settings = FrontendSettings::defaults(path.to_path_buf(), assets)?;
    settings.apply_saved(bytes)?;
    settings.validate_values()?;
    Ok(settings)
}

/// The repaired file and what the repair changed, or why nothing can be read.
fn repaired(
    path: &Path,
    bytes: &[u8],
    assets: &FrontendAssets,
) -> Result<(SettingsFile, Vec<String>), String> {
    let base = FrontendSettings::defaults(path.to_path_buf(), assets)
        .map_err(|error| error.to_string())?;
    let saved: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("not JSON ({error})"))?;
    let Value::Object(saved) = saved else {
        return Err("not a settings object".to_string());
    };
    let Ok(Value::Object(defaults)) = serde_json::to_value(base.default_file()) else {
        return Err("the default settings could not be built".to_string());
    };
    let mut problems = Vec::new();
    let mut candidate = defaults.clone();
    for (key, value) in saved {
        // Schema 5's output still folds into the display (`migrate`).
        if !defaults.contains_key(&key) && key != "output_presentation" {
            problems.push(format!("Removed `{key}`, which this build does not know."));
            continue;
        }
        let value = match (&key[..], value, defaults.get(&key)) {
            ("fixes" | "modern", Value::Object(inner), Some(Value::Object(fallback))) => {
                inner_keys(&key, inner, fallback, &candidate, &mut problems)
            }
            ("key_selections" | "button_actions", Value::Array(items), _) => {
                let kept = readable_items(&key, items.clone(), &candidate);
                if kept.len() < items.len() {
                    problems.push(format!(
                        "Dropped {} of `{key}` that could not be read.",
                        items.len() - kept.len()
                    ));
                }
                Value::Array(kept)
            }
            (_, value, _) => value,
        };
        let previous = candidate.insert(key.clone(), value);
        if !reads(&candidate) {
            match previous {
                Some(previous) => {
                    candidate.insert(key.clone(), previous);
                }
                None => {
                    candidate.remove(&key);
                }
            }
            problems.push(format!("`{key}` could not be read; it takes its default."));
        }
    }
    let mut file: SettingsFile = serde_json::from_value(Value::Object(candidate))
        .map_err(|error| format!("not a settings file ({error})"))?;
    if !(1..=SCHEMA_VERSION).contains(&file.schema) {
        problems.push(format!(
            "Schema {} is not one this build reads; it is read as {SCHEMA_VERSION}.",
            file.schema
        ));
        file.schema = SCHEMA_VERSION;
    }
    if file.native_words.len() != SETTINGS_WORDS {
        problems.push(format!(
            "It held {} native settings, not {SETTINGS_WORDS}; the rest take their defaults.",
            file.native_words.len()
        ));
        let held = file.native_words.len().min(SETTINGS_WORDS);
        file.native_words.truncate(SETTINGS_WORDS);
        file.native_words
            .extend_from_slice(&base.startup_defaults[held..]);
    }
    for (offset, most) in WORD_LIMITS {
        let index = usize::from(offset) / 4;
        let value = file.native_words[index];
        if value > most {
            let name = base
                .definitions
                .iter()
                .find(|definition| definition.byte_offset == offset)
                .map_or_else(
                    || format!("Setting +{offset:02X}"),
                    |definition| definition.name.clone(),
                );
            let default = base.startup_defaults[index];
            problems.push(format!("{name} was {value}; it is reset to {default}."));
            file.native_words[index] = default;
        }
    }
    let mut bindings = FrontendSettings::defaults(path.to_path_buf(), assets)
        .map_err(|error| error.to_string())?;
    if bindings
        .load_bindings(&file.key_selections, &file.button_actions)
        .is_err()
    {
        problems
            .push("The key bindings could not be used; they are the originals again.".to_string());
        file.key_selections.clear();
        file.button_actions.clear();
    }
    // Written in today's schema, as any save writes it.
    file.migrate();
    file.schema = SCHEMA_VERSION;
    let bytes = serde_json::to_vec(&file).map_err(|error| error.to_string())?;
    loads(path, &bytes, assets)
        .map_err(|error| format!("a repair would still not load ({error})"))?;
    if problems.is_empty() {
        problems.push("It is written again in this build's form.".to_string());
    }
    Ok((file, problems))
}

/// `key`'s object (the fixes or the Vulkan settings) over its defaults: keys
/// this build does not know dropped, each other key kept where it reads.
fn inner_keys(
    key: &str,
    inner: Map<String, Value>,
    fallback: &Map<String, Value>,
    candidate: &Map<String, Value>,
    problems: &mut Vec<String>,
) -> Value {
    let mut kept = fallback.clone();
    for (name, value) in inner {
        if !fallback.contains_key(&name) {
            problems.push(format!(
                "Removed `{key}.{name}`, which this build does not know."
            ));
            continue;
        }
        let previous = kept.insert(name.clone(), value);
        let mut trial = candidate.clone();
        trial.insert(key.to_string(), Value::Object(kept.clone()));
        if !reads(&trial) {
            if let Some(previous) = previous {
                kept.insert(name.clone(), previous);
            }
            problems.push(format!(
                "`{key}.{name}` could not be read; it takes its default."
            ));
        }
    }
    Value::Object(kept)
}

/// The items of `key`'s list that read on their own.
fn readable_items(key: &str, items: Vec<Value>, candidate: &Map<String, Value>) -> Vec<Value> {
    items
        .into_iter()
        .filter(|item| {
            let mut trial = candidate.clone();
            trial.insert(key.to_string(), Value::Array(vec![item.clone()]));
            reads(&trial)
        })
        .collect()
}

/// Whether the exact reader takes `candidate`.
fn reads(candidate: &Map<String, Value>) -> bool {
    serde_json::from_value::<SettingsFile>(Value::Object(candidate.clone())).is_ok()
}

#[cfg(test)]
#[path = "frontend_settings_repair_tests.rs"]
mod tests;
