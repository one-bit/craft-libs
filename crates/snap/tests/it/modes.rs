//! The modes beside the first ones: same-side matching, proportions kept from a corner or a
//! side handle, a path box for a turned shape, nudging to the first target ahead within a step or anywhere, size match in a view, and a line
//! mark that runs to the nearest target.

use craft_snap::{
    About, Anchors, Axis, Constraint, Cuts, Edges, Id, Kind, KindRule, Limits, LineSpan, Mark, Matching, NudgeReach, Object, Parts, Reach, Scope,
    Settings, Subject, TargetsBuilder, TurnedBox,
};
use kurbo::{Point, Rect};

use crate::common::*;

mod same_side {
    use super::*;

    fn hits_with(matching: Matching, centre_x: f64) -> Vec<(Kind, craft_snap::Side)> {
        // One object, 20 wide at x 100 to 120. The box is far from it across, so no place pulls.
        let t = scene(&[(1, frame(110.0, 0.0, 20.0, 4.0, 0.0))], &[], &[]);
        let s = Settings { matching, ..Settings::default() };
        let q = query(Subject::Box { frame: frame(centre_x, 50.0, 10.0, 4.0, 0.0), bounds: None, u: Anchors::ALL, v: Anchors::NONE, own: &[] }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r.hits.iter().filter_map(|h| h.side.map(|(side, _)| (h.kind, side))).collect()
    }

    #[test]
    fn a_low_side_meets_a_low_edge_and_not_a_high_one_under_same_side() {
        // The box's low side is at 118: 2 from the object's high edge at 120, and far from its low edge at 100.
        assert!(hits_with(Matching::AnyToAny, 123.0).contains(&(Kind::Edge, craft_snap::Side::Min)));
        assert!(hits_with(Matching::LikeToLike, 123.0).contains(&(Kind::Edge, craft_snap::Side::Min)), "an edge meets an edge");
        let same = hits_with(Matching::SameSide, 123.0);
        assert!(same.is_empty(), "the low side does not meet a high edge: {same:?}");
        let same = hits_with(Matching::SameSide, 117.0);
        assert!(same.contains(&(Kind::Edge, craft_snap::Side::Max)), "{same:?}");
        let same = hits_with(Matching::SameSide, 112.0);
        assert!(same.contains(&(Kind::Centre, craft_snap::Side::Mid)), "{same:?}");
    }

    #[test]
    fn guides_have_no_side_and_meet_any_side_under_every_rule() {
        let t = scene(&[], &[125.0], &[]);
        for matching in [Matching::AnyToAny, Matching::LikeToLike, Matching::SameSide] {
            let s = Settings { matching, ..Settings::default() };
            let q = query(Subject::Box { frame: frame(123.0, 50.0, 10.0, 4.0, 0.0), bounds: None, u: Anchors::ALL, v: Anchors::NONE, own: &[] }, &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!(r.hits.iter().any(|h| h.kind == Kind::Guide), "{matching:?}");
        }
    }

    #[test]
    fn two_boxes_turned_alike_meet_by_the_same_side_along_their_own_direction() {
        let a = 30f64.to_radians();
        let t = scene(&[(1, frame(0.0, 0.0, 20.0, 6.0, a))], &[], &[]);
        let mut s = Settings { matching: Matching::SameSide, frames: craft_snap::Frames { page: false, own: true }, ..Settings::default() };
        for k in [Kind::Corner, Kind::SideMiddle] {
            s.kinds.set(k, KindRule { on: false, reach: Reach::Screen(6.0), priority: 1, scope: Scope::All });
        }
        // The same box moved 2 along its own u: every side is 2 off the same side of the object, and not the other side.
        let (c, sn) = (a.cos(), a.sin());
        let f = frame(2.0 * c, 2.0 * sn, 20.0, 6.0, a);
        let q = query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::NONE, own: &[] }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(r.hits.iter().any(|h| h.kind == Kind::Edge), "{:?}", r.hits);
        assert!((r.frame.unwrap().centre - Point::ORIGIN).hypot() < 1e-9, "it lands back on the object's sides");
    }
}

mod proportions {
    use super::*;

    fn resize(constraint: Constraint, moving: Edges) -> craft_snap::Snapped {
        let t = scene(&[], &[12.5], &[]);
        let s = Settings::default();
        let f = frame(0.0, 0.0, 20.0, 10.0, 0.0);
        let q = craft_snap::Query { constraint, ..query(Subject::Edges { frame: f, moving }, &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    }

    #[test]
    fn a_corner_snaps_inside_the_kept_proportions() {
        let corner = Edges { u_max: true, v_max: true, ..Edges::default() };
        let r = resize(Constraint::Aspect { ratio: 2.0, about: About::Fixed }, corner);
        let f = r.frame.unwrap();
        assert!((f.size.width - 22.5).abs() < 1e-9 && (f.size.height - 11.25).abs() < 1e-9, "scaled about the fixed corner onto the guide: {f:?}");
        assert!(r.hits.iter().any(|h| h.kind == Kind::Guide));
        let free = resize(Constraint::None, corner);
        assert!((free.frame.unwrap().size.height - 10.0).abs() < 1e-9, "without the constraint the other side stays");
    }

    #[test]
    fn a_side_handle_keeps_the_opposite_side_and_grows_the_other_axis_about_its_middle() {
        let r = resize(Constraint::Aspect { ratio: 2.0, about: About::Fixed }, Edges { u_max: true, ..Edges::default() });
        let f = r.frame.unwrap();
        assert!((f.size.width - 22.5).abs() < 1e-9 && (f.size.height - 11.25).abs() < 1e-9, "{f:?}");
        assert!((f.centre.x - 1.25).abs() < 1e-9, "the low side stays at -10: {f:?}");
        assert!(f.centre.y.abs() < 1e-9, "the other axis grew about its middle: {f:?}");
        let c = resize(Constraint::Aspect { ratio: 2.0, about: About::Centre }, Edges { u_max: true, ..Edges::default() });
        assert_eq!(c.frame.unwrap().centre, Point::ORIGIN);
    }
}

mod nudging {
    use super::*;

    fn nudge(reach: NudgeReach, fallback: f64, guides: &[f64]) -> (craft_snap::Snapped, craft_snap::Targets, Settings) {
        let t = scene(&[], guides, &[]);
        let s = Settings::default();
        let q = query(Subject::Nudge { frame: frame(0.0, 0.0, 20.0, 4.0, 0.0), bounds: None, axis: Axis::X, forward: true, reach, fallback }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        (r, t, s)
    }

    #[test]
    fn within_stops_at_the_step_and_anywhere_goes_to_the_target() {
        // The box's right side is at 10; the first guide ahead is 3.7 past it.
        let (r, ..) = nudge(NudgeReach::Within(0.25), 0.25, &[13.7]);
        assert_eq!(r.delta.x, 0.25);
        let (r, ..) = nudge(NudgeReach::Anywhere, 1.0, &[13.7]);
        assert!((r.delta.x - 3.7).abs() < 1e-9, "{:?}", r.delta);
        assert_eq!(r.hits.first().map(|h| h.kind), Some(Kind::Guide));
    }

    #[test]
    fn a_nudge_with_nothing_ahead_moves_by_the_fallback_and_one_a_billion_away_travels_the_whole_way_at_the_same_cost() {
        let (none, ..) = nudge(NudgeReach::Anywhere, 0.5, &[-50.0]);
        assert_eq!(none.delta.x, 0.5);
        assert!(none.hits.is_empty());
        let (far, ..) = nudge(NudgeReach::Anywhere, 0.5, &[1.0e9]);
        assert!((far.delta.x - (1.0e9 - 10.0)).abs() < 1e-3, "{:?}", far.delta);
        assert_eq!(far.work.bisect_steps, none.work.bisect_steps, "the same bisections, however far the target is");
        assert_eq!(far.work.visited, none.work.visited);
        assert!(far.work.line_steps <= none.work.line_steps + 1, "the mark of the line it stops on costs one step more");
    }

    #[test]
    fn a_nudge_stops_at_the_next_equal_gap_as_it_does_at_a_line() {
        // Boxes 10 wide at x 0, 30 and 60 (gaps of 20), and a box at 128 to 138 after them: the next place that repeats the gap is 100 to 110.
        let objs: Vec<(u64, TurnedBox)> =
            [0.0, 30.0, 60.0].iter().enumerate().map(|(i, x)| (i as u64, frame(x + 5.0, 0.0, 10.0, 10.0, 0.0))).collect();
        let t = scene(&objs, &[], &[]);
        let s = Settings { spacing_reach: craft_snap::SpacingReach { times: 4.0, at_least: 500.0 }, ..Settings::default() };
        let f = frame(110.0 - 5.0 + 1.0, 0.0, 10.0, 10.0, 0.0);
        let q = query(Subject::Nudge { frame: f, bounds: None, axis: Axis::X, forward: false, reach: NudgeReach::Anywhere, fallback: 0.1 }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?}", r.hits);
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Gap { hit: true, .. })), "{:?}", r.marks);
    }

    #[test]
    fn a_nudge_that_cannot_be_told_its_distance_refuses() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        for (reach, fallback) in [
            (NudgeReach::Within(f64::NAN), 1.0),
            (NudgeReach::Anywhere, f64::INFINITY),
            (NudgeReach::Anywhere, -1.0),
            (NudgeReach::Within(1.0), f64::NAN),
        ] {
            let q = query(Subject::Nudge { frame: frame(0.0, 0.0, 1.0, 1.0, 0.0), bounds: None, axis: Axis::X, forward: true, reach, fallback }, &s);
            let r = run(&t, &q);
            assert_eq!((r.delta.x, r.work.refused), (0.0, Some(craft_snap::Refusal::NotFinite)), "{reach:?} {fallback}");
            r.check(&t, &q).unwrap();
        }
    }
}

mod size_in_view {
    use super::*;

    fn view_rule(settings: &mut Settings) {
        settings.kinds.set(Kind::Size, KindRule { on: true, reach: Reach::Screen(6.0), priority: 6, scope: Scope::View });
    }

    fn resize_width(view: Option<Rect>, targets: &craft_snap::Targets) -> craft_snap::Snapped {
        let mut s = Settings::default();
        view_rule(&mut s);
        let index = view.map(|v| targets.in_view(v));
        let q = craft_snap::Query {
            view: index.as_ref(),
            ..query(Subject::Edges { frame: frame(0.0, 0.0, 8.0, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s)
        };
        let r = run(targets, &q);
        r.check(targets, &q).unwrap();
        r
    }

    #[test]
    fn a_size_match_looks_only_at_the_objects_in_the_view() {
        let t = scene(&[(1, frame(500.0, 0.0, 7.0, 3.0, 0.0))], &[], &[]);
        assert!(resize_width(None, &t).hits.iter().any(|h| h.kind == Kind::Size), "no view: every object");
        assert!(resize_width(Some(Rect::new(400.0, -50.0, 600.0, 50.0)), &t).hits.iter().any(|h| h.kind == Kind::Size));
        let out = resize_width(Some(Rect::new(-50.0, -50.0, 50.0, 50.0)), &t);
        assert!(!out.hits.iter().any(|h| h.kind == Kind::Size), "{:?}", out.hits);
        assert!(!out.marks.iter().any(|m| matches!(m, Mark::Size { .. })));
    }

    #[test]
    fn one_match_in_the_view_among_a_hundred_thousand_equal_lengths_outside_it_is_found_by_one_bisection() {
        let mut b = TargetsBuilder::new(Limits::default());
        for i in 0..100_000u64 {
            b.object(
                Id(i + 10),
                Object {
                    frame: frame(10_000.0 + (i % 400) as f64 * 3.0, 1000.0 + (i / 400) as f64 * 3.0, 7.0, 2.0, 0.0),
                    bounds: None,
                    angle: None,
                    parts: Parts { size: true, ..Parts::NONE },
                },
            )
            .unwrap();
        }
        // The one in view: also 7 wide, a hair off so that it is the nearest to 7.3.
        b.object(Id(1), Object { frame: frame(5.0, 0.0, 7.2, 2.0, 0.0), bounds: None, angle: None, parts: Parts { size: true, ..Parts::NONE } })
            .unwrap();
        let t = b.build();
        let view = Rect::new(-50.0, -50.0, 50.0, 50.0);
        let index = t.in_view(view);
        assert_eq!((index.len(), index.tested()), (1, 100_001), "one bounds test for each object, a sort of the one in view");
        let mut s = Settings::default();
        view_rule(&mut s);
        let q = craft_snap::Query {
            view: Some(&index),
            ..query(Subject::Edges { frame: frame(0.0, 0.0, 7.5, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s)
        };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let hit = r.hits.iter().find(|h| h.kind == Kind::Size).unwrap();
        assert_eq!(hit.id, Some(Id(1)));
        assert!((r.frame.unwrap().size.width - 7.2).abs() < 1e-9);
        assert!(r.work.bisect_steps < 80 && r.work.line_steps < 8, "{:?}", r.work);
    }
}

mod bounds {
    use super::*;

    fn turned_with_bounds() -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        let f = frame(100.0, 0.0, 20.0, 10.0, 30f64.to_radians());
        b.object(Id(1), Object { frame: f, bounds: Some(Rect::new(80.0, -30.0, 140.0, 30.0)), angle: None, parts: Parts::ALL }).unwrap();
        b.build()
    }

    #[test]
    fn a_turned_objects_page_lines_are_the_box_the_app_gives_when_it_gives_one() {
        let t = turned_with_bounds();
        let s = Settings::default();
        let q = query(box_subject(frame(132.0, 100.0, 10.0, 4.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!((r.delta.x - 3.0).abs() < 1e-9, "the right side lands on 140: {:?}", r.delta);
    }

    #[test]
    fn a_turned_subjects_own_bounds_are_what_it_aligns_by_on_the_page_axes_and_move_with_it() {
        let t = scene(&[], &[50.0], &[]);
        let s = Settings::default();
        // The frame is 10 wide at 20 to 30; the box round its paths reaches to 46 on the right: 4 from the guide at 50.
        let f = frame(25.0, 0.0, 10.0, 4.0, 30f64.to_radians());
        let with = |bounds: Option<Rect>| {
            let q = query(Subject::Box { frame: f, bounds, u: Anchors::ALL, v: Anchors::ALL, own: &[] }, &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            r
        };
        let by_paths = with(Some(Rect::new(10.0, -6.0, 46.0, 6.0)));
        assert!((by_paths.delta.x - 4.0).abs() < 1e-9, "{:?}", by_paths.delta);
        let by_corners = with(None);
        assert!(by_corners.hits.iter().all(|h| h.side.is_none_or(|(_, own)| own == craft_snap::OwnOrBounds::Own) || by_corners.delta.x != 4.0));
    }

    #[test]
    fn bounds_that_are_not_a_number_are_ignored() {
        let mut b = TargetsBuilder::new(Limits::default());
        let f = frame(0.0, 0.0, 4.0, 4.0, 0.3);
        b.object(Id(1), Object { frame: f, bounds: Some(Rect::new(f64::NAN, 0.0, 1.0, 1.0)), angle: None, parts: Parts::ALL }).unwrap();
        let t = b.build();
        let s = Settings::default();
        let q = query(box_subject(frame(10.0, 0.0, 2.0, 2.0, 0.0)), &s);
        run(&t, &q).check(&t, &q).unwrap();
    }
}

mod line_span {
    use super::*;

    fn three_boxes() -> craft_snap::Targets {
        // Three boxes in a column on the line x = 50 (their right sides): at y 0, 100 and 300.
        let objs: Vec<(u64, TurnedBox)> = [(1u64, 0.0), (2, 100.0), (3, 300.0)].iter().map(|(i, y)| (*i, frame(45.0, *y, 10.0, 10.0, 0.0))).collect();
        scene(&objs, &[], &[])
    }

    fn mark_for(span: LineSpan) -> (craft_snap::Snapped, craft_snap::Targets) {
        let t = three_boxes();
        let s = Settings { line_span: span, frames: craft_snap::Frames { page: true, own: false }, ..Settings::default() };
        // The subject's right side is on x = 50, near the first box, at y 130 to 140.
        let q = query(
            Subject::Box {
                frame: frame(45.0, 135.0, 10.0, 10.0, 0.0),
                bounds: None,
                u: Anchors { min: false, mid: false, max: true },
                v: Anchors::NONE,
                own: &[],
            },
            &s,
        );
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        (r, t)
    }

    fn reach_of(r: &craft_snap::Snapped) -> (f64, f64) {
        let m = r
            .marks
            .iter()
            .find_map(|m| if let Mark::Line { segment, kinds, .. } = m { kinds.contains(&Kind::Edge).then_some(*segment) } else { None })
            .unwrap();
        (m.p0.y.min(m.p1.y), m.p0.y.max(m.p1.y))
    }

    #[test]
    fn a_line_through_three_boxes_stops_at_the_nearer_one_under_nearest_and_covers_them_all_under_every() {
        let (every, _) = mark_for(LineSpan::Every);
        let (lo, hi) = reach_of(&every);
        assert!(lo <= -5.0 + 1e-9 && hi >= 305.0 - 1e-9, "{lo} {hi}");
        let (nearest, _) = mark_for(LineSpan::Nearest);
        let (lo, hi) = reach_of(&nearest);
        // The subject spans y 130 to 140; the nearest box is the one at y 100 (95 to 105), and the one at 300 is not reached.
        assert!((lo - 95.0).abs() < 1e-9 && (hi - 140.0).abs() < 1e-9, "{lo} {hi}");
        let count = nearest
            .marks
            .iter()
            .find_map(|m| if let Mark::Line { count, kinds, .. } = m { kinds.contains(&Kind::Edge).then_some(*count) } else { None });
        assert_eq!(count, Some(3), "a target the segment does not reach is still counted");
    }

    #[test]
    fn a_hundred_thousand_lines_at_one_place_cost_one_step_under_every_and_one_bisection_under_nearest() {
        let mut b = TargetsBuilder::new(Limits::default());
        for i in 0..100_000u64 {
            b.line(Some(Id(i)), Axis::X, 50.0, Some((i as f64 * 3.0, i as f64 * 3.0 + 1.0)), Kind::Guide).unwrap();
        }
        let t = b.build();
        assert_eq!(t.report().line_places, 1);
        let run_with = |span| {
            let s = Settings { line_span: span, ..Settings::default() };
            let q = query(
                Subject::Box {
                    frame: frame(45.0, 1500.0, 10.0, 10.0, 0.0),
                    bounds: None,
                    u: Anchors { min: false, mid: false, max: true },
                    v: Anchors::NONE,
                    own: &[],
                },
                &s,
            );
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            r
        };
        let every = run_with(LineSpan::Every);
        let nearest = run_with(LineSpan::Nearest);
        assert_eq!(every.work.line_steps, nearest.work.line_steps, "one entry either way");
        let seg = |r: &craft_snap::Snapped| {
            r.marks
                .iter()
                .find_map(|m| {
                    if let Mark::Line { segment, count, .. } = m {
                        Some((segment.p0.y.min(segment.p1.y), segment.p0.y.max(segment.p1.y), *count))
                    } else {
                        None
                    }
                })
                .unwrap()
        };
        let (el, eh, ec) = seg(&every);
        let (nl, nh, nc) = seg(&nearest);
        assert!(el <= 0.0 && eh >= 299_998.0 && ec == 100_000, "{el} {eh} {ec}");
        assert!(nl >= 1490.0 && nh <= 1520.0 && nc == 100_000, "the segment reaches the member by the subject only: {nl} {nh}");
        let _ = Cuts::default();
    }
}

mod lattices {
    use craft_snap::{Anchors, Constraint, Lattice, Mark, Reach, Rounding};

    use super::*;

    fn grid(origin: Point, pitch: (f64, f64)) -> Lattice {
        Lattice { origin, pitch: vec2(pitch.0, pitch.1), kind: Kind::Grid }
    }

    #[test]
    fn a_side_lands_on_the_nearest_line_of_a_lattice_from_any_distance_on_each_axis_and_it_is_marked() {
        // An object far off keeps the origin of the targets away from 0: the lattice is in the app's frame.
        let t = scene(&[(1, frame(1.0e4, 1.0e4, 4.0, 4.0, 0.0))], &[], &[]);
        let s = Settings::default();
        let lat = [grid(Point::new(1.5, -2.0), (10.0, 5.0))];
        // Sides at x 36.5, 41.5, 46.5 (lines at 41.5, 51.5: the middle is on one), y: the nearest lines are 3 from the low side.
        let f = frame(41.5, 20.0, 10.0, 4.0, 0.0);
        let q = craft_snap::Query { lattices: &lat, ..query(box_subject(f), &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!(r.delta.x, 0.0, "{:?}", r.delta);
        // y lines are at -2 + 5n: 18 and 23; the sides at 18, 20, 22 (the low side is on 18).
        assert_eq!(r.delta.y, 0.0, "{:?}", r.delta);
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Line { kinds, .. } if kinds == &[Kind::Grid])));
        // Moved a little, it snaps back from any distance (the grid reaches everywhere), to the nearest line.
        let f = frame(43.4, 21.4, 10.0, 4.0, 0.0);
        let q = craft_snap::Query { lattices: &lat, ..query(box_subject(f), &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!((r.delta.x + 1.9).abs() < 1e-9, "the middle goes to 41.5: {:?}", r.delta);
        // y lines at -2 + 5n; the sides at 19.4, 21.4, 23.4: the nearest line is 23 (0.4 from the high side).
        assert!((r.delta.y + 0.4).abs() < 1e-9, "{:?}", r.delta);
        assert!(r.hits.iter().all(|h| h.kind == Kind::Grid || h.kind.is_place()), "{:?}", r.hits);
    }

    #[test]
    fn a_lattice_whose_pitch_or_origin_is_not_a_number_or_whose_kind_is_off_has_no_lines() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let f = frame(43.4, 21.4, 10.0, 4.0, 0.0);
        for lat in [
            grid(Point::new(0.0, 0.0), (0.0, 0.0)),
            grid(Point::new(f64::NAN, 0.0), (10.0, 10.0)),
            grid(Point::new(0.0, 0.0), (-1.0, f64::INFINITY)),
            Lattice { kind: Kind::Ruler, ..grid(Point::ORIGIN, (1.0, 1.0)) },
        ] {
            let lats = [lat];
            let q = craft_snap::Query { lattices: &lats, ..query(box_subject(f), &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            // A lattice with an origin that is not a number has no line on that axis, and the other axis is its own.
            let nan_x = lat.origin.x.is_nan();
            assert_eq!(r.delta.x, 0.0, "{lat:?}");
            assert!(nan_x || r.delta.y == 0.0, "{lat:?}");
        }
        // One axis with a pitch and the other without: only the first has lines.
        let lats = [grid(Point::new(0.0, 0.0), (10.0, f64::NAN))];
        let q = craft_snap::Query { lattices: &lats, ..query(box_subject(f), &s) };
        let r = run(&t, &q);
        assert!(r.delta.x != 0.0 && r.delta.y == 0.0, "{:?}", r.delta);
    }

    #[test]
    fn a_lattice_under_an_axis_lock_a_rounding_and_a_view_of_the_far_page_still_holds() {
        let t = scene(&[(1, frame(-3.0e5, 2.0e5, 4.0, 4.0, 0.0))], &[], &[]);
        let s = Settings::default();
        let lat = [grid(Point::new(0.25, 0.75), (2.5, 2.5))];
        for (constraint, rounding) in [
            (Constraint::None, Rounding::None),
            (Constraint::Axis(craft_snap::Axis::X), Rounding::None),
            (Constraint::None, Rounding::Position(vec2(0.25, 0.25))),
            (Constraint::Ray { origin: Point::ORIGIN, dir: vec2(1.0, 0.5) }, Rounding::None),
        ] {
            let f = frame(33.3, 17.1, 6.0, 3.0, 0.0);
            let q = craft_snap::Query {
                lattices: &lat,
                constraint,
                rounding,
                ..query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &[] }, &s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{constraint:?} {rounding:?}: {b:?}"));
        }
        let mut s2 = Settings::default();
        s2.kinds.set(Kind::Grid, KindRule { on: true, reach: Reach::Screen(0.5), priority: 8, scope: Scope::All });
        let q = craft_snap::Query { lattices: &lat, ..query(box_subject(frame(33.3, 17.1, 6.0, 3.0, 0.0)), &s2) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
    }
}

mod reach_across {
    use craft_snap::{Budget, Cuts, Frames, Reach};

    use super::*;

    fn settings(reach: f64) -> Settings {
        let mut s = Settings { align_reach: Some(Reach::Document(reach)), frames: Frames { page: true, own: true }, ..Settings::default() };
        for k in [Kind::Corner, Kind::SideMiddle, Kind::Spacing, Kind::Size] {
            s.kinds.set(k, KindRule { on: false, reach: Reach::Screen(6.0), priority: 1, scope: Scope::All });
        }
        s
    }

    #[test]
    fn turned_objects_at_every_angle_are_aligned_by_their_own_sides_within_a_reach_across() {
        for a in [0.3, 1.0, 1.5, 2.0, 2.8, 3.5, 4.0, 5.5] {
            let (c, s) = (f64::cos(a), f64::sin(a));
            // One object turned by `a`; the subject turned alike, moved 2 along its own u and 0.5 along its own v.
            let t = scene(&[(1, frame(0.0, 0.0, 20.0, 8.0, a))], &[], &[]);
            let st = Settings { frames: Frames { page: false, own: true }, ..settings(100.0) };
            let f = frame(2.0 * c - 0.5 * s, 2.0 * s + 0.5 * c, 20.0, 8.0, a);
            let q = query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &[] }, &st);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{a}: {b:?}"));
            assert!(r.hits.iter().any(|h| h.side.is_some_and(|(_, own)| own == craft_snap::OwnOrBounds::Own)), "{a}: {:?}", r.hits);
            assert!((r.frame.unwrap().centre - Point::ORIGIN).hypot() < 1e-9, "{a}: it lands back on the object's sides: {:?}", r.frame);
            // The segment of a mark runs along the object's side and the subject's, which coincide here: it is no longer than one of them.
            for m in &r.marks {
                if let craft_snap::Mark::Line { segment, .. } = m {
                    assert!((segment.p1 - segment.p0).hypot() <= 20.0 + 1e-6, "{a}: {m:?}");
                }
            }
        }
    }

    #[test]
    fn lines_in_another_direction_are_not_lines_of_this_one() {
        // The object's horizontal lines lie at y = 50 (its centre) and near 48 and 52, which are where the subject's vertical sides are in x; its
        // vertical lines are at x = 500. Aligning in x must find none of them.
        let t = scene(&[(1, frame(500.0, 50.0, 4.0, 4.0, 0.0))], &[], &[]);
        let st = settings(2000.0);
        let q = query(Subject::Box { frame: frame(50.0, 900.0, 4.0, 4.0, 0.0), bounds: None, u: Anchors::ALL, v: Anchors::NONE, own: &[] }, &st);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(r.hits.is_empty() && r.delta.x == 0.0, "{:?}", r.hits);
    }

    #[test]
    fn an_object_with_only_bounds_lines_or_only_own_lines_is_still_a_target() {
        let mut b = TargetsBuilder::new(Limits::default());
        b.object(
            Id(1),
            Object { frame: frame(0.0, 0.0, 10.0, 10.0, 0.0), bounds: None, angle: None, parts: Parts { bounds_lines: true, ..Parts::NONE } },
        )
        .unwrap();
        b.object(
            Id(2),
            Object { frame: frame(20.0, 30.0, 10.0, 10.0, 0.0), bounds: None, angle: None, parts: Parts { own_lines: true, ..Parts::NONE } },
        )
        .unwrap();
        let t = b.build();
        let st = settings(100.0);
        let low = query(
            Subject::Box {
                frame: frame(10.0, 15.0, 8.0, 4.0, 0.0),
                bounds: None,
                u: Anchors { min: true, mid: false, max: false },
                v: Anchors::NONE,
                own: &[],
            },
            &st,
        );
        let r = run(&t, &low);
        r.check(&t, &low).unwrap();
        assert!(r.hits.iter().any(|h| h.id == Some(Id(1))), "only bounds lines: {:?}", r.hits);
        let high = query(
            Subject::Box {
                frame: frame(24.0, 15.0, 8.0, 4.0, 0.0),
                bounds: None,
                u: Anchors { min: true, mid: false, max: false },
                v: Anchors::NONE,
                own: &[],
            },
            &st,
        );
        let r2 = run(&t, &high);
        r2.check(&t, &high).unwrap();
        assert!(r2.hits.iter().any(|h| h.id == Some(Id(2))), "only own lines: {:?}", r2.hits);
    }

    #[test]
    fn a_window_cut_by_the_visits_or_by_the_candidates_says_which() {
        let objs: Vec<(u64, TurnedBox)> = (0..40u64).map(|i| (i, frame(i as f64 * 3.0, 0.0, 2.0, 2.0, 0.0))).collect();
        let t = scene(&objs, &[], &[]);
        let st = settings(1000.0);
        let run_with = |budget: Budget| {
            let q = craft_snap::Query { budget, ..query(box_subject(frame(60.0, 0.0, 2.0, 2.0, 0.0)), &st) };
            run(&t, &q).work
        };
        let few = run_with(Budget { candidates: 3, ..Budget::default() });
        assert!(few.cut.has(Cuts::CANDIDATES) && !few.cut.has(Cuts::WINDOW_VISITS), "{few:?}");
        let short = run_with(Budget { window_visits: 6, ..Budget::default() });
        assert!(short.cut.has(Cuts::WINDOW_VISITS), "{short:?}");
    }
}

mod nudge_ahead {
    use craft_snap::{Lattice, NudgeReach, Reach};

    use super::*;

    fn nudge(t: &craft_snap::Targets, s: &Settings, lats: &[Lattice], f: TurnedBox, axis: Axis, forward: bool) -> craft_snap::Snapped {
        let q = craft_snap::Query {
            lattices: lats,
            ..query(Subject::Nudge { frame: f, bounds: None, axis, forward, reach: NudgeReach::Anywhere, fallback: 0.5 }, s)
        };
        let r = run(t, &q);
        r.check(t, &q).unwrap();
        r
    }

    fn only_lines() -> Settings {
        let mut s = Settings::default();
        for k in [Kind::Corner, Kind::SideMiddle, Kind::Spacing, Kind::Size] {
            s.kinds.set(k, KindRule { on: false, reach: Reach::Screen(6.0), priority: 1, scope: Scope::All });
        }
        s
    }

    fn grid(pitch: f64) -> Lattice {
        Lattice { origin: Point::new(0.0, 0.0), pitch: vec2(pitch, pitch), kind: Kind::Grid }
    }

    #[test]
    fn the_next_lattice_line_ahead_is_the_stop_in_either_direction_and_a_side_on_a_line_moves_off_it() {
        let t = scene(&[], &[], &[]);
        let s = only_lines();
        let lats = [grid(7.0)];
        // Sides at -10, 0, 10: the lines are at multiples of 7. Forward: -10 to -7 (3), 0 to 7 (7), 10 to 14 (4): the nearest is 3.
        let f = frame(0.0, 0.0, 20.0, 4.0, 0.0);
        assert!((nudge(&t, &s, &lats, f, Axis::X, true).delta.x - 3.0).abs() < 1e-9);
        // Backward: 10 to 7 (3), 0 to -7 (7), -10 to -14 (4).
        assert!((nudge(&t, &s, &lats, f, Axis::X, false).delta.x + 3.0).abs() < 1e-9);
        // Sides on lines (-7, 0, 7): the next is a whole pitch away, not the line it is on.
        let on = frame(0.0, 0.0, 14.0, 4.0, 0.0);
        assert!((nudge(&t, &s, &lats, on, Axis::X, true).delta.x - 7.0).abs() < 1e-9);
        assert!((nudge(&t, &s, &lats, on, Axis::X, false).delta.x + 7.0).abs() < 1e-9);
        // Along y, with the height's sides at -3.5, 0, 3.5 on lines.
        let tall = frame(0.0, 0.0, 4.0, 7.0, 0.0);
        assert!((nudge(&t, &s, &lats, tall, Axis::Y, true).delta.y - 3.5).abs() < 1e-9);
        assert!((nudge(&t, &s, &lats, tall, Axis::Y, false).delta.y + 3.5).abs() < 1e-9);
        // A pitch that is not a number has no lines: the fallback.
        let bad = [Lattice { pitch: vec2(f64::NAN, 0.0), ..grid(1.0) }];
        assert_eq!(nudge(&t, &s, &bad, f, Axis::X, true).delta.x, 0.5);
    }

    #[test]
    fn a_guide_behind_stops_a_backward_nudge_and_one_just_past_the_tolerance_counts_as_ahead() {
        let s = only_lines();
        let f = frame(0.0, 0.0, 20.0, 4.0, 0.0);
        let t = scene(&[], &[-13.7], &[]);
        assert!((nudge(&t, &s, &[], f, Axis::X, false).delta.x + 3.7).abs() < 1e-9);
        assert_eq!(nudge(&t, &s, &[], f, Axis::X, true).delta.x, 0.5, "behind is not ahead");
        // A guide exactly on the box's high side is not ahead of that side (the middle side, 10 behind it, is the one that reaches it); one 1e-6 past it is.
        let on = scene(&[], &[10.0], &[]);
        assert!((nudge(&on, &s, &[], f, Axis::X, true).delta.x - 10.0).abs() < 1e-9);
        let past = scene(&[], &[10.000_001], &[]);
        assert!((nudge(&past, &s, &[], f, Axis::X, true).delta.x - 0.000_001).abs() < 1e-9);
    }

    #[test]
    fn within_a_reach_across_the_next_object_line_ahead_is_found_and_one_beyond_the_reach_is_not() {
        let t =
            scene(&[(1, frame(35.0, 0.0, 10.0, 4.0, 0.0)), (2, frame(-35.0, 0.0, 10.0, 4.0, 0.0)), (3, frame(900.0, 0.0, 10.0, 4.0, 0.0))], &[], &[]);
        let s = Settings { align_reach: Some(Reach::Document(60.0)), ..only_lines() };
        let f = frame(0.0, 0.0, 20.0, 4.0, 0.0);
        assert!((nudge(&t, &s, &[], f, Axis::X, true).delta.x - 20.0).abs() < 1e-9, "the edge at 30 from the high side at 10");
        assert!((nudge(&t, &s, &[], f, Axis::X, false).delta.x + 20.0).abs() < 1e-9);
        let far = scene(&[(3, frame(900.0, 0.0, 10.0, 4.0, 0.0))], &[], &[]);
        assert_eq!(nudge(&far, &s, &[], f, Axis::X, true).delta.x, 0.5, "past the reach there is nothing ahead");
        let anywhere = only_lines();
        assert!((nudge(&far, &anywhere, &[], f, Axis::X, true).delta.x - 885.0).abs() < 1e-9, "anywhere on the page it is found");
    }
}
