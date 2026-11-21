// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use crate::entry::Entry;
#[path = "../../tests/fixtures/audio.rs"]
#[allow(dead_code)]
mod fixtures;
use fixtures::{flac, jpeg, mp3};
use std::io::Cursor;

fn tagged_mp3(cover: Option<&[u8]>) -> Vec<u8> {
    mp3(
        &[
            ("TIT2", "Harbour Lights"),
            ("TPE1", "The Ferry Pilots"),
            ("TALB", "Crossings"),
            ("TDRC", "2024-03-02"),
            ("TRCK", "3/11"),
        ],
        cover,
        8,
    )
}

fn ranged(data: &[u8], budget: u64) -> RangeReader<impl FnMut(u64, usize) -> io::Result<Vec<u8>> + '_> {
    RangeReader::new(data.len() as u64, budget, move |off, max| {
        let start = (off as usize).min(data.len());
        Ok(data[start..(start + max).min(data.len())].to_vec())
    })
}

#[test]
fn mp3_tags_and_front_cover_are_read() {
    let art = jpeg(8, [200, 80, 40]);
    let read = read_audio(Cursor::new(tagged_mp3(Some(&art))), true).expect("parsed");
    assert_eq!(
        read.tags,
        AudioTags {
            title: Some("Harbour Lights".into()),
            artist: Some("The Ferry Pilots".into()),
            album: Some("Crossings".into()),
            year: Some(2024),
            track: Some(3),
            track_total: Some(11),
        }
    );
    assert_eq!(read.cover.as_deref(), Some(&art[..]));
}

#[test]
fn cover_is_left_out_when_not_wanted() {
    let read = read_audio(Cursor::new(tagged_mp3(Some(&jpeg(8, [1, 2, 3])))), false).expect("parsed");
    assert_eq!(read.cover, None);
    assert_eq!(read.tags.title.as_deref(), Some("Harbour Lights"));
}

#[test]
fn flac_vorbis_comments_and_picture_are_read() {
    let art = jpeg(8, [10, 200, 90]);
    let file = flac(
        &[
            "TITLE=Ice Road",
            "ARTIST=Lake Choir",
            "ALBUM=Winter",
            "DATE=1999",
            "TRACKNUMBER=7",
        ],
        Some(&art),
    );
    let read = read_audio(Cursor::new(file), true).expect("parsed");
    assert_eq!(read.tags.title.as_deref(), Some("Ice Road"));
    assert_eq!(read.tags.artist.as_deref(), Some("Lake Choir"));
    assert_eq!(read.tags.album.as_deref(), Some("Winter"));
    assert_eq!(read.tags.year, Some(1999));
    assert_eq!(read.tags.track, Some(7));
    assert_eq!(read.tags.track_total, None);
    assert_eq!(read.cover.as_deref(), Some(&art[..]));
}

#[test]
fn untagged_file_has_empty_tags_and_no_cover() {
    let read = read_audio(Cursor::new(mp3(&[], None, 8)), true).expect("parsed");
    assert_eq!(read.tags, AudioTags::default());
    assert_eq!(read.cover, None);
}

#[test]
fn not_audio_is_none() {
    assert_eq!(
        read_audio(Cursor::new(b"just some text, not audio".to_vec()), true),
        None
    );
}

#[test]
fn tag_text_is_plain_trimmed_and_capped() {
    assert_eq!(
        clean("  Harbour\u{0007} Lights \n"),
        Some("Harbour Lights".into())
    );
    assert_eq!(clean("\u{202E}evil\u{2066}"), Some("evil".into()));
    assert_eq!(clean(" \t\u{0000} "), None);
    assert_eq!(
        clean("emoji 👩\u{200D}🚀 stays"),
        Some("emoji 👩\u{200D}🚀 stays".into())
    );
    assert_eq!(
        clean(&"x".repeat(500)).map(|s| s.chars().count()),
        Some(MAX_TEXT_CHARS)
    );
    let read = read_audio(
        Cursor::new(mp3(&[("TIT2", "<b>bold</b>\u{001B}[31m")], None, 4)),
        false,
    )
    .unwrap();
    assert_eq!(
        read.tags.title.as_deref(),
        Some("<b>bold</b>[31m"),
        "markup kept as text"
    );
}

#[test]
fn bad_numbers_are_dropped() {
    let read = read_audio(
        Cursor::new(mp3(&[("TRCK", "0/0"), ("TDRC", "0000")], None, 4)),
        false,
    )
    .unwrap();
    assert_eq!(
        (read.tags.track, read.tags.track_total, read.tags.year),
        (None, None, None)
    );
}

#[test]
fn a_big_file_costs_only_its_tag_blocks() {
    // ~20 MB of audio after the tag: parser must seek past it, not read it.
    let file = tagged_mp3(Some(&jpeg(64, [9, 9, 9])));
    let mut big = file.clone();
    big.extend(std::iter::repeat_n(0u8, 20 * 1024 * 1024));
    let mut reader = ranged(&big, READ_BUDGET);
    let read = parse(&mut reader, true).expect("parsed");
    assert_eq!(read.tags.title.as_deref(), Some("Harbour Lights"));
    assert!(read.cover.is_some());
    assert!(
        reader.fetched() <= 4 * BLOCK,
        "fetched {} bytes",
        reader.fetched()
    );
}

#[test]
fn over_budget_read_fails_instead_of_fetching_more() {
    let art = vec![0xAB; 300 * 1024];
    let file = tagged_mp3(Some(&art));
    let mut reader = ranged(&file, 2 * BLOCK);
    assert_eq!(parse(&mut reader, true), None);
    assert!(reader.fetched() <= 2 * BLOCK);
}

#[test]
fn range_reader_reads_and_seeks_like_a_cursor() {
    let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let mut calls = 0;
    let mut r = RangeReader::new(data.len() as u64, u64::MAX, |off, max| {
        calls += 1;
        // Short reads: at most 1000 bytes per call.
        let s = off as usize;
        Ok(data[s..(s + max.min(1000)).min(data.len())].to_vec())
    });
    let mut buf = [0u8; 10];
    r.seek(SeekFrom::Start(70_000)).unwrap();
    r.read_exact(&mut buf).unwrap();
    assert_eq!(buf[..], data[70_000..70_010]);
    assert_eq!(r.seek(SeekFrom::End(-5)).unwrap(), 199_995);
    let mut tail = Vec::new();
    r.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, data[199_995..]);
    assert_eq!(r.seek(SeekFrom::Current(-100)).unwrap(), 199_900);
    assert!(r.seek(SeekFrom::Current(-300_000)).is_err());
    assert_eq!(r.seek(SeekFrom::Start(500_000)).unwrap(), 500_000);
    assert_eq!(r.read(&mut buf).unwrap(), 0, "past the end reads nothing");
    assert_eq!(r.fetched(), BLOCK + (200_000 - 3 * BLOCK));
    r.seek(SeekFrom::Start(70_100)).unwrap();
    r.read_exact(&mut buf).unwrap();
    assert_eq!(
        r.fetched(),
        BLOCK + (200_000 - 3 * BLOCK),
        "kept block not fetched again"
    );
}

#[test]
fn range_reader_stops_at_an_early_end() {
    let data = vec![7u8; 1000];
    let mut r = RangeReader::new(5000, u64::MAX, |off, max| {
        let s = (off as usize).min(data.len());
        Ok(data[s..(s + max).min(data.len())].to_vec())
    });
    let mut all = Vec::new();
    r.read_to_end(&mut all).unwrap();
    assert_eq!(all.len(), 1000);
}

#[test]
fn range_reader_passes_fetch_errors_on() {
    let mut r = RangeReader::new(100, u64::MAX, |_, _| Err(io::Error::other("gone")));
    assert!(r.read(&mut [0u8; 4]).is_err());
}

fn file(name: &str) -> Entry {
    Entry::new(name.as_bytes(), Kind::File)
}

#[test]
fn folder_cover_prefers_cover_then_folder_then_front() {
    let names = |es: &[Entry]| folder_cover(es).map(|e| String::from_utf8_lossy(&e.name).into_owned());
    let mut es = vec![file("song.mp3"), file("Front.PNG"), file("folder.jpeg")];
    assert_eq!(names(&es).as_deref(), Some("folder.jpeg"));
    es.push(file("COVER.JPG"));
    assert_eq!(names(&es).as_deref(), Some("COVER.JPG"));
    assert_eq!(names(&[file("Front.PNG")]).as_deref(), Some("Front.PNG"));
    assert_eq!(
        names(&[file("cover.gif"), file("covers.jpg"), file("cover")]),
        None
    );
    assert_eq!(
        names(&[Entry::new(b"cover.jpg", Kind::Dir)]),
        None,
        "folders never"
    );
}
