//! The oracle watched: `Snapped::check` is what every other test asserts, so each rule of it is shown to fire. A real result is taken, one thing
//! about it is made false, and the check must say which rule broke. Without these a check that always said yes would pass every other test.

use craft_snap::{About, Axis, Broken, Constraint, Edges, Kind, Mark, Reach, Refusal, Rounding, Settings, SpacingReach, Subject, Targets, TurnedBox};
use kurbo::{Line, Point};

use crate::common::*;

/// The rule that fired, or "" for a result that passed.
fn rule(t: &Targets, q: &craft_snap::Query<'_>, r: &craft_snap::Snapped) -> &'static str {
    r.check(t, q).err().map_or("", |Broken { rule, .. }| rule)
}

/// A query, its result as the engine gave it (which must pass), and the check after `tamper` has made one thing false.
fn fires(t: &Targets, q: &craft_snap::Query<'_>, tamper: impl FnOnce(&mut craft_snap::Snapped)) -> &'static str {
    let mut r = run(t, q);
    assert_eq!(rule(t, q, &r), "", "the untouched result passes: {r:?}");
    tamper(&mut r);
    rule(t, q, &r)
}

fn guides() -> (Targets, Settings) {
    (scene(&[(1, frame(30.0, 0.0, 10.0, 10.0, 0.0))], &[5.0], &[20.0]), Settings::default())
}

/// A box with its low side exactly on a guide and its top 0.4 under another.
fn on_guides() -> TurnedBox {
    frame(10.0, 14.6, 10.0, 10.0, 0.0)
}

fn line_mark(r: &mut craft_snap::Snapped) -> &mut Mark {
    r.marks.iter_mut().find(|m| matches!(m, Mark::Line { .. })).unwrap()
}

mod box_and_marks {
    use super::*;

    #[test]
    fn the_box_the_delta_and_the_angle_are_the_subjects() {
        let (t, s) = guides();
        let q = query(box_subject(on_guides()), &s);
        assert_eq!(fires(&t, &q, |r| r.frame.as_mut().unwrap().centre.x += 0.5), "the returned box is not the subject moved by the returned delta");
        assert_eq!(fires(&t, &q, |r| r.delta.y += 0.5), "the returned box is not the subject moved by the returned delta");
        assert_eq!(fires(&t, &q, |r| r.frame.as_mut().unwrap().size.width += 1.0), "the returned box is not the subject moved by the returned delta");
        assert_eq!(fires(&t, &q, |r| r.frame.as_mut().unwrap().angle += 0.1), "the angle of the subject changed");
        let p = query(Subject::Point { at: Point::new(4.7, 0.0), base: None }, &s);
        assert_eq!(fires(&t, &p, |r| r.point.as_mut().unwrap().x += 0.5), "the returned point is not the subject moved by the returned delta");
    }

    #[test]
    fn a_refused_result_moves_nothing_and_a_cut_result_says_so() {
        let (t, s) = guides();
        let q = query(box_subject(on_guides()), &s);
        assert_eq!(fires(&t, &q, |r| r.work.refused = Some(Refusal::Zoom)), "a refused query moved the subject");
        assert_eq!(fires(&t, &q, |r| r.angle = Some(f64::NAN)), "the angle is not a finite number");
        assert_eq!(fires(&t, &q, |r| r.work.marks_dropped = 2), "marks were dropped");
        // A refusal that moved nothing, with nothing drawn, passes.
        let mut r = run(&t, &q);
        r.work.refused = Some(Refusal::Zoom);
        r.delta = kurbo::Vec2::ZERO;
        r.marks.clear();
        assert_eq!(rule(&t, &q, &r), "");
    }

    #[test]
    fn a_hit_needs_its_mark_unless_the_marks_were_cut() {
        let (t, s) = guides();
        let q = query(box_subject(on_guides()), &s);
        assert_eq!(fires(&t, &q, |r| r.marks.clear()), "a hit has no mark");
        let mut r = run(&t, &q);
        r.marks.clear();
        r.work.cut = craft_snap::Cuts(craft_snap::Cuts::MARKS);
        assert_eq!(rule(&t, &q, &r), "");
        // A mark for another direction does not stand for the hit.
        assert_eq!(
            fires(&t, &q, |r| {
                r.marks.retain(|m| matches!(m, Mark::Line { direction, .. } if direction.angle() == 0.0));
            }),
            "a hit has no mark"
        );
    }

    #[test]
    fn a_line_mark_is_finite_along_its_line_under_a_side_and_names_a_target_that_is_there() {
        let (t, s) = guides();
        let q = query(box_subject(on_guides()), &s);
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { offset, .. } = line_mark(r) {
                    *offset = f64::NAN;
                }
            }),
            "a line mark is not finite"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { segment, .. } = line_mark(r) {
                    segment.p1.x += 3.0;
                    segment.p1.y += 3.0;
                }
            }),
            "a line mark's segment is not along its line"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { segment, .. } = line_mark(r) {
                    *segment = Line::new(Point::new(segment.p0.x + 2.0, segment.p0.y), Point::new(segment.p1.x + 2.0, segment.p1.y));
                }
            }),
            "no side of the final geometry is on a line mark, or its segment does not cover it"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { segment, .. } = line_mark(r) {
                    // Along the line, but far from the side it is the mark of.
                    *segment = Line::new(Point::new(segment.p0.x, segment.p0.y + 500.0), Point::new(segment.p1.x, segment.p1.y + 500.0));
                }
            }),
            "no side of the final geometry is on a line mark, or its segment does not cover it"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { kinds, .. } = line_mark(r) {
                    *kinds = vec![Kind::Edge];
                }
            }),
            "a line mark names a target that is not in the set"
        );
    }

    #[test]
    fn a_place_mark_is_on_a_place_of_the_geometry_and_names_a_place_that_is_in_the_set() {
        let t = scene(&[(7, frame(100.0, 100.0, 20.0, 10.0, 0.0))], &[], &[]);
        let s = Settings::default();
        // The box's high corner is 0.5 from the object's low corner on both axes: it takes the corner.
        let q = query(box_subject(frame(84.5, 90.5, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Place { .. })), "{:?}", r.marks);
        assert_eq!(
            fires(&t, &q, |r| {
                for m in &mut r.marks {
                    if let Mark::Place { at, .. } = m {
                        at.x += 30.0;
                    }
                }
            }),
            "a place mark is not on a place of the final geometry"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                for m in &mut r.marks {
                    if let Mark::Place { id, .. } = m {
                        *id = Some(craft_snap::Id(99));
                    }
                }
            }),
            "a place mark names a place that is not in the set"
        );
    }
}

mod constraints {
    use super::*;

    #[test]
    fn an_axis_lock_a_ray_and_a_nudge_hold_their_line() {
        let (t, s) = guides();
        let locked = craft_snap::Query { constraint: Constraint::Axis(Axis::X), ..query(box_subject(on_guides()), &s) };
        assert_eq!(
            fires(&t, &locked, |r| {
                r.delta.y += 0.3;
                r.frame.as_mut().unwrap().centre.y += 0.3;
            }),
            "an axis lock moved the other axis"
        );
        let ray = craft_snap::Query {
            constraint: Constraint::Ray { origin: Point::new(10.0, 14.6), dir: kurbo::Vec2::new(1.0, 0.0) },
            ..query(box_subject(on_guides()), &s)
        };
        assert_eq!(
            fires(&t, &ray, |r| {
                r.delta.y += 0.3;
                r.frame.as_mut().unwrap().centre.y += 0.3;
            }),
            "the reference point left the ray"
        );
        let nudge = query(
            Subject::Nudge {
                frame: frame(0.0, 0.0, 20.0, 4.0, 0.0),
                bounds: None,
                axis: Axis::X,
                forward: true,
                reach: craft_snap::NudgeReach::Within(3.0),
                fallback: 3.0,
            },
            &s,
        );
        assert_eq!(
            fires(&t, &nudge, |r| {
                r.delta.y += 0.3;
                r.frame.as_mut().unwrap().centre.y += 0.3;
            }),
            "a nudge left its axis"
        );
    }

    #[test]
    fn a_rounding_holds_by_the_distance_moved_or_by_the_min_corner() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let by_position =
            craft_snap::Query { rounding: Rounding::Position(vec2(1.0, 1.0)), ..query(box_subject(frame(3.2, 4.3, 2.0, 2.0, 0.0)), &s) };
        assert_eq!(
            fires(&t, &by_position, |r| {
                r.delta.x += 0.3;
                r.frame.as_mut().unwrap().centre.x += 0.3;
            }),
            "the rounding does not hold"
        );
        let by_delta = craft_snap::Query {
            rounding: Rounding::Delta { step: vec2(1.0, 1.0), from: Point::new(0.0, 0.0) },
            ..query(box_subject(frame(3.2, 4.3, 2.0, 2.0, 0.0)), &s)
        };
        assert_eq!(
            fires(&t, &by_delta, |r| {
                r.delta.y += 0.3;
                r.frame.as_mut().unwrap().centre.y += 0.3;
            }),
            "the rounding does not hold"
        );
        let point =
            craft_snap::Query { rounding: Rounding::Position(vec2(1.0, 1.0)), ..query(Subject::Point { at: Point::new(3.2, 4.3), base: None }, &s) };
        assert_eq!(
            fires(&t, &point, |r| {
                r.delta.x += 0.3;
                r.point.as_mut().unwrap().x += 0.3;
            }),
            "the rounding does not hold"
        );
    }

    #[test]
    fn a_hard_lattice_of_angles_is_met() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let q = craft_snap::Query {
            constraint: Constraint::Angles { step: 15f64.to_radians(), zero: 0.0 },
            ..query(Subject::Rotate { pivot: Point::ORIGIN, angle: 31f64.to_radians(), radius: 150.0, step: None }, &s)
        };
        assert_eq!(fires(&t, &q, |r| r.angle = Some(31f64.to_radians())), "a turn under a hard lattice is not on it");
    }
}

mod resizes {
    use super::*;

    fn dragged(frame: TurnedBox, moving: Edges) -> Subject<'static> {
        Subject::Edges { frame, moving }
    }

    #[test]
    fn the_sides_that_do_not_move_stay_and_the_box_keeps_a_size() {
        let (t, s) = guides();
        let q = query(dragged(frame(0.0, 0.0, 20.0, 10.0, 0.0), Edges { u_max: true, ..Edges::default() }), &s);
        assert_eq!(fires(&t, &q, |r| r.frame.as_mut().unwrap().centre.y += 0.5), "a side that does not move moved");
        assert_eq!(
            fires(&t, &q, |r| {
                let f = r.frame.as_mut().unwrap();
                f.size.height = 0.0;
            }),
            "a resize collapsed or flipped the box"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                let f = r.frame.as_mut().unwrap();
                f.size.width = -3.0;
            }),
            "a resize collapsed or flipped the box"
        );
        // A box that was no box to begin with is not called collapsed.
        let flat = query(dragged(frame(0.0, 0.0, 0.0, 10.0, 0.0), Edges { u_max: true, ..Edges::default() }), &s);
        let r = run(&t, &flat);
        assert_eq!(rule(&t, &flat, &r), "");
    }

    #[test]
    fn a_resize_about_the_centre_keeps_it_and_one_that_keeps_the_proportions_keeps_them() {
        let (t, s) = guides();
        let about = craft_snap::Query {
            constraint: Constraint::Centre,
            ..query(dragged(frame(0.0, 0.0, 20.0, 10.0, 0.0), Edges { u_max: true, ..Edges::default() }), &s)
        };
        assert_eq!(fires(&t, &about, |r| r.frame.as_mut().unwrap().centre.x += 0.5), "a resize about the centre moved the centre");
        let kept = craft_snap::Query {
            constraint: Constraint::Aspect { ratio: 2.0, about: About::Fixed },
            ..query(dragged(frame(0.0, 0.0, 20.0, 10.0, 0.0), Edges { u_max: true, v_max: true, ..Edges::default() }), &s)
        };
        assert_eq!(fires(&t, &kept, |r| r.frame.as_mut().unwrap().size.height += 1.0), "a resize that keeps the proportions changed them");
    }

    #[test]
    fn a_rounded_resize_has_a_point_of_a_moving_side_on_the_lattice() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let q = craft_snap::Query {
            rounding: Rounding::Position(vec2(2.0, 2.0)),
            ..query(dragged(frame(0.0, 0.7, 21.3, 9.0, 0.0), Edges { u_max: true, ..Edges::default() }), &s)
        };
        // The high side lands on 10; moved by 0.37 it is on no lattice line, and none of its points (the ends at 3.8 under and 5.2 over, the middle at 0.7) is.
        assert_eq!(
            fires(&t, &q, |r| {
                let f = r.frame.as_mut().unwrap();
                f.centre.x += 0.185;
                f.size.width += 0.37;
            }),
            "a resize's leading point is not on the lattice on either axis"
        );
        // The same with the corner dragged and the proportions kept: every side may have moved.
        let kept = craft_snap::Query {
            rounding: Rounding::Position(vec2(2.0, 2.0)),
            constraint: Constraint::Aspect { ratio: 2.0, about: About::Fixed },
            ..query(dragged(frame(0.0, 0.7, 21.3, 10.65, 0.0), Edges { u_max: true, v_max: true, ..Edges::default() }), &s)
        };
        assert_eq!(
            fires(&t, &kept, |r| {
                let f = r.frame.as_mut().unwrap();
                f.centre.x += 0.185;
                f.centre.y += 0.0925;
                f.size.width += 0.37;
                f.size.height += 0.185;
            }),
            "a resize's leading point is not on the lattice on either axis"
        );
        // A step too wide for the box is not asked of it.
        let wide = craft_snap::Query {
            rounding: Rounding::Position(vec2(40.0, 40.0)),
            ..query(dragged(frame(0.0, 0.7, 21.3, 9.0, 0.0), Edges { u_max: true, ..Edges::default() }), &s)
        };
        let r = run(&t, &wide);
        assert_eq!(rule(&t, &wide, &r), "");
    }

    #[test]
    fn a_size_mark_is_the_final_length_of_a_side_and_names_an_object_that_has_it() {
        let t = scene(&[(1, frame(100.0, 100.0, 7.0, 2.0, 0.0))], &[], &[]);
        let s = Settings::default();
        let q = query(dragged(frame(0.0, 0.0, 7.2, 5.0, 0.0), Edges { u_max: true, ..Edges::default() }), &s);
        let edit = |f: fn(&mut Mark)| {
            move |r: &mut craft_snap::Snapped| {
                for m in &mut r.marks {
                    if matches!(m, Mark::Size { .. }) {
                        f(m);
                    }
                }
            }
        };
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Size { value, .. } = m {
                        *value += 1.0;
                    }
                })
            ),
            "a size mark is not the final width or height"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Size { count, .. } = m {
                        *count = 0;
                    }
                })
            ),
            "a size mark's side is not a side of the final box"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Size { side, .. } = m {
                        *side = Line::new(Point::new(side.p0.x + 3.0, side.p0.y), Point::new(side.p1.x + 3.0, side.p1.y));
                    }
                })
            ),
            "a size mark's side is not a side of the final box"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Size { matched, .. } = m {
                        *matched = matched.map(|(_, line)| (craft_snap::Id(77), line));
                    }
                })
            ),
            "a size mark names an object that has no side of that length"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Size { matched, .. } = m {
                        *matched = matched.map(|(id, line)| (id, Line::new(line.p0, Point::new(line.p1.x + 4.0, line.p1.y))));
                    }
                })
            ),
            "a size mark names an object that has no side of that length"
        );
        assert_eq!(fires(&t, &q, |r| r.frame = None), "a size mark without a final box");
    }
}

mod spacing_and_angles {
    use super::*;

    fn row_scene() -> (Targets, Settings) {
        let objs: Vec<(u64, TurnedBox)> =
            [0.0, 30.0, 60.0, 95.0].iter().enumerate().map(|(i, x)| (i as u64, frame(x + 5.0, 0.0, 10.0, 10.0, 0.0))).collect();
        (scene(&objs, &[], &[]), Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 200.0 }, ..Settings::default() })
    }

    fn gaps(f: impl Fn(&mut Mark) + 'static) -> impl FnOnce(&mut craft_snap::Snapped) {
        move |r| {
            for m in &mut r.marks {
                if matches!(m, Mark::Gap { .. }) {
                    f(m);
                }
            }
        }
    }

    #[test]
    fn a_gap_mark_runs_along_its_direction_between_facing_sides_and_a_chain_has_one_value() {
        let (t, s) = row_scene();
        let q = query(box_subject(frame(128.0, 0.0, 10.0, 10.0, 0.0)), &s);
        assert_eq!(
            fires(
                &t,
                &q,
                gaps(|m| {
                    if let Mark::Gap { value, .. } = m {
                        *value *= 2.0;
                    }
                })
            ),
            "a gap mark is not along its direction, or its length is not its value"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                gaps(|m| {
                    if let Mark::Gap { segment, .. } = m {
                        segment.p1.y += 2.0;
                    }
                })
            ),
            "a gap mark is not along its direction, or its length is not its value"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                gaps(|m| {
                    if let Mark::Gap { segment, .. } = m {
                        *segment = Line::new(Point::new(segment.p0.x + 4.0, segment.p0.y), Point::new(segment.p1.x + 4.0, segment.p1.y));
                    }
                })
            ),
            "an end of a gap mark is not on a facing side of an object"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                gaps(|m| {
                    if let Mark::Gap { segment, .. } = m {
                        // Off to the side of every object.
                        *segment = Line::new(Point::new(segment.p0.x, segment.p0.y + 80.0), Point::new(segment.p1.x, segment.p1.y + 80.0));
                    }
                })
            ),
            "an end of a gap mark is not on a facing side of an object"
        );
        // The chains of a box with a row each way, made one number, hold two values.
        let (t2, s2) = (
            scene(
                &[
                    (1, frame(5.0, 5.0, 10.0, 10.0, 0.0)),
                    (2, frame(17.0, 5.0, 10.0, 10.0, 0.0)),
                    (3, frame(29.0, 18.0, 10.0, 10.0, 0.0)),
                    (4, frame(29.0, 31.0, 10.0, 10.0, 0.0)),
                ],
                &[],
                &[],
            ),
            s.clone(),
        );
        let q2 = query(box_subject(frame(29.0, 5.0, 10.0, 10.0, 0.0)), &s2);
        let mut r2 = run(&t2, &q2);
        let first = r2.marks.iter().find_map(|m| if let Mark::Gap { chain, .. } = m { Some(*chain) } else { None }).unwrap();
        for m in &mut r2.marks {
            if let Mark::Gap { chain, .. } = m {
                *chain = first;
            }
        }
        assert_eq!(rule(&t2, &q2, &r2), "a chain holds gaps of different values");
    }

    #[test]
    fn an_angle_mark_is_the_final_angle_of_an_object_that_has_it_and_a_whole_number_of_steps() {
        let objs = [(0, frame(0.0, 0.0, 5.0, 5.0, 33f64.to_radians()))];
        let t = scene(&objs, &[], &[]);
        let s = Settings::default();
        let rotate = |step: Option<(f64, f64)>| Subject::Rotate { pivot: Point::ORIGIN, angle: 31f64.to_radians(), radius: 150.0, step };
        let q = query(rotate(None), &s);
        let angle_mark = |f: fn(&mut Mark)| {
            move |r: &mut craft_snap::Snapped| {
                for m in &mut r.marks {
                    if matches!(m, Mark::Angle { .. }) {
                        f(m);
                    }
                }
            }
        };
        assert_eq!(
            fires(
                &t,
                &q,
                angle_mark(|m| {
                    if let Mark::Angle { angle, .. } = m {
                        *angle += 0.01;
                    }
                })
            ),
            "an angle mark is not the final angle"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                angle_mark(|m| {
                    if let Mark::Angle { matched, .. } = m {
                        *matched = Some(craft_snap::Id(42));
                    }
                })
            ),
            "an angle mark names an object that does not have the angle"
        );
        assert_eq!(
            fires(
                &t,
                &q,
                angle_mark(|m| {
                    if let Mark::Angle { step, .. } = m {
                        *step = Some(5f64.to_radians());
                    }
                })
            ),
            "an angle mark has a step but the query has none"
        );
        // With a lattice in the query the step is checked against the angle.
        let nearest = Settings {
            kinds: {
                let mut k = s.kinds.clone();
                let mut rule = k.rule(Kind::Angle);
                rule.reach = Reach::Screen(0.0);
                k.set(Kind::Angle, rule);
                k
            },
            ..Settings::default()
        };
        let stepped_only = query(rotate(Some((5f64.to_radians(), 0.0))), &nearest);
        assert_eq!(
            fires(
                &t,
                &stepped_only,
                angle_mark(|m| {
                    if let Mark::Angle { step, .. } = m {
                        *step = Some(7f64.to_radians());
                    }
                })
            ),
            "an angle mark's angle is not a whole number of steps"
        );
    }
}

mod finer_rules {
    use craft_snap::{Direction, Id, Lattice, Scope};

    use super::*;

    /// The first line mark whose line is in a given direction.
    fn mark_in(r: &mut craft_snap::Snapped, angle: f64) -> &mut Mark {
        r.marks.iter_mut().find(|m| matches!(m, Mark::Line { direction, .. } if direction.angle() == angle)).unwrap()
    }

    #[test]
    fn a_mark_of_the_lattice_is_on_it() {
        let (t, s) = guides();
        let lat = [Lattice { origin: Point::ORIGIN, pitch: vec2(10.0, 10.0), kind: Kind::Grid }];
        let q = craft_snap::Query { lattices: &lat, ..query(box_subject(on_guides()), &s) };
        // The guide at 5 is not a line of a lattice of 10; the guide at 20 is.
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { kinds, .. } = mark_in(r, 0.0) {
                    *kinds = vec![Kind::Grid];
                }
            }),
            "a line mark names a target that is not in the set"
        );
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { kinds, .. } = mark_in(r, std::f64::consts::FRAC_PI_2) {
                    *kinds = vec![Kind::Grid];
                }
            }),
            ""
        );
    }

    #[test]
    fn an_axis_lock_is_asked_of_a_move_and_not_of_a_resize() {
        let (t, s) = guides();
        // The top side is pulled to the lattice, so the box moves along y though the lock says x: a resize is not held to a lock.
        let q = craft_snap::Query {
            constraint: Constraint::Axis(Axis::X),
            rounding: Rounding::Position(vec2(2.0, 2.0)),
            ..query(Subject::Edges { frame: frame(0.0, 0.0, 20.0, 9.3, 0.0), moving: Edges { v_max: true, ..Edges::default() } }, &s)
        };
        let r = run(&t, &q);
        assert!(r.delta.y != 0.0, "{:?}", r.delta);
        assert_eq!(rule(&t, &q, &r), "");
    }

    #[test]
    fn the_ray_is_kept_to_within_the_tolerance_and_not_to_a_multiple_of_it() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let q = craft_snap::Query {
            constraint: Constraint::Ray { origin: Point::ORIGIN, dir: kurbo::Vec2::new(3.0, 4.0) },
            ..query(box_subject(frame(3.0, 4.0, 2.0, 2.0, 0.0)), &s)
        };
        let shift = |d: kurbo::Vec2| {
            move |r: &mut craft_snap::Snapped| {
                r.delta += d;
                r.frame.as_mut().unwrap().centre += d;
            }
        };
        assert_eq!(fires(&t, &q, shift(kurbo::Vec2::new(-4.0e-10, 3.0e-10))), "", "5e-10 off the ray is within the tolerance");
        assert_eq!(fires(&t, &q, shift(kurbo::Vec2::new(-4.0e-9, 3.0e-9))), "the reference point left the ray");
    }

    #[test]
    fn a_rounding_is_on_the_lattice_of_its_own_pitch_whatever_it_is() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let q = craft_snap::Query { rounding: Rounding::Position(vec2(2.5, 2.5)), ..query(box_subject(frame(3.2, 4.3, 2.0, 2.0, 0.0)), &s) };
        assert_eq!(
            fires(&t, &q, |r| {
                r.delta.x += 0.3;
                r.frame.as_mut().unwrap().centre.x += 0.3;
            }),
            "the rounding does not hold"
        );
        // A pitch of no end rounds nothing.
        let q =
            craft_snap::Query { rounding: Rounding::Position(vec2(f64::INFINITY, 2.5)), ..query(box_subject(frame(3.2, 4.3, 2.0, 2.0, 0.0)), &s) };
        let r = run(&t, &q);
        assert_eq!(rule(&t, &q, &r), "");
    }

    #[test]
    fn a_line_through_a_corner_is_the_mark_of_a_resize_and_not_of_a_move_and_must_cover_the_corner() {
        // A resize: a corner of the dragged side is on the guide at 13.
        let a = 30f64.to_radians();
        let (t, s) = (scene(&[], &[13.0], &[]), Settings::default());
        let q = query(Subject::Edges { frame: frame(0.0, 0.0, 20.0, 10.0, a), moving: Edges { u_max: true, ..Edges::default() } }, &s);
        assert_eq!(
            fires(&t, &q, |r| {
                if let Mark::Line { segment, .. } = line_mark(r) {
                    *segment = Line::new(Point::new(segment.p0.x, segment.p0.y + 100.0), Point::new(segment.p1.x, segment.p1.y + 100.0));
                }
            }),
            "no side of the final geometry is on a line mark, or its segment does not cover it"
        );
        // A move: a guide at the height of a corner that is not the highest or the lowest is not on any side of the box round the turned one.
        let f = frame(0.0, 0.0, 10.0, 6.0, 0.3);
        let y = f.corners()[1].y;
        let t = scene(&[], &[], &[y]);
        let q = craft_snap::Query { zoom: 100.0, ..query(box_subject(f), &s) };
        let bogus = Mark::Line {
            direction: Direction::new(std::f64::consts::FRAC_PI_2).unwrap(),
            offset: y,
            segment: Line::new(Point::new(0.0, y), Point::new(10.0, y)),
            kinds: vec![Kind::Guide],
            first: Some(Id(2000)),
            count: 1,
            hit: false,
        };
        assert_eq!(fires(&t, &q, |r| r.marks.push(bogus)), "no side of the final geometry is on a line mark, or its segment does not cover it");
    }

    #[test]
    fn a_size_mark_is_whole_in_its_side_and_names_an_object_in_the_view() {
        let a_side = |r: &mut craft_snap::Snapped| {
            for m in &mut r.marks {
                if let Mark::Size { side, .. } = m {
                    side.p1.x += 1.0;
                }
            }
        };
        let t = scene(&[(1, frame(100.0, 100.0, 7.0, 2.0, 0.0))], &[], &[]);
        let s = Settings::default();
        let q = query(Subject::Edges { frame: frame(0.0, 0.0, 7.2, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s);
        assert_eq!(fires(&t, &q, a_side), "a size mark's side is not a side of the final box");
        // The view: one box in it, and four outside it, each outside in one direction only.
        let mut b = craft_snap::TargetsBuilder::new(craft_snap::Limits::default());
        let only_size = craft_snap::Parts { size: true, ..craft_snap::Parts::NONE };
        let at = |x: f64, y: f64| craft_snap::Object { frame: frame(x, y, 7.0, 2.0, 0.0), bounds: None, angle: None, parts: only_size };
        b.object(Id(1), at(30.0, 30.0)).unwrap();
        for (i, (x, y)) in [(150.0, 40.0), (-50.0, 40.0), (40.0, 150.0), (40.0, -50.0)].into_iter().enumerate() {
            b.object(Id(10 + i as u64), at(x, y)).unwrap();
        }
        let t = b.build();
        let mut s = Settings::default();
        s.kinds.set(Kind::Size, craft_snap::KindRule { on: true, reach: Reach::Screen(6.0), priority: 6, scope: Scope::View });
        let index = t.in_view(kurbo::Rect::new(0.0, 0.0, 100.0, 100.0));
        let q = craft_snap::Query {
            view: Some(&index),
            ..query(Subject::Edges { frame: frame(50.0, 50.0, 7.2, 5.0, 0.0), moving: Edges { u_max: true, ..Edges::default() } }, &s)
        };
        let r = run(&t, &q);
        assert!(r.marks.iter().any(|m| matches!(m, Mark::Size { matched: Some((Id(1), _)), .. })), "{:?}", r.marks);
        for out in [10u64, 11, 12, 13] {
            let rule_of = fires(&t, &q, |r| {
                for m in &mut r.marks {
                    if let Mark::Size { matched: Some((id, _)), .. } = m {
                        *id = Id(out);
                    }
                }
            });
            assert_eq!(rule_of, "a size mark names an object that has no side of that length", "{out}");
        }
    }

    #[test]
    fn a_place_mark_of_an_app_place_names_its_object_and_its_number() {
        let mut b = craft_snap::TargetsBuilder::new(craft_snap::Limits::default());
        b.point(Some(Id(9)), Point::new(50.0, 50.0), Kind::Point(0), 3).unwrap();
        let t = b.build();
        let s = Settings::default();
        let q = query(Subject::Point { at: Point::new(50.5, 50.5), base: None }, &s);
        let edit = |f: fn(&mut Mark)| {
            move |r: &mut craft_snap::Snapped| {
                for m in &mut r.marks {
                    if matches!(m, Mark::Place { .. }) {
                        f(m);
                    }
                }
            }
        };
        let want = "a place mark names a place that is not in the set";
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Place { number, .. } = m {
                        *number = Some(4);
                    }
                })
            ),
            want
        );
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Place { id, .. } = m {
                        *id = Some(Id(8));
                    }
                })
            ),
            want
        );
        assert_eq!(
            fires(
                &t,
                &q,
                edit(|m| {
                    if let Mark::Place { kind, .. } = m {
                        *kind = Kind::Point(1);
                    }
                })
            ),
            want
        );
    }

    #[test]
    fn a_gap_is_more_than_nothing_and_its_ends_are_on_boxes_that_cover_the_mark_across() {
        // A gap of no length between two boxes that touch is no gap.
        let t =
            scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(15.0, 0.0, 10.0, 10.0, 0.0)), (3, frame(45.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let s = Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 200.0 }, ..Settings::default() };
        let q = query(box_subject(frame(75.0, 0.0, 10.0, 10.0, 0.0)), &s);
        let zero = |r: &mut craft_snap::Snapped| {
            r.marks.push(Mark::Gap {
                direction: Direction::new(0.0).unwrap(),
                segment: Line::new(Point::new(10.0, 0.0), Point::new(10.0, 0.0)),
                value: 0.0,
                chain: 9,
                subject: false,
                hit: false,
            });
        };
        assert_eq!(fires(&t, &q, zero), "a gap mark is not along its direction, or its length is not its value");
        // A staircase: the first box across [-5, 5], the second across [3, 13], and the gap is marked across their overlap, at 4.
        let linked = Settings { spacing_row: craft_snap::SpacingRow::Linked, ..s };
        let t = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 8.0, 10.0, 10.0, 0.0))], &[], &[]);
        let q = query(box_subject(frame(65.2, 13.0, 10.0, 10.0, 0.0)), &linked);
        let first_gap_across = |y: f64| {
            move |r: &mut craft_snap::Snapped| {
                if let Some(Mark::Gap { segment, .. }) = r.marks.iter_mut().find(|m| matches!(m, Mark::Gap { subject: false, .. })) {
                    *segment = Line::new(Point::new(segment.p0.x, y), Point::new(segment.p1.x, y));
                }
            }
        };
        let want = "an end of a gap mark is not on a facing side of an object";
        // Across -3 only the first box is there, and across 10 only the second.
        assert_eq!(fires(&t, &q, first_gap_across(-3.0)), want);
        assert_eq!(fires(&t, &q, first_gap_across(10.0)), want);
    }

    #[test]
    fn a_refused_result_that_moved_nothing_but_drew_something_is_not_whole() {
        let (t, s) = guides();
        let q = query(box_subject(on_guides()), &s);
        assert_eq!(
            fires(&t, &q, |r| {
                r.work.refused = Some(Refusal::Zoom);
                r.delta = kurbo::Vec2::ZERO;
            }),
            "a refused query moved the subject"
        );
    }
}
