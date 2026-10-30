use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    process_sound::ProcessSoundRuntime,
    terrain_runtime::{TerrainHeightScale, TerrainRuntime, TerrainSampling, TerrainSpacing},
    world_furniture_effect::{
        resident::{ResidentFurnitureExecutor, ResidentFurniturePlacement},
        FurnitureCallbackContext, FurnitureCallbackRequest, FurnitureEffectExecutorError,
        FurnitureEffectExecutorHost, FurnitureEffectProgram, FurnitureEffectQueue,
        FurnitureExecutionPlacement, FurnitureQueueRequest,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TerrainResourceLookup,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

fn assets() -> FrontendAssets {
    FrontendAssets::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
        .unwrap()
}
fn empty_terrain() -> TerrainRuntime {
    let mut raw = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        raw[TERRAIN_PREFIX_BYTES + index * 2] = (-1_i8) as u8;
    }
    let span = ByteSpan::new(0, raw.len());
    let bank = parse_terrain_bank(
        &raw,
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

#[test]
fn supported_loader_storage_is_zero_before_any_query_constructor() {
    let assets = assets();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
    assert_eq!(process.fallback, [0; 4]);
    assert_eq!(process.query_init_mask, 0);
    assert_eq!(process.query_bases, [MenuBasis { words: [0; 10] }; 5]);
    assert_ne!(process.query_bases[0], MenuBasis::INITIAL);
    assert!(executable.mapped(0x0054_83c4, 4).is_err());
    assert!(executable.mapped(0x0055_f0dc, 1).is_err());
}

#[test]
fn state_byte_and_ordered_clear_preserve_the_other_process_fields() {
    let assets = assets();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
    //Explicit supplied current-record fixture; not a production default.
    process.fallback = [0x34, 0x12, 0xa4, 37];
    process.query_init_mask = 0x81;
    process.query_bases[0].words[4] = 0x7654_3210;
    process.write_fallback_state(0xb9);
    assert_eq!(process.fallback, [0x34, 0x12, 0xb9, 37]);
    assert_eq!(
        process.fallback_payload(),
        TerrainSparsePayload {
            state: 0xb9,
            resource_lookup: TerrainResourceLookup::from_raw(37),
        }
    );
    process.clear_fallback_payload();
    assert_eq!(process.fallback, [0x34, 0x12, 0, 0]);
    assert_eq!(process.query_init_mask, 0x81);
    assert_eq!(process.query_bases[0].words[4], 0x7654_3210);
}

const PROGRAM: u32 = 0x1000;
//Only the program allocation is an explicit supplied fixture. Resource,
//fallback, sound and callback operations forward to the actual resident host.
struct Programs<'a> {
    resident: ResidentFurnitureExecutor<'a>,
    words: Vec<u32>,
}
impl<'a> FurnitureEffectExecutorHost for Programs<'a> {
    type Error = String;
    type Placement = ResidentFurniturePlacement<'a>;
    fn program_word(&mut self, address: u32) -> Result<u32, String> {
        let offset = address
            .checked_sub(PROGRAM)
            .ok_or("fixture program before allocation")?;
        if offset & 3 != 0 {
            return Err("fixture unaligned program read".into());
        }
        self.words
            .get(offset as usize / 4)
            .copied()
            .ok_or_else(|| "fixture program exhausted".into())
    }
    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<FurnitureExecutionPlacement<Self::Placement>>, String> {
        self.resident.placement_slot(lookup)
    }
    fn preload_model_if_required(&mut self, model_slot: u16) -> Result<(), String> {
        self.resident.preload_model_if_required(model_slot)
    }
    fn fallback_sparse_payload(&mut self) -> Result<TerrainSparsePayload, String> {
        self.resident.fallback_sparse_payload()
    }
    fn write_fallback_sparse_state(&mut self, state: u8) -> Result<(), String> {
        self.resident.write_fallback_sparse_state(state)
    }
    fn clear_fallback_sparse_payload(&mut self) -> Result<(), String> {
        self.resident.clear_fallback_sparse_payload()
    }
    fn callback(
        &mut self,
        queue: &mut FurnitureEffectQueue,
        index: usize,
        terrain: &mut TerrainRuntime,
        context: &mut FurnitureCallbackContext,
        request: FurnitureCallbackRequest<Self::Placement>,
        rng: &mut u32,
    ) -> Result<bool, String> {
        self.resident
            .callback(queue, index, terrain, context, request, rng)
    }
}

fn run_program(
    process: &mut ProcessFurnitureQueryState,
    words: Vec<u32>,
) -> (
    Result<
        crate::world_furniture_effect::FurnitureExecutorOutcome,
        FurnitureEffectExecutorError<String>,
    >,
    FurnitureEffectQueue,
    u32,
    usize,
) {
    let assets = assets();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let mut terrain = empty_terrain();
    assert!(!terrain.cell_at([128, 0]).has_sparse_record());
    let mut queue = FurnitureEffectQueue::default();
    queue
        .enqueue(
            &terrain,
            FurnitureQueueRequest {
                xz: [128, 0],
                program: FurnitureEffectProgram::new(PROGRAM).unwrap(),
            },
        )
        .unwrap();
    let mut rng = 0x1234_5678;
    let result = queue.tick(
        &mut terrain,
        1,
        &mut Programs {
            resident: ResidentFurnitureExecutor {
                resident: &resident,
                executable,
                sounds: &mut sounds,
                process,
                effects: &mut crate::world_furniture_effect::RequiredTestFurnitureRadius,
                chain: crate::world_furniture_effect::FurnitureChainLoan {
                    policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                        crate::terrain_runtime::TerrainSpacing::Units256,
                    ),
                    detail: 0,
                    world_flags: None,
                    network_active: false,
                },
            },
            words,
        },
        &mut rng,
    );
    (result, queue, rng, sounds.pending_len())
}

#[test]
fn real_resident_state_and_timer_callbacks_share_the_current_collision_payload() {
    let assets = assets();
    let mut process = ProcessFurnitureQueryState::native_startup(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    process.fallback = [0x34, 0x12, 0xa4, 37];
    let (result, queue, rng, sounds) = run_program(
        &mut process,
        vec![0, 0x0043_7650, 1, 0, 0x0043_7780, 0, 0, 0x0043_7750, 0],
    );
    let outcome = result.unwrap();
    assert_eq!(outcome.callbacks, 3);
    assert_eq!(outcome.removed_records, 1);
    assert!(queue.records().is_empty());
    assert_eq!(process.fallback, [0x34, 0x12, 0xad, 37]);
    //The test-only collision census consumes this same current payload view.
    assert_eq!(process.fallback_payload().resource_lookup.raw(), 37);
    assert_eq!(rng, 0x1234_5678);
    assert_eq!(sounds, 0);
}

#[test]
fn actual_required_callback_retains_prior_state_timer_cursor_and_rng_prefix() {
    let assets = assets();
    let mut process = ProcessFurnitureQueryState::native_startup(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    process.fallback = [0x34, 0x12, 0xa4, 37];
    let (result, queue, rng, sounds) = run_program(
        &mut process,
        vec![0, 0x0043_7650, 1, 0, 0x0043_7780, 0, 0, 0x0043_75d0, 7],
    );
    assert!(
        matches!(result, Err(FurnitureEffectExecutorError::Host(ref error))
        if error.contains("004375D0"))
    );
    assert_eq!(process.fallback, [0x34, 0x12, 0xad, 37]);
    assert_eq!(queue.records().len(), 1);
    assert_eq!(queue.records()[0].program.address(), PROGRAM + 24);
    assert_eq!(queue.records()[0].remaining_microseconds, 7_000_000);
    assert_eq!(rng, 0x1234_5678);
    assert_eq!(sounds, 0);
}

#[test]
fn clear_callback_and_queue_cleanup_do_not_rebuild_process_storage() {
    let assets = assets();
    let mut process = ProcessFurnitureQueryState::native_startup(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    process.fallback = [0x34, 0x12, 0xa4, 37];
    process.query_init_mask = 1;
    process.query_bases[0].words[4] = 0x7654_3210;
    let (result, mut queue, rng, sounds) =
        run_program(&mut process, vec![0, 0x0043_6e00, 0, 0, 0x0043_7750, 0]);
    assert_eq!(result.unwrap().callbacks, 2);
    queue.clear(); //Actual native436B80-owned portable queue cleanup.
    assert_eq!(process.fallback, [0x34, 0x12, 0, 0]);
    assert_eq!(process.query_init_mask, 1);
    assert_eq!(process.query_bases[0].words[4], 0x7654_3210);
    assert_eq!(rng, 0x1234_5678);
    assert_eq!(sounds, 0);
}

mod native_guards;
