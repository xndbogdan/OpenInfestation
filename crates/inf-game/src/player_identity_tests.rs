//! Focused checks for 00463550/00463D10: cache/flag custody, registry
//! operation order, fallback chains, the `%dup` initials default, and the
//! 00463D10 initials-into-net-buffer quirk.

use super::*;
use std::collections::HashMap;
use std::convert::Infallible;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyId {
    User,
    Machine,
}

#[derive(Clone, Debug)]
struct RecordingStore {
    user_open: bool,
    machine_open: bool,
    user: HashMap<&'static str, Vec<u8>>,
    machine: HashMap<&'static str, Vec<u8>>,
    log: Vec<String>,
}

impl RecordingStore {
    fn user_open() -> Self {
        Self {
            user_open: true,
            machine_open: false,
            user: HashMap::new(),
            machine: HashMap::new(),
            log: Vec::new(),
        }
    }

    fn user_closed() -> Self {
        Self {
            user_open: false,
            machine_open: false,
            user: HashMap::new(),
            machine: HashMap::new(),
            log: Vec::new(),
        }
    }
}

impl IdentityStore for RecordingStore {
    type Key = KeyId;
    type Error = Infallible;

    fn open_user_key(&mut self) -> Result<Option<Self::Key>, Self::Error> {
        self.log.push("open HKCU".into());
        Ok(self.user_open.then_some(KeyId::User))
    }

    fn open_machine_key(&mut self) -> Result<Option<Self::Key>, Self::Error> {
        self.log.push("open HKLM".into());
        Ok(self.machine_open.then_some(KeyId::Machine))
    }

    fn query_value(
        &mut self,
        key: &Self::Key,
        value: &'static str,
        capacity: u32,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.log.push(format!("query {key:?}.{value}"));
        let table = match key {
            KeyId::User => &self.user,
            KeyId::Machine => &self.machine,
        };
        // RegQueryValueExA fails (ERROR_MORE_DATA) when the value is longer
        // than the cap; the game then takes its fallback path.
        Ok(table
            .get(value)
            .filter(|bytes| bytes.len() <= capacity as usize)
            .cloned())
    }

    fn close_key(&mut self, key: Self::Key) {
        self.log.push(format!("close {key:?}"));
    }
}

#[test]
fn missing_user_key_returns_novice_and_leaves_the_flag_clear() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_closed();
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Novice");
    assert_eq!(loaded.initials, b"");
    assert!(!identity.loaded());
    assert_eq!(store.log, ["open HKCU"]);
    // The flag stays clear: the next read retries the registry.
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Novice");
    assert_eq!(store.log, ["open HKCU", "open HKCU"]);
}

#[test]
fn phase9_copies_hkcu_values_and_cached_reads_skip_the_store() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.user.insert("Player Name", b"Ripley".to_vec());
    store.user.insert("Player Initials", b"BG".to_vec());
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Ripley");
    assert_eq!(loaded.initials, b"BG");
    assert!(identity.loaded());
    assert_eq!(identity.name_cache(), b"Ripley");
    assert_eq!(identity.initials_cache(), b"BG");
    assert_eq!(
        store.log,
        [
            "open HKCU",
            "query User.Player Name",
            "query User.Player Initials",
            "close User",
        ]
    );
    // 0054EC59 is set: the cached read performs no registry operations.
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Ripley");
    assert_eq!(loaded.initials, b"BG");
    assert_eq!(store.log.len(), 4);
}

#[test]
fn phase9_empty_values_fall_back_to_jameson_and_dup_default() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.user.insert("Player Name", b"".to_vec());
    store.user.insert("Player Initials", b"".to_vec());
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Jameson");
    assert_eq!(loaded.initials, b"0up");
    assert_eq!(
        store.log,
        [
            "open HKCU",
            "query User.Player Name",
            "query User.Player Initials",
            "close User",
        ]
    );
}

#[test]
fn phase9_missing_values_attempt_the_machine_key_twice() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 3)
        .unwrap();
    assert_eq!(loaded.name, b"Jameson");
    assert_eq!(loaded.initials, b"3up");
    assert_eq!(
        store.log,
        [
            "open HKCU",
            "query User.Player Name",
            "open HKLM",
            "query User.Player Initials",
            "open HKLM",
            "close User",
        ]
    );
}

#[test]
fn phase9_machine_fallbacks_fill_name_and_initials() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.machine_open = true;
    store.machine.insert("Player Name", b"Legacy".to_vec());
    store.machine.insert("Player Initials", b"LL".to_vec());
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Legacy");
    assert_eq!(loaded.initials, b"LL");
    assert_eq!(
        store.log,
        [
            "open HKCU",
            "query User.Player Name",
            "open HKLM",
            "query Machine.Player Name",
            "query User.Player Initials",
            "query Machine.Player Initials",
            "close User",
            "close Machine",
        ]
    );
}

#[test]
fn phase9_reuses_the_machine_key_open_for_the_name_fallback() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.machine_open = true;
    store.machine.insert("Player Initials", b"LL".to_vec());
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Jameson");
    assert_eq!(loaded.initials, b"LL");
    assert_eq!(
        store.log,
        [
            "open HKCU",
            "query User.Player Name",
            "open HKLM",
            "query Machine.Player Name",
            "query User.Player Initials",
            "query Machine.Player Initials",
            "close User",
            "close Machine",
        ]
    );
}

#[test]
fn overlong_registry_values_fail_like_more_data() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.user.insert("Player Name", vec![b'x'; 200]);
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Jameson");
    assert!(store.log.contains(&"open HKLM".to_string()));
}

#[test]
fn phase11_net_name_chain_uses_the_name_cache_not_the_registry() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    // `Player Name` exists in the registry, but the 00463D10 fallback chain
    // reads the name cache, which a fresh phase-11 read has not filled.
    store.user.insert("Player Name", b"NameOnly".to_vec());
    let loaded = identity
        .read(IdentityRead::NetPlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Jameson");
    assert_eq!(loaded.initials, b"0up");
    assert_eq!(
        store.log,
        [
            "open HKCU",
            "query User.Net Player Name",
            "open HKLM",
            "query User.Player Initials",
            "open HKLM",
            "close User",
        ]
    );
}

#[test]
fn phase11_after_phase9_reads_the_shared_initials_but_no_net_name() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.user.insert("Player Name", b"Pilot".to_vec());
    store.user.insert("Player Initials", b"PL".to_vec());
    let _ = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    // 00463550 sets the shared loaded flag but never fills 0054EBC8, so the
    // cached phase-11 read returns the empty net-name cache.
    let loaded = identity
        .read(IdentityRead::NetPlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"");
    assert_eq!(loaded.initials, b"PL");
}

#[test]
fn phase11_hklm_initials_are_swallowed_by_the_net_name_buffer() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.machine_open = true;
    store.user.insert("Net Player Name", b"NetName".to_vec());
    store.machine.insert("Player Initials", b"MM".to_vec());
    let loaded = identity
        .read(IdentityRead::NetPlayerName, &mut store, 0)
        .unwrap();
    // The returned name was copied before the quirk ran.
    assert_eq!(loaded.name, b"NetName");
    // The HKLM initials never reached the initials cache; the %dup default
    // still applies.
    assert_eq!(loaded.initials, b"0up");
    // The quirk overwrote the net-name cache for later cached reads.
    assert_eq!(identity.net_name_cache(), b"MM");
}

#[test]
fn clear_loaded_forces_the_next_read_back_to_the_registry() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    store.user.insert("Player Name", b"First".to_vec());
    store.user.insert("Player Initials", b"F1".to_vec());
    let _ = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    identity.clear_loaded();
    assert!(!identity.loaded());
    store.user.insert("Player Name", b"Second".to_vec());
    store.user.insert("Player Initials", b"S2".to_vec());
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name, b"Second");
    assert_eq!(loaded.initials, b"S2");
}

#[test]
fn default_initials_formats_like_printf_d_up() {
    assert_eq!(default_initials(0), b"0up");
    assert_eq!(default_initials(1), b"1up");
    assert_eq!(default_initials(4), b"4up");
    assert_eq!(default_initials(12), b"12up");
}

#[test]
fn identity_read_tracks_the_constructor_phase_mapping() {
    assert_eq!(
        IdentityRead::for_phase(SessionPhase::Phase9),
        Some(IdentityRead::PlayerName)
    );
    assert_eq!(
        IdentityRead::for_phase(SessionPhase::Phase10),
        Some(IdentityRead::NetPlayerName)
    );
    assert_eq!(
        IdentityRead::for_phase(SessionPhase::Phase11),
        Some(IdentityRead::NetPlayerName)
    );
    assert_eq!(IdentityRead::for_phase(SessionPhase::Frontend), None);
}

#[test]
fn read_caps_match_the_constructor_buffers() {
    let mut identity = PlayerIdentity::default();
    let mut store = RecordingStore::user_open();
    let long = b"abcdefghijklmnopqrstuvwxyz0123456789"; // 36 bytes
    store.user.insert("Player Name", long.to_vec());
    store.user.insert("Player Initials", b"ABCDE".to_vec());
    let loaded = identity
        .read(IdentityRead::PlayerName, &mut store, 0)
        .unwrap();
    assert_eq!(loaded.name.len(), 30);
    assert_eq!(loaded.initials, b"ABCD");
}

#[test]
fn host_errors_propagate_without_caching() {
    #[derive(Debug)]
    struct Broken;
    impl IdentityStore for Broken {
        type Key = ();
        type Error = &'static str;
        fn open_user_key(&mut self) -> Result<Option<Self::Key>, Self::Error> {
            Err("disk fault")
        }
        fn open_machine_key(&mut self) -> Result<Option<Self::Key>, Self::Error> {
            unreachable!("the user open fails first")
        }
        fn query_value(
            &mut self,
            _key: &Self::Key,
            _value: &'static str,
            _capacity: u32,
        ) -> Result<Option<Vec<u8>>, Self::Error> {
            unreachable!("the user open fails first")
        }
        fn close_key(&mut self, _key: Self::Key) {}
    }
    let mut identity = PlayerIdentity::default();
    let error = identity
        .read(IdentityRead::PlayerName, &mut Broken, 0)
        .unwrap_err();
    assert_eq!(error, "disk fault");
    assert!(!identity.loaded());
}
