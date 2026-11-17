//! A turn by a handle: rotation match and angle steps.

use craft_snap::{Choose, Constraint, Id, Kind, Limits, Mark, Object, Parts, Settings, Subject, TargetsBuilder};
use kurbo::Point;

use crate::common::*;

fn turned_scene(angles: &[f64]) -> craft_snap::Targets {
    let mut b = TargetsBuilder::new(Limits::default());
    for (i, a) in angles.iter().enumerate() {
        b.object(Id(i as u64), Object { frame: frame(i as f64 * 20.0, 0.0, 5.0, 5.0, *a), angle: None, parts: Parts::ALL }).unwrap();
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
    let q = craft_snap::Query { constraint: Constraint::Angles { step: 15f64.to_radians(), zero: 0.0 }, ..query(rotate(31.0f64.to_radians(), 150.0, None), &s) };
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

mod nudges {
    use craft_snap::{Axis, Kind, Mark, Settings, Subject};
    use kurbo::Point;

    use crate::common::*;

    fn nudge(f: craft_snap::TurnedBox, axis: Axis, forward: bool, step: f64) -> Subject<'static> {
        Subject::Nudge { frame: f, axis, forward, step }
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
