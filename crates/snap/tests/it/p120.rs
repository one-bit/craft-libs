//! The modes the owner's ruling P120 adds beside the design's: same-side matching, proportions kept at a corner, the box round an outline,
//! nudging to the next place or by a plain step, size match scoped to a view, and labels to three places.

use craft_snap::{
    About, Anchors, Axis, BoxRound, Budget, Constraint, Cuts, Detail, Edges, Id, Kind, Limits, Matching, NoDetail, NudgeMode, Object, Parts, Reach,
    Settings, SizeScope, Spend, Subject, TargetsBuilder, TurnedBox,
};
use kurbo::{Point, Rect};

use crate::common::*;

mod same_side {
    use super::*;

    fn box_at(centre_x: f64) -> TurnedBox {
        frame(centre_x, 50.0, 10.0, 4.0, 0.0)
    }

    fn hits_with(matching: Matching, centre_x: f64) -> Vec<(Kind, craft_snap::Side)> {
        // One object, 20 wide at x 100 to 120.
        let t = scene(&[(1, frame(110.0, 0.0, 20.0, 4.0, 0.0))], &[], &[]);
        let s = Settings { matching, ..Settings::default() };
        let q = query(Subject::Box { frame: box_at(centre_x), u: Anchors::ALL, v: Anchors { min: false, mid: false, max: false }, own: &[] }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r.hits.iter().filter_map(|h| h.side.map(|(side, _)| (h.kind, side))).collect()
    }

    #[test]
    fn a_low_side_meets_a_low_edge_and_not_a_high_one_under_same_side() {
        // The box's low side is at 118: 2 from the object's high edge at 120, and far from its low edge at 100.
        let any = hits_with(Matching::AnyToAny, 123.0);
        assert!(any.contains(&(Kind::Edge, craft_snap::Side::Min)), "{any:?}");
        let like = hits_with(Matching::LikeToLike, 123.0);
        assert!(like.contains(&(Kind::Edge, craft_snap::Side::Min)), "an edge meets an edge: {like:?}");
        let same = hits_with(Matching::SameSide, 123.0);
        assert!(same.is_empty(), "the low side does not meet a high edge: {same:?}");
        // The box's high side at 122 is 2 from the high edge at 120: the same side meets.
        let same = hits_with(Matching::SameSide, 117.0);
        assert!(same.contains(&(Kind::Edge, craft_snap::Side::Max)), "{same:?}");
        // Middles meet middles under every rule that has sides.
        let same = hits_with(Matching::SameSide, 112.0);
        assert!(same.contains(&(Kind::Centre, craft_snap::Side::Mid)), "{same:?}");
    }
}

mod proportions {
    use super::*;

    fn resize(settings: &Settings, constraint: Constraint, moving: Edges) -> craft_snap::Snapped {
        let t = scene(&[], &[12.5], &[]);
        let f = frame(0.0, 0.0, 20.0, 10.0, 0.0);
        let q = craft_snap::Query { constraint, ..query(Subject::Edges { frame: f, moving }, settings) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    }

    #[test]
    fn a_corner_keeps_its_proportions_with_no_key_when_the_settings_say_so() {
        let corner = Edges { u_max: true, v_max: true, ..Edges::default() };
        let off = resize(&Settings::default(), Constraint::None, corner);
        let f = off.frame.unwrap();
        assert!((f.size.width - 22.5).abs() < 1e-9 && (f.size.height - 10.0).abs() < 1e-9, "{f:?}");
        let keep = Settings { corners_keep_proportions: true, ..Settings::default() };
        let on = resize(&keep, Constraint::None, corner);
        let f = on.frame.unwrap();
        assert!((f.size.width - 22.5).abs() < 1e-9 && (f.size.height - 11.25).abs() < 1e-9, "the corner scaled about the fixed one: {f:?}");
        assert!(on.hits.iter().any(|h| h.kind == Kind::Guide));
        // A side handle is not a corner: it keeps nothing unless asked.
        let side = resize(&keep, Constraint::None, Edges { u_max: true, ..Edges::default() });
        assert!((side.frame.unwrap().size.height - 10.0).abs() < 1e-12);
    }

    #[test]
    fn a_kept_ratio_on_any_handle_is_the_apps_to_ask_for() {
        let side = Edges { u_max: true, ..Edges::default() };
        let r = resize(&Settings::default(), Constraint::Aspect { ratio: 2.0, about: About::Fixed }, side);
        let f = r.frame.unwrap();
        assert!((f.size.width / f.size.height - 2.0).abs() < 1e-9, "{f:?}");
        assert!((f.size.width - 22.5).abs() < 1e-9, "the side lands on the guide: {f:?}");
        let c = resize(&Settings::default(), Constraint::Aspect { ratio: 2.0, about: About::Centre }, side);
        let f = c.frame.unwrap();
        assert!((f.size.width / f.size.height - 2.0).abs() < 1e-9 && f.centre == Point::new(0.0, 0.0), "{f:?}");
    }
}

mod nudging {
    use super::*;

    fn nudge(mode: NudgeMode, guides: &[f64], step: f64) -> (craft_snap::Snapped, craft_snap::Targets, Settings) {
        let t = scene(&[], guides, &[]);
        let s = Settings { nudge: mode, ..Settings::default() };
        let q = query(Subject::Nudge { frame: frame(0.0, 0.0, 20.0, 4.0, 0.0), axis: Axis::X, forward: true, step }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        (r, t, s)
    }

    #[test]
    fn the_next_place_ahead_is_taken_however_far_else_one_tick() {
        // The box's right side is at 10; a guide at 13.7 is far past a step of 0.25.
        let (r, ..) = nudge(NudgeMode::Next { tick: 1.0 }, &[13.7], 0.25);
        assert!((r.delta.x - 3.7).abs() < 1e-9, "{:?}", r.delta);
        assert_eq!(r.hits.first().map(|h| h.kind), Some(Kind::Guide));
        // Nothing ahead: one tick, not the step.
        let (r, ..) = nudge(NudgeMode::Next { tick: 1.0 }, &[-50.0], 0.25);
        assert_eq!(r.delta.x, 1.0);
        assert!(r.hits.is_empty());
        // The design's mode stops only within a step.
        let (r, ..) = nudge(NudgeMode::Stop, &[13.7], 0.25);
        assert_eq!(r.delta.x, 0.25);
    }

    #[test]
    fn a_plain_nudge_is_the_step_exactly_and_ignores_targets() {
        let (r, ..) = nudge(NudgeMode::Plain, &[10.1], 0.25);
        assert_eq!(r.delta.x, 0.25);
        assert!(r.hits.is_empty());
        let (r, ..) = nudge(NudgeMode::Plain, &[], 1.0);
        assert_eq!((r.delta.x, r.delta.y), (1.0, 0.0));
    }

    #[test]
    fn a_nudge_that_cannot_be_told_its_tick_refuses() {
        let t = scene(&[], &[], &[]);
        for tick in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let s = Settings { nudge: NudgeMode::Next { tick }, ..Settings::default() };
            let q = query(Subject::Nudge { frame: frame(0.0, 0.0, 1.0, 1.0, 0.0), axis: Axis::X, forward: true, step: 1.0 }, &s);
            let r = run(&t, &q);
            assert_eq!((r.delta.x, r.work.refused), (0.0, Some(craft_snap::Refusal::Settings)), "{tick}");
        }
    }
}

mod size_scope {
    use super::*;

    fn resize_width(scope: SizeScope) -> craft_snap::Snapped {
        // Another object 7 wide, at x 500.
        let t = scene(&[(1, frame(500.0, 0.0, 7.0, 3.0, 0.0))], &[], &[]);
        let s = Settings { size_scope: scope, ..Settings::default() };
        let f = frame(0.0, 0.0, 8.0, 5.0, 0.0);
        let q = query(Subject::Edges { frame: f, moving: Edges { u_max: true, ..Edges::default() } }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    }

    #[test]
    fn a_size_match_is_scoped_to_the_view_when_one_is_given() {
        let anywhere = resize_width(SizeScope::Anywhere);
        assert!(anywhere.hits.iter().any(|h| h.kind == Kind::Size), "{:?}", anywhere.hits);
        let inside = resize_width(SizeScope::View(Rect::new(400.0, -50.0, 600.0, 50.0)));
        assert!(inside.hits.iter().any(|h| h.kind == Kind::Size));
        let outside = resize_width(SizeScope::View(Rect::new(-50.0, -50.0, 50.0, 50.0)));
        assert!(!outside.hits.iter().any(|h| h.kind == Kind::Size), "{:?}", outside.hits);
        assert!(!outside.marks.iter().any(|m| matches!(m, craft_snap::Mark::Size { .. })));
    }
}

mod box_round {
    use super::*;

    /// A turned object whose outline reaches farther than its corners do: the app gives the box round the outline.
    fn scene_with_bounds() -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        let f = frame(100.0, 0.0, 20.0, 10.0, 30f64.to_radians());
        let round = Rect::new(80.0, -30.0, 140.0, 30.0);
        b.object(Id(1), Object { frame: f, bounds: Some(round), angle: None, parts: Parts::ALL }).unwrap();
        b.build()
    }

    #[test]
    fn the_page_lines_of_a_turned_object_are_the_box_round_its_outline_when_it_is_given() {
        let t = scene_with_bounds();
        let s = Settings::default();
        // A box whose right side is 3 left of the outline's right edge at 140.
        let f = frame(132.0, 100.0, 10.0, 4.0, 0.0);
        let q = query(box_subject(f), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!((r.delta.x - 3.0).abs() < 1e-9, "{:?}", r.delta);
        assert!(r.hits.iter().any(|h| h.kind == Kind::Edge && h.id == Some(Id(1))));
    }

    #[test]
    fn the_subjects_outline_box_is_used_under_paths_and_ignored_under_corners() {
        let t = scene(&[], &[50.0], &[]);
        // The subject's frame is 10 wide at 20 to 30; the box round its outline reaches to 46 on the right.
        let f = frame(25.0, 0.0, 10.0, 4.0, 30f64.to_radians());
        let round = Some(Rect::new(10.0, -6.0, 46.0, 6.0));
        for (mode, expect_guide) in [(BoxRound::Paths, true), (BoxRound::Corners, false)] {
            let s = Settings { box_round: mode, ..Settings::default() };
            let q = craft_snap::Query { bounds: round, ..query(box_subject(f), &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert_eq!(
                r.hits.iter().any(|h| h.kind == Kind::Guide && h.side.is_some_and(|(_, own)| own == craft_snap::OwnOrBounds::Bounds)),
                expect_guide,
                "{mode:?}: {:?}",
                r.hits
            );
        }
    }

    /// A detail source that knows the box round the outline of object 1 only.
    struct Outlines {
        asked: Vec<Id>,
    }

    impl Detail for Outlines {
        fn points(&mut self, _id: Id, _spend: &mut Spend<'_>, _out: &mut Vec<(Point, Kind, u32)>) {}
        fn outline(&mut self, _id: Id, _spend: &mut Spend<'_>, _out: &mut Vec<kurbo::BezPath>) {}
        fn bounds(&mut self, id: Id, spend: &mut Spend<'_>) -> Option<Rect> {
            self.asked.push(id);
            spend.detail(1).then(|| (id == Id(1)).then(|| Rect::new(80.0, -30.0, 140.0, 30.0))).flatten()
        }
    }

    fn near_scene() -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for (i, x) in [100.0, 160.0, 220.0].iter().enumerate() {
            b.object(
                Id(i as u64 + 1),
                Object { frame: frame(*x, 0.0, 20.0, 10.0, 30f64.to_radians()), bounds: None, angle: None, parts: Parts::ALL },
            )
            .unwrap();
        }
        b.build()
    }

    #[test]
    fn the_box_round_a_near_objects_outline_is_asked_on_demand_charged_and_capped() {
        let t = near_scene();
        let s = Settings { box_round: BoxRound::Paths, align_reach: Some(Reach::Document(200.0)), ..Settings::default() };
        let f = frame(132.0, 100.0, 10.0, 4.0, 0.0);
        let q = query(box_subject(f), &s);
        let mut d = Outlines { asked: Vec::new() };
        let r = craft_snap::snap(&t, &q, &mut d);
        r.check(&t, &q).unwrap();
        assert!(!d.asked.is_empty() && d.asked.len() <= 3, "{:?}", d.asked);
        assert_eq!(r.work.outlines as usize, d.asked.len());
        assert_eq!(r.work.detail as usize, d.asked.len());
        assert!((r.delta.x - 3.0).abs() < 1e-9, "the detail source's box for object 1 is the line: {:?}", r.delta);
        // With a budget for one, only one is asked and the result says so.
        let q = craft_snap::Query { budget: Budget { outlines: 1, ..Budget::default() }, ..query(box_subject(f), &s) };
        let mut d = Outlines { asked: Vec::new() };
        let r = craft_snap::snap(&t, &q, &mut d);
        assert_eq!(d.asked.len(), 1);
        assert!(r.work.cut.has(Cuts::OUTLINES) && r.work.truncated);
        // Under `Corners` it is never asked.
        let s = Settings { box_round: BoxRound::Corners, ..s };
        let q = query(box_subject(f), &s);
        let mut d = Outlines { asked: Vec::new() };
        let _ = craft_snap::snap(&t, &q, &mut d);
        assert!(d.asked.is_empty());
        let _ = NoDetail;
    }

    #[test]
    fn a_far_outline_box_that_is_not_a_number_is_ignored() {
        let mut b = TargetsBuilder::new(Limits::default());
        let f = frame(0.0, 0.0, 4.0, 4.0, 0.3);
        b.object(Id(1), Object { frame: f, bounds: Some(Rect::new(f64::NAN, 0.0, 1.0, 1.0)), angle: None, parts: Parts::ALL }).unwrap();
        let t = b.build();
        let s = Settings::default();
        let q = query(box_subject(frame(10.0, 0.0, 2.0, 2.0, 0.0)), &s);
        run(&t, &q).check(&t, &q).unwrap();
    }
}
