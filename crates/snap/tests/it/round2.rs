//! Fix round 2: what the review of the crate found. Each test is a crafted input from the review, or the smallest one of its kind.

use craft_snap::{Edges, Kind, Lattice, Query, Rounding, Settings, Subject};
use kurbo::{Point, Vec2};

use crate::common::*;

mod far_jumps {
    use super::*;

    const GRID_X: fn() -> [Lattice; 1] = || [Lattice { origin: Point::new(0.3, 0.0), pitch: Vec2::new(1.0, f64::NAN), kind: Kind::Grid }];
    const GRID_Y: fn() -> [Lattice; 1] = || [Lattice { origin: Point::new(0.0, 0.3), pitch: Vec2::new(f64::NAN, 1.0), kind: Kind::Grid }];

    #[test]
    fn two_nearly_parallel_directions_through_a_kind_of_any_reach_never_throw_the_box_far() {
        for tiny in [1.0e-6, 1.0e-7, 1.0e-8] {
            let t = scene(&[(1, frame(0.5, 1000.0, 10.0, 10.0, tiny))], &[], &[]);
            let s = Settings::default();
            let lats = GRID_X();
            let q = Query { lattices: &lats, ..query(box_subject(frame(0.0, 0.0, 10.0, 10.0, tiny)), &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!(r.delta.hypot() < 1_000.0, "{tiny}: the box was thrown {:?}", r.delta);
        }
    }

    #[test]
    fn a_resize_along_a_line_nearly_parallel_to_its_travel_does_not_grow_without_end() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let lats = GRID_Y();
        let moving = Edges { u_max: true, ..Edges::default() };
        for angle in [1.0e-6, 1.0e-8, 1.0e-10, 0.0] {
            let f = frame(0.0, 0.0, 10.0, 10.0, angle);
            let q = Query { lattices: &lats, ..query(Subject::Edges { frame: f, moving }, &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!((r.frame.unwrap().size.width - 10.0).abs() < 1_000.0, "{angle}: {:?}", r.frame);
            let rounded = Query { rounding: Rounding::Position(Vec2::new(1.0, 1.0)), ..q };
            let r = run(&t, &rounded);
            r.check(&t, &rounded).unwrap();
            assert!((r.frame.unwrap().size.width - 10.0).abs() < 1_000.0, "{angle} rounded: {:?}", r.frame);
        }
    }

    #[test]
    fn a_full_grid_under_a_rounding_does_not_grow_a_nearly_unturned_box_without_end() {
        let t = scene(&[], &[], &[]);
        let s = Settings::default();
        let lats = [Lattice { origin: Point::new(0.3, 0.3), pitch: Vec2::new(1.0, 1.0), kind: Kind::Grid }];
        let moving = Edges { u_max: true, ..Edges::default() };
        for angle in [1.0e-10, 1.0e-6] {
            let q = Query {
                lattices: &lats,
                rounding: Rounding::Position(Vec2::new(1.0, 1.0)),
                ..query(Subject::Edges { frame: frame(0.0, 0.0, 10.0, 10.0, angle), moving }, &s)
            };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!((r.frame.unwrap().size.width - 10.0).abs() < 1_000.0, "{angle}: {:?}", r.frame);
        }
    }
}

mod pair_travel {
    use craft_snap::{Direction, Id, Limits, TargetsBuilder};

    use super::*;

    /// Two guides whose directions are `tilt` apart, one 3 off the box's side to the one side and the other 3 off to the other.
    fn two_guides(tilt: f64) -> craft_snap::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        // A vertical guide 3 to the right of the box's left side (x = -5.x), and a guide `tilt` off vertical 3 to its left, on the own frame of the box.
        b.line(Some(Id(1)), craft_snap::Axis::X, -2.0, None, Kind::Guide).unwrap();
        b.line_at(Some(Id(2)), Direction::new(tilt).unwrap(), -8.0, None, Kind::Guide).unwrap();
        b.build()
    }

    #[test]
    fn two_directions_a_little_apart_within_a_reach_do_not_throw_the_box_past_it() {
        let s = Settings::default();
        for tilt in [1.0e-2, 1.0e-3, 1.0e-4] {
            let t = two_guides(tilt);
            let q = query(box_subject(frame(0.0, 0.0, 10.0, 10.0, tilt)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            // The reach is 6: the best pair would travel 6 / tilt along the lines, and no single pull is more than the reach.
            assert!(r.delta.hypot() <= 6.0 + 1e-9, "{tilt}: {:?}", r.delta);
        }
    }
}

mod grid_of_boxes {
    use super::*;

    fn grid(columns: u64, rows: u64) -> Vec<(u64, craft_snap::TurnedBox)> {
        (0..columns * rows).map(|i| (i, frame((i % columns) as f64 * 9.0, (i / columns) as f64 * 9.0, 6.0, 4.0, 0.0))).collect()
    }

    #[test]
    fn a_box_dropped_among_a_three_by_three_grid_passes_the_check() {
        let t = scene(&grid(3, 3), &[], &[]);
        let s = Settings::default();
        // Over the middle box, off its place by a hair, so that gaps are repeated along both directions and one number would name two chains.
        for at in [(9.4, 9.3), (4.6, 13.8), (13.4, 4.6), (9.0, 9.0), (4.5, 4.5)] {
            let q = query(box_subject(frame(at.0, at.1, 6.0, 4.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{at:?}: {b:?}"));
            let chains: Vec<(u16, f64)> = r
                .marks
                .iter()
                .filter_map(|m| if let craft_snap::Mark::Gap { chain, value, .. } = m { Some((*chain, *value)) } else { None })
                .collect();
            for (c, v) in &chains {
                assert!(chains.iter().filter(|(d, _)| d == c).all(|(_, w)| (w - v).abs() < 1e-9), "{at:?}: {chains:?}");
            }
        }
    }

    #[test]
    fn a_box_among_five_thousand_in_a_grid_passes_the_check() {
        let t = scene(&grid(400, 13), &[], &[]);
        let s = Settings::default();
        for at in [(900.4, 45.3), (1800.4, 63.3), (1.0, 1.0)] {
            let q = query(box_subject(frame(at.0, at.1, 6.0, 4.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{at:?}: {b:?}"));
        }
    }
}
