use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, ObjectList, ObjectNodeKind, Selector12GrantContext,
};
use crate::terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime};
const SINE: [u16; 4096] = [0; 4096];
fn fixture(class: FamilyUpdateClass) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let request = DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        },
        world_mask_limits: [3; 2],
    };
    let actor = match class {
        FamilyUpdateClass::Family4236d0 => {
            arena.allocate_family4236d0_actor(request, &mut 0).unwrap()
        }
        FamilyUpdateClass::Selector10 => arena.allocate_selector10_actor(request, &mut 0).unwrap(),
        FamilyUpdateClass::Selector36 => arena.allocate_selector36_actor(request, &mut 0).unwrap(),
        FamilyUpdateClass::Selector35 => arena.allocate_selector35_actor(request, &mut 0).unwrap(),
        FamilyUpdateClass::Selector34 => arena.allocate_selector34_actor(request, &mut 0).unwrap(),
        FamilyUpdateClass::Selector12 => arena
            .allocate_selector12_actor(
                request,
                &mut 0,
                Selector12GrantContext {
                    table: &inf_formats::GrantTable {
                        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
                        records: vec![],
                    },
                    first_player_grants: None,
                },
            )
            .unwrap(),
    };
    (arena, actor)
}
struct Terrain {
    current: TerrainRuntime,
    calls: usize,
    position_after_acquisition: Option<[u16; 3]>,
    fail: bool,
}
impl Terrain {
    fn new() -> Self {
        Self {
            current: native_probe_fixture_terrain(&serde_json::json!({
                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                "xz":[0,0],"heights":[2,2,2,2],"alternate":false,
            })),
            calls: 0,
            position_after_acquisition: None,
            fail: false,
        }
    }
}
impl ActorTerrainHost for Terrain {
    type Error = &'static str;
    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        if let Some(position) = self.position_after_acquisition {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
        }
        if self.fail {
            Err("H12 unavailable")
        } else {
            Ok(&self.current)
        }
    }
}
#[test]
fn dormant_false_and_selector12_update_bypass_unread_owners() {
    for class in [
        FamilyUpdateClass::Selector10,
        FamilyUpdateClass::Selector36,
        FamilyUpdateClass::Selector12,
    ] {
        let (mut arena, actor) = fixture(class);
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        let before = format!("{arena:?}");
        let mut host = Terrain::new();
        host.fail = true;
        assert_eq!(
            arena.dispatch_family_dormant(
                actor,
                FamilyDormantRequest::Change { entering: false },
                &mut host
            ),
            Ok(())
        );
        if class == FamilyUpdateClass::Selector12 {
            for microseconds in [0, u32::MAX] {
                assert_eq!(
                    arena.dispatch_family_dormant(
                        actor,
                        FamilyDormantRequest::Update { microseconds },
                        &mut host
                    ),
                    Ok(())
                );
            }
        }
        assert_eq!(host.calls, 0);
        assert_eq!(format!("{arena:?}"), before);
    }
}
#[test]
fn dormant_entry_preserves_selector12_flags_and_shared_membership() {
    for class in [
        FamilyUpdateClass::Selector10,
        FamilyUpdateClass::Selector36,
        FamilyUpdateClass::Selector12,
    ] {
        let (mut arena, actor) = fixture(class);
        arena.set_flag0(actor, true).unwrap();
        arena.set_flag1(actor, true).unwrap();
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.flags |= 0x8100_0104;
        base.dirty_word = 0x8000;
        let flags = base.flags;
        let flag0 = arena.list(ObjectList::Flag0Primary).to_vec();
        let flag1 = arena.list(ObjectList::Flag1Primary).to_vec();
        let mut host = Terrain::new();
        arena
            .dispatch_family_dormant(
                actor,
                FamilyDormantRequest::Change { entering: true },
                &mut host,
            )
            .unwrap();
        assert_eq!(
            arena.family428650_actor(actor).unwrap().word_228,
            0x0053_0d44
        );
        let base = &arena.object(actor).unwrap().base;
        assert_eq!(
            (base.flags, base.dirty_word),
            if class == FamilyUpdateClass::Selector12 {
                (flags, 0x8000)
            } else {
                (flags & 0xffff_fefb, 0x8002)
            }
        );
        assert_eq!(arena.list(ObjectList::Flag0Primary), flag0);
        assert_eq!(arena.list(ObjectList::Flag1Primary), flag1);
        assert_eq!(host.calls, 0);
    }
}
#[test]
fn shared_dormant_uses_captured_xz_and_post_acquisition_y_before_disposal() {
    for class in [FamilyUpdateClass::Selector10, FamilyUpdateClass::Selector36] {
        for (y, disposes) in [(14, false), (13, true), (100, false), (10, true)] {
            let (mut arena, actor) = fixture(class);
            arena.owned_mut(actor).unwrap().state.base.word_28 = 40;
            arena.set_flag0(actor, true).unwrap();
            arena.set_flag1(actor, true).unwrap();
            let mut host = Terrain::new();
            host.position_after_acquisition = Some([1024, y as u16, 1024]);
            assert_eq!(host.current.height_at([0, 0]), 64);
            arena
                .dispatch_family_dormant(
                    actor,
                    FamilyDormantRequest::Update {
                        microseconds: u32::MAX,
                    },
                    &mut host,
                )
                .unwrap();
            assert_eq!(host.calls, 1);
            assert_eq!(
                arena
                    .list(ObjectList::Disposal)
                    .iter()
                    .any(|node| node.object == actor),
                disposes
            );
            assert_eq!(
                arena.object(actor).unwrap().base.position_words,
                Some([1024, y as u16, 1024])
            );
            if disposes {
                assert!(arena.object(actor).is_ok());
                for kind in [ObjectNodeKind::Flag0, ObjectNodeKind::Flag1] {
                    assert!(arena
                        .list(ObjectList::Parking)
                        .iter()
                        .any(|node| node.object == actor && node.kind == kind));
                }
                assert_eq!(arena.object(actor).unwrap().base.flags & 3, 0);
            }
        }
    }
}
#[test]
fn required_ground_failure_keeps_host_prefix_and_never_disposes() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector36);
    let mut host = Terrain::new();
    host.fail = true;
    host.position_after_acquisition = Some([4, 5, 6]);
    assert_eq!(
        arena.dispatch_family_dormant(
            actor,
            FamilyDormantRequest::Update { microseconds: 0 },
            &mut host
        ),
        Err(FamilyDormantError::Terrain("H12 unavailable"))
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([4, 5, 6])
    );
    assert!(arena.list(ObjectList::Disposal).is_empty());
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    assert_eq!(
        arena.dispatch_family_dormant(
            actor,
            FamilyDormantRequest::Update { microseconds: 0 },
            &mut host
        ),
        Err(FamilyDormantError::Object(
            ObjectError::ActorPositionUnknown(actor)
        ))
    );
    assert_eq!(host.calls, 1);
}
#[test]
fn selector35_dormant_effects_remain_required_even_on_false_entry() {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector35);
    let before = format!("{arena:?}");
    let mut host = Terrain::new();
    for (request, address) in [
        (
            FamilyDormantRequest::Change { entering: false },
            0x0043_e120,
        ),
        (FamilyDormantRequest::Change { entering: true }, 0x0043_e120),
        (
            FamilyDormantRequest::Update { microseconds: 0 },
            0x0043_e1c0,
        ),
    ] {
        assert_eq!(
            arena.dispatch_family_dormant(actor, request, &mut host),
            Err(FamilyDormantError::RequiredCallback { actor, address })
        );
        assert_eq!(format!("{arena:?}"), before);
    }
    assert_eq!(host.calls, 0);
}

#[test]
fn complete_dormant_bodies_and_actual_final_tables_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in [
        (
            0x469380usize,
            0x469394usize,
            "ca892dc9e098b4604b28a8ba26630f3e627569781accde380d077f5ad5ef7604",
        ),
        (
            0x4693a0usize,
            0x4693b4usize,
            "9b30cdc646bd4b1e5b8a3829e15f2ba9f2e393a02f41ac440559d95b8c7ad36f",
        ),
        (
            0x428630usize,
            0x428645usize,
            "8c09f41405158bedc1906b5dc14ccf75fae2408fe65fbb64b00d4ac946d379a9",
        ),
        (
            0x404840usize,
            0x404843usize,
            "e598d0c3ba86d917b177d7adde0556aa99bc355543c57aee0a3e50b684dd7e99",
        ),
        (
            0x424790usize,
            0x4247bfusize,
            "b6ae4485687775d5a0a5ee1c455d2210242dfe3abef1636b2868ccab390a334d",
        ),
        (
            0x4247c0usize,
            0x424822usize,
            "95ea0274699a591b0e50ba5cf9a46c04ca52e2e5e093e871d24835ec6b4e7382",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[start - 0x400000..end - 0x400000])
            ),
            expected
        );
    }
    for (table, dc, e0) in [
        (0x520cc8usize, 0x4247c0, 0x424790),
        (0x527ee0, 0x4247c0, 0x424790),
        (0x527058, 0x404840, 0x428630),
        (0x52273c, 0x43e1c0, 0x43e120),
    ] {
        for (slot, address) in [(0xdc, dc), (0xe0, e0)] {
            let offset = table - 0x400000 + slot;
            assert_eq!(
                u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
                address
            );
        }
    }
}
