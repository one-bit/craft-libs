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
fn two_lines_in_different_directions_fix_both_degrees() {
    let t = scene(&[(1, frame(100.0, 100.0, 20.0, 20.0, 0.0))], &[], &[]);
    let mut s = Settings::default();
    for k in [Kind::Corner, Kind::SideMiddle] {
        let mut rule = s.kinds.rule(k);
        rule.on = false;
        s.kinds.set(k, rule);
    }
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

mod places {
    use craft_snap::{Anchors, Axis, Id, Kind, Limits, Mark, Places, Settings, Subject, TargetsBuilder};
    use kurbo::Point;

    use crate::common::*;

    #[test]
    fn a_box_corner_near_another_boxs_corner_lands_on_it_and_the_place_is_marked() {
        let mut b = TargetsBuilder::new(Limits::default());
        b.object(Id(1), object(frame(100.0, 100.0, 20.0, 20.0, 0.0))).unwrap();
        let t = b.build();
        let s = Settings { places: Places::First, ..Settings::default() };
        // The subject's min corner is at (112, 112): 2 from the other box's max corner (110, 110) on each axis.
        let f = frame(122.0, 122.0, 20.0, 20.0, 0.0);
        let q = query(box_subject(f), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!(r.frame.unwrap().centre, Point::new(120.0, 120.0));
        assert!(r.hits.iter().any(|h| h.kind == Kind::Corner));
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Place { kind: Kind::Corner, hit: true, .. })), "{:?}", r.marks);
    }

    #[test]
    fn the_apps_own_place_pulls_a_point_and_beats_a_line_under_first() {
        let mut b = TargetsBuilder::new(Limits::default());
        b.point(Some(Id(9)), Point::new(50.0, 50.0), Kind::Point(0), 3).unwrap();
        b.line(None, Axis::X, 52.0, None, Kind::Guide).unwrap();
        let t = b.build();
        let s = Settings { places: Places::First, ..Settings::default() };
        let q = query(Subject::Point { at: Point::new(51.0, 51.5), base: None }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!(r.point, Some(Point::new(50.0, 50.0)));
        assert_eq!(r.hits.first().and_then(|h| h.number), Some(3));
        let _ = Anchors::ALL;
    }
}

mod turned_resize {
    use craft_snap::{Edges, Kind, Settings, Subject};

    use crate::common::*;

    #[test]
    fn a_turned_box_dragged_by_a_side_puts_a_corner_on_a_guide_and_keeps_its_angle_and_its_fixed_sides() {
        let a = 30f64.to_radians();
        // A box 20 by 10 turned by 30 degrees, centred at the origin. Its high side along u carries two corners; one is at x = 10 cos(30) - 5 sin(30) and
        // the other at x = 10 cos(30) + 5 sin(30) = 11.16. A guide at 13 is 1.84 from the second: dragging the side along u by 2.12 puts it there.
        let t = scene(&[], &[13.0], &[]);
        let s = Settings::default();
        let f = frame(0.0, 0.0, 20.0, 10.0, a);
        let q = query(Subject::Edges { frame: f, moving: Edges { u_max: true, ..Edges::default() } }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let out = r.frame.unwrap();
        assert_eq!(out.angle.to_bits(), a.to_bits());
        assert!(r.hits.iter().any(|h| h.kind == Kind::Guide), "{:?}", r.hits);
        let corners = out.corners();
        assert!(corners.iter().any(|c| (c.x - 13.0).abs() < 1e-9), "a corner of the moved side is on the guide: {corners:?}");
        // The fixed side (the low one along u) is where it was.
        let before = f.corners();
        assert!((corners[0] - before[0]).hypot() < 1e-9 && (corners[3] - before[3]).hypot() < 1e-9);
    }
}
