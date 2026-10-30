//! RIFF/AVI container framing for retail video sources (`Title.avi`).
//!
//! Native lifecycle, recovered from the supported retail `Infest.exe` and the
//! installed `Title.avi` bytes (see the title-handoff section of
//! [frontend runtime](../../../docs/re/FRONTEND_RUNTIME.md)):
//!
//! - Graph construction `00494AE0` opens the AVI through DirectShow; status
//!   poll `00494DB0` reads handle `+18` with 2 meaning finished; per-frame
//!   draw `00494F80` presents one frame; close `00494DC0` releases the graph.
//! - `Title.avi` is a standard RIFF AVI: `avih` (15 fps, 315 frames,
//!   640x480), a `vids` stream with handler `IV50` (Indeo 5) plus a
//!   `BITMAPINFOHEADER` pixel description, and an `auds` stream with
//!   `WAVEFORMATEX` PCM stereo 22050 Hz 16-bit.
//! - The `movi` list interleaves `00dc` video chunks (315) and `01wb` audio
//!   chunks (21 x 88200 bytes = 1,852,200 bytes PCM); `idx1` mirrors that
//!   order.
//!
//! This owner frames the container only: stream metadata, raw video-chunk
//! bytes in presentation order, and concatenated PCM audio bytes. IV50 pixel
//! decoding has no portable owner yet and stays an explicit boundary: there
//! is no `decode_frame` method here. A future decoder must document its
//! reference comparison against the TTD `ir50_32.dll` output buffers before
//! pixel acceptance can be claimed.

use thiserror::Error;

/// Parsed AVI stream metadata.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AviInfo {
    pub width: u32,
    pub height: u32,
    /// Frames per second from `strh` rate/scale (falls back to `avih`).
    pub fps: f32,
    /// Total video frames from `avih`, falling back to the chunk count.
    pub total_frames: u32,
    pub audio_rate: u32,
    pub audio_channels: u16,
    pub audio_bits: u16,
    /// Raw `vids` handler FourCC (e.g. `IV50`); pixel decode stays open.
    pub video_fourcc: [u8; 4],
}

/// AVI container with raw chunks: video in presentation order plus
/// concatenated PCM audio.
#[derive(Clone, Debug)]
pub struct AviContainer {
    pub info: AviInfo,
    video_chunks: Vec<Vec<u8>>,
    audio_data: Vec<u8>,
}

impl AviContainer {
    /// Total number of video (`00dc`) chunks available.
    pub fn frame_count(&self) -> usize {
        self.video_chunks.len()
    }

    /// Raw bytes of one video chunk in presentation order, or `None` when
    /// out of range. Bytes are the stored IV50 payload, not decoded pixels.
    pub fn video_chunk(&self, index: usize) -> Option<&[u8]> {
        self.video_chunks.get(index).map(Vec::as_slice)
    }

    /// Concatenated PCM audio bytes from every `01wb` chunk.
    pub fn audio_pcm(&self) -> &[u8] {
        &self.audio_data
    }

    /// Parse an AVI file from raw bytes.
    pub fn open(data: &[u8]) -> Result<Self, AviError> {
        if data.len() < 12 {
            return Err(AviError::NotRiff);
        }
        if &data[0..4] != b"RIFF" {
            return Err(AviError::NotRiff);
        }
        if &data[8..12] != b"AVI " {
            return Err(AviError::NotAvi);
        }

        let mut state = AviParseState::default();
        parse_chunks(data, 12, data.len(), &mut state)?;

        if !state.found_video {
            return Err(AviError::NoVideoStream);
        }
        if state.total_frames == 0 {
            state.total_frames = state.video_chunks.len() as u32;
        }

        Ok(Self {
            info: AviInfo {
                width: state.width,
                height: state.height,
                fps: state.fps,
                total_frames: state.total_frames,
                audio_rate: state.audio_rate,
                audio_channels: state.audio_channels,
                audio_bits: state.audio_bits,
                video_fourcc: state.video_fourcc,
            },
            video_chunks: state.video_chunks,
            audio_data: state.audio_data,
        })
    }
}

/// Errors during AVI container framing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AviError {
    #[error("not a RIFF file")]
    NotRiff,
    #[error("not an AVI file")]
    NotAvi,
    #[error("no video stream found")]
    NoVideoStream,
    #[error("truncated input at byte {offset:#x} while reading {context}")]
    Truncated {
        offset: usize,
        context: &'static str,
    },
}

#[derive(Default)]
struct AviParseState {
    width: u32,
    height: u32,
    fps: f32,
    total_frames: u32,
    audio_rate: u32,
    audio_channels: u16,
    audio_bits: u16,
    video_fourcc: [u8; 4],
    found_video: bool,
    video_chunks: Vec<Vec<u8>>,
    audio_data: Vec<u8>,
    current_stream: Option<[u8; 4]>,
}

impl Default for AviInfo {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            fps: 15.0,
            total_frames: 0,
            audio_rate: 22050,
            audio_channels: 1,
            audio_bits: 8,
            video_fourcc: [0; 4],
        }
    }
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, AviError> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(AviError::Truncated {
            offset,
            context: "u32",
        })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, AviError> {
    data.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(AviError::Truncated {
            offset,
            context: "u16",
        })
}

fn read_tag(data: &[u8], offset: usize) -> Result<[u8; 4], AviError> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(|b: [u8; 4]| b)
        .ok_or(AviError::Truncated {
            offset,
            context: "chunk tag",
        })
}

fn parse_chunks(
    data: &[u8],
    start: usize,
    end: usize,
    state: &mut AviParseState,
) -> Result<(), AviError> {
    let mut pos = start;
    while pos + 8 <= end {
        let tag = read_tag(data, pos)?;
        let size = read_u32(data, pos + 4)? as usize;
        let chunk_end =
            pos.checked_add(8)
                .and_then(|v| v.checked_add(size))
                .ok_or(AviError::Truncated {
                    offset: pos,
                    context: "chunk extent",
                })?;
        if chunk_end > end {
            return Err(AviError::Truncated {
                offset: pos,
                context: "chunk body",
            });
        }

        match &tag {
            b"LIST" => {
                if pos + 12 > end {
                    return Err(AviError::Truncated {
                        offset: pos,
                        context: "LIST type",
                    });
                }
                let list_type = read_tag(data, pos + 8)?;
                match &list_type {
                    b"hdrl" | b"strl" | b"movi" => {
                        parse_chunks(data, pos + 12, chunk_end.min(end), state)?;
                    }
                    _ => {}
                }
            }
            b"avih" => {
                if size < 40 {
                    return Err(AviError::Truncated {
                        offset: pos,
                        context: "avih",
                    });
                }
                let body = pos + 8;
                let usec = read_u32(data, body)?;
                if usec > 0 {
                    state.fps = 1_000_000.0 / usec as f32;
                }
                state.total_frames = read_u32(data, body + 16)?;
                state.width = read_u32(data, body + 32)?;
                state.height = read_u32(data, body + 36)?;
            }
            b"strh" => {
                if size < 32 {
                    return Err(AviError::Truncated {
                        offset: pos,
                        context: "strh",
                    });
                }
                let body = pos + 8;
                let stream_type = read_tag(data, body)?;
                let handler = read_tag(data, body + 4)?;
                state.current_stream = Some(stream_type);
                if &stream_type == b"vids" {
                    state.found_video = true;
                    state.video_fourcc = handler;
                    let scale = read_u32(data, body + 20)?;
                    let rate = read_u32(data, body + 24)?;
                    if scale > 0 {
                        state.fps = rate as f32 / scale as f32;
                    }
                }
            }
            b"strf" => {
                let body = pos + 8;
                match state.current_stream.as_ref() {
                    Some(b"vids") => {
                        if size < 40 {
                            return Err(AviError::Truncated {
                                offset: pos,
                                context: "strf BITMAPINFOHEADER",
                            });
                        }
                        state.width = read_u32(data, body + 4)?;
                        let height = read_u32(data, body + 8)? as i32;
                        state.height = height.unsigned_abs();
                        let compression = read_tag(data, body + 16)?;
                        if compression != [0, 0, 0, 0] {
                            state.video_fourcc = compression;
                        }
                    }
                    Some(b"auds") => {
                        if size < 16 {
                            return Err(AviError::Truncated {
                                offset: pos,
                                context: "strf WAVEFORMATEX",
                            });
                        }
                        state.audio_channels = read_u16(data, body + 2)?;
                        state.audio_rate = read_u32(data, body + 4)?;
                        state.audio_bits = read_u16(data, body + 14)?;
                    }
                    _ => {}
                }
            }
            _ => {
                if tag[2] == b'd' && tag[3] == b'c' {
                    state.video_chunks.push(data[pos + 8..chunk_end].to_vec());
                } else if tag[2] == b'w' && tag[3] == b'b' {
                    state
                        .audio_data
                        .extend_from_slice(&data[pos + 8..chunk_end]);
                }
            }
        }

        pos = chunk_end;
        if pos % 2 != 0 {
            pos += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "avi_tests.rs"]
mod tests;
