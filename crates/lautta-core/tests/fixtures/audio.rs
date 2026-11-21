// SPDX-License-Identifier: LGPL-2.1-or-later
//! Hand-assembled audio files for tests: MP3 (ID3v2.4 then silent MPEG frames) and FLAC (Vorbis
//! comment, picture block).

/// Text frames by ID (`TIT2`, `TPE1`, `TALB`, `TDRC`, `TRCK`), UTF-8.
pub fn mp3(texts: &[(&str, &str)], cover: Option<&[u8]>, audio_frames: usize) -> Vec<u8> {
    let pictures: Vec<(u8, &[u8])> = cover.into_iter().map(|c| (3, c)).collect();
    mp3_pictures(texts, &pictures, audio_frames)
}

/// As [`mp3`], pictures as (ID3 picture type, JPEG); 3 = front cover.
pub fn mp3_pictures(texts: &[(&str, &str)], pictures: &[(u8, &[u8])], audio_frames: usize) -> Vec<u8> {
    let mut frames = Vec::new();
    for (id, value) in texts {
        let mut body = vec![3u8];
        body.extend_from_slice(value.as_bytes());
        id3_frame(&mut frames, id, &body);
    }
    for (i, (kind, data)) in pictures.iter().enumerate() {
        let mut body = vec![3u8];
        body.extend_from_slice(b"image/jpeg\0");
        body.push(*kind);
        // Descriptions must differ between pictures.
        body.extend_from_slice(i.to_string().as_bytes());
        body.push(0);
        body.extend_from_slice(data);
        id3_frame(&mut frames, "APIC", &body);
    }
    let mut out = b"ID3\x04\x00\x00".to_vec();
    out.extend_from_slice(&synchsafe(frames.len() as u32));
    out.extend_from_slice(&frames);
    for _ in 0..audio_frames {
        // MPEG-1 layer III, 128 kbit/s, 44.1 kHz: 417 bytes per frame.
        out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        out.extend(std::iter::repeat_n(0u8, 413));
    }
    out
}

fn synchsafe(n: u32) -> [u8; 4] {
    [
        (n >> 21) as u8 & 0x7F,
        (n >> 14) as u8 & 0x7F,
        (n >> 7) as u8 & 0x7F,
        n as u8 & 0x7F,
    ]
}

fn id3_frame(out: &mut Vec<u8>, id: &str, body: &[u8]) {
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(&synchsafe(body.len() as u32));
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(body);
}

/// Vorbis comments `KEY=value`, optional front cover (JPEG).
pub fn flac(comments: &[&str], cover: Option<&[u8]>) -> Vec<u8> {
    let mut out = b"fLaC".to_vec();
    let mut info = Vec::new();
    info.extend_from_slice(&4096u16.to_be_bytes());
    info.extend_from_slice(&4096u16.to_be_bytes());
    info.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    // 44100 Hz, 2 channels, 16 bits, 44100 samples.
    let packed: u64 = (44100u64 << 44) | (1u64 << 41) | (15u64 << 36) | 44100;
    info.extend_from_slice(&packed.to_be_bytes());
    info.extend_from_slice(&[0u8; 16]);
    let mut vc = Vec::new();
    let vendor = b"lautta-test";
    vc.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    vc.extend_from_slice(vendor);
    vc.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for c in comments {
        vc.extend_from_slice(&(c.len() as u32).to_le_bytes());
        vc.extend_from_slice(c.as_bytes());
    }
    let mut blocks: Vec<(u8, Vec<u8>)> = vec![(0, info), (4, vc)];
    if let Some(data) = cover {
        let mut pic = Vec::new();
        pic.extend_from_slice(&3u32.to_be_bytes());
        let mime = b"image/jpeg";
        pic.extend_from_slice(&(mime.len() as u32).to_be_bytes());
        pic.extend_from_slice(mime);
        pic.extend_from_slice(&0u32.to_be_bytes());
        pic.extend_from_slice(&[0u8; 16]);
        pic.extend_from_slice(&(data.len() as u32).to_be_bytes());
        pic.extend_from_slice(data);
        blocks.push((6, pic));
    }
    let last = blocks.len() - 1;
    for (i, (kind, body)) in blocks.into_iter().enumerate() {
        let flag = if i == last { 0x80 } else { 0 };
        out.push(flag | kind);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        out.extend_from_slice(&body);
    }
    // One frame header's worth of audio.
    out.extend_from_slice(&[0xFF, 0xF8, 0x69, 0x08, 0x00, 0x00]);
    out.extend(std::iter::repeat_n(0u8, 64));
    out
}

/// Small solid JPEG, `side` square.
pub fn jpeg(side: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(side, side, image::Rgb(rgb));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageOutputFormat::Jpeg(90))
        .unwrap();
    out.into_inner()
}
