//! A nudge of an app whose alignment has a reach across: the next line of a kind ahead of a side is found even when the nearest line of that kind is
//! behind it.

use craft_snap::{Axis, Kind, NudgeReach, Query, Reach, Settings, Snapped, Subject};
use kurbo::Rect;

use crate::common::*;

/// Boxes A, 0 to 2 (id 1), and B, 5 to 7 (id 2), a box between them and the edges and centres of boxes on from 6 screen units, at 72 to the unit.
fn nudge(reach: Option<f64>, between: Rect, forward: bool, extra: &[(u64, Rect)]) -> Snapped {
    let mut boxes = vec![(1, Rect::new(0.0, 0.0, 2.0, 1.0)), (2, Rect::new(5.0, 0.0, 7.0, 1.0))];
    boxes.extend_from_slice(extra);
    let t = world(&boxes, &[], &[]);
    let mut s: Settings = only(&[(Kind::Edge, Reach::Screen(6.0), 3), (Kind::Centre, Reach::Screen(6.0), 3)]);
    s.align_reach = reach.map(Reach::Document);
    let subject = Subject::Nudge { frame: rect_box(between), bounds: None, axis: Axis::X, forward, reach: NudgeReach::Anywhere, fallback: 0.125 };
    let q = Query { zoom: 72.0, ..query(subject, &s) };
    let r = run(&t, &q);
    r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
    r
}

fn stop(r: &Snapped) -> (f64, Vec<(Kind, Option<u64>)>) {
    (r.delta.x, r.hits.iter().map(|h| (h.kind, h.id.map(|i| i.0))).collect())
}

#[test]
fn a_nudge_finds_the_edge_ahead_when_the_nearest_edge_of_its_kind_is_behind_with_a_reach_across_as_without_one() {
    // A box from 2 to 3 right of A: the nearest edge to each of its sides is A's (2 is the side's own place, behind it), and the stop is B's edge at 5, 2 from
    // the high side. By a reach across alignment the objects near the subject are read one by one, the nearest of each kind first.
    for reach in [None, Some(20.0)] {
        let (dx, hits) = stop(&nudge(reach, Rect::new(2.0, 0.0, 3.0, 1.0), true, &[]));
        assert_eq!((dx, hits), (2.0, vec![(Kind::Edge, Some(2))]), "reach {reach:?}");
        // 0.4 farther along: the high side is at 3.4 and B's edge 1.6 ahead.
        let (dx, hits) = stop(&nudge(reach, Rect::new(2.4, 0.0, 3.4, 1.0), true, &[]));
        assert!((dx - 1.6).abs() < 1e-9 && hits == vec![(Kind::Edge, Some(2))], "reach {reach:?}: {dx} {hits:?}");
        // Backward, from a box 4 to 5 left of B: A's edge at 2 is 2 behind the low side, and B's own edge at 5 is the nearest of the kind to each side.
        let (dx, hits) = stop(&nudge(reach, Rect::new(4.0, 0.0, 5.0, 1.0), false, &[]));
        assert_eq!((dx, hits), (-2.0, vec![(Kind::Edge, Some(1))]), "reach {reach:?} backward");
    }
}

#[test]
fn a_line_the_same_place_as_a_side_is_no_stop_for_a_nudge_with_a_reach_across() {
    // A shape of no width at 3 + 5e-10 is the same place as a box of no width at 3: it is not ahead of it (the rule is 1e-9), and the stop is B's edge.
    let wall = (3, Rect::new(3.0 + 5.0e-10, 0.0, 3.0 + 5.0e-10, 1.0));
    for reach in [None, Some(20.0)] {
        let (dx, hits) = stop(&nudge(reach, Rect::new(3.0, 0.0, 3.0, 1.0), true, &[wall]));
        assert_eq!((dx, hits), (2.0, vec![(Kind::Edge, Some(2))]), "reach {reach:?}");
    }
}
