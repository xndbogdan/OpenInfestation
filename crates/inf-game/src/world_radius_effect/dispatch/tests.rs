use super::*;
use crate::{
    object_runtime::{BaseContext, ObjectClassKind, ObjectId},
    world_radius_effect::RadiusEffectBlock,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Origin {
    position: ObjectId,
    block_identity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Terrain,
    Actors,
    Network,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Visit {
    phase: Phase,
    request: WorldRadiusEffectRequest<Origin>,
    position: [u16; 3],
    block: RadiusEffectBlock,
    rng: u32,
}

struct Host {
    block: RadiusEffectBlock,
    visits: Vec<Visit>,
    furniture_count: u32,
    alternate_hits: u32,
    fail_at: Option<Phase>,
}

impl Host {
    fn visit(
        &mut self,
        phase: Phase,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Origin>,
        rng: &mut u32,
    ) -> Result<(), Phase> {
        let position = arena
            .object(request.origin.position)
            .unwrap()
            .base
            .position_words
            .unwrap();
        self.visits.push(Visit {
            phase,
            request,
            position,
            block: self.block,
            rng: *rng,
        });
        // Each service can mutate the original allocations before returning,
        // including on failure. Later phases must observe these live writes.
        self.block.strength += 10;
        arena
            .initialize_context(
                request.origin.position,
                BaseContext {
                    position_words: Some(position.map(|word| word.wrapping_add(1))),
                    placement_index: 0,
                },
            )
            .unwrap();
        *rng = rng.wrapping_add(1);
        if self.fail_at == Some(phase) {
            Err(phase)
        } else {
            Ok(())
        }
    }
}

impl WorldRadiusEffectHost for Host {
    type Error = Phase;
    type Source = Origin;

    fn furniture_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Origin>,
        rng: &mut u32,
    ) -> Result<u32, Phase> {
        self.visit(Phase::Terrain, arena, request, rng)?;
        Ok(self.furniture_count)
    }

    fn actor_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Origin>,
        rng: &mut u32,
    ) -> Result<ActorRadiusEffectOutcome, Phase> {
        self.visit(Phase::Actors, arena, request, rng)?;
        Ok(ActorRadiusEffectOutcome {
            alternate_hits: self.alternate_hits,
        })
    }

    fn replicate_radius_effect(
        &mut self,
        arena: &mut ObjectArena,
        request: WorldRadiusEffectRequest<Origin>,
        rng: &mut u32,
    ) -> Result<(), Phase> {
        self.visit(Phase::Network, arena, request, rng)
    }
}

fn fixture() -> (ObjectArena, WorldRadiusEffectRequest<Origin>, Host, u32) {
    let mut arena = ObjectArena::new();
    let position = arena.allocate(ObjectClassKind::Base);
    arena
        .initialize_context(
            position,
            BaseContext {
                position_words: Some([u16::MAX, 100, 200]),
                placement_index: 0,
            },
        )
        .unwrap();
    let request = WorldRadiusEffectRequest {
        origin: Origin {
            position,
            block_identity: 0x0053_1250,
        },
        damage_source: ActorHandle::from_raw(0x8123_4567),
        replicate: true,
    };
    let host = Host {
        block: RadiusEffectBlock::from_authored_bytes([0xe8, 3, 9, 0, 0, 0, 0xf4, 1, 0xc8, 0]),
        visits: Vec::new(),
        furniture_count: 7,
        alternate_hits: 2,
        fail_at: None,
    };
    (arena, request, host, 50)
}

#[test]
fn phases_keep_original_identity_and_observe_reached_source_and_rng_writes() {
    let (mut arena, request, mut host, mut rng) = fixture();
    let outcome = apply_world_radius_effect(&mut arena, request, &mut host, &mut rng).unwrap();
    assert_eq!(
        outcome,
        WorldRadiusEffectOutcome {
            furniture_count: 7,
            alternate_hits: 2,
            replicated: true,
        }
    );
    assert_eq!(
        host.visits
            .iter()
            .map(|visit| visit.phase)
            .collect::<Vec<_>>(),
        [Phase::Terrain, Phase::Actors, Phase::Network]
    );
    for (index, visit) in host.visits.iter().enumerate() {
        assert_eq!(visit.request, request);
        assert_eq!(
            visit.position,
            [
                u16::MAX.wrapping_add(index as u16),
                100 + index as u16,
                200 + index as u16
            ]
        );
        assert_eq!(visit.block.strength, 1000 + 10 * index as i16);
        assert_eq!(visit.rng, 50 + index as u32);
    }
    assert_eq!(rng, 53);
    assert_eq!(host.block.strength, 1030);
    assert_eq!(
        arena
            .object(request.origin.position)
            .unwrap()
            .base
            .position_words,
        Some([2, 103, 203])
    );
}

#[test]
fn network_admission_uses_only_nonzero_alternate_hits_and_replication_byte() {
    for furniture_count in [0, u32::MAX] {
        for alternate_hits in [0, 1, 0x8000_0000, u32::MAX] {
            for replicate in [false, true] {
                let (mut arena, mut request, mut host, mut rng) = fixture();
                request.replicate = replicate;
                host.furniture_count = furniture_count;
                host.alternate_hits = alternate_hits;
                let outcome =
                    apply_world_radius_effect(&mut arena, request, &mut host, &mut rng).unwrap();
                let should_replicate = alternate_hits != 0 && replicate;
                assert_eq!(outcome.furniture_count, furniture_count);
                assert_eq!(outcome.alternate_hits, alternate_hits);
                assert_eq!(outcome.replicated, should_replicate);
                assert_eq!(host.visits[0].phase, Phase::Terrain);
                assert_eq!(host.visits[1].phase, Phase::Actors);
                assert_eq!(host.visits.len(), if should_replicate { 3 } else { 2 });
            }
        }
    }
}

#[test]
fn each_required_phase_failure_keeps_its_prefix_without_calling_later_services() {
    for (phase, reached) in [(Phase::Terrain, 1), (Phase::Actors, 2), (Phase::Network, 3)] {
        let (mut arena, request, mut host, mut rng) = fixture();
        host.fail_at = Some(phase);
        let error =
            apply_world_radius_effect(&mut arena, request, &mut host, &mut rng).unwrap_err();
        match (phase, error) {
            (Phase::Terrain, WorldRadiusEffectError::Terrain(Phase::Terrain))
            | (Phase::Actors, WorldRadiusEffectError::Actors(Phase::Actors))
            | (Phase::Network, WorldRadiusEffectError::Network(Phase::Network)) => {}
            (_, error) => panic!("wrong failed phase: {error:?}"),
        }
        assert_eq!(host.visits.len(), reached);
        assert_eq!(host.visits.last().unwrap().phase, phase);
        assert_eq!(rng, 50 + reached as u32);
        assert_eq!(host.block.strength, 1000 + 10 * reached as i16);
        assert_eq!(
            arena
                .object(request.origin.position)
                .unwrap()
                .base
                .position_words,
            Some([
                u16::MAX.wrapping_add(reached as u16),
                100 + reached as u16,
                200 + reached as u16
            ])
        );
    }
}
