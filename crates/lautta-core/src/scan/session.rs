// SPDX-License-Identifier: LGPL-2.1-or-later

use super::pdf::{PageImage, PdfWriter, Pixels};
use super::tone::{black_white, grayscale, pack_bits};
use super::warp::{output_size, warp};
use super::{detect, Colour, Quad};
use crate::error::{Error, ErrorKind, Result};
use crate::paths::AppPaths;
use crate::preview::exif;
use crate::thumbs::apply_orientation;
use image::codecs::jpeg::{JpegDecoder, JpegEncoder};
use image::imageops::{self, FilterType};
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use serde::Serialize;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const WORK_SIDE: u32 = 1600;
pub const PAGE_SIDE: u32 = 2339;
const DECODE_SIDE: u32 = 3200;
const MAX_PIXELS: u64 = 100_000_000;
const WORK_QUALITY: u8 = 85;
const PAGE_QUALITY: u8 = 80;

fn bad_picture(e: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::Unsupported, format!("not a usable picture: {e}"))
}

fn no_page() -> Error {
    Error::new(ErrorKind::NotFound, "no such page")
}

fn load_upright(path: &Path) -> Result<RgbImage> {
    let bytes = std::fs::read(path)?;
    let reader = image::io::Reader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(bad_picture)?;
    let jpeg = reader.format() == Some(ImageFormat::Jpeg);
    let (w, h) = reader.into_dimensions().map_err(bad_picture)?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(bad_picture("too large"));
    }
    let img = if jpeg {
        let mut decoder = JpegDecoder::new(Cursor::new(&bytes)).map_err(bad_picture)?;
        let side = u16::try_from(DECODE_SIDE).unwrap_or(u16::MAX);
        decoder.scale(side, side).map_err(bad_picture)?;
        DynamicImage::from_decoder(decoder)
    } else {
        image::load_from_memory(&bytes)
    }
    .map_err(bad_picture)?;
    Ok(apply_orientation(img, exif::orientation(&bytes)).into_rgb8())
}

fn jpeg<I: image::GenericImageView>(img: &I, quality: u8) -> Result<Vec<u8>>
where
    I::Pixel: image::PixelWithColorType,
{
    let mut data = Vec::new();
    JpegEncoder::new_with_quality(&mut data, quality)
        .encode_image(img)
        .map_err(bad_picture)?;
    Ok(data)
}

enum Rendered {
    Colour(RgbImage),
    Gray(GrayImage),
    Ink(GrayImage),
}

fn render(src: &RgbImage, page: &Page, max_long: u32) -> Rendered {
    let (w, h) = output_size(&page.quad, src.width(), src.height(), max_long);
    let flat = warp(src, &page.quad, w, h);
    let turned = match page.turns {
        1 => imageops::rotate90(&flat),
        2 => imageops::rotate180(&flat),
        3 => imageops::rotate270(&flat),
        _ => flat,
    };
    match page.colour {
        Colour::Original => Rendered::Colour(turned),
        Colour::Grayscale => Rendered::Gray(grayscale(&turned)),
        Colour::BlackWhite => Rendered::Ink(black_white(&grayscale(&turned))),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Page {
    id: u32,
    shot: u32,
    quad: Quad,
    turns: u8,
    colour: Colour,
    found: bool,
    revision: u32,
    view: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageInfo {
    pub id: u32,
    pub picture: String,
    pub view: String,
    pub quad: [f32; 8],
    pub turns: u8,
    pub colour: &'static str,
    pub found: bool,
}

pub struct ScanSession {
    dir: TempDir,
    pages: Vec<Page>,
    next_id: u32,
    next_shot: u32,
}

impl ScanSession {
    pub fn create(paths: &AppPaths) -> Result<ScanSession> {
        let root = paths.scan_dir();
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root)?;
        Ok(ScanSession {
            dir: tempfile::Builder::new().prefix("scan-").tempdir_in(&root)?,
            pages: Vec::new(),
            next_id: 1,
            next_shot: 1,
        })
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    fn photo(&self, shot: u32) -> PathBuf {
        self.dir().join(format!("photo-{shot}.jpg"))
    }

    fn work(&self, shot: u32) -> PathBuf {
        self.dir().join(format!("work-{shot}.jpg"))
    }

    fn page(&self, index: usize) -> Result<&Page> {
        self.pages.get(index).ok_or_else(no_page)
    }

    pub fn position(&self, id: u32) -> Option<usize> {
        self.pages.iter().position(|p| p.id == id)
    }

    pub fn pages(&self) -> Vec<PageInfo> {
        self.pages
            .iter()
            .map(|p| PageInfo {
                id: p.id,
                picture: self.work(p.shot).display().to_string(),
                view: p.view.display().to_string(),
                quad: p.quad.to_flat(),
                turns: p.turns,
                colour: p.colour.name(),
                found: p.found,
            })
            .collect()
    }

    fn refresh(&self, page: &mut Page) -> Result<()> {
        let work = image::open(self.work(page.shot))
            .map_err(bad_picture)?
            .into_rgb8();
        let revision = page.revision + 1;
        let (name, data) = match render(&work, page, WORK_SIDE) {
            Rendered::Colour(img) => ("jpg", jpeg(&img, WORK_QUALITY)?),
            Rendered::Gray(img) => ("jpg", jpeg(&img, WORK_QUALITY)?),
            Rendered::Ink(img) => {
                let mut out = Cursor::new(Vec::new());
                img.write_to(&mut out, ImageFormat::Png).map_err(bad_picture)?;
                ("png", out.into_inner())
            }
        };
        let view = self.dir().join(format!("view-{}-{revision}.{name}", page.shot));
        std::fs::write(&view, data)?;
        let _ = std::fs::remove_file(&page.view);
        page.revision = revision;
        page.view = view;
        Ok(())
    }

    fn forget(&self, page: &Page) {
        for path in [self.photo(page.shot), self.work(page.shot), page.view.clone()] {
            let _ = std::fs::remove_file(path);
        }
    }

    fn import(&mut self, photo: &Path, id: u32, colour: Colour) -> Result<Page> {
        let shot = self.next_shot;
        self.next_shot += 1;
        let kept = self.photo(shot);
        if photo.starts_with(self.dir()) {
            std::fs::rename(photo, &kept)?;
        } else {
            std::fs::copy(photo, &kept)?;
        }
        let mut page = Page {
            id,
            shot,
            quad: Quad::FULL,
            turns: 0,
            colour,
            found: false,
            revision: 0,
            view: PathBuf::new(),
        };
        let made = self.prepare(&mut page);
        if made.is_err() {
            self.forget(&page);
        }
        made.map(|()| page)
    }

    fn prepare(&self, page: &mut Page) -> Result<()> {
        let upright = DynamicImage::ImageRgb8(load_upright(&self.photo(page.shot))?);
        let work = if upright.width().max(upright.height()) > WORK_SIDE {
            upright.resize(WORK_SIDE, WORK_SIDE, FilterType::Triangle)
        } else {
            upright
        }
        .into_rgb8();
        std::fs::write(self.work(page.shot), jpeg(&work, WORK_QUALITY)?)?;
        let found = detect::find_page(&work);
        page.found = found.is_some();
        page.quad = found.unwrap_or(Quad::FULL);
        self.refresh(page)
    }

    pub fn add(&mut self, photo: &Path) -> Result<usize> {
        let colour = self.pages.last().map(|p| p.colour).unwrap_or_default();
        let page = self.import(photo, self.next_id, colour)?;
        self.next_id += 1;
        self.pages.push(page);
        Ok(self.pages.len() - 1)
    }

    pub fn retake(&mut self, index: usize, photo: &Path) -> Result<()> {
        let (id, colour) = self.page(index).map(|p| (p.id, p.colour))?;
        let page = self.import(photo, id, colour)?;
        let old = std::mem::replace(&mut self.pages[index], page);
        self.forget(&old);
        Ok(())
    }

    fn change(&mut self, index: usize, edit: impl FnOnce(&mut Page)) -> Result<()> {
        let mut page = self.page(index)?.clone();
        edit(&mut page);
        self.refresh(&mut page)?;
        self.pages[index] = page;
        Ok(())
    }

    pub fn set_colour(&mut self, index: usize, colour: Colour) -> Result<()> {
        self.change(index, |p| p.colour = colour)
    }

    pub fn turn(&mut self, index: usize, quarters: i32) -> Result<()> {
        self.change(index, |p| {
            p.turns = (i32::from(p.turns) + quarters).rem_euclid(4) as u8;
        })
    }

    pub fn set_quad(&mut self, index: usize, quad: Quad) -> Result<()> {
        self.change(index, |p| {
            p.quad = quad;
            p.found = !quad.is_full();
        })
    }

    pub fn remove(&mut self, index: usize) -> Result<()> {
        self.page(index)?;
        let page = self.pages.remove(index);
        self.forget(&page);
        Ok(())
    }

    pub fn move_page(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.pages.len() || to >= self.pages.len() {
            return Err(no_page());
        }
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        Ok(())
    }

    fn page_image(&self, page: &Page) -> Result<PageImage> {
        let photo = load_upright(&self.photo(page.shot))?;
        let (width, height, pixels) = match render(&photo, page, PAGE_SIDE) {
            Rendered::Colour(img) => {
                let data = jpeg(&img, PAGE_QUALITY)?;
                (img.width(), img.height(), Pixels::Jpeg { data, gray: false })
            }
            Rendered::Gray(img) => {
                let data = jpeg(&img, PAGE_QUALITY)?;
                (img.width(), img.height(), Pixels::Jpeg { data, gray: true })
            }
            Rendered::Ink(img) => (img.width(), img.height(), Pixels::Bits(pack_bits(&img))),
        };
        Ok(PageImage {
            width,
            height,
            pixels,
        })
    }

    pub fn write_pdf<W: Write>(
        &self,
        out: W,
        title: &str,
        created: &str,
        progress: &dyn Fn(usize, usize),
    ) -> Result<W> {
        if self.pages.is_empty() {
            return Err(Error::new(ErrorKind::InvalidArgument, "no pages to save"));
        }
        let mut writer = PdfWriter::new(out)?;
        for (done, page) in self.pages.iter().enumerate() {
            writer.add_page(&self.page_image(page)?)?;
            progress(done + 1, self.pages.len());
        }
        writer.finish(title, created)
    }
}

#[cfg(test)]
mod tests;
