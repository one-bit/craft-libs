//! Two properties that no single scene shows: a scene turned as a whole gives the result turned by the same angle, and a scene moved a long way
//! gives the same hits and marks.

use craft_snap::{Anchors, Constraint, Edges, Frames, Mark, Settings, Snapped, Subject, Targets, TargetsBuilder, TurnedBox};
use kurbo::{Point, Vec2};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

use crate::common::*;

static ANGLES: [f64; 3] = [0.0, 0.3, 0.7];
/// A shape that is not square to the page has its own frame; one that is has none to add to the page's.
static TURNED: [f64; 3] = [0.3, 0.7, 1.1];

/// An object on a grid of halves, so that equal sizes, equal gaps and shared lines come by themselves: (x, y, width, height, angle).
fn on_grid(angles: &'static [f64]) -> impl Strategy<Value = (f64, f64, f64, f64, f64)> {
    (-24i32..24, -24i32..24, 2u32..12, 2u32..8, prop::sample::select(angles.to_vec()))
        .prop_map(|(x, y, w, h, a)| (f64::from(x) * 0.5, f64::from(y) * 0.5, f64::from(w) * 0.5, f64::from(h) * 0.5, a))
}

/// The subject, off the grid, so that no two candidates are exactly as far as each other.
fn off_grid(angles: &'static [f64]) -> impl Strategy<Value = (f64, f64, f64, f64, f64)> {
    (-12.0f64..12.0, -12.0f64..12.0, 1.0f64..6.0, 1.0f64..6.0, prop::sample::select(angles.to_vec()))
}

fn turned(f: &TurnedBox, a: f64, about: Point) -> TurnedBox {
    let (s, c) = a.sin_cos();
    let d = f.centre - about;
    TurnedBox { centre: about + Vec2::new(c * d.x - s * d.y, s * d.x + c * d.y), size: f.size, angle: f.angle + a }
}

fn rotate(v: Vec2, a: f64) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

fn build(objects: &[TurnedBox]) -> Targets {
    let numbered: Vec<(u64, TurnedBox)> = objects.iter().enumerate().map(|(i, f)| (i as u64, *f)).collect();
    scene(&numbered, &[], &[])
}

/// What a result says, without where it says it and without which of two lines at one place is called the hit: how far each hit pulled (a number
/// that does not change with where the scene is or how it is turned), and each mark with its kind, its target and its length, in an order that does
/// not depend on where the scene is. The ids of the neighbours of a gap are left out for the same reason as the names of hits: an equal gap has two
/// ends, and which is named follows the order along a direction.
fn describe(r: &Snapped) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = r.hits.iter().map(|h| ("hit".to_string(), h.distance)).collect();
    for m in &r.marks {
        out.push(match m {
            Mark::Line { kinds, first, count, .. } => {
                let mut k: Vec<String> = kinds.iter().map(|k| format!("{k:?}")).collect();
                k.sort();
                (format!("line {k:?} {first:?} {count}"), 0.0)
            }
            Mark::Gap { value, subject, .. } => (format!("gap {subject}"), *value),
            Mark::Size { value, matched, count, .. } => (format!("size {:?} {count}", matched.as_ref().map(|m| m.0)), *value),
            Mark::Angle { matched, .. } => (format!("angle {matched:?}"), 0.0),
            Mark::Place { kind, id, number, .. } => (format!("place {kind:?} {id:?} {number:?}"), 0.0),
        });
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out
}

/// Whether two descriptions say the same, the numbers within `tol`.
fn same_story(a: &[(String, f64)], b: &[(String, f64)], tol: f64) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() <= tol)
}

#[derive(Clone, Copy, Debug)]
enum Kind3 {
    Box,
    Resize(bool, bool, bool, bool),
    Point,
}

fn subject_kind() -> impl Strategy<Value = Kind3> {
    prop_oneof![
        2 => Just(Kind3::Box),
        2 => (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>())
            .prop_filter("some side moves", |(a, b, c, d)| *a || *b || *c || *d)
            .prop_map(|(a, b, c, d)| Kind3::Resize(a, b, c, d)),
        1 => Just(Kind3::Point),
    ]
}

fn run_kind(t: &Targets, s: &Settings, kind: Kind3, f: TurnedBox, constraint: Constraint) -> Snapped {
    let subject = match kind {
        Kind3::Box => Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &[] },
        Kind3::Resize(u_min, u_max, v_min, v_max) => Subject::Edges { frame: f, moving: Edges { u_min, u_max, v_min, v_max } },
        Kind3::Point => Subject::Point { at: f.centre, base: None },
    };
    let q = craft_snap::Query { constraint, ..query(subject, s) };
    let r = run(t, &q);
    r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}"));
    r
}

/// Whether a small nudge of the subject leaves what is said the same. Where it does not, the result sits on a tie (two candidates as far as each other, or
/// a place and the lines through it), and which of two equal answers is given follows the noise of the arithmetic: nothing there is to be compared.
fn stable(t: &Targets, s: &Settings, kind: Kind3, f: TurnedBox, constraint: Constraint, eps: f64) -> bool {
    let names = |r: &Snapped| describe(r).into_iter().map(|d| d.0).collect::<Vec<String>>();
    let base = names(&run_kind(t, s, kind, f, constraint));
    [(eps, 0.0), (-eps, 0.0), (0.0, eps), (0.0, -eps)].iter().all(|(dx, dy)| {
        let nudged = TurnedBox { centre: f.centre + Vec2::new(*dx, *dy), ..f };
        names(&run_kind(t, s, kind, nudged, constraint)) == base
    })
}

/// The configuration: a fixed seed, so that a run is the same run each time, and, for a longer search, the number of cases and the seed from the
/// environment (`CRAFT_SNAP_CASES`, `CRAFT_SNAP_SEED`). The long searches find the rare ties that 300 cases do not, and each find is a case to read.
fn config() -> ProptestConfig {
    let number = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(default);
    ProptestConfig {
        cases: u32::try_from(number("CRAFT_SNAP_CASES", 300)).unwrap_or(300),
        rng_seed: RngSeed::Fixed(number("CRAFT_SNAP_SEED", 0x5eed)),
        max_global_rejects: 1_000_000,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// A scene turned as a whole, about a point, by an angle, with the page directions left out, gives the same hits and marks, and a move turned by
    /// the same angle.
    #[test]
    fn a_scene_turned_as_a_whole_gives_the_turned_result(
        objects in prop::collection::vec(on_grid(&TURNED), 0..10),
        sub in off_grid(&TURNED),
        kind in subject_kind(),
        centre in prop::bool::ANY,
        a in prop_oneof![0.0f64..std::f64::consts::TAU, Just(std::f64::consts::FRAC_PI_2), Just(std::f64::consts::PI), Just(3.0 * std::f64::consts::FRAC_PI_2)],
        about in (-20.0f64..20.0, -20.0f64..20.0),
    ) {
        let s = Settings { frames: Frames { page: false, own: true }, align_reach: None, ..Settings::default() };
        let about = Point::new(about.0, about.1);
        let objs: Vec<TurnedBox> = objects.iter().map(|o| frame(o.0, o.1, o.2, o.3, o.4)).collect();
        let f = frame(sub.0, sub.1, sub.2, sub.3, sub.4);
        let constraint = if centre && matches!(kind, Kind3::Resize(..)) { Constraint::Centre } else { Constraint::None };
        let targets = build(&objs);
        prop_assume!(stable(&targets, &s, kind, f, constraint, 1e-4));
        let base = run_kind(&targets, &s, kind, f, constraint);
        let at = |angle: f64| {
            let moved: Vec<TurnedBox> = objs.iter().map(|o| turned(o, angle, about)).collect();
            run_kind(&build(&moved), &s, kind, turned(&f, angle, about), constraint)
        };
        let after = at(a);
        // Turning the scene by a hair more, or less, must say the same: where it does not, a decision turns on the last bits of the arithmetic
        // (two candidates as far as each other by the shape of the scene), and the answer there follows the noise.
        let names = |r: &Snapped| describe(r).into_iter().map(|d| d.0).collect::<Vec<String>>();
        for hair in [1.3e-9, -2.1e-9] {
            prop_assume!(names(&at(a + hair)) == names(&after));
        }
        // A window that reaches past the buckets it may read is cut where the buckets fall, and where they fall depends on where the scene is: those are
        // two different questions, and the cut says so.
        prop_assume!(!base.work.truncated && !after.work.truncated);
        prop_assert!(same_story(&describe(&base), &describe(&after), 1e-6), "{:?}\n{:?}", describe(&base), describe(&after));
        let want = rotate(base.delta, a);
        prop_assert!((after.delta - want).hypot() < 1e-6, "delta {:?} turned is {:?}, got {:?}", base.delta, want, after.delta);
        if let (Some(b), Some(t)) = (base.frame, after.frame) {
            prop_assert!((t.size.width - b.size.width).abs() < 1e-6 && (t.size.height - b.size.height).abs() < 1e-6, "{b:?} {t:?}");
            let want = turned(&b, a, about);
            prop_assert!((t.centre - want.centre).hypot() < 1e-6, "{want:?} {t:?}");
        }
    }

    /// A scene moved a long way gives the same hits and marks, and a move that is the same within what the numbers there can say.
    #[test]
    fn a_scene_moved_a_long_way_gives_the_same_hits_and_marks(
        objects in prop::collection::vec(on_grid(&ANGLES), 0..10),
        gx in prop::collection::vec(-40i32..40, 0..3),
        sub in off_grid(&ANGLES),
        kind in subject_kind(),
        far in prop::sample::select(vec![1.0e3f64, 1.0e6, 1.0e9, 1.0e11]),
    ) {
        // What counts as one place is a little wider far from the origin, for the numbers there cannot say a place closer (the query widens it itself).
        // The scene near the origin is asked with the same width, so that two candidates the width cannot tell apart are told apart by the same rule.
        let mut s = Settings::default();
        let narrow = s.clone();
        s.coincide = s.coincide.at(far);
        let objs: Vec<TurnedBox> = objects.iter().map(|o| frame(o.0, o.1, o.2, o.3, o.4)).collect();
        let f = frame(sub.0, sub.1, sub.2, sub.3, sub.4);
        let guides: Vec<f64> = gx.iter().map(|g| f64::from(*g) * 0.25).collect();
        // The scene and the subject moved by `by`, asked with `settings`.
        let at = |by: Vec2, settings: &Settings| {
            // The lines are grouped by the same rule the query will use, so that a place the numbers cannot say twice is one place in both scenes.
            let mut b = TargetsBuilder::new(craft_snap::Limits { coincide: s.coincide, ..craft_snap::Limits::default() });
            for (i, o) in objs.iter().enumerate() {
                b.object(craft_snap::Id(i as u64), object(TurnedBox { centre: o.centre + by, ..*o })).unwrap();
            }
            for x in &guides {
                b.line(Some(craft_snap::Id(1000)), craft_snap::Axis::X, *x + by.x, None, craft_snap::Kind::Guide).unwrap();
            }
            let t = b.build();
            let r = run_kind(&t, settings, kind, TurnedBox { centre: f.centre + by, ..f }, Constraint::None);
            (r, t)
        };
        let base = at(Vec2::ZERO, &s).0;
        // Where a decision turns on the last bits of the numbers (a tie, or an object whose box just meets a window), a scene moved by a hair says
        // something else, and the same is asked of the far one: such a case has nothing to be compared.
        let names = |r: &Snapped| describe(r).into_iter().map(|d| d.0).collect::<Vec<String>>();
        // The hair is as wide as the numbers far away are coarse, so that a decision the far scene cannot make the same way is not asked of it.
        let h = (far * 4.0e-15).max(1.0e-9);
        for hair in [Vec2::new(1.3 * h, -0.7 * h), Vec2::new(-2.1 * h, 1.1 * h), Vec2::new(0.4 * h, 2.3 * h)] {
            prop_assume!(names(&at(hair, &s).0) == names(&base));
        }
        let after = at(Vec2::new(far, -far * 0.5), &narrow).0;
        // At 1e11 a number is 1.5e-5 coarse: the scenes are compared to what the numbers there can say, up to 1e11 as the design asks, and the lines of both
        // scenes are grouped by the same widened rule, or two lines the numbers there cannot tell apart would be one place in one scene and two in the other.
        let tol = 1e-9 + far * 4.0e-15;
        prop_assert!(same_story(&describe(&base), &describe(&after), tol), "{:?}\n{:?}", describe(&base), describe(&after));
        prop_assert!((after.delta - base.delta).hypot() < tol, "{:?} {:?}", base.delta, after.delta);
    }
}
