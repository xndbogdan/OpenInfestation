use super::*;

fn executable(bytes: &[u8]) -> FrontendExecutable<'_> {
    FrontendExecutable::parse(bytes).unwrap()
}

#[derive(Default)]
struct Host {
    notices: Vec<ParticleCreationNotice>,
    impacts: usize,
    fail_impact: Option<usize>,
    phase_after_notice: Option<u32>,
    phase: crate::process_particles::factories::phase_fixture::FixtureSessionPhase,
}

impl ParticleFactoryHost for Host {
    type Error = &'static str;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(self.phase.current())
    }
    fn network_notice(
        &mut self,
        request: ParticleCreationNotice,
        _: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        self.notices.push(request);
        if let Some(phase) = self.phase_after_notice {
            context.process.network_flag = 0;
            self.phase.set(Some(phase));
            sources.position = ParticleVectorBytes([None; 6]);
        }
        Ok(())
    }
    fn impact_489600(
        &mut self,
        request: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        assert_eq!((request.selector, request.flags), (0, 0));
        self.impacts += 1;
        if self.fail_impact == Some(self.impacts) {
            Err("impact")
        } else {
            Ok(())
        }
    }
}

pub(super) fn raw_executable() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}

fn sources() -> ParticleFactorySources {
    ParticleFactorySources {
        position: [10, 20, 30].into(),
        separate_velocity: Some([40, 50, 60].into()),
    }
}

fn state() -> ParticleFactoryProcessState {
    ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(7)),
    }
}

fn request(kind: ParticleFactoryKind, selector: u32) -> ParticleFactoryRequest {
    ParticleFactoryRequest {
        kind,
        selector,
        source_handle: ActorHandle::from_raw(7),
        flags: 0,
        velocity_source: ParticleVelocitySource::SamePosition,
    }
}

#[test]
fn native_startup_does_not_authorize_offline_allocation_or_snapshot_later_network_writes() {
    let raw = raw_executable();
    let mut pool = ProcessParticleRuntime::default();
    let mut process = ParticleFactoryProcessState::native_startup();
    let mut sources = sources();
    let before_sources = sources.clone();
    let mut rng = 123;
    let mut host = Host::default();
    host.phase.set(None);
    let mut context = ParticleFactoryContext {
        process: &mut process,
        rng: &mut rng,
        executable: executable(&raw),
    };
    let mut create = request(ParticleFactoryKind::Factory466710, 2);
    create.source_handle = ActorHandle::from_raw(0);
    assert!(matches!(
        pool.create_factory_particle(create, &mut context, &mut sources, &mut host),
        Err(ParticleFactoryError::Particle(ParticleError::MissingPhase))
    ));
    assert_eq!(pool.free_count(), 400);
    assert!(host.notices.is_empty());
    assert_eq!(sources, before_sources);
    assert_eq!(*context.rng, 123);
    assert_eq!(host.phase.snapshot(), None);

    // A full-DWORD nonzero network value bypasses the unavailable phase at
    // the actual allocator and reaches the real creation notice first.
    context.process.network_flag = 0x100;
    let particle = pool
        .create_factory_particle(create, &mut context, &mut sources, &mut host)
        .unwrap()
        .unwrap();
    assert!(matches!(particle, ParticleId::Pool(_)));
    assert_eq!(pool.free_count(), 399);
    assert_eq!(host.notices.len(), 1);
    assert_eq!(host.notices[0].event, 0x17);
    assert_eq!(host.notices[0].source_handle, None);
    assert_eq!(host.phase.snapshot(), None);
}

#[test]
fn invalid_full_dword_selectors_bypass_inputs_and_notification_rereads_allocation_state() {
    let raw = raw_executable();
    let mut pool = ProcessParticleRuntime::default();
    let mut state = state();
    state.process_null_handle = None;
    let mut sources = sources();
    let mut rng = 123;
    let mut context = ParticleFactoryContext {
        process: &mut state,

        rng: &mut rng,
        executable: executable(&raw),
    };
    let mut host = Host::default();
    host.phase.set(None);
    for kind in [
        ParticleFactoryKind::Factory4665D0,
        ParticleFactoryKind::Factory466710,
        ParticleFactoryKind::Factory466850,
    ] {
        assert_eq!(
            pool.create_factory_particle(
                request(kind, 0x10002),
                &mut context,
                &mut sources,
                &mut host
            )
            .unwrap(),
            None
        );
    }
    assert_eq!(pool.free_count(), 400);
    context.process.network_flag = 1;
    context.process.process_null_handle = Some(ActorHandle::from_raw(7));
    host.phase_after_notice = Some(3);
    let id = pool
        .create_factory_particle(
            request(ParticleFactoryKind::Factory466710, 2),
            &mut context,
            &mut sources,
            &mut host,
        )
        .unwrap()
        .unwrap();
    assert_eq!(id, ParticleId::Scratch);
    assert_eq!(host.notices[0].event, 0x17);
    assert_eq!(host.notices[0].source_handle, None);
    assert_eq!(
        pool.record(id).unwrap().source_bytes::<6>(4).unwrap(),
        [None; 6]
    );
    assert_eq!(
        pool.record(id).unwrap().source_bytes::<6>(0x0a).unwrap(),
        [None; 6]
    );
    assert!(pool.pending(3).unwrap().is_empty());
}

#[test]
fn lazy_velocity_failure_follows_birth_publication_and_flags_zero_two_retain_allocation() {
    let raw = raw_executable();
    let mut pool = ProcessParticleRuntime::default();
    let mut state = state();
    let mut sources = sources();
    sources.separate_velocity = None;
    let mut rng = 0;
    let mut context = ParticleFactoryContext {
        process: &mut state,

        rng: &mut rng,
        executable: executable(&raw),
    };
    let mut host = Host::default();
    let mut create = request(ParticleFactoryKind::Factory466850, 8);
    create.velocity_source = ParticleVelocitySource::Separate;
    assert!(matches!(
        pool.create_factory_particle(create, &mut context, &mut sources, &mut host),
        Err(ParticleFactoryError::MissingVelocitySource)
    ));
    let partial = ParticleId::Pool(399);
    assert_eq!(
        pool.record(partial).unwrap().position().unwrap(),
        [10, 20, 30]
    );
    assert_eq!(
        pool.record(partial).unwrap().read_dword(0).unwrap(),
        0x526ed8
    );
    assert!(pool.pending(3).unwrap().contains(&partial));
    for flags in [0, 2] {
        let id = pool
            .create_factory_particle(
                request(ParticleFactoryKind::Factory466850, 8),
                &mut context,
                &mut sources,
                &mut host,
            )
            .unwrap()
            .unwrap();
        let free = pool.free_count();
        pool.delete_factory_particle(
            ParticleDeleteRequest {
                particle: id,
                flags,
            },
            &mut context,
            &mut host,
        )
        .unwrap();
        assert_eq!(pool.free_count(), free);
        assert!(!pool.pending(3).unwrap().contains(&id));
        let ParticleId::Pool(slot) = id else {
            unreachable!()
        };
        assert_eq!(pool.occupant(slot as usize), Some(id));
        assert_eq!(pool.record(id).unwrap().read_dword(0).unwrap(), 0x526ed8);
    }
}

#[test]
fn reclaim_uses_live_efc_impact_service_and_keeps_late_failure_prefix() {
    let raw = raw_executable();
    let mut pool = ProcessParticleRuntime::default();
    let mut state = state();
    let mut sources = sources();
    let mut rng = 0;
    let mut context = ParticleFactoryContext {
        process: &mut state,

        rng: &mut rng,
        executable: executable(&raw),
    };
    let mut host = Host::default();
    for _ in 0..400 {
        pool.create_factory_particle(
            request(ParticleFactoryKind::Factory466710, 2),
            &mut context,
            &mut sources,
            &mut host,
        )
        .unwrap();
    }
    pool.transfer_pending();
    let tail = *pool.active(3).unwrap().back().unwrap();
    pool.record_mut(tail).unwrap().write_word(0x12, 0).unwrap();
    pool.record_mut(tail)
        .unwrap()
        .write_word(0x0a, 100)
        .unwrap();
    pool.record_mut(tail).unwrap().write_word(0x0c, 0).unwrap();
    pool.record_mut(tail).unwrap().write_word(0x0e, 0).unwrap();
    host.fail_impact = Some(3);
    assert!(matches!(
        pool.create_factory_particle(
            request(ParticleFactoryKind::Factory466710, 3),
            &mut context,
            &mut sources,
            &mut host
        ),
        Err(ParticleFactoryError::Impact("impact"))
    ));
    assert_eq!(host.impacts, 3);
    assert_eq!(pool.free_count(), 0);
    assert!(pool.active(3).unwrap().contains(&tail));
    assert_eq!(pool.record(tail).unwrap().read_dword(0).unwrap(), 0x526efc);
    let scale = f32::from_bits(dword(context.executable, 0x526ef8).unwrap()) as u16;
    assert_eq!(
        pool.record(tail).unwrap().position().unwrap(),
        [10 + 2 * scale, 20, 30]
    );
}

#[test]
fn reclaim_flag_admission_is_lazy_and_reads_only_the_final_category_low_byte() {
    for (category, low, reclaimed) in [(0, None, true), (1, Some(0), true), (1, Some(0x80), false)]
    {
        let mut pool = ProcessParticleRuntime::default();
        let allocation = ParticleAllocationRequest {
            payload_bytes: 0x40,
            last_reclaim_category: -1,
            network_active: false,
            phase: Some(4),
        };
        for _ in 0..400 {
            let id = pool.allocate(allocation).unwrap();
            pool.construct(
                id,
                ParticleConstruction {
                    category,
                    position: [1, 2, 3].into(),
                    velocity: None,
                    source_handle: 0,
                    flags: 0,
                },
            )
            .unwrap();
        }
        pool.transfer_pending();
        let tail = *pool.active(category).unwrap().back().unwrap();
        pool.record_mut(tail)
            .unwrap()
            .copy_bytes(0x12, &[low, None])
            .unwrap();
        let selected = pool
            .allocate(ParticleAllocationRequest {
                last_reclaim_category: 1,
                ..allocation
            })
            .unwrap();
        assert_eq!(selected, if reclaimed { tail } else { ParticleId::Scratch });
        assert_eq!(
            pool.record(tail).unwrap().source_bytes::<2>(0x12).unwrap(),
            [low, None]
        );
    }
}

#[test]
fn intermediate_animation_class_after_header_error_has_its_real_normal_destructor() {
    let mut raw = raw_executable();
    let offset = executable(&raw)
        .mapped(0x539f28, 4)
        .unwrap()
        .file_span
        .start;
    raw[offset..offset + 4].copy_from_slice(&0xdead0000_u32.to_le_bytes());
    let mut pool = ProcessParticleRuntime::default();
    let mut state = state();
    let mut sources = sources();
    let mut rng = 0;
    let mut context = ParticleFactoryContext {
        process: &mut state,

        rng: &mut rng,
        executable: executable(&raw),
    };
    let mut host = Host::default();
    for _ in 0..400 {
        assert!(matches!(
            pool.create_factory_particle(
                request(ParticleFactoryKind::Factory466710, 2),
                &mut context,
                &mut sources,
                &mut host
            ),
            Err(ParticleFactoryError::Executable(_))
        ));
    }
    pool.transfer_pending();
    let tail = *pool.active(3).unwrap().back().unwrap();
    assert_eq!(pool.record(tail).unwrap().read_dword(0).unwrap(), 0x526f40);
    pool.record_mut(tail).unwrap().write_word(0x12, 0).unwrap();
    assert!(matches!(
        pool.create_factory_particle(
            request(ParticleFactoryKind::Factory466710, 2),
            &mut context,
            &mut sources,
            &mut host
        ),
        Err(ParticleFactoryError::Executable(_))
    ));
    assert_eq!(host.impacts, 0);
    assert!(!pool.active(3).unwrap().contains(&tail));
    assert!(pool.pending(3).unwrap().contains(&tail));
    pool.delete_factory_particle(
        ParticleDeleteRequest {
            particle: tail,
            flags: 1,
        },
        &mut context,
        &mut host,
    )
    .unwrap();
    assert_eq!(pool.free_count(), 1);
    assert_eq!(pool.record(tail).unwrap().read_dword(0).unwrap(), 0x526ed8);
}

/// A gun's bolt in flight reads back for the tracer's light with the sprite
/// its frame shows and its scale.
#[test]
fn bolts_read_back_with_their_sprite() {
    let raw = raw_executable();
    let mut pool = ProcessParticleRuntime::default();
    let mut process = state();
    let mut sources = sources();
    let mut rng = 123;
    let mut host = Host::default();
    let mut context = ParticleFactoryContext {
        process: &mut process,
        rng: &mut rng,
        executable: executable(&raw),
    };
    assert!(pool.bolts(executable(&raw)).is_empty());
    pool.create_factory_particle(
        request(ParticleFactoryKind::Factory466710, 2),
        &mut context,
        &mut sources,
        &mut host,
    )
    .unwrap()
    .unwrap();
    let bolts = pool.bolts(executable(&raw));
    assert_eq!(bolts.len(), 1);
    assert_eq!(bolts[0].position, [10, 20, 30]);
    // Selector 2's table 00539D28 shows sprite 2, at scale 0x1C0.
    assert_eq!((bolts[0].sprite, bolts[0].scale), (2, 1.75));
}
