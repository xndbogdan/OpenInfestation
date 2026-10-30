//! Current H11/0 custody for damage admission and the timed-effect executor.
//! Native null models use the same H8 lookup policy as actor model metadata.

use inf_formats::{FrontendExecutable, MissionPlacementTable, TerrainResourceLookup};
use thiserror::Error;

use super::{
    run_furniture_chain, run_furniture_particle_callback, run_furniture_radius_callback,
    FurnitureCallbackContext, FurnitureCallbackRequest, FurnitureChainLoan,
    FurnitureEffectExecutorHost, FurnitureEffectQueue, FurnitureExecutionPlacement,
    FurnitureParticleCallbackHost, FurnitureParticleCallbackInputs, FurnitureRadiusCallbackHost,
    FurnitureRadiusCallbackInputs, ProcessFurnitureQueryState, ResidentFurnitureChainResources,
};
use crate::{
    object_runtime::resident_model_metadata,
    process_particles::audio::ParticleSoundRequest,
    process_sound::ProcessSoundRuntime,
    terrain_runtime::{TerrainRuntime, TerrainSparsePayload},
    HandlerResources, LoadedOverlay, ResidentRuntime,
};

/// One typed current H11/H8 resource boundary shared by radius and furniture
/// services. String-based host contracts format this only at their existing edge.
#[derive(Debug, Error)]
pub enum ResidentFurnitureResourceError {
    #[error("required resident H11/0 placement table")]
    RequiredH11,
    #[error("H11 registration does not select an owned placement-table entry")]
    H11Registration,
    #[error("H11 lookup {lookup} outside {entries} entries")]
    Lookup { lookup: u8, entries: usize },
    #[error(transparent)]
    Model(#[from] crate::object_runtime::ObjectError),
}

pub fn resident_placement_table(
    resident: &ResidentRuntime,
) -> Result<&MissionPlacementTable, ResidentFurnitureResourceError> {
    let source = resident
        .resource(11, 0)
        .ok_or(ResidentFurnitureResourceError::RequiredH11)?;
    match source
        .overlay
        .handlers()
        .get(source.registration.region_index)
    {
        Some(HandlerResources::MissionPlacements(bank)) => bank
            .entries
            .get(usize::from(source.registration.local_index))
            .ok_or(ResidentFurnitureResourceError::H11Registration),
        _ => Err(ResidentFurnitureResourceError::H11Registration),
    }
}

///0043648A..9E resolves H11 first WORD through the actual H8 manager.
/// Negative, strictly out-of-bound and loader-null slots return None;
/// index equal to the allocation count remains the explicit unsafe boundary.
pub fn resident_furniture_model_radius(
    resident: &ResidentRuntime,
    lookup: TerrainResourceLookup,
) -> Result<Option<u16>, ResidentFurnitureResourceError> {
    let table = resident_placement_table(resident)?;
    let slot = table.slots.get(usize::from(lookup.raw())).ok_or(
        ResidentFurnitureResourceError::Lookup {
            lookup: lookup.raw(),
            entries: table.slots.len(),
        },
    )?;
    resident_model_metadata(resident, slot.resource_word)
        .map(|metadata| metadata.map(|model| model.radius_word))
        .map_err(ResidentFurnitureResourceError::Model)
}

#[derive(Clone, Copy, Debug)]
pub struct ResidentFurniturePlacement<'a> {
    /// Actual immutable allocation identity, retained for the current burst.
    pub allocation: &'a LoadedOverlay,
    pub table_region_index: usize,
    pub table_local_index: u16,
    pub entry_index: u8,
}

impl ResidentFurniturePlacement<'_> {
    /// The entry's second WORD in its own allocation, whatever the current
    /// registration names.
    fn parameter_word(&self) -> Result<u16, String> {
        let bank = match self.allocation.handlers().get(self.table_region_index) {
            Some(HandlerResources::MissionPlacements(bank)) => bank,
            _ => return Err("cached H11 entry's allocation has no placement table".into()),
        };
        bank.entries
            .get(usize::from(self.table_local_index))
            .and_then(|table| table.slots.get(usize::from(self.entry_index)))
            .map(|slot| slot.parameter_word)
            .ok_or_else(|| "cached H11 entry is outside its placement table".into())
    }
}

/// Settled timer/queue/terrain callbacks execute inside FurnitureEffectQueue.
/// One-shot audio delegates to the retained process sound queue; 004375A0
/// delegates to the live world-radius loan. Other effect/model-creation
/// callbacks remain required. Shared fallback operations
/// loan the retained process owner; queue admission completes no callback.
pub trait ResidentFurnitureEffects:
    FurnitureRadiusCallbackHost<Error = String>
    + FurnitureParticleCallbackHost<Error = String>
    + super::FurnitureCameraCallbackHost<Error = String>
    + super::FurniturePickupCallbackHost<Error = String>
{
}
impl<T> ResidentFurnitureEffects for T where
    T: FurnitureRadiusCallbackHost<Error = String>
        + FurnitureParticleCallbackHost<Error = String>
        + super::FurnitureCameraCallbackHost<Error = String>
        + super::FurniturePickupCallbackHost<Error = String>
{
}

pub struct ResidentFurnitureExecutor<'a> {
    pub resident: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'a>,
    ///00436DE0 shares the retained process list with actor/particle sounds.
    pub sounds: &'a mut ProcessSoundRuntime,
    ///Same005483C4 owner loaned to collision, retained across worlds/screens.
    pub process: &'a mut ProcessFurnitureQueryState,
    /// Borrowed only when 004375A0 is reached. Queue/terrain/XYZ/sound/RNG
    /// arrive from this active callback and remain their actual live owners.
    pub effects: &'a mut dyn ResidentFurnitureEffects,
    /// 004373B0's fine spacing scope and the settings 00450370 rereads, and
    /// the network flag 00436240 tests.
    pub chain: FurnitureChainLoan<'a>,
}

impl<'a> FurnitureEffectExecutorHost for ResidentFurnitureExecutor<'a> {
    type Error = String;
    type Placement = ResidentFurniturePlacement<'a>;

    fn program_word(&mut self, address: u32) -> Result<u32, String> {
        self.executable
            .word(address)
            .map_err(|error| error.to_string())
    }

    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<FurnitureExecutionPlacement<Self::Placement>>, String> {
        let source = self
            .resident
            .resource(11, 0)
            .ok_or("required resident H11/0 placement table")?;
        let table = resident_placement_table(self.resident).map_err(|error| error.to_string())?;
        let slot = table.slots.get(usize::from(lookup.raw())).ok_or_else(|| {
            format!(
                "H11 lookup {} is outside {} entries",
                lookup.raw(),
                table.slots.len()
            )
        })?;
        Ok(Some(FurnitureExecutionPlacement {
            identity: ResidentFurniturePlacement {
                allocation: source.overlay,
                table_region_index: source.registration.region_index,
                table_local_index: source.registration.local_index,
                entry_index: lookup.raw(),
            },
            model_word: slot.resource_word as i16,
        }))
    }

    fn preload_model_if_required(&mut self, model_slot: u16) -> Result<(), String> {
        // Metadata lookup settles the same inclusive bound and genuine null
        // loader policy; it does not create a replacement model allocation.
        resident_model_metadata(self.resident, model_slot)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn fallback_sparse_payload(&mut self) -> Result<TerrainSparsePayload, String> {
        Ok(self.process.fallback_payload())
    }
    fn write_fallback_sparse_state(&mut self, state: u8) -> Result<(), String> {
        self.process.write_fallback_state(state);
        Ok(())
    }
    fn clear_fallback_sparse_payload(&mut self) -> Result<(), String> {
        self.process.clear_fallback_payload();
        Ok(())
    }
    fn callback(
        &mut self,
        queue: &mut FurnitureEffectQueue,
        _: usize,
        terrain: &mut TerrainRuntime,
        context: &mut FurnitureCallbackContext,
        request: FurnitureCallbackRequest<Self::Placement>,
        rng: &mut u32,
    ) -> Result<bool, String> {
        if request.callback_address == 0x0043_6de0 {
            //436DE0 reads stack+4 XYZ and stack+8 selector, ignoring receiver
            //and H11 userdata. It invokes487EC0 with volume/pitch65536 and
            //flags0, then returns AL1. Enqueue copies the current mutable
            //stack WORDs and captures current H9 custody; playback is later.
            self.sounds
                .enqueue(
                    self.resident,
                    ParticleSoundRequest {
                        sound: request.argument,
                        position: context.position_words,
                        volume: 65_536,
                        pitch: 65_536,
                        flags: 0,
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(true)
        } else if request.callback_address == 0x0043_75a0 {
            // Original receiver ECX and cached H11 userdata EDX are unused.
            // Source capture and the normal-only AL1 suffix share the tested
            // intrinsic adapter; no block/XYZ read belongs in this dispatch.
            run_furniture_radius_callback(
                self.effects,
                FurnitureRadiusCallbackInputs {
                    queue,
                    terrain,
                    context,
                    block_address: request.argument,
                    sounds: self.sounds,
                    rng,
                },
            )
            .map_err(|error| error.to_string())
        } else if request.callback_address == 0x0043_75d0 {
            // 004375D0 ignores receiver and userdata: camera, optional
            // argument-radius gate, then 00487D70 with F00h; AL is one.
            self.effects.camera_flash(
                request.argument,
                super::FurnitureCameraCallbackInputs {
                    context,
                    terrain,
                    rng,
                },
            )?;
            Ok(true)
        } else if request.callback_address == 0x0043_6d20 {
            run_furniture_particle_callback(
                request.argument,
                self.effects,
                FurnitureParticleCallbackInputs {
                    context,
                    sounds: self.sounds,
                    rng,
                },
            )
        } else if request.callback_address == 0x0043_7760 {
            // 00437760 passes the current XYZ and selector to 0048AD70 with
            // flags zero; AL1 is returned only after the native call returns.
            self.effects.emit(
                super::FurnitureParticleEffect::TerrainFlash {
                    selector: request.argument,
                },
                FurnitureParticleCallbackInputs {
                    context,
                    sounds: self.sounds,
                    rng,
                },
            )?;
            Ok(true)
        } else if request.callback_address == 0x0043_7880 {
            // 00437880 tests the cached entry in EDX, then its byte +2.
            let byte = request
                .placement
                .map(|placement| placement.parameter_word().map(|word| word as u8))
                .transpose()?;
            super::run_furniture_pickup_callback(
                byte,
                self.effects,
                super::FurniturePickupCallbackInputs {
                    context,
                    terrain,
                    rng,
                },
            )
            .map_err(|error| error.to_string())
        } else if request.callback_address == 0x0043_73b0 {
            // 004373B0 ignores the receiver and the cached H11 userdata; its
            // unflagged cells read the process record 005483C4, which the
            // walk's enqueues leave alone.
            let fallback = self.process.fallback_payload();
            run_furniture_chain(
                queue,
                terrain,
                context.position_words,
                fallback,
                &mut self.chain,
                &mut ResidentFurnitureChainResources {
                    resident: self.resident,
                    executable: self.executable,
                },
            )
            .map_err(|error| error.to_string())
        } else if matches!(request.callback_address, 0x0043_7060 | 0x0043_70f0) {
            let replacement = if request.callback_address == 0x0043_7060 {
                super::FurnitureReplacement::ClearCellAndLookupFf
            } else {
                super::FurnitureReplacement::LookupFd
            };
            super::run_furniture_replacement(
                terrain,
                self.process,
                context.position_words,
                replacement,
                rng,
            )
            .map_err(|error| error.to_string())
        } else {
            Err(format!(
                "required furniture effect callback {:08X}",
                request.callback_address
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        frontend_assets::FrontendAssets,
        frontend_scene::SceneView,
        menu::MenuBasis,
        process_sound::{ProcessSoundAudio, ProcessSoundListener, ProcessSoundParameters},
        terrain_runtime::TerrainSpacing,
        world_placement::WorldPlacements,
        Installation, ResidencyRequest,
    };

    #[derive(Default)]
    struct Audio(Vec<(i16, ProcessSoundParameters)>);
    impl crate::process_sound::ProcessSoundVoiceTeardown for Audio {
        fn delete_voice(
            &mut self,
            record: crate::process_sound::ProcessSoundRef,
            voice: crate::process_sound::ProcessSoundVoiceRef,
        ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
            crate::process_sound::ProcessSoundVoiceTeardown::delete_voice(
                &mut crate::process_sound::RequiredProcessSoundVoiceTeardown,
                record,
                voice,
            )
        }
    }
    impl ProcessSoundAudio for Audio {
        type Error = std::convert::Infallible;
        fn operation(
            &mut self,
            request: crate::process_sound::ProcessSoundVoiceRequest<'_>,
        ) -> Result<
            crate::process_sound::ProcessSoundVoiceReply,
            crate::process_sound::ProcessSoundVoiceServiceFault<Self::Error>,
        > {
            use crate::process_sound::{
                ProcessSoundVoiceOperation as Operation, ProcessSoundVoiceReply as Reply,
                ProcessSoundVoiceRequest as Request, ProcessSoundVoiceServiceFault as Fault,
            };
            let Request::OneShot {
                resource,
                parameters,
                effects_volume: _,
                rng: _,
            } = request
            else {
                return Err(Fault::Required(match request {
                    Request::Create { .. } => Operation::Create,
                    Request::Start { .. } => Operation::Start,
                    Request::Update { .. } => Operation::Update,
                    Request::OneShot { .. } => unreachable!(),
                }));
            };
            (|| -> Result<bool, Self::Error> {
                self.0.push((resource.requested_slot(), parameters));
                Ok(true)
            })()
            .map(Reply::OneShot)
            .map_err(Fault::Backend)
        }
    }

    #[test]
    fn furniture_audio_copies_current_position_and_retains_actual_h9_custody_until_draw() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        resident
            .apply(ResidencyRequest::Activate { level: 48 })
            .unwrap();
        resident
            .apply(ResidencyRequest::Activate { level: 32 })
            .unwrap();
        let allocation = std::sync::Arc::downgrade(resident.resource(9, 6).unwrap().overlay);
        let mut terrain = WorldPlacements::new(&resident, TerrainSpacing::Units256)
            .unwrap()
            .terrain()
            .clone();
        let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
        let mut process = ProcessFurnitureQueryState::native_startup(executable).unwrap();
        let mut furniture = FurnitureEffectQueue::default();
        let mut context = FurnitureCallbackContext {
            position_words: [4096, 0, 0],
        };
        let mut rng = 0x1234_5678;
        {
            let mut executor = ResidentFurnitureExecutor {
                resident: &resident,
                executable,
                sounds: &mut sounds,
                process: &mut process,
                effects: &mut crate::world_furniture_effect::RequiredTestFurnitureRadius,
                chain: crate::world_furniture_effect::FurnitureChainLoan {
                    policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                        crate::terrain_runtime::TerrainSpacing::Units256,
                    ),
                    detail: 0,
                    world_flags: None,
                    network_active: false,
                },
            };
            assert!(executor
                .callback(
                    &mut furniture,
                    usize::MAX,
                    &mut terrain,
                    &mut context,
                    FurnitureCallbackRequest {
                        callback_address: 0x0043_6de0,
                        argument: 6,
                        placement: None
                    },
                    &mut rng,
                )
                .unwrap());
            // Later furniture callbacks share these mutable stack WORDs. The
            // admitted sound keeps the XYZ that was present at its own call.
            context.position_words = [8192, 0, 0];
            assert!(executor
                .callback(
                    &mut furniture,
                    usize::MAX,
                    &mut terrain,
                    &mut context,
                    FurnitureCallbackRequest {
                        callback_address: 0x0043_75d0,
                        argument: 6,
                        placement: None
                    },
                    &mut rng,
                )
                .unwrap_err()
                .contains("004375D0"));
        }
        assert_eq!(sounds.pending_len(), 1);
        assert_eq!(rng, 0x1234_5678);
        assert!(furniture.records().is_empty());
        drop(resident);
        assert!(
            allocation.upgrade().is_some(),
            "the queued sound retains actual H9 allocation"
        );
        let mut audio = Audio::default();
        assert_eq!(
            sounds
                .service_world_draw(
                    ProcessSoundListener {
                        view: SceneView {
                            eye: [0; 3],
                            basis: MenuBasis::INITIAL
                        },
                        phase: 4,
                        suppressed: false,
                        water: None,
                    },
                    &mut audio,
                    15,
                    &mut rng,
                )
                .unwrap(),
            1
        );
        assert_eq!(
            audio.0,
            [(
                6,
                ProcessSoundParameters {
                    volume: 32_768,
                    pitch: 65_536,
                    pan: 9999,
                    control_flag: false,
                    placement: Some(crate::process_sound::SoundPlacement {
                        rotated: [4096, 0, 0],
                        under_water: false,
                    }),
                }
            )]
        );
        assert_eq!(rng, 0x1234_5678);
        assert!(sounds.is_empty());
        assert!(
            allocation.upgrade().is_none(),
            "queue cleanup releases the retained source after voice submission"
        );
    }
}
