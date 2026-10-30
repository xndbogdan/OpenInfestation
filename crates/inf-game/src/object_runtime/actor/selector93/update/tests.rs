use super::*;
use crate::{
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        ObjectClass, ObjectClassKind,
    },
    terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime},
};
use inf_formats::{
    native_basis::NativeBasis, parse_frontend_inverse_sine_table, parse_frontend_sine_table,
    FrontendExecutable, FRONTEND_INVERSE_SINE_TABLE_ADDRESS, FRONTEND_SINE_TABLE_ADDRESS,
};
use serde_json::json;
use std::{path::Path, sync::OnceLock};

const DEAD_SINE: [u16; 4096] = [0; 4096];
const UP: [f32; 3] = [0.0, 1.0, 0.0];

struct Tables {
    math: NativeBasisMath,
    sine: [u16; 4096],
    inverse: [i16; 1024],
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let bytes =
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
                .expect("selector93 source tables require the original local executable");
        let executable = FrontendExecutable::parse(&bytes).unwrap();
        let sine = parse_frontend_sine_table(executable.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values;
        let inverse = parse_frontend_inverse_sine_table(
            executable.data(),
            FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
        )
        .unwrap()
        .values;
        Tables {
            math: NativeBasisMath::new(sine),
            sine,
            inverse,
        }
    })
}

fn request(microseconds: u32, up: Option<&[f32; 3]>) -> Selector93TickRequest<'_> {
    let tables = tables();
    Selector93TickRequest {
        microseconds,
        settings_7c: 0,
        math: &tables.math,
        inverse_sine: &tables.inverse,
        sine_quarter: &tables.sine,
        up,
    }
}

fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 17,
            world_flags: None,
            sine_quarter: &DEAD_SINE,
        },
    }
}

fn fixture(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena.allocate_selector93(allocation(position)).unwrap();
    (arena, actor)
}

fn cached_target(arena: &mut ObjectArena, actor: ObjectId, focus: [u16; 3]) {
    let referent = arena.allocate_actor(allocation(Some(focus))).unwrap();
    let target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(referent).unwrap().handle,
        ))
        .unwrap();
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: referent,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    //Retained genuine type2 cache is a declared ordinary live-state fixture.
    //No public raw setter, native pointer identity or writer execution claim.
    let target =
        arena.import_fixture_target(crate::object_runtime::NativeTarget::ActorFollow(target));
    arena.selector93_mut(actor).unwrap().target_148 = Some(target);
}

fn arena_owners(arena: &ObjectArena) -> String {
    format!(
        "{:?}{:?}",
        (
            &arena.identity,
            &arena.slots,
            &arena.free,
            &arena.live_count,
            &arena.lists,
            &arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations
        ),
        (
            &arena.actor_handles,
            &arena.actor_allocation,
            &arena.actor_payloads,
            &arena.constructor_allocations,
            &arena.selector60_rows,
            &arena.players,
            &arena.world_load
        )
    )
}

fn patch_terrain(xz: [u16; 2], height: i8) -> TerrainRuntime {
    native_probe_fixture_terrain(&json!({
        "spacing":128,"height_shift":4,"plane_base":0,"plane_shift":9,
        "xz":xz,"heights":[height,height,height,height],"alternate":false,
    }))
}

#[derive(Clone, Copy)]
enum Effect {
    None,
    Position([u16; 3]),
    PositionVelocity([u16; 3], [u16; 3]),
    ForgetPosition,
}

struct Terrain<'a> {
    banks: [&'a TerrainRuntime; 2],
    effects: [Effect; 2],
    fail_at: Option<usize>,
    calls: Vec<Option<[u16; 3]>>,
}

impl<'a> Terrain<'a> {
    fn new(terrain: &'a TerrainRuntime) -> Self {
        Self {
            banks: [terrain; 2],
            effects: [Effect::None; 2],
            fail_at: None,
            calls: Vec::new(),
        }
    }
}

impl ActorTerrainHost for Terrain<'_> {
    type Error = &'static str;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        let index = self.calls.len();
        self.calls
            .push(arena.object(actor).unwrap().base.position_words);
        if self.fail_at == Some(index) {
            return Err("required H12/0");
        }
        match self.effects[index] {
            Effect::None => {}
            Effect::Position(words) => {
                arena.copy_actor_position(actor, words).unwrap();
            }
            Effect::PositionVelocity(position, velocity) => {
                arena.copy_actor_position(actor, position).unwrap();
                arena.actor_mut(actor).unwrap().words_8e_92 = velocity;
            }
            Effect::ForgetPosition => {
                arena.owned_mut(actor).unwrap().state.base.position_words = None;
            }
        }
        Ok(self.banks[index])
    }
}

#[test]
fn one_actor_can_select_self_and_near_bypasses_required_terrain_and_up() {
    let (mut arena, actor) = fixture(Some([0x8000, 0xffff, 9]));
    let handle = arena.actor(actor).unwrap().handle;
    arena.resolve_actor_handle(handle).unwrap();
    let before = arena_owners(&arena);
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    terrain.fail_at = Some(0);
    let mut rng = 1;
    arena
        .tick_selector93(actor, request(u32::MAX, None), &mut rng, &mut terrain)
        .unwrap();
    assert_eq!(rng, 0x0027_aae0);
    assert!(terrain.calls.is_empty());
    assert_eq!(arena_owners(&arena), before);
}

#[test]
fn selection_uses_the_whole_current_fifo_without_lifecycle_or_self_filter() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let first = arena
        .allocate_actor(allocation(Some([0, 1500, 4000])))
        .unwrap();
    let second = arena
        .allocate_actor(allocation(Some([0, 1500, 5000])))
        .unwrap();
    let selected = arena
        .allocate_actor(allocation(Some([0, 1500, 6000])))
        .unwrap();
    arena.actor_mut(selected).unwrap().word_c4 = 7;
    let others = [first, second, selected].map(|id| arena.object(id).unwrap().clone());
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    terrain.fail_at = Some(0);
    let mut rng = 0; //First draw lowWORD19;19%4 selects the fourth FIFO actor.
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Terrain("required H12/0"))
    );
    assert_eq!(rng, 0x0013_d573);
    let target = arena
        .actor_follow_target(arena.selector93(actor).unwrap().target_148.unwrap())
        .unwrap();
    assert_eq!(target.handle_04, arena.actor(selected).unwrap().handle);
    assert_eq!(target.focus_08, Some([0, 1500, 6000]));
    assert_eq!(target.velocity_1a, Some([0; 3]));
    for (id, before) in [first, second, selected].into_iter().zip(others) {
        assert_eq!(arena.object(id).unwrap(), &before);
    }
}

#[test]
fn stopped_construction_random_target_preserves_full_divisor_rng_and_has_no_allocation() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let first = arena.allocate_actor(allocation(Some([0; 3]))).unwrap();
    let second = arena.allocate_actor(allocation(Some([0; 3]))).unwrap();
    let stopped = arena.allocate_actor(allocation(None)).unwrap();
    arena.stop_object_construction(stopped).unwrap();
    let nodes = arena.actor_list().to_vec();
    assert_eq!(nodes, [actor, first, second, stopped]);
    let before = arena_owners(&arena);
    let allocations = arena.actor_service_heap().attempts().len();
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 0; // Native first draw 19, full count 4, selected index 3.

    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::StoppedConstructionTarget { object: stopped })
    );
    assert_eq!(rng, 0x0013_d573);
    assert_eq!(arena.actor_service_heap().attempts().len(), allocations);
    assert_eq!(arena.selector93(actor).unwrap().target_148, None);
    assert!(terrain.calls.is_empty());
    assert_eq!(arena_owners(&arena), before);
    assert_eq!(arena.actor_list(), nodes);
}

#[test]
fn stopped_disposal_random_target_preserves_rng_index_and_has_no_allocation() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let first = arena.allocate_actor(allocation(Some([0; 3]))).unwrap();
    let second = arena.allocate_actor(allocation(Some([0; 3]))).unwrap();
    let stopped = arena.allocate_actor(allocation(None)).unwrap();
    arena.dispose(stopped).unwrap();
    arena.owned_mut(stopped).unwrap().disposal_stop =
        Some(crate::object_runtime::ObjectDisposalStop {
            object: stopped,
            phase: crate::object_runtime::ObjectDisposalPhase::DeletingDestructor,
            reason: "supplied destructor stop".into(),
        });
    let nodes = arena.actor_list().to_vec();
    assert_eq!(nodes, [actor, first, second, stopped]);
    let before = arena_owners(&arena);
    let allocations = arena.actor_service_heap().attempts().len();
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 0; // Native lowWORD19, count4, selected index3.
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Object(
            ObjectError::StoppedDisposal {
                object: stopped,
                phase: crate::object_runtime::ObjectDisposalPhase::DeletingDestructor,
            }
        ))
    );
    assert_eq!(rng, 0x0013_d573);
    assert_eq!(arena.actor_service_heap().attempts().len(), allocations);
    assert_eq!(arena.selector93(actor).unwrap().target_148, None);
    assert!(terrain.calls.is_empty());
    assert_eq!(arena_owners(&arena), before);
    assert_eq!(arena.actor_list(), nodes);
}

#[test]
fn constructor_failure_retains_rng_and_cache_without_publishing_target() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let selected = arena.allocate_actor(allocation(None)).unwrap();
    let before = arena.object(actor).unwrap().clone();
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 0;
    assert_eq!(
        arena.tick_selector93(actor, request(0, Some(&UP)), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Object(
            ObjectError::ActorPositionUnknown(selected)
        ))
    );
    assert_eq!(rng, 0x0013_d573);
    assert_eq!(arena.object(actor).unwrap(), &before);
    assert!(terrain.calls.is_empty());
    assert_eq!(
        arena
            .resolve_actor_handle(arena.actor(selected).unwrap().handle)
            .unwrap(),
        Some(selected)
    );
}

#[test]
fn present_stale_cache_and_strict_distance_threshold_preserve_the_referent_policy() {
    let flat = patch_terrain([0, 0], 0);
    for (distance, far) in [(1999, false), (2000, true)] {
        let (mut arena, actor) = fixture(Some([0, 1500, 0]));
        cached_target(&mut arena, actor, [0, 1500, distance]);
        let target = arena.selector93(actor).unwrap().target_148;
        let mut terrain = Terrain::new(&flat);
        terrain.fail_at = Some(0);
        let mut rng = 0x7654_3210;
        let result = arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain);
        assert_eq!(rng, 0x7654_3210);
        if far {
            assert_eq!(
                result,
                Err(Selector93UpdateError::Terrain("required H12/0"))
            );
            assert_eq!(arena.selector93(actor).unwrap().target_148, target);
            assert_eq!(terrain.calls.len(), 1);
        } else {
            result.unwrap();
            assert!(arena.selector93(actor).unwrap().target_148.is_none());
            assert!(terrain.calls.is_empty());
        }
    }
}

#[test]
fn wrapped_word_near_delta_and_overflow_distance_follow_distinct_paths() {
    let flat = patch_terrain([0, 0], 0);
    let (mut arena, actor) = fixture(Some([65530, 1500, 0]));
    cached_target(&mut arena, actor, [5, 1500, 0]);
    let mut terrain = Terrain::new(&flat);
    terrain.fail_at = Some(0);
    let mut rng = 0;
    arena
        .tick_selector93(actor, request(0, None), &mut rng, &mut terrain)
        .unwrap();
    assert!(arena.selector93(actor).unwrap().target_148.is_none());
    assert!(terrain.calls.is_empty());

    let (mut arena, actor) = fixture(Some([0; 3]));
    cached_target(&mut arena, actor, [0x8000; 3]);
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Terrain("required H12/0"))
    );
    //Three signedWORD squares overflow the sign bit. The shared distance
    //owner recovers by shifting the unsigned sum before sqrt, so this is far.
    assert!(arena.selector93(actor).unwrap().target_148.is_some());
    assert_eq!(terrain.calls.len(), 1);
}

#[test]
fn unknown_self_position_errors_after_real_refresh_without_consuming_rng() {
    let (mut arena, actor) = fixture(None);
    let referent = arena
        .allocate_actor(allocation(Some([0, 1500, 4000])))
        .unwrap();
    arena.actor_mut(referent).unwrap().words_8e_92 = [1, 2, 3];
    arena.owned_mut(referent).unwrap().state.base.flags |= 0x0004_0000;
    let mut target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(referent).unwrap().handle,
        ))
        .unwrap();
    target.focus_14 = Some([9, 8, 7]);
    let target =
        arena.import_fixture_target(crate::object_runtime::NativeTarget::ActorFollow(target));
    arena.selector93_mut(actor).unwrap().target_148 = Some(target);
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 42;
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::MissingPosition {
            actor,
            address: 0x0044_025f
        })
    );
    let target = arena
        .actor_follow_target(arena.selector93(actor).unwrap().target_148.unwrap())
        .unwrap();
    assert_eq!(target.focus_08, Some([0, 1500, 4000]));
    assert_eq!(target.focus_14, Some([9, 8, 7]));
    assert_eq!(target.velocity_1a, Some([0xffff, 0xfffe, 0xfffd]));
    assert_eq!(rng, 42);
    assert!(terrain.calls.is_empty());
}

#[test]
fn missing_registry_or_cached_focus_stops_at_the_reached_owner() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    cached_target(&mut arena, actor, [0, 1500, 4000]);
    let target = arena.selector93(actor).unwrap().target_148;
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 19;
    let registry = arena.actor_handles.take();
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Object(
            ObjectError::MissingActorRegistry
        ))
    );
    assert_eq!(arena.selector93(actor).unwrap().target_148, target);
    arena.actor_handles = registry;
    let reference = arena.selector93(actor).unwrap().target_148.unwrap();
    let mut data = arena.actor_follow_target(reference).unwrap();
    data.focus_08 = None;
    arena
        .write_native_target(
            reference,
            crate::object_runtime::NativeTarget::ActorFollow(data),
        )
        .unwrap();
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::MissingTargetFocus(actor))
    );
    assert_eq!(rng, 19);
    assert!(terrain.calls.is_empty());
}

#[test]
fn first_sample_keeps_xyz_captured_before_mutating_acquisition() {
    let (mut arena, actor) = fixture(Some([0, 1000, 0]));
    cached_target(&mut arena, actor, [0, 1000, 4000]);
    let raised = patch_terrain([0, 0], 64);
    let flat = patch_terrain([1024, 1024], 0);
    let mut terrain = Terrain::new(&flat);
    terrain.banks[0] = &raised;
    terrain.effects[0] = Effect::Position([1024, 2000, 1024]);
    let mut rng = 0;
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::MissingUp)
    );
    //Captured Y1000-height1024=-24; correction324 adds to live Y2000.
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([1024, 2324, 1024])
    );
    assert_eq!(
        terrain.calls,
        [Some([0, 1000, 0]), Some([1024, 2000, 1024])]
    );
}

#[test]
fn second_sample_uses_pre_service_xyz_but_fresh_post_service_velocity() {
    let (mut arena, actor) = fixture(Some([0, 1000, 0]));
    cached_target(&mut arena, actor, [0, 1000, 4000]);
    let flat = patch_terrain([0, 0], 0);
    let raised = patch_terrain([256, 0], 64);
    let mut terrain = Terrain::new(&flat);
    terrain.banks[1] = &raised;
    terrain.effects = [
        Effect::Position([128, 1500, 0]),
        Effect::PositionVelocity([1024, 2100, 0], [128, 0, 0]),
    ];
    let mut rng = 0;
    assert_eq!(
        arena.tick_selector93(actor, request(262144, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::MissingUp)
    );
    //Second capturedY1500-height1024=476. Saved clearance476 produces724
    //atdelta>>3=32768, applied to liveY2100. Neither refreshed XYZ nor
    //pre-acquisition velocity may be substituted for the actual operands.
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([1024, 2824, 0])
    );
    assert_eq!(terrain.calls, [Some([0, 1000, 0]), Some([128, 1500, 0])]);
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [128, 0, 0]);
}

#[test]
fn both_terrain_failures_retain_target_refresh_and_no_premature_motion() {
    let flat = patch_terrain([0, 0], 0);
    for fail_at in [0, 1] {
        let (mut arena, actor) = fixture(Some([0, 1500, 0]));
        cached_target(&mut arena, actor, [0, 1500, 4000]);
        let stale_handle = arena
            .actor_follow_target(arena.selector93(actor).unwrap().target_148.unwrap())
            .unwrap()
            .handle_04;
        let old_registry = arena.actor_handles.clone();
        let mut expected_registry = old_registry.clone().unwrap();
        // 00482698 reaches 00462190: an exact stale slot caches its handle and NULL.
        assert_eq!(expected_registry.resolve(stale_handle), None);
        assert_eq!(expected_registry.cache().handle, stale_handle);
        assert_eq!(expected_registry.cache().object, None);
        let before = arena_owners(&arena);
        let mut terrain = Terrain::new(&flat);
        terrain.fail_at = Some(fail_at);
        let mut rng = 33;
        assert_eq!(
            arena.tick_selector93(actor, request(32, Some(&UP)), &mut rng, &mut terrain),
            Err(Selector93UpdateError::Terrain("required H12/0"))
        );
        assert_eq!(terrain.calls.len(), fail_at + 1);
        assert_eq!(
            format!("{:?}", arena.actor_handles.as_ref().unwrap()),
            format!("{expected_registry:?}")
        );
        // Compare every other owner against its input after verifying the whole
        // registry prefix above. This restores only the compared registry.
        arena.actor_handles = old_registry;
        assert_eq!(arena_owners(&arena), before);
        assert_eq!(rng, 33);
    }
}

#[test]
fn unknown_second_xyz_is_reached_after_first_sample_before_second_acquisition() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let initial_dirty = arena.object(actor).unwrap().base.dirty_word;
    cached_target(&mut arena, actor, [0, 1500, 4000]);
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    terrain.effects[0] = Effect::ForgetPosition;
    let mut rng = 0;
    assert_eq!(
        arena.tick_selector93(actor, request(32, Some(&UP)), &mut rng, &mut terrain),
        Err(Selector93UpdateError::MissingPosition {
            actor,
            address: 0x0044_02df
        })
    );
    assert_eq!(terrain.calls.len(), 1);
    assert_eq!(
        arena.actor(actor).unwrap().rotating.basis,
        NativeBasis::INITIAL
    );
    assert_eq!(arena.selector93(actor).unwrap().dword_144, 0);
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0; 3]);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, initial_dirty);
}

#[test]
fn signed_clearance_thresholds_use_saved_clearance_after_the_first_y_write() {
    let flat = patch_terrain([0, 0], 0);
    for (clearance, expected_y) in [
        (299, 1201),
        (300, 1200),
        (1199, 1200),
        (1200, 1200),
        (1500, 1500),
        (1501, 1501),
        (1502, 1501),
    ] {
        let (mut arena, actor) = fixture(Some([0, clearance, 0]));
        let initial_dirty = arena.object(actor).unwrap().base.dirty_word;
        cached_target(&mut arena, actor, [0, clearance, 4000]);
        let mut terrain = Terrain::new(&flat);
        let mut rng = 0;
        assert_eq!(
            arena.tick_selector93(actor, request(262144, None), &mut rng, &mut terrain),
            Err(Selector93UpdateError::MissingUp)
        );
        assert_eq!(
            arena.object(actor).unwrap().base.position_words,
            Some([0, expected_y, 0])
        );
        assert_eq!(arena.object(actor).unwrap().base.dirty_word, initial_dirty);
        assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 1);
    }
    //Signed negative Y and signed lowAX height, followed by WORD wrap.
    let (mut arena, actor) = fixture(Some([0, (-100_i16) as u16, 0]));
    cached_target(&mut arena, actor, [0, (-100_i16) as u16, 4000]);
    let positive_height = patch_terrain([0, 0], 64);
    let mut terrain = Terrain::new(&positive_height);
    let mut rng = 0;
    assert_eq!(
        arena.tick_selector93(actor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::MissingUp)
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([0, 1324, 0])
    );
}

#[test]
fn steering_clamps_both_signs_and_retains_basis_when_late_up_is_unavailable() {
    let flat = patch_terrain([0, 0], 0);
    for (focus, expected_angle, count) in [
        ([4000, 1500, 4000], 4, 0),
        ([4000, 1500, (-4000_i16) as u16], 4, 100),
        ([0, 1500, (-4000_i16) as u16], -4, 0),
        ([(-4000_i16) as u16, 1500, 4000], -4, 0),
    ] {
        let (mut arena, actor) = fixture(Some([0, 1500, 0]));
        let initial_dirty = arena.object(actor).unwrap().base.dirty_word;
        cached_target(&mut arena, actor, focus);
        let mut expected = NativeBasis::INITIAL;
        expected.words[0] = count;
        arena.actor_mut(actor).unwrap().rotating.basis = expected;
        tables().math.rotate(&mut expected, expected_angle);
        let mut terrain = Terrain::new(&flat);
        let mut rng = 0;
        assert_eq!(
            arena.tick_selector93(actor, request(640, None), &mut rng, &mut terrain),
            Err(Selector93UpdateError::MissingUp)
        );
        assert_eq!(arena.actor(actor).unwrap().rotating.basis, expected);
        assert_eq!(arena.selector93(actor).unwrap().dword_144, 0);
        assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0; 3]);
        assert_eq!(arena.object(actor).unwrap().base.dirty_word, initial_dirty);
    }
}

#[test]
fn full_speed_high_bits_survive_lowword_velocity_and_original_delta_physics() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let initial_dirty = arena.object(actor).unwrap().base.dirty_word;
    cached_target(&mut arena, actor, [0, 1500, 4000]);
    let state = arena.selector93_mut(actor).unwrap();
    state.dword_144 = 0x1234_fff0;
    state.dwords_138_140 = [0x8000_0000, 0x7654_3210, u32::MAX];
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 42;
    arena
        .tick_selector93(actor, request(524288, Some(&UP)), &mut rng, &mut terrain)
        .unwrap();
    assert_eq!(arena.selector93(actor).unwrap().dword_144, 0x1235_05dc);
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 1500]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([0, 1500, 750])
    );
    assert_eq!(
        arena.object(actor).unwrap().base.dirty_word,
        initial_dirty | 0xc0
    );
    assert_eq!(
        arena.selector93(actor).unwrap().dwords_138_140,
        [0x8000_0000, 0x7654_3210, u32::MAX]
    );
    assert_eq!(rng, 42);
    assert_eq!(terrain.calls.len(), 2);
}

#[test]
fn late_physics_position_error_keeps_yaw_speed_and_all_velocity_stores() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let initial_dirty = arena.object(actor).unwrap().base.dirty_word;
    cached_target(&mut arena, actor, [0, 1500, 4000]);
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    terrain.effects[1] = Effect::ForgetPosition;
    let mut rng = 0;
    assert_eq!(
        arena.tick_selector93(actor, request(524288, Some(&UP)), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Motion(
            ActorMotionError::MissingPosition(actor)
        ))
    );
    assert_eq!(arena.selector93(actor).unwrap().dword_144, 1500);
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 1500]);
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 1);
    assert_eq!(arena.object(actor).unwrap().base.position_words, None);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, initial_dirty);
}

#[test]
fn complete_zero_delta_changes_only_documented_target_basis_velocity_and_dirty_fields() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let referent = arena
        .allocate_actor(allocation(Some([0, 1500, 4000])))
        .unwrap();
    let target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(referent).unwrap().handle,
        ))
        .unwrap();
    let target =
        arena.import_fixture_target(crate::object_runtime::NativeTarget::ActorFollow(target));
    arena.selector93_mut(actor).unwrap().target_148 = Some(target);
    arena.selector93_mut(actor).unwrap().dword_144 = 0x1234_fff0;
    let old_object = arena.object(actor).unwrap().clone();
    let before = arena_owners(&arena);
    let mut expected = old_object.clone();
    let ObjectClass::Actor(expected_actor) = &mut expected.class else {
        panic!("actual constructor93");
    };
    expected_actor.rotating.basis.words[0] = 1;
    expected_actor.words_8e_92 = [0, 0, (-16_i16) as u16];
    let super::super::ActorExtension::Selector93(state) = &mut expected_actor.extension else {
        panic!("constructor93");
    };
    let mut expected_target = arena
        .actor_follow_target(state.target_148.unwrap())
        .unwrap();
    expected_target.velocity_1a = Some([0; 3]);
    expected.base.dirty_word |= 0xc0;
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 7;
    arena
        .tick_selector93(actor, request(0, Some(&UP)), &mut rng, &mut terrain)
        .unwrap();
    assert_eq!(arena.object(actor).unwrap(), &expected);
    assert_eq!(
        arena
            .actor_follow_target(arena.selector93(actor).unwrap().target_148.unwrap())
            .unwrap(),
        expected_target
    );
    assert_eq!(rng, 7);
    arena.owned_mut(actor).unwrap().state = old_object;
    assert_eq!(arena_owners(&arena), before);
}

#[test]
fn full_recovery_reaches_world_angular_physics_and_wrong_class_preserves_custody() {
    let (mut arena, actor) = fixture(Some([0, 1500, 0]));
    let initial_dirty = arena.object(actor).unwrap().base.dirty_word;
    cached_target(&mut arena, actor, [0, 1500, 4000]);
    let flat = patch_terrain([0, 0], 0);
    let mut terrain = Terrain::new(&flat);
    let mut rng = 0;
    arena
        .tick_selector93(
            actor,
            request(32, Some(&[0.0, -1.0, 0.0])),
            &mut rng,
            &mut terrain,
        )
        .unwrap();
    assert_eq!(
        arena.actor(actor).unwrap().words_94_9c,
        [0, 0, (-0x2000_i32) as u32]
    );
    assert_eq!(
        arena.object(actor).unwrap().base.dirty_word,
        initial_dirty | 0xd0
    );
    let nonactor = arena.allocate(ObjectClassKind::Base);
    let before = arena_owners(&arena);
    let calls = terrain.calls.len();
    assert_eq!(
        arena.tick_selector93(nonactor, request(0, None), &mut rng, &mut terrain),
        Err(Selector93UpdateError::Object(ObjectError::NotActor(
            nonactor
        )))
    );
    assert_eq!(arena_owners(&arena), before);
    assert_eq!(terrain.calls.len(), calls);
}

#[test]
fn allocation_null_retains_rng_and_continues_old_speed_without_selected_constructor() {
    let flat = patch_terrain([0, 0], 0);
    for up in [None, Some(&UP)] {
        let (mut arena, actor) = fixture(Some([0, 1500, 0]));
        // Seed0 selects this second FIFO actor. Unknown XYZ would stop the
        // follow constructor if allocation succeeded; NULL must bypass it.
        let selected = arena.allocate_actor(allocation(None)).unwrap();
        let selected_before = arena.object(selected).unwrap().clone();
        arena.selector93_mut(actor).unwrap().dword_144 = 4000;
        arena
            .actor_service_heap_mut()
            .script([crate::object_runtime::FixtureActorServiceAllocation::Null]);
        let mut terrain = Terrain::new(&flat);
        terrain.fail_at = Some(0);
        let mut rng = 0;
        let result = arena.tick_selector93(actor, request(524288, up), &mut rng, &mut terrain);
        assert_eq!(rng, 0x0013_d573);
        assert_eq!(arena.actor_service_heap().attempts().len(), 1);
        assert_eq!(arena.selector93(actor).unwrap().target_148, None);
        assert_eq!(arena.object(selected).unwrap(), &selected_before);
        assert!(terrain.calls.is_empty());
        if up.is_none() {
            assert_eq!(result, Err(Selector93UpdateError::MissingUp));
            assert_eq!(arena.selector93(actor).unwrap().dword_144, 4000);
        } else {
            result.unwrap();
            // Captured4000 is capped to3000, then the exact signed smoothing
            // step at delta524288 stores3000 before velocity/physics.
            assert_eq!(arena.selector93(actor).unwrap().dword_144, 3000);
            assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 3000]);
        }
    }
}
