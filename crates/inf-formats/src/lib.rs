//! Strict, provenance-preserving parsers for Infestation's container formats.
//!
//! Container framing and each asset family stay separate so a proven handler
//! interpretation cannot silently leak into a different resource type.

mod avi;
mod constructor_assets;
mod error;
mod factory_shop;
mod fonts;
mod frontend;
mod frontend_executable;
mod grant_table;
mod handler13;
mod input_tables;
mod instance_assets;
mod instance_command_views;
mod key_names;
mod materials;
mod missions;
mod model_auxiliary;
mod model_diagnostic;
mod models;
pub mod native_basis;
pub mod observed_model;
mod overlay;
mod preload;
mod reader;
mod record_tables;
mod replay;
mod sample_grids;
mod save;
mod save_report;
mod scene;
mod scene_tree;
mod sound;
mod sprites;
mod strings;
mod terrain;
mod vehicle_behavior;
mod words;
mod world_assets;

pub use avi::{AviContainer, AviError, AviInfo};
pub use constructor_assets::{
    constructor_dispatch, parse_constructor_asset_bank, ActorConstructorPayload,
    BehaviorResourcePayload, BehaviorResourcePayloadTail, ConstructorAsset, ConstructorAssetBank,
    ConstructorAssetError, ConstructorDamageResponse, ConstructorDispatch, ConstructorField,
    ConstructorModelBinding, ConstructorPayloadView, ConstructorSoundBinding,
    DerivedActorConstructorPayload, Family40d950Payload, Family4236d0Payload, Family425190Payload,
    Family467070Payload, FlagsWordsConstructorPayload, GoalConstructorPayload,
    PointTableConstructorPayload, Selector15ConstructorPayload, Selector30ConstructorPayload,
    Selector34ConstructorPayload, Selector35ConstructorPayload, Selector45ConstructorPayload,
    Selector46ConstructorPayload, Selector74ConstructorPayload, Selector83ConstructorPayload,
    Selector86ConstructorPayload, TerrainLightConstructorPayload, CONSTRUCTOR_SELECTOR_COUNT,
};
pub use error::{Error, Result};
pub use factory_shop::{
    parse_factory_products, parse_factory_shop_layouts, parse_factory_shop_rows,
    parse_factory_upgrade_ids, FactoryProduct, FactoryProducts, FactoryShopTableError,
    FACTORY_COMMON_PRODUCTS, FACTORY_COMMON_PRODUCTS_ADDRESS, FACTORY_PRODUCT_BYTES,
    FACTORY_PRODUCT_LIST_ADDRESSES, FACTORY_SHOP_LAYOUTS_ADDRESS, FACTORY_SHOP_ROWS,
    FACTORY_SHOP_ROWS_ADDRESS, FACTORY_UPGRADE_IDS_ADDRESS, FACTORY_UPGRADE_ROWS,
};
pub use fonts::{
    parse_font_bank, Font, FontBank, FontError, FontMode, GlyphMetrics, FONT_HANDLER,
    FONT_HEADER_BYTES, FONT_METRIC_BYTES,
};
pub use frontend::{
    parse_frontend_attract_selection_table, parse_frontend_camera_presets,
    parse_frontend_inverse_sine_table, parse_frontend_menu, parse_frontend_menu_items,
    parse_frontend_menu_layouts, parse_frontend_scene_models, parse_frontend_sine_table,
    FrontendAttractSelectionTable, FrontendCameraPreset, FrontendData, FrontendError,
    FrontendInverseSineTable, FrontendMenu, FrontendMenuItem, FrontendMenuLayout,
    FrontendSceneModel, FrontendSineTable, BRIEFING_DETAIL_MENU_ADDRESS,
    BRIEFING_LIST_MENU_ADDRESS, BRIEFING_MENU_ADDRESS, FRONTEND_ATTRACT_SELECTION_ADDRESS,
    FRONTEND_ATTRACT_SELECTION_ENTRIES, FRONTEND_CAMERA_PRESETS_ADDRESS,
    FRONTEND_CAMERA_PRESET_BYTES, FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    FRONTEND_INVERSE_SINE_TABLE_ENTRIES, FRONTEND_MENU_DEFINITION_BYTES, FRONTEND_MENU_ITEM_BYTES,
    FRONTEND_MENU_LAYOUT_BYTES, FRONTEND_ROOT_MENU_ADDRESS, FRONTEND_SCENE_MODELS_ADDRESS,
    FRONTEND_SCENE_MODEL_BYTES, FRONTEND_SINE_TABLE_ADDRESS, FRONTEND_SINE_TABLE_ENTRIES,
    PHASE12_MENU_ADDRESS, PHASE5_MENU_ADDRESS,
};
pub use frontend_executable::{
    FrontendExecutable, FrontendExecutableError, FrontendExecutableRange, FrontendLoaderZeroRange,
};
pub use grant_table::{
    grant_closure, grant_into, grant_is_owned, merge_grants, parse_grant_flags, parse_grant_models,
    parse_grant_table, parse_grant_texts, GrantClosureError, GrantRecord, GrantTable,
    GrantTableError, GRANT_FIRST_ID, GRANT_LOW_TABLE_ADDRESS, GRANT_MASK_BITS, GRANT_MASK_WORDS,
    GRANT_OBSERVER, GRANT_RECORD_BYTES, GRANT_RECORD_COUNT, GRANT_TABLE_ADDRESS, GRANT_TEXT_IDS,
    PLAYER_INITIAL_GRANTS,
};
pub use handler13::{
    global_relocation_requested, scene_relocation_census, RelocationSceneCensus, HANDLER_13,
    HANDLER_14, RELOCATION_PHASE_ORDER,
};
pub use input_tables::{
    parse_analog_action_descriptor, parse_analog_input_table, parse_digital_action_descriptor,
    parse_digital_input_table, parse_direct_analog_input_table, parse_frontend_input_tables,
    parse_phase12_input_tables, parse_phase3_input_tables, parse_phase5_input_tables,
    parse_phase7_input_tables, parse_phase8_input_tables, parse_screen_input_tables,
    parse_screen_input_tables_direct, parse_startup_input_tables, parse_text_entry_input_tables,
    parse_title_input_tables, AnalogActionDescriptor, AnalogInputRecord, AnalogInputTable,
    DigitalActionDescriptor, DigitalInputRecord, DigitalInputTable, InputTableError,
    ScreenInputTables, ANALOG_ACTION_DESCRIPTOR_BYTES, ANALOG_INPUT_RECORD_BYTES,
    DIGITAL_ACTION_DESCRIPTOR_BYTES, DIGITAL_INPUT_RECORD_BYTES, FRONTEND_ANALOG_TABLE_POINTERS,
    FRONTEND_DIGITAL_TABLE_ADDRESSES, GAMEPLAY_ANALOG_TABLE_POINTERS,
    GAMEPLAY_DIGITAL_TABLE_ADDRESSES, GAMEPLAY_SUB_ANALOG_TABLE_ADDRESSES,
    GAMEPLAY_SUB_DIGITAL_TABLE_ADDRESSES, INPUT_DEVICE_SLOTS, PHASE12_ANALOG_TABLE_POINTERS,
    PHASE12_DIGITAL_TABLE_ADDRESSES, PHASE3_ANALOG_TABLE_POINTERS, PHASE3_DIGITAL_TABLE_ADDRESSES,
    PHASE5_DIGITAL_TABLE_ADDRESSES, PHASE7_ANALOG_TABLE_POINTERS, PHASE7_DIGITAL_TABLE_ADDRESSES,
    PHASE8_ANALOG_TABLE_POINTERS, PHASE8_DIGITAL_TABLE_ADDRESSES, STARTUP_ANALOG_TABLE_POINTERS,
    STARTUP_DIGITAL_TABLE_ADDRESSES, TEXT_ENTRY_ANALOG_TABLE_POINTERS,
    TEXT_ENTRY_DIGITAL_TABLE_ADDRESSES, TITLE_ANALOG_TABLE_POINTERS, TITLE_DIGITAL_TABLE_ADDRESSES,
};
pub use instance_assets::{
    parse_world_instance_bank, InstanceAssetBank, InstanceAssetError, InstanceCommand,
    InstanceField, PlacementInstanceAssets, WorldInstanceAssets,
};
pub use instance_command_views::{
    decode_instance_command, ConditionalInstanceDword, ConditionalInstanceWord,
    InstanceCommandPayload, InstanceCommandReadScope, InstanceCommandRequest,
    InstanceCommandTarget, InstanceCommandView, InstanceModelSlot, PlacementSpawnList,
    PlacementSpawnRow,
};
pub use key_names::{
    parse_key_name_table, KeyName, KeyNameError, KeyNameTable, KEY_NAME_COUNT, KEY_NAME_MAX_BYTES,
    KEY_NAME_TABLE_ADDRESS,
};
pub use materials::{
    parse_material_bank, MaterialBank, MaterialError, MaterialPalette8, MaterialValue, Rgb555,
    MATERIAL_ENTRY_BYTES, MATERIAL_HANDLER, TERRAIN_MATERIAL_PALETTE_ENTRIES,
};
pub use missions::{
    parse_mission_parameters, parse_mission_placements, MissionError, MissionParameterBank,
    MissionParameters, MissionPlacementBank, MissionPlacementSlot, MissionPlacementTable,
    MISSION_PARAMETER_BYTES, MISSION_PARAMETER_HANDLER, MISSION_PLACEMENT_BYTES,
    MISSION_PLACEMENT_HANDLER, MISSION_PLACEMENT_SLOTS,
};
pub use model_auxiliary::{
    decode_model_auxiliary, ModelAuxiliaryError, ModelAuxiliaryInstruction,
    ModelAuxiliaryInstructionKind, ModelAuxiliaryProgram, ModelAuxiliaryRotation,
};
pub use model_diagnostic::{
    analyze_model_diagnostic, ModelDiagnosticCompleteness, ModelDiagnosticFaceFlags,
    ModelDiagnosticFaceLighting, ModelDiagnosticFacePresentation, ModelDiagnosticGeometry,
    ModelDiagnosticOmission, ModelDiagnosticOmissionKind, ModelDiagnosticProjection,
    ModelDiagnosticRequest, ModelDiagnosticUnavailable, ModelDiagnosticUnavailableKind,
    ModelDiagnosticVertex, ModelDiagnosticView, ModelDiagnosticViewPlaneDecision,
    MODEL_DIAGNOSTIC_BASIS,
};
pub use models::{
    decode_model_program, parse_model_bank, ModelAuxiliaryRecord, ModelAxis, ModelBank, ModelBlock,
    ModelDynamicDependency, ModelEdge, ModelEntry, ModelError, ModelFaceFlags, ModelFanFlags,
    ModelFanVertex, ModelFanVertexAttributes, ModelFlags, ModelHeader, ModelIdentity,
    ModelInstance, ModelInstruction, ModelInstructionKind, ModelMaterialDomain, ModelMaterialRef,
    ModelName, ModelNestedKind, ModelNestedProgram, ModelOrientation, ModelProgram, ModelQuad,
    ModelQuadLighting, ModelRawCommand, ModelResourceRef, ModelSentinelList, ModelSlotRef,
    ModelTriangle, ModelTriangleFan, ModelTriangleLighting, ModelUnresolvedCommand, ModelVectorI16,
    MODEL_HANDLER, MODEL_HEADER_BYTES, MODEL_MAX_PROGRAM_DEPTH,
};
pub use overlay::{
    parse_overlay, AtlasRecord, HandlerFraming, HandlerId, HandlerRegion, ModelSubBlock,
    OverlayKey, OverlayLayout, OverlayVariant, RelocationTail,
};
pub use preload::{
    BranchKind, HandlerRequest, OverlayLeaf, Preload, PreloadHeader, RequestKind, RequestTable,
    ResourceSegment, SceneNode, HANDLER_COUNT,
};
pub use reader::ByteSpan;
pub use record_tables::{
    parse_handler10_bank, parse_handler5_bank, Handler10Bank, Handler10Record, Handler5Bank,
    Handler5Record, RecordTableError, HANDLER_10, HANDLER_10_RECORD_BYTES, HANDLER_5,
    HANDLER_5_RECORD_BYTES,
};
pub use replay::{
    CallbackObservation, ChildBasisObservation, ReplayCaptureMeta, ReplayError, RuntimeFrame,
    ANIMATION_REGISTER_COUNT, REPLAY_CAPTURE_META_KIND, REPLAY_FRAME_KIND, REPLAY_SCHEMA_VERSION,
};
pub use sample_grids::{
    decode_sample_grid, SampleGrid, SampleGridError, SampleGridRole, SampleGridValues,
};
pub use save::{
    copy_save_state, encode_save_records, read_save_state, save_crc32, MissionElapsedPolicy,
    MissionResetRequest, MissionSaveState, PlayerSaveState, SaveArchive, SaveBlockSource,
    SaveChecksumMismatch, SaveCopy, SaveDirectoryEnd, SaveDirectoryScan, SaveError, SaveRecord,
    SaveStateCopyRequest, SaveStateCopySource, SaveStateReadRequest, SaveStateReadTarget,
    MISSION_SAVE_BYTES, PLAYER_SAVE_BYTES, SAVE_BLOCKS_PER_RECORD, SAVE_BLOCK_PAYLOAD_BYTES,
    SAVE_COPY_BYTES, SAVE_MAX_RECORDS, SAVE_NAME_MAX_BYTES, SAVE_PAYLOAD_BYTES, SAVE_RECORD_BYTES,
};
pub use save_report::{
    world_status, MissionReport, PlayerReport, SaveProfileReport, WorldReport, ENERGY_ROWS,
};
pub use scene::{active_phases_for_handler, resolve_scene_winner, SceneContext};
pub use scene_tree::{LeafOperation, SceneTree, SceneTreeError};
pub use sound::{
    encode_sound_wav, parse_sound_bank, SoundBank, SoundEntry, SoundEntryKind, SoundError,
    WavError, SOUND_BITS_PER_SAMPLE, SOUND_CHANNELS, SOUND_ENTRY_BYTES, SOUND_HANDLER,
    SOUND_SAMPLE_RATE,
};
pub use sprites::{
    parse_sprite_bank, Rgba8, SpriteAtlasOrigin, SpriteBank, SpriteBlendMode, SpriteEncoding,
    SpriteEntry, SpriteError, SpriteFlags, SpriteModeSelectors, SpriteOrphanEntry, SpritePalette,
    SpriteRenderMode, SpriteShade, SpriteVariant, SPRITE_ATLAS_STRIDE_BYTES,
    SPRITE_ENTRY_HEADER_BYTES, SPRITE_HANDLER, SPRITE_SHADE_LEVELS, SPRITE_VARIANT_BYTES,
};
pub use strings::{parse_string_bank, StringBank, StringEntry, StringError, STRING_HANDLER};
pub use terrain::{
    parse_terrain_bank, terrain_blend, terrain_blend_table, TerrainBank, TerrainBlend,
    TerrainBlendSymmetry, TerrainBucket, TerrainCell, TerrainEntry, TerrainError, TerrainPrefix,
    TerrainRecord, TerrainResourceLookup, TERRAIN_BLEND_COUNT, TERRAIN_BLEND_TEXTURE_RESOURCE_BASE,
    TERRAIN_CANONICAL_BLEND_COMBOS, TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES,
    TERRAIN_GRID_SIDE, TERRAIN_HANDLER, TERRAIN_PREFIX_BYTES, TERRAIN_RECORD_BYTES,
};
pub use vehicle_behavior::{
    parse_vehicle_behavior_table, parse_vehicle_upgrade_limits, VEHICLE_BEHAVIOR_RECORDS,
    VEHICLE_BEHAVIOR_RECORD_DWORDS, VEHICLE_BEHAVIOR_TABLE_ADDRESS, VEHICLE_BEHAVIOR_TABLE_DWORDS,
    VEHICLE_UPGRADE_LIMITS_ADDRESS,
};
pub use words::{
    parse_word_resource_bank, WordResource, WordResourceBank, WordResourceError,
    WORD_RESOURCE_BYTES, WORD_RESOURCE_HANDLERS,
};
pub use world_assets::{
    parse_constructor_bank, parse_world_bank, AssetRelocation, ConstructorBank,
    ConstructorResource, WorldAssetError, WorldBank, WorldPlacement, WorldResource,
    CONSTRUCTOR_PREFIX_BYTES, WORLD_HEADER_BYTES, WORLD_PLACEMENT_PREFIX_BYTES,
};
mod briefing;
pub use briefing::{
    parse_briefing_program, BriefingCamera, BriefingError, BriefingModel, BriefingPayload,
    BriefingProgram, BriefingRecord, BRIEFING_RECORD_BYTES,
};
