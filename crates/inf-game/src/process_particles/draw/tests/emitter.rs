use super::*;

struct NoCalls;
impl ParticleDrawLighting for NoCalls {
    type Error = &'static str;
    fn terrain_light(&mut self, _: TerrainLightRequest, _: [i32; 3]) -> Result<(), Self::Error> {
        panic!("RET draw lighting")
    }
}
impl<'r> ParticleDrawHost<'r> for NoCalls {
    fn sprite(&mut self, _: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        panic!("RET draw sprite")
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        panic!("RET draw sorting")
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        panic!("RET draw identity")
    }
    fn submit(&mut self, _: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        panic!("RET draw submission")
    }
}

#[test]
fn emitter_draw_executes_real_ret_policy_without_reading_unwritten_animation_fields() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut pool = ProcessParticleRuntime::default();
    let id = pool
        .allocate(ParticleAllocationRequest {
            payload_bytes: 64,
            last_reclaim_category: 3,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    pool.construct(
        id,
        ParticleConstruction {
            category: 3,
            position: [1, 2, 3].into(),
            velocity: None,
            source_handle: 0,
            flags: 0,
        },
    )
    .unwrap();
    pool.record_mut(id)
        .unwrap()
        .write_dword(0, 0x526f80)
        .unwrap();
    let request = ParticleDrawRequest {
        bank: ParticleDrawBank::Pending,
        context: ParticleDrawContext {
            position: [0; 3],
            basis: MenuBasis::INITIAL,
            view: SceneView {
                eye: [0; 3],
                basis: MenuBasis::INITIAL,
            },
            projection: ModelProjection {
                focal: [320; 2],
                viewport: [640, 480],
                center: [320, 240],
                fade: [0, 10000],
            },
            coordinate_shift: 0,
            fog_factor: 0,
            projection_dispatch: ModelDispatch::Far,
            far_color: 0,
        },
        terrain: None,
    };
    let report = pool.draw_particles(request, executable, &mut NoCalls);
    assert_eq!(report.visited, 1);
    assert_eq!(report.commands, 0);
    assert_eq!(report.stopped, 0);
    assert!(report.omissions.is_empty());
    assert!(pool.record(id).unwrap().read_dword(0x30).is_err());
    assert!(pool.record(id).unwrap().read_dword(0x28).is_err());
}
