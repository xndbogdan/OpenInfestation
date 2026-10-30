use super::*;
use crate::frontend_camera::AnimationClock;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};
use crate::terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime};

fn assets() -> &'static crate::frontend_assets::FrontendAssets {
    static ASSETS: std::sync::OnceLock<crate::frontend_assets::FrontendAssets> =
        std::sync::OnceLock::new();
    ASSETS.get_or_init(|| {
        crate::frontend_assets::FrontendAssets::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
        )
        .unwrap()
    })
}
fn executable() -> FrontendExecutable<'static> {
    FrontendExecutable::parse(assets().executable_bytes()).unwrap()
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut constructor_rng = 0;
    let actor = arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([384, 64, 640]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &assets().sine_table().values,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut constructor_rng,
        )
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.word_28 = 1024;
    (arena, actor)
}
struct Terrain {
    current: TerrainRuntime,
    calls: usize,
    fail_at: Option<usize>,
    install_position: Option<[u16; 3]>,
}
impl Terrain {
    fn new() -> Self {
        Self {
            current: native_probe_fixture_terrain(&serde_json::json!({
                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                "xz":[384,640],"heights":[2,2,2,2],"alternate":false,
            })),
            calls: 0,
            fail_at: None,
            install_position: None,
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
        if let Some(position) = self.install_position {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
        }
        if self.fail_at == Some(self.calls) {
            Err("required actual H12")
        } else {
            Ok(&self.current)
        }
    }
}
struct UnreachedHeader;
impl ActorContactHeaderHost for UnreachedHeader {
    type Error = &'static str;
    fn header(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<&[u8], Self::Error> {
        panic!("native byte160=0 does not acquire H14")
    }
}
fn visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    status: i32,
    delta: u32,
    controls: [i32; 2],
    terrain: &mut Terrain,
) -> Result<Selector36MotionNextPhase, Selector36MotionError<&'static str, &'static str>> {
    arena.actor_mut(actor).unwrap().word_c4 = status as u32;
    let clock = AnimationClock::default();
    let plane = Terrain::new().current;
    let math = NativeBasisMath::new(assets().sine_table().values);
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable().data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    arena.tick_selector36_motion(
        actor,
        Selector36MotionRequest {
            microseconds: delta,
            controls,
            executable: executable(),
            environment: ActorContactEnvironment {
                animation_clock: &clock,
                plane_policy: &plane,
                up: &[0.0, 1.0, 0.0],
                sine_quarter: &assets().sine_table().values,
            },
            stack_policy: ActorContactStackPolicy::RequiredNativeAddress,
            math: &math,
            inverse_sine: &inverse.values,
        },
        terrain,
        &mut UnreachedHeader,
    )
}

#[test]
fn constructor_scalar_is_unknown_and_nonpositive_status_still_runs_contact_and_real_physics() {
    let (mut arena, actor) = fixture();
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, None);
    // Unreached invalid motor pointers distinguish a zero-control bypass
    // from the actual status<=0 contact/alignment/physics path.
    arena.family4236d0_actor_mut(actor).unwrap().table_254 = 0xdead0000;
    let before = arena.object(actor).unwrap().base.position_words.unwrap();
    let mut terrain = Terrain::new();
    assert_eq!(
        visit(&mut arena, actor, 0, 65536, [31, 1000], &mut terrain).unwrap(),
        Selector36MotionNextPhase::PersistentAudio481b10 {
            controls: [31, 1000]
        }
    );
    assert_eq!(terrain.calls, 3);
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, Some(0));
    // Genuine physics mask1F: gravity -64, drag -63, signed integration -4.
    assert_eq!(
        arena.actor(actor).unwrap().words_8e_92,
        [0, (-63i16) as u16, 0]
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([before[0], 60, before[2]])
    );
    assert_ne!(arena.object(actor).unwrap().base.dirty_word & 0xc0, 0);
}

#[test]
fn alignment_reacquires_h12_after_real_contact_and_retains_y_impulse_scalar_on_failure() {
    let (mut arena, actor) = fixture();
    let mut terrain = Terrain::new();
    terrain.fail_at = Some(3);
    let result = visit(&mut arena, actor, 1, 1024, [0, 1000], &mut terrain);
    assert!(matches!(
        result,
        Err(Selector36MotionError::Contact(ActorContactError::Terrain(
            "required actual H12"
        )))
    ));
    assert_eq!(terrain.calls, 3);
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, Some(0));
    assert_eq!(
        arena.object(actor).unwrap().base.position_words.unwrap()[1],
        320
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 1, 0]);
}

#[test]
fn angular_table_failure_preserves_actual_forward_increment_before_lateral_and_physics() {
    let (mut arena, actor) = fixture();
    arena.family4236d0_actor_mut(actor).unwrap().table_258 = 0xdead0000;
    let mut terrain = Terrain::new();
    let result = visit(&mut arena, actor, 1, 1024, [400, 1000], &mut terrain);
    assert!(matches!(
        result,
        Err(Selector36MotionError::Table(
            FrontendExecutableError::DataAddress(0xdead0000)
        ))
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, Some(0));
    let velocity = arena.actor(actor).unwrap().words_8e_92;
    assert_eq!(velocity[0], 0);
    assert_eq!(velocity[1], 1);
    assert!(velocity[2] > 0);
    assert_eq!(arena.actor(actor).unwrap().words_94_9c, [0; 3]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words.unwrap()[1],
        320
    );
}

#[test]
fn zero_delta_still_acquires_and_publishes_contact_before_audio_boundary() {
    let (mut arena, actor) = fixture();
    let mut terrain = Terrain::new();
    visit(&mut arena, actor, -1, 0, [0, 0], &mut terrain).unwrap();
    assert_eq!(terrain.calls, 3);
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, Some(0));
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([384, 64, 640])
    );
}

#[test]
fn ground_damping_keeps_native_logical_shift_and_axis_thresholds() {
    assert_eq!(ground_velocity((-100i16) as u16, 32768, 64), 100);
    assert_eq!(ground_velocity(64, 32, 64), 0);
    assert_eq!(ground_velocity((-64i16) as u16, 32, 64), 0);
    assert_eq!(ground_velocity(25, 32, 24), 25);
    assert_eq!(ground_velocity(24, 32, 24), 0);
}

#[test]
fn inverse_float_owner_uses_float_dwords_and_retained_zy_then_x_sum() {
    let mut basis = NativeBasis::INITIAL;
    // Supplied intrinsic float domain: cancellation discriminates sum order;
    // it does not claim an actual unit-normal/basis contact pose.
    basis.words[4] = 1.0f32.to_bits();
    basis.words[5] = 1.0f32.to_bits();
    basis.words[6] = 1.0f32.to_bits();
    assert_eq!(
        inverse_floats(basis, [1.0, -1152921504606846976.0, 1152921504606846976.0])[1],
        1.0
    );
    assert_eq!(
        inverse_floats(NativeBasis::INITIAL, [0.25, 0.75, -0.5]),
        [0.25, 0.75, -0.5]
    );
}

#[test]
fn complete_alignment_uses_real_slope_normal_and_prelookup_captured_y() {
    let (mut arena, actor) = fixture();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([384, 64, 640]);
    let mut terrain = Terrain {
        current: native_probe_fixture_terrain(&serde_json::json!({
            "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
            "xz":[384,640],"heights":[0,20,20,0],"alternate":false,
        })),
        calls: 0,
        fail_at: None,
        install_position: None,
    };
    let mut normal = [None; 3];
    arena
        .actor_terrain_normal::<Terrain, &'static str>(actor, &mut terrain, &mut normal)
        .unwrap();
    let normal = normal.map(Option::unwrap);
    assert!(
        normal[0].abs() > 0.5,
        "supplied real H12 gradient must enter native angle correction"
    );
    // Native482172 captures Y before H12 virtual acquisition. This service
    // mutation must persist, while captured Y still determines alignment bounds.
    terrain.install_position = Some([384, 32000, 640]);
    let math = NativeBasisMath::new(assets().sine_table().values);
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable().data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    arena
        .align_selector36::<Terrain, &'static str>(
            actor,
            normal,
            0xffff,
            32,
            &math,
            &inverse.values,
            &mut terrain,
        )
        .unwrap();
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([384, 32000, 640])
    );
    assert_ne!(
        arena.actor(actor).unwrap().rotating.basis,
        NativeBasis::INITIAL
    );
}

#[test]
fn complete_alignment_inverse_services_and_selected_motor_tables_are_guarded() {
    use sha2::{Digest, Sha256};
    let bytes = assets().executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    // BF09 .text raw offsets equal RVA; the whole EXE fixes this PE profile.
    // These bodies prove the new alignment/float phases and reused DWORD
    // inverse, without executing original instructions or claiming a pose.
    for (start, end, hash) in [
        (
            0x00482160usize,
            0x004823E9usize,
            "20d0d866659d8b2e4326e02b0e50bdc5782345da95c8ed9febe073cf367e05da",
        ),
        (
            0x00425E60usize,
            0x00425F47usize,
            "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
        ),
        (
            0x0047A6A0usize,
            0x0047A6B5usize,
            "122ac8a433cc356fc8ae9769a154f2eaaac76baa2bffeff474b3e440dc778afe",
        ),
        (
            0x0047A680usize,
            0x0047A695usize,
            "c4dd0ecc183d684ea563bc07fa4c319c357177b039d32b7f0882dbeef9b3583f",
        ),
        (
            0x00490710usize,
            0x004907D2usize,
            "9b1af0868984acd6eda5633b63cd6e3b0f6aacc38ec253f726d2014fdf7fa2fe",
        ),
        (
            0x004921B0usize,
            0x004921D2usize,
            "f158abed69aea2eb4296b1bab75798a434e5f2320aa651c4c3e19b8772247009",
        ),
        (
            0x004921E0usize,
            0x00492202usize,
            "0890e7a92d41a81de4713fb4de5f5ccf2cb18299140430be0471795d15cfcdd6",
        ),
        (
            0x00492210usize,
            0x00492232usize,
            "67d4760adb00878803dacb40a2a9d3e4088f7882fa601ebc8cc3e591e86f3c49",
        ),
        (
            0x00490870usize,
            0x004908F8usize,
            "e697522def0bbf36e51720c35c5bcc9848ab3154ae0a2835f640fbbee4ec4638",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[start - 0x400000..end - 0x400000])
            ),
            hash
        );
    }
    let exe = executable();
    for (address, length, hash) in [
        (
            0x0052801Cu32,
            4usize,
            "461fd2eb57b562e156fa72295897db3f1dff3372c6dda9a1275d4b3d786abae6",
        ),
        (
            0x0052FE38u32,
            8usize,
            "058cd4aba7e0564f60fafba8153908b8f1e0cb4da13c2299d66e39ea34ef3b3e",
        ),
        (
            0x0052FE40u32,
            8usize,
            "0dd884a8410dc972142344cf1c0ec55016a3b240313dcbbd0c4b47d04d865d78",
        ),
        (
            0x0052FE48u32,
            16usize,
            "abe3cbf8cab30d713d4c4f6482890769a69d874bddbdc04d7bdbbd1fc5b91305",
        ),
        (
            0x0052FED8u32,
            16usize,
            "5d9c904fe6a50ba45c19f9fcb6da63d00bde825efae059479753e196f0fc9df3",
        ),
        (
            0x0052FEE8u32,
            16usize,
            "10109311b9d93d2ec6d100987ebcf6181b5bcc4fac9d4fa2d6cd569c0924caaa",
        ),
        (
            0x0052FEF8u32,
            16usize,
            "716e7d6690788649a03ab2c5cbd09e8a91ecdf3bfc59d154c16bbe83f16a51e3",
        ),
        (
            0x0052FF08u32,
            16usize,
            "13aa1c56ae6e2ac7bcb27f38cad7d0b055c3cf9ec98477b76b3a0dcb402c3b04",
        ),
        (
            0x0052FF18u32,
            16usize,
            "fe8c32aeb3081a9139e5b05731ab86dd603cc4f1953c67410ab3d82a1349e0e6",
        ),
        (
            0x0052FF28u32,
            16usize,
            "fb51772dc14c941063afd4c26fbd030d04dc2d34d7e1254c807541cc1f5caaa7",
        ),
        (
            0x0052FF38u32,
            16usize,
            "06d1255006613fa5530e48be74739b15d1bfb9a33511b3788c92cbee9a98d6da",
        ),
        (
            0x0052FF48u32,
            16usize,
            "36d2f52ddc64e013c777d00d0c37d5abb670e36e3629cc967073cf25f0b2a942",
        ),
        (
            0x0052FF88u32,
            16usize,
            "bdfa856e3c9c9a842c424512e11b65ffc3bebc58d38e9da6259a939fdeb4c4ed",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(exe.mapped(address, length).unwrap().bytes)
            ),
            hash
        );
    }
}
