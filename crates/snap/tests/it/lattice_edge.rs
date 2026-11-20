//! A nudge on a lattice whose pitch is a hair above the same-place rule.

use craft_snap::{Axis, Kind, Lattice, NudgeReach, Query, Settings, Subject, TargetsBuilder, TurnedBox};
use kurbo::{Point, Size, Vec2};

use crate::common::*;

/// A nudge on a lattice whose pitch is a hair above the same-place rule: the next line may be nearer than the rule once the line positions are rounded.
mod lattice_residue {
    use super::*;

    fn nudge(c: f64, pitch: f64, forward: bool) -> (Result<(), craft_snap::Broken>, craft_snap::Snapped) {
        let t = TargetsBuilder::new(craft_snap::Limits::default()).build();
        let s = Settings::default();
        let lats = [Lattice { origin: Point::new(c, c), pitch: Vec2::new(pitch, pitch), kind: Kind::Grid }];
        let subject = Subject::Nudge {
            frame: TurnedBox { centre: Point::new(c, c), size: Size::new(2.0, 2.0), angle: 0.0 },
            bounds: None,
            axis: Axis::X,
            forward,
            reach: NudgeReach::Anywhere,
            fallback: 0.125,
        };
        let q = Query { lattices: &lats, ..query(subject, &s) };
        let r = run(&t, &q);
        (r.check(&t, &q), r)
    }

    #[test]
    fn named_pitches_just_above_the_rule_far_from_the_origin_give_a_stop_the_check_takes() {
        // Found by a sweep: the line the nudge stopped on was nearer than the rule, since the lines are only as exact as the numbers they are made of.
        for (c, pitch) in
            [(1.0e5, 1.3553472630766514e-9), (1.0e5, 1.3555993435510772e-9), (1.0e6, 4.552913998202368e-9), (1.0e6, 4.553187161023095e-9)]
        {
            for forward in [true, false] {
                let (ok, r) = nudge(c, pitch, forward);
                ok.unwrap_or_else(|b| panic!("c {c:e} pitch {pitch:e} forward {forward}: {b:?}\n{r:?}"));
            }
        }
        for (c, pitch) in [(3.0e6, 1.166543903269029e-8), (3.0e6, 1.1665508981536509e-8)] {
            for forward in [true, false] {
                let (ok, r) = nudge(c, pitch, forward);
                ok.unwrap_or_else(|b| panic!("c {c:e} pitch {pitch:e} forward {forward}: {b:?}\n{r:?}"));
            }
        }
    }

    #[test]
    fn a_sweep_of_pitches_across_the_band_just_above_the_rule_is_taken_by_the_check_from_a_thousand_to_ten_million() {
        for c in [1.0e3, 1.0e5, 1.0e6, 3.0e6, 1.0e7] {
            let tol = 1.0e-9 + 16.0 * f64::EPSILON * c;
            for forward in [true, false] {
                for i in 0..4_000 {
                    let pitch = tol * (0.99 + 0.04 * f64::from(i) / 4_000.0);
                    let (ok, r) = nudge(c, pitch, forward);
                    ok.unwrap_or_else(|b| panic!("c {c:e} pitch {pitch:e} forward {forward}: {b:?}\n{r:?}"));
                }
            }
        }
    }
}
