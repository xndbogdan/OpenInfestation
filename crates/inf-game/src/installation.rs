use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use inf_formats::{parse_overlay, Preload, RequestKind, RELOCATION_PHASE_ORDER};

use crate::{Error, HandlerResources, LoadedOverlay, LoadedScene, Result, SceneContext};

pub(crate) enum OverlayPhysicalReads<'a> {
    NoCallbacks,
    Loading {
        state: &'a mut crate::loading_read::ResourceReadState,
        host: &'a mut dyn crate::loading_read::ResourceReadHost,
    },
}

/// A validated `PreLoad.dat` and the explicit root of its retail data.
/// Files are opened read-only; no extracted assets or executable are required.
#[derive(Clone, Debug)]
pub struct Installation {
    data_root: PathBuf,
    preload_bytes: Vec<u8>,
    preload: Preload,
}

impl Installation {
    pub fn open(data_root: &Path) -> Result<Self> {
        let path = retail_path(data_root, "PreLoad.dat");
        let preload_bytes = read_required(&path)?;
        let preload = Preload::parse(&preload_bytes).map_err(|source| Error::Format {
            path: path.clone(),
            source,
        })?;
        validate_requests(&preload)?;
        Ok(Self {
            data_root: data_root.to_owned(),
            preload_bytes,
            preload,
        })
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn preload(&self) -> &Preload {
        &self.preload
    }

    pub fn preload_bytes(&self) -> &[u8] {
        &self.preload_bytes
    }

    /// Build one independent scene in `FUN_004EE680` phase order. A phase
    /// with no requested handlers consumes no file, even if one exists.
    pub fn prepare_scene(&self, context: SceneContext) -> Result<LoadedScene> {
        let mut scene = LoadedScene::new(context, self.preload.header.handler_capacities);
        let mut reads = OverlayPhysicalReads::NoCallbacks;
        for phase in RELOCATION_PHASE_ORDER {
            if let Some(overlay) = self.prepare_overlay(context, phase, &mut reads)? {
                scene.register_overlay(overlay);
            }
        }
        Ok(scene)
    }

    /// Shared physical loading boundary for isolated preparation and native
    /// residency/reload. Zero-request phases do not read a file (004EE4E0).
    pub(crate) fn prepare_overlay(
        &self,
        context: SceneContext,
        phase: RequestKind,
        reads: &mut OverlayPhysicalReads<'_>,
    ) -> Result<Option<LoadedOverlay>> {
        let requests = context
            .request_tables(&self.preload)
            .ok_or(Error::UnknownScene {
                level: context.level,
            })?;
        let index = crate::scene::phase_index(phase);
        let table = requests[index];
        if !table.has_active_handlers() {
            return Ok(None);
        }
        let key = context.overlay_keys()[index].clone();
        let path = retail_path(
            &self.data_root,
            Path::new("Overlay").join(&context.overlay_filenames()[index]),
        );
        let bytes = match reads {
            OverlayPhysicalReads::NoCallbacks => read_required(&path)?,
            OverlayPhysicalReads::Loading { state, host } => {
                // WholeFileReadCadenceApproximation: real physical reads use
                // 004F3DE0's adaptive policy, grouped as one OVL request.
                // Native header/handler block grouping remains separate.
                use crate::loading_read::{read_resource_4f3de0, ResourceReadRequest};
                let read_error = |source| Error::Read {
                    path: path.clone(),
                    source,
                };
                let mut file = fs::File::open(&path).map_err(read_error)?;
                let length = file.metadata().map_err(read_error)?.len();
                let total = u32::try_from(length).map_err(|_| {
                    read_error(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "resource file exceeds the native DWORD read domain",
                    ))
                })?;
                let mut bytes = vec![0; total as usize];
                read_resource_4f3de0(ResourceReadRequest {
                    state: &mut **state,
                    source: &mut file,
                    destination: &mut bytes,
                    host: &mut **host,
                })
                .map_err(|source| Error::LoadingRead {
                    path: path.clone(),
                    source,
                })?;
                bytes
            }
        };
        let layout = parse_overlay(&bytes, table).map_err(|source| Error::Format {
            path: path.clone(),
            source,
        })?;
        let handlers = layout
            .regions
            .iter()
            .map(|region| HandlerResources::parse(&bytes, region, table, &key, &path))
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(LoadedOverlay {
            key,
            path,
            bytes,
            requests: table.clone(),
            layout,
            handlers,
        }))
    }
}

/// `root.join(relative)` for a retail file, named as Windows (where retail
/// ran) matches names: the exact path when it exists, else each component in
/// turn by [`find_child`]; when a part is missing, the exact path, so errors
/// and missing-file checks name the file the port expects.
pub fn retail_path(root: &Path, relative: impl AsRef<Path>) -> PathBuf {
    let exact = root.join(&relative);
    if exact.exists() {
        return exact;
    }
    relative
        .as_ref()
        .components()
        .try_fold(root.to_path_buf(), |dir, part| {
            find_child(&dir, part.as_os_str())
        })
        .unwrap_or(exact)
}

/// OpenV2K's `setup::find_child` with the exact name first: `parent`'s entry
/// `name`, else the one whose name equals it ignoring ASCII case (of two that
/// differ only in case, the lowest name, so every machine with the same files
/// loads the same one, whatever order the filesystem lists them in).
fn find_child(parent: &Path, name: &OsStr) -> Option<PathBuf> {
    let exact = parent.join(name);
    if exact.exists() {
        return Some(exact);
    }
    any_case_child(parent, name)
}

/// [`find_child`]'s scan of `parent`'s entries, past the exact try.
// shortcut: one directory scan per wrong-case name; cache the listing if an
// all-upper-case Overlay folder makes the launcher's check slow.
fn any_case_child(parent: &Path, name: &OsStr) -> Option<PathBuf> {
    fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .filter(|found| found.eq_ignore_ascii_case(name))
        .min()
        .map(|found| parent.join(found))
}

fn read_required(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })
}

/// Validate the entire installation's registration plan before allocating any
/// scene. Signed deltas may map backwards and overwrite earlier local entries;
/// neither aliases nor holes in the global pool are errors.
fn validate_requests(preload: &Preload) -> Result<()> {
    let mut levels = HashSet::new();
    for leaf in preload.leaves() {
        let level = leaf.overlay_id;
        if !levels.insert(level) {
            return Err(Error::DuplicateScene { level });
        }
        for phase in RELOCATION_PHASE_ORDER {
            for (handler, request) in leaf.requests(phase).iter() {
                let handler = handler as u8;
                let count = u16::try_from(request.count).map_err(|_| Error::NegativeCount {
                    level,
                    phase,
                    handler,
                    count: request.count,
                })?;
                for local_index in 0..count {
                    let global_index =
                        request
                            .global_index(local_index)
                            .ok_or(Error::InvalidMapping {
                                level,
                                phase,
                                handler,
                                local_index,
                            })?;
                    let capacity = preload.header.handler_capacities[handler as usize];
                    if global_index >= capacity {
                        return Err(Error::Capacity {
                            level,
                            phase,
                            handler,
                            local_index,
                            global_index,
                            capacity,
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "installation_tests.rs"]
mod tests;
