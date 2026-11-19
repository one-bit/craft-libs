//! A subject that travels along one page axis (a move held to it, or a nudge) meets the places on the lines its own places travel along.

use craft_snap::{
    Anchors, Axis, Constraint, Detail, Id, Kind, Limits, Mark, NudgeReach, Object, Parts, Query, Rounding, Settings, Snapped, Spend, Subject,
    Targets, TargetsBuilder, TravelPlaces, TurnedBox,
};
use kurbo::{BezPath, Point};

use crate::common::*;

/// Targets with places of the app's own (id, x, y; the id is also the place's number) and objects that offer only their corners and side middles.
pub fn targets(points: &[(u64, f64, f64)], objects: &[(u64, TurnedBox)]) -> Targets {
    let mut b = TargetsBuilder::new(Limits::default());
    for (i, x, y) in points {
        b.point(Some(Id(*i)), Point::new(*x, *y), Kind::Point(0), *i as u32).unwrap();
    }
    for (id, f) in objects {
        let parts = Parts { corners: true, side_middles: true, detail: true, ..Parts::NONE };
        b.object(Id(*id), Object { frame: *f, bounds: None, angle: None, parts }).unwrap();
    }
    b.build()
}

/// Places pull a held move and stop a nudge; nothing else is on that could pull the subject off its line.
pub fn on_line() -> Settings {
    Settings { locked_places: TravelPlaces::OnLine, nudge_places: TravelPlaces::OnLine, ..Settings::default() }
}

fn held(t: &Targets, s: &Settings, f: TurnedBox, axis: Axis) -> Snapped {
    let q = Query { constraint: Constraint::Axis(axis), ..query(box_subject(f), s) };
    let r = run(t, &q);
    r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
    r
}

fn place_marks(r: &Snapped) -> Vec<(Kind, bool)> {
    r.marks.iter().filter_map(|m| if let Mark::Place { kind, hit, .. } = m { Some((*kind, *hit)) } else { None }).collect()
}

mod held_move {
    use super::*;

    /// A box 4 wide and 2 high at (0, 5): its places are on the lines y = 4, 5 and 6.
    fn subject() -> TurnedBox {
        frame(0.0, 5.0, 4.0, 2.0, 0.0)
    }

    #[test]
    fn a_place_on_the_line_of_travel_pulls_a_held_move_and_the_subject_lands_on_it() {
        let t = targets(&[(9, 6.0, 5.0)], &[]);
        let r = held(&t, &on_line(), subject(), Axis::X);
        // The right middle (2, 5) is the nearest of the box's places on y = 5: 4 from the place.
        assert_eq!(r.delta.x, 4.0);
        assert_eq!(r.delta.y.to_bits(), 0.0f64.to_bits(), "the other delta is 0 to the bit");
        assert_eq!(r.frame.unwrap().centre, Point::new(4.0, 5.0));
        let h = r.hits.first().unwrap();
        assert_eq!((h.kind, h.id, h.number, h.distance), (Kind::Point(0), Some(Id(9)), Some(9), 4.0));
        assert_eq!(place_marks(&r), vec![(Kind::Point(0), true)]);
        // The same move with the lock on the other axis does not meet it: the place is not on a line the box travels along.
        let across = held(&t, &on_line(), subject(), Axis::Y);
        assert!(across.hits.is_empty() && across.delta.x == 0.0 && across.delta.y == 0.0, "{across:?}");
    }

    #[test]
    fn a_place_off_the_line_is_not_a_candidate_and_one_within_the_same_place_rule_is() {
        let s = on_line();
        // 1e-3 off the line y = 5: not on it. (The unheld move would take the place, which is 4 away along x and 1e-3 across.)
        let off = held(&targets(&[(9, 6.0, 5.001)], &[]), &s, subject(), Axis::X);
        assert!(off.hits.is_empty() && off.delta.x == 0.0 && place_marks(&off).is_empty(), "{off:?}");
        // 5e-10 off: within the same-place rule (1e-9), on the line.
        let near = held(&targets(&[(9, 6.0, 5.0 + 5.0e-10)], &[]), &s, subject(), Axis::X);
        assert_eq!(near.delta.x, 4.0);
        assert_eq!(near.delta.y.to_bits(), 0.0f64.to_bits());
        // 3e-9 off: past it.
        let past = held(&targets(&[(9, 6.0, 5.0 + 3.0e-9)], &[]), &s, subject(), Axis::X);
        assert!(past.hits.is_empty(), "{past:?}");
    }

    #[test]
    fn a_place_pulls_within_the_reach_of_its_kind_along_the_line_and_either_way() {
        let s = on_line();
        // The reach is 6 screen units: at zoom 1 a place 6 from the box's side pulls, one 6.5 does not.
        let at = |x: f64| held(&targets(&[(9, x, 5.0)], &[]), &s, subject(), Axis::X);
        assert_eq!(at(2.0 + 6.0).delta.x, 6.0, "the boundary is inclusive");
        assert!(at(2.0 + 6.5).hits.is_empty());
        // Behind: the left middle (-2, 5) goes to a place at -8.
        assert_eq!(at(-2.0 - 6.0).delta.x, -6.0);
        // At zoom 2 the same reach is 3 document units.
        let t = targets(&[(9, 2.0 + 4.0, 5.0)], &[]);
        let q = Query { zoom: 2.0, constraint: Constraint::Axis(Axis::X), ..query(box_subject(subject()), &s) };
        assert!(run(&t, &q).hits.is_empty());
        let q = Query { zoom: 0.5, constraint: Constraint::Axis(Axis::X), ..query(box_subject(subject()), &s) };
        assert_eq!(run(&t, &q).delta.x, 4.0);
    }

    #[test]
    fn each_line_of_the_box_has_its_places_and_the_axis_may_be_y() {
        let s = on_line();
        // The right-hand corners (2, 4) and (2, 6) travel along y = 4 and y = 6.
        let r = held(&targets(&[(9, 7.0, 4.0)], &[]), &s, subject(), Axis::X);
        assert_eq!((r.delta.x, r.hits.len()), (5.0, 1));
        let r = held(&targets(&[(9, -7.0, 6.0)], &[]), &s, subject(), Axis::X);
        assert_eq!(r.delta.x, -5.0);
        // Held to y: the lines the box's places travel along are x = -2, 0 and 2. The top middle (0, 6) goes up to a place at (0, 10), 4 away.
        let r = held(&targets(&[(9, 0.0, 10.0)], &[]), &s, subject(), Axis::Y);
        assert_eq!((r.delta.y, r.delta.x), (4.0, 0.0));
        // The bottom right corner (2, 4) goes down to (2, -0.5), 4.5 away (the other two places on x = 2 are past the reach).
        let r = held(&targets(&[(9, 2.0, -0.5)], &[]), &s, subject(), Axis::Y);
        assert_eq!(r.delta.y, -4.5);
        assert_eq!(r.delta.x.to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn the_setting_off_leaves_a_held_move_as_it_was_and_a_place_beats_a_nearer_line() {
        let t = targets(&[(9, 6.0, 5.0)], &[]);
        let off = held(&t, &Settings::default(), subject(), Axis::X);
        assert!(off.hits.is_empty() && off.delta.x == 0.0, "places did not pull a held move, and still do not by default: {off:?}");
        // A guide at x = 3 is 1 from the box's right side, and the place 4 away; the place is taken (a place takes the degree before any line) when on.
        let mut b = TargetsBuilder::new(Limits::default());
        b.point(Some(Id(9)), Point::new(6.0, 5.0), Kind::Point(0), 9).unwrap();
        b.line(Some(Id(1)), Axis::X, 3.0, None, Kind::Guide).unwrap();
        let t = b.build();
        let guide = held(&t, &Settings::default(), subject(), Axis::X);
        assert_eq!((guide.delta.x, guide.hits[0].kind), (1.0, Kind::Guide));
        let place = held(&t, &on_line(), subject(), Axis::X);
        assert_eq!((place.delta.x, place.hits[0].kind), (4.0, Kind::Point(0)));
        for places in [craft_snap::Places::First, craft_snap::Places::Compete { bias: 0.25 }] {
            let s = Settings { places, ..on_line() };
            assert_eq!(held(&t, &s, subject(), Axis::X).delta.x, 4.0, "{places:?}: one line never beats a place");
        }
        // With the place out of reach the box is not pulled by it.
        let far = targets(&[(9, 20.0, 5.0)], &[]);
        assert!(held(&far, &on_line(), subject(), Axis::X).hits.is_empty());
    }

    #[test]
    fn a_rounding_that_takes_the_subject_off_the_place_drops_it() {
        let s = on_line();
        let q = |rounding| Query { constraint: Constraint::Axis(Axis::X), rounding, ..query(box_subject(subject()), &s) };
        let whole = Rounding::Delta { step: vec2(1.0, 1.0), from: Point::new(0.0, 5.0) };
        // The place is 3.7 away; a whole-unit distance moved cannot land on it.
        let t = targets(&[(9, 5.7, 5.0)], &[]);
        let r = run(&t, &q(whole));
        assert!(r.hits.is_empty() && r.delta.x == 0.0, "{r:?}");
        r.check(&t, &q(whole)).unwrap();
        // 4.0 away is a whole unit: taken.
        let t = targets(&[(9, 6.0, 5.0)], &[]);
        let r = run(&t, &q(whole));
        assert_eq!(r.delta.x, 4.0);
        r.check(&t, &q(whole)).unwrap();
    }

    #[test]
    fn a_kind_that_is_off_does_not_pull_and_a_priority_decides_between_places_that_are_as_near() {
        let t = targets(&[(9, 6.0, 5.0)], &[]);
        let mut s = on_line();
        let mut rule = s.kinds.rule(Kind::Point(0));
        rule.on = false;
        s.kinds.set(Kind::Point(0), rule);
        assert!(held(&t, &s, subject(), Axis::X).hits.is_empty());
        // Two places equally far, a corner of another box and a place of the app's: the lower priority number wins.
        let t = targets(&[(9, 6.0, 5.0)], &[(1, frame(8.0, 5.0, 4.0, 2.0, 0.0))]);
        let mut s = on_line();
        let mut rule = s.kinds.rule(Kind::SideMiddle);
        rule.priority = 0;
        s.kinds.set(Kind::SideMiddle, rule);
        // The box's left middle (6, 5) is on the same place as the app's point: both at 4.
        let r = held(&t, &s, subject(), Axis::X);
        assert_eq!((r.delta.x, r.hits[0].kind), (4.0, Kind::SideMiddle));
        // The subject lands on the app's place and on the box's left middle, and its corners on the box's: each is marked, and only the middle is the hit.
        let marks = place_marks(&r);
        assert_eq!(marks.len(), 4, "{:?}", r.marks);
        assert!(
            marks.contains(&(Kind::SideMiddle, true)) && marks.contains(&(Kind::Point(0), false)) && marks.contains(&(Kind::Corner, false)),
            "{marks:?}"
        );
    }

    #[test]
    fn the_places_that_move_with_the_box_and_a_turned_box_travel_along_their_own_lines() {
        let s = on_line();
        let t = targets(&[(9, 13.0, 8.0)], &[]);
        let f = subject();
        let own = [Point::new(10.0, 8.0)];
        let q = Query {
            constraint: Constraint::Axis(Axis::X),
            ..query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &own }, &s)
        };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!((r.delta.x, r.hits[0].number), (3.0, Some(9)), "{r:?}");
        // A box turned by 0.5 radians has nine places on nine different lines; a place on the line of its corner is met by that corner.
        let turned = frame(0.0, 5.0, 4.0, 2.0, 0.5);
        let corner = turned.corners()[2];
        let t = targets(&[(9, corner.x + 4.5, corner.y)], &[]);
        let r = held(&t, &s, turned, Axis::X);
        assert!((r.delta.x - 4.5).abs() < 1e-12 && r.delta.y.to_bits() == 0.0f64.to_bits(), "{r:?}");
        assert_eq!(r.hits.len(), 1);
        // And a place on a line none of them travels along is not.
        let t = targets(&[(9, corner.x + 4.5, corner.y + 0.01)], &[]);
        assert!(held(&t, &s, turned, Axis::X).hits.is_empty());
    }

    #[test]
    fn a_point_held_to_an_axis_meets_the_places_on_its_line() {
        let s = on_line();
        let t = targets(&[(9, 3.0, 5.0), (8, 1.0, 5.5)], &[]);
        let q = Query { constraint: Constraint::Axis(Axis::X), ..query(Subject::Point { at: Point::new(0.0, 5.0), base: None }, &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!((r.point, r.delta.y.to_bits()), (Some(Point::new(3.0, 5.0)), 0.0f64.to_bits()));
    }

    #[test]
    fn the_corners_and_side_middles_of_other_boxes_and_the_points_a_detail_source_gives_are_places_on_the_line() {
        let s = on_line();
        // A box 8 by 6 at (11, 10): its left corners and middle are at x = 7. The subject's right side (x = 2) is 5 away.
        let t = targets(&[], &[(1, frame(11.0, 10.0, 8.0, 6.0, 0.0))]);
        let r = held(&t, &s, frame(0.0, 10.0, 4.0, 6.0, 0.0), Axis::X);
        assert_eq!(r.delta.x, 5.0);
        assert!(r.hits.iter().all(|h| matches!(h.kind, Kind::Corner | Kind::SideMiddle)), "{:?}", r.hits);
        // A source gives an object a point on the line y = 10 that no box place is on.
        struct Pins;
        impl Detail for Pins {
            fn points(&mut self, id: Id, _spend: &mut Spend<'_>, out: &mut Vec<(Point, Kind, u32)>) {
                if id == Id(1) {
                    out.push((Point::new(5.5, 10.0), Kind::Point(0), 4));
                    out.push((Point::new(5.5, 10.25), Kind::Point(0), 5));
                }
            }
            fn outline(&mut self, _id: Id, _spend: &mut Spend<'_>, _out: &mut Vec<BezPath>) {}
        }
        let q = Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(frame(0.0, 10.0, 4.0, 6.0, 0.0)), &s) };
        let r = craft_snap::snap(&t, &q, &mut Pins);
        r.check(&t, &q).unwrap();
        assert_eq!((r.delta.x, r.hits[0].number), (3.5, Some(4)), "the pin on the box's middle line is 3.5 from its right middle: {r:?}");
        assert!(r.work.outlines >= 1);
    }

    #[test]
    fn the_crossings_and_vertices_of_outlines_are_places_on_the_line() {
        let s = on_line();
        // An outline of two lines crossing at (6, 5) and a vertex at (6, 3) off the line: the subject's right middle meets the crossing.
        let mut b = TargetsBuilder::new(Limits::default());
        b.object(Id(1), Object { frame: frame(6.0, 5.0, 1.0, 10.0, 0.0), bounds: None, angle: None, parts: Parts { detail: true, ..Parts::NONE } })
            .unwrap();
        let mut a = BezPath::new();
        a.move_to((4.0, 3.0));
        a.line_to((8.0, 7.0));
        let mut c = BezPath::new();
        c.move_to((4.0, 7.0));
        c.line_to((8.0, 3.0));
        b.path(Id(1), a).unwrap();
        b.path(Id(1), c).unwrap();
        let t = b.build();
        let r = held(&t, &s, subject(), Axis::X);
        assert_eq!(r.delta.x, 4.0);
        assert_eq!(r.hits[0].kind, Kind::Intersection);
    }
}
