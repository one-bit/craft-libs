//! A subject that travels along one page axis (a move held to it, or a nudge) meets the places on the lines its own places travel along.

use craft_snap::{
    Anchors, Axis, Choose, Constraint, Detail, Id, Kind, Limits, Mark, NudgeReach, Object, Parts, Query, Rounding, Settings, Snapped, Spend, Subject,
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
        r.check_with_detail(&t, &q, &mut Pins).unwrap();
        assert_eq!((r.delta.x, r.hits[0].number), (3.5, Some(4)), "the pin on the box's middle line is 3.5 from its right middle: {r:?}");
        assert!(r.work.outlines >= 1);
    }

    #[test]
    fn places_of_no_reach_pull_nothing_not_even_from_where_the_subject_already_is() {
        // A place exactly under the box's right middle, every kind of place with no reach: no window is read, and nothing pulls, held or free.
        let t = targets(&[(9, 2.0, 5.0)], &[]);
        let mut s = on_line();
        for k in [Kind::Point(0), Kind::Corner, Kind::SideMiddle, Kind::Vertex, Kind::Midpoint, Kind::Nearest, Kind::Intersection] {
            s.kinds.set(k, craft_snap::KindRule { on: true, reach: craft_snap::Reach::Screen(0.0), priority: 1, scope: craft_snap::Scope::All });
        }
        let r = held(&t, &s, subject(), Axis::X);
        assert!(r.hits.is_empty() && r.work.points == 0 && r.work.visited == 0, "{r:?}");
        let q = query(box_subject(subject()), &s);
        let free = run(&t, &q);
        assert!(free.hits.is_empty() && free.work.points == 0, "{free:?}");
        // With a reach that is more than nothing the same place is met, at no distance.
        let s = on_line();
        let r = held(&t, &s, subject(), Axis::X);
        assert_eq!((r.delta.x, r.hits.len(), r.hits[0].distance), (0.0, 1, 0.0));
    }

    #[test]
    fn a_vertex_wins_a_near_tie_by_fifteen_percent_of_its_reach_in_a_held_move_and_in_a_free_one() {
        // A place of the app 4 from the box's right middle, and the end of an outline `x - 2` from it, on the same line: the end wins by 15% of the reach of
        // 6 (0.9), so at 4.5 it does and at 5 it does not. The same for a move that is not held.
        let at = |x: f64| {
            let mut b = TargetsBuilder::new(Limits::default());
            b.point(Some(Id(9)), Point::new(6.0, 5.0), Kind::Point(0), 9).unwrap();
            let parts = Parts { detail: true, ..Parts::NONE };
            b.object(Id(1), Object { frame: frame(x + 0.75, 7.0, 1.5, 4.0, 0.0), bounds: None, angle: None, parts }).unwrap();
            let mut p = BezPath::new();
            p.move_to((x, 5.0));
            p.line_to((x + 1.5, 9.0));
            b.path(Id(1), p).unwrap();
            b.build()
        };
        let s = on_line();
        for (x, vertex_wins) in [(6.5, true), (7.0, false)] {
            let t = at(x);
            let r = held(&t, &s, subject(), Axis::X);
            let (want, kind) = if vertex_wins { (x - 2.0, Kind::Vertex) } else { (4.0, Kind::Point(0)) };
            assert_eq!((r.delta.x, r.hits[0].kind), (want, kind), "held, the end at {x}: {r:?}");
            let q = query(box_subject(subject()), &s);
            let free = run(&t, &q);
            free.check(&t, &q).unwrap();
            assert_eq!((free.delta, free.hits[0].kind), (vec2(want, 0.0), kind), "free, the end at {x}: {free:?}");
        }
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

    #[test]
    fn a_subject_wider_than_a_window_may_read_still_meets_the_places_past_its_corners() {
        // A box `w` wide and 10 high from x = 0: its right side is at `w`. A place 1 past it, on its top line, is within the reach (6) and pulls.
        for w in [40.0, 100.0, 400.0, 1000.0] {
            let f = frame(w / 2.0, 5.0, w, 10.0, 0.0);
            let t = targets(&[(9, w + 1.0, 10.0), (8, -30.0, 0.0)], &[]);
            let r = held(&t, &on_line(), f, Axis::X);
            assert_eq!((r.delta.x, r.hits.first().and_then(|h| h.number)), (1.0, Some(9)), "width {w}: {r:?}");
            // And one 2 before the left side, on the bottom line.
            let t = targets(&[(9, -2.0, 0.0), (8, w + 30.0, 10.0)], &[]);
            let r = held(&t, &on_line(), f, Axis::X);
            assert_eq!((r.delta.x, r.hits.first().and_then(|h| h.number)), (-2.0, Some(9)), "width {w}: {r:?}");
        }
    }
}

mod nudge {
    use craft_snap::{Cuts, Lattice, Reach};

    use super::*;

    /// A box 4 wide and 2 high at (0, 5): its places are on the lines y = 4, 5 and 6, and its sides at x = -2, 0 and 2.
    fn subject() -> TurnedBox {
        frame(0.0, 5.0, 4.0, 2.0, 0.0)
    }

    fn press(t: &Targets, s: &Settings, f: TurnedBox, axis: Axis, forward: bool, reach: NudgeReach, lats: &[Lattice]) -> Snapped {
        // With a step the fallback is the step, as it is for an app; with none it is half a unit.
        let fallback = if let NudgeReach::Within(step) = reach { step } else { 0.5 };
        let q = Query { lattices: lats, ..query(Subject::Nudge { frame: f, bounds: None, axis, forward, reach, fallback }, s) };
        let r = run(t, &q);
        r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        r
    }

    fn right(t: &Targets, s: &Settings) -> Snapped {
        press(t, s, subject(), Axis::X, true, NudgeReach::Anywhere, &[])
    }

    #[test]
    fn a_place_on_the_line_of_travel_stops_a_nudge_where_no_line_would() {
        let t = targets(&[(9, 13.0, 5.0)], &[]);
        let r = right(&t, &on_line());
        // The right middle (2, 5) goes to the place at (13, 5): 11.
        assert_eq!(r.delta.x, 11.0);
        assert_eq!(r.delta.y.to_bits(), 0.0f64.to_bits());
        let h = r.hits.first().unwrap();
        assert_eq!((h.kind, h.id, h.number, h.distance), (Kind::Point(0), Some(Id(9)), Some(9), 11.0));
        assert_eq!(place_marks(&r), vec![(Kind::Point(0), true)]);
        // By default places do not stop it: the fallback.
        let off = right(&t, &Settings::default());
        assert_eq!((off.delta.x, off.hits.len()), (0.5, 0));
        // Only the setting for a nudge stops it: a place pulls a held move without stopping a nudge.
        let held_only = Settings { locked_places: TravelPlaces::OnLine, ..Settings::default() };
        assert_eq!(right(&t, &held_only).delta.x, 0.5);
        let nudge_only = Settings { nudge_places: TravelPlaces::OnLine, ..Settings::default() };
        assert_eq!(right(&t, &nudge_only).delta.x, 11.0);
    }

    #[test]
    fn a_place_off_the_line_does_not_stop_it_and_one_within_the_same_place_rule_does() {
        let s = on_line();
        assert_eq!(right(&targets(&[(9, 13.0, 5.001)], &[]), &s).delta.x, 0.5);
        assert_eq!(right(&targets(&[(9, 13.0, 5.0 + 5.0e-10)], &[]), &s).delta.x, 11.0);
        assert_eq!(right(&targets(&[(9, 13.0, 5.0 + 3.0e-9)], &[]), &s).delta.x, 0.5);
        // On a line of the box (y = 4 or 6) the corner meets it.
        assert_eq!(right(&targets(&[(9, 13.0, 6.0)], &[]), &s).delta.x, 11.0);
        assert_eq!(right(&targets(&[(9, 13.0, 6.5)], &[]), &s).delta.x, 0.5, "between the box's lines of travel");
    }

    #[test]
    fn the_nearest_stop_wins_whatever_its_kind() {
        let s = on_line();
        let with = |guide_x: f64, place_x: f64| {
            let mut b = TargetsBuilder::new(Limits::default());
            b.point(Some(Id(9)), Point::new(place_x, 5.0), Kind::Point(0), 9).unwrap();
            b.line(Some(Id(1)), Axis::X, guide_x, None, Kind::Guide).unwrap();
            right(&b.build(), &s)
        };
        // The box's right side is at 2, its centre at 0, its left at -2. A guide at 9 and a place at 6: the place is nearer.
        let r = with(9.0, 6.0);
        assert_eq!((r.delta.x, r.hits[0].kind), (4.0, Kind::Point(0)));
        // A guide at 4 and a place at 6: the guide is nearer.
        let r = with(4.0, 6.0);
        assert_eq!((r.delta.x, r.hits[0].kind), (2.0, Kind::Guide));
        // The same distance, 4: the lower priority number wins, the place's 1 against the guide's 2.
        let r = with(6.0, 6.0);
        assert_eq!((r.delta.x, r.hits[0].kind), (4.0, Kind::Point(0)));
        // Give the guide the better priority and it keeps the tie; the same priority leaves the line the stop.
        for (priority, want) in [(0, Kind::Guide), (1, Kind::Guide), (2, Kind::Point(0))] {
            let mut s = on_line();
            let mut rule = s.kinds.rule(Kind::Guide);
            rule.priority = priority;
            s.kinds.set(Kind::Guide, rule);
            let mut b = TargetsBuilder::new(Limits::default());
            b.point(Some(Id(9)), Point::new(6.0, 5.0), Kind::Point(0), 9).unwrap();
            b.line(Some(Id(1)), Axis::X, 6.0, None, Kind::Guide).unwrap();
            let r = right(&b.build(), &s);
            assert_eq!((r.delta.x, r.hits[0].kind), (4.0, want), "guide priority {priority}");
        }
        // A grid line is a stop too: a place before it stops the nudge, and one beyond it does not.
        let grid = [Lattice { origin: Point::new(0.0, 0.0), pitch: vec2(3.0, 3.0), kind: Kind::Grid }];
        let before = press(&targets(&[(9, 2.5, 5.0)], &[]), &s, subject(), Axis::X, true, NudgeReach::Anywhere, &grid);
        assert_eq!((before.delta.x, before.hits[0].kind), (0.5, Kind::Point(0)), "the place is 0.5 from the right side, the grid line at 3 is 1");
        let beyond = press(&targets(&[(9, 3.5, 5.0)], &[]), &s, subject(), Axis::X, true, NudgeReach::Anywhere, &grid);
        assert_eq!((beyond.delta.x, beyond.hits[0].kind), (1.0, Kind::Grid), "the place at 3.5 is 1.5 away");
    }

    /// Under `Choose::Ranked` a move takes the better-ranked place at any distance, but a nudge stops at the first place ahead: the nearer place wins,
    /// whatever the priorities, and the priority only tells apart places as near as each other.
    #[test]
    fn under_ranked_a_nudge_still_stops_at_the_nearest_place_and_priority_only_breaks_a_tie() {
        // A box 2 by 2 at (1, 1): its right side is at 2 and its places are on the lines y = 0, 1 and 2.
        let f = frame(1.0, 1.0, 2.0, 2.0, 0.0);
        let ranked = |connection: u8, other: u8| {
            let mut s = Settings { choose: Choose::Ranked, ..on_line() };
            for (kind, priority) in [(Kind::Point(0), connection), (Kind::Point(1), other)] {
                s.kinds.set(kind, craft_snap::KindRule { on: true, reach: Reach::Screen(6.0), priority, scope: craft_snap::Scope::All });
            }
            s
        };
        let places = |near: f64, far: f64| {
            let mut b = TargetsBuilder::new(Limits::default());
            b.point(Some(Id(1)), Point::new(2.0 + near, 2.0), Kind::Point(1), 1).unwrap();
            b.point(Some(Id(2)), Point::new(2.0 + far, 2.0), Kind::Point(0), 2).unwrap();
            b.build()
        };
        // The connection point (priority 0) is 3 ahead and the other place (priority 1) is 1 ahead: the nearer one stops it, ranked or not.
        let t = places(1.0, 3.0);
        for choose in [Choose::Nearest, Choose::Ranked] {
            let s = Settings { choose, ..ranked(0, 1) };
            let r = press(&t, &s, f, Axis::X, true, NudgeReach::Anywhere, &[]);
            assert_eq!((r.delta.x, r.hits[0].kind, r.hits[0].number), (1.0, Kind::Point(1), Some(1)), "{choose:?}: {r:?}");
        }
        // Backward as well: the same two places a box's width off to the left.
        let mut b = TargetsBuilder::new(Limits::default());
        b.point(Some(Id(1)), Point::new(-1.0, 0.0), Kind::Point(1), 1).unwrap();
        b.point(Some(Id(2)), Point::new(-3.0, 0.0), Kind::Point(0), 2).unwrap();
        let back = press(&b.build(), &ranked(0, 1), f, Axis::X, false, NudgeReach::Anywhere, &[]);
        assert_eq!((back.delta.x, back.hits[0].number), (-1.0, Some(1)), "{back:?}");
        // As near as each other, within the same-place rule: the lower priority number wins, under both rules.
        let tie = places(2.0, 2.0 + 5.0e-10);
        for choose in [Choose::Nearest, Choose::Ranked] {
            for (connection, other, want) in [(0, 1, Kind::Point(0)), (1, 0, Kind::Point(1))] {
                let s = Settings { choose, ..ranked(connection, other) };
                let r = press(&tie, &s, f, Axis::X, true, NudgeReach::Anywhere, &[]);
                assert_eq!(r.hits[0].kind, want, "{choose:?} priorities {connection} and {other}: {r:?}");
                assert!((r.delta.x - 2.0).abs() < 1e-9, "{r:?}");
            }
        }
        // A step that reaches the farther place only does not make it the nearer.
        let r = press(&t, &ranked(0, 1), f, Axis::X, true, NudgeReach::Within(3.0), &[]);
        assert_eq!((r.delta.x, r.hits[0].number), (1.0, Some(1)));
        // The case with an outline: a vertex (priority 1) 1 ahead and a connection point (priority 0) 3 ahead, and nothing else on.
        let mut b = TargetsBuilder::new(Limits::default());
        b.point(Some(Id(1)), Point::new(5.0, 2.0), Kind::Point(0), 1).unwrap();
        let parts = Parts { detail: true, ..Parts::NONE };
        b.object(Id(9), Object { frame: frame(3.0, 2.25, 0.5, 0.5, 0.0), bounds: None, angle: None, parts }).unwrap();
        let mut path = BezPath::new();
        path.move_to((3.0, 2.0));
        path.line_to((3.2, 2.4));
        path.line_to((2.8, 2.4));
        path.close_path();
        b.path(Id(9), path).unwrap();
        let t = b.build();
        let mut s = Settings { choose: Choose::Ranked, ..on_line() };
        for (kind, priority, on) in [(Kind::Point(0), 0, true), (Kind::Vertex, 1, true), (Kind::Midpoint, 2, false), (Kind::Nearest, 7, false)] {
            s.kinds.set(kind, craft_snap::KindRule { on, reach: Reach::Screen(6.0), priority, scope: craft_snap::Scope::All });
        }
        let r = press(&t, &s, f, Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!((r.delta.x, r.hits[0].kind), (1.0, Kind::Vertex), "{r:?}");
        // A held move is another thing: it takes the better-ranked place at any distance within its reach, as it always did.
        let held_q = Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(f), &s) };
        let held = run(&t, &held_q);
        held.check(&t, &held_q).unwrap();
        assert_eq!((held.delta.x, held.hits[0].kind), (3.0, Kind::Point(0)), "{held:?}");
    }

    #[test]
    fn the_stop_is_strictly_ahead_and_a_place_behind_stops_the_other_way() {
        let s = on_line();
        // A place at the box's own right middle (2, 5) is not ahead of that place; the centre (0, 5) is 2 behind it, and goes there.
        let r = right(&targets(&[(9, 2.0, 5.0)], &[]), &s);
        assert_eq!(r.delta.x, 2.0);
        // Within the same-place rule it is the same place.
        let r = right(&targets(&[(9, 2.0 + 5.0e-10, 5.0)], &[]), &s);
        assert!((r.delta.x - 2.0).abs() < 1e-9, "{r:?}");
        // Behind: a place at (-9, 5) is not ahead of a rightward nudge, and stops a leftward one at 7 (the left middle at -2).
        let t = targets(&[(9, -9.0, 5.0)], &[]);
        assert_eq!(right(&t, &s).delta.x, 0.5);
        let left = press(&t, &s, subject(), Axis::X, false, NudgeReach::Anywhere, &[]);
        assert_eq!(left.delta.x, -7.0);
        assert_eq!(left.delta.y.to_bits(), 0.0f64.to_bits());
        // A place the box is on, every one of its places on lines with it: the next press moves off it.
        let t = targets(&[(9, 2.0, 5.0), (8, 6.0, 5.0)], &[]);
        let first = right(&t, &s);
        assert_eq!(first.delta.x, 2.0, "the centre reaches the place the right middle is on");
        let moved = press(&t, &s, frame(2.0, 5.0, 4.0, 2.0, 0.0), Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!(moved.delta.x, 2.0, "now the right middle (4) goes to 6 and the centre (2) is on the first: the next is 2");
    }

    #[test]
    fn a_nudge_within_a_step_is_stopped_by_a_place_within_it_and_not_by_one_beyond_it() {
        let s = on_line();
        let within = |x: f64, step: f64| press(&targets(&[(9, x, 5.0)], &[]), &s, subject(), Axis::X, true, NudgeReach::Within(step), &[]);
        let r = within(3.5, 2.0);
        assert_eq!((r.delta.x, r.hits.len()), (1.5, 1));
        let r = within(6.0, 2.0);
        assert_eq!((r.delta.x, r.hits.len()), (2.0, 0), "the place is 4 away and the step 2: the step");
        let r = within(4.0, 2.0);
        assert_eq!((r.delta.x, r.hits.len()), (2.0, 1), "exactly the step: stops on it");
        let r = within(4.0 + 1e-6, 2.0);
        assert_eq!((r.delta.x, r.hits.len()), (2.0, 0));
    }

    #[test]
    fn a_place_a_hair_past_the_step_is_a_stop_because_the_same_place_rule_allows_it_and_is_read() {
        let s = on_line();
        // The right side is at x0 + 2 and the step is 2, so the window ends at x0 + 4, and a place just past that is on a bucket boundary's far side. The
        // rule is 1e-9 at the origin and a few times that at 1e6.
        for x0 in [0.0, 1.0e6] {
            for (past, stops) in [(0.0, true), (1.2e-10, true), (5.0e-10, true), (1.0e-6, false)] {
                let t = targets(&[(9, x0 + 4.0 + past, 5.0)], &[]);
                let r = press(&t, &s, frame(x0, 5.0, 4.0, 2.0, 0.0), Axis::X, true, NudgeReach::Within(2.0), &[]);
                assert_eq!(r.hits.len(), usize::from(stops), "x0 {x0:e}, {past:e} past the step: {r:?}");
                assert!((r.delta.x - 2.0).abs() <= if stops { 1.0e-8 } else { 0.0 }, "{r:?}");
                // The same going the other way: the left side is at x0 - 2.
                let t = targets(&[(9, x0 - 4.0 - past, 5.0)], &[]);
                let r = press(&t, &s, frame(x0, 5.0, 4.0, 2.0, 0.0), Axis::X, false, NudgeReach::Within(2.0), &[]);
                assert_eq!(r.hits.len(), usize::from(stops), "x0 {x0:e}, {past:e} past the step, backward: {r:?}");
            }
        }
    }

    #[test]
    fn a_nudge_along_y_and_a_nudge_of_a_turned_box_meet_the_places_on_their_lines() {
        let s = on_line();
        // Up (y grows): the box's lines are x = -2, 0 and 2. A place at (0, 12) is met by the top middle (0, 6): 6.
        let t = targets(&[(9, 0.0, 12.0)], &[]);
        let up = press(&t, &s, subject(), Axis::Y, true, NudgeReach::Anywhere, &[]);
        assert_eq!((up.delta.y, up.delta.x.to_bits()), (6.0, 0.0f64.to_bits()));
        let down = press(&t, &s, subject(), Axis::Y, false, NudgeReach::Anywhere, &[]);
        assert_eq!(down.delta.y, -0.5);
        // A box turned by 0.5 radians: a place on the line of one of its corners.
        let turned = frame(0.0, 5.0, 4.0, 2.0, 0.5);
        let corner = turned.corners()[1];
        let t = targets(&[(9, corner.x + 7.0, corner.y)], &[]);
        let r = press(&t, &s, turned, Axis::X, true, NudgeReach::Anywhere, &[]);
        assert!((r.delta.x - 7.0).abs() < 1e-12, "{r:?}");
        assert_eq!(r.hits.len(), 1);
    }

    #[test]
    fn the_places_of_other_boxes_and_of_a_detail_source_stop_a_nudge_and_so_does_a_crossing() {
        let s = on_line();
        let t = targets(&[], &[(1, frame(11.0, 10.0, 8.0, 6.0, 0.0))]);
        let r = press(&t, &s, frame(0.0, 10.0, 4.0, 6.0, 0.0), Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!(r.delta.x, 5.0);
        assert!(r.hits.iter().all(|h| matches!(h.kind, Kind::Corner | Kind::SideMiddle)), "{:?}", r.hits);
        struct Pins;
        impl Detail for Pins {
            fn points(&mut self, id: Id, _spend: &mut Spend<'_>, out: &mut Vec<(Point, Kind, u32)>) {
                if id == Id(1) {
                    out.push((Point::new(5.5, 10.0), Kind::Point(0), 4));
                }
            }
            fn outline(&mut self, _id: Id, _spend: &mut Spend<'_>, _out: &mut Vec<BezPath>) {}
        }
        let q = query(
            Subject::Nudge {
                frame: frame(0.0, 10.0, 4.0, 6.0, 0.0),
                bounds: None,
                axis: Axis::X,
                forward: true,
                reach: NudgeReach::Anywhere,
                fallback: 0.5,
            },
            &s,
        );
        let r = craft_snap::snap(&t, &q, &mut Pins);
        r.check_with_detail(&t, &q, &mut Pins).unwrap();
        assert_eq!((r.delta.x, r.hits[0].number), (3.5, Some(4)), "{r:?}");
        // Two outlines cross at (6, 5): the box's right middle (2, 5) is 4 from it.
        let mut b = TargetsBuilder::new(Limits::default());
        b.object(Id(1), Object { frame: frame(6.0, 5.0, 1.0, 10.0, 0.0), bounds: None, angle: None, parts: Parts { detail: true, ..Parts::NONE } })
            .unwrap();
        let line = |a: (f64, f64), z: (f64, f64)| {
            let mut p = BezPath::new();
            p.move_to(a);
            p.line_to(z);
            p
        };
        b.path(Id(1), line((4.0, 3.0), (8.0, 7.0))).unwrap();
        b.path(Id(1), line((4.0, 7.0), (8.0, 3.0))).unwrap();
        let t = b.build();
        let r = right(&t, &s);
        assert_eq!((r.delta.x, r.hits[0].kind), (4.0, Kind::Intersection));
    }

    #[test]
    fn a_place_ties_a_line_of_the_same_box_and_wins_on_priority_and_the_setting_off_leaves_the_line() {
        // A box at (11, 10), 8 by 6, with all its lines and places: the subject's right side (x = 2) is 5 from its left edge (x = 7), and the subject's right
        // middle (2, 10) is 5 from its left middle (7, 10).
        let t = scene(&[(1, frame(11.0, 10.0, 8.0, 6.0, 0.0))], &[], &[]);
        let f = frame(0.0, 10.0, 4.0, 6.0, 0.0);
        let on = press(&t, &on_line(), f, Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!(on.delta.x, 5.0);
        assert!(
            matches!(on.hits[0].kind, Kind::Corner | Kind::SideMiddle),
            "a place (priority 1) beats the box's edge (3) at the same distance: {:?}",
            on.hits
        );
        let off = press(&t, &Settings::default(), f, Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!((off.delta.x, off.hits[0].kind), (5.0, Kind::Edge));
        // A reach: the lattice and the place agree on the same 0.5 grid.
        let _ = Reach::Any;
    }

    #[test]
    fn a_place_farther_than_the_buckets_allow_is_not_looked_for_and_the_cut_says_so() {
        let s = on_line();
        // The buckets of a set of places are 1 wide, and a window reads at most 46 of them from the box: a place 40 away is found, one 60 away is not.
        let near = right(&targets(&[(9, 42.0, 5.0)], &[]), &s);
        assert_eq!((near.delta.x, near.hits.len()), (40.0, 1));
        assert!(!near.work.truncated && !near.work.cut.has(Cuts::WINDOW_VISITS), "everything there is lies in the window: {:?}", near.work);
        let far = right(&targets(&[(9, 62.0, 5.0)], &[]), &s);
        assert_eq!((far.delta.x, far.hits.len()), (0.5, 0));
        assert!(far.work.truncated && far.work.cut.has(Cuts::WINDOW_VISITS), "{:?}", far.work);
        // With a line stop nearer than that, nothing is cut: the window ends at the line.
        let mut b = TargetsBuilder::new(Limits::default());
        b.point(Some(Id(9)), Point::new(62.0, 5.0), Kind::Point(0), 9).unwrap();
        b.line(Some(Id(1)), Axis::X, 12.0, None, Kind::Guide).unwrap();
        let r = right(&b.build(), &s);
        assert_eq!((r.delta.x, r.hits[0].kind, r.work.truncated), (10.0, Kind::Guide, false));
    }

    #[test]
    fn a_nudge_that_looks_as_far_as_it_can_says_so_only_when_the_set_holds_something_past_where_it_looked() {
        let s = on_line();
        let report = |t: &Targets, forward: bool| {
            let r = press(t, &s, subject(), Axis::X, forward, NudgeReach::Anywhere, &[]);
            (r.delta.x, r.work.truncated, r.work.cut.has(Cuts::WINDOW_VISITS))
        };
        // Nothing at all: the fallback, and nothing was left unread.
        assert_eq!(report(&targets(&[], &[]), true), (0.5, false, false));
        // One place behind a nudge that goes the other way, or one behind it: the same.
        let behind = targets(&[(9, -5.0, 5.0)], &[]);
        assert_eq!(report(&behind, true), (0.5, false, false));
        // A place ahead inside the window is found, and nothing lies past it.
        assert_eq!(report(&targets(&[(9, 30.0, 5.0)], &[]), true), (28.0, false, false));
        // A place just inside the last bucket read (the window ends 46 past the right side, at 48): found, not cut.
        assert_eq!(report(&targets(&[(9, 48.0, 5.0)], &[]), true), (46.0, false, false));
        // Something past the window, on the line or not: the nudge cannot know without reading it, and says so.
        assert_eq!(report(&targets(&[(9, 48.5, 5.0)], &[]), true), (0.5, true, true));
        assert_eq!(report(&targets(&[(9, 90.0, 400.0)], &[]), true), (0.5, true, true));
        // The other way: the left side is at -2, and the window runs 46 before it to -48.
        assert_eq!(report(&targets(&[(9, -48.0, 5.0)], &[]), false), (-46.0, false, false));
        assert_eq!(report(&targets(&[(9, -49.0, 5.0)], &[]), false), (-0.5, true, true));
        // Objects are in a grid of buckets 8 wide (twice the longer side of a box 4 across), so a window 46 of them long reads 368 past the subject: the
        // box at 100 is read, and its left corner is the stop (98 from the right side, 96 on). One past the window says so, and one wholly behind does not.
        assert_eq!(report(&targets(&[], &[(1, frame(100.0, 5.0, 4.0, 2.0, 0.0))]), true), (96.0, false, false));
        assert_eq!(report(&targets(&[], &[(1, frame(1000.0, 5.0, 4.0, 2.0, 0.0))]), true), (0.5, true, true));
        assert_eq!(report(&targets(&[], &[(1, frame(-100.0, 5.0, 4.0, 2.0, 0.0))]), true), (0.5, false, false));
        // Within a step nothing is clamped, and so nothing is said.
        let step = press(&targets(&[(9, 900.0, 5.0)], &[]), &s, subject(), Axis::X, true, NudgeReach::Within(2.0), &[]);
        assert!(!step.work.truncated, "{:?}", step.work);
    }

    /// A box `w` wide and 10 high whose left side is at 0: its places are on the lines y = 0, 5 and 10, and along x at 0, `w / 2` and `w`.
    fn wide(w: f64) -> TurnedBox {
        frame(w / 2.0, 5.0, w, 10.0, 0.0)
    }

    #[test]
    fn a_subject_wider_than_a_window_may_read_still_stops_at_the_nearest_place_past_either_corner() {
        let s = on_line();
        let two = |a: f64, b: f64| {
            let mut t = TargetsBuilder::new(Limits::default());
            t.point(Some(Id(1)), Point::new(a, 10.0), Kind::Point(0), 1).unwrap();
            t.point(Some(Id(2)), Point::new(b, 10.0), Kind::Point(0), 2).unwrap();
            t.build()
        };
        for w in [40.0, 100.0, 400.0, 1000.0] {
            // One place 1 past the leading corner and one 20 past the trailing one: the first is nearer, from the leading place.
            let t = two(w + 1.0, 20.0);
            let fwd = press(&t, &s, wide(w), Axis::X, true, NudgeReach::Anywhere, &[]);
            assert_eq!((fwd.delta.x, fwd.hits[0].number), (1.0, Some(1)), "width {w}: {fwd:?}");
            // The same, going the other way: 1 past the left corner and 20 before the right one.
            let t = two(-1.0, w - 20.0);
            let back = press(&t, &s, wide(w), Axis::X, false, NudgeReach::Anywhere, &[]);
            assert_eq!((back.delta.x, back.hits[0].number), (-1.0, Some(1)), "width {w}: {back:?}");
            // Only a place ahead of the middle of the subject, past neither corner (the other is far past the leading one): the middle place reaches it.
            let t = two(w * 0.75, w * 2.0);
            let mid = press(&t, &s, wide(w), Axis::X, true, NudgeReach::Anywhere, &[]);
            assert_eq!((mid.delta.x, mid.hits[0].number), (w * 0.25, Some(1)), "width {w}: {mid:?}");
            // Within a step, the same.
            let t = two(w + 1.0, 20.0);
            let step = press(&t, &s, wide(w), Axis::X, true, NudgeReach::Within(3.0), &[]);
            assert_eq!((step.delta.x, step.hits[0].number), (1.0, Some(1)), "width {w}: {step:?}");
        }
        // Far wider than a strip may be (a line on a bucket boundary is two buckets across, and a strip covers 2,304 buckets in all): the part of the
        // line round the trailing place is read, the rest is not, and the answer says so.
        let t = two(5001.0, 20.0);
        let far = press(&t, &s, wide(5000.0), Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!((far.delta.x, far.hits[0].number), (20.0, Some(2)), "{far:?}");
        assert!(far.work.truncated && (far.work.cut.has(Cuts::CANDIDATES) || far.work.cut.has(Cuts::WINDOW_VISITS)), "{:?}", far.work);
        // The same along y, for a tall box.
        let mut t = TargetsBuilder::new(Limits::default());
        t.point(Some(Id(1)), Point::new(10.0, 101.0), Kind::Point(0), 1).unwrap();
        t.point(Some(Id(2)), Point::new(10.0, 20.0), Kind::Point(0), 2).unwrap();
        let up = press(&t.build(), &s, frame(5.0, 50.0, 10.0, 100.0, 0.0), Axis::Y, true, NudgeReach::Anywhere, &[]);
        assert_eq!((up.delta.y, up.hits[0].number), (1.0, Some(1)), "{up:?}");
        // With objects: the corners of a box 1 past the subject's leading side are met, a box inside its span too.
        let t = targets(&[], &[(7, frame(111.0, 5.0, 20.0, 10.0, 0.0))]);
        let r = press(&t, &s, wide(100.0), Axis::X, true, NudgeReach::Anywhere, &[]);
        assert_eq!(r.delta.x, 1.0, "{r:?}");
    }
}

/// The subject's places and the targets' places against a scan: for a held move and for a nudge, on a lattice where places meet lines by themselves,
/// mirrored, moved a long way and turned by a quarter turn.
mod against_a_scan {
    use std::f64::consts::FRAC_PI_2;

    use proptest::prelude::*;

    use super::*;

    /// Everything a scene is made of, to be mapped as a whole.
    #[derive(Clone, Debug)]
    struct Scene {
        points: Vec<Point>,
        objects: Vec<TurnedBox>,
        subject: TurnedBox,
        own: Vec<Point>,
    }

    impl Scene {
        /// The scene moved by `f` and turned by `turn`: places go where `f` takes them, a box's centre too and its angle grows by `turn`.
        fn mapped(&self, f: &dyn Fn(Point) -> Point, turn: f64) -> Scene {
            let frame = |b: &TurnedBox| TurnedBox { centre: f(b.centre), size: b.size, angle: b.angle + turn };
            Scene {
                points: self.points.iter().map(|p| f(*p)).collect(),
                objects: self.objects.iter().map(frame).collect(),
                subject: frame(&self.subject),
                own: self.own.iter().map(|p| f(*p)).collect(),
            }
        }

        fn targets(&self) -> Targets {
            let numbered: Vec<(u64, TurnedBox)> = self.objects.iter().enumerate().map(|(i, b)| (i as u64 + 100, *b)).collect();
            let points: Vec<(u64, f64, f64)> = self.points.iter().enumerate().map(|(i, p)| (i as u64 + 1, p.x, p.y)).collect();
            targets(&points, &numbered)
        }
    }

    /// The nine places of a box, from the definition: its corners, the middles of its sides and its centre.
    fn nine(b: &TurnedBox) -> Vec<Point> {
        let (s, c) = b.angle.sin_cos();
        let at = |u: f64, v: f64| {
            Point::new(
                b.centre.x + c * u * b.size.width * 0.5 - s * v * b.size.height * 0.5,
                b.centre.y + s * u * b.size.width * 0.5 + c * v * b.size.height * 0.5,
            )
        };
        let mut out = vec![];
        for (u, v) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, 0.0)] {
            out.push(at(u, v));
        }
        out
    }

    /// The places of the targets: the app's, and the corners and side middles (not the centre) of each object.
    fn target_places(s: &Scene) -> Vec<Point> {
        let mut out = s.points.clone();
        for o in &s.objects {
            out.extend(nine(o).into_iter().take(8));
        }
        out
    }

    /// The settings with only places on, all of one reach and one priority, so that the nearest is the one that wins and nothing else pulls.
    fn only_places() -> Settings {
        only_places_ranked(Choose::Nearest, [1, 1, 1])
    }

    /// The same, with the tie rule and the priorities of the three kinds of place (the app's own, a corner, a side middle) given.
    fn only_places_ranked(choose: Choose, priorities: [u8; 3]) -> Settings {
        let mut s = Settings { choose, ..on_line() };
        for k in [
            Kind::Guide,
            Kind::Edge,
            Kind::Centre,
            Kind::Spacing,
            Kind::Size,
            Kind::Angle,
            Kind::Vertex,
            Kind::Midpoint,
            Kind::Nearest,
            Kind::Intersection,
            Kind::Frame,
            Kind::Layout,
        ] {
            let mut r = s.kinds.rule(k);
            r.on = false;
            s.kinds.set(k, r);
        }
        for (k, priority) in [Kind::Point(0), Kind::Corner, Kind::SideMiddle].into_iter().zip(priorities) {
            s.kinds.set(k, craft_snap::KindRule { on: true, reach: craft_snap::Reach::Document(6.25), priority, scope: craft_snap::Scope::All });
        }
        s
    }

    fn tol(a: f64, b: f64, far: f64) -> f64 {
        (1e-9 + 16.0 * f64::EPSILON * far).max(8.0 * f64::EPSILON * a.abs().max(b.abs()))
    }

    fn far_of(s: &Scene) -> f64 {
        let big = |p: &Point| p.x.abs().max(p.y.abs());
        let mut m = 0.0f64;
        for b in s.objects.iter().chain([&s.subject]) {
            m = m.max(nine(b).iter().map(big).fold(0.0, f64::max));
        }
        m.max(s.points.iter().map(big).fold(0.0, f64::max)).max(s.own.iter().map(big).fold(0.0, f64::max))
    }

    /// The signed distances along the axis from a place of the subject to a place of the targets on the same line.
    fn meetings(s: &Scene, axis: Axis, with_own: bool) -> Vec<f64> {
        let far = far_of(s);
        let mut mine = nine(&s.subject);
        if with_own {
            mine.extend(s.own.iter().copied());
        }
        let split = |p: &Point| if axis == Axis::X { (p.x, p.y) } else { (p.y, p.x) };
        let mut out = vec![];
        for q in &mine {
            for g in target_places(s) {
                let ((qa, qc), (ga, gc)) = (split(q), split(&g));
                if (gc - qc).abs() <= tol(gc, qc, far) {
                    out.push(ga - qa);
                }
            }
        }
        out
    }

    fn lattice() -> impl Strategy<Value = f64> {
        (-16i32..16).prop_map(|v| f64::from(v) * 0.5)
    }

    fn a_box() -> impl Strategy<Value = TurnedBox> {
        (lattice(), lattice(), 1i32..8, 1i32..8, prop_oneof![Just(0.0), Just(FRAC_PI_2), Just(0.5)]).prop_map(|(x, y, w, h, a)| TurnedBox {
            centre: Point::new(x, y),
            size: kurbo::Size::new(f64::from(w) * 0.5, f64::from(h) * 0.5),
            angle: a,
        })
    }

    /// A subject far wider than a window of the places' grid may be read in one piece (20 to 120 units, where a bucket of the places is 1), square to the
    /// page or a quarter turn from it: the places past its corners and between them are met as for a small one.
    fn wide_box() -> impl Strategy<Value = TurnedBox> {
        (lattice(), lattice(), 40i32..240, 1i32..8, prop_oneof![Just(0.0), Just(FRAC_PI_2)]).prop_map(|(x, y, w, h, a)| TurnedBox {
            centre: Point::new(x, y),
            size: kurbo::Size::new(f64::from(w) * 0.5, f64::from(h) * 0.5),
            angle: a,
        })
    }

    fn scene() -> impl Strategy<Value = Scene> {
        (
            prop::collection::vec((lattice(), lattice()), 0..8),
            prop::collection::vec(a_box(), 0..4),
            prop_oneof![3 => a_box(), 2 => wide_box()],
            prop::collection::vec((0usize..9, -12i32..12), 0..4),
            prop::collection::vec((lattice(), lattice()), 0..3),
        )
            .prop_map(|(points, objects, subject, derived, own)| {
                // Some points are put on the line of one of the subject's places, whatever the turn, so that a turned box has places on its lines too.
                let mine = nine(&subject);
                let mut points: Vec<Point> = points.iter().map(|(x, y)| Point::new(*x, *y)).collect();
                for (i, dx) in derived {
                    let p = mine[i];
                    points.push(Point::new(p.x + f64::from(dx) * 0.5, p.y));
                    points.push(Point::new(p.x, p.y + f64::from(dx) * 0.5));
                }
                Scene { points, objects, subject, own: own.iter().map(|(x, y)| Point::new(*x, *y)).collect() }
            })
    }

    /// A held move along `axis`: the delta, checked.
    fn held_delta(s: &Scene, axis: Axis) -> (Snapped, f64) {
        let t = s.targets();
        let st = only_places();
        let q = Query {
            constraint: Constraint::Axis(axis),
            ..query(Subject::Box { frame: s.subject, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &s.own }, &st)
        };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        let along = if axis == Axis::X { r.delta.x } else { r.delta.y };
        (r, along)
    }

    /// A nudge: the signed travel along the axis, checked.
    fn nudge_travel(s: &Scene, axis: Axis, forward: bool, reach: NudgeReach, fallback: f64) -> (Snapped, f64) {
        nudge_travel_ranked(s, axis, forward, reach, fallback, Choose::Nearest, [1, 1, 1])
    }

    /// A nudge under a tie rule and priorities.
    fn nudge_travel_ranked(
        s: &Scene,
        axis: Axis,
        forward: bool,
        reach: NudgeReach,
        fallback: f64,
        choose: Choose,
        priorities: [u8; 3],
    ) -> (Snapped, f64) {
        let t = s.targets();
        let st = only_places_ranked(choose, priorities);
        let q = query(Subject::Nudge { frame: s.subject, bounds: None, axis, forward, reach, fallback }, &st);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        let along = if axis == Axis::X { r.delta.x } else { r.delta.y };
        (r, along)
    }

    /// The expected signed travel of a held move: the nearest meeting within the reach, or none. Ties at one distance on both sides are told by the
    /// caller (two answers are right).
    fn expected_held(s: &Scene, axis: Axis) -> Vec<f64> {
        let far = far_of(s);
        let within: Vec<f64> = meetings(s, axis, true).into_iter().filter(|d| d.abs() <= 6.25).collect();
        let least = within.iter().map(|d| d.abs()).fold(f64::INFINITY, f64::min);
        let mut best: Vec<f64> = Vec::new();
        for d in within.into_iter().filter(|d| (d.abs() - least).abs() <= tol(least, least, far)) {
            if !best.iter().any(|b| (b - d).abs() <= tol(d, *b, far)) {
                best.push(d);
            }
        }
        if best.is_empty() {
            best.push(0.0);
        }
        best
    }

    /// The expected signed travel of a nudge: the nearest meeting strictly ahead within the reach, else the fallback.
    fn expected_nudge(s: &Scene, axis: Axis, forward: bool, limit: f64, fallback: f64) -> f64 {
        let far = far_of(s);
        let sign = if forward { 1.0 } else { -1.0 };
        let ahead =
            meetings(s, axis, false).into_iter().map(|d| d * sign).filter(|t| *t > tol(*t, 0.0, far) && *t <= limit).fold(f64::INFINITY, f64::min);
        sign * if ahead.is_finite() { ahead } else { fallback }
    }

    /// A fixed seed, so that a run is the same run each time; `CRAFT_SNAP_CASES` and `CRAFT_SNAP_SEED` run a longer search, and each find is a case to read.
    fn config() -> ProptestConfig {
        let number = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(default);
        ProptestConfig {
            cases: u32::try_from(number("CRAFT_SNAP_CASES", 1500)).unwrap_or(1500),
            rng_seed: proptest::test_runner::RngSeed::Fixed(number("CRAFT_SNAP_SEED", 0x7a5e)),
            max_global_rejects: 1_000_000,
            ..ProptestConfig::default()
        }
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn a_held_move_goes_to_the_nearest_place_on_a_line_its_places_travel_along(s in scene(), x_axis in any::<bool>()) {
            let axis = if x_axis { Axis::X } else { Axis::Y };
            let (r, got) = held_delta(&s, axis);
            let far = far_of(&s);
            let want = expected_held(&s, axis);
            prop_assert!(want.iter().any(|w| (got - w).abs() <= tol(*w, got, far)), "got {} want one of {:?}\n{:?}\n{:?}", got, want, s, r);
            let across = if axis == Axis::X { r.delta.y } else { r.delta.x };
            prop_assert_eq!(across.to_bits(), 0.0f64.to_bits());
            if !meetings(&s, axis, true).iter().any(|d| d.abs() <= 6.25) {
                prop_assert!(r.hits.is_empty(), "no place on a line within reach: {:?}", r);
            }
        }

        #[test]
        fn a_nudge_goes_to_the_nearest_place_strictly_ahead_on_a_line_its_places_travel_along(
            s in scene(), x_axis in any::<bool>(), forward in any::<bool>(), step in prop_oneof![Just(None), Just(Some(2.25f64)), Just(Some(7.75f64))],
            ranked in any::<bool>(), priorities in (0u8..4, 0u8..4, 0u8..4),
        ) {
            let axis = if x_axis { Axis::X } else { Axis::Y };
            let (reach, limit, fallback) = match step { None => (NudgeReach::Anywhere, f64::INFINITY, 0.75), Some(w) => (NudgeReach::Within(w), w, w) };
            // The nearest wins under either tie rule and whatever the priorities: they only break a tie.
            let choose = if ranked { Choose::Ranked } else { Choose::Nearest };
            let (r, got) = nudge_travel_ranked(&s, axis, forward, reach, fallback, choose, [priorities.0, priorities.1, priorities.2]);
            let want = expected_nudge(&s, axis, forward, limit, fallback);
            prop_assert!((got - want).abs() <= tol(want, got, far_of(&s)), "got {} want {}\n{:?}\n{:?}", got, want, s, r);
        }

        /// Mirrored along x, the same scene gives the mirrored result; moved a long way, the same; and turned by a quarter, the result turned.
        #[test]
        fn mirrored_moved_and_quarter_turned_scenes_give_the_same_result(
            s in scene(), x_axis in any::<bool>(), forward in any::<bool>(), far in prop::sample::select(vec![1.0e3f64, 1.0e6, 1.0e9]),
        ) {
            let axis = if x_axis { Axis::X } else { Axis::Y };
            // Held. Where two meetings are as near on either side, either is right, and a scene turned or mirrored may take the other.
            let (_, base) = held_delta(&s, axis);
            prop_assume!(expected_held(&s, axis).len() == 1);
            let mirror = s.mapped(&|p| Point::new(-p.x, p.y), 0.0);
            // A mirrored box is the same box, its angle negated: the frame of a mirrored scene turns the other way.
            let mirror = Scene {
                subject: TurnedBox { angle: -s.subject.angle, ..mirror.subject },
                objects: mirror.objects.iter().zip(&s.objects).map(|(m, o)| TurnedBox { angle: -o.angle, ..*m }).collect(),
                ..mirror
            };
            let (_, m) = held_delta(&mirror, axis);
            let sign = if axis == Axis::X { -1.0 } else { 1.0 };
            prop_assert!((m - sign * base).abs() <= 1e-9, "mirrored {} vs {}", m, base);
            let moved = s.mapped(&|p| Point::new(p.x + far, p.y - far * 0.5), 0.0);
            let (_, mv) = held_delta(&moved, axis);
            prop_assert!((mv - base).abs() <= 1e-9 + far * 4e-15, "moved {} vs {}", mv, base);
            // A quarter turn about the origin takes x to y: a hold along x becomes a hold along y.
            let turn = s.mapped(&|p| Point::new(-p.y, p.x), FRAC_PI_2);
            let (_, tu) = held_delta(&turn, axis.other());
            let sign = if axis == Axis::X { 1.0 } else { -1.0 };
            prop_assert!((tu - sign * base).abs() <= 1e-9, "turned {} vs {}", tu, base);
            // Nudged.
            let (_, n) = nudge_travel(&s, axis, forward, NudgeReach::Anywhere, 0.75);
            let (_, nm) = nudge_travel(&mirror, axis, if axis == Axis::X { !forward } else { forward }, NudgeReach::Anywhere, 0.75);
            prop_assert!((nm.abs() - n.abs()).abs() <= 1e-9, "mirrored nudge {} vs {}", nm, n);
            // A quarter turn takes +x to +y and +y to -x.
            let (_, nt) = nudge_travel(&turn, axis.other(), if axis == Axis::X { forward } else { !forward }, NudgeReach::Anywhere, 0.75);
            prop_assert!((nt.abs() - n.abs()).abs() <= 1e-9, "turned nudge {} vs {}", nt, n);
            let (_, nv) = nudge_travel(&moved, axis, forward, NudgeReach::Anywhere, 0.75);
            prop_assert!((nv - n).abs() <= 1e-9 + far * 4e-15, "moved nudge {} vs {}", nv, n);
        }
    }
}

/// The costs of the worst inputs: every cap counted, beside a yardstick of the same query with places left out (`Ignore`), and of the same places a few.
mod hostile {
    use craft_snap::{Cuts, Work};

    use super::*;

    /// `n` places along the line y = 5 + off: 100,000 of them 1e-3 apart across a box at x = 50, or 40 of them 2e-2 apart beside it. None is exactly on
    /// a place of the box.
    fn along_the_line(n: u64, off: f64) -> Targets {
        let (from, pitch) = if n > 1_000 { (5.0e-4, 1.0e-3) } else { (49.0005, 2.0e-2) };
        let places: Vec<(u64, f64, f64)> = (0..n).map(|i| (i + 1, from + i as f64 * pitch, 5.0 + off)).collect();
        targets(&places, &[])
    }

    /// A box 4 by 2 at (50, 5): its places are on the lines y = 4, 5 and 6.
    fn subject() -> TurnedBox {
        frame(50.0, 5.0, 4.0, 2.0, 0.0)
    }

    fn held_with(t: &Targets, s: &Settings) -> Snapped {
        held(t, s, subject(), Axis::X)
    }

    fn nudge_with(t: &Targets, s: &Settings) -> Snapped {
        let q = query(Subject::Nudge { frame: subject(), bounds: None, axis: Axis::X, forward: true, reach: NudgeReach::Anywhere, fallback: 0.5 }, s);
        let r = run(t, &q);
        r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        r
    }

    /// What the places cost on top of the query without them: the bucket entries visited, the places weighed, the objects kept.
    fn extra(on: &Work, off: &Work) -> (u64, u64, u64) {
        (on.visited - off.visited, on.points - off.points, on.candidates - off.candidates)
    }

    #[test]
    fn a_hundred_thousand_places_on_the_line_cost_what_the_caps_say_and_the_result_is_still_the_nearest() {
        let (few, many) = (along_the_line(40, 0.0), along_the_line(100_000, 0.0));
        let off = Settings::default();
        let (r_few, r) = (held_with(&few, &on_line()), held_with(&many, &on_line()));
        // The nearest place to the subject's places on the line: the box's centre is 5e-4 from the place beside it.
        assert!(r.delta.x.abs() <= 6.0e-4 && r.hits.len() == 1, "{r:?}");
        // Three lines of travel, one strip each. A strip visits at most the cap of bucket entries and keeps at most the candidate cap of places, whatever
        // lies on the line; the box's line meets the bucket row of the line above it too, so two strips read the crowd (measured).
        assert_eq!(extra(&r.work, &held_with(&many, &off).work), (2 * 4_096, 64, 64), "{:?}", r.work);
        assert!(
            r.work.cut.has(Cuts::WINDOW_VISITS) && r.work.truncated,
            "more places on the line than the caps look at, and it says so: {:?}",
            r.work
        );
        // The few cost what they hold, and are all of them looked at: the cost does not grow with the places past the caps.
        assert_eq!(extra(&r_few.work, &held_with(&few, &off).work), (40 * 2, 40, 40), "{:?}", r_few.work);
        assert!(!r_few.work.truncated);
        assert!(r.delta.x.abs() <= 5.0e-4 + 1e-12);
        // Of the forty (49.0005 to 49.7805) the nearest to a place of the box is the last, 0.2195 short of its centre at 50.
        assert!((r_few.delta.x + 0.2195).abs() < 1e-9, "{r_few:?}");
    }

    #[test]
    fn a_hundred_thousand_places_on_the_line_of_a_nudge_cost_what_the_caps_say() {
        let (few, many) = (along_the_line(40, 0.0), along_the_line(100_000, 0.0));
        let off = Settings::default();
        let (r_few, r) = (nudge_with(&few, &on_line()), nudge_with(&many, &on_line()));
        assert!(r.delta.x > 0.0 && r.delta.x <= 1.0e-3 + 1e-9 && r.hits.len() == 1, "the first place ahead of one of the box's places: {r:?}");
        assert_eq!(extra(&r.work, &nudge_with(&many, &off).work), (2 * 4_096, 64, 64), "{:?}", r.work);
        assert_eq!(extra(&r_few.work, &nudge_with(&few, &off).work), (40 * 2, 40, 40), "{:?}", r_few.work);
        assert!(r_few.hits.len() == 1 && r_few.delta.x > 0.0);
    }

    #[test]
    fn places_a_hair_off_the_line_are_looked_at_for_what_the_caps_say_and_none_is_met() {
        // Beyond twice the same-place rule: not even in a strip. Between once and twice: in one, weighed, and not on the line. Within it: on it.
        let off = Settings::default();
        for (hair, meets, visited) in [(5.0e-9, false, 4_096), (1.5e-9, false, 2 * 4_096), (5.0e-10, true, 2 * 4_096)] {
            let t = along_the_line(100_000, hair);
            let r = held_with(&t, &on_line());
            let e = extra(&r.work, &held_with(&t, &off).work);
            assert_eq!(!r.hits.is_empty(), meets, "{hair:e}: {r:?}");
            assert_eq!(e.0, visited, "{hair:e}: {:?}", r.work);
            assert!(e.1 <= 64 && e.2 <= 64 && (e.1 > 0) == (hair < 4.0e-9), "{hair:e}: {e:?}");
            let n = nudge_with(&t, &on_line());
            assert_eq!(n.hits.len() == 1, meets, "{hair:e}: {n:?}");
            assert_eq!(extra(&n.work, &nudge_with(&t, &off).work).0, visited, "{hair:e}");
        }
    }

    #[test]
    fn a_hundred_thousand_boxes_whose_places_are_on_the_lines_cost_what_the_caps_say() {
        // Boxes 0.002 wide and 2 high whose corners and middles are on the three lines of the box at (50, 5), all within its reach, 100,000 of them.
        let objs: Vec<(u64, TurnedBox)> = (0..100_000u64).map(|i| (i, frame(50.5 + (i % 1000) as f64 * 0.004, 5.0, 0.002, 2.0, 0.0))).collect();
        let t = targets(&[], &objs);
        let with = |s: &Settings| {
            let q = Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(subject()), s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
            r
        };
        let (on, off) = (with(&on_line()), with(&Settings::default()));
        eprintln!("boxes on {:?}\nboxes off {:?}", on.work, off.work);
        assert!(on.hits.len() == 1 && (on.delta.x - (0.5 - 0.001)).abs() < 5e-3, "{:?}", on.hits);
        let e = extra(&on.work, &off.work);
        assert!(e.0 <= 3 * 4_096 && e.1 <= 4_096 && e.2 <= 3 * 64 && on.work.outlines <= 16, "{:?}", on.work);
        assert!(on.work.truncated, "{:?}", on.work);
    }

    #[test]
    fn the_same_scene_far_from_the_origin_gives_the_same_result_and_costs_the_same() {
        let at = |shift: f64| {
            let places: Vec<(u64, f64, f64)> =
                (0..12u64).map(|i| (i + 1, shift + 6.0 + i as f64 * 0.5, shift + 5.0 + if i % 3 == 0 { 0.0 } else { 1.0 })).collect();
            let t = targets(&places, &[(100, frame(shift + 15.0, shift + 5.0, 2.0, 2.0, 0.0))]);
            let f = frame(shift + 2.0, shift + 5.0, 4.0, 2.0, 0.0);
            let s = on_line();
            let h = Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(f), &s) };
            let hr = run(&t, &h);
            hr.check(&t, &h).unwrap_or_else(|b| panic!("{shift:e}: {b:?}\n{hr:?}"));
            let n = query(Subject::Nudge { frame: f, bounds: None, axis: Axis::X, forward: true, reach: NudgeReach::Anywhere, fallback: 0.5 }, &s);
            let nr = run(&t, &n);
            nr.check(&t, &n).unwrap_or_else(|b| panic!("{shift:e}: {b:?}\n{nr:?}"));
            (hr, nr)
        };
        let (h0, n0) = at(0.0);
        assert!(!h0.hits.is_empty() && !n0.hits.is_empty(), "{h0:?}\n{n0:?}");
        for shift in [1.0e6, 1.0e9, 1.0e11] {
            let (h, n) = at(shift);
            let tol = 1e-9 + shift * 4.0e-15;
            assert!(
                (h.delta - h0.delta).hypot() <= tol && (n.delta - n0.delta).hypot() <= tol,
                "{shift:e}: {:?} {:?} / {:?} {:?}",
                h.delta,
                h0.delta,
                n.delta,
                n0.delta
            );
            assert_eq!(h.hits.len(), h0.hits.len());
            assert_eq!(h.marks.len(), h0.marks.len(), "{shift:e}");
            for (a, b) in [(&h.work, &h0.work), (&n.work, &n0.work)] {
                assert_eq!(
                    (a.visited, a.points, a.gathers, a.examined, a.candidates),
                    (b.visited, b.points, b.gathers, b.examined, b.candidates),
                    "{shift:e}"
                );
            }
        }
    }
}

/// How many windows a query reads for places on its lines of travel, and what an object that meets several of them costs.
mod windows {
    use super::*;

    #[test]
    fn a_turned_box_and_the_places_that_move_with_it_share_at_most_four_strips_of_each_grid() {
        // A turned box has nine lines of travel, and five places that move with it are on five more: fourteen lines, joined into four strips of each of
        // the two grids, so that eight of the query's sixteen windows are for places and not fourteen of each.
        let t = targets(&[(1, 7.0, 5.5), (2, 9.0, 40.0)], &[(10, frame(8.0, 5.0, 2.0, 2.0, 0.0))]);
        let own = [Point::new(0.0, 20.0), Point::new(0.0, 30.0), Point::new(0.0, 40.0), Point::new(0.0, 50.0), Point::new(0.0, 60.0)];
        let f = frame(0.0, 5.0, 4.0, 2.0, 0.5);
        let with = |s: &Settings| {
            let q = Query {
                constraint: Constraint::Axis(Axis::X),
                ..query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &own }, s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
            r
        };
        let (on, off) = (with(&on_line()), with(&Settings::default()));
        assert_eq!(on.work.gathers - off.work.gathers, 8, "{:?} {:?}", on.work, off.work);
        // The same without the five: nine lines, still four strips of each. An unturned box has three lines, and three strips of each grid.
        let (on_s, off_s) = (on_line(), Settings::default());
        let gathers = |s: &Settings, f: TurnedBox| {
            let q = Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(f), s) };
            run(&t, &q).work.gathers
        };
        assert_eq!(gathers(&on_s, f) - gathers(&off_s, f), 8);
        let flat = frame(0.0, 5.0, 4.0, 2.0, 0.0);
        assert_eq!(gathers(&on_s, flat) - gathers(&off_s, flat), 6);
    }

    #[test]
    fn lines_that_lie_near_each_other_share_a_strip_and_the_strips_leave_the_gaps_between_them_unread() {
        // The box's three lines (y 4, 5 and 6) and the lines of six places that move with it (20 and 21, 40 and 41, 80 and 81): nine lines, four strips, cut
        // at the widest gaps. A place between the lines of a strip is weighed (and is not on a line); one in a gap between strips is not even looked at.
        let places: Vec<(u64, f64, f64)> =
            [(1, 10.0), (2, 30.0), (3, 60.0), (4, 5.5), (5, 20.5), (6, 70.0)].iter().map(|(i, y)| (*i, 3.0, *y)).collect();
        let t = targets(&places, &[]);
        let own: Vec<Point> = [20.0, 21.0, 40.0, 41.0, 80.0, 81.0].iter().map(|y| Point::new(0.0, *y)).collect();
        let with = |s: &Settings| {
            let q = Query {
                constraint: Constraint::Axis(Axis::X),
                ..query(Subject::Box { frame: frame(0.0, 5.0, 4.0, 2.0, 0.0), bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &own }, s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            r.work
        };
        let (on, off) = (with(&on_line()), with(&Settings::default()));
        assert_eq!((on.gathers - off.gathers, on.points - off.points), (4, 2), "{on:?} {off:?}");
    }

    #[test]
    fn an_object_that_meets_several_strips_is_looked_at_once() {
        // A box 2 by 2 at (60, 5) meets all three lines of the box at (50, 5): its eight places are weighed once, and its detail asked for once.
        let t = targets(&[], &[(1, frame(58.0, 5.0, 2.0, 2.0, 0.0))]);
        let s = on_line();
        let q = Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(frame(50.0, 5.0, 4.0, 2.0, 0.0)), &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!((r.work.points, r.work.outlines), (8, 1), "{:?}", r.work);
        assert_eq!(r.delta.x, 5.0, "the box's right middle (52, 5) goes to the other's left middle (57, 5)");
    }

    #[test]
    fn a_nudge_looks_no_farther_than_the_line_it_would_stop_on() {
        // A guide 10 ahead of the box's right side, and places beyond it: the places are not looked at (no bucket entry visited for them).
        let places: Vec<(u64, f64, f64)> = (0..200u64).map(|i| (i + 1, 20.0 + i as f64 * 0.1, 5.0)).collect();
        let mut b = TargetsBuilder::new(Limits::default());
        for (i, x, y) in &places {
            b.point(Some(Id(*i)), Point::new(*x, *y), Kind::Point(0), *i as u32).unwrap();
        }
        b.line(Some(Id(1)), Axis::X, 12.0, None, Kind::Guide).unwrap();
        let t = b.build();
        let frame = frame(0.0, 5.0, 4.0, 2.0, 0.0);
        let (on_s, off_s) = (on_line(), Settings::default());
        let nudged = |t: &Targets, s: &Settings| {
            let q = query(Subject::Nudge { frame, bounds: None, axis: Axis::X, forward: true, reach: NudgeReach::Anywhere, fallback: 0.5 }, s);
            let r = run(t, &q);
            r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
            r
        };
        let (on, off) = (nudged(&t, &on_s), nudged(&t, &off_s));
        assert_eq!((on.delta.x, on.hits[0].kind), (10.0, Kind::Guide));
        assert_eq!((on.work.visited, on.work.points, on.work.truncated), (off.work.visited, 0, false), "{:?}", on.work);
        // With the guide beyond the places, they are looked at and the nearest stops it.
        let mut b = TargetsBuilder::new(Limits::default());
        for (i, x, y) in &places {
            b.point(Some(Id(*i)), Point::new(*x, *y), Kind::Point(0), *i as u32).unwrap();
        }
        b.line(Some(Id(1)), Axis::X, 40.0, None, Kind::Guide).unwrap();
        let t = b.build();
        let r = nudged(&t, &on_s);
        assert_eq!((r.delta.x, r.hits[0].kind), (18.0, Kind::Point(0)));
        assert!(r.work.points >= 1 && r.work.visited > 0);
    }
}
