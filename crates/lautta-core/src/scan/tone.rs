// SPDX-License-Identifier: LGPL-2.1-or-later
//! Page colours: gray with stretched contrast, and black-and-white that follows uneven light.

use image::{GrayImage, Luma, RgbImage};

/// Share of pixels allowed to clip at each end when stretching contrast.
const CLIP: f64 = 0.01;
/// A pixel this much darker than its surroundings is ink.
const INK: u64 = 15;
/// Surroundings: square with sides of one eighth of the page's longer side.
const WINDOW_DIVISOR: u32 = 8;

fn luma(p: &image::Rgb<u8>) -> u8 {
    let [r, g, b] = p.0.map(u32::from);
    ((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8
}

/// Darkest and brightest level once [`CLIP`] of the pixels is cut off each end.
fn levels(hist: &[u64; 256], total: u64) -> (u8, u8) {
    let cut = (total as f64 * CLIP) as u64;
    let edge = |levels: Box<dyn Iterator<Item = usize>>| {
        let mut seen = 0;
        let mut levels = levels;
        levels
            .find(|&i| {
                seen += hist[i];
                seen > cut
            })
            .unwrap_or(0) as u8
    };
    (edge(Box::new(0..256)), edge(Box::new((0..256).rev())))
}

/// Gray page, darkest parts black and brightest white.
pub fn grayscale(src: &RgbImage) -> GrayImage {
    let mut out = GrayImage::from_fn(src.width(), src.height(), |x, y| {
        Luma([luma(src.get_pixel(x, y))])
    });
    let mut hist = [0u64; 256];
    for p in out.pixels() {
        hist[usize::from(p[0])] += 1;
    }
    let (low, high) = levels(&hist, u64::from(src.width()) * u64::from(src.height()));
    if high > low {
        let span = u32::from(high - low);
        for p in out.pixels_mut() {
            let v = u32::from(p[0].clamp(low, high) - low);
            p[0] = ((v * 255 + span / 2) / span) as u8;
        }
    }
    out
}

/// Sums of all pixels above and left of each position; one row and column of zeros in front.
fn integral(gray: &GrayImage) -> Vec<u64> {
    let (w, h) = (gray.width() as usize, gray.height() as usize);
    let mut sums = vec![0u64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0u64;
        for x in 0..w {
            row += u64::from(gray.as_raw()[y * w + x]);
            sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
        }
    }
    sums
}

/// Ink where a pixel is [`INK`] percent darker than the mean around it (Bradley-Roth); 0 = ink,
/// 255 = paper.
pub fn black_white(gray: &GrayImage) -> GrayImage {
    let (w, h) = (gray.width() as usize, gray.height() as usize);
    let sums = integral(gray);
    let half = (gray.width().max(gray.height()) / WINDOW_DIVISOR / 2).max(1) as usize;
    GrayImage::from_fn(gray.width(), gray.height(), |x, y| {
        let (x, y) = (x as usize, y as usize);
        let (x0, y0) = (x.saturating_sub(half), y.saturating_sub(half));
        let (x1, y1) = ((x + half + 1).min(w), (y + half + 1).min(h));
        let at = |px: usize, py: usize| sums[py * (w + 1) + px];
        let sum = at(x1, y1) + at(x0, y0) - at(x1, y0) - at(x0, y1);
        let count = ((x1 - x0) * (y1 - y0)) as u64;
        let value = u64::from(gray.as_raw()[y * w + x]);
        Luma([if value * count * 100 < sum * (100 - INK) {
            0
        } else {
            255
        }])
    })
}

/// Rows packed to one bit per pixel, most significant bit first, 1 = paper; rows start on a byte.
pub fn pack_bits(bw: &GrayImage) -> Vec<u8> {
    let (w, h) = (bw.width() as usize, bw.height() as usize);
    let stride = w.div_ceil(8);
    let mut out = vec![0u8; stride * h];
    for (i, v) in bw.as_raw().iter().enumerate() {
        if *v >= 128 {
            let (x, y) = (i % w, i / w);
            out[y * stride + x / 8] |= 0x80 >> (x % 8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn luma_weighs_green_most() {
        assert_eq!(luma(&Rgb([255, 255, 255])), 255);
        assert_eq!(luma(&Rgb([0, 0, 0])), 0);
        assert_eq!(luma(&Rgb([255, 0, 0])), 76);
        assert_eq!(luma(&Rgb([0, 255, 0])), 150);
        assert_eq!(luma(&Rgb([0, 0, 255])), 29);
    }

    #[test]
    fn gray_page_is_stretched_to_full_contrast() {
        let src = RgbImage::from_fn(10, 10, |x, _| {
            let v = 100 + (x as u8) * 10;
            Rgb([v, v, v])
        });
        let out = grayscale(&src);
        assert_eq!(out.get_pixel(0, 0)[0], 0);
        assert_eq!(out.get_pixel(9, 0)[0], 255);
        assert_eq!(out.get_pixel(3, 0)[0], 85);
        assert_eq!(out.get_pixel(5, 5)[0], 142);
    }

    #[test]
    fn a_few_stray_pixels_do_not_set_the_range() {
        // 1000 pixels: 5 black specks, 5 white ones, the rest between 100 and 200.
        let src = RgbImage::from_fn(100, 10, |x, y| {
            let v = match (x, y) {
                (0..=4, 0) => 0,
                (5..=9, 0) => 255,
                (_, _) if x < 50 => 100,
                _ => 200,
            };
            Rgb([v, v, v])
        });
        let out = grayscale(&src);
        assert_eq!(out.get_pixel(20, 5)[0], 0);
        assert_eq!(out.get_pixel(80, 5)[0], 255);
        assert_eq!(out.get_pixel(0, 0)[0], 0);
        assert_eq!(out.get_pixel(5, 0)[0], 255);
    }

    #[test]
    fn levels_cut_one_percent_each_end() {
        let mut hist = [0u64; 256];
        hist[10] = 10;
        hist[50] = 1;
        hist[120] = 978;
        hist[200] = 1;
        hist[250] = 10;
        // Ten of a thousand may clip: level 10 and 250 go, 50 and 200 stay.
        assert_eq!(levels(&hist, 1000), (50, 200));
        hist[10] = 9;
        hist[250] = 11;
        assert_eq!(levels(&hist, 1000), (120, 250));
    }

    #[test]
    fn flat_page_stays_as_it_is() {
        let src = RgbImage::from_pixel(4, 4, Rgb([90, 90, 90]));
        assert!(grayscale(&src).pixels().all(|p| p[0] == 90));
        assert_eq!(grayscale(&RgbImage::new(0, 0)).len(), 0);
    }

    #[test]
    fn integral_sums_rectangles() {
        let gray = GrayImage::from_fn(3, 2, |x, y| Luma([(1 + x + 3 * y) as u8]));
        let sums = integral(&gray);
        assert_eq!(sums.len(), 12);
        assert_eq!(&sums[..4], &[0, 0, 0, 0]);
        assert_eq!(&sums[4..8], &[0, 1, 3, 6]);
        assert_eq!(&sums[8..], &[0, 5, 12, 21]);
    }

    #[test]
    fn ink_stays_ink_in_light_and_shadow() {
        // Left half lit (paper 220, ink 120), right half in shadow (paper 110, ink 40): one
        // level for the whole page would lose one side.
        let gray = GrayImage::from_fn(160, 80, |x, y| {
            let ink = y % 16 < 2 && x % 80 > 10 && x % 80 < 70;
            Luma([match (x < 80, ink) {
                (true, false) => 220,
                (true, true) => 120,
                (false, false) => 110,
                (false, true) => 40,
            }])
        });
        let out = black_white(&gray);
        for (x, y) in [(20, 16), (40, 33), (100, 16), (140, 49)] {
            assert_eq!(out.get_pixel(x, y)[0], 0, "ink at {x},{y}");
        }
        for (x, y) in [(20, 8), (40, 40), (100, 8), (140, 60), (2, 2), (158, 78)] {
            assert_eq!(out.get_pixel(x, y)[0], 255, "paper at {x},{y}");
        }
    }

    #[test]
    fn ink_must_be_fifteen_percent_darker() {
        let page = |ink: u8| {
            let mut gray = GrayImage::from_pixel(64, 64, Luma([200]));
            gray.put_pixel(32, 32, Luma([ink]));
            black_white(&gray).get_pixel(32, 32)[0]
        };
        assert_eq!(page(169), 0);
        assert_eq!(page(171), 255);
    }

    #[test]
    fn window_is_an_eighth_of_the_longer_side() {
        // Dark block of 40 × 40 on a page of 320: window of 41 around its centre sees only
        // the block, so the centre counts as paper of a dark area, not ink.
        let gray = GrayImage::from_fn(320, 80, |x, y| {
            Luma([if (140..180).contains(&x) && (20..60).contains(&y) {
                60
            } else {
                220
            }])
        });
        let out = black_white(&gray);
        assert_eq!(out.get_pixel(160, 40)[0], 255);
        assert_eq!(out.get_pixel(141, 40)[0], 0);
        assert_eq!(out.get_pixel(160, 21)[0], 0);
        assert_eq!(out.get_pixel(100, 40)[0], 255);
    }

    #[test]
    fn bits_are_packed_high_bit_first_with_padded_rows() {
        let bw = GrayImage::from_fn(10, 2, |x, y| Luma([if (x + y) % 2 == 0 { 255 } else { 0 }]));
        assert_eq!(
            pack_bits(&bw),
            vec![0b1010_1010, 0b1000_0000, 0b0101_0101, 0b0100_0000]
        );
        let exact = GrayImage::from_pixel(8, 1, Luma([255]));
        assert_eq!(pack_bits(&exact), vec![0xff]);
        assert_eq!(pack_bits(&GrayImage::from_pixel(3, 1, Luma([127]))), vec![0]);
        assert_eq!(
            pack_bits(&GrayImage::from_pixel(3, 1, Luma([128]))),
            vec![0b1110_0000]
        );
    }
}
