//! Counted cost tests: each hostile input of the design holds the work to a number near the measured value, counted in the units the loops walk.
//! No test here reads a clock.

use craft_snap::{
    Anchors, Axis, Budget, Constraint, Id, Kind, Limits, Matching, Object, Parts, Settings, SpacingReach, Subject, Targets, TargetsBuilder,
};
use kurbo::Point;

use crate::common::*;

fn big(n: u64, turned: impl Fn(u64) -> f64) -> Targets {
    let mut b = TargetsBuilder::new(Limits::default());
    for i in 0..n {
        let (x, y) = ((i % 400) as f64 * 9.0, (i / 400) as f64 * 9.0);
        b.object(
            Id(i),
            Object {
                frame: frame(x, y, 6.0, 4.0, turned(i)),
                bounds: None,
                angle: None,
                parts: Parts { bounds_lines: true, own_lines: true, ..Parts::NONE },
            },
        )
        .unwrap();
    }
    b.build()
}

fn align_only() -> Settings {
    let mut s = Settings::default();
    for k in [
        Kind::Corner,
        Kind::SideMiddle,
        Kind::Spacing,
        Kind::Size,
        Kind::Angle,
        Kind::Point(0),
        Kind::Vertex,
        Kind::Midpoint,
        Kind::Nearest,
        Kind::Intersection,
    ] {
        let mut r = s.kinds.rule(k);
        r.on = false;
        s.kinds.set(k, r);
    }
    s
}

fn probe(t: &Targets, s: &Settings, at: (f64, f64)) -> craft_snap::Work {
    let q = query(Subject::Box { frame: frame(at.0, at.1, 5.0, 3.0, 0.3), bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &[] }, s);
    let r = run(t, &q);
    r.check(t, &q).unwrap();
    r.work
}

#[test]
fn a_hundred_thousand_objects_cost_a_logarithm_more_than_a_hundred_and_gather_nothing() {
    let s = align_only();
    let (small, large) = (big(100, |i| (i % 5) as f64 * 0.2), big(100_000, |i| (i % 5) as f64 * 0.2));
    assert_eq!(large.report().objects, 100_000);
    let (a, b) = (probe(&small, &s, (500.0, 20.0)), probe(&large, &s, (500.0, 20.0)));
    // Alignment reads the sorted arrays only: no bucket entry is visited, however many objects there are.
    assert_eq!((a.visited, b.visited, b.examined, b.candidates), (0, 0, 0, 0));
    assert!(b.made.has(craft_snap::Made::OBJECT_LINES) && !b.made.has(craft_snap::Made::BUCKETS));
    // The bisection steps grow with the logarithm of the lines, and by no more than the lookups made times the extra bits.
    let extra = b.bisect_steps - a.bisect_steps;
    assert!(extra > 0 && extra <= 12 * 4 * 14, "{} -> {}", a.bisect_steps, b.bisect_steps);
    assert!(b.bisect_steps < 4_000, "{}", b.bisect_steps);
}

#[test]
fn a_hundred_thousand_objects_at_a_hundred_thousand_angles_are_found_by_bisection_among_the_directions() {
    let s = align_only();
    let t = big(100_000, |i| 0.001 + i as f64 * 1.0e-5);
    let w = probe(&t, &s, (500.0, 20.0));
    assert!(w.bisect_steps < 5_000 && w.line_steps < 200, "{w:?}");
    assert_eq!(w.visited, 0);
}

#[test]
fn the_same_scene_moved_far_gives_the_same_hits_and_marks() {
    let mut reference = None;
    for far in [0.0, 1.0e6, 1.0e9, 1.0e11] {
        let objs: Vec<(u64, craft_snap::TurnedBox)> = (0..30u64)
            .map(|i| {
                (
                    i,
                    frame(
                        far + (i % 6) as f64 * 12.0,
                        far + (i / 6) as f64 * 12.0,
                        8.0,
                        6.0,
                        if i % 7 == 0 { std::f64::consts::FRAC_PI_2 } else { 0.0 },
                    ),
                )
            })
            .collect();
        let t = scene(&objs, &[far + 3.0], &[far + 7.0]);
        let s = Settings::default();
        let q = query(box_subject(frame(far + 27.5, far + 20.25, 8.0, 6.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let shape = (r.hits.iter().map(|h| (h.kind, h.id, h.side)).collect::<Vec<_>>(), r.marks.len(), r.delta);
        match &reference {
            None => reference = Some(shape),
            Some((hits, marks, delta)) => {
                assert_eq!((&shape.0, shape.1), (hits, *marks), "{far}");
                assert!((shape.2 - *delta).hypot() <= 1.0e-4_f64.max(far * 1.0e-15), "{far}: {:?} {:?}", shape.2, delta);
            }
        }
    }
}

#[test]
fn a_stack_of_boxes_on_one_spot_is_cut_at_the_gather_caps_and_says_so() {
    let mut b = TargetsBuilder::new(Limits::default());
    for i in 0..2_500u64 {
        b.object(Id(i), Object { frame: frame(10.0, 10.0, 4.0, 4.0, 0.0), bounds: None, angle: None, parts: Parts::ALL }).unwrap();
    }
    let t = b.build();
    let s = Settings { align_reach: Some(craft_snap::Reach::Document(50.0)), ..Settings::default() };
    let q = query(box_subject(frame(11.0, 11.0, 4.0, 4.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.work.visited <= craft_snap::MAX_GATHERS * 4_096 && r.work.candidates <= craft_snap::MAX_GATHERS * 64, "{:?}", r.work);
    assert!(r.work.truncated && (r.work.cut.has(craft_snap::Cuts::CANDIDATES) || r.work.cut.has(craft_snap::Cuts::WINDOW_VISITS)));
}

#[test]
fn a_row_of_ten_thousand_equal_gaps_is_cut_at_the_row_cap_and_the_marks_at_theirs() {
    let objs: Vec<(u64, craft_snap::TurnedBox)> = (0..10_000u64).map(|i| (i, frame(i as f64 * 15.0, 0.0, 10.0, 10.0, 0.0))).collect();
    let t = scene(&objs, &[], &[]);
    let s = Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 200_000.0 }, ..Settings::default() };
    // Between the 5,000th and the 5,001st: a gap of 5 there, a box 5 wide would sit centred.
    let q = query(box_subject(frame(5_000.0 * 15.0 + 7.5, 0.0, 4.0, 4.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.work.row <= craft_snap::MAX_GATHERS * 256, "{:?}", r.work);
    assert!(r.work.truncated, "a window cut to the buckets round its middle says so: {:?}", r.work);
    assert!(r.marks.len() <= 64 + 8, "{}", r.marks.len());
}

#[test]
fn two_nearly_parallel_directions_never_throw_the_box_far_along_a_line() {
    let t = scene(&[(1, frame(0.0, 0.0, 20.0, 20.0, 1.0e-6))], &[30.0], &[]);
    let s = Settings::default();
    let q = query(box_subject(frame(33.0, 100.0, 10.0, 4.0, 2.0e-6)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.delta.hypot() <= 6.0 + 1e-9, "{:?}", r.delta);
}

#[test]
fn a_boundary_pull_is_inclusive_at_every_zoom_along_a_page_axis_and_a_turned_one() {
    let a = 30f64.to_radians();
    for zoom in [0.5, 1.0, 2.0, 8.0] {
        let d = 6.0 / zoom;
        for (angle, dirs) in [(0.0, Some(craft_snap::Direction::X)), (a, craft_snap::Direction::new(a))] {
            let mut b = TargetsBuilder::new(Limits::default());
            let dir = dirs.unwrap();
            b.line_at(Some(Id(1)), dir, 100.0 + d, None, Kind::Guide).unwrap();
            let t = b.build();
            let s = Settings::default();
            // A point whose offset along the direction is 100: on the line's normal.
            let n = dir.normal();
            let p = Point::new(100.0 * n.x, 100.0 * n.y);
            let _ = angle;
            let near = craft_snap::Query { zoom, ..query(Subject::Point { at: p, base: None }, &s) };
            assert!(run(&t, &near).hits.iter().any(|h| h.kind == Kind::Guide), "{zoom}: at the boundary");
            let mut b = TargetsBuilder::new(Limits::default());
            b.line_at(Some(Id(1)), dir, 100.0 + d * (1.0 + 1.0e-9), None, Kind::Guide).unwrap();
            let t = b.build();
            let past = craft_snap::Query { zoom, ..query(Subject::Point { at: p, base: None }, &s) };
            assert!(run(&t, &past).hits.is_empty(), "{zoom}: past it");
        }
    }
}

#[test]
fn insertion_order_does_not_change_a_result_to_the_bit() {
    let objs: Vec<(u64, craft_snap::TurnedBox)> = (0..60u64)
        .map(|i| (i, frame(((i * 37) % 23) as f64 * 5.0, ((i * 11) % 17) as f64 * 5.0, 3.0 + (i % 4) as f64, 3.0, (i % 3) as f64 * 0.4)))
        .collect();
    let mut rev = objs.clone();
    rev.reverse();
    let mut shuffled = objs.clone();
    shuffled.rotate_left(17);
    let s = Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 500.0 }, ..Settings::default() };
    let result = |list: &[(u64, craft_snap::TurnedBox)]| {
        let t = scene(list, &[40.0, 12.5], &[33.0]);
        let q = query(box_subject(frame(61.0, 41.5, 6.0, 4.0, 0.0)), &s);
        run(&t, &q)
    };
    let base = result(&objs);
    assert_eq!(base, result(&rev));
    assert_eq!(base, result(&shuffled));
    assert!(!base.hits.is_empty());
}

#[test]
fn a_quarter_turned_subject_is_the_unturned_one_with_its_sides_swapped_and_angle_zero_is_the_same_with_or_without_own_frames() {
    let t = scene(&[(1, frame(100.0, 100.0, 20.0, 10.0, 0.0)), (2, frame(130.0, 90.0, 6.0, 6.0, 0.0))], &[96.0], &[104.0]);
    let s = Settings::default();
    let tall = query(box_subject(frame(105.0, 102.0, 4.0, 12.0, 0.0)), &s);
    let turned = query(box_subject(frame(105.0, 102.0, 12.0, 4.0, std::f64::consts::FRAC_PI_2)), &s);
    let (a, b) = (run(&t, &tall), run(&t, &turned));
    assert_eq!(a.delta, b.delta);
    assert_eq!(a.hits.iter().map(|h| (h.kind, h.id)).collect::<Vec<_>>(), b.hits.iter().map(|h| (h.kind, h.id)).collect::<Vec<_>>());
    let no_own = Settings { frames: craft_snap::Frames { page: true, own: false }, ..Settings::default() };
    let with = run(&t, &tall);
    let without = run(&t, &query(box_subject(frame(105.0, 102.0, 4.0, 12.0, 0.0)), &no_own));
    assert_eq!((with.delta, with.hits), (without.delta, without.hits));
}

#[test]
fn mirroring_x_mirrors_the_result_and_costs_the_same() {
    let objs: Vec<(u64, craft_snap::TurnedBox)> =
        (0..40u64).map(|i| (i, frame(((i * 29) % 31) as f64 * 4.0 + 0.5, ((i * 7) % 13) as f64 * 5.0, 3.0 + (i % 5) as f64, 4.0, 0.0))).collect();
    let mirrored: Vec<(u64, craft_snap::TurnedBox)> =
        objs.iter().map(|(i, f)| (*i, craft_snap::TurnedBox { centre: Point::new(-f.centre.x, f.centre.y), ..*f })).collect();
    let s = Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 500.0 }, matching: Matching::AnyToAny, ..Settings::default() };
    let run_on = |list: &[(u64, craft_snap::TurnedBox)], gx: f64, x: f64| {
        let t = scene(list, &[gx], &[]);
        let q = query(box_subject(frame(x, 21.3, 5.0, 3.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    };
    for x in [10.2, 33.7, 61.1, 90.9] {
        let (a, b) = (run_on(&objs, 40.0, x), run_on(&mirrored, -40.0, -x));
        assert!((a.delta.x + b.delta.x).abs() < 1e-9 && a.delta.y == b.delta.y, "{x}: {:?} {:?}", a.delta, b.delta);
        assert_eq!((a.work.bisect_steps, a.work.row, a.work.gathers), (b.work.bisect_steps, b.work.row, b.work.gathers), "{x}");
    }
}

#[test]
fn the_index_gives_what_a_scan_of_every_line_gives() {
    // Guides and edges and centres only, one direction: the nearest line within reach, by a scan.
    let mut s = align_only();
    s.frames = craft_snap::Frames { page: true, own: false };
    for k in [Kind::Vertex, Kind::Midpoint, Kind::Nearest, Kind::Intersection] {
        let mut r = s.kinds.rule(k);
        r.on = false;
        s.kinds.set(k, r);
    }
    let objs: Vec<(u64, craft_snap::TurnedBox)> = (0..200u64)
        .map(|i| (i, frame(((i * 7919) % 1009) as f64 * 0.37, ((i * 104_729) % 997) as f64 * 0.41, 1.0 + (i % 6) as f64, 2.0 + (i % 5) as f64, 0.0)))
        .collect();
    let t = scene(&objs, &[12.3, 80.7, 150.0], &[]);
    // Anywhere (the sorted arrays), and within a reach across that holds every object (the gather, with room for all of them).
    for reach in [None, Some(craft_snap::Reach::Document(60.0))] {
        let s = Settings { align_reach: reach, ..s.clone() };
        for i in 0..60 {
            let x = f64::from(i) * 6.1 + 0.77;
            // Within a reach across, the subject sits among the objects (they run to about 370 along x and 410 along y); anywhere, far off.
            let y0 = if reach.is_some() { 200.0 } else { 1000.0 };
            let f = frame(x, y0, 4.0, 2.0, 0.0);
            let q = craft_snap::Query {
                constraint: Constraint::Axis(Axis::X),
                ..query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::NONE, own: &[] }, &s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            let sides = [x - 2.0, x, x + 2.0];
            let mut lines: Vec<f64> = vec![12.3, 80.7, 150.0];
            for (_, o) in &objs {
                // Within a reach, only the objects whose boxes meet the subject's box grown by it.
                let near = reach.is_none()
                    || ((o.centre.x - x).abs() <= o.size.width / 2.0 + 2.0 + 60.0 && (o.centre.y - y0).abs() <= o.size.height / 2.0 + 1.0 + 60.0);
                if near {
                    lines.extend([o.centre.x - o.size.width / 2.0, o.centre.x, o.centre.x + o.size.width / 2.0]);
                }
            }
            let want =
                sides.iter().flat_map(|s| lines.iter().map(move |l| l - s)).filter(|d| d.abs() <= 6.0).min_by(|a, b| a.abs().total_cmp(&b.abs()));
            match want {
                Some(d) => assert!((r.delta.x - d).abs() < 1e-9 || r.hits.iter().any(|h| h.kind == Kind::Guide), "{x}: scan {d}, got {:?}", r.delta),
                None => assert!(r.hits.is_empty(), "{x}: {:?}", r.hits),
            }
        }
    }
}

#[test]
fn every_counter_is_at_most_its_cap_and_truncated_says_exactly_when_a_cap_was_met() {
    let objs: Vec<(u64, craft_snap::TurnedBox)> =
        (0..3_000u64).map(|i| (i, frame(((i * 37) % 53) as f64 * 0.5, ((i * 11) % 47) as f64 * 0.5, 1.0, 1.0, (i % 3) as f64 * 0.2))).collect();
    let t = scene(&objs, &[3.0], &[4.0]);
    for (cap, label) in [
        (Budget::default(), "default"),
        (Budget { window_visits: 40, candidates: 5, row: 7, marks: 3, points: 20, solves: 4, line_steps: 9, ..Budget::default() }, "small"),
    ] {
        let s = Settings {
            align_reach: Some(craft_snap::Reach::Document(40.0)),
            spacing_reach: SpacingReach { times: 4.0, at_least: 100.0 },
            ..Settings::default()
        };
        let q = craft_snap::Query { budget: cap, ..query(box_subject(frame(12.2, 9.1, 3.0, 2.0, 0.2)), &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let w = &r.work;
        assert!(w.gathers <= craft_snap::MAX_GATHERS, "{label}: {w:?}");
        assert!(w.visited <= w.gathers * u64::from(cap.window_visits), "{label}: {w:?}");
        assert!(w.candidates <= w.gathers * u64::from(cap.candidates) && w.row <= w.gathers * u64::from(cap.row), "{label}: {w:?}");
        assert!(
            w.points <= u64::from(cap.points) && w.solves <= u64::from(cap.solves) && w.line_steps <= u64::from(cap.line_steps),
            "{label}: {w:?}"
        );
        assert!(r.marks.len() as u64 <= u64::from(cap.marks), "{label}: {} marks", r.marks.len());
        assert_eq!(w.truncated, w.cut.any(), "{label}: truncated is exactly some cap met");
    }
}

/// The body of the child process of the memory test: a build at the default limits, every object turned, and one query.
#[test]
fn child_of_the_memory_test() {
    if std::env::var_os("CRAFT_SNAP_CHILD").is_none() {
        return;
    }
    let mut b = TargetsBuilder::new(Limits::default());
    let mut refused = 0;
    for i in 0..200_010u64 {
        let f = frame((i % 500) as f64 * 9.0, (i / 500) as f64 * 9.0, 6.0, 4.0, 0.3 + (i % 7) as f64 * 0.1);
        if b.object(Id(i), Object { frame: f, bounds: None, angle: None, parts: Parts { bounds_lines: true, own_lines: true, ..Parts::NONE } })
            .is_err()
        {
            refused += 1;
        }
    }
    let t = b.build();
    let s = align_only();
    let w = probe(&t, &s, (500.0, 20.0));
    println!(
        "CHILD objects={} refused={} over_limit={} places_query_bisect={}",
        t.report().objects,
        refused,
        t.report().refused_over_limit,
        w.bisect_steps
    );
}

#[test]
fn a_build_at_the_default_limits_with_every_object_turned_fits_in_a_child_under_a_memory_limit_and_refuses_past_them() {
    let exe = std::env::current_exe().unwrap();
    // 3 GB of address space for the child: the build holds at most 200,000 objects, a hundred bytes each, and twelve transient lines each.
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("ulimit -v 3000000 && exec \"{}\" --exact cost::child_of_the_memory_test --nocapture --test-threads 1", exe.display()))
        .env("CRAFT_SNAP_CHILD", "1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("CHILD objects=200000 refused=10 over_limit=10"), "{text}");
}

/// Prints the counts the README quotes. Run with `cargo test -p craft-snap --test it measure -- --ignored --nocapture`.
#[test]
#[ignore = "prints the measured costs the README quotes"]
fn measure() {
    let s = align_only();
    for n in [100u64, 10_000, 100_000] {
        let t = big(n, |i| (i % 5) as f64 * 0.2);
        let w = probe(&t, &s, (500.0, 20.0));
        let far = probe(&t, &s, (-1.0e6, -1.0e6));
        println!(
            "objects {n}: lines {} places {} | query: bisect_steps {} line_steps {} gathers {} | far query: bisect_steps {}",
            n * 12,
            t.report().line_places,
            w.bisect_steps,
            w.line_steps,
            w.gathers,
            far.bisect_steps
        );
    }
    let t = big(100_000, |i| 0.001 + i as f64 * 1.0e-5);
    let w = probe(&t, &s, (500.0, 20.0));
    println!("100,000 angles: bisect_steps {} line_steps {}", w.bisect_steps, w.line_steps);
    let full = Settings::default();
    let t = big(5_000, |i| (i % 5) as f64 * 0.2);
    let w = probe(&t, &full, (500.0, 20.0));
    println!("5,000 objects, default settings: {w:?}");
}
