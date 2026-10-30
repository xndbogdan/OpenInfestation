use std::path::PathBuf;

use inf_formats::RequestKind;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot read required resource file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("loading resource file {path}: {source}")]
    LoadingRead {
        path: PathBuf,
        #[source]
        source: crate::loading_read::ResourceReadError,
    },
    #[error("invalid resource file {path}: {source}")]
    Format {
        path: PathBuf,
        #[source]
        source: inf_formats::Error,
    },
    #[error("invalid handler {handler} in {path}: {source}")]
    Handler {
        path: PathBuf,
        handler: u8,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("scene {level} is absent from PreLoad.dat")]
    UnknownScene { level: u8 },
    #[error("PreLoad.dat contains duplicate scene {level}")]
    DuplicateScene { level: u8 },
    #[error("scene {level} {phase:?} handler {handler} has negative request count {count}")]
    NegativeCount {
        level: u8,
        phase: RequestKind,
        handler: u8,
        count: i16,
    },
    #[error(
        "scene {level} {phase:?} handler {handler} local {local_index} has no valid global mapping"
    )]
    InvalidMapping {
        level: u8,
        phase: RequestKind,
        handler: u8,
        local_index: u16,
    },
    #[error("scene {level} {phase:?} handler {handler} local {local_index} maps to global {global_index}, outside capacity {capacity}")]
    Capacity {
        level: u8,
        phase: RequestKind,
        handler: u8,
        local_index: u16,
        global_index: u16,
        capacity: u16,
    },
}

impl From<inf_formats::SceneTreeError> for Error {
    fn from(error: inf_formats::SceneTreeError) -> Self {
        match error {
            inf_formats::SceneTreeError::UnknownScene { level } => Self::UnknownScene { level },
            inf_formats::SceneTreeError::DuplicateScene { level } => Self::DuplicateScene { level },
        }
    }
}
