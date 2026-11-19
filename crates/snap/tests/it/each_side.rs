//! A line mark that runs from the nearest target on one side of the subject to the nearest on the other (`LineSpan::EachSide`), against `Every`
//! and `Nearest`, and against a scan of every span.

use craft_snap::{Anchors, Axis, Frames, Id, Kind, Limits, LineSpan, Mark, Settings, Subject, Targets, TargetsBuilder};
use proptest::prelude::*;

use crate::common::*;

/// Objects 10 wide whose right sides are on x = 50, one for each span along y, and guides on the same line with a span each: two kinds at one place.
fn column_with(spans: &[(f64, f64)], guides: &[(f64, f64)], dx: f64, mirror: bool) -> Targets {
    let sign = if mirror { -1.0 } else { 1.0 };
    let objs: Vec<(u64, craft_snap::TurnedBox)> =
        spans.iter().enumerate().map(|(i, (lo, hi))| (i as u64 + 1, frame(45.0 + dx, sign * 0.5 * (lo + hi), 10.0, hi - lo, 0.0))).collect();
    let mut b = TargetsBuilder::new(Limits::default());
    for (id, f) in &objs {
        b.object(Id(*id), object(*f)).unwrap();
    }
    for (i, (lo, hi)) in guides.iter().enumerate() {
        let (a, z) = if mirror { (-hi, -lo) } else { (*lo, *hi) };
        b.line(Some(Id(1000 + i as u64)), Axis::X, 50.0 + dx, Some((a, z)), Kind::Guide).unwrap();
    }
    b.build()
}

fn column(spans: &[(f64, f64)], dx: f64, mirror: bool) -> Targets {
    column_with(spans, &[], dx, mirror)
}

/// Only lines pull: a corner of the subject near a corner of a target would take both its degrees, and the subject would not be where it was put.
fn settings(span: LineSpan) -> Settings {
    let mut s = Settings { line_span: span, frames: Frames { page: true, own: false }, ..Settings::default() };
    for k in [Kind::Corner, Kind::SideMiddle, Kind::Spacing, Kind::Size, Kind::Vertex, Kind::Midpoint, Kind::Nearest, Kind::Intersection] {
        let mut r = s.kinds.rule(k);
        r.on = false;
        s.kinds.set(k, r);
    }
    s
}

/// The subject: 10 wide with its right side on x = 50 and the extent `ext` along y; only that side snaps.
fn drag(t: &Targets, s: &Settings, ext: (f64, f64), dx: f64, mirror: bool) -> craft_snap::Snapped {
    let sign = if mirror { -1.0 } else { 1.0 };
    let q = query(
        Subject::Box {
            frame: frame(45.0 + dx, sign * 0.5 * (ext.0 + ext.1), 10.0, ext.1 - ext.0, 0.0),
            bounds: None,
            u: Anchors { min: false, mid: false, max: true },
            v: Anchors::NONE,
            own: &[],
        },
        s,
    );
    let r = run(t, &q);
    r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
    r
}

/// The extent along y of the mark on the line of the right sides (the targets' edges or guides).
fn reach(r: &craft_snap::Snapped, mirror: bool) -> Option<(f64, f64)> {
    r.marks.iter().find_map(|m| match m {
        Mark::Line { segment, kinds, .. } if kinds.iter().any(|k| matches!(k, Kind::Edge | Kind::Guide)) => {
            let (a, b) = (segment.p0.y.min(segment.p1.y), segment.p0.y.max(segment.p1.y));
            Some(if mirror { (-b, -a) } else { (a, b) })
        }
        _ => None,
    })
}

/// What a mark from the nearest target on each side reaches, from the definition and a scan of every span: of those that start before the subject, the
/// one or ones with the least gap to it; of those that end past it, the same; and the subject's own extent.
fn expected(spans: &[(f64, f64)], ext: (f64, f64)) -> (f64, f64) {
    let (a, b) = ext;
    let (mut lo, mut hi) = ext;
    let mut take = |on: Vec<&(f64, f64)>, gap: &dyn Fn(&(f64, f64)) -> f64| {
        let least = on.iter().map(|s| gap(s)).fold(f64::INFINITY, f64::min);
        for s in on.into_iter().filter(|s| gap(s) == least) {
            (lo, hi) = (lo.min(s.0), hi.max(s.1));
        }
    };
    take(spans.iter().filter(|s| s.0 < a).collect(), &|s| (a - s.1).max(0.0));
    take(spans.iter().filter(|s| s.1 > b).collect(), &|s| (s.0 - b).max(0.0));
    (lo, hi)
}

#[test]
fn a_subject_between_two_targets_is_marked_to_both_where_nearest_reaches_one_and_every_reaches_them_all() {
    // The subject is at y 130 to 140. On its low side are targets at 95 to 105 and -105 to -95; on its high side, 175 to 185 and 295 to 305.
    let spans = [(-105.0, -95.0), (95.0, 105.0), (175.0, 185.0), (295.0, 305.0)];
    let t = column(&spans, 0.0, false);
    let every = reach(&drag(&t, &settings(LineSpan::Every), (130.0, 140.0), 0.0, false), false).unwrap();
    assert_eq!(every, (-105.0, 305.0));
    let nearest = reach(&drag(&t, &settings(LineSpan::Nearest), (130.0, 140.0), 0.0, false), false).unwrap();
    assert_eq!(nearest, (95.0, 140.0), "one side only: the nearer of the two nearest targets, 25 away against 35");
    let each = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(reach(&each, false), Some((95.0, 185.0)), "from the nearest below to the nearest above, through the subject");
    let count =
        each.marks.iter().find_map(|m| if let Mark::Line { count, kinds, .. } = m { kinds.contains(&Kind::Edge).then_some(*count) } else { None });
    assert_eq!(count, Some(4), "a target the mark does not reach is still counted");
}

#[test]
fn with_targets_on_one_side_only_the_mark_runs_to_the_nearest_of_those() {
    let t = column(&[(175.0, 185.0), (295.0, 305.0), (500.0, 501.0)], 0.0, false);
    let r = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(reach(&r, false), Some((130.0, 185.0)));
    let t = column(&[(-105.0, -95.0), (-300.0, -290.0)], 0.0, false);
    assert_eq!(reach(&drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false), false), Some((-105.0, 140.0)));
}

#[test]
fn a_target_that_overlaps_the_subject_is_the_nearest_on_its_side_and_one_inside_it_is_on_neither() {
    // The subject is 130 to 140. 120 to 135 starts before it and reaches it; 132 to 138 is inside it; 138 to 150 ends past it and starts inside; 160 to 170
    // is the next on the right if nothing overlaps, and 100 to 110 the next on the left.
    let spans = [(120.0, 135.0), (132.0, 138.0), (138.0, 150.0), (160.0, 170.0), (100.0, 110.0)];
    let t = column(&spans, 0.0, false);
    let r = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(
        reach(&r, false),
        Some((120.0, 150.0)),
        "the overlapping ones are the nearest on their sides, so 100 to 110 and 160 to 170 are not reached"
    );
    // One target that covers the subject is the nearest on both sides.
    let t = column(&[(100.0, 200.0), (300.0, 310.0), (0.0, 10.0)], 0.0, false);
    assert_eq!(reach(&drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false), false), Some((100.0, 200.0)));
    // Touching is overlapping: the target that ends where the subject starts has no gap.
    let t = column(&[(110.0, 130.0), (60.0, 70.0), (140.0, 141.0), (300.0, 310.0)], 0.0, false);
    assert_eq!(reach(&drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false), false), Some((110.0, 141.0)));
}

#[test]
fn targets_the_same_distance_away_are_all_reached_and_the_others_are_not() {
    // Two targets end 20 below the subject (one long, one short): both are the nearest, and the join reaches the long one's far end.
    let t = column(&[(0.0, 110.0), (60.0, 110.0), (30.0, 40.0), (200.0, 210.0), (200.0, 260.0)], 0.0, false);
    let r = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(reach(&r, false), Some((0.0, 260.0)));
}

#[test]
fn targets_of_different_kinds_at_one_place_are_chosen_between_together() {
    // A box's span and a guide's, on one line: the guide touches the subject from below (110 to 130) and the box overlaps it (125 to 135), so both are the
    // nearest below and the mark runs to the lower of their starts; a guide far above and a box nearer above: the nearer one only.
    let t = column_with(&[(125.0, 135.0), (170.0, 180.0)], &[(110.0, 130.0), (300.0, 310.0)], 0.0, false);
    let r = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(reach(&r, false), Some((110.0, 180.0)));
    // The same with nothing touching: the nearest below is a guide, the nearest above a box.
    let t = column_with(&[(60.0, 70.0), (160.0, 170.0)], &[(100.0, 120.0), (200.0, 210.0), (0.0, 5.0)], 0.0, false);
    let r = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(reach(&r, false), Some((100.0, 170.0)));
    // A guide that ends where the subject starts and one that overlaps it, both guides' spans from one place, with a box beyond: the lower start.
    let t = column_with(&[(300.0, 310.0)], &[(90.0, 130.0), (128.0, 133.0)], 0.0, false);
    let r = drag(&t, &settings(LineSpan::EachSide), (130.0, 140.0), 0.0, false);
    assert_eq!(reach(&r, false), Some((90.0, 310.0)));
}

#[test]
fn the_setting_changes_nothing_for_guides_with_no_extent_and_leaves_nearest_as_it_was() {
    // A guide runs for ever: there is no span to run to, and the mark is the subject's own side under every setting.
    let mut b = TargetsBuilder::new(Limits::default());
    b.line(Some(Id(1)), Axis::X, 50.0, None, Kind::Guide).unwrap();
    let t = b.build();
    let q = |span| {
        let s = settings(span);
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
        r.marks.iter().find_map(|m| {
            if let Mark::Line { segment, .. } = m { Some((segment.p0.y.min(segment.p1.y), segment.p0.y.max(segment.p1.y))) } else { None }
        })
    };
    assert_eq!(q(LineSpan::EachSide), q(LineSpan::Nearest));
    assert_eq!(q(LineSpan::EachSide), q(LineSpan::Every));
}

/// A fixed seed, so that a run is the same run each time; `CRAFT_SNAP_CASES` and `CRAFT_SNAP_SEED` run a longer search, and each find is a case to read.
fn config() -> ProptestConfig {
    let number = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(default);
    ProptestConfig {
        cases: u32::try_from(number("CRAFT_SNAP_CASES", 1500)).unwrap_or(1500),
        rng_seed: proptest::test_runner::RngSeed::Fixed(number("CRAFT_SNAP_SEED", 0xe5)),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// The mark equals the one the definition gives from a scan of every span, on a lattice where touching and equal gaps come by themselves, and the same
    /// scene mirrored, and moved a long way, gives the mirrored and the moved mark.
    #[test]
    fn the_mark_is_what_a_scan_of_every_span_gives_mirrored_and_moved(
        spans in prop::collection::vec((-20i32..20, 1i32..8), 0..14),
        guides in prop::collection::vec((-20i32..20, 1i32..8), 0..5),
        ext in (-20i32..20, 1i32..8),
        far in prop::sample::select(vec![0.0f64, 1.0e3, 1.0e6]),
    ) {
        let spans: Vec<(f64, f64)> = spans.iter().map(|(lo, len)| (f64::from(*lo), f64::from(*lo) + f64::from(*len))).collect();
        let guides: Vec<(f64, f64)> = guides.iter().map(|(lo, len)| (f64::from(*lo), f64::from(*lo) + f64::from(*len))).collect();
        let every_span: Vec<(f64, f64)> = spans.iter().chain(&guides).copied().collect();
        let ext = (f64::from(ext.0), f64::from(ext.0) + f64::from(ext.1));
        let want = expected(&every_span, ext);
        let s = settings(LineSpan::EachSide);
        let plain = drag(&column_with(&spans, &guides, 0.0, false), &s, ext, 0.0, false);
        let got = reach(&plain, false);
        // With nothing on the line there is no mark to draw.
        prop_assert_eq!(got, (!every_span.is_empty()).then_some(want), "spans {:?} guides {:?} ext {:?}", spans, guides, ext);
        // Mirrored: the same scene, y negated.
        let mirrored = drag(&column_with(&spans, &guides, 0.0, true), &s, ext, 0.0, true);
        prop_assert_eq!(reach(&mirrored, true), got);
        // The same scene far from the origin (along x, so that the extents stay exact): the same mark.
        if far > 0.0 {
            let moved = settings(LineSpan::EachSide);
            let t = column_with(&spans, &guides, far, false);
            let q = query(
                Subject::Box { frame: frame(45.0 + far, 0.5 * (ext.0 + ext.1), 10.0, ext.1 - ext.0, 0.0), bounds: None, u: Anchors { min: false, mid: false, max: true }, v: Anchors::NONE, own: &[] },
                &moved,
            );
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            prop_assert_eq!(reach(&r, false), got);
        }
        // `Nearest` reaches a subset of what `EachSide` does, and `Every` a superset.
        let near = reach(&drag(&column_with(&spans, &guides, 0.0, false), &settings(LineSpan::Nearest), ext, 0.0, false), false);
        let every = reach(&drag(&column_with(&spans, &guides, 0.0, false), &settings(LineSpan::Every), ext, 0.0, false), false);
        if let (Some(g), Some(n), Some(e)) = (got, near, every) {
            prop_assert!(g.0 <= n.0 && n.1 <= g.1, "nearest {:?} within each side {:?}", n, g);
            prop_assert!(e.0 <= g.0 && g.1 <= e.1, "each side {:?} within every {:?}", g, e);
        }
    }
}

#[test]
fn a_hundred_thousand_members_at_one_place_cost_a_few_bisections_and_reach_the_nearest_on_each_side() {
    let mut b = TargetsBuilder::new(Limits::default());
    // 100,000 guides on one line, each a span of 1 at every third unit: the subject is in the middle of them.
    for i in 0..100_000u64 {
        b.line(Some(Id(i)), Axis::X, 50.0, Some((i as f64 * 3.0, i as f64 * 3.0 + 1.0)), Kind::Guide).unwrap();
    }
    let t = b.build();
    assert_eq!(t.report().line_places, 1);
    let run_with = |span| {
        let s = settings(span);
        let q = query(
            Subject::Box {
                frame: frame(45.0, 150_001.0, 10.0, 10.0, 0.0),
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
    let each = run_with(LineSpan::EachSide);
    // The subject spans 149,996 to 150,006. Below it the member that ends nearest is 49,998 (149,994 to 149,995); the one that starts where the subject ends
    // and reaches past it is 50,002 (150,006 to 150,007), so the mark runs from 149,994 to 150,007.
    let (lo, hi) = reach(&each, false).unwrap();
    assert_eq!((lo, hi), (149_994.0, 150_007.0));
    let (el, eh) = reach(&every, false).unwrap();
    assert!(el <= 1.0 && eh >= 299_990.0);
    let (nl, nh) = reach(&nearest, false).unwrap();
    assert!(lo <= nl && nh <= hi);
    // The same entries are walked, and the members cost bisections: five an entry, 17 steps for 100,000 members each.
    assert_eq!(each.work.line_steps, every.work.line_steps);
    assert!(each.work.bisect_steps <= every.work.bisect_steps + 5 * 17 * 4, "{} against {}", each.work.bisect_steps, every.work.bisect_steps);
    assert!(each.work.bisect_steps > every.work.bisect_steps, "the five bisections of the members are charged");
    assert!(!each.work.truncated, "{:?}", each.work);
}
