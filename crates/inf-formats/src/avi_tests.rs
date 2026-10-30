use super::*;

fn build_test_avi(
    width: u32,
    height: u32,
    fourcc: &[u8; 4],
    frames: &[&[u8]],
    audio_chunks: &[&[u8]],
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(b"AVI ");

    let hdrl_start = data.len();
    data.extend_from_slice(b"LIST");
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(b"hdrl");

    data.extend_from_slice(b"avih");
    data.extend_from_slice(&56u32.to_le_bytes());
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&66666u32.to_le_bytes());
    avih[16..20].copy_from_slice(&(frames.len() as u32).to_le_bytes());
    avih[32..36].copy_from_slice(&width.to_le_bytes());
    avih[36..40].copy_from_slice(&height.to_le_bytes());
    data.extend_from_slice(&avih);

    // Video strl.
    let strl_start = data.len();
    data.extend_from_slice(b"LIST");
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(b"strl");
    data.extend_from_slice(b"strh");
    data.extend_from_slice(&56u32.to_le_bytes());
    let mut strh = vec![0u8; 56];
    strh[0..4].copy_from_slice(b"vids");
    strh[4..8].copy_from_slice(fourcc);
    strh[20..24].copy_from_slice(&1u32.to_le_bytes());
    strh[24..28].copy_from_slice(&15u32.to_le_bytes());
    data.extend_from_slice(&strh);
    // BITMAPINFOHEADER strf.
    data.extend_from_slice(b"strf");
    data.extend_from_slice(&40u32.to_le_bytes());
    let mut strf = vec![0u8; 40];
    strf[0..4].copy_from_slice(&40u32.to_le_bytes());
    strf[4..8].copy_from_slice(&width.to_le_bytes());
    strf[8..12].copy_from_slice(&(height as i32).to_le_bytes());
    strf[12..14].copy_from_slice(&1u16.to_le_bytes());
    strf[14..16].copy_from_slice(&24u16.to_le_bytes());
    strf[16..20].copy_from_slice(fourcc);
    data.extend_from_slice(&strf);
    let strl_size = data.len() - strl_start - 8;
    data[strl_start + 4..strl_start + 8].copy_from_slice(&(strl_size as u32).to_le_bytes());

    // Audio strl.
    let astrl_start = data.len();
    data.extend_from_slice(b"LIST");
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(b"strl");
    data.extend_from_slice(b"strh");
    data.extend_from_slice(&56u32.to_le_bytes());
    let mut astrh = vec![0u8; 56];
    astrh[0..4].copy_from_slice(b"auds");
    astrh[20..24].copy_from_slice(&4u32.to_le_bytes());
    astrh[24..28].copy_from_slice(&88200u32.to_le_bytes());
    data.extend_from_slice(&astrh);
    data.extend_from_slice(b"strf");
    data.extend_from_slice(&16u32.to_le_bytes());
    let mut astrf = vec![0u8; 16];
    astrf[0..2].copy_from_slice(&1u16.to_le_bytes());
    astrf[2..4].copy_from_slice(&2u16.to_le_bytes());
    astrf[4..8].copy_from_slice(&22050u32.to_le_bytes());
    astrf[8..12].copy_from_slice(&88200u32.to_le_bytes());
    astrf[12..14].copy_from_slice(&4u16.to_le_bytes());
    astrf[14..16].copy_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(&astrf);
    let astrl_size = data.len() - astrl_start - 8;
    data[astrl_start + 4..astrl_start + 8].copy_from_slice(&(astrl_size as u32).to_le_bytes());

    let hdrl_size = data.len() - hdrl_start - 8;
    data[hdrl_start + 4..hdrl_start + 8].copy_from_slice(&(hdrl_size as u32).to_le_bytes());

    // movi LIST.
    let movi_start = data.len();
    data.extend_from_slice(b"LIST");
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(b"movi");
    for frame in frames {
        data.extend_from_slice(b"00dc");
        data.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        data.extend_from_slice(frame);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    for audio in audio_chunks {
        data.extend_from_slice(b"01wb");
        data.extend_from_slice(&(audio.len() as u32).to_le_bytes());
        data.extend_from_slice(audio);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    let movi_size = data.len() - movi_start - 8;
    data[movi_start + 4..movi_start + 8].copy_from_slice(&(movi_size as u32).to_le_bytes());

    let riff_size = data.len() - 8;
    data[4..8].copy_from_slice(&(riff_size as u32).to_le_bytes());
    data
}

#[test]
fn rejects_non_riff() {
    assert_eq!(
        AviContainer::open(b"NOT_RIFF_DATA").unwrap_err(),
        AviError::NotRiff
    );
}

#[test]
fn rejects_non_avi() {
    let mut data = vec![0u8; 12];
    data[0..4].copy_from_slice(b"RIFF");
    data[8..12].copy_from_slice(b"WAVE");
    assert_eq!(AviContainer::open(&data).unwrap_err(), AviError::NotAvi);
}

#[test]
fn rejects_missing_video_stream() {
    let mut data = vec![0u8; 12];
    data[0..4].copy_from_slice(b"RIFF");
    data[8..12].copy_from_slice(b"AVI ");
    assert_eq!(
        AviContainer::open(&data).unwrap_err(),
        AviError::NoVideoStream
    );
}

#[test]
fn parses_iv50_container_without_pixel_decode() {
    let frames: &[&[u8]] = &[&[1, 2, 3, 4], &[5, 6, 7]];
    let audio: &[&[u8]] = &[&[10, 20, 30, 40]];
    let data = build_test_avi(640, 480, b"IV50", frames, audio);
    let avi = AviContainer::open(&data).unwrap();
    assert_eq!(avi.info.width, 640);
    assert_eq!(avi.info.height, 480);
    assert_eq!(avi.info.fps, 15.0);
    assert_eq!(avi.info.total_frames, 2);
    assert_eq!(avi.info.video_fourcc, *b"IV50");
    assert_eq!(avi.info.audio_rate, 22050);
    assert_eq!(avi.info.audio_channels, 2);
    assert_eq!(avi.info.audio_bits, 16);
    assert_eq!(avi.frame_count(), 2);
    assert_eq!(avi.video_chunk(0), Some(&[1, 2, 3, 4][..]));
    assert_eq!(avi.video_chunk(1), Some(&[5, 6, 7][..]));
    assert_eq!(avi.video_chunk(2), None);
    assert_eq!(avi.audio_pcm(), &[10, 20, 30, 40]);
}

#[test]
fn total_frames_falls_back_to_chunk_count() {
    let mut data = build_test_avi(8, 8, b"IV50", &[&[1, 2]], &[]);
    // Zero the avih total-frames field: avih body starts after "RIFF AVI LIST hdrl avih size".
    let avih_body = data
        .windows(4)
        .position(|w| w == b"avih")
        .map(|i| i + 8)
        .unwrap();
    data[avih_body + 16..avih_body + 20].copy_from_slice(&0u32.to_le_bytes());
    let avi = AviContainer::open(&data).unwrap();
    assert_eq!(avi.info.total_frames, 1);
}

#[test]
fn rejects_truncated_chunk_body() {
    let mut data = build_test_avi(8, 8, b"IV50", &[&[1, 2, 3, 4]], &[]);
    data.truncate(data.len() - 2);
    assert!(matches!(
        AviContainer::open(&data),
        Err(AviError::Truncated { .. })
    ));
}
