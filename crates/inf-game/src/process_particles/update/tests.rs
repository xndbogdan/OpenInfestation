use super::*;
use crate::process_particles::{births::*, ParticleAllocationRequest, ParticleConstruction};
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TerrainEntry,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde::Deserialize;

mod drops;
mod ground;
mod impact_sparks;
mod projectile;
mod short_smoke;

fn no_terrain_request(
    process: &mut crate::process_particles::factories::ParticleFactoryProcessState,
    delta: i32,
) -> ParticleUpdateRequest<'_> {
    ParticleUpdateRequest {
        delta_microseconds: delta,
        load_factor: 65536,
        birth_inputs: ParticleUpdateBirthInputs {
            network_active: false,
            phase: Some(4),
            process,
        },
        terrain: None,
    }
}

#[derive(Default)]
struct Effects {
    explosion_calls: usize,
    impacts: Vec<ParticleSoundRequest>,
    fail: bool,
    /// Run 00465FA0 for the projectile classes; otherwise they stop as an
    /// unowned collision, as the default host does.
    walk_projectiles: bool,
    /// 00465FA0's actor sweeps: (slot, radius), with no actors supplied.
    sweeps: Vec<(u16, i32)>,
    /// 004887D0 calls: (packet, slot, source, replicate).
    radial: Vec<(u32, u16, u32, u32)>,
}

fn pool_slot(id: ParticleId) -> u16 {
    match id {
        ParticleId::Pool(slot) => slot,
        other => panic!("{other:?} is not a pool cell"),
    }
}
impl ParticleBirthAudio for Effects {
    type Error = &'static str;
    fn explosion_sound(
        &mut self,
        _: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.explosion_calls += 1;
        Ok(())
    }
}
impl ParticleUpdateHost for Effects {
    type Error = &'static str;
    fn impact_sound(
        &mut self,
        request: ParticleSoundRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.impacts.push(request);
        if self.fail {
            Err("required positional sound")
        } else {
            Ok(())
        }
    }

    fn collide_projectile(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        movement: [u16; 3],
        radius: i32,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<bool, ParticleUpdateError<Self::Error>> {
        if !self.walk_projectiles {
            return Err(ParticleUpdateError::Collision {
                id,
                vtable: particles.record(id)?.read_dword(0)?,
            });
        }
        self.sweeps.push((pool_slot(id), radius));
        particles.collide_projectile_ordered(
            id,
            movement,
            radius,
            &[],
            request,
            executable,
            rng,
            self,
        )
    }

    fn retire_factory_particle(
        &mut self,
        particles: &mut ProcessParticleRuntime,
        id: ParticleId,
        request: &mut ParticleUpdateRequest<'_>,
        executable: FrontendExecutable<'_>,
        rng: &mut u32,
    ) -> Result<(), ParticleUpdateError<Self::Error>> {
        particles.retire_factory_particle(id, request, executable, rng, self)
    }
}

impl crate::process_particles::impact::collision::ParticleProjectileHost for Effects {
    fn actor_geometry(
        &mut self,
        _: crate::process_particles::impact::collision::ParticleActorGeometryRequest,
        _: &mut crate::process_particles::impact::collision::ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<bool, Self::Error> {
        unreachable!("no actors are supplied")
    }
    fn furniture_query(
        &mut self,
        _: [u16; 3],
        _: i32,
        _: u32,
        _: &mut crate::process_particles::impact::collision::ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<
        Option<crate::process_particles::impact::collision::ParticleFurnitureContact>,
        Self::Error,
    > {
        Ok(None)
    }
    fn actor_impact(
        &mut self,
        _: ParticleId,
        _: crate::object_runtime::ActorHandle,
        _: u32,
        _: u32,
        _: &mut crate::process_particles::impact::collision::ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        unreachable!("no actors are supplied")
    }
    fn furniture_impact(
        &mut self,
        _: crate::process_particles::impact::collision::ParticleFurnitureContact,
        _: u32,
        _: &mut crate::process_particles::impact::collision::ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        unreachable!("no furniture is supplied")
    }
    fn terrain_impact(
        &mut self,
        _: u32,
        _: [u16; 3],
        _: u32,
        _: &mut crate::process_particles::impact::collision::ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        unreachable!("the projectile's own terrain arm is not reached")
    }
    fn radial_impact(
        &mut self,
        packet: u32,
        particle: ParticleId,
        source: u32,
        _: &mut crate::process_particles::impact::collision::ParticleCollisionContext<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        self.radial.push((packet, pool_slot(particle), source, 1));
        Ok(())
    }
}

#[derive(Deserialize)]
struct Receipt {
    fields: Vec<(usize, usize)>,
    cases: Vec<Case>,
    reclaims: Vec<ReclaimCase>,
}
#[derive(Deserialize)]
struct ReclaimCase {
    name: String,
    category: usize,
    unprotect: Vec<u16>,
    scratch: bool,
    returned_slot: Option<u16>,
    marker_slot: Option<u16>,
    active: Vec<u16>,
}
#[derive(Deserialize)]
struct Record {
    slot: u16,
    occupied: bool,
    fields: Vec<u32>,
}
#[derive(Deserialize)]
struct Impact {
    sound: u32,
    position: [u16; 3],
    volume: u32,
    pitch: u32,
    flags: u32,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    kind: String,
    particle_type: u32,
    rng: u32,
    flags: Option<u32>,
    position: Option<[u16; 3]>,
    velocity: Option<[u16; 3]>,
    deltas: Option<Vec<i32>>,
    load_factor: Option<u32>,
    plane: Option<i32>,
    height: Option<i8>,
    material: Option<u8>,
    vary_material: Option<bool>,
    world_flags: Option<u32>,
    wave_clock: Option<u32>,
    impact_selectors: Option<[u32; 5]>,
    spacing: Option<u16>,
    height_shift: Option<u32>,
    set_flags: Option<u16>,
    lifetime: Option<u16>,
    set_vtable: Option<u32>,
    pending_only: Option<bool>,
    future_cell_birth: Option<bool>,
    clear_after: Option<bool>,
    records: Vec<Record>,
    final_rng: u32,
    gravity: u32,
    motion_microseconds: u32,
    free_count: usize,
    pending: Vec<u16>,
    active: Vec<u16>,
    calls: Vec<(u32, u16)>,
    impact_audio: Vec<Impact>,
    audio_error_cleanup_count: usize,
    set_dwords: Option<Vec<(usize, u32)>>,
    sweeps: Option<Vec<(u16, i32)>>,
    radial: Option<Vec<(u32, u16, u32, u32)>>,
    objects: Option<Vec<(u32, [u16; 3])>>,
}

fn source(case: &Case) -> TerrainEntry {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    bytes[0..4].copy_from_slice(&case.plane.unwrap_or(0).to_le_bytes());
    for index in 0..TERRAIN_CELL_COUNT {
        let material = if case.vary_material.unwrap_or(false) {
            (index / 512) as u8 % 4
        } else {
            case.material.unwrap_or(0)
        };
        let cell = u16::from(case.height.unwrap_or(0) as u8) | (u16::from(material) << 8);
        let offset = TERRAIN_PREFIX_BYTES + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&cell.to_le_bytes());
    }
    let span = ByteSpan::new(0, bytes.len());
    parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0)
}

fn birth(
    particles: &mut ProcessParticleRuntime,
    case: &Case,
    process: &crate::process_particles::factories::ParticleFactoryProcessState,
    executable: FrontendExecutable<'_>,
    rng: &mut u32,
    effects: &mut Effects,
) {
    if case.future_cell_birth.unwrap_or(false) {
        // Match the supplied native free-stack permutation 0,399..1.
        particles.free.remove(0);
        particles.free.push(0);
    }
    let request = ParticleBirthRequest {
        particle_type: case.particle_type,
        position: case.position.unwrap_or([65530, 3200, 7]),
        velocity: Some(case.velocity.unwrap_or([250, 65136, 300])),
        flags: case.flags.unwrap_or(0),
        inputs: ParticleBirthInputs {
            network_active: false,
            phase: Some(4),
            process,
        },
    };
    match case.kind.as_str() {
        "debris" => {
            particles.birth_debris(request, executable, rng).unwrap();
        }
        "central" => {
            particles
                .birth_central(request, executable, rng, effects)
                .unwrap();
        }
        "trail" => {
            particles.birth_trail(request, executable, rng).unwrap();
        }
        "splash" => {
            particles.birth_splash(request, executable, rng).unwrap();
        }
        _ => panic!("unknown supplied producer"),
    }
    for slot in 0..super::super::PARTICLE_POOL_SIZE {
        if let Some(id) = particles.occupant(slot) {
            for (offset, value) in case.set_dwords.iter().flatten() {
                particles
                    .record_mut(id)
                    .unwrap()
                    .write_dword(*offset, *value)
                    .unwrap();
            }
            if let Some(flags) = case.set_flags {
                particles
                    .record_mut(id)
                    .unwrap()
                    .write_word(0x12, flags)
                    .unwrap();
            }
            if let Some(lifetime) = case.lifetime {
                particles
                    .record_mut(id)
                    .unwrap()
                    .write_word(0x10, lifetime)
                    .unwrap();
            }
            if let Some(vtable) = case.set_vtable {
                particles
                    .record_mut(id)
                    .unwrap()
                    .write_dword(0, vtable)
                    .unwrap();
            }
        }
    }
    if !case.pending_only.unwrap_or(false) {
        particles.transfer_pending();
    }
}

fn receipt() -> Receipt {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/particle_updates_native.json"
    ))
    .unwrap()
}

fn executable_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("particle native update receipts require the supported local retail executable")
}

#[test]
fn process_particles_updates_match_original_bytecode_for_all_owned_classes_and_teardown() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let sine_bytes = executable.mapped(0x0053_df00, 8192).unwrap().bytes;
    let sine: [u16; 4096] = std::array::from_fn(|index| {
        u16::from_le_bytes(sine_bytes[index * 2..index * 2 + 2].try_into().unwrap())
    });
    let receipt = receipt();
    let mut sound_selectors = std::collections::BTreeSet::new();
    for case in &receipt.cases {
        assert_update_case(
            case,
            &receipt.fields,
            executable,
            &sine,
            &mut sound_selectors,
        );
        if case.future_cell_birth.unwrap_or(false) {
            assert_eq!(case.calls, [(0x0048_a550, 0), (0x0048_a060, 399)]);
        }
    }
    assert_eq!(sound_selectors.into_iter().collect::<Vec<_>>(), [9, 10]);
}

/// 00526F20/00526F60 through the original pool update: gravity or the
/// selector-8 trail, the shared 00465FA0 collision walk with each class's
/// terrain contact, radius effects, and 004661B0's central particle.
#[test]
fn factory_particle_classes_match_original_update_collision_and_retirement() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let sine_bytes = executable.mapped(0x0053_df00, 8192).unwrap().bytes;
    let sine: [u16; 4096] = std::array::from_fn(|index| {
        u16::from_le_bytes(sine_bytes[index * 2..index * 2 + 2].try_into().unwrap())
    });
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../tests/fixtures/factory_particle_updates_native.json"
    ))
    .unwrap();
    let mut sound_selectors = std::collections::BTreeSet::new();
    let mut radial = 0;
    for case in &receipt.cases {
        let effects = assert_update_case(
            case,
            &receipt.fields,
            executable,
            &sine,
            &mut sound_selectors,
        );
        assert_eq!(
            Some(&effects.sweeps),
            case.sweeps.as_ref(),
            "{} actor sweeps",
            case.name
        );
        assert_eq!(
            Some(&effects.radial),
            case.radial.as_ref(),
            "{} radius effects",
            case.name
        );
        assert_eq!(
            case.objects.as_deref(),
            Some(&[][..]),
            "{} factory objects",
            case.name
        );
        radial += effects.radial.len();
    }
    assert_eq!(receipt.cases.len(), 24);
    assert_eq!(radial, 18);
    assert_eq!(sound_selectors.into_iter().collect::<Vec<_>>(), [10]);
}

/// The whole native comparison for one receipt case; returns the host.
fn assert_update_case(
    case: &Case,
    fields: &[(usize, usize)],
    executable: FrontendExecutable<'_>,
    sine: &[u16; 4096],
    sound_selectors: &mut std::collections::BTreeSet<u32>,
) -> Effects {
    let mut terrain = TerrainRuntime::new(
        &source(&case),
        TerrainSampling {
            spacing: if case.spacing.unwrap_or(128) == 256 {
                TerrainSpacing::Units256
            } else {
                TerrainSpacing::Units128
            },
            height_scale: if case.height_shift.unwrap_or(5) == 4 {
                TerrainHeightScale::Units16
            } else {
                TerrainHeightScale::Units32
            },
        },
    );
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = case.rng;
    let mut effects = Effects {
        walk_projectiles: case.sweeps.is_some(),
        ..Effects::default()
    };
    let mut birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
    };
    birth(
        &mut particles,
        &case,
        &birth_process,
        executable,
        &mut rng,
        &mut effects,
    );
    for delta in case.deltas.as_deref().unwrap_or(&[20000]) {
        let report = particles.tick_particles(
            ParticleUpdateRequest {
                delta_microseconds: *delta,
                load_factor: case.load_factor.unwrap_or(65536) as i32,
                birth_inputs: ParticleUpdateBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &mut birth_process,
                },
                terrain: Some(ParticleTerrainContext {
                    terrain: &mut terrain,
                    sine_quarter: &sine,
                    wave_clock: case.wave_clock.unwrap_or(37),
                    world_flags: case.world_flags.unwrap_or(0),
                    impact_selectors: case.impact_selectors.unwrap_or([1, 2, 3, 4, 6]),
                }),
            },
            executable,
            &mut rng,
            &mut effects,
        );
        assert!(
            report.omissions.is_empty(),
            "{}: {:?}",
            case.name,
            report.omissions
        );
    }
    if case.clear_after.unwrap_or(false) {
        particles.clear().unwrap();
    }
    assert_eq!(rng, case.final_rng, "{} RNG", case.name);
    assert_eq!(
        particles.update_globals.gravity as u32, case.gravity,
        "{} gravity",
        case.name
    );
    assert_eq!(
        particles.update_globals.motion_microseconds as u32, case.motion_microseconds,
        "{} motion",
        case.name
    );
    assert_eq!(
        particles.free_count(),
        case.free_count,
        "{} free cells",
        case.name
    );
    for (actual, expected) in [
        (&particles.active[1], &case.active),
        (&particles.pending[1], &case.pending),
    ] {
        assert_eq!(
            actual.iter().copied().collect::<Vec<_>>(),
            expected
                .iter()
                .map(|slot| ParticleId::Pool(*slot))
                .collect::<Vec<_>>(),
            "{} list order",
            case.name
        );
    }
    for record in &case.records {
        let id = ParticleId::Pool(record.slot);
        assert_eq!(
            particles.occupant(usize::from(record.slot)).is_some(),
            record.occupied,
            "{} occupancy {id:?}",
            case.name
        );
        for ((offset, width), expected) in fields.iter().zip(&record.fields) {
            let actual = match width {
                2 => particles
                    .record(id)
                    .unwrap()
                    .read_word(*offset)
                    .map(u32::from),
                4 => particles.record(id).unwrap().read_dword(*offset),
                _ => unreachable!(),
            };
            match actual {
                Ok(value) => assert_eq!(value, *expected, "{} {id:?} +{offset:02X}", case.name),
                Err(ParticleError::Unwritten { .. }) => assert_eq!(
                    *expected,
                    if *width == 2 { 0xa5a5 } else { 0xa5a5a5a5 },
                    "{} native must retain supplied unwritten heap pattern",
                    case.name
                ),
                Err(error) => panic!("{}: {error}", case.name),
            }
        }
    }
    let expected = case
        .impact_audio
        .iter()
        .map(|impact| ParticleSoundRequest {
            sound: impact.sound,
            position: impact.position,
            volume: impact.volume,
            pitch: impact.pitch,
            flags: impact.flags,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        effects.impacts, expected,
        "{} positional audio arguments",
        case.name
    );
    assert_eq!(
        effects.explosion_calls + effects.impacts.len(),
        case.audio_error_cleanup_count,
        "{} nullable audio error cleanup",
        case.name
    );
    sound_selectors.extend(effects.impacts.iter().map(|impact| impact.sound));
    effects
}

#[test]
fn process_particles_reclamation_matches_original_tail_selection_for_full_pool() {
    for case in receipt().reclaims {
        let mut particles = ProcessParticleRuntime::default();
        let request = ParticleAllocationRequest {
            payload_bytes: 52,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(4),
        };
        for _ in 0..400 {
            let id = particles.allocate(request).unwrap();
            particles
                .construct(
                    id,
                    ParticleConstruction {
                        category: case.category,
                        position: [1, 2, 3].into(),
                        velocity: Some([4, 5, 6].into()),
                        source_handle: 0x12345678,
                        flags: 0,
                    },
                )
                .unwrap();
        }
        particles.transfer_pending();
        for slot in case.unprotect {
            particles
                .record_mut(ParticleId::Pool(slot))
                .unwrap()
                .write_word(0x12, 0)
                .unwrap();
        }
        let id = particles.allocate(request).unwrap();
        assert_eq!(
            id == ParticleId::Scratch,
            case.scratch,
            "{} scratch",
            case.name
        );
        if let Some(slot) = case.returned_slot {
            assert_eq!(id, ParticleId::Pool(slot), "{} returned slot", case.name);
        }
        assert_eq!(
            particles.allocation_marker(),
            case.marker_slot,
            "{} marker",
            case.name
        );
        assert_eq!(
            particles
                .active(case.category)
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            case.active
                .into_iter()
                .map(ParticleId::Pool)
                .collect::<Vec<_>>(),
            "{} active order",
            case.name
        );
    }
}

#[test]
fn process_particles_missing_terrain_retains_manager_prefix_without_replay_until_clear() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 1;
    let mut birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(12)),
    };
    let inputs = ParticleBirthInputs {
        network_active: false,
        phase: Some(4),
        process: &birth_process,
    };
    let id = particles
        .birth_debris(
            ParticleBirthRequest {
                particle_type: 1,
                position: [0, 1000, 0],
                velocity: Some([0; 3]),
                flags: 0,
                inputs,
            },
            executable,
            &mut rng,
        )
        .unwrap()
        .unwrap();
    let life = particles.record(id).unwrap().read_word(0x10).unwrap();
    let mut effects = Effects::default();
    let report = particles.tick_particles(
        no_terrain_request(&mut birth_process, 2048),
        executable,
        &mut rng,
        &mut effects,
    );
    assert!(
        matches!(report.omissions[0].error, ParticleUpdateError::MissingTerrain { id: reached } if reached == id)
    );
    assert_eq!(
        particles.record(id).unwrap().read_word(0x10).unwrap(),
        life - 2
    );
    assert_eq!(
        particles.record(id).unwrap().read_word(0x12).unwrap() & 0xa0,
        0
    );
    particles
        .record_mut(id)
        .unwrap()
        .write_word(0x10, 1)
        .unwrap();
    let report = particles.tick_particles(
        no_terrain_request(&mut birth_process, 2048),
        executable,
        &mut rng,
        &mut effects,
    );
    assert_eq!(report.destroyed, 0);
    assert_eq!(report.stopped, 1);
    assert_eq!(particles.record(id).unwrap().read_word(0x10).unwrap(), 1);
    particles.clear().unwrap();
    assert_eq!(particles.stopped_phase(id), None);
    assert_eq!(particles.free_count(), 400);
    assert_eq!(
        particles.record(id).unwrap().read_dword(0).unwrap(),
        0x0052_8934
    );
}

#[test]
fn process_particles_clear_returns_physical_header_for_scratch_without_payload_destruction() {
    let mut particles = ProcessParticleRuntime::default();
    let allocated = particles
        .allocate(ParticleAllocationRequest {
            payload_bytes: 52,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(4),
        })
        .unwrap();
    assert_eq!(allocated, ParticleId::Pool(399));
    let scratch = particles
        .allocate(ParticleAllocationRequest {
            payload_bytes: 52,
            last_reclaim_category: 1,
            network_active: false,
            phase: Some(2),
        })
        .unwrap();
    particles
        .construct(
            scratch,
            ParticleConstruction {
                category: 1,
                position: [1, 2, 3].into(),
                velocity: None,
                source_handle: 7,
                flags: 0,
            },
        )
        .unwrap();
    particles
        .record_mut(scratch)
        .unwrap()
        .write_dword(0, 0xdeadbeef)
        .unwrap();
    assert_eq!(particles.occupant(399), Some(scratch));
    particles.clear().unwrap();
    assert_eq!(particles.free_count(), 400);
    assert_eq!(
        particles.record(scratch).unwrap().read_dword(0).unwrap(),
        0xdeadbeef
    );
    assert!(particles.pending(1).unwrap().is_empty());
}

#[test]
fn missing_birth_source_does_not_block_nonemitting_central_updates() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x1234_5678)),
    };
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 123;
    let mut effects = Effects::default();
    let id = particles
        .birth_central(
            ParticleBirthRequest {
                particle_type: 0,
                position: [0, 1000, 0],
                velocity: None,
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &process,
                },
            },
            executable,
            &mut rng,
            &mut effects,
        )
        .unwrap()
        .unwrap();
    let life = particles.record(id).unwrap().read_word(0x10).unwrap();
    let before_rng = rng;
    process.process_null_handle = None;
    let report = particles.tick_particles(
        ParticleUpdateRequest {
            delta_microseconds: 2048,
            load_factor: 65536,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: None,
                process: &mut process,
            },
            terrain: None,
        },
        executable,
        &mut rng,
        &mut effects,
    );
    assert!(report.omissions.is_empty());
    assert_eq!(report.updated, 1);
    assert_eq!(
        particles.record(id).unwrap().read_word(0x10).unwrap(),
        life - 2
    );
    assert_eq!(particles.free_count(), 399);
    assert_eq!(rng, before_rng);
    assert_eq!(process.process_null_handle, None);
}

#[test]
fn emitting_debris_keeps_manager_motion_and_allocation_before_missing_source() {
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x1234_5678)),
    };
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = 123;
    let id = particles
        .birth_debris(
            ParticleBirthRequest {
                particle_type: 0,
                position: [0, 1000, 0],
                velocity: Some([1000, 0, 0]),
                flags: 0,
                inputs: ParticleBirthInputs {
                    network_active: false,
                    phase: Some(4),
                    process: &process,
                },
            },
            executable,
            &mut rng,
        )
        .unwrap()
        .unwrap();
    particles
        .record_mut(id)
        .unwrap()
        .write_dword(0x34, 6)
        .unwrap();
    let life = particles.record(id).unwrap().read_word(0x10).unwrap();
    let before_rng = rng;
    process.process_null_handle = None;
    let mut effects = Effects::default();
    let result = particles.tick_particles(
        ParticleUpdateRequest {
            delta_microseconds: 20_000,
            load_factor: 65536,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut process,
            },
            terrain: None,
        },
        executable,
        &mut rng,
        &mut effects,
    );
    assert!(matches!(
        result.omissions[0].error,
        ParticleUpdateError::Source(ParticleBirthSourceError {
            producer: ParticleBirthProducer::Trail
        })
    ));
    assert_eq!(
        particles.record(id).unwrap().position().unwrap(),
        [19, 1000, 0]
    );
    assert_eq!(
        particles.record(id).unwrap().read_word(0x10).unwrap(),
        life - (20_000 >> 10)
    );
    assert_eq!(
        particles.record(id).unwrap().read_word(0x12).unwrap() & 0x80,
        0
    );
    assert_ne!(particles.update_globals.gravity, 0);
    assert_eq!(particles.free_count(), 398);
    assert_eq!(particles.allocation_marker(), Some(398));
    assert_eq!(particles.occupant(398), None);
    assert_eq!(rng, before_rng);
    assert!(effects.impacts.is_empty());
}

#[test]
fn splash_source_stop_retains_contact_sound_rng_and_new_allocation() {
    let case = receipt()
        .cases
        .into_iter()
        .find(|case| case.name == "water-sound-0")
        .unwrap();
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut terrain = TerrainRuntime::new(
        &source(&case),
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    );
    let sine = [0; 4096];
    let mut process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: None,
    };
    let mut particles = ProcessParticleRuntime::default();
    let mut rng = case.rng;
    let mut effects = Effects::default();
    let birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
    };
    birth(
        &mut particles,
        &case,
        &birth_process,
        executable,
        &mut rng,
        &mut effects,
    );
    let id = ParticleId::Pool(399);
    let life = particles.record(id).unwrap().read_word(0x10).unwrap();
    let result = particles.tick_particles(
        ParticleUpdateRequest {
            delta_microseconds: 20_000,
            load_factor: 65536,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut process,
            },
            terrain: Some(ParticleTerrainContext {
                terrain: &mut terrain,
                sine_quarter: &sine,
                wave_clock: 37,
                world_flags: 0,
                impact_selectors: [1, 2, 3, 4, 6],
            }),
        },
        executable,
        &mut rng,
        &mut effects,
    );
    assert!(matches!(
        result.omissions[0].error,
        ParticleUpdateError::Source(ParticleBirthSourceError {
            producer: ParticleBirthProducer::Splash
        })
    ));
    let mut expected_rng = case.rng;
    // Three original birth draws, then water/sound/pitch; no splash scale RNG.
    for _ in 0..6 {
        name_entry_rng_next(&mut expected_rng);
    }
    assert_eq!(rng, expected_rng);
    assert_eq!(particles.record(id).unwrap().read_word(6).unwrap(), 1000);
    assert_eq!(
        particles.record(id).unwrap().read_word(0x10).unwrap(),
        life - (20_000 >> 10)
    );
    assert_eq!(effects.impacts.len(), 1);
    assert_eq!(particles.free_count(), 398);
    assert_eq!(particles.allocation_marker(), Some(398));
    assert_eq!(particles.occupant(398), None);
}

#[test]
fn process_particles_required_impact_sound_failure_keeps_rng_and_contact_writes_before_splash() {
    let case = receipt()
        .cases
        .into_iter()
        .find(|case| case.name == "water-sound-0")
        .unwrap();
    assert_eq!(case.impact_audio.len(), 1);
    let raw = executable_bytes();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut terrain = TerrainRuntime::new(
        &source(&case),
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    );
    // Waves are disabled here; the sine block is provably unread.
    let sine = [0; 4096];
    let mut rng = case.rng;
    let mut particles = ProcessParticleRuntime::default();
    let mut effects = Effects {
        fail: true,
        ..Effects::default()
    };
    let id = ParticleId::Pool(399);
    let mut birth_process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(crate::object_runtime::ActorHandle::from_raw(0x12345678)),
    };
    birth(
        &mut particles,
        &case,
        &birth_process,
        executable,
        &mut rng,
        &mut effects,
    );
    let lifetime = particles.record(id).unwrap().read_word(0x10).unwrap();
    let result = particles.tick_particles(
        ParticleUpdateRequest {
            delta_microseconds: 20000,
            load_factor: 65536,
            birth_inputs: ParticleUpdateBirthInputs {
                network_active: false,
                phase: Some(4),
                process: &mut birth_process,
            },
            terrain: Some(ParticleTerrainContext {
                terrain: &mut terrain,
                sine_quarter: &sine,
                wave_clock: 37,
                world_flags: 0,
                impact_selectors: [1, 2, 3, 4, 6],
            }),
        },
        executable,
        &mut rng,
        &mut effects,
    );
    assert!(matches!(
        result.omissions[0].error,
        ParticleUpdateError::Audio("required positional sound")
    ));
    let mut expected_rng = case.rng;
    // Three birth draws, water selector draw, sound selector and pitch draws.
    for _ in 0..6 {
        name_entry_rng_next(&mut expected_rng);
    }
    assert_eq!(rng, expected_rng);
    assert_eq!(particles.record(id).unwrap().read_word(6).unwrap(), 1000);
    assert_eq!(
        particles.record(id).unwrap().read_word(0x10).unwrap(),
        lifetime - (20000 >> 10)
    );
    assert_eq!(particles.free_count(), 399);
    assert!(particles.pending(1).unwrap().is_empty());
    assert_eq!(effects.impacts.len(), 1);
}
