// SPDX-License-Identifier: LGPL-2.1-or-later
use super::*;
use image::Rgb;

const PAPER: [u8; 3] = [230, 230, 230];
const TABLE: [u8; 3] = [40, 40, 40];
const SHEET: [(f32, f32); 4] = [(0.2, 0.15), (0.8, 0.2), (0.85, 0.85), (0.15, 0.8)];

fn within(p: (f32, f32), c: &[(f32, f32); 4]) -> bool {
    (0..4).all(|i| {
        let (a, b) = (c[i], c[(i + 1) % 4]);
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= 0.0
    })
}

/// Sheet with the given corners (relative) on a table; edges smooth (4 × 4 samples a pixel).
fn photo(w: u32, h: u32, corners: [(f32, f32); 4], sheet: [u8; 3], table: [u8; 3]) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let mut on = 0u32;
        for sy in 0..4 {
            for sx in 0..4 {
                let p = (
                    (x as f32 + (sx as f32 + 0.5) / 4.0) / w as f32,
                    (y as f32 + (sy as f32 + 0.5) / 4.0) / h as f32,
                );
                on += u32::from(within(p, &corners));
            }
        }
        Rgb([0, 1, 2].map(|c| ((u32::from(sheet[c]) * on + u32::from(table[c]) * (16 - on)) / 16) as u8))
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
            "corner {got:?}, wanted {want:?}: {quad:?}"
        );
    }
}

#[test]
fn bright_sheet_on_dark_table_is_found() {
    assert_close(find_page(&photo(640, 480, SHEET, PAPER, TABLE)), SHEET, 0.004);
}

#[test]
fn dark_sheet_on_bright_table_is_found() {
    assert_close(find_page(&photo(640, 480, SHEET, TABLE, PAPER)), SHEET, 0.004);
}

#[test]
fn sheet_of_the_table_brightness_but_another_colour_is_found() {
    let (green, purple) = ([60, 140, 60], [140, 60, 150]);
    assert_close(find_page(&photo(640, 480, SHEET, green, purple)), SHEET, 0.004);
}

#[test]
fn portrait_photos_work_like_landscape_ones() {
    assert_close(find_page(&photo(300, 800, SHEET, PAPER, TABLE)), SHEET, 0.004);
}

#[test]
fn strongly_turned_sheet_is_found_with_top_left_first() {
    let diamond = [(0.3, 0.3), (0.6, 0.1), (0.75, 0.6), (0.4, 0.85)];
    assert_close(find_page(&photo(640, 480, diamond, PAPER, TABLE)), diamond, 0.004);
}

/// Sides leaning 10° and 20° to the middle: 20° apart is perspective, 40° no page.
#[test]
fn sides_converge_only_as_far_as_perspective_makes_them() {
    let photo_of = |top: (f32, f32)| {
        let corners = [(top.0, 0.05), (top.1, 0.05), (0.9, 0.95), (0.1, 0.95)];
        (corners, find_page(&photo(640, 480, corners, PAPER, TABLE)))
    };
    let (seen, found) = photo_of((0.219, 0.781));
    assert_close(found, seen, 0.004);
    assert_eq!(photo_of((0.345, 0.655)).1, None);
}

#[test]
fn text_on_the_sheet_does_not_move_the_corners() {
    let mut img = photo(640, 480, SHEET, PAPER, TABLE);
    for line in 0..12 {
        fill(&mut img, 180..460, 130 + line * 20..133 + line * 20, [30, 30, 30]);
    }
    assert_close(find_page(&img), SHEET, 0.004);
}

#[test]
fn picture_printed_on_the_sheet_is_not_the_page() {
    let square = [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)];
    let mut img = photo(640, 480, square, PAPER, TABLE);
    fill(&mut img, 150..420, 120..300, [200, 30, 30]);
    fill(&mut img, 160..480, 320..350, [90, 90, 90]);
    assert_close(find_page(&img), square, 0.004);
}

#[test]
fn frame_printed_close_to_a_faint_edge_is_not_the_page() {
    // Sharp black frame just inside a sheet barely lighter than the table.
    let square = [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)];
    let mut img = photo(1600, 1200, square, [215, 215, 215], [185, 185, 185]);
    let (l, t, r, b) = (334, 254, 1266, 946);
    fill(&mut img, l..r, t..t + 5, [20, 20, 20]);
    fill(&mut img, l..r, b - 5..b, [20, 20, 20]);
    fill(&mut img, l..l + 5, t..b, [20, 20, 20]);
    fill(&mut img, r - 5..r, t..b, [20, 20, 20]);
    assert_close(find_page(&img), square, 0.002);
}

#[test]
fn table_edge_beyond_the_sheet_is_no_side() {
    // Dark band across the photo above the sheet: the sheet's sides lead up to it.
    let square = [(0.25, 0.3), (0.75, 0.3), (0.75, 0.85), (0.25, 0.85)];
    let mut img = photo(640, 480, square, PAPER, [120, 120, 120]);
    fill(&mut img, 0..640, 0..96, [30, 30, 30]);
    assert_close(find_page(&img), square, 0.004);
}

#[test]
fn corners_on_a_large_photo_are_found_to_the_pixel() {
    let corners = [(0.213, 0.161), (0.788, 0.192), (0.842, 0.851), (0.157, 0.803)];
    let found = find_page(&photo(1600, 1200, corners, [225, 222, 215], [120, 100, 80]));
    assert_close(found, corners, 0.0015);
}

#[test]
fn small_far_sheet_is_found() {
    let far = [(0.42, 0.4), (0.62, 0.41), (0.61, 0.62), (0.41, 0.6)];
    let mut img = photo(1600, 1200, far, PAPER, [110, 90, 70]);
    for line in 0..10 {
        fill(&mut img, 700..960, 510 + line * 22..514 + line * 22, [40, 40, 40]);
    }
    assert_close(find_page(&img), far, 0.003);
}

#[test]
fn sheet_cut_by_the_photo_edge_has_no_page() {
    let cut = [(0.0, 0.2), (0.7, 0.2), (0.7, 0.8), (0.0, 0.8)];
    assert_eq!(find_page(&photo(640, 480, cut, PAPER, TABLE)), None);
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
        find_page(&photo(640, 480, SHEET, [150, 150, 150], [147, 147, 147])),
        None
    );
    let clear = find_page(&photo(640, 480, SHEET, [150, 150, 150], [135, 135, 135]));
    assert_close(clear, SHEET, 0.004);
}

#[test]
fn small_spot_is_not_a_page() {
    // A little under, and over, a thirtieth of the photo.
    let spot = [(0.4, 0.4), (0.57, 0.4), (0.57, 0.57), (0.4, 0.57)];
    assert_eq!(find_page(&photo(640, 480, spot, PAPER, TABLE)), None);
    let page = [(0.4, 0.4), (0.6, 0.4), (0.6, 0.6), (0.4, 0.6)];
    assert_close(find_page(&photo(640, 480, page, PAPER, TABLE)), page, 0.004);
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
fn angle_gap_is_the_smaller_way_round_half_a_turn() {
    assert!((angle_gap(0.1, PI - 0.1) - 0.2).abs() < 1e-5);
    assert!((angle_gap(0.3, 0.1) - 0.2).abs() < 1e-5);
    assert!((angle_gap(0.1, 0.3) - 0.2).abs() < 1e-5);
    assert!((angle_gap(PI / 2.0, 0.0) - PI / 2.0).abs() < 1e-5);
}

fn near(a: V, b: V) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

#[test]
fn lines_meet_where_both_hold() {
    let across = Line {
        n: (0.0, 1.0),
        rho: 5.0,
    };
    let down = Line {
        n: (1.0, 0.0),
        rho: 3.0,
    };
    assert!(near(across.meet(&down).unwrap(), (3.0, 5.0)));
    let slant = Line::from_angle(PI / 4.0, 2f32.sqrt() * 4.0);
    assert!(near(slant.meet(&down).unwrap(), (3.0, 5.0)));
    assert!(near(down.meet(&slant).unwrap(), (3.0, 5.0)));
    assert_eq!(
        across.meet(&Line {
            n: (0.0, 1.0),
            rho: 9.0
        }),
        None
    );
}

#[test]
fn line_angle_and_distance() {
    let line = Line::from_angle(PI * 0.75, 2.0);
    assert!((line.angle() - PI * 0.75).abs() < 1e-5);
    let flipped = Line {
        n: (-line.n.0, -line.n.1),
        rho: -2.0,
    };
    assert!(
        (flipped.angle() - PI * 0.75).abs() < 1e-5,
        "same line: {}",
        flipped.angle()
    );
    let across = Line {
        n: (0.0, 1.0),
        rho: 5.0,
    };
    assert!(near(across.along(), (-1.0, 0.0)));
    assert!((across.off((7.0, 8.0)) - 3.0).abs() < 1e-5);
    assert!((across.off((7.0, 1.0)) + 4.0).abs() < 1e-5);
}

#[test]
fn fit_runs_through_the_points_facing_the_given_way() {
    let points = [(0.0, 2.0), (4.0, 4.0), (8.0, 6.0), (2.0, 3.0)];
    let up = Line::fit(&points, (0.0, -1.0)).unwrap();
    for p in points {
        assert!(up.off(p).abs() < 1e-4, "{up:?} misses {p:?}");
    }
    assert!(up.n.1 < 0.0);
    let down = Line::fit(&points, (0.0, 1.0)).unwrap();
    assert!(down.n.1 > 0.0 && (down.rho + up.rho).abs() < 1e-4);
    // Least squares: middle of two parallel rows.
    let rows = [(0.0, 0.0), (10.0, 0.0), (0.0, 2.0), (10.0, 2.0)];
    let mid = Line::fit(&rows, (0.0, 1.0)).unwrap();
    assert!(near(mid.n, (0.0, 1.0)) && (mid.rho - 1.0).abs() < 1e-4, "{mid:?}");
    assert_eq!(Line::fit(&[(1.0, 1.0)], (0.0, 1.0)), None);
}

#[test]
fn span_is_the_part_inside_the_picture() {
    let across = Line {
        n: (0.0, 1.0),
        rho: 5.0,
    };
    let (start, len) = across.span(10.0, 8.0).unwrap();
    assert!(
        near(start, (10.0, 5.0)) && (len - 10.0).abs() < 1e-4,
        "{start:?} {len}"
    );
    let down = Line {
        n: (1.0, 0.0),
        rho: 3.0,
    };
    let (start, len) = down.span(10.0, 8.0).unwrap();
    assert!(
        near(start, (3.0, 0.0)) && (len - 8.0).abs() < 1e-4,
        "{start:?} {len}"
    );
    // On the border: still inside.
    let (start, len) = Line {
        n: (1.0, 0.0),
        rho: 0.0,
    }
    .span(10.0, 8.0)
    .unwrap();
    assert!(
        near(start, (0.0, 0.0)) && (len - 8.0).abs() < 1e-4,
        "{start:?} {len}"
    );
    let slant = Line::from_angle(PI / 4.0, 2f32.sqrt() * 4.0);
    let (start, len) = slant.span(10.0, 8.0).unwrap();
    let end = add(start, slant.along(), len);
    assert!(
        near(start, (8.0, 0.0)) && near(end, (0.0, 8.0)),
        "{start:?} {end:?}"
    );
    assert_eq!(
        Line {
            n: (0.0, 1.0),
            rho: 9.0
        }
        .span(10.0, 8.0),
        None
    );
    assert_eq!(
        Line {
            n: (1.0, 0.0),
            rho: -1.0
        }
        .span(10.0, 8.0),
        None
    );
    assert_eq!(slant.span(0.5, 0.5), None, "corner clipped shorter than a pixel");
}

#[test]
fn corners_turn_clockwise_from_top_left() {
    let square = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
    assert_eq!(clockwise(square), Some(square));
    let backwards = [(4.0, 4.0), (4.0, 0.0), (0.0, 0.0), (0.0, 4.0)];
    assert_eq!(clockwise(backwards), Some(square));
    // Tie on x + y: the upper corner leads.
    let diamond = [(0.0, 5.0), (5.0, 0.0), (10.0, 5.0), (5.0, 10.0)];
    assert_eq!(
        clockwise(diamond),
        Some([(5.0, 0.0), (10.0, 5.0), (5.0, 10.0), (0.0, 5.0)])
    );
    let crossed = [(0.0, 0.0), (4.0, 4.0), (4.0, 0.0), (0.0, 4.0)];
    assert_eq!(clockwise(crossed), None);
    let dented = [(0.0, 0.0), (4.0, 0.0), (1.0, 1.0), (0.0, 4.0)];
    assert_eq!(clockwise(dented), None);
}

#[test]
fn area_and_corner_angles() {
    let rect = [(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)];
    assert_eq!(area(&rect), 8.0);
    assert_eq!(area(&[rect[3], rect[2], rect[1], rect[0]]), 8.0);
    assert!(square_enough(&rect));
    // Parallelogram with 45° corners passes, 35° ones do not.
    assert!(square_enough(&[(0.0, 0.0), (4.0, 0.0), (6.0, 2.0), (2.0, 2.0)]));
    let flat = 35f32.to_radians().tan().recip() * 2.0;
    assert!(!square_enough(&[
        (0.0, 0.0),
        (4.0, 0.0),
        (4.0 + flat, 2.0),
        (flat, 2.0)
    ]));
    // One sharp corner and its blunt neighbour are enough to fail.
    assert!(!square_enough(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 1.0),
        (0.0, 16.0)
    ]));
}

/// Edges marked by hand: `marks` = (x range, y range, angle of the normal).
fn edges(w: usize, h: usize, marks: &[(std::ops::Range<usize>, std::ops::Range<usize>, f32)]) -> Edges {
    let mut angle = vec![f32::NAN; w * h];
    for (xs, ys, a) in marks {
        for y in ys.clone() {
            for x in xs.clone() {
                angle[y * w + x] = *a;
            }
        }
    }
    Edges { w, h, angle }
}

const ACROSS: f32 = PI / 2.0;

#[test]
fn nothing_outside_the_picture_is_an_edge() {
    let e = edges(4, 3, &[(0..4, 0..3, 0.0)]);
    assert!(e.on((0.5, 0.5), 0.0) && e.on((3.5, 2.5), 0.0));
    for p in [
        (-0.5, 1.5),
        (4.5, 1.5),
        (1.5, -0.5),
        (1.5, 3.5),
        (-0.5, -0.5),
        (4.5, 2.5),
    ] {
        assert!(!e.on(p, 0.0), "{p:?}");
    }
}

#[test]
fn track_counts_edges_along_the_line_and_beside_it() {
    // Row 10 marked from x 20 to 59, row 11 from 70 to 79, row 13 from 85 to 89 (too far).
    let e = edges(
        100,
        30,
        &[
            (20..60, 10..11, ACROSS),
            (70..80, 11..12, ACROSS),
            (85..90, 13..14, ACROSS),
        ],
    );
    let track = Track::new(
        &e,
        Line {
            n: (0.0, 1.0),
            rho: 10.5,
        },
    )
    .unwrap();
    assert_eq!(track.between((0.0, 10.5), (100.0, 10.5)), (100.0, 50));
    assert_eq!(track.between((60.0, 10.5), (20.0, 10.5)), (40.0, 40));
    assert_eq!(
        track.between((0.0, 10.5), (150.0, 10.5)),
        (150.0, 50),
        "past the end"
    );
    assert_eq!(
        track.between((30.0, 0.0), (40.0, 30.0)).1,
        10,
        "feet on the line count"
    );
    // Edges at another angle do not count.
    let tilted = edges(100, 30, &[(20..60, 10..11, ACROSS + 0.3)]);
    let track = Track::new(
        &tilted,
        Line {
            n: (0.0, 1.0),
            rho: 10.5,
        },
    )
    .unwrap();
    assert_eq!(track.between((0.0, 10.5), (100.0, 10.5)).1, 0);
    let close = edges(100, 30, &[(20..60, 10..11, ACROSS + 0.15)]);
    let track = Track::new(
        &close,
        Line {
            n: (0.0, 1.0),
            rho: 10.5,
        },
    )
    .unwrap();
    assert_eq!(track.between((0.0, 10.5), (100.0, 10.5)).1, 40, "within MATCH");
}

/// Sides of a square from (20, 20) to (80, 80) in a 100 × 100 picture, top, right, bottom, left.
fn square_lines() -> [Line; 4] {
    [
        Line {
            n: (0.0, 1.0),
            rho: 20.5,
        },
        Line {
            n: (1.0, 0.0),
            rho: 79.5,
        },
        Line {
            n: (0.0, 1.0),
            rho: 79.5,
        },
        Line {
            n: (1.0, 0.0),
            rho: 20.5,
        },
    ]
}

fn judged(e: &Edges, lines: [Line; 4]) -> Result<([V; 4], f32), Miss> {
    let tracks = lines.map(|l| Track::new(e, l).unwrap());
    judge(
        [&tracks[0], &tracks[1], &tracks[2], &tracks[3]],
        e.w as f32,
        e.h as f32,
    )
}

/// Square's outline, with `gap` (x range, y range) left out of it.
fn outline(gap: (std::ops::Range<usize>, std::ops::Range<usize>)) -> Edges {
    let mut e = edges(
        100,
        100,
        &[
            (20..80, 20..21, ACROSS),
            (20..80, 79..80, ACROSS),
            (20..21, 20..80, 0.0),
            (79..80, 20..80, 0.0),
        ],
    );
    for y in gap.1 {
        for x in gap.0.clone() {
            e.angle[y * 100 + x] = f32::NAN;
        }
    }
    e
}

#[test]
fn judge_takes_a_square_with_edges_all_round() {
    let (corners, size) = judged(&outline((0..0, 0..0)), square_lines()).unwrap();
    assert!(
        near(corners[0], (79.5, 20.5)) && near(corners[3], (20.5, 20.5)),
        "{corners:?}"
    );
    assert!((size - 59.0 * 59.0).abs() < 1e-2, "{size}");
}

/// Diamond with every corner half a pixel outside the 100 × 100 picture.
#[test]
fn judge_takes_corners_up_to_a_pixel_outside() {
    let c = [(-0.5, 50.0), (50.0, -0.5), (100.5, 50.0), (50.0, 100.5)];
    let mut angle = vec![f32::NAN; 100 * 100];
    let sides = [0, 1, 2, 3].map(|k| {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        let line = Line::fit(&[a, b], (1.0, 1.0)).unwrap();
        for s in 0..=400 {
            let p = add(a, (b.0 - a.0, b.1 - a.1), s as f32 / 400.0);
            if (0.0..100.0).contains(&p.0) && (0.0..100.0).contains(&p.1) {
                angle[p.1 as usize * 100 + p.0 as usize] = line.angle();
            }
        }
        line
    });
    let e = Edges {
        w: 100,
        h: 100,
        angle,
    };
    let (corners, _) = judged(&e, sides).unwrap();
    for want in c {
        assert!(
            corners.iter().any(|got| near(*got, want)),
            "{want:?}: {corners:?}"
        );
    }
}

#[test]
fn judge_says_why_lines_make_no_page() {
    let full = outline((0..0, 0..0));
    let mut out = square_lines();
    out[0] = Line::fit(&[(20.5, 20.5), (79.5, -5.0)], (0.0, 1.0)).unwrap();
    assert_eq!(
        judged(&full, out),
        Err(Miss::Open),
        "top meets right above the picture"
    );
    let mut parallel = square_lines();
    parallel[1] = parallel[0];
    assert_eq!(judged(&full, parallel), Err(Miss::Open));
    let small = [
        Line {
            n: (0.0, 1.0),
            rho: 40.0,
        },
        Line {
            n: (1.0, 0.0),
            rho: 57.0,
        },
        Line {
            n: (0.0, 1.0),
            rho: 57.0,
        },
        Line {
            n: (1.0, 0.0),
            rho: 40.0,
        },
    ];
    assert_eq!(judged(&full, small), Err(Miss::Small));
    // Parallelogram leaning 39° off the horizontal.
    let mut slanted = square_lines();
    slanted[1] = Line::fit(&[(97.0, 20.5), (25.0, 79.5)], (1.0, 0.0)).unwrap();
    slanted[3] = Line::fit(&[(77.0, 20.5), (5.0, 79.5)], (-1.0, 0.0)).unwrap();
    assert_eq!(judged(&full, slanted), Err(Miss::Slanted));
    // Bow tie: corners (10, 10), (90, 60), (90, 10), (10, 90).
    let through = |a: V, b: V| Line::fit(&[a, b], (0.0, 1.0)).unwrap();
    let crossed = [
        through((10.0, 90.0), (10.0, 10.0)),
        through((10.0, 10.0), (90.0, 60.0)),
        through((90.0, 60.0), (90.0, 10.0)),
        through((90.0, 10.0), (10.0, 90.0)),
    ];
    assert_eq!(judged(&full, crossed), Err(Miss::Crossed));
    // Top side bare near its right corner: a fifth of it is 12 pixels, 9 or 7 of them missing.
    assert_eq!(
        judged(&outline((71..80, 20..21)), square_lines()),
        Err(Miss::BareEnd)
    );
    assert!(
        judged(&outline((73..80, 20..21)), square_lines()).is_ok(),
        "7 of 12 missing"
    );
    // Top side bare in the middle: ends fine, side not.
    assert_eq!(
        judged(&outline((30..55, 20..21)), square_lines()),
        Err(Miss::BareSide)
    );
    assert!(
        judged(&outline((33..53, 20..21)), square_lines()).is_ok(),
        "20 of 59 missing"
    );
    // Each side bare for 21 of 59 in the middle: sides pass, all four together not.
    let mut sparse = outline((0..0, 0..0));
    for (xs, ys) in [
        (39..60, 20..21),
        (39..60, 79..80),
        (20..21, 39..60),
        (79..80, 39..60),
    ] {
        for y in ys {
            for x in xs.clone() {
                sparse.angle[y * 100 + x] = f32::NAN;
            }
        }
    }
    assert_eq!(judged(&sparse, square_lines()), Err(Miss::Sparse));
}

#[test]
fn edges_are_thin_and_lie_across_the_change() {
    let img = imageops::blur(
        &photo(
            60,
            40,
            [(0.5, -1.0), (2.0, -1.0), (2.0, 2.0), (0.5, 2.0)],
            PAPER,
            TABLE,
        ),
        1.0,
    );
    let e = Edges::new(&img);
    for y in 2..38 {
        let row: Vec<usize> = (0..60).filter(|x| !e.angle[y * 60 + x].is_nan()).collect();
        assert_eq!(row.len(), 1, "row {y}: {row:?}");
        assert!((29..=30).contains(&row[0]), "row {y}: {row:?}");
        assert!(angle_gap(e.angle[y * 60 + row[0]], 0.0) < 0.01);
    }
    assert!(e.on((row_of(&e, 20) as f32 + 0.5, 20.5), 0.1));
    assert!(!e.on((row_of(&e, 20) as f32 + 0.5, 20.5), 0.3));
    assert!(
        !e.on((-0.5, 20.5), 0.0)
            && !e.on((60.5, 20.5), 0.0)
            && !e.on((5.0, -0.5), 0.0)
            && !e.on((5.0, 40.5), 0.0)
    );
    // Same in the other direction.
    let img = imageops::blur(
        &photo(
            40,
            60,
            [(-1.0, 0.5), (2.0, 0.5), (2.0, 2.0), (-1.0, 2.0)],
            PAPER,
            TABLE,
        ),
        1.0,
    );
    let e = Edges::new(&img);
    let col: Vec<usize> = (0..60).filter(|y| !e.angle[y * 40 + 20].is_nan()).collect();
    assert_eq!(col.len(), 1, "{col:?}");
    assert!(angle_gap(e.angle[col[0] * 40 + 20], ACROSS) < 0.01);
    // Diagonals are thinned along the diagonal: one pixel, or up to three where neighbours
    // lie as far either side.
    let img = imageops::blur(
        &photo(
            60,
            60,
            [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)],
            PAPER,
            TABLE,
        ),
        1.0,
    );
    let e = Edges::new(&img);
    for y in 10..50 {
        let row: Vec<usize> = (0..60).filter(|x| !e.angle[y * 60 + x].is_nan()).collect();
        assert!(
            !row.is_empty() && row.len() <= 3 && row.iter().all(|x| x.abs_diff(y) <= 1),
            "row {y}: {row:?}"
        );
    }
    let other = imageops::blur(
        &photo(
            60,
            60,
            [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (1.0, 0.0)],
            PAPER,
            TABLE,
        ),
        1.0,
    );
    let e = Edges::new(&other);
    for y in 10..50 {
        let row: Vec<usize> = (0..60).filter(|x| !e.angle[y * 60 + x].is_nan()).collect();
        assert!(
            !row.is_empty() && row.len() <= 3 && row.iter().all(|x| x.abs_diff(59 - y) <= 1),
            "row {y}: {row:?}"
        );
    }
    // A faint step leaves no edge.
    let faint = Edges::new(&photo(
        60,
        40,
        [(0.5, -1.0), (2.0, -1.0), (2.0, 2.0), (0.5, 2.0)],
        [100; 3],
        [97; 3],
    ));
    assert!(faint.angle.iter().all(|a| a.is_nan()));
}

fn row_of(e: &Edges, y: usize) -> usize {
    (0..e.w).find(|x| !e.angle[y * e.w + x].is_nan()).unwrap()
}

#[test]
fn colour_change_alone_makes_an_edge() {
    let img = photo(
        60,
        40,
        [(0.5, -1.0), (2.0, -1.0), (2.0, 2.0), (0.5, 2.0)],
        [60, 140, 60],
        [140, 60, 150],
    );
    let e = Edges::new(&imageops::blur(&img, 1.0));
    assert!((2..38).all(|y| (0..60).any(|x| !e.angle[y * 60 + x].is_nan())));
}

#[test]
fn hough_finds_long_lines_once_and_skips_short_ones() {
    // Long line along row 30 (x 10..190), short one along column 150 (y 50..57).
    let e = edges(200, 100, &[(10..190, 30..31, ACROSS), (150..151, 50..57, 0.0)]);
    let lines = e.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        angle_gap(lines[0].angle(), ACROSS) < 1e-3 && (lines[0].off((100.0, 30.5))).abs() < 0.05,
        "{lines:?}"
    );
    // Two parallel lines 6 apart are both kept; a twin 2 apart is not.
    let e = edges(
        200,
        100,
        &[
            (10..190, 30..31, ACROSS),
            (10..190, 36..37, ACROSS),
            (10..190, 38..39, ACROSS),
        ],
    );
    assert_eq!(e.lines().len(), 2);
    // Votes across angle 0 = π: steep line leaning either way.
    let lean = |a: f32| {
        let mut marks = Vec::new();
        for k in 0..80usize {
            let x = (100.0 + (k as f32 - 40.0) * a.tan()).round() as usize;
            marks.push((x..x + 1, 10 + k..11 + k, (PI - a).rem_euclid(PI)));
        }
        edges(200, 100, &marks).lines()
    };
    for a in [0.02f32, -0.02] {
        let lines = lean(a);
        assert_eq!(lines.len(), 1, "{a}: {lines:?}");
        assert!(lines[0].off((100.5, 50.5)).abs() < 0.6, "{a}: {lines:?}");
    }
}

/// Votes needed grow with the picture: 4 % of its long side, at least 8.
#[test]
fn hough_needs_more_votes_on_larger_pictures() {
    let line = |len: usize| edges(300, 60, &[(100..100 + len, 30..31, ACROSS)]).lines().len();
    assert_eq!(line(10), 0);
    assert_eq!(line(12), 1);
}

#[test]
fn settle_fits_the_edge_pixels_along_the_whole_line() {
    let coarse = Line {
        n: (0.0, 1.0),
        rho: 11.0,
    };
    // Edges only near the far end of the line.
    let line = edges(100, 20, &[(80..96, 10..11, ACROSS)])
        .settle(coarse)
        .unwrap();
    assert!(
        (line.rho - 10.5).abs() < 1e-3 && near(line.n, (0.0, 1.0)),
        "{line:?}"
    );
    assert!(edges(100, 20, &[(80..88, 10..11, ACROSS)])
        .settle(coarse)
        .is_some());
    assert_eq!(
        edges(100, 20, &[(80..87, 10..11, ACROSS)]).settle(coarse),
        None,
        "7 points"
    );
}

#[test]
fn many_lines_keep_the_strongest() {
    // Columns 6 apart, each shorter than the one before.
    let marks: Vec<_> = (0..60)
        .map(|k| (6 * k + 3..6 * k + 4, 0..(190 - 2 * k), 0.0))
        .collect();
    let lines = edges(400, 200, &marks).lines();
    assert_eq!(lines.len(), MAX_LINES);
    for k in 0..10 {
        let x = 6.0 * k as f32 + 3.5;
        assert!(
            lines[..12]
                .iter()
                .any(|l| l.off((x, 50.0)).abs() < 0.1 && angle_gap(l.angle(), 0.0) < 1e-3),
            "column {k}: {lines:?}"
        );
    }
}

#[test]
fn sample_blends_neighbours() {
    let img = RgbImage::from_fn(2, 2, |x, y| Rgb([(x * 100) as u8, (y * 200) as u8, 50]));
    assert_eq!(sample(&img, (1.0, 1.0)), [50.0, 100.0, 50.0]);
    assert_eq!(sample(&img, (0.5, 1.5)), [0.0, 200.0, 50.0]);
    assert_eq!(sample(&img, (-3.0, 9.0)), [0.0, 200.0, 50.0]);
    assert_eq!(sample(&img, (1.25, 0.5)), [75.0, 0.0, 50.0]);
}

#[test]
fn refine_moves_onto_the_outermost_clear_edge() {
    // Page edge at x = 100 (half as sharp), print line at x 106..111 (sharp), table left.
    let mut img = RgbImage::from_fn(200, 100, |x, _| Rgb(if x < 100 { [110; 3] } else { [210; 3] }));
    fill(&mut img, 106..111, 0..100, [20, 20, 20]);
    let line = refine(&img, (105.0, 10.0), (105.0, 90.0), (150.0, 50.0), 8.0).unwrap();
    assert!(line.off((100.0, 50.0)).abs() < 0.5, "{line:?}");
    // Facing the other way round the side, outermost means the print's far edge.
    let line = refine(&img, (105.0, 90.0), (105.0, 10.0), (50.0, 50.0), 8.0).unwrap();
    assert!(line.off((111.0, 50.0)).abs() < 0.6, "{line:?}");
    // A much fainter edge outside loses to the clear one.
    let mut img = RgbImage::from_fn(200, 100, |x, _| Rgb(if x < 100 { [200; 3] } else { [205; 3] }));
    fill(&mut img, 106..200, 0..100, [120, 120, 120]);
    let line = refine(&img, (105.0, 10.0), (105.0, 90.0), (150.0, 50.0), 8.0).unwrap();
    assert!(line.off((106.0, 50.0)).abs() < 0.6, "{line:?}");
}

#[test]
fn refine_drops_stray_points_and_gives_up_on_noise() {
    // Straight edge at x = 100 with a dark blot crossing it at one probe.
    let mut img = RgbImage::from_fn(200, 100, |x, _| Rgb(if x < 100 { [60; 3] } else { [220; 3] }));
    fill(&mut img, 93..97, 40..44, [220, 220, 220]);
    let line = refine(&img, (102.0, 10.0), (102.0, 90.0), (150.0, 50.0), 6.0).unwrap();
    assert!(
        angle_gap(line.angle(), 0.0) < 0.002 && line.off((100.0, 50.0)).abs() < 0.2,
        "{line:?}"
    );
    let flat = RgbImage::from_pixel(200, 100, Rgb([128, 128, 128]));
    assert_eq!(
        refine(&flat, (102.0, 10.0), (102.0, 90.0), (150.0, 50.0), 6.0),
        None
    );
}
