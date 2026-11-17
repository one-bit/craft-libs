//! A turn by a handle: rotation match and angle steps.

use craft_snap::{Choose, Constraint, Id, Kind, Limits, Mark, Object, Parts, Settings, Subject, TargetsBuilder};
use kurbo::Point;

use crate::common::*;

fn turned_scene(angles: &[f64]) -> craft_snap::Targets {
    let mut b = TargetsBuilder::new(Limits::default());
    for (i, a) in angles.iter().enumerate() {
        b.object(Id(i as u64), Object { frame: frame(i as f64 * 20.0, 0.0, 5.0, 5.0, *a), bounds: None, angle: None, parts: Parts::ALL }).unwrap();
    }
    b.build()
}

fn rotate(angle: f64, radius: f64, step: Option<(f64, f64)>) -> Subject<'static> {
    Subject::Rotate { pivot: Point::new(0.0, 0.0), angle, radius, step }
}

#[test]
fn a_turn_near_another_objects_angle_lands_on_it_within_the_reach_at_the_pointer() {
    let t = turned_scene(&[33f64.to_radians()]);
    let s = Settings::default();
    // 150 away from the pivot at zoom 1: 6 screen units of reach is atan(6 / 150) = 2.3 degrees.
    let q = query(rotate(31.0f64.to_radians(), 150.0, None), &s);
    let r = run(&t, &q);
    assert_eq!(r.angle, Some(33f64.to_radians() + 0.0));
    assert!(matches!(r.marks.first(), Some(Mark::Angle { matched: Some(Id(0)), hit: true, .. })), "{:?}", r.marks);
    r.check(&t, &q).unwrap();
    // 4 degrees out is past it.
    let far = query(rotate(37.5f64.to_radians(), 150.0, None), &s);
    let r = run(&t, &far);
    assert_eq!(r.angle, Some(37.5f64.to_radians()));
    assert!(r.marks.is_empty() && r.hits.is_empty());
    // A pointer within the reach of the pivot aims at nothing.
    let near = query(rotate(31.0f64.to_radians(), 5.0, None), &s);
    assert!(run(&t, &near).hits.is_empty());
}

#[test]
fn a_match_within_reach_beats_the_step_when_the_angle_ranks_first_and_not_otherwise() {
    let t = turned_scene(&[33f64.to_radians()]);
    let step = Some((5f64.to_radians(), 0.0));
    let mut s = Settings { choose: Choose::Ranked, ..Settings::default() };
    let mut rule = s.kinds.rule(Kind::Angle);
    rule.priority = 6;
    s.kinds.set(Kind::Angle, rule);
    let q = query(rotate(34.0f64.to_radians(), 150.0, step), &s);
    let r = run(&t, &q);
    assert_eq!(r.angle, Some(33f64.to_radians()), "ranked: the match, not the 35-degree step");
    r.check(&t, &q).unwrap();
    let nearest = Settings::default();
    let q = query(rotate(34.9f64.to_radians(), 150.0, step), &nearest);
    let r = run(&t, &q);
    assert!((r.angle.unwrap() - 35f64.to_radians()).abs() < 1e-12, "nearest: the step is nearer");
    r.check(&t, &q).unwrap();
}

#[test]
fn a_hard_lattice_allows_only_angles_on_it_and_a_match_off_it_is_not_taken() {
    let t = turned_scene(&[33f64.to_radians()]);
    let s = Settings::default();
    let q = craft_snap::Query {
        constraint: Constraint::Angles { step: 15f64.to_radians(), zero: 0.0 },
        ..query(rotate(31.0f64.to_radians(), 150.0, None), &s)
    };
    let r = run(&t, &q);
    assert!((r.angle.unwrap() - 30f64.to_radians()).abs() < 1e-12, "{:?}", r.angle);
    r.check(&t, &q).unwrap();
}

#[test]
fn a_full_turn_is_one_and_a_period_of_half_a_turn_matches_across_it() {
    let t = turned_scene(&[10f64.to_radians()]);
    let s = Settings::default();
    let q = query(rotate(10f64.to_radians() + std::f64::consts::TAU + 0.01, 150.0, None), &s);
    let r = run(&t, &q);
    assert!((r.angle.unwrap() - (10f64.to_radians() + std::f64::consts::TAU)).abs() < 1e-12);
    let t = turned_scene(&[10f64.to_radians()]);
    let half = Settings { angle_period: std::f64::consts::PI, ..Settings::default() };
    let q = query(rotate(190f64.to_radians() + 0.01, 150.0, None), &half);
    let r = run(&t, &q);
    assert!((r.angle.unwrap() - 190f64.to_radians()).abs() < 1e-12, "{:?}", r.angle);
    r.check(&t, &q).unwrap();
}

#[test]
fn a_match_with_a_reach_of_any_distance_takes_the_angle_however_far_off_it_is() {
    // Half a turn is the farthest anything is; 1.4 radians is not past it.
    let t = turned_scene(&[0.0]);
    let mut s = Settings::default();
    let mut rule = s.kinds.rule(Kind::Angle);
    rule.reach = craft_snap::Reach::Any;
    s.kinds.set(Kind::Angle, rule);
    let q = query(rotate(1.4, 150.0, None), &s);
    let r = run(&t, &q);
    assert_eq!(r.angle, Some(0.0), "{:?}", r.hits);
    r.check(&t, &q).unwrap();
}

#[test]
fn a_pointer_exactly_a_reach_from_the_pivot_aims_at_nothing_and_one_hair_beyond_it_does() {
    let t = turned_scene(&[33f64.to_radians()]);
    let s = Settings::default();
    // Six screen units of reach at zoom 1.
    let on = query(rotate(31.0f64.to_radians(), 6.0, None), &s);
    assert!(run(&t, &on).hits.is_empty());
    let past = query(rotate(31.0f64.to_radians(), 6.0 + 1.0e-6, None), &s);
    let r = run(&t, &past);
    assert_eq!(r.angle, Some(33f64.to_radians()));
    r.check(&t, &past).unwrap();
}

#[test]
fn a_step_lattice_is_taken_within_half_a_turn_and_not_beyond_it() {
    // The steps have the reach of any distance, but a step a turn and a half wide is farther than half a turn from most angles.
    let t = turned_scene(&[]);
    let s = Settings::default();
    let q = query(rotate(1.0, 150.0, Some((2.0, 0.0))), &s);
    let r = run(&t, &q);
    assert_eq!(r.angle, Some(2.0), "{:?}", r.hits);
    r.check(&t, &q).unwrap();
    let wide = query(rotate(5.0, 150.0, Some((9.42, 0.0))), &s);
    let r = run(&t, &wide);
    assert_eq!(r.angle, Some(5.0), "{:?}", r.hits);
    assert!(r.hits.is_empty());
}

#[test]
fn a_lattice_of_angles_with_a_zero_off_the_origin_is_met_from_that_zero() {
    let t = turned_scene(&[]);
    let s = Settings::default();
    let (step, zero) = (15f64.to_radians(), 0.1);
    // 31 degrees is 4.7 from the lattice point at 5.7 + 30 and 10.3 from the one before it.
    let hard = craft_snap::Query { constraint: Constraint::Angles { step, zero }, ..query(rotate(31f64.to_radians(), 150.0, None), &s) };
    let r = run(&t, &hard);
    assert!((r.angle.unwrap() - (zero + 2.0 * step)).abs() < 1e-12, "{:?}", r.angle);
    r.check(&t, &hard).unwrap();
    // The same as a soft lattice of steps.
    let soft = query(rotate(31f64.to_radians(), 150.0, Some((step, zero))), &s);
    let r = run(&t, &soft);
    assert!((r.angle.unwrap() - (zero + 2.0 * step)).abs() < 1e-12, "{:?}", r.angle);
    r.check(&t, &soft).unwrap();
    assert!(matches!(r.marks.first(), Some(Mark::Angle { step: Some(_), .. })), "{:?}", r.marks);
}

mod nudges {
    use craft_snap::{Axis, Kind, Mark, Settings, Subject};
    use kurbo::Point;

    use crate::common::*;

    fn nudge(f: craft_snap::TurnedBox, axis: Axis, forward: bool, step: f64) -> Subject<'static> {
        Subject::Nudge { frame: f, bounds: None, axis, forward, reach: craft_snap::NudgeReach::Within(step), fallback: step }
    }

    #[test]
    fn a_nudge_stops_at_the_first_guide_ahead_within_a_step_and_the_next_press_moves_off_it() {
        let t = scene(&[], &[10.1], &[]);
        let s = Settings::default();
        let f = frame(0.0, 0.0, 20.0, 4.0, 0.0);
        // The box's right side is at 10; a guide at 10.1 is 0.1 ahead of a step of 0.25.
        let q = query(nudge(f, Axis::X, true, 0.25), &s);
        let r = run(&t, &q);
        assert!((r.delta.x - 0.1).abs() < 1e-12 && r.delta.y == 0.0, "{:?}", r.delta);
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Line { kinds, .. } if kinds == &[Kind::Guide])));
        r.check(&t, &q).unwrap();
        let on = r.frame.unwrap();
        let again = query(nudge(on, Axis::X, true, 0.25), &s);
        let r2 = run(&t, &again);
        assert!((r2.delta.x - 0.25).abs() < 1e-12, "the guide it is on is not ahead: {:?}", r2.delta);
        r2.check(&t, &again).unwrap();
        // The other way there is nothing within a step.
        let back = query(nudge(f, Axis::X, false, 0.25), &s);
        let r3 = run(&t, &back);
        assert!((r3.delta.x + 0.25).abs() < 1e-12);
        let _ = Point::ORIGIN;
    }

    #[test]
    fn a_nudge_takes_a_whole_step_when_nothing_is_ahead_within_it() {
        let t = scene(&[], &[10.6], &[]);
        let s = Settings::default();
        let q = query(nudge(frame(0.0, 0.0, 20.0, 4.0, 0.0), Axis::X, true, 0.25), &s);
        let r = run(&t, &q);
        assert_eq!(r.delta.x, 0.25);
        r.check(&t, &q).unwrap();
    }

    #[test]
    fn a_turned_boxs_own_side_ahead_stops_a_nudge_along_the_axis() {
        // A box turned by 30 degrees has a side whose line meets the x axis at a slant: the travel is (c - side) / (n . e).
        let t = scene(&[(1, frame(50.0, 0.0, 10.0, 10.0, 30f64.to_radians()))], &[], &[]);
        let s = Settings::default();
        let f = frame(0.0, 0.0, 10.0, 10.0, 30f64.to_radians());
        let q = query(nudge(f, Axis::X, true, 100.0), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(r.delta.x > 0.0 && r.delta.x < 100.0, "{:?}", r.delta);
        assert!(!r.hits.is_empty());
    }
}

mod hostile {
    use craft_snap::{Budget, Constraint, Id, Kind, Limits, Object, Parts, Refusal, Settings, Subject, TargetsBuilder};
    use kurbo::Point;

    use crate::common::*;

    fn scene_at(angle: f64) -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        b.object(Id(1), Object { frame: frame(0.0, 0.0, 5.0, 5.0, angle), bounds: None, angle: None, parts: Parts::ALL }).unwrap();
        b.build()
    }

    fn rot(angle: f64, step: Option<(f64, f64)>) -> Subject<'static> {
        Subject::Rotate { pivot: Point::ORIGIN, angle, radius: 150.0, step }
    }

    /// Whether `a` is on the lattice within the same tolerance the crate reads: the angle tolerance, or the relative one at large angles.
    fn on(a: f64, step: f64, zero: f64) -> bool {
        let co = craft_snap::Coincide::default();
        let x = ((a - zero) + 0.5 * step).rem_euclid(step) - 0.5 * step;
        a.is_finite() && x.abs() <= co.angle.max(co.rel * a.abs().max(zero.abs()))
    }

    #[test]
    fn a_hard_lattice_result_is_on_the_lattice_at_every_size_of_angle() {
        let step = std::f64::consts::TAU / 4.0;
        let t = scene_at(0.0);
        let s = Settings::default();
        for base in [1.0e3, 1.0e8, 1.0e12, 1.0e15] {
            for i in 0..300 {
                let raw = base + f64::from(i) * 0.37;
                let q = craft_snap::Query { constraint: Constraint::Angles { step, zero: 0.0 }, ..query(rot(raw, None), &s) };
                let r = run(&t, &q);
                if let Some(a) = r.angle {
                    assert!(on(a, step, 0.0), "{raw}: {a}");
                }
                r.check(&t, &q).unwrap_or_else(|b| panic!("{raw}: {b:?}"));
            }
        }
    }

    #[test]
    fn hostile_angles_and_steps_never_give_a_result_that_is_not_a_number() {
        let t = scene_at(0.3);
        let s = Settings::default();
        let tiny = Some((1.0e-300, 0.0));
        for raw in [1.0e300, -1.0e300, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1.0e308, f64::MAX] {
            for (constraint, step) in [
                (Constraint::None, None),
                (Constraint::None, tiny),
                (Constraint::Angles { step: 1.0e-300, zero: 0.0 }, None),
                (Constraint::Angles { step: 0.5, zero: 1.0e300 }, None),
            ] {
                let q = craft_snap::Query { constraint, ..query(rot(raw, step), &s) };
                let r = run(&t, &q);
                assert!(r.angle.is_none_or(f64::is_finite), "{raw} {constraint:?} {step:?}: {:?}", r.angle);
                assert!(r.hits.iter().all(|h| h.distance.is_finite()), "{raw}: {:?}", r.hits);
                for m in &r.marks {
                    if let craft_snap::Mark::Angle { angle, .. } = m {
                        assert!(angle.is_finite());
                    }
                }
                r.check(&t, &q).unwrap_or_else(|b| panic!("{raw} {constraint:?} {step:?}: {b:?}"));
            }
        }
        let q = query(rot(f64::NAN, None), &s);
        let r = run(&t, &q);
        assert_eq!((r.angle, r.work.refused), (None, Some(Refusal::NotFinite)));
        // A hard lattice no finite angle lies on: no angle, and the reason.
        let q = craft_snap::Query { constraint: Constraint::Angles { step: 1.0e-300, zero: 0.0 }, ..query(rot(1.0e300, None), &s) };
        let r = run(&t, &q);
        assert_eq!((r.angle, r.work.refused), (None, Some(Refusal::NotFinite)));
    }

    #[test]
    fn the_angle_mark_is_charged_to_the_marks_budget_like_every_other_mark() {
        let t = scene_at(33f64.to_radians());
        let s = Settings::default();
        let with = |marks: u32| {
            let q = craft_snap::Query { budget: Budget { marks, ..Budget::default() }, ..query(rot(33f64.to_radians() + 0.001, None), &s) };
            (run(&t, &q), q)
        };
        let (r, q) = with(1);
        assert_eq!(r.marks.len(), 1);
        assert!(!r.work.truncated);
        r.check(&t, &q).unwrap();
        let (r, q) = with(0);
        assert!(r.marks.is_empty(), "{:?}", r.marks);
        assert!(r.work.truncated && r.work.cut.has(craft_snap::Cuts::MARKS));
        r.check(&t, &q).unwrap();
        assert_eq!(r.hits.first().map(|h| h.kind), Some(Kind::Angle), "the hit is still reported");
    }
}
