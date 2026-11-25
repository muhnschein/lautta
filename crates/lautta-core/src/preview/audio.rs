// SPDX-License-Identifier: LGPL-2.1-or-later

use crate::entry::{Entry, Kind};
use lofty::config::{apply_global_options, GlobalOptions, ParseOptions, ParsingMode};
use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::Tag;
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

pub const READ_BUDGET: u64 = 16 * 1024 * 1024;
const BLOCK: u64 = 64 * 1024;
const MAX_TEXT_CHARS: usize = 200;
const FOLDER_COVER_STEMS: [&str; 3] = ["cover", "folder", "front"];
const FOLDER_COVER_EXTS: [&str; 3] = ["jpg", "jpeg", "png"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u16>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioRead {
    pub tags: AudioTags,
    pub cover: Option<Vec<u8>>,
}

pub fn read_audio<R: Read + Seek>(reader: R, want_cover: bool) -> Option<AudioRead> {
    crate::panics::catch(move || parse(reader, want_cover))
        .ok()
        .flatten()
}

fn parse<R: Read + Seek>(reader: R, want_cover: bool) -> Option<AudioRead> {
    apply_global_options(GlobalOptions::new().allocation_limit(READ_BUDGET as usize));
    let options = ParseOptions::new()
        .read_properties(false)
        .read_cover_art(want_cover)
        .parsing_mode(ParsingMode::BestAttempt);
    let file = Probe::new(reader)
        .options(options)
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    let mut tags: Vec<&Tag> = file.primary_tag().into_iter().collect();
    tags.extend(
        file.tags()
            .iter()
            .filter(|t| Some(t.tag_type()) != file.primary_tag().map(Tag::tag_type)),
    );
    Some(AudioRead {
        tags: merged(&tags),
        cover: want_cover.then(|| cover(&tags)).flatten(),
    })
}

fn merged(tags: &[&Tag]) -> AudioTags {
    let text = |get: fn(&Tag) -> Option<std::borrow::Cow<'_, str>>| tags.iter().find_map(|t| clean(&get(t)?));
    AudioTags {
        title: text(|t| t.title()),
        artist: text(|t| t.artist()),
        album: text(|t| t.album()),
        year: tags
            .iter()
            .find_map(|t| t.date().map(|d| d.year).filter(|y| (1..=9999).contains(y))),
        track: tags.iter().find_map(|t| t.track().filter(|n| *n > 0)),
        track_total: tags.iter().find_map(|t| t.track_total().filter(|n| *n > 0)),
    }
}

fn cover(tags: &[&Tag]) -> Option<Vec<u8>> {
    let pictures = || {
        tags.iter()
            .flat_map(|t| t.pictures())
            .filter(|p| !p.data().is_empty())
    };
    pictures()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures().next())
        .map(|p| p.data().to_vec())
}

fn shown(c: char) -> bool {
    !c.is_control() && !matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

pub fn clean(raw: &str) -> Option<String> {
    let kept: String = raw
        .trim()
        .chars()
        .filter(|c| shown(*c))
        .take(MAX_TEXT_CHARS)
        .collect();
    let kept = kept.trim();
    (!kept.is_empty()).then(|| kept.to_owned())
}

pub fn folder_cover(entries: &[Entry]) -> Option<&Entry> {
    let rank = |e: &Entry| -> Option<usize> {
        if e.kind != Kind::File {
            return None;
        }
        let name = std::str::from_utf8(&e.name).ok()?.to_ascii_lowercase();
        let (stem, ext) = name.rsplit_once('.')?;
        FOLDER_COVER_EXTS.contains(&ext).then_some(())?;
        FOLDER_COVER_STEMS.iter().position(|s| *s == stem)
    };
    entries
        .iter()
        .filter_map(|e| Some((rank(e)?, e)))
        .min_by_key(|(r, _)| *r)
        .map(|(_, e)| e)
}

pub struct RangeReader<F> {
    fetch: F,
    size: u64,
    pos: u64,
    blocks: HashMap<u64, Vec<u8>>,
    budget: u64,
    fetched: u64,
}

impl<F: FnMut(u64, usize) -> io::Result<Vec<u8>>> RangeReader<F> {
    pub fn new(size: u64, budget: u64, fetch: F) -> Self {
        RangeReader {
            fetch,
            size,
            pos: 0,
            blocks: HashMap::new(),
            budget,
            fetched: 0,
        }
    }

    pub fn fetched(&self) -> u64 {
        self.fetched
    }

    fn block(&mut self, index: u64) -> io::Result<&[u8]> {
        if !self.blocks.contains_key(&index) {
            let start = index * BLOCK;
            let want = BLOCK.min(self.size - start);
            if self.fetched + want > self.budget {
                return Err(io::Error::other("audio read budget spent"));
            }
            let mut data = Vec::with_capacity(want as usize);
            while (data.len() as u64) < want {
                let chunk = (self.fetch)(start + data.len() as u64, (want - data.len() as u64) as usize)?;
                if chunk.is_empty() {
                    break;
                }
                let room = (want - data.len() as u64) as usize;
                data.extend_from_slice(&chunk[..chunk.len().min(room)]);
            }
            self.fetched += data.len() as u64;
            self.blocks.insert(index, data);
        }
        Ok(self.blocks.get(&index).map_or(&[][..], Vec::as_slice))
    }
}

impl<F: FnMut(u64, usize) -> io::Result<Vec<u8>>> Read for RangeReader<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.size || buf.is_empty() {
            return Ok(0);
        }
        let pos = self.pos;
        let data = self.block(pos / BLOCK)?;
        let from = (pos % BLOCK) as usize;
        let n = data.len().saturating_sub(from).min(buf.len());
        buf[..n].copy_from_slice(&data[from..from + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl<F> Seek for RangeReader<F> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(d) => self.size.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = target.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        Ok(self.pos)
    }
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
