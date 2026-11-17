// Label placement ported from FigCraft: storytold/figcraft at 4548d6c00953dd051ee36af3bada376d1a80d05c,
// crates/ui-egui/src/canvas/measure_overlay.rs (`place_labels`).
// Copyright (c) 2026 ArtCraft Team and the FigCraft contributors. MIT OR Apache-2.0.
// Changed: from egui types to kurbo, in f64, and bounded (labels, spots and boxes to avoid are capped).

//! Overlap-free placement of the labels of gap, size and angle marks, in the app's screen units.
//!
//! A label goes where it is tried first: centred on its line when the line is long enough to show round it; else slid along the line, both
//! ways; then beside it, on one side and then the other, farther out each time, until it clears every box to avoid and every label placed
//! before it. A label that finds no spot is stacked below everything placed.
//!
//! The crate does not format numbers: the app formats each value in its own unit and measures the text.

use kurbo::{Point, Rect, Size, Vec2};

/// The clear space kept round a label, in the app's screen units.
pub const GAP: f64 = 4.0;

/// The most labels placed in one call; lines past it get none.
pub const MAX_LABELS: usize = 32;

/// The most spots tried for one label.
pub const MAX_SPOTS: usize = 96;

/// The most boxes to avoid that are read.
pub const MAX_AVOID: usize = 64;

/// How many decimal places a value is shown with by default.
pub const DEFAULT_PLACES: u8 = 3;

/// The most decimal places [`format`] writes.
pub const MAX_PLACES: u8 = 12;

/// A length as a label shows it: `places` decimal places (at most [`MAX_PLACES`]), none trimmed, so that a column of labels lines up. A value
/// that is not a finite number gives an empty label; a negative zero is written as zero. The app converts to its own unit first.
pub fn format(value: f64, places: u8) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let places = usize::from(places.min(MAX_PLACES));
    let s = std::format!("{value:.places$}");
    // A value that rounds to nothing has no sign.
    if s.starts_with('-') && s.chars().all(|c| matches!(c, '-' | '0' | '.')) { s.trim_start_matches('-').to_string() } else { s }
}

/// A line to label: its two ends and the size of its label.
pub type Labelled = (Point, Point, Size);

fn grown(r: Rect, d: f64) -> Rect {
    Rect::new(r.x0 - d, r.y0 - d, r.x1 + d, r.y1 + d)
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

fn centred(c: Point, size: Size) -> Rect {
    Rect::new(c.x - size.width / 2.0, c.y - size.height / 2.0, c.x + size.width / 2.0, c.y + size.height / 2.0)
}

/// The spots to try for one label, in order, at most [`MAX_SPOTS`].
fn spots(a: Point, b: Point, size: Size, gap: f64) -> Vec<Point> {
    let mid = a.midpoint(b);
    let d = b - a;
    let horizontal = d.x.abs() >= d.y.abs();
    let (dir, out) = if horizontal { (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)) } else { (Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)) };
    let (along, across) = if horizontal { (size.width, size.height) } else { (size.height, size.width) };
    let room = (d.hypot() - along) / 2.0 - gap;
    let mut centres: Vec<Point> = Vec::new();
    if room >= 0.0 {
        centres.push(mid);
        let mut n = 1.0;
        while n * (along + gap) <= room && centres.len() < 32 {
            centres.extend([mid + dir * (n * (along + gap)), mid - dir * (n * (along + gap))]);
            n += 1.0;
        }
    }
    for n in 0..32 {
        let off = across / 2.0 + gap + f64::from(n) * (across + gap);
        centres.extend([mid + out * off, mid - out * off]);
    }
    centres.truncate(MAX_SPOTS);
    centres
}

/// Puts a label box beside each line, in the app's screen units. Lines may run in any direction. At most [`MAX_LABELS`] lines are labelled
/// and [`MAX_AVOID`] boxes avoided, so a call tests at most `32 × 96 × 96` pairs of boxes. A line that is not finite gets no label.
pub fn place_with(lines: &[Labelled], avoid: &[Rect], gap: f64) -> Vec<Rect> {
    let mut placed: Vec<Rect> =
        avoid.iter().take(MAX_AVOID).copied().filter(|r| r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite()).collect();
    let gap = if gap.is_finite() && gap >= 0.0 { gap } else { GAP };
    let mut out = Vec::new();
    for (a, b, size) in lines.iter().take(MAX_LABELS) {
        if !(a.is_finite() && b.is_finite() && size.width.is_finite() && size.height.is_finite() && size.width >= 0.0 && size.height >= 0.0) {
            continue;
        }
        let free = |r: &Rect| !placed.iter().any(|p| overlaps(&grown(*p, gap / 2.0), r));
        let spot = spots(*a, *b, *size, gap).into_iter().map(|c| centred(c, *size)).find(free);
        let r = spot.unwrap_or_else(|| {
            let mid = a.midpoint(*b);
            let bottom = placed.iter().map(|p| p.y1).fold(mid.y, f64::max);
            Rect::new(mid.x - size.width / 2.0, bottom + gap, mid.x + size.width / 2.0, bottom + gap + size.height)
        });
        placed.push(r);
        out.push(r);
    }
    out
}

/// [`place_with`] with the default clear space, [`GAP`].
pub fn place(lines: &[Labelled], avoid: &[Rect]) -> Vec<Rect> {
    place_with(lines, avoid, GAP)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size::new(24.0, 14.0);

    fn no_overlaps(r: &[Rect]) -> bool {
        r.iter().enumerate().all(|(i, a)| r.iter().skip(i + 1).all(|b| !overlaps(a, b)))
    }

    #[test]
    fn values_are_shown_to_three_places_by_default_and_never_hostile() {
        assert_eq!(format(1.25, DEFAULT_PLACES), "1.250");
        assert_eq!(format(2.0, 0), "2");
        assert_eq!(format(-0.0004, 3), "0.000");
        assert_eq!(format(-1.5, 1), "-1.5");
        assert_eq!(format(f64::NAN, 3), "");
        assert_eq!(format(f64::INFINITY, 3), "");
        assert_eq!(format(0.5, 200).len(), 2 + usize::from(MAX_PLACES));
        assert_eq!(crate::Settings::default().label_places, 3);
        assert_eq!(format(1.0e300, 3).len(), 301 + 4);
    }

    #[test]
    fn long_lines_carry_their_label_in_the_middle() {
        let r = place(&[(Point::new(0.0, 50.0), Point::new(200.0, 50.0), SIZE), (Point::new(50.0, 0.0), Point::new(50.0, 300.0), SIZE)], &[]);
        assert_eq!(r[0].center(), Point::new(100.0, 50.0));
        assert_eq!(r[1].center(), Point::new(50.0, 150.0));
    }

    #[test]
    fn short_lines_put_the_label_beside_them() {
        let r = place(&[(Point::new(0.0, 50.0), Point::new(10.0, 50.0), SIZE), (Point::new(50.0, 0.0), Point::new(50.0, 6.0), SIZE)], &[]);
        assert!(r[0].y0 > 50.0, "below a short horizontal line: {:?}", r[0]);
        assert!(r[1].x0 > 50.0, "right of a short vertical line: {:?}", r[1]);
    }

    #[test]
    fn labels_never_overlap_even_when_crowded() {
        let crowd: Vec<Labelled> =
            (0..100).map(|i| (Point::new(10.0, 10.0), Point::new(10.0 + f64::from(i % 2) * 0.5, 10.0 + f64::from(i % 3) * 0.5), SIZE)).collect();
        let r = place(&crowd, &[]);
        assert_eq!(r.len(), MAX_LABELS);
        assert!(no_overlaps(&r));
        let close: Vec<Labelled> = (0..6).map(|i| (Point::new(0.0, f64::from(i) * 3.0), Point::new(100.0, f64::from(i) * 3.0), SIZE)).collect();
        let r = place(&close, &[]);
        assert!(no_overlaps(&r));
        assert_eq!(r[0].center(), Point::new(50.0, 0.0), "the first keeps its place");
    }

    #[test]
    fn a_label_slides_off_a_box_to_avoid_and_hostile_input_costs_the_caps() {
        let line = (Point::new(0.0, 50.0), Point::new(200.0, 50.0), SIZE);
        let r = place(&[line], &[Rect::new(80.0, 40.0, 120.0, 60.0)]);
        assert!(!overlaps(&r[0], &Rect::new(80.0, 40.0, 120.0, 60.0)));
        let many: Vec<Rect> = (0..500).map(|i| Rect::new(f64::from(i), 0.0, f64::from(i) + 1.0, 1.0)).collect();
        assert_eq!(place(&[line], &many).len(), 1);
        let bad = place(&[(Point::new(f64::NAN, 0.0), Point::new(1.0, 1.0), SIZE), line], &[]);
        assert_eq!(bad.len(), 1, "a line that is not a number gets no label");
        assert!(spots(Point::new(0.0, 0.0), Point::new(1.0e9, 0.0), SIZE, GAP).len() <= MAX_SPOTS);
    }
}
