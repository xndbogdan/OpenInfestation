use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FamilyTerrainProbeMissHost,
};
use crate::terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn subject(arena: &mut ObjectArena) -> ObjectId {
    let mut random_state = 0;
    let actor = arena
        .allocate_family429e20_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([100, 20, 300]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut random_state,
        )
        .unwrap();
    arena.family429e20_actor_mut(actor).unwrap().probe_234 = Some(Box::new(
        TerrainProbeAllocation::new(TerrainProbeCache::new(8)),
    ));
    actor
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = subject(&mut arena);
    (arena, actor)
}
fn decoded(arena: &ObjectArena, actor: ObjectId) -> &TerrainProbeCache {
    arena
        .family429e20_actor(actor)
        .unwrap()
        .probe_234
        .as_ref()
        .unwrap()
        .decoded_cache()
}

#[test]
fn hit_bypasses_callback_and_unknown_services() {
    let (mut arena, actor) = fixture();
    arena
        .family429e20_actor_mut(actor)
        .unwrap()
        .probe_234
        .as_mut()
        .unwrap()
        .decoded_cache_mut()
        .probe([64, 64], |_, _| Ok::<_, ()>(7))
        .unwrap();
    let before = decoded(&arena, actor).clone();
    let result = arena.probe_family_terrain_cache(actor, [65, 66], |_, _, _| {
        Err::<u32, _>("unowned terrain and collision must remain unread")
    });
    assert_eq!(result, Ok(7));
    assert_eq!(decoded(&arena, actor), &before);
}

#[test]
fn callback_sees_installed_owner_and_completion_uses_captured_cells_fresh_storage() {
    let (mut arena, actor) = fixture();
    let identity = Rc::downgrade(
        &arena
            .family429e20_actor(actor)
            .unwrap()
            .probe_234
            .as_ref()
            .unwrap()
            .identity,
    );
    let result = arena.probe_family_terrain_cache(actor, [64, 64], |arena, current, point| {
        assert_eq!((current, point), (actor, [64, 64]));
        let allocation = arena
            .family429e20_actor_mut(current)
            .unwrap()
            .probe_234
            .as_mut()
            .unwrap();
        assert!(Rc::ptr_eq(
            &identity.upgrade().unwrap(),
            &allocation.identity
        ));
        // Supplied in-place header/storage edit. It retains the actor's actual
        // allocation identity, unlike replacement of actor+234 itself.
        *allocation.decoded_cache_mut() = TerrainProbeCache::new(16);
        let cache = allocation.decoded_cache_mut();
        cache.word_04 = 90;
        cache.word_06 = 91;
        cache.words_00[4] = 2;
        Ok::<_, &'static str>(0x1234_abff)
    });
    assert_eq!(result, Ok(255));
    let current = decoded(&arena, actor);
    assert_eq!(
        (current.word_04(), current.word_06(), current.stride_words()),
        (90, 91, 4)
    );
    // Captured cells [1,1] -> fresh stride4 index4. BYTE255+1 spills into
    // the next nibble. Requantizing against origin90/91 would change this.
    assert_eq!(current.words()[4], 0x1002);
    assert_eq!(current.words()[2], 0);
}

#[test]
fn miss_error_retains_window_and_callback_writes_without_encoding() {
    let (mut arena, actor) = fixture();
    let result = arena.probe_family_terrain_cache(actor, [4096, 4096], |arena, current, point| {
        assert_eq!(point, [4096, 4096]);
        let cache = arena
            .family429e20_actor_mut(current)
            .unwrap()
            .probe_234
            .as_mut()
            .unwrap()
            .decoded_cache_mut();
        assert_eq!((cache.word_04(), cache.word_06()), (64, 64));
        cache.word_04 = 77;
        cache.words_00[1] = 0x1234;
        Err::<u32, _>("supplied callback stop")
    });
    assert_eq!(
        result,
        Err(FamilyTerrainProbeError::Cache(TerrainProbeError::Host(
            "supplied callback stop"
        )))
    );
    assert_eq!(
        (
            decoded(&arena, actor).word_04(),
            decoded(&arena, actor).word_06()
        ),
        (77, 64)
    );
    assert_eq!(decoded(&arena, actor).words()[0], 0);
    assert_eq!(decoded(&arena, actor).words()[1], 0x1234);
}

#[test]
fn decoded_equal_clone_has_fresh_identity_and_cannot_complete_old_allocation() {
    let (mut arena, actor) = fixture();
    let result = arena.probe_family_terrain_cache(actor, [4096, 4096], |arena, current, _| {
        let old = arena
            .family429e20_actor(current)
            .unwrap()
            .probe_234
            .as_ref()
            .unwrap();
        let replacement = old.clone();
        assert_eq!(old, &replacement);
        assert!(!Rc::ptr_eq(&old.identity, &replacement.identity));
        arena.family429e20_actor_mut(current).unwrap().probe_234 = Some(replacement);
        Ok::<_, &'static str>(1)
    });
    assert_eq!(
        result,
        Err(FamilyTerrainProbeError::ReplacedAllocation(actor))
    );
    assert_eq!(
        (
            decoded(&arena, actor).word_04(),
            decoded(&arena, actor).word_06()
        ),
        (64, 64)
    );
    assert!(decoded(&arena, actor).words().iter().all(|&word| word == 0));
}

#[test]
fn removed_cache_is_not_restored_and_token_does_not_retain_storage() {
    let (mut arena, actor) = fixture();
    let identity = Rc::downgrade(
        &arena
            .family429e20_actor(actor)
            .unwrap()
            .probe_234
            .as_ref()
            .unwrap()
            .identity,
    );
    let result = arena.probe_family_terrain_cache(actor, [64, 64], |arena, current, _| {
        arena.family429e20_actor_mut(current).unwrap().probe_234 = None;
        Ok::<_, &'static str>(1)
    });
    assert_eq!(result, Err(FamilyTerrainProbeError::MissingCache(actor)));
    assert!(arena.family429e20_actor(actor).unwrap().probe_234.is_none());
    assert!(identity.upgrade().is_none());
}

#[test]
fn released_actor_does_not_receive_completion_in_a_reused_slot() {
    let (mut arena, actor) = fixture();
    let mut replacement = None;
    let result = arena.probe_family_terrain_cache(actor, [64, 64], |arena, current, _| {
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: current,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        replacement = Some(subject(arena));
        Ok::<_, &'static str>(1)
    });
    assert_eq!(
        result,
        Err(FamilyTerrainProbeError::Object(ObjectError::UnknownObject(
            actor
        )))
    );
    let replacement = replacement.unwrap();
    assert_ne!(replacement, actor);
    assert!(decoded(&arena, replacement)
        .words()
        .iter()
        .all(|&word| word == 0));
}

#[test]
fn same_allocation_storage_fault_retains_prefix_and_never_substitutes_a_sample() {
    let (mut arena, actor) = fixture();
    let result = arena.probe_family_terrain_cache(actor, [64, 64], |arena, current, _| {
        let cache = arena
            .family429e20_actor_mut(current)
            .unwrap()
            .probe_234
            .as_mut()
            .unwrap()
            .decoded_cache_mut();
        cache.words_00 = vec![0x42].into_boxed_slice();
        cache.word_04 = 99;
        Ok::<_, &'static str>(1)
    });
    assert_eq!(
        result,
        Err(FamilyTerrainProbeError::Cache(
            TerrainProbeError::StorageRange {
                word_index: 2,
                words: 1,
            }
        ))
    );
    assert_eq!(decoded(&arena, actor).word_04(), 99);
    assert_eq!(decoded(&arena, actor).words(), &[0x42]);
}

struct MissHost {
    terrain: TerrainRuntime,
    expected_position: [u16; 3],
    calls: Vec<&'static str>,
}
impl FamilyTerrainProbeMissHost for MissHost {
    type Error = &'static str;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        assert!(arena.family429e20_actor(actor).unwrap().probe_234.is_some());
        self.calls.push("terrain");
        Ok(&self.terrain)
    }
    fn collide(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error> {
        assert!(arena.family429e20_actor(actor).unwrap().probe_234.is_some());
        assert_eq!(
            arena.object(actor).unwrap().base.position_words,
            Some(self.expected_position)
        );
        assert_ne!(
            arena.family428650_actor(actor).unwrap().word_22e & 0x8000,
            0
        );
        self.calls.push("collision");
        Err("required collision owner")
    }
}
#[test]
fn real_miss_kernel_can_reborrow_arena_and_retains_collision_failure_prefix() {
    let (mut arena, actor) = fixture();
    arena.derived_actor_mut(actor).unwrap().byte_160 = 1;
    let mut host = MissHost {
        terrain: native_probe_fixture_terrain(&serde_json::json!({
            "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
            "xz":[4096,4096],"heights":[2,2,2,2],"alternate":false,
        })),
        // 00450BD0 coarse lower-half sampling snaps X/Z to cell origin +64.
        expected_position: [4160, 64, 4160],
        calls: Vec::new(),
    };
    let result = arena.probe_family_terrain_cache(actor, [4096, 4096], |arena, current, point| {
        arena
            .probe_family_terrain_miss(current, point, &mut host)
            .map(u32::from)
    });
    assert!(matches!(
        result,
        Err(FamilyTerrainProbeError::Cache(TerrainProbeError::Host(
            crate::object_runtime::FamilyTerrainProbeMissError::Host {
                effect: "0047C1D0 collision",
                error: "required collision owner"
            }
        )))
    ));
    assert_eq!(host.calls, ["terrain", "collision"]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([4160, 64, 4160])
    );
    assert_ne!(
        arena.family428650_actor(actor).unwrap().word_22e & 0x8000,
        0
    );
    assert_eq!(
        (
            decoded(&arena, actor).word_04(),
            decoded(&arena, actor).word_06()
        ),
        (64, 64)
    );
    assert!(decoded(&arena, actor).words().iter().all(|&word| word == 0));
}
