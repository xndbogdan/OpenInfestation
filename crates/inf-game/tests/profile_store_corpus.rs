//! The installed save's profiles, copied and deleted in a scratch copy of it:
//! the game writes `Infest.sav` itself, but a test changes no save it did
//! not make.

use inf_formats::{encode_save_records, SaveArchive};
use inf_game::profile_store::{ProfileStore, ProfileStoreError, PROFILE_LIMIT};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn installed_profiles_survive_copy_and_delete() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let retail = root.join("Infest.sav");
    if !retail.is_file() {
        eprintln!("retail Infest.sav is unavailable; skipping private save corpus");
        return;
    }
    let original = fs::read(&retail).unwrap();
    let canonical = encode_save_records(SaveArchive::parse(&original).unwrap().records()).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sidecar = root.join(format!("../.tmp/inf-profile-corpus-{nonce}/Infest.sav"));
    fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    fs::copy(&retail, &sidecar).unwrap();
    let mut store = ProfileStore::open(&sidecar).unwrap();
    assert_eq!(fs::read(&sidecar).unwrap(), original);
    let count = store.query(false).unwrap().count;
    if count == 0 {
        assert_eq!(fs::read(&retail).unwrap(), original);
        return;
    }
    if count == PROFILE_LIMIT {
        assert!(matches!(
            store.copy_profile(0),
            Err(ProfileStoreError::Full)
        ));
        assert_eq!(fs::read(&sidecar).unwrap(), original);
        assert_eq!(fs::read(&retail).unwrap(), original);
        return;
    }
    let source = store.load(0).unwrap().clone();
    store.copy_profile(0).unwrap();
    assert_eq!(store.query(false).unwrap().count, count + 1);
    let copied = store.load(count).unwrap();
    assert_eq!(copied.mission(), source.mission());
    assert_eq!(copied.player(), source.player());
    store.delete_profile(count).unwrap();
    assert_eq!(store.query(false).unwrap().count, count);
    // Native mutation rewrites redundant pairs; imported unused backups may
    // differ or be corrupt even though the first copy was valid. The installed
    // four-profile evidence happens to equal this canonical encoding exactly.
    assert_eq!(fs::read(&sidecar).unwrap(), canonical);
    assert_eq!(fs::read(&retail).unwrap(), original);
}
