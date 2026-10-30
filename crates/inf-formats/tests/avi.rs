use inf_formats::AviContainer;
use std::fs;
use std::path::Path;

#[test]
fn retail_title_avi_framing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let bytes = fs::read(root.join("Title.avi")).unwrap();
    let avi = AviContainer::open(&bytes).unwrap();
    assert_eq!(avi.info.width, 640);
    assert_eq!(avi.info.height, 480);
    assert_eq!(avi.info.fps, 15.0);
    assert_eq!(avi.info.total_frames, 315);
    assert_eq!(avi.info.video_fourcc, *b"IV50");
    assert_eq!(avi.info.audio_rate, 22050);
    assert_eq!(avi.info.audio_channels, 2);
    assert_eq!(avi.info.audio_bits, 16);
    assert_eq!(avi.frame_count(), 315);
    assert_eq!(avi.audio_pcm().len(), 1_852_200);
    assert!(avi.video_chunk(0).unwrap().len() == 2560);
}
