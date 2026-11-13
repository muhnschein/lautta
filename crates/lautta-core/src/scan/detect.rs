// SPDX-License-Identifier: LGPL-2.1-or-later
//! Finds the page in a photo: the largest four-sided shape whose sides run along straight edges
//! all the way round. Lines from a Hough search over colour edges on small copies; sides then
//! moved onto the edge in the photo itself. No answer (page cut by the photo, too faint, a stack
//! of like sheets) = caller keeps the whole picture.

use super::{Point, Quad};
use image::imageops::{self, FilterType};
use image::RgbImage;
use std::f32::consts::PI;

/// Longer side of the small copies searched. Coarse one sees past print on a page that fills the
/// photo; fine one finds a page further away. Larger page found wins.
const SCALES: [u32; 2] = [320, 640];
/// Smooths sensor noise and paper grain before edges.
const BLUR_SIGMA: f32 = 1.0;
/// Weakest edge kept: colour gradient, Sobel on 0..255 channels.
const MIN_EDGE: f32 = 24.0;
/// Hough angle steps over half a turn.
const ANGLES: usize = 180;
/// An edge pixel votes for lines this many angle steps either side of its own direction.
const SPREAD: i32 = 5;
/// Strongest lines tried as page sides.
const MAX_LINES: usize = 48;
/// Edge pixel counts for a line when their directions differ at most this much.
const MATCH: f32 = 10.0 * PI / 180.0;
/// Opposite sides lean at most this much against each other (perspective).
const MAX_LEAN: f32 = 35.0 * PI / 180.0;
/// Corners of a page seen at a slant stay within these angles.
const MIN_CORNER: f32 = 40.0 * PI / 180.0;
/// Share of the photo a page covers at least.
const MIN_COVER: f32 = 0.03;
/// Share of its length every side runs along an edge …
const MIN_SIDE: f32 = 0.6;
/// … and all four together.
const MIN_MEAN: f32 = 0.75;
/// Near each corner (this share of the side) edges cover at least MIN_END: a side running on
/// past the page has none there.
const END: f32 = 0.2;
const MIN_END: f32 = 0.35;
/// Points measured per side on the photo, and how far from the found side (in pixels of the
/// small copy).
const PROBES: usize = 24;
const REACH: f32 = 1.5;
/// Outermost edge at least this strong against the strongest wins: print near the page's edge
/// is often sharper than the edge itself.
const OUTER: f32 = 0.5;
/// No edge on the photo where colour changes less (three samples summed, over two pixels).
const MIN_RISE: f32 = 12.0;

type V = (f32, f32);

fn dot(a: V, b: V) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

fn add(a: V, b: V, k: f32) -> V {
    (a.0 + b.0 * k, a.1 + b.1 * k)
}

/// Difference of two undirected angles, 0..π/2.
fn angle_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(PI);
    d.min(PI - d)
}

/// Straight line: points p with n·p = rho, n unit.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Line {
    n: V,
    rho: f32,
}

impl Line {
    fn from_angle(angle: f32, rho: f32) -> Line {
        Line {
            n: (angle.cos(), angle.sin()),
            rho,
        }
    }

    /// Direction of the normal, 0..π.
    fn angle(&self) -> f32 {
        self.n.1.atan2(self.n.0).rem_euclid(PI)
    }

    fn along(&self) -> V {
        (-self.n.1, self.n.0)
    }

    fn meet(&self, other: &Line) -> Option<V> {
        let det = self.n.0 * other.n.1 - self.n.1 * other.n.0;
        if det.abs() < 1e-6 {
            return None;
        }
        Some((
            (self.rho * other.n.1 - other.rho * self.n.1) / det,
            (self.n.0 * other.rho - other.n.0 * self.rho) / det,
        ))
    }

    /// Signed distance of `p`, positive on the side `n` points to.
    fn off(&self, p: V) -> f32 {
        dot(self.n, p) - self.rho
    }

    /// Total least squares through `points`, normal turned towards `towards`.
    fn fit(points: &[V], towards: V) -> Option<Line> {
        if points.len() < 2 {
            return None;
        }
        let k = points.len() as f32;
        let m = points.iter().fold((0.0, 0.0), |s, p| add(s, *p, 1.0 / k));
        let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
        for p in points {
            let (dx, dy) = (p.0 - m.0, p.1 - m.1);
            xx += dx * dx;
            yy += dy * dy;
            xy += dx * dy;
        }
        let normal = 0.5 * (2.0 * xy).atan2(xx - yy) + PI / 2.0;
        let mut n = (normal.cos(), normal.sin());
        if dot(n, towards) < 0.0 {
            n = (-n.0, -n.1);
        }
        Some(Line { n, rho: dot(n, m) })
    }

    /// Part inside a `w` × `h` picture: start point and length.
    fn span(&self, w: f32, h: f32) -> Option<(V, f32)> {
        let foot = (self.n.0 * self.rho, self.n.1 * self.rho);
        let d = self.along();
        let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
        for (c, dc, most) in [(foot.0, d.0, w), (foot.1, d.1, h)] {
            if dc.abs() < 1e-6 {
                if !(0.0..=most).contains(&c) {
                    return None;
                }
            } else {
                let (a, b) = (-c / dc, (most - c) / dc);
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
            }
        }
        (hi - lo >= 1.0).then(|| (add(foot, d, lo), hi - lo))
    }
}

/// Thinned colour edges of a small picture.
struct Edges {
    w: usize,
    h: usize,
    /// Direction across the edge, 0..π; NaN off edges.
    angle: Vec<f32>,
}

impl Edges {
    fn new(img: &RgbImage) -> Edges {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut planes = [vec![0.0f32; w * h], vec![0.0f32; w * h], vec![0.0f32; w * h]];
        for (i, p) in img.pixels().enumerate() {
            for (c, plane) in planes.iter_mut().enumerate() {
                plane[i] = f32::from(p[c]);
            }
        }
        let mut mag = vec![0.0f32; w * h];
        let mut dir = vec![0.0f32; w * h];
        for y in 1..h.saturating_sub(1) {
            for x in 1..w - 1 {
                let (i, up, down) = (y * w + x, (y - 1) * w + x, (y + 1) * w + x);
                let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
                for v in &planes {
                    let gx =
                        v[up + 1] + 2.0 * v[i + 1] + v[down + 1] - v[up - 1] - 2.0 * v[i - 1] - v[down - 1];
                    let gy = v[down - 1] + 2.0 * v[down] + v[down + 1] - v[up - 1] - 2.0 * v[up] - v[up + 1];
                    xx += gx * gx;
                    yy += gy * gy;
                    xy += gx * gy;
                }
                // Di Zenzo: strongest change over all three channels, so tint counts too.
                let half = 0.5 * (xx - yy);
                mag[i] = (0.5 * (xx + yy) + (half * half + xy * xy).sqrt()).sqrt();
                dir[i] = (0.5 * (2.0 * xy).atan2(xx - yy)).rem_euclid(PI);
            }
        }
        let mut angle = vec![f32::NAN; w * h];
        for y in 1..h.saturating_sub(1) {
            for x in 1..w - 1 {
                let i = y * w + x;
                if mag[i] < MIN_EDGE {
                    continue;
                }
                // Kept where strongest across the edge.
                let step = match ((dir[i] / (PI / 4.0)).round() as usize) % 4 {
                    0 => 1,
                    1 => w + 1,
                    2 => w,
                    _ => w - 1,
                };
                if mag[i] > mag[i + step] && mag[i] >= mag[i - step] {
                    angle[i] = dir[i];
                }
            }
        }
        Edges { w, h, angle }
    }

    /// Edge pixel under `p` running at `angle` (of its normal).
    fn on(&self, p: V, angle: f32) -> bool {
        let (x, y) = (p.0.floor(), p.1.floor());
        if x < 0.0 || y < 0.0 || x >= self.w as f32 || y >= self.h as f32 {
            return false;
        }
        angle_gap(self.angle[y as usize * self.w + x as usize], angle) <= MATCH
    }

    /// Strongest lines, each with votes of a side of a small page at least.
    fn lines(&self) -> Vec<Line> {
        let reach = (self.w as f32).hypot(self.h as f32).ceil() as i32;
        let rows = (2 * reach + 1) as usize;
        let unit: Vec<V> = (0..ANGLES)
            .map(|t| {
                let a = t as f32 * PI / ANGLES as f32;
                (a.cos(), a.sin())
            })
            .collect();
        let mut votes = vec![0u32; ANGLES * rows];
        for (i, a) in self.angle.iter().enumerate() {
            if a.is_nan() {
                continue;
            }
            let p = ((i % self.w) as f32 + 0.5, (i / self.w) as f32 + 0.5);
            let own = (a * ANGLES as f32 / PI).round() as i32;
            for t in own - SPREAD..=own + SPREAD {
                let t = t.rem_euclid(ANGLES as i32) as usize;
                let r = dot(p, unit[t]).round() as i32 + reach;
                votes[t * rows + r as usize] += 1;
            }
        }
        // Across angle 0 = π the same line has rho negated.
        let at = |t: i32, r: i32| -> (u32, usize) {
            let (t, r) = if (0..ANGLES as i32).contains(&t) {
                (t, r)
            } else {
                (t.rem_euclid(ANGLES as i32), 2 * reach - r)
            };
            if !(0..rows as i32).contains(&r) {
                return (0, 0);
            }
            let i = t as usize * rows + r as usize;
            (votes[i], i)
        };
        let least = (0.04 * self.w.max(self.h) as f32).max(8.0) as u32;
        let mut peaks: Vec<(u32, usize)> = Vec::new();
        for t in 0..ANGLES as i32 {
            for r in 0..rows as i32 {
                let own = at(t, r);
                if own.0 < least {
                    continue;
                }
                let top = (-2..=2)
                    .flat_map(|dt| (-2..=2).map(move |dr| (dt, dr)))
                    .all(|(dt, dr)| (dt, dr) == (0, 0) || at(t + dt, r + dr) < own);
                if top {
                    peaks.push(own);
                }
            }
        }
        peaks.sort_unstable_by(|a, b| b.cmp(a));
        let mut kept: Vec<Line> = Vec::new();
        for (_, i) in peaks {
            let coarse = Line::from_angle(
                (i / rows) as f32 * PI / ANGLES as f32,
                (i % rows) as i32 as f32 - reach as f32,
            );
            let Some(line) = self.settle(coarse) else {
                continue;
            };
            let twin = kept.iter().any(|k| {
                angle_gap(k.angle(), line.angle()) < 2f32.to_radians()
                    && (dot(k.n, line.n) * line.rho - k.rho).abs() < 3.0
            });
            if !twin {
                kept.push(line);
                if kept.len() == MAX_LINES {
                    break;
                }
            }
        }
        kept
    }

    /// Hough line fitted to the edge pixels along it, twice, the second time closer.
    fn settle(&self, mut line: Line) -> Option<Line> {
        for width in [2.0, 1.0] {
            let (start, len) = line.span(self.w as f32, self.h as f32)?;
            let (d, angle) = (line.along(), line.angle());
            let mut points = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for k in 0..len as usize {
                let p = add(start, d, k as f32 + 0.5);
                for o in [-width, -width / 2.0, 0.0, width / 2.0, width] {
                    let q = add(p, line.n, o);
                    let pixel = (q.0.floor(), q.1.floor());
                    if self.on(q, angle) && seen.insert((pixel.0 as i32, pixel.1 as i32)) {
                        points.push((pixel.0 + 0.5, pixel.1 + 0.5));
                    }
                }
            }
            if points.len() < 8 {
                return None;
            }
            line = Line::fit(&points, line.n)?;
        }
        Some(line)
    }
}

/// Where along a line edges run, counted from where it enters the picture.
struct Track {
    line: Line,
    start: V,
    /// Running count of steps with an edge.
    hits: Vec<u32>,
}

impl Track {
    fn new(edges: &Edges, line: Line) -> Option<Track> {
        let (start, len) = line.span(edges.w as f32, edges.h as f32)?;
        let (d, angle) = (line.along(), line.angle());
        let mut hits = vec![0u32];
        for k in 0..len as usize {
            let p = add(start, d, k as f32 + 0.5);
            let found = [0.0, -1.0, 1.0]
                .iter()
                .any(|o| edges.on(add(p, line.n, *o), angle));
            hits.push(hits[k] + u32::from(found));
        }
        Some(Track { line, start, hits })
    }

    /// Length between `a` and `b` (on the line), and steps with an edge there.
    fn between(&self, a: V, b: V) -> (f32, u32) {
        let d = self.line.along();
        let last = self.hits.len() - 1;
        let step =
            |p: V| (dot((p.0 - self.start.0, p.1 - self.start.1), d).round().max(0.0) as usize).min(last);
        let (i, j) = (step(a), step(b));
        let length = (a.0 - b.0).hypot(a.1 - b.1).max(1.0);
        (length, self.hits[i.max(j)] - self.hits[i.min(j)])
    }
}

/// Corners clockwise on screen from the one nearest the top left, if convex.
fn clockwise(mut corners: [V; 4]) -> Option<[V; 4]> {
    let turn = |c: &[V; 4], i: usize| {
        let (a, b, p) = (c[i], c[(i + 1) % 4], c[(i + 2) % 4]);
        (b.0 - a.0) * (p.1 - b.1) - (b.1 - a.1) * (p.0 - b.0)
    };
    if (0..4).all(|i| turn(&corners, i) < 0.0) {
        corners.reverse();
    }
    if !(0..4).all(|i| turn(&corners, i) > 0.0) {
        return None;
    }
    let key = |p: V| (p.0 + p.1, p.1);
    let first = (0..4)
        .min_by(|&i, &j| {
            key(corners[i])
                .partial_cmp(&key(corners[j]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    Some([0, 1, 2, 3].map(|i| corners[(first + i) % 4]))
}

fn area(c: &[V; 4]) -> f32 {
    let twice: f32 = (0..4)
        .map(|i| {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    twice.abs() / 2.0
}

/// Every corner between MIN_CORNER and its complement.
fn square_enough(c: &[V; 4]) -> bool {
    (0..4).all(|i| {
        let (a, p, b) = (c[(i + 3) % 4], c[i], c[(i + 1) % 4]);
        let (u, v) = ((a.0 - p.0, a.1 - p.1), (b.0 - p.0, b.1 - p.1));
        let cos = dot(u, v) / (u.0.hypot(u.1) * v.0.hypot(v.1)).max(1e-6);
        cos.abs() <= MIN_CORNER.cos()
    })
}

/// Why four lines make no page.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Miss {
    /// A corner outside the picture, or two sides parallel.
    Open,
    Small,
    Slanted,
    Crossed,
    BareEnd,
    BareSide,
    Sparse,
}

/// Page with sides on `sides` (in order round it) in a `w` × `h` picture: corners (corner k
/// ends side k) and score, the area.
fn judge(sides: [&Track; 4], w: f32, h: f32) -> Result<([V; 4], f32), Miss> {
    let inside = |p: &V| (-1.0..=w + 1.0).contains(&p.0) && (-1.0..=h + 1.0).contains(&p.1);
    let mut corners = [(0.0, 0.0); 4];
    for k in 0..4 {
        corners[k] = sides[k]
            .line
            .meet(&sides[(k + 1) % 4].line)
            .filter(inside)
            .ok_or(Miss::Open)?;
    }
    let size = area(&corners);
    if size < MIN_COVER * w * h {
        return Err(Miss::Small);
    }
    if !square_enough(&corners) {
        return Err(Miss::Slanted);
    }
    clockwise(corners).ok_or(Miss::Crossed)?;
    let (mut total, mut hits) = (0.0, 0u32);
    for k in 0..4 {
        let (a, b) = (corners[(k + 3) % 4], corners[k]);
        let at = |f: f32| add(a, (b.0 - a.0, b.1 - a.1), f);
        for (p, q) in [(a, at(END)), (at(1.0 - END), b)] {
            let (len, found) = sides[k].between(p, q);
            if (found as f32) < MIN_END * len {
                return Err(Miss::BareEnd);
            }
        }
        let (len, found) = sides[k].between(a, b);
        if (found as f32) < MIN_SIDE * len {
            return Err(Miss::BareSide);
        }
        total += len;
        hits += found;
    }
    if (hits as f32) < MIN_MEAN * total {
        return Err(Miss::Sparse);
    }
    Ok((corners, size))
}

/// Largest page on a small picture: its corners.
fn best_quad(edges: &Edges) -> Option<[V; 4]> {
    let tracks: Vec<Track> = edges
        .lines()
        .into_iter()
        .filter_map(|l| Track::new(edges, l))
        .collect();
    let (w, h) = (edges.w as f32, edges.h as f32);
    let inside = |p: V| (0.0..=w).contains(&p.0) && (0.0..=h).contains(&p.1);
    // Opposite sides: near parallel, not crossing in the picture.
    let mut pairs = Vec::new();
    for i in 0..tracks.len() {
        for j in i + 1..tracks.len() {
            let (a, b) = (&tracks[i].line, &tracks[j].line);
            if angle_gap(a.angle(), b.angle()) <= MAX_LEAN && !a.meet(b).is_some_and(inside) {
                pairs.push((i, j));
            }
        }
    }
    let mut best: Option<([V; 4], f32)> = None;
    for (x, &(a, b)) in pairs.iter().enumerate() {
        for &(c, d) in &pairs[x + 1..] {
            if [c, d].contains(&a) || [c, d].contains(&b) {
                continue;
            }
            if let Ok((corners, size)) = judge([a, c, b, d].map(|k| &tracks[k]), w, h) {
                if best.is_none_or(|(_, top)| size > top) {
                    best = Some((corners, size));
                }
            }
        }
    }
    best.map(|(corners, _)| corners)
}

/// Colour at `p` (pixel centres at .5), bilinear, edge pixels repeated.
fn sample(img: &RgbImage, p: V) -> [f32; 3] {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let x = (p.0 - 0.5).clamp(0.0, w - 1.0);
    let y = (p.1 - 0.5).clamp(0.0, h - 1.0);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x1, y1) = ((x0 + 1.0).min(w - 1.0), (y0 + 1.0).min(h - 1.0));
    let get = |x: f32, y: f32| img.get_pixel(x as u32, y as u32).0.map(f32::from);
    let (a, b, c, d) = (get(x0, y0), get(x1, y0), get(x0, y1), get(x1, y1));
    [0, 1, 2].map(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy)
}

/// Side from `a` to `b` of a page around `centre`, moved onto the outermost clear edge within
/// `reach` on the photo.
fn refine(photo: &RgbImage, a: V, b: V, centre: V, reach: f32) -> Option<Line> {
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let d = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    let mut n = (-d.1, d.0);
    if dot(n, (a.0 - centre.0, a.1 - centre.1)) < 0.0 {
        n = (-n.0, -n.1);
    }
    let steps = reach.ceil() as i32;
    let mut points = Vec::with_capacity(PROBES);
    for j in 0..PROBES {
        let p = add(
            a,
            (b.0 - a.0, b.1 - a.1),
            0.15 + 0.7 * j as f32 / (PROBES - 1) as f32,
        );
        // Mean of three points along the side: text strokes count less.
        let colour = |o: f32| {
            [-2.0, 0.0, 2.0].iter().fold([0.0f32; 3], |sum, s| {
                let c = sample(photo, add(add(p, d, *s), n, o));
                [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]]
            })
        };
        let rise = |o: i32| {
            let (u, v) = (colour(o as f32 - 1.0), colour(o as f32 + 1.0));
            (0..3).map(|k| (v[k] - u[k]).powi(2)).sum::<f32>().sqrt()
        };
        let profile: Vec<f32> = (-steps - 1..=steps + 1).map(rise).collect();
        let inner = 1..profile.len() - 1;
        let most = inner.clone().map(|i| profile[i]).fold(0.0, f32::max);
        if most < MIN_RISE {
            continue;
        }
        let Some(top) = inner
            .filter(|&i| {
                profile[i] >= OUTER * most && profile[i] >= profile[i - 1] && profile[i] >= profile[i + 1]
            })
            .max()
        else {
            continue;
        };
        let (l, c, r) = (profile[top - 1], profile[top], profile[top + 1]);
        let bend = l - 2.0 * c + r;
        let shift = if bend < 0.0 { 0.5 * (l - r) / bend } else { 0.0 };
        points.push(add(p, n, top as f32 - (steps + 1) as f32 + shift));
    }
    let line = Line::fit(&points, n)?;
    let mut off: Vec<f32> = points.iter().map(|p| line.off(*p).abs()).collect();
    off.sort_unstable_by(f32::total_cmp);
    // Points off by more than three spreads (median deviation) are print or shadow.
    let limit = (3.0 * 1.4826 * off[off.len() / 2]).max(1.0);
    let kept: Vec<V> = points
        .into_iter()
        .filter(|p| line.off(*p).abs() <= limit)
        .collect();
    if kept.len() < PROBES / 2 {
        return None;
    }
    Line::fit(&kept, n)
}

/// Corners of the page on a copy of `photo` with `side` as its longer side, in photo pixels,
/// and the copy's scale.
fn coarse(photo: &RgbImage, side: u32) -> Option<([V; 4], f32)> {
    let long = photo.width().max(photo.height());
    let k = side as f32 / long as f32;
    let (w, h) = (
        ((photo.width() as f32 * k).round() as u32).max(8),
        ((photo.height() as f32 * k).round() as u32).max(8),
    );
    let small = imageops::blur(&imageops::resize(photo, w, h, FilterType::Triangle), BLUR_SIGMA);
    let corners = best_quad(&Edges::new(&small))?;
    let (sx, sy) = (photo.width() as f32 / w as f32, photo.height() as f32 / h as f32);
    Some((corners.map(|p| (p.0 * sx, p.1 * sy)), sx.max(sy)))
}

/// `photo`: upright, any size.
pub fn find_page(photo: &RgbImage) -> Option<Quad> {
    if photo.width() < 8 || photo.height() < 8 {
        return None;
    }
    let long = photo.width().max(photo.height());
    let mut sides = SCALES.map(|s| s.min(long)).to_vec();
    sides.dedup();
    let (corners, scale) = sides
        .into_iter()
        .filter_map(|side| coarse(photo, side))
        .max_by(|a, b| area(&a.0).total_cmp(&area(&b.0)))?;
    let centre = corners.iter().fold((0.0, 0.0), |s, p| add(s, *p, 0.25));
    let reach = REACH * scale + 2.0;
    let moved = (0..4)
        .map(|k| refine(photo, corners[(k + 3) % 4], corners[k], centre, reach))
        .collect::<Option<Vec<Line>>>()
        .and_then(|lines| {
            let mut c = [(0.0, 0.0); 4];
            for k in 0..4 {
                c[k] = lines[k].meet(&lines[(k + 1) % 4])?;
                // A corner moving far means a side caught something else.
                if (c[k].0 - corners[k].0).hypot(c[k].1 - corners[k].1) > 2.0 * reach {
                    return None;
                }
            }
            Some(c)
        });
    let (w, h) = (photo.width() as f32, photo.height() as f32);
    let found = clockwise(moved.unwrap_or(corners))?;
    let quad = Quad(found.map(|p| Point::new((p.0 / w).clamp(0.0, 1.0), (p.1 / h).clamp(0.0, 1.0))));
    quad.is_page().then_some(quad)
}

#[cfg(test)]
mod tests;
