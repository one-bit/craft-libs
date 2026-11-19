//! Hostile inputs of the design that need a crafted scene, each with a counted test: a hundred thousand distinct lines inside one reach,
//! degenerate boxes and rows, and every early return of a bound checked against the classes of input the bound has to catch.
//! No test here asserts elapsed time in a default run. The `#[ignore]`d `timed_*` tests time each case beside a yardstick, back to back.

use craft_snap::{
    Axis, Budget, Constraint, Cuts, Edges, Id, Kind, Lattice, Limits, Mark, NudgeReach, Object, Parts, Reach, Rounding, Settings, SpacingReach,
    Subject, Targets, TargetsBuilder, TurnedBox, Work,
};
use kurbo::Point;

use crate::common::*;

/// `n` guides across the page, one every `pitch`, the first at `first`: each is its own place, none within the coincidence of another.
fn guides(n: u64, first: f64, pitch: f64) -> Targets {
    let mut b = TargetsBuilder::new(Limits::default());
    for i in 0..n {
        b.line(Some(Id(1000)), Axis::X, first + i as f64 * pitch, None, Kind::Guide).unwrap();
    }
    b.build()
}

fn only_guides() -> Settings {
    let mut s = Settings::default();
    for k in [Kind::Corner, Kind::SideMiddle, Kind::Spacing, Kind::Size, Kind::Angle, Kind::Edge, Kind::Centre] {
        let mut r = s.kinds.rule(k);
        r.on = false;
        s.kinds.set(k, r);
    }
    s
}

/// What a scan of every guide gives for a box along the page: the signed distance from the nearest side to the nearest guide.
fn scan(guides: &[f64], f: &TurnedBox) -> f64 {
    let sides = [f.centre.x - f.size.width / 2.0, f.centre.x, f.centre.x + f.size.width / 2.0];
    sides.iter().flat_map(|s| guides.iter().map(move |g| g - s)).min_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(0.0)
}

mod distinct_lines_in_one_reach {
    use super::*;

    fn run_on(t: &Targets, s: &Settings, f: TurnedBox) -> craft_snap::Snapped {
        let q = query(box_subject(f), s);
        let r = run(t, &q);
        r.check(t, &q).unwrap();
        r
    }

    #[test]
    fn the_nearest_of_a_hundred_thousand_is_found_by_bisection_and_walks_nothing() {
        let s = only_guides();
        // A hundred thousand guides a ten-thousandth apart, all within 6 of the box; the box is not on the lattice of them.
        let (big, small) = (guides(100_000, -5.0, 1.0e-4), guides(100, -5.0, 0.1));
        let list: Vec<f64> = (0..100_000u64).map(|i| -5.0 + i as f64 * 1.0e-4).collect();
        let f = frame(0.00003, 0.0, 4.00002, 3.0, 0.0);
        let r = run_on(&big, &s, f);
        assert!((r.delta.x - scan(&list, &f)).abs() < 1e-9, "scan {} got {:?}", scan(&list, &f), r.delta);
        assert_eq!(r.hits.len(), 1, "{:?}", r.hits);
        let w = &r.work;
        assert_eq!((w.visited, w.gathers, w.examined, w.truncated), (0, 0, 0, false), "{w:?}");
        assert_eq!(w.cut, Cuts::default(), "no cap met: {w:?}");
        // The walk past a bisection is the entries at the one place: the guide itself.
        assert_eq!((w.bisect_steps, w.line_steps), (114, 1), "{w:?}");
        // Against a hundred guides, only the logarithm grows: six lookups, each ten bits (a thousandfold the lines) deeper.
        let a = run_on(&small, &s, f).work;
        assert_eq!((a.bisect_steps, a.line_steps), (54, 1), "{a:?}");
    }

    #[test]
    fn lines_just_beyond_the_coincidence_of_each_other_are_walked_past_only_as_far_as_the_one_at_the_spot() {
        let s = only_guides();
        // A pitch of 1.5e-9 against a coincidence of 1e-9: no two are the same place, and each is within a coincidence of its neighbours' edges.
        let t = guides(100_000, 0.0, 1.5e-9);
        assert_eq!(t.report().line_places, 100_000, "{:?}", t.report());
        let r = run_on(&t, &s, frame(0.00005, 0.0, 4.0, 3.0, 0.0));
        assert_eq!((r.work.bisect_steps, r.work.line_steps, r.work.truncated), (114, 1, false), "{:?}", r.work);
    }
}

mod equal_lengths {
    use super::*;

    /// `n` objects of width 7 and height 2 that have only a size to offer, spread over the page.
    fn widths(n: u64) -> Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for i in 0..n {
            let at = frame(10_000.0 + (i % 400) as f64 * 30.0, 1000.0 + (i / 400) as f64 * 30.0, 7.0, 2.0, 0.0);
            b.object(Id(i + 10), Object { frame: at, bounds: None, angle: None, parts: Parts { size: true, ..Parts::NONE } }).unwrap();
        }
        b.build()
    }

    fn resized(t: &Targets) -> craft_snap::Snapped {
        let s = Settings::default();
        let q = query(Subject::Edges { frame: frame(0.0, 0.0, 7.2, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s);
        let r = run(t, &q);
        r.check(t, &q).unwrap();
        r
    }

    #[test]
    fn a_hundred_thousand_equal_widths_are_counted_to_the_walk_cap_and_the_cut_says_so() {
        let (big, small) = (widths(100_000), widths(100));
        let (r, a) = (resized(&big), resized(&small));
        assert!(r.hits.iter().any(|h| h.kind == Kind::Size) && a.hits.iter().any(|h| h.kind == Kind::Size));
        assert!((r.frame.unwrap().size.width - 7.0).abs() < 1e-9, "{:?}", r.frame);
        let w = &r.work;
        assert_eq!(w.line_steps, u64::from(Budget::default().line_steps), "{w:?}");
        assert!(w.cut.has(Cuts::LINE_STEPS) && w.truncated, "{w:?}");
        // A hundred equal widths are walked whole, and the cut is not met.
        assert_eq!(a.work.line_steps, 100, "{:?}", a.work);
        assert!(!a.work.cut.has(Cuts::LINE_STEPS), "{:?}", a.work);
        // The size mark counts what was walked, not what there is.
        let marked = r.marks.iter().find_map(|m| if let Mark::Size { count, .. } = m { Some(*count) } else { None }).unwrap();
        assert_eq!(marked, 4_096, "{:?}", r.marks);
    }
}

/// Settings in which only the kinds named are on, with rows that reach far.
fn only(kinds: &[Kind]) -> Settings {
    let mut s = Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 500.0 }, ..Settings::default() };
    for k in [
        Kind::Guide,
        Kind::Corner,
        Kind::SideMiddle,
        Kind::Spacing,
        Kind::Size,
        Kind::Angle,
        Kind::Edge,
        Kind::Centre,
        Kind::Vertex,
        Kind::Midpoint,
        Kind::Nearest,
        Kind::Intersection,
    ] {
        let mut r = s.kinds.rule(k);
        r.on = kinds.contains(&k);
        s.kinds.set(k, r);
    }
    s
}

fn gaps(r: &craft_snap::Snapped) -> Vec<f64> {
    r.marks.iter().filter_map(|m| if let Mark::Gap { value, .. } = m { Some(*value) } else { None }).collect()
}

mod degenerate {
    use super::*;

    fn dropped(objs: &[(u64, TurnedBox)], s: &Settings, f: TurnedBox) -> craft_snap::Snapped {
        let t = scene(objs, &[], &[]);
        let q = query(box_subject(f), s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    }

    #[test]
    fn boxes_of_no_width_are_a_row_with_gaps_between_their_places() {
        let s = only(&[Kind::Spacing]);
        let objs: Vec<(u64, TurnedBox)> = [0.0, 20.0, 40.0].iter().enumerate().map(|(i, x)| (i as u64, frame(*x, 0.0, 0.0, 10.0, 0.0))).collect();
        let r = dropped(&objs, &s, frame(66.0, 0.0, 10.0, 10.0, 0.0));
        assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?}", r.hits);
        assert!((r.frame.unwrap().centre.x - 65.0).abs() < 1e-9, "the box's left side 20 from the last place, as the gaps of the row: {:?}", r.frame);
        assert!(gaps(&r).iter().all(|g| (g - 20.0).abs() < 1e-9) && gaps(&r).len() == 3, "{:?}", gaps(&r));
        // A subject of no width is a row place too.
        let r = dropped(&objs, &s, frame(61.5, 0.0, 0.0, 10.0, 0.0));
        assert!((r.frame.unwrap().centre.x - 60.0).abs() < 1e-9, "{:?}", r.frame);
        // Boxes of no width and no height, at one place: nothing is a gap, and nothing breaks.
        let same: Vec<(u64, TurnedBox)> = (0..5).map(|i| (i, frame(10.0, 10.0, 0.0, 0.0, 0.0))).collect();
        let r = dropped(&same, &s, frame(10.5, 10.0, 0.0, 0.0, 0.0));
        assert!(gaps(&r).is_empty(), "{:?}", r.marks);
    }

    #[test]
    fn boxes_that_touch_have_no_gap_and_a_gap_of_nothing_is_never_repeated() {
        let s = only(&[Kind::Spacing]);
        let touching: Vec<(u64, TurnedBox)> = (0..3).map(|i| (i, frame(5.0 + i as f64 * 10.0, 0.0, 10.0, 10.0, 0.0))).collect();
        let r = dropped(&touching, &s, frame(35.4, 0.0, 10.0, 10.0, 0.0));
        assert!(gaps(&r).is_empty() && r.hits.is_empty() && r.delta == vec2(0.0, 0.0), "{:?} {:?}", r.marks, r.hits);
        // The same boxes a little apart do make a gap, so the empty result above is not the scene being out of reach.
        let apart: Vec<(u64, TurnedBox)> = (0..3).map(|i| (i, frame(5.0 + i as f64 * 12.0, 0.0, 10.0, 10.0, 0.0))).collect();
        let r = dropped(&apart, &s, frame(41.4, 0.0, 10.0, 10.0, 0.0));
        assert!(!gaps(&r).is_empty() && r.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?} {:?}", r.marks, r.hits);
    }

    #[test]
    fn overlapping_boxes_are_not_in_the_row_and_never_make_a_gap_below_nothing() {
        let s = only(&[Kind::Spacing]);
        // A and B overlap by 5; C stands 25 beyond B.
        let objs = [(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(10.0, 0.0, 10.0, 10.0, 0.0)), (3, frame(45.0, 0.0, 10.0, 10.0, 0.0))];
        let r = dropped(&objs, &s, frame(80.5, 0.0, 10.0, 10.0, 0.0));
        assert!(gaps(&r).iter().all(|g| *g > 0.0), "{:?}", gaps(&r));
        // The gap B to C is 25: the box lands 25 beyond C's high side, which is at 50.
        assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing) && (r.frame.unwrap().centre.x - 80.0).abs() < 1e-9, "{:?} {:?}", r.hits, r.frame);
        // Two boxes one on the other, and nothing else: no row, no gap.
        let stacked = [(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(8.0, 0.0, 10.0, 10.0, 0.0))];
        let r = dropped(&stacked, &s, frame(40.5, 0.0, 10.0, 10.0, 0.0));
        assert!(gaps(&r).is_empty() && r.hits.is_empty(), "{:?}", r.marks);
    }

    #[test]
    fn a_box_wider_than_the_room_between_its_neighbours_has_no_centred_candidate() {
        let s = only(&[Kind::Spacing]);
        let objs = [(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))];
        // The room between them is 20 wide.
        let r = dropped(&objs, &s, frame(20.3, 0.0, 6.0, 10.0, 0.0));
        assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing) && (r.frame.unwrap().centre.x - 20.0).abs() < 1e-9, "centred: {:?}", r.hits);
        for wide in [20.0, 20.5, 40.0] {
            let r = dropped(&objs, &s, frame(20.3, 0.0, wide, 10.0, 0.0));
            assert!(r.hits.is_empty() && r.delta == vec2(0.0, 0.0), "{wide}: {:?} {:?}", r.hits, r.marks);
        }
    }

    /// A box with its low side at 0, dragged by its high side to `u_max_at`, among guides.
    pub(super) fn drag(guides: &[f64], u_max_at: f64) -> craft_snap::Snapped {
        let (s, t) = (only(&[Kind::Guide]), scene(&[], guides, &[]));
        let f = frame(u_max_at / 2.0, 0.0, u_max_at, 10.0, 0.0);
        let q = query(Subject::Edges { frame: f, moving: Edges { u_max: true, ..Edges::default() } }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    }

    #[test]
    fn two_sides_that_would_each_land_on_one_guide_do_not_collapse_the_box_between_them() {
        // Both sides of the width are dragged and the one guide is inside the box: each side alone may take it, both together leave no width.
        let (s, t) = (only(&[Kind::Guide]), scene(&[], &[3.0], &[]));
        for (u_min, u_max, centre) in [(true, true, 2.0), (true, true, 2.4), (true, true, 1.6)] {
            let f = frame(centre, 0.0, 4.0, 10.0, 0.0);
            let q = query(Subject::Edges { frame: f, moving: Edges { u_min, u_max, ..Edges::default() } }, &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{centre}: {b:?} {:?}", r.frame));
            assert!(r.frame.unwrap().size.width > 0.0, "{centre}: {:?}", r.frame);
            assert_eq!(r.hits.len(), 1, "{centre}: one side takes the guide: {:?}", r.hits);
        }
    }

    #[test]
    fn a_resize_that_would_collapse_or_flip_the_box_is_not_offered() {
        // Near a guide at 6 the box is 6 wide: a plain snap, so the cases below are not out of reach.
        assert!((drag(&[6.0], 5.2).frame.unwrap().size.width - 6.0).abs() < 1e-9);
        assert!((drag(&[6.0], 1.5).frame.unwrap().size.width - 6.0).abs() < 1e-9, "a guide 4.5 off is in reach");
        // A guide at the low side would leave the box no width, and one beyond it would flip it: neither is offered.
        for (guides, at) in [(&[0.0][..], 1.5), (&[0.0][..], 0.4), (&[-2.0][..], 1.5), (&[-2.0][..], 0.4), (&[0.0, -2.0][..], 0.9)] {
            let r = drag(guides, at);
            let after = r.frame.unwrap();
            assert!(r.hits.is_empty() && (after.size.width - at).abs() < 1e-9, "{guides:?} {at}: {:?} {:?}", r.frame, r.hits);
        }
    }
}

mod shortcuts {
    use super::*;

    fn dropped(t: &Targets, s: &Settings, f: TurnedBox, lattices: &[Lattice]) -> craft_snap::Snapped {
        let q = craft_snap::Query { lattices, ..query(box_subject(f), s) };
        let r = run(t, &q);
        r.check(t, &q).unwrap();
        r
    }

    #[test]
    fn a_window_with_nothing_in_it_still_reads_the_guides_the_lattices_and_the_far_objects_when_the_reach_is_not_across() {
        let s = Settings { align_reach: Some(Reach::Document(50.0)), ..only(&[Kind::Guide, Kind::Grid, Kind::Edge]) };
        let lat = [Lattice { origin: Point::new(500.0, 0.0), pitch: vec2(1000.0, 1.0), kind: Kind::Grid }];
        // One object far beyond the reach across, a guide 1 off the box's high side, and a lattice 0.3 off its centre line.
        let t = scene(&[(1, frame(1.0e6, 0.0, 10.0, 10.0, 0.0))], &[11.0], &[]);
        let r = dropped(&t, &s, frame(0.0, 0.3, 20.0, 10.0, 0.0), &lat);
        let kinds: Vec<Kind> = r.hits.iter().map(|h| h.kind).collect();
        assert!(kinds.contains(&Kind::Guide) && kinds.contains(&Kind::Grid), "{kinds:?}");
        assert_eq!(
            (r.work.gathers, r.work.candidates, r.work.visited),
            (2, 0, 0),
            "the windows (candidates, and the marks' look) held nothing: {:?}",
            r.work
        );
        // The object's edge in the same row of the page is across the page, out of the reach across: not a target.
        let level = scene(&[(1, frame(1.0e6, 2.2, 10.0, 4.0, 0.0))], &[], &[]);
        let r = dropped(&level, &s, frame(0.0, 5.0, 20.0, 6.0, 0.0), &[]);
        assert!(r.hits.is_empty(), "{:?}", r.hits);
        // With no reach across the same object is a target at any distance.
        let anywhere = Settings { align_reach: None, ..s };
        let r = dropped(&level, &anywhere, frame(0.0, 5.0, 20.0, 6.0, 0.0), &[]);
        assert!(r.hits.iter().any(|h| h.kind == Kind::Edge && h.id == Some(Id(1))), "{:?}", r.hits);
    }

    #[test]
    fn a_row_of_one_object_makes_no_gap_and_does_not_hide_its_other_places() {
        let s = only(&[Kind::Spacing, Kind::Edge]);
        let t = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        // Its top is level with the box's, and there is no gap to repeat.
        let r = dropped(&t, &s, frame(40.0, 0.4, 10.0, 10.0, 0.0), &[]);
        assert!(gaps(&r).is_empty() && r.hits.iter().all(|h| h.kind == Kind::Edge) && !r.hits.is_empty(), "{:?} {:?}", r.hits, r.marks);
        // The second object makes the gap, and the same box repeats it.
        let two = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let r = dropped(&two, &s, frame(65.4, 0.0, 10.0, 10.0, 0.0), &[]);
        assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing) && gaps(&r).len() == 2, "{:?} {:?}", r.hits, r.marks);
    }

    #[test]
    fn an_exact_hit_is_still_a_hit_with_its_mark_and_does_not_end_the_search_on_the_other_axis() {
        let s = only(&[Kind::Guide]);
        let t = scene(&[], &[5.0], &[20.0]);
        // The low side is exactly on the guide at 5; the high side is 0.4 under the one at 20.
        let r = dropped(&t, &s, frame(10.0, 14.6, 10.0, 10.0, 0.0), &[]);
        assert_eq!(r.delta.x, 0.0);
        assert!((r.delta.y - 0.4).abs() < 1e-9, "{:?}", r.delta);
        assert_eq!(r.hits.iter().filter(|h| h.kind == Kind::Guide).count(), 2, "{:?}", r.hits);
        let lines = r.marks.iter().filter(|m| matches!(m, Mark::Line { hit: true, .. })).count();
        assert_eq!(lines, 2, "{:?}", r.marks);
        // Exactly on the repeat of a gap: the gap marks are there, with the subject's own, and the box does not move.
        let s = only(&[Kind::Spacing]);
        let two = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let r = dropped(&two, &s, frame(65.0, 0.0, 10.0, 10.0, 0.0), &[]);
        assert_eq!(r.delta, vec2(0.0, 0.0));
        assert_eq!(gaps(&r).len(), 2, "the row's gap and the subject's own: {:?}", r.marks);
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Gap { subject: true, hit: true, .. })));
        // Exactly the width of another box: the size mark is there.
        let s = only(&[Kind::Size]);
        let t = scene(&[(1, frame(100.0, 100.0, 7.0, 2.0, 0.0))], &[], &[]);
        let q = query(Subject::Edges { frame: frame(0.0, 0.0, 7.0, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Size { hit: true, .. })), "{:?}", r.marks);
        // Exactly on the lattice of a rounding: no move, and the rounding is not skipped for the other axis.
        let q = craft_snap::Query { rounding: Rounding::Position(vec2(1.0, 1.0)), ..query(box_subject(frame(3.0, 4.3, 2.0, 2.0, 0.0)), &s) };
        let empty = scene(&[], &[], &[]);
        let r = run(&empty, &q);
        r.check(&empty, &q).unwrap();
        assert_eq!(r.delta.x, 0.0);
        assert!((r.delta.y - -0.3).abs() < 1e-9 || (r.delta.y - 0.7).abs() < 1e-9, "{:?}", r.delta);
    }
}

mod caps {
    use super::*;

    /// A scene that uses every cap there is something to charge for without a detail source: objects across a reach, guides, a row of gaps,
    /// sizes, places.
    pub(super) fn scene_of_everything() -> Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for i in 0..40u64 {
            let f = frame(((i * 37) % 53) as f64 * 1.5, ((i * 11) % 47) as f64 * 1.5, 1.0 + (i % 4) as f64, 1.0 + (i % 3) as f64, 0.0);
            b.object(Id(i), Object { frame: f, bounds: None, angle: None, parts: Parts { detail: false, ..Parts::ALL } }).unwrap();
        }
        for g in [3.0, 30.0] {
            b.line(Some(Id(1000)), Axis::X, g, None, Kind::Guide).unwrap();
            b.line(Some(Id(2000)), Axis::Y, g + 1.0, None, Kind::Guide).unwrap();
        }
        b.build()
    }

    pub(super) fn settings() -> Settings {
        Settings { align_reach: Some(Reach::Document(20.0)), spacing_reach: SpacingReach { times: 4.0, at_least: 25.0 }, ..Settings::default() }
    }

    /// Each cap, how to set it, the counter in the work it limits and the bit it sets.
    #[allow(clippy::type_complexity)]
    pub(super) fn fields() -> Vec<(&'static str, fn(&mut Budget, u32), fn(&Work) -> u64, u16)> {
        vec![
            ("window_visits", |b, v| b.window_visits = v, |w| w.visited, Cuts::WINDOW_VISITS),
            ("candidates", |b, v| b.candidates = v, |w| w.candidates, Cuts::CANDIDATES),
            ("own", |b, v| b.own = v, |w| w.own, Cuts::OWN),
            ("line_steps", |b, v| b.line_steps = v, |w| w.line_steps, Cuts::LINE_STEPS),
            ("solves", |b, v| b.solves = v, |w| w.solves, Cuts::SOLVES),
            ("points", |b, v| b.points = v, |w| w.points, Cuts::POINTS),
            ("outlines", |b, v| b.outlines = v, |w| w.outlines, Cuts::OUTLINES),
            ("pieces", |b, v| b.pieces = v, |w| w.pieces, Cuts::PIECES),
            ("pairs", |b, v| b.pairs = v, |w| w.pairs, Cuts::PAIRS),
            ("row", |b, v| b.row = v, |w| w.row, Cuts::ROW),
            ("marks", |b, v| b.marks = v, |w| w.marks, Cuts::MARKS),
        ]
    }

    fn subjects() -> Vec<(&'static str, Subject<'static>, Constraint)> {
        vec![
            ("move", box_subject(frame(40.3, 33.1, 3.0, 2.0, 0.0)), Constraint::None),
            ("turned move", box_subject(frame(40.3, 33.1, 3.0, 2.0, 0.4)), Constraint::None),
            (
                "resize",
                Subject::Edges { frame: frame(40.3, 33.1, 3.0, 2.0, 0.0), moving: Edges { u_max: true, v_max: true, ..Edges::default() } },
                Constraint::None,
            ),
            ("point", Subject::Point { at: Point::new(40.3, 33.1), base: None }, Constraint::None),
            ("locked move", box_subject(frame(40.3, 33.1, 3.0, 2.0, 0.0)), Constraint::Axis(Axis::X)),
            ("locked turned move", box_subject(frame(40.3, 33.1, 3.0, 2.0, 0.4)), Constraint::Axis(Axis::Y)),
            (
                "nudge",
                Subject::Nudge {
                    frame: frame(40.3, 33.1, 3.0, 2.0, 0.0),
                    bounds: None,
                    axis: Axis::X,
                    forward: true,
                    reach: NudgeReach::Within(9.0),
                    fallback: 9.0,
                },
                Constraint::None,
            ),
        ]
    }

    #[test]
    fn a_cap_of_nothing_or_one_gives_a_result_that_checks_with_the_cut_set_exactly_where_the_work_was_asked_for() {
        let t = scene_of_everything();
        let mut exercised = std::collections::BTreeSet::new();
        // As an app that turns on the places a held move and a nudge meet is, and as one that does not.
        for s in
            [settings(), Settings { locked_places: craft_snap::TravelPlaces::OnLine, nudge_places: craft_snap::TravelPlaces::OnLine, ..settings() }]
        {
            for (what, subject, constraint) in subjects() {
                let base = craft_snap::Query { constraint, ..query(subject, &s) };
                let full = run(&t, &base);
                full.check(&t, &base).unwrap();
                assert!(!full.work.cut.any() && !full.work.truncated, "{what}: the default caps are not met by this scene: {:?}", full.work);
                for (name, set, counter, bit) in fields() {
                    for v in [0u32, 1] {
                        let mut budget = Budget::default();
                        set(&mut budget, v);
                        let q = craft_snap::Query { budget, ..base.clone() };
                        let r = run(&t, &q);
                        r.check(&t, &q).unwrap_or_else(|b| panic!("{what} {name}={v}: {b:?} {:?}", r.work));
                        assert_eq!(r.work.truncated, r.work.cut.any(), "{what} {name}={v}: {:?}", r.work);
                        // The work the cap limits is never past the cap (windows and rows once for each window gathered).
                        let allowed = u64::from(v).saturating_mul(craft_snap::MAX_GATHERS.max(1));
                        assert!(counter(&r.work) <= allowed, "{what} {name}={v}: {:?}", r.work);
                        // If the full run did the work, a cap of nothing stopped some of it, and says so.
                        if v == 0 && counter(&full.work) > 0 {
                            exercised.insert(name);
                            assert!(r.work.cut.has(bit), "{what} {name}=0: {:?}", r.work);
                        }
                    }
                }
            }
        }
        // The scene reaches every cap that needs no detail source.
        for name in ["window_visits", "candidates", "line_steps", "solves", "points", "row", "marks"] {
            assert!(exercised.contains(name), "{name} was never exercised: {exercised:?}");
        }
    }
}

/// Real-time checks of the worst inputs. Each times a yardstick beside the case, back to back, and compares the two, so a loaded machine slows
/// both. They are ignored in a default run: `cargo test -p craft-snap --test it -- --ignored timed`.
mod timed {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    use super::*;

    /// The fastest of `n` runs of `f`, each repeated `reps` times.
    fn best(n: usize, reps: usize, mut f: impl FnMut()) -> Duration {
        (0..n)
            .map(|_| {
                let start = Instant::now();
                (0..reps).for_each(|_| f());
                start.elapsed()
            })
            .min()
            .unwrap_or_default()
    }

    /// The case against its yardstick, back to back, twice over so that a slow start does not count against either.
    fn beside(yardstick: &mut dyn FnMut(), case: &mut dyn FnMut()) -> (Duration, Duration) {
        let (a, b) = (best(5, 20, &mut *yardstick), best(5, 20, &mut *case));
        let (c, d) = (best(5, 20, &mut *yardstick), best(5, 20, &mut *case));
        (a.min(c), b.min(d))
    }

    fn ratio(case: Duration, yardstick: Duration) -> f64 {
        case.as_secs_f64() / yardstick.as_secs_f64().max(1e-9)
    }

    #[test]
    #[ignore = "real time"]
    fn timed_a_hundred_thousand_distinct_lines_beside_a_hundred() {
        let (big, small) = (guides(100_000, -5.0, 1.0e-4), guides(100, -5.0, 0.1));
        let s = only_guides();
        let f = frame(0.00003, 0.0, 4.00002, 3.0, 0.0);
        let q = query(box_subject(f), &s);
        let (y, c) = beside(&mut || drop(black_box(run(&small, &q))), &mut || drop(black_box(run(&big, &q))));
        println!("distinct lines: yardstick {y:?}, case {c:?}, ratio {:.2}", ratio(c, y));
        assert!(ratio(c, y) < 4.0, "a thousand times the lines is a logarithm more: {y:?} {c:?}");
    }

    #[test]
    #[ignore = "real time"]
    fn timed_a_hundred_thousand_equal_widths_beside_a_hundred() {
        let widths = |n: u64| {
            let mut b = TargetsBuilder::new(Limits::default());
            for i in 0..n {
                let at = frame(10_000.0 + (i % 400) as f64 * 30.0, 1000.0 + (i / 400) as f64 * 30.0, 7.0, 2.0, 0.0);
                b.object(Id(i + 10), Object { frame: at, bounds: None, angle: None, parts: Parts { size: true, ..Parts::NONE } }).unwrap();
            }
            b.build()
        };
        let (big, small) = (widths(100_000), widths(100));
        let s = Settings::default();
        let q = query(Subject::Edges { frame: frame(0.0, 0.0, 7.2, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s);
        let (y, c) = beside(&mut || drop(black_box(run(&small, &q))), &mut || drop(black_box(run(&big, &q))));
        println!("equal widths: yardstick {y:?}, case {c:?}, ratio {:.2}", ratio(c, y));
        // The walk is cut at 4,096 steps against a hundred: no more than the cap's worth over the yardstick.
        assert!(ratio(c, y) < 80.0, "{y:?} {c:?}");
    }

    #[test]
    #[ignore = "real time"]
    fn timed_degenerate_rows_beside_an_ordinary_row() {
        let ordinary: Vec<(u64, TurnedBox)> = (0..40).map(|i| (i, frame(5.0 + i as f64 * 12.0, 0.0, 10.0, 10.0, 0.0))).collect();
        let touching: Vec<(u64, TurnedBox)> = (0..40).map(|i| (i, frame(5.0 + i as f64 * 10.0, 0.0, 10.0, 10.0, 0.0))).collect();
        let overlapping: Vec<(u64, TurnedBox)> = (0..40).map(|i| (i, frame(5.0 + i as f64 * 6.0, 0.0, 10.0, 10.0, 0.0))).collect();
        let nowidth: Vec<(u64, TurnedBox)> = (0..40).map(|i| (i, frame(i as f64 * 12.0, 0.0, 0.0, 10.0, 0.0))).collect();
        let s = only(&[Kind::Spacing, Kind::Edge, Kind::Centre]);
        let q = query(box_subject(frame(200.5, 0.0, 10.0, 10.0, 0.0)), &s);
        let yard = scene(&ordinary, &[], &[]);
        for (what, objs) in [("touching", &touching), ("overlapping", &overlapping), ("no width", &nowidth)] {
            let t = scene(objs, &[], &[]);
            let (y, c) = beside(&mut || drop(black_box(run(&yard, &q))), &mut || drop(black_box(run(&t, &q))));
            println!("{what}: yardstick {y:?}, case {c:?}, ratio {:.2}", ratio(c, y));
            assert!(ratio(c, y) < 6.0, "{what}: {y:?} {c:?}");
        }
    }

    #[test]
    #[ignore = "real time"]
    fn timed_a_cap_of_nothing_beside_the_default_caps() {
        let t = super::caps::scene_of_everything();
        let s = super::caps::settings();
        let base = query(box_subject(frame(40.3, 33.1, 3.0, 2.0, 0.0)), &s);
        for (name, set, ..) in super::caps::fields() {
            let mut budget = Budget::default();
            set(&mut budget, 0);
            let cut = craft_snap::Query { budget, ..base.clone() };
            let (y, c) = beside(&mut || drop(black_box(run(&t, &base))), &mut || drop(black_box(run(&t, &cut))));
            println!("{name}=0: yardstick {y:?}, case {c:?}, ratio {:.2}", ratio(c, y));
            assert!(ratio(c, y) < 3.0, "a cap that stops work never costs more than the work: {name}: {y:?} {c:?}");
        }
    }

    /// Two cases, each a hostile input and the ordinary input beside it, compared back to back.
    fn compare(name: &str, yardstick: (&Targets, &craft_snap::Query<'_>), case: (&Targets, &craft_snap::Query<'_>), most: f64) {
        let (y, c) = beside(&mut || drop(black_box(run(yardstick.0, yardstick.1))), &mut || drop(black_box(run(case.0, case.1))));
        println!("{name}: yardstick {y:?}, case {c:?}, ratio {:.2}", ratio(c, y));
        assert!(ratio(c, y) < most, "{name}: {y:?} {c:?}");
    }

    #[test]
    #[ignore = "real time"]
    fn timed_a_hundred_thousand_places_on_the_line_of_a_held_move_and_a_nudge_beside_forty() {
        let s = crate::travel::on_line();
        let on_a_line = |n: u64, from: f64, pitch: f64| {
            let places: Vec<(u64, f64, f64)> = (0..n).map(|i| (i + 1, from + i as f64 * pitch, 5.0)).collect();
            crate::travel::targets(&places, &[])
        };
        let (few, many) = (on_a_line(40, 49.0005, 2.0e-2), on_a_line(100_000, 5.0e-4, 1.0e-3));
        let f = frame(50.0, 5.0, 4.0, 2.0, 0.0);
        let held = craft_snap::Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(f), &s) };
        compare("a held move among places", (&few, &held), (&many, &held), 60.0);
        let nudge = query(Subject::Nudge { frame: f, bounds: None, axis: Axis::X, forward: true, reach: NudgeReach::Anywhere, fallback: 0.5 }, &s);
        compare("a nudge among places", (&few, &nudge), (&many, &nudge), 60.0);
    }

    #[test]
    #[ignore = "real time"]
    fn timed_a_box_wider_than_its_room_and_a_resize_that_would_collapse_beside_ordinary_ones() {
        let s = only(&[Kind::Spacing]);
        let two = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let (wide, centred) = (query(box_subject(frame(20.3, 0.0, 40.0, 10.0, 0.0)), &s), query(box_subject(frame(20.3, 0.0, 6.0, 10.0, 0.0)), &s));
        compare("wider than the room", (&two, &centred), (&two, &wide), 4.0);
        let g = only(&[Kind::Guide]);
        let (collapsing, plain) = (scene(&[], &[0.0], &[]), scene(&[], &[6.0], &[]));
        let edges = |at: f64| Subject::Edges { frame: frame(at / 2.0, 0.0, at, 10.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } };
        let (qc, qp) = (query(edges(1.5), &g), query(edges(5.2), &g));
        compare("a resize that would collapse", (&plain, &qp), (&collapsing, &qc), 4.0);
    }

    #[test]
    #[ignore = "real time"]
    fn timed_the_shortcuts_beside_ordinary_inputs() {
        // A window with nothing in it, against one with an object in it.
        let s = Settings { align_reach: Some(Reach::Document(50.0)), ..only(&[Kind::Guide, Kind::Edge]) };
        let (empty, busy) =
            (scene(&[(1, frame(1.0e6, 0.0, 10.0, 10.0, 0.0))], &[11.0], &[]), scene(&[(1, frame(30.0, 0.0, 10.0, 10.0, 0.0))], &[11.0], &[]));
        let q = query(box_subject(frame(0.0, 0.3, 20.0, 10.0, 0.0)), &s);
        compare("a window with nothing in it", (&busy, &q), (&empty, &q), 4.0);
        // A row of one object, against a row of two.
        let s = only(&[Kind::Spacing, Kind::Edge]);
        let (one, two) = (
            scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]),
            scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]),
        );
        let q = query(box_subject(frame(65.4, 0.4, 10.0, 10.0, 0.0)), &s);
        compare("a row of one object", (&two, &q), (&one, &q), 4.0);
        // An exact hit, against one that is a hair off.
        let s = only(&[Kind::Guide]);
        let t = scene(&[], &[5.0], &[20.0]);
        let (exact, off) = (query(box_subject(frame(10.0, 14.6, 10.0, 10.0, 0.0)), &s), query(box_subject(frame(10.3, 14.6, 10.0, 10.0, 0.0)), &s));
        compare("an exact hit", (&t, &off), (&t, &exact), 4.0);
    }

    #[test]
    #[ignore = "real time"]
    fn timed_a_path_of_two_million_elements_that_is_refused_beside_a_short_one() {
        // A path is charged for before it is looked at, and one the caps refuse is never copied: refused costs what a short one does, not a copy of it.
        let path = |n: usize| {
            let mut p = kurbo::BezPath::new();
            p.move_to((0.0, 0.0));
            (1..n).for_each(|i| p.line_to((i as f64 * 1.0e-3, ((i % 7) as f64) * 1.0e-3)));
            p
        };
        let with = |n: usize| {
            let mut b = TargetsBuilder::new(Limits::default());
            b.object(Id(1), Object { frame: frame(5.0, 5.0, 10.0, 10.0, 0.0), bounds: None, angle: None, parts: Parts::ALL }).unwrap();
            b.path(Id(1), path(n)).unwrap();
            b.build()
        };
        let (short, long) = (with(100), with(1_999_000));
        let s = Settings::default();
        let q = query(Subject::Point { at: Point::new(0.05, 0.002), base: Some(Point::new(3.0, 3.0)) }, &s);
        let (y, c) = beside(&mut || drop(black_box(run(&short, &q))), &mut || drop(black_box(run(&long, &q))));
        println!("a refused path: yardstick {y:?}, case {c:?}, ratio {:.2}", ratio(c, y));
        assert!(ratio(c, y) < 3.0, "{y:?} {c:?}");
    }
}
