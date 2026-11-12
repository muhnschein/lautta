// SPDX-License-Identifier: LGPL-2.1-or-later
//! Finds the page in a photo: the largest area whose colour differs from the photo's border, if
//! it is four-sided. No answer (page fills photo, low contrast, clutter) = caller keeps the whole
//! picture.

use super::{Point, Quad};
use image::imageops::{self, FilterType};
use image::{GrayImage, Luma, RgbImage};

/// Longer side of the picture the search runs on.
const WORK_SIDE: u32 = 256;
/// Blur wide enough to swallow lines of text.
const BLUR_SIGMA: f32 = 2.0;
/// Page and table differ at least this much in one colour channel (of 255).
const MIN_CONTRAST: f64 = 40.0;
/// Share of the photo a page covers at least.
const MIN_COVER: f32 = 0.10;
/// Four corners must cover this much of the area's outline, else it is no sheet.
const MIN_FIT: f32 = 0.85;

type P = (i32, i32);

/// Otsu: the level splitting the histogram into two groups with the largest spread between
/// them, and how far the groups' means lie apart.
fn threshold(img: &GrayImage) -> (u8, f64) {
    let mut hist = [0u64; 256];
    for p in img.pixels() {
        hist[usize::from(p[0])] += 1;
    }
    let total = u64::from(img.width()) * u64::from(img.height());
    let sum: u64 = hist.iter().enumerate().map(|(i, n)| i as u64 * n).sum();
    let (mut below, mut sum_below, mut best) = (0u64, 0u64, (0.0f64, 0u8, 0.0f64));
    for (i, n) in hist.iter().enumerate() {
        below += n;
        sum_below += i as u64 * n;
        let above = total - below;
        if below == 0 || above == 0 {
            continue;
        }
        let gap = (sum - sum_below) as f64 / above as f64 - sum_below as f64 / below as f64;
        let spread = below as f64 * above as f64 * gap * gap;
        if spread > best.0 {
            best = (spread, i as u8, gap);
        }
    }
    (best.1, best.2)
}

/// Middle colour of the pixels on the photo's border: the table.
fn border_colour(img: &RgbImage) -> [u8; 3] {
    let (w, h) = (img.width(), img.height());
    let ring = (0..w)
        .flat_map(|x| [(x, 0), (x, h - 1)])
        .chain((0..h).flat_map(|y| [(0, y), (w - 1, y)]));
    let mut channels: [Vec<u8>; 3] = Default::default();
    for (x, y) in ring {
        let p = img.get_pixel(x, y);
        for (c, values) in channels.iter_mut().enumerate() {
            values.push(p[c]);
        }
    }
    channels.map(|mut values| {
        values.sort_unstable();
        values[values.len() / 2]
    })
}

/// How far each pixel is from `colour`: largest difference of the three channels.
fn distance(img: &RgbImage, colour: [u8; 3]) -> GrayImage {
    GrayImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        Luma([(0..3).map(|c| p[c].abs_diff(colour[c])).max().unwrap_or(0)])
    })
}

/// Pixels of the largest connected area of `mask` (4 neighbours).
fn largest_area(mask: &[bool], w: usize, h: usize) -> Vec<P> {
    let mut seen = vec![false; mask.len()];
    let mut best: Vec<P> = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut area = Vec::new();
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            area.push((x as i32, y as i32));
            let mut visit = |ok: bool, j: usize| {
                if ok && mask[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            visit(x > 0, i.wrapping_sub(1));
            visit(x + 1 < w, i + 1);
            visit(y > 0, i.wrapping_sub(w));
            visit(y + 1 < h, i + w);
        }
        if area.len() > best.len() {
            best = area;
        }
    }
    best
}

fn cross(o: P, a: P, b: P) -> i64 {
    i64::from(a.0 - o.0) * i64::from(b.1 - o.1) - i64::from(a.1 - o.1) * i64::from(b.0 - o.0)
}

/// Convex outline (monotone chain), clockwise on screen, no three points on one line.
fn hull(mut points: Vec<P>) -> Vec<P> {
    points.sort_unstable();
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut out: Vec<P> = Vec::with_capacity(points.len() + 1);
    for pass in 0..2 {
        let floor = out.len() + 1;
        let ordered: Box<dyn Iterator<Item = &P>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &p in ordered {
            while out.len() > floor && cross(out[out.len() - 2], out[out.len() - 1], p) <= 0 {
                out.pop();
            }
            out.push(p);
        }
        out.pop();
    }
    out
}

/// Twice the area inside a closed outline.
fn area2(outline: &[P]) -> i64 {
    (0..outline.len())
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
            i64::from(a.0) * i64::from(b.1) - i64::from(b.0) * i64::from(a.1)
        })
        .sum::<i64>()
        .abs()
}

/// The four outline points spanning the largest area, in outline order. For every diagonal the
/// point farthest from it on each side.
fn largest_quad(outline: &[P]) -> Option<[P; 4]> {
    let n = outline.len();
    let mut best: Option<(i64, [P; 4])> = None;
    for i in 0..n {
        for j in i + 2..n {
            let far = |range: std::ops::Range<usize>| {
                range
                    .map(|k| (cross(outline[i], outline[j], outline[k % n]).abs(), k % n))
                    .max()
            };
            let (Some((a, k)), Some((b, l))) = (far(i + 1..j), far(j + 1..n + i)) else {
                continue;
            };
            if best.is_none_or(|(area, _)| a + b > area) {
                best = Some((a + b, [outline[i], outline[k], outline[j], outline[l]]));
            }
        }
    }
    best.map(|(_, q)| q)
}

/// Clockwise corners starting with the one nearest the photo's top left.
fn from_top_left(corners: [P; 4]) -> [P; 4] {
    let first = (0..4)
        .min_by_key(|&i| (corners[i].0 + corners[i].1, corners[i].1))
        .unwrap_or(0);
    [0, 1, 2, 3].map(|i| corners[(first + i) % 4])
}

/// Centre of a pixel, relative to a picture of `w` × `h`: the outline runs through the outermost
/// pixels of the page.
fn centre(p: P, w: usize, h: usize) -> Point {
    Point::new((p.0 as f32 + 0.5) / w as f32, (p.1 as f32 + 0.5) / h as f32)
}

/// `photo`: upright, any size.
pub fn find_page(photo: &RgbImage) -> Option<Quad> {
    if photo.width() < 8 || photo.height() < 8 {
        return None;
    }
    // Squeezed to a square: corners are relative, so the shape does not matter.
    let small = if photo.width().max(photo.height()) > WORK_SIDE {
        imageops::resize(photo, WORK_SIDE, WORK_SIDE, FilterType::Triangle)
    } else {
        photo.clone()
    };
    let small = imageops::blur(&small, BLUR_SIGMA);
    let (w, h) = (small.width() as usize, small.height() as usize);
    let apart = distance(&small, border_colour(&small));
    let (level, contrast) = threshold(&apart);
    if contrast < MIN_CONTRAST {
        return None;
    }
    let mask: Vec<bool> = apart.pixels().map(|p| p[0] > level).collect();
    let area = largest_area(&mask, w, h);
    let total = (w * h) as f32;
    if (area.len() as f32) < MIN_COVER * total {
        return None;
    }
    let outline = hull(area);
    let corners = from_top_left(largest_quad(&outline)?);
    let (inside, around) = (area2(&corners) as f32, area2(&outline) as f32);
    if inside < MIN_FIT * around {
        return None;
    }
    let quad = Quad(corners.map(|p| centre(p, w, h)));
    quad.is_page().then_some(quad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    const PAPER: [u8; 3] = [230, 230, 230];
    const TABLE: [u8; 3] = [40, 40, 40];

    fn inside(p: (f32, f32), c: &[(f32, f32); 4]) -> bool {
        (0..4).all(|i| {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= 0.0
        })
    }

    /// Sheet with the given corners (relative) on a table.
    fn photo(w: u32, h: u32, corners: [(f32, f32); 4], sheet: [u8; 3], table: [u8; 3]) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let p = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            Rgb(if inside(p, &corners) { sheet } else { table })
        })
    }

    fn fill(img: &mut RgbImage, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>, colour: [u8; 3]) {
        for y in ys {
            for x in xs.clone() {
                img.put_pixel(x, y, Rgb(colour));
            }
        }
    }

    fn assert_close(found: Option<Quad>, corners: [(f32, f32); 4], tolerance: f32) {
        let quad = found.expect("page found");
        for (got, want) in quad.0.iter().zip(corners) {
            assert!(
                (got.x - want.0).abs() <= tolerance && (got.y - want.1).abs() <= tolerance,
                "corner {got:?}, wanted {want:?}"
            );
        }
    }

    const SHEET: [(f32, f32); 4] = [(0.2, 0.15), (0.8, 0.2), (0.85, 0.85), (0.15, 0.8)];

    #[test]
    fn bright_sheet_on_dark_table_is_found() {
        assert_close(find_page(&photo(640, 480, SHEET, PAPER, TABLE)), SHEET, 0.02);
    }

    #[test]
    fn dark_sheet_on_bright_table_is_found() {
        assert_close(find_page(&photo(640, 480, SHEET, TABLE, PAPER)), SHEET, 0.02);
    }

    #[test]
    fn sheet_of_the_table_brightness_but_another_colour_is_found() {
        let (green, brown) = ([90, 160, 90], [170, 120, 70]);
        assert_close(find_page(&photo(640, 480, SHEET, green, brown)), SHEET, 0.02);
    }

    #[test]
    fn portrait_photos_work_like_landscape_ones() {
        assert_close(find_page(&photo(300, 800, SHEET, PAPER, TABLE)), SHEET, 0.02);
    }

    /// Every corner is the centre of a pixel of a `w` × `h` picture.
    fn assert_on_grid(found: Option<Quad>, w: f32, h: f32) {
        for c in found.expect("page found").0 {
            let (x, y) = (c.x * w - 0.5, c.y * h - 0.5);
            assert!(
                (x - x.round()).abs() < 1e-3 && (y - y.round()).abs() < 1e-3,
                "corner {c:?} off the {w} x {h} grid"
            );
        }
    }

    #[test]
    fn small_photos_are_searched_as_they_are() {
        for (w, h) in [(120, 90), (256, 192)] {
            let found = find_page(&photo(w, h, SHEET, PAPER, TABLE));
            assert_close(found, SHEET, 0.04);
            assert_on_grid(found, w as f32, h as f32);
        }
    }

    #[test]
    fn large_photos_are_searched_shrunk() {
        let found = find_page(&photo(640, 480, SHEET, PAPER, TABLE));
        assert_on_grid(found, WORK_SIDE as f32, WORK_SIDE as f32);
    }

    #[test]
    fn corners_are_pixel_centres() {
        assert_eq!(centre((0, 0), 4, 2), Point::new(0.125, 0.25));
        assert_eq!(centre((3, 1), 4, 2), Point::new(0.875, 0.75));
    }

    #[test]
    fn text_on_the_sheet_does_not_move_the_corners() {
        let mut img = photo(640, 480, SHEET, PAPER, TABLE);
        for line in 0..12 {
            fill(&mut img, 180..460, 130 + line * 20..133 + line * 20, [30, 30, 30]);
        }
        assert_close(find_page(&img), SHEET, 0.02);
    }

    #[test]
    fn dark_and_coloured_parts_of_the_sheet_stay_inside() {
        let square = [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)];
        let mut img = photo(640, 480, square, PAPER, TABLE);
        // Red picture in the sheet's top left quarter, dark bar further down.
        fill(&mut img, 128..320, 96..240, [200, 30, 30]);
        fill(&mut img, 160..480, 300..334, [90, 90, 90]);
        assert_close(find_page(&img), square, 0.02);
    }

    #[test]
    fn strongly_turned_sheet_is_found_with_top_left_first() {
        let diamond = [(0.3, 0.3), (0.6, 0.1), (0.75, 0.6), (0.4, 0.85)];
        assert_close(find_page(&photo(640, 480, diamond, PAPER, TABLE)), diamond, 0.02);
    }

    #[test]
    fn sheet_cut_by_the_photo_edge_is_found() {
        let cut = [(0.0, 0.2), (0.7, 0.2), (0.7, 0.8), (0.0, 0.8)];
        assert_close(find_page(&photo(640, 480, cut, PAPER, TABLE)), cut, 0.02);
    }

    #[test]
    fn plain_photo_has_no_page() {
        assert_eq!(
            find_page(&RgbImage::from_pixel(640, 480, Rgb([128, 128, 128]))),
            None
        );
    }

    #[test]
    fn sheet_filling_the_photo_has_no_page() {
        let mut img = RgbImage::from_pixel(640, 480, Rgb(PAPER));
        for line in 0..12 {
            fill(&mut img, 100..540, 100 + line * 20..103 + line * 20, [30, 30, 30]);
        }
        assert_eq!(find_page(&img), None);
    }

    #[test]
    fn faint_sheet_is_not_a_page() {
        assert_eq!(
            find_page(&photo(640, 480, SHEET, [150, 150, 150], [120, 120, 120])),
            None
        );
        let clear = find_page(&photo(640, 480, SHEET, [170, 170, 170], [120, 120, 120]));
        assert_close(clear, SHEET, 0.02);
    }

    #[test]
    fn small_bright_spot_is_not_a_page() {
        let spot = [(0.4, 0.4), (0.6, 0.4), (0.6, 0.6), (0.4, 0.6)];
        assert_eq!(find_page(&photo(640, 480, spot, PAPER, TABLE)), None);
        // Just above a tenth of the photo.
        let page = [(0.3, 0.3), (0.66, 0.3), (0.66, 0.66), (0.3, 0.66)];
        assert_close(find_page(&photo(640, 480, page, PAPER, TABLE)), page, 0.02);
    }

    #[test]
    fn round_plate_is_not_a_page() {
        let img = RgbImage::from_fn(400, 400, |x, y| {
            let (dx, dy) = (x as f32 - 200.0, y as f32 - 200.0);
            Rgb(if dx.hypot(dy) < 150.0 { PAPER } else { TABLE })
        });
        assert_eq!(find_page(&img), None);
    }

    #[test]
    fn tiny_pictures_are_not_searched() {
        assert_eq!(find_page(&photo(7, 400, SHEET, PAPER, TABLE)), None);
        assert_eq!(find_page(&photo(400, 7, SHEET, PAPER, TABLE)), None);
    }

    #[test]
    fn threshold_lies_between_two_groups() {
        let img = GrayImage::from_fn(10, 10, |x, _| Luma([if x < 3 { 20 } else { 200 }]));
        let (level, contrast) = threshold(&img);
        assert!((20..200).contains(&level), "level {level}");
        assert!((contrast - 180.0).abs() < 1e-9, "contrast {contrast}");
        assert_eq!(threshold(&GrayImage::from_pixel(4, 4, Luma([7]))), (0, 0.0));
    }

    #[test]
    fn threshold_weighs_group_sizes_and_the_squared_gap() {
        // 40 dark, 40 middle, 20 bright: splitting off the small bright group wins only by
        // its far mean, squared.
        let img = GrayImage::from_fn(10, 10, |x, _| {
            Luma([match x {
                0..=3 => 0,
                4..=7 => 100,
                _ => 255,
            }])
        });
        assert_eq!(threshold(&img), (100, 205.0));
    }

    #[test]
    fn border_colour_is_the_middle_of_the_border_only() {
        // Border mostly table; a bright strip covers a third of the top edge, the inside is paper.
        let mut img = RgbImage::from_pixel(30, 20, Rgb(PAPER));
        for x in 0..30 {
            img.put_pixel(x, 0, Rgb(if x < 10 { [250, 240, 10] } else { [40, 50, 60] }));
            img.put_pixel(x, 19, Rgb([40, 50, 60]));
        }
        for y in 0..20 {
            img.put_pixel(0, y, Rgb([40, 50, 60]));
            img.put_pixel(29, y, Rgb([40, 50, 60]));
        }
        img.put_pixel(0, 0, Rgb([250, 240, 10]));
        assert_eq!(border_colour(&img), [40, 50, 60]);
    }

    #[test]
    fn distance_is_the_largest_channel_difference() {
        let img = RgbImage::from_fn(2, 1, |x, _| Rgb(if x == 0 { [10, 20, 30] } else { [200, 25, 0] }));
        let apart = distance(&img, [10, 20, 30]);
        assert_eq!((apart.get_pixel(0, 0)[0], apart.get_pixel(1, 0)[0]), (0, 190));
        let apart = distance(&img, [10, 120, 30]);
        assert_eq!((apart.get_pixel(0, 0)[0], apart.get_pixel(1, 0)[0]), (100, 190));
        let apart = distance(&img, [10, 20, 250]);
        assert_eq!((apart.get_pixel(0, 0)[0], apart.get_pixel(1, 0)[0]), (220, 250));
    }

    #[test]
    fn largest_area_wins_and_diagonals_do_not_connect() {
        // Two pixels touching by a corner, then a row of three.
        let mask = [
            true, false, false, false, true, false, false, false, false, true, true, true,
        ];
        let area = largest_area(&mask, 3, 4);
        let mut sorted = area.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![(0, 3), (1, 3), (2, 3)]);
        assert!(largest_area(&[false; 6], 3, 2).is_empty());
    }

    #[test]
    fn areas_join_in_all_four_directions() {
        // Plus sign: centre found first only through its neighbours.
        let mask = [false, true, false, true, true, true, false, true, false];
        assert_eq!(largest_area(&mask, 3, 3).len(), 5);
    }

    #[test]
    fn areas_join_upwards_too() {
        // U: right arm is reached from below only.
        let mask = [true, false, true, true, true, true];
        assert_eq!(largest_area(&mask, 3, 2).len(), 5);
    }

    #[test]
    fn areas_do_not_wrap_around_the_picture_edge() {
        // Right end of row 0 and left end of row 1 are neighbours in memory only.
        let mask = [false, false, true, true, false, false];
        assert_eq!(largest_area(&mask, 3, 2).len(), 1);
    }

    #[test]
    fn hull_drops_inner_points_and_points_on_edges() {
        let points = vec![(0, 0), (4, 0), (4, 4), (0, 4), (2, 2), (2, 0), (1, 3), (4, 2)];
        let outline = hull(points);
        assert_eq!(outline, vec![(0, 0), (4, 0), (4, 4), (0, 4)]);
        assert_eq!(area2(&outline), 32);
        assert_eq!(hull(vec![(1, 1), (1, 1), (2, 2)]), vec![(1, 1), (2, 2)]);
        assert_eq!(hull(vec![(3, 3)]), vec![(3, 3)]);
        assert_eq!(hull(vec![(0, 4), (4, 0), (0, 0)]), vec![(0, 0), (4, 0), (0, 4)]);
    }

    #[test]
    fn largest_quad_skips_cut_corners() {
        // Square with one corner cut by a short edge.
        let outline = [(0, 0), (9, 0), (10, 1), (10, 10), (0, 10)];
        let quad = largest_quad(&outline).unwrap();
        assert_eq!(area2(&quad), 190);
        assert!(quad.contains(&(0, 0)) && quad.contains(&(10, 10)) && quad.contains(&(0, 10)));
        assert_eq!(largest_quad(&[(0, 0), (5, 0), (0, 5)]), None);
    }

    #[test]
    fn largest_quad_is_found_wherever_the_outline_starts() {
        // Square whose left side bulges: outline starts in the bulge.
        let outline = [(1, 15), (0, 10), (1, 5), (2, 0), (22, 0), (22, 20), (2, 20)];
        assert_eq!(largest_quad(&outline), Some([(2, 0), (22, 0), (22, 20), (2, 20)]));
    }

    #[test]
    fn largest_quad_adds_both_sides_of_the_diagonal() {
        // Far points of diagonal (8,2)-(4,7) multiply to more than those of the best one.
        let outline = [(2, 4), (8, 2), (10, 4), (10, 11), (4, 7)];
        assert_eq!(largest_quad(&outline), Some([(2, 4), (8, 2), (10, 4), (10, 11)]));
    }

    #[test]
    fn corner_order_starts_top_left() {
        let turned = [(10, 0), (10, 10), (0, 10), (0, 0)];
        assert_eq!(from_top_left(turned), [(0, 0), (10, 0), (10, 10), (0, 10)]);
        // Tie on x + y: the upper corner leads.
        let diamond = [(0, 5), (5, 0), (10, 5), (5, 10)];
        assert_eq!(from_top_left(diamond), [(5, 0), (10, 5), (5, 10), (0, 5)]);
    }
}
