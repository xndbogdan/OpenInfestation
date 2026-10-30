//! Reached manager-phase loan after the native synchronous notice.
//! Supplied manager states are independent of retained process globals.
use super::phase_fixture::FixtureSessionPhase;
use super::*;

#[derive(Clone, Copy)]
enum NoticeAction {
    Keep,
    Offline(u32),
    OfflineThenError(u32),
}
struct Host {
    phase: FixtureSessionPhase,
    action: NoticeAction,
    phase_error: bool,
    order: Vec<&'static str>,
}
impl ParticleFactoryHost for Host {
    type Error = &'static str;
    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        self.order.push("phase");
        let value = self.phase.current();
        if self.phase_error {
            Err("phase")
        } else {
            Ok(value)
        }
    }
    fn network_notice(
        &mut self,
        _: ParticleCreationNotice,
        _: &mut ProcessParticleRuntime,
        context: &mut ParticleFactoryContext<'_, '_>,
        sources: &mut ParticleFactorySources,
    ) -> Result<(), Self::Error> {
        self.order.push("notice");
        match self.action {
            NoticeAction::Keep => Ok(()),
            NoticeAction::Offline(phase) | NoticeAction::OfflineThenError(phase) => {
                context.process.network_flag = 0;
                self.phase.set(Some(phase));
                sources.position = [77, 88, 99].into();
                if matches!(self.action, NoticeAction::OfflineThenError(_)) {
                    Err("notice")
                } else {
                    Ok(())
                }
            }
        }
    }
    fn impact_489600(
        &mut self,
        _: ParticleFactoryImpactRequest,
        _: &mut ProcessParticleRuntime,
        _: &mut ParticleFactoryContext<'_, '_>,
    ) -> Result<(), Self::Error> {
        panic!("unreached impact")
    }
}
fn host(phase: Option<u32>, action: NoticeAction) -> Host {
    Host {
        phase: FixtureSessionPhase::supplied(phase),
        action,
        phase_error: false,
        order: Vec::new(),
    }
}
fn create(
    host: &mut Host,
    network: u32,
    flags: u32,
) -> (
    Result<Option<ParticleId>, ParticleFactoryError<&'static str>>,
    ProcessParticleRuntime,
    ParticleFactoryProcessState,
    ParticleFactorySources,
    u32,
) {
    let raw = super::tests::raw_executable();
    let mut pool = ProcessParticleRuntime::default();
    let mut process = ParticleFactoryProcessState {
        network_flag: network,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(ActorHandle::from_raw(7)),
    };
    let mut sources = ParticleFactorySources {
        position: [10, 20, 30].into(),
        separate_velocity: None,
    };
    let mut rng = 123;
    let result = pool.create_factory_particle(
        ParticleFactoryRequest {
            kind: ParticleFactoryKind::Factory466710,
            selector: 2,
            source_handle: ActorHandle::from_raw(7),
            flags,
            velocity_source: ParticleVelocitySource::SamePosition,
        },
        &mut ParticleFactoryContext {
            process: &mut process,
            rng: &mut rng,
            executable: FrontendExecutable::parse(&raw).unwrap(),
        },
        &mut sources,
        host,
    );
    (result, pool, process, sources, rng)
}
#[test]
fn post_notice_offline_phase_is_fresh_and_allocation_uses_the_changed_source() {
    let mut host = host(Some(3), NoticeAction::Offline(4));
    let (result, pool, process, _, rng) = create(&mut host, 1, 0);
    let id = result.unwrap().unwrap();
    assert!(matches!(id, ParticleId::Pool(_)));
    assert_eq!(host.order, ["notice", "phase"]);
    assert_eq!(host.phase.reads(), 1);
    assert_eq!(process.network_flag, 0);
    assert_eq!(pool.record(id).unwrap().position().unwrap(), [77, 88, 99]);
    assert_eq!(pool.free_count(), 399);
    assert!(pool.pending(3).unwrap().contains(&id));
    assert_eq!(rng, 123); //466710 has no4665D0 random lifetime suffix.
}
#[test]
fn full_dword_online_path_bypasses_missing_or_failing_manager_phase() {
    let mut host = host(None, NoticeAction::Keep);
    host.phase_error = true;
    let (result, pool, _, _, rng) = create(&mut host, 0x100, 1);
    assert!(matches!(result.unwrap(), Some(ParticleId::Pool(_))));
    assert!(host.order.is_empty());
    assert_eq!(host.phase.reads(), 0);
    assert_eq!(pool.free_count(), 399);
    assert_eq!(rng, 123);
}
#[test]
fn post_notice_non_gameplay_phase_selects_scratch_without_pool_publication() {
    let mut host = host(Some(4), NoticeAction::Offline(3));
    let (result, pool, process, _, rng) = create(&mut host, 1, 0);
    assert_eq!(result.unwrap(), Some(ParticleId::Scratch));
    assert_eq!(host.order, ["notice", "phase"]);
    assert_eq!(host.phase.reads(), 1);
    assert_eq!(process.network_flag, 0);
    assert_eq!(pool.free_count(), 400);
    assert_eq!(pool.allocation_marker(), None);
    assert!(pool.pending(3).unwrap().is_empty());
    //Native non4 allocation returns the scratch sentinel, not factoryNULL.
    //The existing normal suffix still writes its payload.
    assert_eq!(
        pool.record(ParticleId::Scratch)
            .unwrap()
            .position()
            .unwrap(),
        [77, 88, 99]
    );
    assert_eq!(
        pool.record(ParticleId::Scratch)
            .unwrap()
            .read_dword(0x3c)
            .unwrap(),
        2
    );
    assert_eq!(rng, 123);
}
#[test]
fn failed_notice_retains_mutations_without_phase_read_or_allocation() {
    let mut host = host(Some(4), NoticeAction::OfflineThenError(3));
    let (result, pool, process, sources, rng) = create(&mut host, 1, 0);
    assert!(matches!(
        result,
        Err(ParticleFactoryError::Network("notice"))
    ));
    assert_eq!(host.order, ["notice"]);
    assert_eq!(host.phase.snapshot(), Some(3));
    assert_eq!(host.phase.reads(), 0);
    assert_eq!(process.network_flag, 0);
    assert_eq!(sources.position, [77, 88, 99].into());
    assert_eq!(pool.free_count(), 400);
    assert_eq!(pool.allocation_marker(), None);
    assert_eq!(rng, 123);
}
#[test]
fn failed_phase_service_preserves_notice_prefix_before_pool_or_payload() {
    let mut host = host(Some(4), NoticeAction::Offline(4));
    host.phase_error = true;
    let (result, pool, process, sources, rng) = create(&mut host, 1, 0);
    assert!(matches!(result, Err(ParticleFactoryError::Phase("phase"))));
    assert_eq!(host.order, ["notice", "phase"]);
    assert_eq!(host.phase.reads(), 1);
    assert_eq!(process.network_flag, 0);
    assert_eq!(sources.position, [77, 88, 99].into());
    assert_eq!(pool.free_count(), 400);
    assert_eq!(pool.allocation_marker(), None);
    assert!(pool.pending(3).unwrap().is_empty());
    assert_eq!(rng, 123);
}

#[test]
fn complete_native_allocator_phase_gate_and_manager_getter_remain_source_guarded() {
    use sha2::{Digest, Sha256};
    let raw = super::tests::raw_executable();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    //BF09 pins the original PE layout: .text raw offset equals RVA.
    //FrontendExecutable::mapped is data-only and is not used for code.
    let text_bytes = |address: u32, length: usize| {
        let offset = (address - 0x0040_0000) as usize;
        &raw[offset..offset + length]
    };
    //Four complete original bodies. No instruction execution is asserted.
    for (start, end, hash) in [
        (
            0x00464ed0,
            0x00464f60,
            "500bc4d63165fc4713c5fa2690a94af18b334a1548763dc36836b1e74f1172b6",
        ),
        (
            0x00466100,
            0x0046610e,
            "049de42a7991720b8a5be11b8e13a114a2f64b563d700d4ee9d7eea8d7bf9576",
        ),
        (
            0x004665d0,
            0x00466703,
            "d468a53f5f7e6583c23f55cb6ee951fb347f8b8ff87497266e393b5fdd3193fd",
        ),
        (
            0x004391b0,
            0x004391b6,
            "1214fde4371880e5b9ef3d887378d1907f5d0d34a321544615a7840419c65700",
        ),
    ] {
        let bytes = text_bytes(start, (end - start) as usize);
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), hash);
    }
    assert_eq!(text_bytes(0x0046_4edc, 2), &[0x75, 0x0e]);
    assert_eq!(text_bytes(0x0046_4ede, 5), &[0xe8, 0xcd, 0x42, 0xfd, 0xff]);
    assert_eq!(
        text_bytes(0x0046_4ee3, 7),
        &[0x83, 0xb8, 0x4c, 0x06, 0x00, 0x00, 0x04]
    );
    assert_eq!(text_bytes(0x0046_4eea, 2), &[0x75, 0x40]);
    assert_eq!(
        text_bytes(0x0043_91b0, 6),
        &[0xa1, 0x2c, 0x6f, 0x54, 0x00, 0xc3]
    );
}
