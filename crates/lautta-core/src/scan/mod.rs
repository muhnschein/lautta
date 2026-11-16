// SPDX-License-Identifier: LGPL-2.1-or-later
//! Document scanning: photos of pages become one PDF. Pure CPU, call from blocking contexts.

pub mod detect;
pub mod pdf;
pub mod session;
pub mod tone;
pub mod warp;

use serde::{Deserialize, Serialize};

/// Position in the upright photo, both axes 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }
}

/// Page corners: top left, top right, bottom right, bottom left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quad(pub [Point; 4]);

/// Corners closer than this to each other on either diagonal make no page.
const MIN_AREA: f32 = 0.0004;

impl Quad {
    pub const FULL: Quad = Quad([
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ]);

    pub fn is_full(&self) -> bool {
        *self == Quad::FULL
    }

    /// Flat `[x0, y0, … x3, y3]`, corners clamped into the photo. `None` unless eight finite
    /// numbers forming a convex shape with corners in clockwise order.
    pub fn from_flat(values: &[f32]) -> Option<Quad> {
        let v: &[f32; 8] = values.try_into().ok()?;
        if v.iter().any(|n| !n.is_finite()) {
            return None;
        }
        let at = |i: usize| Point::new(v[2 * i].clamp(0.0, 1.0), v[2 * i + 1].clamp(0.0, 1.0));
        let quad = Quad([at(0), at(1), at(2), at(3)]);
        quad.is_page().then_some(quad)
    }

    pub fn to_flat(&self) -> [f32; 8] {
        let p = &self.0;
        [p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y, p[3].x, p[3].y]
    }

    /// Clockwise on screen (y down), every corner a left turn, not degenerate.
    pub fn is_page(&self) -> bool {
        let p = &self.0;
        let turns_right = (0..4).all(|i| {
            let (a, b, c) = (p[i], p[(i + 1) % 4], p[(i + 2) % 4]);
            (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x) > 0.0
        });
        turns_right && self.area() >= MIN_AREA
    }

    /// Shoelace formula; positive for clockwise corners on screen.
    pub fn area(&self) -> f32 {
        let p = &self.0;
        (0..4)
            .map(|i| {
                let (a, b) = (p[i], p[(i + 1) % 4]);
                a.x * b.y - b.x * a.y
            })
            .sum::<f32>()
            / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Colour {
    #[default]
    Original,
    Grayscale,
    BlackWhite,
}

impl Colour {
    pub fn name(self) -> &'static str {
        match self {
            Colour::Original => "original",
            Colour::Grayscale => "grayscale",
            Colour::BlackWhite => "blackwhite",
        }
    }

    pub fn parse(name: &str) -> Option<Colour> {
        [Colour::Original, Colour::Grayscale, Colour::BlackWhite]
            .into_iter()
            .find(|c| c.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_picture_is_a_page_of_area_one() {
        assert!(Quad::FULL.is_full());
        assert!(Quad::FULL.is_page());
        assert!((Quad::FULL.area() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn flat_form_round_trips_and_clamps() {
        let flat = [0.1, 0.2, 0.9, 0.1, 0.8, 0.9, 0.2, 0.8];
        let quad = Quad::from_flat(&flat).unwrap();
        assert_eq!(quad.to_flat(), flat);
        assert!(!quad.is_full());
        let outside = Quad::from_flat(&[-0.5, -1.0, 2.0, 0.0, 1.5, 3.0, 0.0, 1.0]).unwrap();
        assert_eq!(outside, Quad::FULL);
    }

    #[test]
    fn broken_corner_lists_are_refused() {
        assert_eq!(Quad::from_flat(&[0.0; 7]), None);
        assert_eq!(Quad::from_flat(&[0.0; 9]), None);
        assert_eq!(
            Quad::from_flat(&[0.0, 0.0, f32::NAN, 0.0, 1.0, 1.0, 0.0, 1.0]),
            None
        );
        // Counter-clockwise.
        assert_eq!(Quad::from_flat(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0]), None);
        // Crossed: top corners swapped.
        assert_eq!(Quad::from_flat(&[1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0]), None);
        // Dent: bottom right corner pulled past the diagonal.
        assert_eq!(Quad::from_flat(&[0.0, 0.0, 1.0, 0.0, 0.2, 0.2, 0.0, 1.0]), None);
        // All corners in one spot.
        assert_eq!(
            Quad::from_flat(&[0.5, 0.5, 0.51, 0.5, 0.51, 0.51, 0.5, 0.51]),
            None
        );
    }

    #[test]
    fn smallest_page_is_two_percent_wide_and_high() {
        let square = |side: f32| Quad::from_flat(&[0.0, 0.0, side, 0.0, side, side, 0.0, side]);
        assert!(square(0.021).is_some());
        assert!(square(0.019).is_none());
    }

    #[test]
    fn colour_names_round_trip() {
        for c in [Colour::Original, Colour::Grayscale, Colour::BlackWhite] {
            assert_eq!(Colour::parse(c.name()), Some(c));
        }
        assert_eq!(Colour::parse("sepia"), None);
        assert_eq!(Colour::default(), Colour::Original);
        assert_eq!(Colour::BlackWhite.name(), "blackwhite");
    }
}
