// SPDX-License-Identifier: LGPL-2.1-or-later
//! Straightens the marked page: perspective map from its four corners onto a rectangle.

use super::{Point, Quad};
use image::{Rgb, RgbImage};

/// Pixel size of the straightened page: mean of opposite sides, longer one at most `max_long`,
/// never larger than the photo shows it.
pub fn output_size(quad: &Quad, src_w: u32, src_h: u32, max_long: u32) -> (u32, u32) {
    let p = quad.0.map(|c| (c.x * src_w as f32, c.y * src_h as f32));
    let side = |a: usize, b: usize| (p[a].0 - p[b].0).hypot(p[a].1 - p[b].1);
    let w = (side(0, 1) + side(3, 2)) / 2.0;
    let h = (side(1, 2) + side(0, 3)) / 2.0;
    let scale = (max_long as f32 / w.max(h)).min(1.0);
    let px = |v: f32| ((v * scale).round() as u32).max(1);
    (px(w), px(h))
}

/// Solves `m · x = b` in place (Gauss-Jordan, partial pivoting). `None` when singular.
fn solve(mut m: [[f64; 9]; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        let pivot = (col..8).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        let lead = m[col];
        for (row, line) in m.iter_mut().enumerate() {
            if row != col {
                let f = line[col] / lead[col];
                for (cell, above) in line.iter_mut().zip(lead).skip(col) {
                    *cell -= f * above;
                }
            }
        }
    }
    let mut x = [0.0; 8];
    for (i, v) in x.iter_mut().enumerate() {
        *v = m[i][8] / m[i][i];
    }
    Some(x)
}

/// Maps output pixel coordinates to photo pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography([f64; 8]);

impl Homography {
    /// Output rectangle `w` × `h` onto `corners` (photo pixels, order of [`Quad`]).
    pub fn new(w: f64, h: f64, corners: [(f64, f64); 4]) -> Option<Homography> {
        let from = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
        let mut m = [[0.0; 9]; 8];
        for i in 0..4 {
            let ((x, y), (u, v)) = (from[i], corners[i]);
            m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        solve(m).map(Homography)
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let h = &self.0;
        let d = h[6] * x + h[7] * y + 1.0;
        ((h[0] * x + h[1] * y + h[2]) / d, (h[3] * x + h[4] * y + h[5]) / d)
    }
}

/// Photo colour at a fractional pixel position (pixel centres at `.5`), edges repeated.
fn sample(src: &RgbImage, x: f64, y: f64) -> Rgb<u8> {
    let (w, h) = (src.width(), src.height());
    let fx = (x - 0.5).clamp(0.0, f64::from(w - 1));
    let fy = (y - 0.5).clamp(0.0, f64::from(h - 1));
    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - f64::from(x0), fy - f64::from(y0));
    let (a, b) = (src.get_pixel(x0, y0), src.get_pixel(x1, y0));
    let (c, d) = (src.get_pixel(x0, y1), src.get_pixel(x1, y1));
    let mut out = [0u8; 3];
    for (i, o) in out.iter_mut().enumerate() {
        let top = f64::from(a[i]) * (1.0 - tx) + f64::from(b[i]) * tx;
        let bottom = f64::from(c[i]) * (1.0 - tx) + f64::from(d[i]) * tx;
        *o = (top * (1.0 - ty) + bottom * ty).round() as u8;
    }
    Rgb(out)
}

/// The page inside `quad`, drawn `out_w` × `out_h`. Corners on one line, as in a photo with an
/// empty side: white.
pub fn warp(src: &RgbImage, quad: &Quad, out_w: u32, out_h: u32) -> RgbImage {
    let mut out = RgbImage::from_pixel(out_w, out_h, Rgb([255, 255, 255]));
    let (sw, sh) = (f64::from(src.width()), f64::from(src.height()));
    let corners = quad.0.map(|c: Point| (f64::from(c.x) * sw, f64::from(c.y) * sh));
    let Some(map) = Homography::new(f64::from(out_w), f64::from(out_h), corners) else {
        return out;
    };
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let (sx, sy) = map.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
        *pixel = sample(src, sx, sy);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(v: [f32; 8]) -> Quad {
        Quad::from_flat(&v).unwrap()
    }

    /// Four colour fields: red, green over blue, yellow.
    fn fields(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| match (x < w / 2, y < h / 2) {
            (true, true) => Rgb([255, 0, 0]),
            (false, true) => Rgb([0, 255, 0]),
            (true, false) => Rgb([0, 0, 255]),
            (false, false) => Rgb([255, 255, 0]),
        })
    }

    #[test]
    fn size_follows_the_marked_area() {
        assert_eq!(output_size(&Quad::FULL, 400, 300, 10_000), (400, 300));
        let half = quad([0.0, 0.0, 0.5, 0.0, 0.5, 1.0, 0.0, 1.0]);
        assert_eq!(output_size(&half, 400, 300, 10_000), (200, 300));
    }

    #[test]
    fn size_is_the_mean_of_opposite_sides() {
        // Top 200 px, bottom 400 px wide; both flanks 100·√2 px long.
        let trapezoid = quad([0.25, 0.0, 0.75, 0.0, 1.0, 1.0, 0.0, 1.0]);
        assert_eq!(output_size(&trapezoid, 400, 100, 10_000), (300, 141));
    }

    #[test]
    fn size_is_capped_on_the_longer_side_and_never_grows() {
        assert_eq!(output_size(&Quad::FULL, 4000, 3000, 2000), (2000, 1500));
        assert_eq!(output_size(&Quad::FULL, 3000, 4000, 2000), (1500, 2000));
        assert_eq!(output_size(&Quad::FULL, 40, 30, 2000), (40, 30));
        let sliver = quad([0.0, 0.0, 1.0, 0.0, 1.0, 0.05, 0.0, 0.05]);
        assert_eq!(output_size(&sliver, 4000, 100, 20), (20, 1));
    }

    #[test]
    fn homography_hits_all_four_corners() {
        let corners = [(10.0, 20.0), (90.0, 5.0), (120.0, 80.0), (0.0, 70.0)];
        let h = Homography::new(50.0, 30.0, corners).unwrap();
        let from = [(0.0, 0.0), (50.0, 0.0), (50.0, 30.0), (0.0, 30.0)];
        for (f, c) in from.iter().zip(corners) {
            let (x, y) = h.apply(f.0, f.1);
            assert!(
                (x - c.0).abs() < 1e-6 && (y - c.1).abs() < 1e-6,
                "{f:?} -> {x},{y}"
            );
        }
        // Perspective, not affine: the centre does not land on the mean of the corners.
        let (cx, cy) = h.apply(25.0, 15.0);
        assert!((cx - 55.0).abs() > 1.0 || (cy - 43.75).abs() > 1.0);
    }

    #[test]
    fn corners_on_one_line_have_no_map() {
        let line = [(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
        assert_eq!(Homography::new(10.0, 10.0, line), None);
    }

    #[test]
    fn whole_picture_at_same_size_is_unchanged() {
        let src = fields(8, 6);
        assert_eq!(warp(&src, &Quad::FULL, 8, 6), src);
    }

    #[test]
    fn marked_quarter_is_cut_out() {
        let src = fields(40, 40);
        let top_right = quad([0.5, 0.0, 1.0, 0.0, 1.0, 0.5, 0.5, 0.5]);
        let out = warp(&src, &top_right, 10, 10);
        assert!(out.pixels().all(|p| *p == Rgb([0, 255, 0])));
        let bottom_left = quad([0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0, 1.0]);
        let out = warp(&src, &bottom_left, 10, 10);
        assert!(out.pixels().all(|p| *p == Rgb([0, 0, 255])));
    }

    #[test]
    fn corner_order_turns_the_page() {
        let src = fields(40, 40);
        // Page lies on its side: its top left is the photo's top right.
        let turned = Quad([
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ]);
        let out = warp(&src, &turned, 40, 40);
        assert_eq!(*out.get_pixel(5, 5), Rgb([0, 255, 0]));
        assert_eq!(*out.get_pixel(35, 5), Rgb([255, 255, 0]));
        assert_eq!(*out.get_pixel(35, 35), Rgb([0, 0, 255]));
        assert_eq!(*out.get_pixel(5, 35), Rgb([255, 0, 0]));
    }

    #[test]
    fn pixels_between_two_colours_are_blended() {
        let src = RgbImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgb([0, 0, 0])
            } else {
                Rgb([200, 100, 50])
            }
        });
        let out = warp(&src, &Quad::FULL, 4, 1);
        assert_eq!(*out.get_pixel(0, 0), Rgb([0, 0, 0]));
        assert_eq!(*out.get_pixel(1, 0), Rgb([50, 25, 13]));
        assert_eq!(*out.get_pixel(2, 0), Rgb([150, 75, 38]));
        assert_eq!(*out.get_pixel(3, 0), Rgb([200, 100, 50]));
    }

    #[test]
    fn rows_are_blended_too() {
        let src = RgbImage::from_fn(1, 2, |_, y| {
            if y == 0 {
                Rgb([0, 0, 0])
            } else {
                Rgb([200, 100, 50])
            }
        });
        let out = warp(&src, &Quad::FULL, 1, 4);
        assert_eq!(*out.get_pixel(0, 1), Rgb([50, 25, 13]));
        assert_eq!(*out.get_pixel(0, 2), Rgb([150, 75, 38]));
    }

    #[test]
    fn positions_beyond_the_photo_repeat_its_edge() {
        let src = fields(2, 2);
        assert_eq!(sample(&src, 5.0, 0.5), Rgb([0, 255, 0]));
        assert_eq!(sample(&src, 0.5, 9.0), Rgb([0, 0, 255]));
        assert_eq!(sample(&src, -3.0, -3.0), Rgb([255, 0, 0]));
        assert_eq!(sample(&src, 7.0, 7.0), Rgb([255, 255, 0]));
    }

    #[test]
    fn empty_photo_or_flat_corners_give_white() {
        let white = RgbImage::from_pixel(3, 2, Rgb([255, 255, 255]));
        assert_eq!(warp(&RgbImage::new(0, 0), &Quad::FULL, 3, 2), white);
        assert_eq!(warp(&RgbImage::new(0, 5), &Quad::FULL, 3, 2), white);
        assert_eq!(warp(&RgbImage::new(5, 0), &Quad::FULL, 3, 2), white);
        let flat = Quad([
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.5),
            Point::new(1.0, 1.0),
            Point::new(0.2, 0.2),
        ]);
        assert_eq!(warp(&fields(8, 8), &flat, 3, 2), white);
    }
}
