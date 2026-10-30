//! Player identity reads 00463550/00463D10 and the cache contract they share
//! with the writers 004633D0/00463490.
//!
//! Native caches: player name 0054E930 (0x80), net player name 0054EBC8
//! (0x80), player initials 0054EBC0 (8 bytes), and the loaded flag 0054EC59.
//! A read that opens the HKCU key fills the caches and sets the flag; cached
//! reads copy without touching the registry. The writers clear the flag
//! (they never update the caches), so the next read re-reads the registry. A
//! failed HKCU open does not set the flag, so every read retries; the
//! returned name is then the `Novice` start value and the initials are the
//! caller's (zeroed) buffer, i.e. empty.
//!
//! Registry access is a required host effect; the exact native operation
//! order, the shared HKLM handle reuse, and the 00463D10 initials-into-
//! net-buffer quirk are owned here. Keys and values:
//!
//! - HKCU `Software\Frontier Developments Ltd\Infestation`
//! - legacy HKLM `Software\Frontier Developments Ltd\Infestation\1.0`
//! - values `Player Name`, `Net Player Name`, `Player Initials`
//!
//! The initials default is `sprintf("%dup", DAT_0055F2F4)` (004636AE /
//! 00463E89): the standard CRT reads this as the `%d` conversion plus the
//! literal `up`. 0055F2F4 is the local slot: 1 from boot (retail's lab read
//! it at the first pass, 2026-10-21; 00493650 sets it to 1) and the slot the
//! transport gave in a network game (1..4), so the offline default is `1up`.
//! That value is written to the cache only; the registry value stays missing
//! until a real write.

use crate::name_entry::{LoadedIdentity, JAMESON};
use crate::session::SessionPhase;

/// 00539610, opened by RegOpenKeyExA on HKEY_CURRENT_USER.
pub const HKCU_IDENTITY_KEY: &str = "Software\\Frontier Developments Ltd\\Infestation";
/// 005396B4, the legacy key opened on HKEY_LOCAL_MACHINE when an HKCU query
/// fails. Note the `\1.0` suffix lives on the machine key, not the user key.
pub const HKLM_IDENTITY_KEY: &str = "Software\\Frontier Developments Ltd\\Infestation\\1.0";
/// 00539690.
pub const PLAYER_NAME_VALUE: &str = "Player Name";
/// 0053969C.
pub const NET_PLAYER_NAME_VALUE: &str = "Net Player Name";
/// 00539680.
pub const PLAYER_INITIALS_VALUE: &str = "Player Initials";
/// 005396E8, the `_strncpy` start value for the returned name.
pub const NOVICE_PLAYER_NAME: &[u8] = b"Novice";
/// RegQueryValueExA byte cap for the name caches 0054E930/0054EBC8.
pub const NAME_CACHE_CAPACITY: u32 = 0x80;
/// RegQueryValueExA byte cap for the initials cache 0054EBC0.
pub const INITIALS_CACHE_CAPACITY: u32 = 8;
/// 00459060 passes name cap 31 and initials cap 5; the final
/// `dst[cap - 1] = 0` of `_strncpy` leaves 30/4 usable bytes, matching
/// [`LoadedIdentity`]'s constructor caps.
pub const NAME_READ_CAPACITY: usize = 30;
pub const INITIALS_READ_CAPACITY: usize = 4;

/// `sprintf("%dup", DAT_0055F2F4)`: `%d` plus the literal `up`. The index is
/// the current network player slot and zero offline.
pub fn default_initials(player_index: u32) -> Vec<u8> {
    let mut bytes = (player_index as i32).to_string().into_bytes();
    bytes.extend_from_slice(b"up");
    bytes
}

/// The read selected by the requested session phase at 00459060: phase 9
/// runs 00463550, phases 10/11 run 00463D10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityRead {
    /// `Player Name` + `Player Initials`.
    PlayerName,
    /// `Net Player Name` + `Player Initials`.
    NetPlayerName,
}

impl IdentityRead {
    pub const fn for_phase(phase: SessionPhase) -> Option<Self> {
        match phase {
            SessionPhase::Phase9 => Some(Self::PlayerName),
            SessionPhase::Phase10 | SessionPhase::Phase11 => Some(Self::NetPlayerName),
            _ => None,
        }
    }
}

/// Required registry host. `Ok(None)` is a failed open or query (an LSTATUS
/// miss), `Ok(Some)` a success, and `Err` a host failure the game has no
/// equivalent for. The owner drives the native open/query/close order and the
/// shared machine-key handle; the host never decides the sequence.
pub trait IdentityStore {
    type Key;
    type Error;

    /// RegOpenKeyExA on HKCU `HKCU_IDENTITY_KEY` with 0xF003F access.
    fn open_user_key(&mut self) -> Result<Option<Self::Key>, Self::Error>;
    /// RegOpenKeyExA on HKLM `HKLM_IDENTITY_KEY`.
    fn open_machine_key(&mut self) -> Result<Option<Self::Key>, Self::Error>;
    /// RegQueryValueExA with `capacity` as the native byte cap. A value
    /// longer than the cap fails the query (ERROR_MORE_DATA) instead of
    /// truncating; a missing value fails; an existing empty value succeeds
    /// with no bytes.
    fn query_value(
        &mut self,
        key: &Self::Key,
        value: &'static str,
        capacity: u32,
    ) -> Result<Option<Vec<u8>>, Self::Error>;
    /// RegCloseKey; the game ignores its result.
    fn close_key(&mut self, key: Self::Key);
}

/// The native cache trio plus the loaded flag. The writers 004633D0/
/// 00463490 clear the flag; see [`PlayerIdentity::clear_loaded`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerIdentity {
    /// 0054E930. Empty until a read fills it; 00463D10 only reads it.
    name_cache: Vec<u8>,
    /// 0054EBC8. Filled only by 00463D10 (or by its swallowed HKLM initials
    /// query); a phase-9 read never writes it.
    net_name_cache: Vec<u8>,
    /// 0054EBC0.
    initials_cache: Vec<u8>,
    /// 0054EC59.
    loaded: bool,
}

impl PlayerIdentity {
    pub fn name_cache(&self) -> &[u8] {
        &self.name_cache
    }
    pub fn net_name_cache(&self) -> &[u8] {
        &self.net_name_cache
    }
    pub fn initials_cache(&self) -> &[u8] {
        &self.initials_cache
    }
    /// DAT_0054EC59: set only by a read that opened the HKCU key.
    pub fn loaded(&self) -> bool {
        self.loaded
    }
    /// Writer tail 004633D0/00463490 (`DAT_0054ec59 = 0`): the writers do
    /// not update the caches, so the next read re-reads the registry.
    pub fn clear_loaded(&mut self) {
        self.loaded = false;
    }

    /// One 00463550 (phase 9) or 00463D10 (phases 10/11) call. `player_index`
    /// is 0055F2F4 for the `%dup` initials default: the current network
    /// player slot, zero offline.
    pub fn read<S: IdentityStore>(
        &mut self,
        kind: IdentityRead,
        store: &mut S,
        player_index: u32,
    ) -> Result<LoadedIdentity, S::Error> {
        match kind {
            IdentityRead::PlayerName => self.read_player_name(store, player_index),
            IdentityRead::NetPlayerName => self.read_net_player_name(store, player_index),
        }
    }

    /// 00463550: `Player Name` then `Player Initials`, both with the legacy
    /// HKLM fallback. The HKLM initials query writes the initials cache
    /// (0054EBC0) directly.
    fn read_player_name<S: IdentityStore>(
        &mut self,
        store: &mut S,
        player_index: u32,
    ) -> Result<LoadedIdentity, S::Error> {
        if self.loaded {
            return Ok(identity_copy(&self.name_cache, &self.initials_cache));
        }
        let Some(user_key) = store.open_user_key()? else {
            // The name keeps its `Novice` start value, the initials keep the
            // caller's zeroed buffer, and the flag stays clear: the next read
            // retries the registry.
            return Ok(LoadedIdentity {
                name: NOVICE_PLAYER_NAME.to_vec(),
                initials: Vec::new(),
            });
        };
        let mut machine_key = None;
        let name = match store.query_value(&user_key, PLAYER_NAME_VALUE, NAME_CACHE_CAPACITY)? {
            Some(value) => value,
            None => {
                machine_key = store.open_machine_key()?;
                match machine_key.as_ref() {
                    Some(machine) => store
                        .query_value(machine, PLAYER_NAME_VALUE, NAME_CACHE_CAPACITY)?
                        .unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        };
        let name = if name.is_empty() {
            JAMESON.to_vec()
        } else {
            name
        };
        self.name_cache = cache_capped(name, NAME_CACHE_CAPACITY as usize);
        let name = identity_name(&self.name_cache);
        let initials =
            match store.query_value(&user_key, PLAYER_INITIALS_VALUE, INITIALS_CACHE_CAPACITY)? {
                Some(value) => value,
                None => {
                    // The machine key may already be open from the name fallback;
                    // otherwise this is a second, independent open attempt.
                    if machine_key.is_none() {
                        machine_key = store.open_machine_key()?;
                    }
                    match machine_key.as_ref() {
                        Some(machine) => store
                            .query_value(machine, PLAYER_INITIALS_VALUE, INITIALS_CACHE_CAPACITY)?
                            .unwrap_or_default(),
                        None => Vec::new(),
                    }
                }
            };
        let initials = if initials.is_empty() {
            default_initials(player_index)
        } else {
            initials
        };
        self.initials_cache = cache_capped(initials, INITIALS_CACHE_CAPACITY as usize);
        self.loaded = true;
        store.close_key(user_key);
        if let Some(machine) = machine_key {
            store.close_key(machine);
        }
        Ok(LoadedIdentity {
            name,
            initials: identity_initials(&self.initials_cache),
        })
    }

    /// 00463D10: `Net Player Name` then `Player Initials`. The empty net-name
    /// fallback copies the name **cache** (0054E930), then `Jameson`; it never
    /// queries `Player Name` from the registry. The HKLM initials query writes
    /// the net-name buffer 0054EBC8 instead of the initials cache, so its
    /// bytes are swallowed and the returned name was already copied.
    fn read_net_player_name<S: IdentityStore>(
        &mut self,
        store: &mut S,
        player_index: u32,
    ) -> Result<LoadedIdentity, S::Error> {
        if self.loaded {
            return Ok(identity_copy(&self.net_name_cache, &self.initials_cache));
        }
        let Some(user_key) = store.open_user_key()? else {
            return Ok(LoadedIdentity {
                name: NOVICE_PLAYER_NAME.to_vec(),
                initials: Vec::new(),
            });
        };
        let mut machine_key = None;
        let net_name =
            match store.query_value(&user_key, NET_PLAYER_NAME_VALUE, NAME_CACHE_CAPACITY)? {
                Some(value) => value,
                None => {
                    machine_key = store.open_machine_key()?;
                    match machine_key.as_ref() {
                        Some(machine) => store
                            .query_value(machine, NET_PLAYER_NAME_VALUE, NAME_CACHE_CAPACITY)?
                            .unwrap_or_default(),
                        None => Vec::new(),
                    }
                }
            };
        let net_name = if net_name.is_empty() {
            if self.name_cache.is_empty() {
                JAMESON.to_vec()
            } else {
                self.name_cache.clone()
            }
        } else {
            net_name
        };
        self.net_name_cache = cache_capped(net_name, NAME_CACHE_CAPACITY as usize);
        // The returned name is copied here, before the initials query can
        // corrupt the net-name cache.
        let name = identity_name(&self.net_name_cache);
        let initials =
            match store.query_value(&user_key, PLAYER_INITIALS_VALUE, INITIALS_CACHE_CAPACITY)? {
                Some(value) => value,
                None => {
                    if machine_key.is_none() {
                        machine_key = store.open_machine_key()?;
                    }
                    match machine_key.as_ref() {
                        // Native quirk: this query targets 0054EBC8, so the HKLM
                        // initials overwrite the net-name cache and never reach
                        // the initials cache.
                        Some(machine) => {
                            if let Some(swallowed) = store.query_value(
                                machine,
                                PLAYER_INITIALS_VALUE,
                                INITIALS_CACHE_CAPACITY,
                            )? {
                                self.net_name_cache =
                                    cache_capped(swallowed, NAME_CACHE_CAPACITY as usize);
                            }
                            Vec::new()
                        }
                        None => Vec::new(),
                    }
                }
            };
        let initials = if initials.is_empty() {
            default_initials(player_index)
        } else {
            initials
        };
        self.initials_cache = cache_capped(initials, INITIALS_CACHE_CAPACITY as usize);
        self.loaded = true;
        store.close_key(user_key);
        if let Some(machine) = machine_key {
            store.close_key(machine);
        }
        Ok(LoadedIdentity {
            name,
            initials: identity_initials(&self.initials_cache),
        })
    }
}

fn identity_copy(name: &[u8], initials: &[u8]) -> LoadedIdentity {
    LoadedIdentity {
        name: identity_name(name),
        initials: identity_initials(initials),
    }
}

fn identity_name(cache: &[u8]) -> Vec<u8> {
    cache[..cache.len().min(NAME_READ_CAPACITY)].to_vec()
}

fn identity_initials(cache: &[u8]) -> Vec<u8> {
    cache[..cache.len().min(INITIALS_READ_CAPACITY)].to_vec()
}

fn cache_capped(value: Vec<u8>, capacity: usize) -> Vec<u8> {
    if value.len() <= capacity {
        value
    } else {
        value[..capacity].to_vec()
    }
}

#[cfg(test)]
#[path = "player_identity_tests.rs"]
mod tests;
