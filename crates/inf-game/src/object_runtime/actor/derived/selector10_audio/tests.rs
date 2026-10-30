use super::super::selector10_tail::tests::{fixture, resident, sound};
use super::*;
#[test]
fn constructor_audio_carries_remain_unknown_and_resource_or_signed_status_bypass_is_lazy() {
    for (resource, status) in [(32, 3), (33, 1), (34, -1), (106, 0)] {
        let (mut arena, actor) = fixture();
        let mut sounds = sound();
        arena.owned_mut(actor).unwrap().state.base.resource_word = resource;
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        arena.actor_mut(actor).unwrap().word_c4 = status as u32;
        assert_eq!(arena.derived_actor(actor).unwrap().word_16c, None);
        assert_eq!(arena.derived_actor(actor).unwrap().word_170, None);
        arena
            .tick_selector10_persistent_audio(actor, &resident(), &mut sounds)
            .unwrap();
        assert_eq!(arena.derived_actor(actor).unwrap().word_16c, None);
        assert_eq!(arena.derived_actor(actor).unwrap().word_170, None);
        assert!(sounds.is_empty());
    }
}
#[test]
fn admitted_audio_creates_then_updates_the_same_real_process_record_without_class36_inputs() {
    let (mut arena, actor) = fixture();
    let resident = resident();
    let mut sounds = sound();
    arena.owned_mut(actor).unwrap().state.base.resource_word = 33;
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    arena
        .tick_selector10_persistent_audio(actor, &resident, &mut sounds)
        .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    let first = sounds.records().snapshot(reference).unwrap();
    assert_eq!(first.position, Some([384, 64, 640]));
    assert_eq!(first.volume, Some(0xaaaa));
    assert_eq!(first.pitch, Some(0xaaaa));
    assert_eq!(first.flags, Some(1));
    assert_eq!(sounds.pending_len(), 1);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([1, 2, 3]);
    arena
        .tick_selector10_persistent_audio(actor, &resident, &mut sounds)
        .unwrap();
    assert_eq!(
        arena.derived_actor(actor).unwrap().sound_168,
        Some(reference)
    );
    assert_eq!(
        sounds.records().snapshot(reference).unwrap().position,
        Some([1, 2, 3])
    );
    assert_eq!(sounds.pending_len(), 1);
}
#[test]
fn failed_constructor_retains_actor_audio_stores_and_linked_pending_record_without_publishing168() {
    let (mut arena, actor) = fixture();
    let mut sounds = sound();
    arena.owned_mut(actor).unwrap().state.base.resource_word = 106;
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    assert!(
        matches!(arena.tick_selector10_persistent_audio(actor,&resident(),&mut sounds),
        Err(Selector10PersistentAudioError::Object(ObjectError::ActorPositionUnknown(actual))) if actual==actor)
    );
    let state = arena.derived_actor(actor).unwrap();
    assert_eq!(state.word_16c, Some(0xaaaa));
    assert_eq!(state.word_170, Some(0xaaaa));
    assert_eq!(state.sound_168, None);
    assert_eq!(sounds.pending_len(), 1);
}
#[test]
fn record_update_error_preserves_actor_audio_stores_but_bypasses_record_gain_pitch_after_xyz_failure(
) {
    let (mut arena, actor) = fixture();
    let resident = resident();
    let mut sounds = sound();
    arena.owned_mut(actor).unwrap().state.base.resource_word = 34;
    arena.actor_mut(actor).unwrap().word_c4 = 2;
    arena
        .tick_selector10_persistent_audio(actor, &resident, &mut sounds)
        .unwrap();
    let reference = arena.derived_actor(actor).unwrap().sound_168.unwrap();
    sounds
        .records_mut()
        .update(
            reference,
            1,
            2,
            None::<fn() -> Result<[u16; 3], ObjectError>>,
        )
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    assert!(matches!(
        arena.tick_selector10_persistent_audio(actor, &resident, &mut sounds),
        Err(Selector10PersistentAudioError::Object(
            ObjectError::ActorPositionUnknown(_)
        ))
    ));
    let record = sounds.records().snapshot(reference).unwrap();
    assert_eq!(record.volume, Some(1));
    assert_eq!(record.pitch, Some(2));
    assert_eq!(arena.derived_actor(actor).unwrap().word_16c, Some(0xaaaa));
    assert_eq!(arena.derived_actor(actor).unwrap().word_170, Some(0xaaaa));
    assert_eq!(record.position, Some([384, 64, 640]));
    assert_eq!(
        arena.derived_actor(actor).unwrap().sound_168,
        Some(reference)
    );
}
