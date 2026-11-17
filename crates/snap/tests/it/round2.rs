//! Fix round 2: what the review of the crate found. Each test is a crafted input from the review, or the smallest one of its kind.

use craft_snap::{Edges, Kind, Lattice, Query, Rounding, Settings, Subject};
use kurbo::{Point, Vec2};

use crate::common::*;

mod far_jumps {
    use super::*;

    const GRID_X: fn() -> [Lattice; 1] = || [Lattice { origin: Point::new(0.3, 0.0), pitch: Vec2::new(1.0, f64::NAN), kind: Kind::Grid }];
    const GRID_Y: fn() -> [Lattice; 1] = || [Lattice { origin: Point::new(0.0, 0.3), pitch: Vec2::new(f64::NAN, 1.0), kind: Kind::Grid }];

    #[test]
    fn two_nearly_parallel_directions_through_a_kind_of_any_reach_never_throw_the_box_far() {
        for tiny in [1.0e-6, 1.0e-7, 1.0e-8] {
            let t = scene(&[(1, frame(0.5, 1000.0, 10.0, 10.0, tiny))], &[], &[]);
            let s = Settings::default();
            let lats = GRID_X();
            let q = Query { lattices: &lats, ..query(box_subject(frame(0.0, 0.0, 10.0, 10.0, tiny)), &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!(r.delta.hypot() < 1_000.0, "{tiny}: the box was thrown {:?}", r.delta);
        }
    }

    #[test]
    fn a_resize_along_a_line_nearly_parallel_to_its_travel_does_not_grow_without_end() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let lats = GRID_Y();
        let moving = Edges { u_max: true, ..Edges::default() };
        for angle in [1.0e-6, 1.0e-8, 1.0e-10, 0.0] {
            let f = frame(0.0, 0.0, 10.0, 10.0, angle);
            let q = Query { lattices: &lats, ..query(Subject::Edges { frame: f, moving }, &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!((r.frame.unwrap().size.width - 10.0).abs() < 1_000.0, "{angle}: {:?}", r.frame);
            let rounded = Query { rounding: Rounding::Position(Vec2::new(1.0, 1.0)), ..q };
            let r = run(&t, &rounded);
            r.check(&t, &rounded).unwrap();
            assert!((r.frame.unwrap().size.width - 10.0).abs() < 1_000.0, "{angle} rounded: {:?}", r.frame);
        }
    }

    #[test]
    fn a_full_grid_under_a_rounding_does_not_grow_a_nearly_unturned_box_without_end() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let lats = [Lattice { origin: Point::new(0.3, 0.3), pitch: Vec2::new(1.0, 1.0), kind: Kind::Grid }];
        let moving = Edges { u_max: true, ..Edges::default() };
        for angle in [1.0e-10, 1.0e-6] {
            let q = Query {
                lattices: &lats,
                rounding: Rounding::Position(Vec2::new(1.0, 1.0)),
                ..query(Subject::Edges { frame: frame(0.0, 0.0, 10.0, 10.0, angle), moving }, &s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!((r.frame.unwrap().size.width - 10.0).abs() < 1_000.0, "{angle}: {:?}", r.frame);
        }
    }
}

mod pair_travel {
    use craft_snap::{Direction, Id, Limits, TargetsBuilder};

    use super::*;

    /// Two guides whose directions are `tilt` apart, one 3 off the box's side to the one side and the other 3 off to the other.
    fn two_guides(tilt: f64) -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        // A vertical guide 3 to the right of the box's left side (x = -5.x), and a guide `tilt` off vertical 3 to its left, on the own frame of the box.
        b.line(Some(Id(1)), craft_snap::Axis::X, -2.0, None, Kind::Guide).unwrap();
        b.line_at(Some(Id(2)), Direction::new(tilt).unwrap(), -8.0, None, Kind::Guide).unwrap();
        b.build()
    }

    #[test]
    fn two_directions_a_little_apart_within_a_reach_do_not_throw_the_box_past_it() {
        let s = Settings::default();
        for tilt in [1.0e-2, 1.0e-3, 1.0e-4] {
            let t = two_guides(tilt);
            let q = query(box_subject(frame(0.0, 0.0, 10.0, 10.0, tilt)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            // The reach is 6: the best pair would travel 6 / tilt along the lines, and no single pull is more than the reach.
            assert!(r.delta.hypot() <= 6.0 + 1e-9, "{tilt}: {:?}", r.delta);
        }
    }
}

mod grid_of_boxes {
    use super::*;

    fn grid(columns: u64, rows: u64) -> Vec<(u64, craft_snap::TurnedBox)> {
        (0..columns * rows).map(|i| (i, frame((i % columns) as f64 * 9.0, (i / columns) as f64 * 9.0, 6.0, 4.0, 0.0))).collect()
    }

    #[test]
    fn a_box_dropped_among_a_three_by_three_grid_passes_the_check() {
        let t = scene(&grid(3, 3), &[], &[]);
        let s = Settings::default();
        // Over the middle box, off its place by a hair, so that gaps are repeated along both directions and one number would name two chains.
        for at in [(9.4, 9.3), (4.6, 13.8), (13.4, 4.6), (9.0, 9.0), (4.5, 4.5)] {
            let q = query(box_subject(frame(at.0, at.1, 6.0, 4.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{at:?}: {b:?}"));
            let chains: Vec<(u16, f64)> = r
                .marks
                .iter()
                .filter_map(|m| if let craft_snap::Mark::Gap { chain, value, .. } = m { Some((*chain, *value)) } else { None })
                .collect();
            for (c, v) in &chains {
                assert!(chains.iter().filter(|(d, _)| d == c).all(|(_, w)| (w - v).abs() < 1e-9), "{at:?}: {chains:?}");
            }
        }
    }

    #[test]
    fn a_box_among_five_thousand_in_a_grid_passes_the_check() {
        let t = scene(&grid(400, 13), &[], &[]);
        let s = Settings::default();
        for at in [(900.4, 45.3), (1800.4, 63.3), (1.0, 1.0)] {
            let q = query(box_subject(frame(at.0, at.1, 6.0, 4.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{at:?}: {b:?}"));
        }
    }
}

mod aspect_ratio {
    use craft_snap::{About, Constraint, Edges, Subject, TurnedBox};

    use super::*;

    fn drag(f: TurnedBox, moving: Edges, ratio: f64, about: About) -> TurnedBox {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let q = Query { constraint: Constraint::Aspect { ratio, about }, ..query(Subject::Edges { frame: f, moving }, &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}"));
        r.frame.unwrap()
    }

    #[test]
    fn a_ratio_other_than_the_dragged_boxs_is_the_ratio_the_box_has() {
        // 4 by 2 dragged by its high corner, a ratio of 3: the smallest box of that ratio that holds it is 6 by 2, and the fixed corner stays.
        let f = frame(2.0, 1.0, 4.0, 2.0, 0.0);
        let out = drag(f, Edges { u_max: true, v_max: true, ..Edges::default() }, 3.0, About::Fixed);
        assert!((out.size.width / out.size.height - 3.0).abs() < 1e-9 && out.size.width >= 4.0 && out.size.height >= 2.0, "{out:?}");
        let (before, after) = (f.corners(), out.corners());
        assert!((after[0] - before[0]).hypot() < 1e-9, "the fixed corner stays: {out:?}");
        // The other way: a ratio of 1 makes the box 4 by 4.
        let out = drag(f, Edges { u_max: true, v_max: true, ..Edges::default() }, 1.0, About::Fixed);
        assert!((out.size.width - 4.0).abs() < 1e-9 && (out.size.height - 4.0).abs() < 1e-9, "{out:?}");
        assert!((out.corners()[0] - before[0]).hypot() < 1e-9, "{out:?}");
        // The low corner dragged: the high corner stays.
        let out = drag(f, Edges { u_min: true, v_min: true, ..Edges::default() }, 1.0, About::Fixed);
        assert!((out.corners()[2] - before[2]).hypot() < 1e-9 && (out.size.width - out.size.height).abs() < 1e-9, "{out:?}");
    }

    #[test]
    fn from_a_side_handle_the_other_axis_grows_about_its_middle_and_about_the_centre_all_of_it_does() {
        let f = frame(2.0, 1.0, 4.0, 2.0, 0.0);
        let out = drag(f, Edges { u_max: true, ..Edges::default() }, 1.0, About::Fixed);
        assert!((out.size.width - 4.0).abs() < 1e-9 && (out.size.height - 4.0).abs() < 1e-9, "{out:?}");
        assert!(
            (out.corners()[0].x - 0.0).abs() < 1e-9 && (out.centre.y - 1.0).abs() < 1e-9,
            "the low side stays and the height grows about the middle: {out:?}"
        );
        let out = drag(f, Edges { u_max: true, v_max: true, ..Edges::default() }, 3.0, About::Centre);
        assert!((out.centre - f.centre).hypot() < 1e-9 && (out.size.width / out.size.height - 3.0).abs() < 1e-9, "{out:?}");
    }

    #[test]
    fn a_turned_box_gets_the_ratio_along_its_own_axes() {
        let f = frame(2.0, 1.0, 4.0, 2.0, 0.4);
        let out = drag(f, Edges { u_max: true, v_max: true, ..Edges::default() }, 3.0, About::Fixed);
        assert!((out.size.width / out.size.height - 3.0).abs() < 1e-9 && out.angle.to_bits() == 0.4f64.to_bits(), "{out:?}");
        assert!((out.corners()[0] - f.corners()[0]).hypot() < 1e-9, "{out:?}");
    }

    #[test]
    fn a_ratio_that_is_not_a_ratio_is_refused() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let f = frame(2.0, 1.0, 4.0, 2.0, 0.0);
        for ratio in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            let moving = Edges { u_max: true, v_max: true, ..Edges::default() };
            let q = Query { constraint: Constraint::Aspect { ratio, about: About::Fixed }, ..query(Subject::Edges { frame: f, moving }, &s) };
            let r = run(&t, &q);
            assert_eq!(r.work.refused, Some(craft_snap::Refusal::NotFinite), "{ratio}");
            r.check(&t, &q).unwrap();
        }
    }
}

mod nudge_and_rounding {
    use craft_snap::{Axis, NudgeReach, Rounding, Subject};

    use super::*;

    #[test]
    fn a_nudge_goes_to_its_target_or_by_its_step_and_is_not_rounded() {
        let s = Settings::default();
        let f = frame(2.3, 1.0, 4.0, 2.0, 0.0);
        let nudge = |guides: &[f64], rounding: Rounding| {
            let t = scene(&[], guides, &[]);
            let q = Query {
                rounding,
                ..query(Subject::Nudge { frame: f, bounds: None, axis: Axis::X, forward: true, reach: NudgeReach::Anywhere, fallback: 0.25 }, &s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{rounding:?}: {b:?}"));
            r.delta.x
        };
        for rounding in [Rounding::Position(Vec2::new(1.0, 1.0)), Rounding::Delta { step: Vec2::new(1.0, 1.0), from: Point::new(2.3, 1.0) }] {
            assert_eq!(nudge(&[], rounding), 0.25, "{rounding:?}: the step is the step");
            assert!((nudge(&[9.55], rounding) - 5.25).abs() < 1e-9, "{rounding:?}: and a target is a target");
        }
    }
}

mod angles_of_many_turns {
    use super::*;

    fn case(a: f64) -> (Vec2, Vec<String>, bool) {
        let t = scene(&[(1, frame(20.0, 0.4, 8.0, 4.0, a)), (2, frame(-20.0, 3.0, 8.0, 4.0, a)), (3, frame(3.0, 30.0, 8.0, 4.0, a))], &[], &[]);
        let s = Settings::default();
        let q = query(box_subject(frame(0.5, 0.0, 8.0, 4.0, a)), &s);
        let out = run(&t, &q);
        let marks = out.marks.iter().map(|m| format!("{m:?}")).collect();
        (out.delta, marks, out.check(&t, &q).is_ok())
    }

    #[test]
    fn an_angle_of_many_turns_has_the_geometry_of_what_it_has_left_and_passes_the_check() {
        for a in [
            0.3,
            0.3 + std::f64::consts::TAU * 1.0e6,
            0.3 + std::f64::consts::TAU * 1.0e9,
            0.3 + std::f64::consts::TAU * 1.0e15,
            1.234e300,
            -1.0e17,
            7.0e15,
        ] {
            let (delta, marks, ok) = case(a);
            assert!(ok, "{a:e}: the check fails");
            // What it has left, by the remainder of the float by a turn (which is exact): the same result to the bit.
            let (delta2, marks2, ok2) = case(a % std::f64::consts::TAU);
            assert!(ok2);
            assert_eq!((delta, marks), (delta2, marks2), "{a:e}");
        }
    }
}

mod numbers_too_big_for_a_place {
    use craft_snap::{Coincide, Id, Limits, Refused, TargetsBuilder};

    use super::*;

    #[test]
    fn two_places_one_of_them_not_a_number_are_not_the_same_place() {
        let co = Settings::default().coincide;
        for x in [0.0, 1.0, -1.0e300, 1.0e308, f64::MAX] {
            assert!(!co.same(f64::INFINITY, x) && !co.same(x, f64::INFINITY), "{x}");
            assert!(!co.same(f64::NEG_INFINITY, x) && !co.same(x, f64::NEG_INFINITY), "{x}");
        }
        assert!(!co.same(f64::NAN, f64::NAN) && !co.same(f64::INFINITY, f64::NEG_INFINITY));
        assert!(
            !co.same(f64::INFINITY, f64::INFINITY) && !co.same(f64::NEG_INFINITY, f64::NEG_INFINITY),
            "an infinity is no place, not even its own"
        );
        assert!(!co.same_point(Point::new(f64::INFINITY, 0.0), Point::new(1.0, 0.0)));
        let _ = Coincide::default;
    }

    #[test]
    fn an_object_whose_corners_overflow_is_refused_and_the_rest_still_mark() {
        let mut b = TargetsBuilder::new(Limits::default());
        b.object(Id(1), object(frame(10.0, 0.0, 4.0, 2.0, 0.0))).unwrap();
        for (i, f) in
            [frame(-1.7e308, 1.7e308, 1.0e308, 1.0e308, 0.0), frame(1.7e308, 0.0, 1.0e308, 1.0, 0.3), frame(0.0, 0.0, 1.0e308, 1.0e308, 0.0)]
                .into_iter()
                .enumerate()
        {
            let r = b.object(Id(20 + i as u64), object(f));
            // The last one has finite corners and a bounds as wide as a number can be: it is taken or refused whole, and either way the rest is not hurt.
            if i < 2 {
                assert_eq!(r, Err(Refused::NotFinite), "{f:?}");
            }
        }
        let t = b.build();
        let s = Settings::default();
        let q = query(Subject::Point { at: Point::new(12.1, 0.3), base: None }, &s);
        let out = run(&t, &q);
        assert!(!out.hits.is_empty() && !out.marks.is_empty(), "{out:?}");
        out.check(&t, &q).unwrap();
    }

    #[test]
    fn bounds_that_are_not_a_size_are_left_out_as_if_there_were_none() {
        let s = Settings::default();
        let with = |bounds: Option<kurbo::Rect>| {
            let mut b = TargetsBuilder::new(Limits::default());
            b.object(Id(1), craft_snap::Object { bounds, ..object(frame(10.0, 0.0, 4.0, 2.0, 0.5)) }).unwrap();
            b.object(Id(2), object(frame(-6.0, 3.0, 4.0, 2.0, 0.0))).unwrap();
            let t = b.build();
            let q = query(box_subject(frame(1.0, 1.0, 3.0, 3.0, 0.0)), &s);
            let out = run(&t, &q);
            out.check(&t, &q).unwrap();
            format!("{out:?}")
        };
        let none = with(None);
        for r in [
            kurbo::Rect::new(f64::NAN, 0.0, 1.0, 1.0),
            kurbo::Rect::new(0.0, 0.0, f64::INFINITY, 1.0),
            kurbo::Rect::new(-1.0e308, 0.0, f64::MAX, 1.0e308),
            kurbo::Rect::new(0.0, -1.0e308, 1.0, f64::MAX),
        ] {
            assert_eq!(with(Some(r)), none, "{r:?}");
        }
        assert_ne!(with(Some(kurbo::Rect::new(0.0, 0.0, 5.0, 5.0))), none, "bounds that are a size are taken");
    }

    #[test]
    fn an_object_as_wide_as_a_number_can_be_does_not_break_what_is_around_it() {
        let s = Settings::default();
        for f in [frame(0.0, 0.0, f64::MAX, f64::MAX, 0.0), frame(0.0, 0.0, 1.0e308, 1.0e308, 0.7), frame(1.0e308, -1.0e308, 1.0e308, 1.0, 0.0)] {
            let mut b = TargetsBuilder::new(Limits::default());
            b.object(Id(1), object(frame(10.0, 0.0, 4.0, 2.0, 0.0))).unwrap();
            let _ = b.object(Id(2), object(f));
            let t = b.build();
            for subject in [Subject::Point { at: Point::new(12.1, 0.3), base: None }, box_subject(frame(1.0, 1.0, 3.0, 3.0, 0.0))] {
                let q = query(subject, &s);
                let out = run(&t, &q);
                out.check(&t, &q).unwrap_or_else(|b| panic!("{f:?}: {b:?}"));
            }
        }
    }
}

mod gaps_of_path_boxes {
    use craft_snap::{Anchors, Id, Limits, Mark, Object, Parts, TargetsBuilder, TurnedBox};
    use kurbo::Rect;

    use super::*;

    fn with_bounds(f: TurnedBox, r: Rect) -> Object {
        Object { frame: f, bounds: Some(r), angle: None, parts: Parts::ALL }
    }

    fn row(objects: &[(u64, Object)]) -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for (id, o) in objects {
            b.object(Id(*id), *o).unwrap();
        }
        b.build()
    }

    /// Settings with the snaps to places off, so a drop among boxes is left to their gaps and lines.
    fn no_places() -> Settings {
        let mut s = Settings::default();
        for k in [Kind::Corner, Kind::SideMiddle, Kind::Vertex, Kind::Midpoint, Kind::Nearest, Kind::Intersection] {
            let mut r = s.kinds.rule(k);
            r.on = false;
            s.kinds.set(k, r);
        }
        s
    }

    fn gaps(out: &craft_snap::Snapped) -> usize {
        out.marks.iter().filter(|m| matches!(m, Mark::Gap { .. })).count()
    }

    #[test]
    fn a_subject_between_two_turned_objects_with_path_boxes_has_gaps_that_pass_the_check() {
        let s = no_places();
        let bx = |x: f64| Rect::new(x - 1.5, -1.5, x + 1.5, 1.5);
        let t = row(&[(1, with_bounds(frame(0.0, 0.0, 4.0, 2.0, 0.5), bx(0.0))), (2, with_bounds(frame(10.0, 0.0, 4.0, 2.0, 0.5), bx(10.0)))]);
        for x in [4.9, 5.0, 5.2, 5.4] {
            let q = query(box_subject(frame(x, 0.0, 3.0, 3.0, 0.0)), &s);
            let out = run(&t, &q);
            out.check(&t, &q).unwrap_or_else(|b| panic!("{x}: {b:?}"));
        }
        // Dropped near the middle, it goes to the middle of the room between the path boxes (1.5 to 8.5, 7 of it, 3 taken by the subject): gaps of 2 each.
        let q = query(box_subject(frame(5.2, 0.0, 3.0, 3.0, 0.0)), &s);
        let out = run(&t, &q);
        assert!(gaps(&out) >= 2, "{out:?}");
        assert!((out.delta.x + 0.2).abs() < 1e-9, "{:?}", out.delta);
        for m in &out.marks {
            if let Mark::Gap { value, .. } = m {
                assert!((value - 2.0).abs() < 1e-9, "{m:?}");
            }
        }
    }

    #[test]
    fn a_subject_with_a_path_box_of_its_own_has_its_gaps_read_from_it() {
        let s = no_places();
        let bx = |x: f64| Rect::new(x - 1.5, -1.5, x + 1.5, 1.5);
        let t = row(&[(1, with_bounds(frame(0.0, 0.0, 4.0, 2.0, 0.0), bx(0.0))), (2, with_bounds(frame(12.0, 0.0, 4.0, 2.0, 0.0), bx(12.0)))]);
        let f = frame(6.3, 0.0, 3.0, 3.0, 0.0);
        // The subject's frame is 3 wide and its path box 5: the gap to either neighbour is read from the path box.
        let subject = |bounds: Option<Rect>| Subject::Box { frame: f, bounds, u: Anchors::ALL, v: Anchors::ALL, own: &[] };
        let wide = Some(Rect::new(6.3 - 2.5, -1.5, 6.3 + 2.5, 1.5));
        let q = query(subject(wide), &s);
        let out = run(&t, &q);
        out.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{out:?}"));
        let q = query(subject(None), &s);
        let plain = run(&t, &q);
        plain.check(&t, &q).unwrap();
        // The path boxes' edges are at 1.5 and 10.5, 9 of room: gaps of 2 around a 5 wide subject, of 3 around a 3 wide one.
        let wide_gap = out.marks.iter().find_map(|m| if let Mark::Gap { value, subject: true, .. } = m { Some(*value) } else { None });
        let plain_gap = plain.marks.iter().find_map(|m| if let Mark::Gap { value, subject: true, .. } = m { Some(*value) } else { None });
        assert!(wide_gap.is_some_and(|g| (g - 2.0).abs() < 1e-9), "{wide_gap:?} {out:?}");
        assert!(plain_gap.is_some_and(|g| (g - 3.0).abs() < 1e-9), "{plain_gap:?} {plain:?}");
    }

    #[test]
    fn bounds_that_are_not_numbers_are_left_out_of_a_row_and_its_check() {
        let s = no_places();
        let bad = Rect::new(f64::NAN, -1.5, 3.0, 1.5);
        let t = row(&[(1, with_bounds(frame(0.0, 0.0, 4.0, 2.0, 0.0), bad)), (2, with_bounds(frame(12.0, 0.0, 4.0, 2.0, 0.0), bad))]);
        let q = query(box_subject(frame(6.3, 0.0, 3.0, 3.0, 0.0)), &s);
        let out = run(&t, &q);
        out.check(&t, &q).unwrap();
        // With the frames: edges at 2 and 10, 8 of room, 3 taken: a gap of 2.5 on either side.
        let g = out.marks.iter().find_map(|m| if let Mark::Gap { value, subject: true, .. } = m { Some(*value) } else { None });
        assert!(g.is_some_and(|g| (g - 2.5).abs() < 1e-9), "{g:?} {out:?}");
    }
}

mod wide_boxes {
    use craft_snap::TurnedBox;

    use super::*;

    #[test]
    fn a_pointer_over_a_small_box_finds_it_among_thousands_of_wide_ones_far_away_and_pays_for_what_it_is_near() {
        let s = Settings::default();
        let mut objs: Vec<(u64, TurnedBox)> =
            (0..5_000u64).map(|i| (10_000 + i, frame(-150.0 + (i % 100) as f64 * 3.0, -75.0 + (i / 100) as f64 * 3.0, 1.0, 1.0, 0.0))).collect();
        objs.push((1, frame(0.0, 0.0, 1.0, 1.0, 0.0)));
        // 4,200 boxes 40 wide and one deep, far from the pointer, each over more than sixteen of the buckets of the small ones.
        objs.extend((0..4_200u64).map(|i| (100 + i, frame(1_000.0 + i as f64 * 50.0, 1_000.0 + i as f64 * 50.0, 40.0, 1.0, 0.0))));
        let t = scene(&objs, &[], &[]);
        let q = query(Subject::Point { at: Point::new(1.2, 0.5), base: None }, &s);
        let out = run(&t, &q);
        out.check(&t, &q).unwrap();
        assert!(out.hits.iter().any(|h| h.id == Some(craft_snap::Id(1))), "the box under the pointer is looked at: {out:?}");
        // Each gather pays for the buckets it is over, not for the wide boxes there are: of the order of a hundred, where it was 6 x 4,000.
        assert!(out.work.visited < 400, "{}", out.work.visited);
    }
}
