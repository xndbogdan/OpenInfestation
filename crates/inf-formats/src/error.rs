use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error(
        "unexpected end of input at byte {offset:#x} while reading {context}: needed {needed} bytes, input length is {input_len}"
    )]
    UnexpectedEof {
        offset: usize,
        needed: usize,
        input_len: usize,
        context: &'static str,
    },

    #[error("integer overflow at byte {offset:#x} while computing {context}")]
    IntegerOverflow {
        offset: usize,
        context: &'static str,
    },

    #[error("PreLoad.dat has {remaining} trailing bytes after byte {offset:#x}")]
    TrailingPreloadBytes { offset: usize, remaining: usize },

    #[error("invalid scene-tree node kind {kind} at byte {offset:#x}")]
    InvalidNodeKind { offset: usize, kind: u8 },

    #[error("scene-tree nesting exceeds the supported depth of {max_depth}")]
    SceneTreeTooDeep { max_depth: usize },

    #[error("invalid overlay filename {name:?}")]
    InvalidOverlayFilename { name: String },

    #[error("handler {handler} has unsupported negative request count {count}")]
    NegativeHandlerCount { handler: u8, count: i16 },

    #[error(
        "handler 3 atlas rectangle at byte {offset:#x} is inverted: ({x_start},{y_start})..({x_end},{y_end})"
    )]
    InvertedAtlasRectangle {
        offset: usize,
        x_start: u16,
        y_start: u16,
        x_end: u16,
        y_end: u16,
    },

    #[error(
        "handler 8 sub-block at byte {offset:#x} contains no entries before requested count {requested} was reached"
    )]
    EmptyModelSubBlock { offset: usize, requested: u32 },

    #[error(
        "handler 8 accumulated {actual} entries, but PreLoad.dat requested exactly {requested}"
    )]
    ModelEntryCountMismatch { requested: u32, actual: u32 },

    #[error("OVL has {remaining} trailing bytes after byte {offset:#x}")]
    TrailingOverlayBytes { offset: usize, remaining: usize },
}
