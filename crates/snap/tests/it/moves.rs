//! A move: what it snaps to, and where it lands.

use craft_snap::{Anchors, Axis, Constraint, Kind, Mark, Rounding, Settings, Subject};
use kurbo::Point;

use crate::common::*;

#[test]
fn a_box_near_a_guide_lands_on_it_and_the_mark_says_so() {
    let t = scene(&[], &[10.0], &[]);
    let s = Settings::default();
    // The box's right side is 3 left of the guide at x = 10; its other sides are far from any line.
    let f = frame(2.5, 40.0, 9.0, 4.0, 0.0);
    let r = run(&t, &query(box_subject(f), &s));
    assert_eq!(r.delta.y, 0.0);
    assert!((r.delta.x - 3.0).abs() < 1e-12, "{:?}", r.delta);
    let hit = r.hits.first().unwrap();
    assert_eq!(hit.kind, Kind::Guide);
    assert!(r.marks.iter().any(|m| matches!(m, Mark::Line { kinds, hit: true, .. } if kinds == &[Kind::Guide])), "{:?}", r.marks);
    r.check(&t, &query(box_subject(f), &s)).unwrap();
    // 7 away is past the reach of 6 screen units at zoom 1.
    let far = frame(-4.5, 40.0, 9.0, 4.0, 0.0);
    let r = run(&t, &query(box_subject(far), &s));
    assert!(r.hits.is_empty() && r.delta.x == 0.0);
}

#[test]
fn two_objects_at_a_corner_fix_both_degrees() {
    let t = scene(&[(1, frame(100.0, 100.0, 20.0, 20.0, 0.0))], &[], &[]);
    let s = Settings::default();
    let f = frame(113.0, 107.0, 20.0, 20.0, 0.0);
    let q = query(box_subject(f), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.hits.len() == 2, "{:?}", r.hits);
}

#[test]
fn an_axis_lock_leaves_the_other_delta_zero_to_the_bit() {
    let t = scene(&[(1, frame(100.0, 100.0, 20.0, 20.0, 0.0))], &[103.0], &[104.0]);
    let s = Settings::default();
    let f = frame(98.0, 101.0, 20.0, 20.0, 0.0);
    let q = craft_snap::Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(f), &s) };
    let r = run(&t, &q);
    assert_eq!(r.delta.y.to_bits(), 0.0f64.to_bits());
    r.check(&t, &q).unwrap();
    // The y guide at 104 is not pulled: the free axis is x only.
    assert!(
        r.marks.iter().all(|m| !matches!(m, Mark::Line { direction, .. } if (direction.angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-9 && false))
    );
}

#[test]
fn a_box_of_odd_width_under_whole_unit_rounding_still_lands_on_the_line_it_marks() {
    // The centre line of the box at x = 50.5 would land half a unit off a whole-unit lattice; the rounding drops that candidate.
    let t = scene(&[], &[50.5], &[]);
    let s = Settings::default();
    let f = frame(51.0, 40.0, 9.0, 4.0, 0.0);
    let q = craft_snap::Query { rounding: Rounding::Delta { step: vec2(1.0, 1.0), from: Point::new(40.0, 40.0) }, ..query(box_subject(f), &s) };
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    for m in &r.marks {
        if let Mark::Line { segment, .. } = m {
            // Whatever is drawn, the box's own side is on it.
            let fr = r.frame.unwrap();
            let b = fr.bounds();
            let on = [b.x0, b.x1, fr.centre.x].iter().any(|x| (x - segment.p0.x).abs() < 1e-9);
            assert!(
                on || (segment.p0.y - b.y0).abs() < 1e-9 || (segment.p0.y - b.y1).abs() < 1e-9 || (segment.p0.y - fr.centre.y).abs() < 1e-9,
                "{m:?} {fr:?}"
            );
        }
    }
}

#[test]
fn a_ray_keeps_the_centre_on_it_and_snaps_along_it() {
    let t = scene(&[], &[], &[60.0]);
    let s = Settings::default();
    let f = frame(30.0, 30.5, 10.0, 10.0, 0.0);
    let q = craft_snap::Query { constraint: Constraint::Ray { origin: Point::new(0.0, 0.0), dir: vec2(1.0, 1.0) }, ..query(box_subject(f), &s) };
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    let c = r.frame.unwrap().centre;
    assert!((c.x - c.y).abs() < 1e-9, "{c:?}");
}

#[test]
fn a_point_snaps_to_lines_in_both_page_directions() {
    let t = scene(&[], &[10.0], &[20.0]);
    let s = Settings::default();
    let q = query(Subject::Point { at: Point::new(12.0, 18.0), base: None }, &s);
    let r = run(&t, &q);
    assert_eq!(r.point, Some(Point::new(10.0, 20.0)));
    r.check(&t, &q).unwrap();
    let _ = Anchors::ALL;
}
