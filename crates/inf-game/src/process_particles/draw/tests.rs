//! Original-bytecode receipts are the packet authority.

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    process_particles::{
        audio::ParticleSoundRequest, births::*, ParticleAllocationRequest, ParticleConstruction,
    },
    render_resources::{RenderResourceKind, RenderResourceRequest, RenderResources},
    Installation, ResidencyRequest, ResidentRuntime,
};
use serde::Deserialize;

mod emitter;
mod projectile;
mod shadow;
mod terrain_flash;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    rng: u32,
    pending: bool,
    context_position: [i32; 3],
    context_basis: [f32; 9],
    eye: [i32; 3],
    view_basis: [f32; 9],
    focal: [i32; 2],
    viewport: [i32; 2],
    center: [i32; 2],
    fog_factor: i32,
    fade: [i32; 2],
    coordinate_shift: u8,
    far_color: u32,
    sorting: bool,
    selector: u8,
    texture_flags: u16,
    dimensions: [u16; 2],
    particles: Vec<Item>,
    packets: Vec<Packet>,
    lights: Vec<Light>,
    draw_order: Vec<u16>,
    records: Vec<Record>,
    sort_queries: usize,
    shadow_prefix: Option<bool>,
}
#[derive(Deserialize)]
struct Item {
    kind: String,
    particle_type: u32,
    position: [u16; 3],
    category: Option<usize>,
    lifetime: Option<u16>,
    flags: Option<u16>,
    floor: Option<i16>,
    null_table: Option<bool>,
    fresh_floor: Option<bool>,
    light_amount: Option<i16>,
    shadow: Option<bool>,
}
#[derive(Deserialize)]
struct Packet {
    depth: u32,
    callback: u32,
    payload: Vec<Option<u8>>,
}
#[derive(Deserialize)]
struct Record {
    slot: u16,
    flags: u16,
    floor: Option<u16>,
}
#[derive(Deserialize)]
struct Light {
    world_xz: [i32; 2],
    radius: i32,
    rgb: [i8; 3],
}

struct BirthAudio;
impl ParticleBirthAudio for BirthAudio {
    type Error = &'static str;
    fn explosion_sound(
        &mut self,
        _: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
struct Host<'r> {
    resource: RenderResource<'r>,
    sorting: bool,
    queries: usize,
    packets: Vec<PrimitiveCommand<'r>>,
    lights: Vec<TerrainLightRequest>,
    /// Where each light shines from, for the tracer.
    emitters: Vec<[i32; 3]>,
    fail_light: bool,
    shadow_color: Option<u32>,
}
impl ParticleDrawLighting for Host<'_> {
    type Error = &'static str;
    fn terrain_light(
        &mut self,
        request: TerrainLightRequest,
        emitter: [i32; 3],
    ) -> Result<(), Self::Error> {
        self.lights.push(request);
        self.emitters.push(emitter);
        if self.fail_light {
            Err("required light writer")
        } else {
            Ok(())
        }
    }
}
impl<'r> ParticleDrawHost<'r> for Host<'r> {
    fn shadow_material(&mut self, slot: i16) -> Result<Option<u32>, Self::Error> {
        assert_eq!(slot, 32);
        Ok(self.shadow_color)
    }
    fn sprite(&mut self, slot: i16) -> Result<Option<RenderResource<'r>>, Self::Error> {
        Ok(Some(RenderResource {
            request: RenderResourceRequest {
                kind: RenderResourceKind::Sprite,
                argument: i32::from(slot),
            },
            ..self.resource
        }))
    }
    fn material_sorting(&mut self) -> Result<bool, Self::Error> {
        self.queries += 1;
        Ok(self.sorting)
    }
    fn texture_sort_identity(
        &mut self,
        _: RenderResource<'r>,
    ) -> Result<TextureSortIdentity, Self::Error> {
        Ok(TextureSortIdentity::NativeEntryAddress(0x750000))
    }
    fn submit(&mut self, command: PrimitiveCommand<'r>) -> Result<(), Self::Error> {
        self.packets.push(command);
        Ok(())
    }
}
fn basis(values: [f32; 9]) -> MenuBasis {
    let mut result = MenuBasis::INITIAL;
    result.words[1..].copy_from_slice(&values.map(f32::to_bits));
    result
}
fn setup() -> (ResidentRuntime, FrontendAssets) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    (runtime, FrontendAssets::open(&root).unwrap())
}
fn construct(case: &Case, exe: FrontendExecutable<'_>) -> ProcessParticleRuntime {
    let mut pool = ProcessParticleRuntime::default();
    let mut rng = case.rng;
    for item in &case.particles {
        let before = (0..400)
            .map(|i| pool.occupant(i).is_some())
            .collect::<Vec<_>>();
        let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
        };
        let request = ParticleBirthRequest {
            particle_type: item.particle_type,
            position: item.position,
            velocity: Some([0; 3]),
            flags: 0,
            inputs: ParticleBirthInputs {
                phase: Some(4),
                network_active: false,
                process: &birth_process,
            },
        };
        match item.kind.as_str() {
            "central" => {
                pool.birth_central(request, exe, &mut rng, &mut BirthAudio)
                    .unwrap();
            }
            "debris" => {
                pool.birth_debris(request, exe, &mut rng).unwrap();
            }
            "trail" => {
                pool.birth_trail(request, exe, &mut rng).unwrap();
            }
            "splash" => {
                pool.birth_splash(request, exe, &mut rng).unwrap();
            }
            "base" => {
                let id = pool
                    .allocate(ParticleAllocationRequest {
                        payload_bytes: 0x34,
                        last_reclaim_category: 1,
                        network_active: false,
                        phase: Some(4),
                    })
                    .unwrap();
                pool.construct(
                    id,
                    ParticleConstruction {
                        category: item.category.unwrap(),
                        position: item.position.into(),
                        velocity: Some([0; 3].into()),
                        source_handle: 0,
                        flags: 0,
                    },
                )
                .unwrap();
                let record = pool.record_mut(id).unwrap();
                for (off, value) in [(0, 0x528914), (0x28, 0x53d2b0), (0x2c, 256), (0x30, 0)] {
                    record.write_dword(off, value).unwrap();
                }
            }
            kind => panic!("unknown fixture birth {kind}"),
        }
        for (slot, occupied) in before.into_iter().enumerate() {
            if occupied {
                continue;
            }
            if let Some(id) = pool.occupant(slot) {
                let record = pool.record_mut(id).unwrap();
                if let Some(lifetime) = item.lifetime {
                    record.write_word(0x10, lifetime).unwrap();
                }
                let flags = item
                    .flags
                    .unwrap_or_else(|| record.read_word(0x12).unwrap());
                record
                    .write_word(
                        0x12,
                        if item.fresh_floor.unwrap_or(false) {
                            flags
                        } else {
                            flags | 0x20
                        },
                    )
                    .unwrap();
                record
                    .write_word(0x24, item.floor.unwrap_or(0) as u16)
                    .unwrap();
                if item.null_table.unwrap_or(false) {
                    record.write_dword(0x28, 0).unwrap();
                }
            }
        }
    }
    if !case.pending {
        pool.transfer_pending();
    }
    pool
}
fn request<'a>(case: &Case) -> ParticleDrawRequest<'a> {
    ParticleDrawRequest {
        bank: if case.pending {
            ParticleDrawBank::Pending
        } else {
            ParticleDrawBank::Active
        },
        context: ParticleDrawContext {
            position: case.context_position,
            basis: basis(case.context_basis),
            view: SceneView {
                eye: case.eye,
                basis: basis(case.view_basis),
            },
            projection: ModelProjection {
                focal: case.focal,
                viewport: case.viewport,
                center: case.center,
                fade: case.fade,
            },
            coordinate_shift: case.coordinate_shift,
            fog_factor: case.fog_factor,
            projection_dispatch: ModelDispatch::Far,
            far_color: case.far_color,
        },
        terrain: None,
    }
}

fn flat_terrain() -> TerrainRuntime {
    flat_terrain_height(0)
}
fn flat_terrain_height(height: i8) -> TerrainRuntime {
    use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
    use inf_formats::{
        parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion,
        TERRAIN_ENTRY_FIXED_BYTES,
    };
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..inf_formats::TERRAIN_CELL_COUNT {
        bytes[inf_formats::TERRAIN_PREFIX_BYTES + index * 2] = height as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

/// Supplied animation data stays fixed-layout and changes no instruction byte.
fn supplied_data(case: &Case, pool: &mut ProcessParticleRuntime, source: &[u8]) -> Vec<u8> {
    let mut raw = source.to_vec();
    let exe = FrontendExecutable::parse(source).unwrap();
    for item in &case.particles {
        if item.light_amount.is_none() && !item.shadow.unwrap_or(false) {
            continue;
        }
        for slot in 0..400 {
            let Some(id) = pool.occupant(slot) else {
                continue;
            };
            let table = pool.record(id).unwrap().read_dword(0x28).unwrap();
            if let Some(amount) = item.light_amount {
                let frame = animation::frame_address(
                    exe,
                    table,
                    pool.record(id).unwrap().read_word(0x10).unwrap(),
                )
                .unwrap();
                let range = exe.mapped(frame + 2, 5).unwrap().file_span;
                raw[range.start..range.start + 2].copy_from_slice(&amount.to_le_bytes());
                raw[range.start + 2..range.start + 5].copy_from_slice(&[128, 127, 255]);
            }
            if item.shadow.unwrap_or(false) {
                let range = exe.mapped(table + 14, 1).unwrap().file_span;
                raw[range.start] |= 8;
                pool.record_mut(id).unwrap().write_dword(0x30, 1).unwrap();
            }
        }
    }
    raw
}

#[test]
fn original_particle_draw_receipts_match_packets_flags_lights_and_banks() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_draws_native.json"
    ))
    .unwrap();
    let (runtime, assets) = setup();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let template = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let RenderResourceData::Sprite(source) = template.data else {
        unreachable!()
    };
    for case in receipt.cases {
        let mut entry = source.clone();
        entry.selectors.mode0 = case.selector;
        entry.variants = vec![source.variants[0].clone(); 4];
        for variant in &mut entry.variants {
            variant.width = case.dimensions[0];
            variant.height = case.dimensions[1];
            variant.flags = inf_formats::SpriteFlags::new(case.texture_flags);
        }
        let mut host = Host {
            resource: RenderResource {
                data: RenderResourceData::Sprite(&entry),
                ..template
            },
            sorting: case.sorting,
            queries: 0,
            packets: Vec::new(),
            lights: Vec::new(),
            emitters: Vec::new(),
            fail_light: false,
            shadow_color: None,
        };
        let mut particles = construct(&case, exe);
        let supplied = supplied_data(&case, &mut particles, assets.executable_bytes());
        let exe = FrontendExecutable::parse(&supplied).unwrap();
        let terrain = flat_terrain();
        let mut draw_request = request(&case);
        draw_request.terrain = Some(&terrain);
        let report = particles.draw_particles(draw_request, exe, &mut host);
        if case.shadow_prefix.unwrap_or(false) {
            assert_eq!(report.omissions.len(), 1);
            assert!(matches!(
                report.omissions[0].error,
                ParticleDrawError::MissingShadowMaterial(32)
            ));
        } else {
            assert!(
                report.omissions.is_empty(),
                "{}: {:?}",
                case.name,
                report.omissions
            );
        }
        assert_eq!(
            report.visited,
            case.draw_order.len(),
            "{} visitation",
            case.name
        );
        assert_eq!(report.commands, case.packets.len(), "{} count", case.name);
        assert_eq!(host.queries, case.sort_queries, "{} sorting", case.name);
        for (actual, want) in host.packets.iter().zip(&case.packets) {
            assert_eq!(actual.depth, want.depth, "{} key", case.name);
            assert_eq!(
                actual.callback.native_address(),
                want.callback,
                "{} callback",
                case.name
            );
            assert_eq!(actual.payload, want.payload, "{} packet", case.name);
            assert_eq!(actual.texture.unwrap().0, 0x24);
            assert_eq!(actual.header_words, [0; 3]);
        }
        let lights: Vec<_> = case
            .lights
            .iter()
            .map(|v| TerrainLightRequest {
                world_xz: v.world_xz,
                radius: v.radius,
                rgb: v.rgb,
            })
            .collect();
        assert_eq!(host.lights, lights, "{} lights", case.name);
        // Each light shines from the particle that shed it, at its X/Z.
        for (emitter, light) in host.emitters.iter().zip(&host.lights) {
            assert_eq!(
                [emitter[0], emitter[2]],
                light.world_xz,
                "{} emitter",
                case.name
            );
        }
        for record in case.records {
            assert_eq!(
                particles
                    .record(ParticleId::Pool(record.slot))
                    .unwrap()
                    .read_word(0x12)
                    .unwrap(),
                record.flags,
                "{} slot{}",
                case.name,
                record.slot
            );
        }
    }
}

#[test]
fn required_light_failure_preserves_prefix_and_continues_other_particles() {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_draws_native.json"
    ))
    .unwrap();
    let case = receipt
        .cases
        .iter()
        .find(|v| v.name == "required-light-prefix")
        .unwrap();
    let (runtime, assets) = setup();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let template = RenderResources::new(&runtime)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: 1805,
        })
        .unwrap();
    let RenderResourceData::Sprite(source) = template.data else {
        unreachable!()
    };
    let mut entry = source.clone();
    entry.selectors.mode0 = case.selector;
    entry.variants = vec![source.variants[0].clone(); 4];
    for variant in &mut entry.variants {
        variant.width = case.dimensions[0];
        variant.height = case.dimensions[1];
        variant.flags = inf_formats::SpriteFlags::new(case.texture_flags);
    }
    let resource = RenderResource {
        data: RenderResourceData::Sprite(&entry),
        ..template
    };
    let mut pool = construct(case, exe);
    let mut host = Host {
        resource,
        sorting: false,
        queries: 0,
        packets: Vec::new(),
        lights: Vec::new(),
        emitters: Vec::new(),
        fail_light: true,
        shadow_color: None,
    };
    let report = pool.draw_particles(request(case), exe, &mut host);
    assert_eq!(report.visited, 2);
    assert_eq!(report.omissions.len(), 1);
    assert_eq!(host.lights.len(), 1);
    assert_eq!(host.packets.len(), 1);
    assert_eq!(host.packets[0].payload, case.packets[1].payload);
    // The frame's light runs before flag10 and queue allocation.
    for omission in report.omissions {
        assert!(matches!(
            omission.error,
            ParticleDrawError::Host("required light writer")
        ));
        assert_eq!(
            pool.record(omission.id).unwrap().read_word(0x12).unwrap() & 0x10,
            0
        );
    }
}

/// Every class the animated branch admits holds the 00466CD0 wrapper at +08
/// in the retail table (00466960 for the unshadowed one), and the excluded
/// RET4 classes do not.
#[test]
fn animated_draw_classes_hold_the_466cd0_wrapper() {
    let (_, assets) = setup();
    let exe = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for vtable in super::ANIMATED_DRAW_VTABLES {
        assert_eq!(exe.word(vtable + 8).unwrap(), 0x0046_6cd0, "{vtable:08X}");
    }
    for vtable in super::UNSHADOWED_DRAW_VTABLES {
        assert_eq!(exe.word(vtable + 8).unwrap(), 0x0046_6960, "{vtable:08X}");
    }
    for vtable in [0x0052_6f80, 0x0052_8834] {
        assert_ne!(exe.word(vtable + 8).unwrap(), 0x0046_6cd0, "{vtable:08X}");
    }
}

/// An explosion's fireball under the water's plane over water is left to the
/// traced water; one above the plane, over dry ground, of another class or
/// with no terrain is drawn.
#[test]
fn fireballs_under_the_water_are_the_traced_waters() {
    use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
    use inf_formats::TERRAIN_GRID_SIDE;
    let ground = |byte: i8| {
        TerrainRuntime::from_raw_cells(
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
            0,
            std::iter::repeat(u16::from(byte as u8)).take(TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE),
        )
    };
    let (lake, land) = (ground(-4), ground(4));
    let explosion = super::super::splashes::CENTRAL_VTABLE;
    let under = [5000, (-50_i16) as u16, 7000];
    let over = [5000, 50, 7000];
    assert!(super::fireball_under_water(explosion, under, Some(&lake)));
    assert!(!super::fireball_under_water(explosion, over, Some(&lake)));
    assert!(!super::fireball_under_water(explosion, under, Some(&land)));
    assert!(!super::fireball_under_water(
        super::super::splashes::BOLT_VTABLE,
        under,
        Some(&lake)
    ));
    assert!(!super::fireball_under_water(explosion, under, None));
}
