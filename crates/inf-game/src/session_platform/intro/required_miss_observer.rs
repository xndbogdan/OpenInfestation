//! Test-only live portable miss-stage provenance. Only actual host calls publish
//! normal prefix-return packets; read-only census never supplies a result.
//! The actual linked fixture publishes context before first_pass's actor tick.

use crate::{
    object_runtime::{
        resident_model_metadata, FamilyAvoidanceError, FamilyObstacleScanError, ObjectArena,
        ObjectId, Selector12TailError, Selector12TailProbeError,
    },
    render_resources::{
        RenderResourceData, RenderResourceKind, RenderResourceRequest, RenderResources,
    },
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::WorldTerrainSource,
    world_furniture_effect::ProcessFurnitureQueryState,
    ResidentRuntime,
};
use inf_formats::{parse_constructor_bank, ByteSpan};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashSet};

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameContext {
    pub iteration: u64,
    pub now_micros: u64,
    pub generation: u64,
    pub revision: u64,
    pub process_radius_word: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MissStage {
    StoppedCallback,
    CollisionPrefixReturned,
}

#[derive(Debug)]
pub(crate) struct MissPacket {
    pub stage: MissStage,
    pub actor: ObjectId,
    pub ordinal: Option<u8>,
    pub position_words: [u16; 2],
    pub error_display: Option<String>,
    pub packet: Value,
}

struct Recorder {
    frame: Option<FrameContext>,
    packets: Vec<MissPacket>,
    seen: HashSet<(ObjectId, MissStage)>,
}
thread_local! { static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) }; }

pub(crate) struct ObserveScope {
    previous: Option<Recorder>,
}
pub(crate) fn arm() -> ObserveScope {
    let previous = RECORDER.with(|r| {
        r.replace(Some(Recorder {
            frame: None,
            packets: vec![],
            seen: HashSet::new(),
        }))
    });
    ObserveScope { previous }
}
impl ObserveScope {
    pub(crate) fn take(&self) -> Vec<MissPacket> {
        RECORDER.with(|r| {
            r.borrow_mut()
                .as_mut()
                .map(|r| std::mem::take(&mut r.packets))
                .unwrap_or_default()
        })
    }
}
impl Drop for ObserveScope {
    fn drop(&mut self) {
        RECORDER.with(|r| {
            r.replace(self.previous.take());
        });
    }
}
pub(crate) fn is_armed() -> bool {
    RECORDER.with(|r| r.borrow().is_some())
}
pub(crate) fn frame_context(frame: FrameContext) {
    RECORDER.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            r.frame = Some(frame);
        }
    });
}

pub(crate) fn miss<E>(
    error: &Selector12TailError<E>,
) -> Option<(ObjectId, u8, [u16; 2], &'static str)> {
    let Selector12TailError::Avoidance(FamilyAvoidanceError::Host {
        effect,
        error:
            FamilyObstacleScanError::Host {
                ordinal,
                error:
                    Selector12TailProbeError::Miss {
                        actor,
                        position_words,
                        ..
                    },
            },
    }) = error
    else {
        return None;
    };
    Some((*actor, *ordinal, *position_words, *effect))
}

pub(crate) fn observe<E: std::fmt::Display>(
    arena: &ObjectArena,
    caller: ObjectId,
    error: &Selector12TailError<E>,
    resident: &ResidentRuntime,
    source: &WorldTerrainSource,
    terrain: &TerrainRuntime,
    process: &ProcessFurnitureQueryState,
) {
    if !is_armed() {
        return;
    }
    let Some((actor, ordinal, position_words, effect)) = miss(error) else {
        return;
    };
    if seen(actor, MissStage::StoppedCallback) {
        return;
    }
    let frame = RECORDER.with(|r| r.borrow().as_ref().and_then(|r| r.frame));
    // Every helper is read-only. Observation errors stay in their own packet;
    // the runtime still logs/stores the original typed error without recovery.
    let captured = capture(
        arena,
        actor,
        caller,
        Some(ordinal),
        position_words,
        effect,
        frame,
        resident,
        source,
        terrain,
        process,
    );
    let mut packet = match captured {
        Ok(packet) => packet,
        Err(error) => json!({ "observation_error": error, "actual_actor": format!("{actor:?}"),
            "caller_actor": format!("{caller:?}"), "miss_xz": position_words }),
    };
    packet["stage"] = json!("stopped_callback");
    packet["coordinate_source"] = json!("actual_obstacle_request_xz");
    RECORDER.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            r.seen.insert((actor, MissStage::StoppedCallback));
            r.packets.push(MissPacket {
                stage: MissStage::StoppedCallback,
                actor,
                ordinal: Some(ordinal),
                position_words,
                error_display: Some(error.to_string()),
                packet,
            });
        }
    });
}

fn seen(actor: ObjectId, stage: MissStage) -> bool {
    RECORDER.with(|r| {
        r.borrow()
            .as_ref()
            .is_some_and(|r| r.seen.contains(&(actor, stage)))
    })
}

/// Armed-only snapshot at the actual collide entry, after429EE0 installed its
/// guard/temporaryXYZ, before47C1D0 mutates query state. No observer recovery.
pub(crate) struct CollisionPrefixObservation {
    actor: ObjectId,
    point: [u16; 2],
    packet: Value,
}
pub(crate) fn begin_collision_prefix_observation(
    arena: &ObjectArena,
    actor: ObjectId,
    resident: &ResidentRuntime,
    source: &WorldTerrainSource,
    terrain: &TerrainRuntime,
    process: &ProcessFurnitureQueryState,
) -> Option<CollisionPrefixObservation> {
    if !is_armed() || seen(actor, MissStage::CollisionPrefixReturned) {
        return None;
    }
    //429EE0 has already installed genuine temporary XYZ before this host call.
    //No original cache-request point can be inferred from its snapped center.
    let position = arena.object(actor).ok()?.base.position_words?;
    let point = [position[0], position[2]];
    let frame = RECORDER.with(|r| r.borrow().as_ref().and_then(|r| r.frame));
    let mut packet = capture(
        arena,
        actor,
        actor,
        None,
        point,
        "actual0047C1D0entry_after429EE0_tempXYZ",
        frame,
        resident,
        source,
        terrain,
        process,
    )
    .unwrap_or_else(|error| {
        json!({"observation_error": error,
            "actual_actor": format!("{actor:?}"), "snapshot_xz": point})
    });
    packet["stage"] = json!("collision_prefix_entry");
    packet["coordinate_source"] = json!("actual450BD0_temporary_collision_xz");
    packet["snapshot_xz"] = json!(point);
    packet
        .as_object_mut()
        .expect("observer packet is an object")
        .remove("miss_xz");
    packet["query_entry"] = json!({"mask": process.query_init_mask(),
        "bases": (*process.query_bases()).map(|basis| basis.words),
        "record": format!("{:?}", process.current_record()),
        "alias_is_546b48": process.pair_alias_is_546b48(),
        "fallback": {"state": process.fallback_payload().state,
            "lookup": process.fallback_payload().resource_lookup.raw()},
        "guard_word22e": arena.family428650_actor(actor).ok().map(|s| s.word_22e)});
    Some(CollisionPrefixObservation {
        actor,
        point,
        packet,
    })
}
pub(crate) fn finish_collision_prefix_observation(
    observation: Option<CollisionPrefixObservation>,
    arena: &ObjectArena,
    process: &ProcessFurnitureQueryState,
) {
    let Some(mut observation) = observation else {
        return;
    };
    // The real owner returned Ok; this is before normal429EE0 restoration and
    // cache completion. Do not label it completed virtual+24 or final cache.
    observation.packet["stage"] = json!("collision_prefix_returned_normally");
    observation.packet["query_return"] = json!({"outcome": "actual_prefix_normal_return",
        "mask": process.query_init_mask(), "bases": (*process.query_bases()).map(|basis| basis.words),
        "record": format!("{:?}", process.current_record()),
        "alias_is_546b48": process.pair_alias_is_546b48(),
        "literal_ret_registration_attempted": process.literal_ret_registration_attempted(),
        "fallback": {"state": process.fallback_payload().state,
            "lookup": process.fallback_payload().resource_lookup.raw()},
        "guard_word22e": arena.family428650_actor(observation.actor).ok().map(|s| s.word_22e),
        "temporary_xyz": arena.object(observation.actor).ok().and_then(|s| s.base.position_words)});
    RECORDER.with(|r| {
        if let Some(r) = r.borrow_mut().as_mut() {
            if r.seen
                .insert((observation.actor, MissStage::CollisionPrefixReturned))
            {
                r.packets.push(MissPacket {
                    stage: MissStage::CollisionPrefixReturned,
                    actor: observation.actor,
                    ordinal: None,
                    position_words: observation.point,
                    error_display: None,
                    packet: observation.packet,
                });
            }
        }
    });
}

/// Actual post-step decoded bytes, read without probing or updating the cache.
/// A moved window is an explicit diagnostic status, not an invented sample.
pub(crate) fn post_step_cache(arena: &ObjectArena, packet: &MissPacket) -> Result<Value, String> {
    let owner = arena
        .family429e20_actor(packet.actor)
        .map_err(|e| e.to_string())?
        .probe_234
        .as_ref()
        .ok_or("post-step cache missing")?;
    let cache = owner.decoded_cache();
    let cells = packet.position_words.map(|word| word >> 6);
    let relative = [
        cells[0].wrapping_sub(cache.word_04()),
        cells[1].wrapping_sub(cache.word_06()),
    ];
    let in_window = relative
        .iter()
        .all(|&cell| cell < u16::from(cache.dimension()));
    let encoded = if in_window {
        let index = usize::from(relative[1]) * usize::from(cache.stride_words())
            + usize::from(relative[0] / 8);
        let word = *cache
            .words()
            .get(index)
            .ok_or("post-step sample outside real storage")?;
        Some((word.wrapping_shr(u32::from(relative[0] & 7) * 4) & 15) as u8)
    } else {
        None
    };
    Ok(json!({"allocation_address": format!("{:p}", &**owner),
        "relative_in_window": in_window, "encoded_nibble_at_snapshot_xz": encoded,
        "snapshot_xz": packet.position_words, "coordinate_source": format!("{:?}", packet.stage),
        "bounds": "Actual post-step bytes at the packet coordinate. A normal packet uses the snapped collision center, not the original cache-request point; this sample is not attributed as that request completion.",
        "guard_word22e": arena.family428650_actor(packet.actor).map_err(|e| e.to_string())?.word_22e,
        "current_xyz": arena.object(packet.actor).map_err(|e| e.to_string())?.base.position_words,
        "word_04": cache.word_04(), "word_06": cache.word_06(), "words": cache.words()}))
}

fn bytes(span: ByteSpan, source: &[u8]) -> Result<Value, String> {
    let raw = span
        .get(source)
        .ok_or_else(|| format!("diagnostic span outside source: {span:?}"))?;
    Ok(
        json!({ "span": span, "sha256": format!("{:x}", Sha256::digest(raw)),
        "hex": raw.iter().map(|b| format!("{b:02x}")).collect::<String>() }),
    )
}

fn constructor(resident: &ResidentRuntime, word: u16) -> Result<Value, String> {
    let source = resident
        .resource(13, word)
        .ok_or_else(|| format!("diagnostic current H13/{word} absent"))?;
    let region = source
        .overlay
        .layout()
        .regions
        .get(source.registration.region_index)
        .ok_or("diagnostic H13 region mismatch")?;
    let bank = parse_constructor_bank(source.overlay.bytes(), region).map_err(|e| e.to_string())?;
    let entry = bank
        .entries
        .get(usize::from(source.registration.local_index))
        .ok_or("diagnostic H13 local entry mismatch")?;
    Ok(
        json!({ "status": "owned", "registration": source.registration,
        "overlay_key": source.overlay.key(), "overlay_path": source.overlay.path(),
        "overlay_sha256": format!("{:x}", Sha256::digest(source.overlay.bytes())),
        "constructor": entry, "prefix": bytes(entry.prefix_span, source.overlay.bytes())? }),
    )
}

fn model(resident: &ResidentRuntime, word: u16) -> Result<Value, String> {
    let capacity = resident
        .resource_capacity(8)
        .ok_or("diagnostic missing H8 allocation")?;
    if (word as i16) < 0 {
        return Ok(json!({ "status": "negative", "model_word": word, "capacity": capacity }));
    }
    if usize::from(word) > capacity {
        return Ok(json!({ "status": "above_capacity", "model_word": word, "capacity": capacity }));
    }
    if usize::from(word) == capacity {
        return Ok(
            json!({ "status": "inclusive_boundary", "model_word": word, "capacity": capacity }),
        );
    }
    let Some(metadata) = resident_model_metadata(resident, word).map_err(|e| e.to_string())? else {
        return Ok(json!({ "status": "loader_null", "model_word": word, "capacity": capacity }));
    };
    let resource = RenderResources::new(resident)
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: i32::from(word as i16),
        })
        .map_err(|e| e.to_string())?;
    let RenderResourceData::Model(entry) = resource.data else {
        return Err("diagnostic H8 type mismatch".into());
    };
    let registration = resource.source.registration;
    let source = resident
        .resource(8, word)
        .ok_or("diagnostic H8 disappeared during immutable loan")?;
    Ok(
        json!({ "status": "owned", "model_word": word, "capacity": capacity,
        "registration": registration, "overlay_key": source.overlay.key(),
        "overlay_path": source.overlay.path(), "overlay_sha256": format!("{:x}", Sha256::digest(resource.source.bytes)),
        "identity": entry.identity, "header": entry.header, "radius_word": metadata.radius_word,
        "entry_span": entry.span, "content_span": entry.content_span,
        "header_bytes": bytes(entry.header_span, resource.source.bytes)?,
        "auxiliary_bytes": bytes(entry.auxiliary_span, resource.source.bytes)?,
        "auxiliary_program": entry.auxiliary_program }),
    )
}

fn capture(
    arena: &ObjectArena,
    actor: ObjectId,
    caller: ObjectId,
    ordinal: Option<u8>,
    point: [u16; 2],
    effect: &'static str,
    frame: Option<FrameContext>,
    resident: &ResidentRuntime,
    source: &WorldTerrainSource,
    terrain: &TerrainRuntime,
    process: &ProcessFurnitureQueryState,
) -> Result<Value, String> {
    let frame = frame.ok_or("armed observer lacks actual linked frame context")?;
    if frame.revision != resident.revision() {
        return Err("observer frame residency changed before the miss".into());
    }
    if actor != caller {
        return Err("typed miss actor differs from actual callback source".into());
    }
    let state = arena.object(actor).map_err(|e| e.to_string())?;
    let actual = arena.actor(actor).map_err(|e| e.to_string())?;
    let derived = arena.derived_actor(actor).map_err(|e| e.to_string())?;
    let class = arena
        .family_update_class(actor)
        .map_err(|e| e.to_string())?;
    let cache_owner = arena
        .family429e20_actor(actor)
        .map_err(|e| e.to_string())?
        .probe_234
        .as_ref()
        .ok_or("diagnostic required +234 cache missing")?;
    let cache = cache_owner.decoded_cache();
    let h12 = resident
        .resource(12, 0)
        .ok_or("diagnostic current H12/0 required")?;
    if !source.is_current(&h12) {
        return Err("diagnostic prepared terrain allocation replaced".into());
    }
    let actor_model = model(resident, actual.rotating.visual.model_word)?;
    let h13 = constructor(resident, state.base.resource_word)?;
    let census = census(
        resident,
        terrain,
        point,
        frame.process_radius_word,
        &actor_model,
        process,
    )?;
    Ok(
        json!({ "actual_actor": format!("{actor:?}"), "caller_actor": format!("{caller:?}"),
        "actual_update_class": format!("{class:?}"), "actual_object_class": format!("{:?}", state.class),
        "ordinal": ordinal, "effect": effect, "miss_xz": point,
        "frame": { "iteration": frame.iteration, "now_micros": frame.now_micros,
            "generation": frame.generation, "revision": frame.revision, "process_radius_word": frame.process_radius_word },
        "base": { "word_28": state.base.word_28, "placement_word": state.base.placement_word,
            "flags": state.base.flags, "resource_word": state.base.resource_word,
            "position_words": state.base.position_words, "dirty_word": state.base.dirty_word,
            "word_3c": state.base.word_3c },
        "current_model_word_42": actual.rotating.visual.model_word, "byte_160": derived.byte_160,
        "current_word_14c": derived.word_14c,
        "target_148_identity": derived.target_148.map(|reference| format!("{reference:?}")),
        "cache_prefix": { "allocation_address": format!("{:p}", &**cache_owner), "word_04": cache.word_04(), "word_06": cache.word_06(),
            "byte_08": cache.byte_08(), "dimension": cache.dimension(), "stride": cache.stride_words(),
            "byte_0b": cache.byte_0b(), "words": cache.words() },
        "h13": h13, "actor_root_h8": actor_model,
        "terrain": { "registration": h12.registration, "allocation_authenticated": true,
            "portable_allocation_identity": format!("{:p}", std::sync::Arc::as_ptr(h12.overlay)),
            "overlay_key": h12.overlay.key(), "overlay_path": h12.overlay.path(),
            "overlay_sha256": format!("{:x}", Sha256::digest(h12.overlay.bytes())),
            "sampling": format!("{:?}", terrain.sampling()), "spacing": terrain.sampling().spacing.units(),
            "miss_cell_raw": terrain.cell_at(point).raw() },
        "census": census,
        "bounds": "Read-only metadata at an explicit portable miss stage. A census is not an executed query. Native47C1D0/auxiliary/contact/retail acceptance is not supplied. Pair root is actor+08, counterpart candidate+0C." }),
    )
}

fn census(
    resident: &ResidentRuntime,
    terrain: &TerrainRuntime,
    point: [u16; 2],
    process_word: u16,
    actor_model: &Value,
    process: &ProcessFurnitureQueryState,
) -> Result<Value, String> {
    if actor_model["status"] != "owned" {
        return Ok(json!({ "stop": "required_actor_header", "cells": [] }));
    }
    let radius = actor_model["radius_word"]
        .as_u64()
        .ok_or("diagnostic owned actor radius absent")? as u16;
    if radius == 0 {
        return Ok(json!({ "stop": "actor_header_zero", "cells": [] }));
    }
    if process_word == 0 {
        return Ok(json!({ "stop": "process_radius_zero_after_actor_header", "cells": [] }));
    }
    //47C2D3 combined lowWORD and47C30A/334 lowWORD differences. Upper
    //register residues cancel before the accepted cell/count domain.
    let radius = radius.wrapping_add(process_word);
    let spacing = terrain.sampling().spacing.units();
    let mask = spacing - 1;
    let start = point.map(|v| v.wrapping_sub(radius).wrapping_add(mask) & !mask);
    let end = point.map(|v| v.wrapping_add(radius) & !mask);
    let counts = [
        usize::from(end[0].wrapping_sub(start[0]) / spacing) + 1,
        usize::from(end[1].wrapping_sub(start[1]) / spacing) + 1,
    ];
    let mut cells = vec![];
    let mut stop = "window_exhausted_without_active_counterpart";
    //This is an output bound, never a collision-service completion limit.
    const DIAGNOSTIC_CELL_LIMIT: usize = 128;
    'x: for xi in 0..counts[0] {
        for zi in 0..counts[1] {
            if cells.len() == DIAGNOSTIC_CELL_LIMIT {
                stop = "diagnostic_cell_limit_incomplete";
                break 'x;
            }
            let xz = [
                start[0].wrapping_add((xi as u16).wrapping_mul(spacing)),
                start[1].wrapping_add((zi as u16).wrapping_mul(spacing)),
            ];
            let cell = terrain.cell_at(xz);
            let mut record = json!({ "xz": xz, "cell_raw": cell.raw(), "ordinal": cells.len() });
            let lookup = if !cell.has_sparse_record() {
                //Current physical process payload; no per-probe zero,
                //FrameContext snapshot or47C1D0/query initialization.
                let fallback = process.fallback_payload();
                record["fallback"] = json!({ "source": "retained_process_005483c4",
                    "state": fallback.state, "lookup_byte_3": fallback.resource_lookup.raw() });
                fallback.resource_lookup
            } else {
                let Some((id, sparse)) = terrain.sparse_record_at(xz) else {
                    record["boundary"] = json!("required_sparse_pointer_with_mask_04_set");
                    cells.push(record);
                    stop = "required_sparse_pointer_with_mask_04_set";
                    break 'x;
                };
                record["sparse"] = json!({ "bucket": id.bucket, "index": id.index,
                    "x": sparse.x, "state": sparse.state, "lookup_byte_3": sparse.resource_lookup.raw() });
                sparse.resource_lookup
            };
            if lookup.raw() == 0 {
                record["boundary"] = json!("zero_lookup_skips_h11_and_counterpart");
                cells.push(record);
                continue;
            }
            let h11 = resident.resource(11, 0);
            let table = crate::world_furniture_effect::resident::resident_placement_table(resident);
            let table = match table {
                Ok(table) => table,
                Err(error) => {
                    record["boundary"] = json!("required_h11");
                    record["error"] = json!(error.to_string());
                    cells.push(record);
                    stop = "required_h11";
                    break 'x;
                }
            };
            record["h11_registration"] = json!(h11.as_ref().map(|r| r.registration));
            let Some(slot) = table.slots.get(usize::from(lookup.raw())) else {
                record["boundary"] = json!("required_h11_lookup_boundary");
                cells.push(record);
                stop = "required_h11_lookup_boundary";
                break 'x;
            };
            record["counterpart_model_word"] = json!(slot.resource_word);
            let counterpart = match model(resident, slot.resource_word) {
                Ok(model) => model,
                Err(error) => {
                    record["boundary"] = json!("required_counterpart_registration");
                    record["error"] = json!(error);
                    cells.push(record);
                    stop = "required_counterpart_registration";
                    break 'x;
                }
            };
            let status = counterpart["status"]
                .as_str()
                .ok_or("diagnostic counterpart status absent")?
                .to_owned();
            let active = match status.as_str() {
                "owned" => {
                    counterpart["radius_word"]
                        .as_u64()
                        .ok_or("diagnostic owned counterpart radius absent")?
                        != 0
                }
                "negative" | "above_capacity" | "loader_null" | "inclusive_boundary" => false,
                _ => return Err("diagnostic unknown counterpart status".into()),
            };
            record["counterpart_h8"] = counterpart;
            if status == "inclusive_boundary" {
                record["boundary"] = json!("required_counterpart_inclusive_boundary");
                cells.push(record);
                stop = "required_counterpart_inclusive_boundary";
                break 'x;
            }
            if active {
                record["boundary"] = json!("active_pair_requires_actual_auxiliary_query");
                cells.push(record);
                stop = "active_pair_requires_actual_auxiliary_query";
                break 'x;
            }
            record["boundary"] = json!("native_negative_above_null_or_zero_counterpart_skip");
            cells.push(record);
        }
    }
    Ok(
        json!({ "center": point, "combined_radius_low_word": radius, "start": start, "end": end,
        "counts_x_z": counts, "total_cells": counts[0] * counts[1], "stop": stop, "cells": cells,
        "execution": "This census executes no collision callback or auxiliary program; actual host execution, if any, is recorded separately by the packet stage." }),
    )
}
