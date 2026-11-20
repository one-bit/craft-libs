//! A candidate dropped where the subject lands leaves the next place in reach to be tried.

use craft_snap::{
    Axis, Choose, Constraint, Kind, KindRule, LineSpan, MarkRule, Matching, Places, Query, Reach, Settings, SizeAxes, SpacingGaps, SpacingReach,
    SpacingRow,
};
use kurbo::Rect;

use crate::common::*;

fn hit_kinds(r: &craft_snap::Snapped) -> Vec<(Kind, Option<u64>, Option<f64>)> {
    r.hits.iter().map(|h| (h.kind, h.id.map(|i| i.0), h.direction.map(|d| d.angle()))).collect()
}

/// A spacing candidate that fails where the subject lands is dropped, and the next place in reach for its direction is tried: a line, alone or paired
/// with what the other direction kept.
mod spacing_then_the_next_place {
    use super::*;

    /// Equal gaps and the lines of boxes, ranked as an app that puts alignment after gaps would, from 6 screen units.
    fn ranked(row: SpacingRow, gaps: SpacingGaps, marks: MarkRule) -> Settings {
        let on = |p| (Reach::Screen(6.0), p);
        let mut s = only(&[(Kind::Edge, on(6).0, 6), (Kind::Centre, on(6).0, 6), (Kind::Layout, on(6).0, 6), (Kind::Spacing, on(6).0, 6)]);
        s.choose = Choose::Ranked;
        s.places = Places::First;
        s.matching = Matching::AnyToAny;
        s.align_reach = Some(Reach::Document(4.0));
        s.spacing_reach = SpacingReach { times: 4.0, at_least: 4.0 };
        s.spacing_row = row;
        s.spacing_gaps = gaps;
        s.size_axes = SizeAxes::SameAxis;
        s.marks = marks;
        s.line_span = LineSpan::Every;
        s
    }

    /// From a harness: a box (id 2) 0.5 by 1.5 and another (id 1) 2.5 by 2 farther on, the subject 1.5 by 0.5 at rest left of them, and layout lines
    /// at 0.25 and 15.75. Under the row of the beyond kind a third shape makes an equal gap as near (1.75) as the edge of box 2: the gap is kept alone
    /// for its direction and, where the subject lands, no longer repeats.
    fn probe() -> (craft_snap::Targets, Rect) {
        let boxes = [(2, Rect::new(6.25, 0.25, 6.75, 1.75)), (1, Rect::new(10.25, 0.5, 12.75, 2.5))];
        let layout = [(Axis::X, 0.25), (Axis::X, 15.75), (Axis::Y, 0.25), (Axis::Y, 15.75)];
        (world(&boxes, &[], &layout), Rect::new(3.0, 0.0, 4.5, 0.5))
    }

    fn ask(t: &craft_snap::Targets, s: &Settings, subject: Rect, zoom: f64, constraint: Constraint) -> craft_snap::Snapped {
        let q = Query { zoom, constraint, ..query(box_subject(rect_box(subject)), s) };
        let r = run(t, &q);
        r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        r
    }

    #[test]
    fn a_gap_that_fails_where_the_subject_lands_leaves_the_edge_it_was_nearer_than_to_be_met_in_a_free_move() {
        let (t, subject) = probe();
        let beyond = ranked(SpacingRow::Beyond, SpacingGaps::Neighbour, MarkRule::HitsOnly);
        let with_gaps_off = {
            let mut s = beyond.clone();
            s.kinds.set(Kind::Spacing, KindRule { on: false, ..s.kinds.rule(Kind::Spacing) });
            s
        };
        let want = ask(&t, &with_gaps_off, subject, 3.0, Constraint::None);
        assert_eq!((want.delta.x, want.delta.y), (1.75, 0.0), "the edge is 1.75 off: {:?}", want.hits);
        let r = ask(&t, &beyond, subject, 3.0, Constraint::None);
        assert_eq!((r.delta.x, r.delta.y), (1.75, 0.0), "{:?}", r.hits);
        let kinds = hit_kinds(&r);
        assert!(kinds.contains(&(Kind::Edge, Some(2), Some(0.0))), "the edge of box 2 on x is met: {kinds:?}");
        assert!(kinds.iter().all(|(k, _, _)| *k != Kind::Spacing), "the gap does not hold where the subject lands: {kinds:?}");
        // The gap was tried first (and dropped there), then the pair of the edge and what the other direction kept.
        assert_eq!(r.work.solves, 2, "{:?}", r.work);
    }

    #[test]
    fn a_gap_that_fails_where_the_subject_lands_leaves_the_edge_it_was_nearer_than_to_be_met_in_a_move_held_to_an_axis() {
        let (t, subject) = probe();
        let beyond = ranked(SpacingRow::Beyond, SpacingGaps::Neighbour, MarkRule::HitsOnly);
        let r = ask(&t, &beyond, subject, 3.0, Constraint::Axis(Axis::X));
        assert_eq!((r.delta.x, r.delta.y), (1.75, 0.0), "{:?}", r.hits);
        assert_eq!(hit_kinds(&r), vec![(Kind::Edge, Some(2), Some(0.0))]);
    }

    /// Three boxes on a row: 1 and 2 to the sides of the subject (4 to 24), and 3, which overlaps the subject along the row until the subject has moved
    /// right, and then is its neighbour: the middle between 1 and 2 (1.0 away) is not the middle between 3 and 2 where the subject lands. A guide is
    /// at `guide` across the row.
    fn middle(row: SpacingRow, gaps: SpacingGaps, choose: Choose, guide: f64, guide_priority: u8) -> (craft_snap::Targets, Settings) {
        let boxes = [(1, Rect::new(0.0, 0.0, 4.0, 2.0)), (2, Rect::new(20.0, 0.0, 24.0, 2.0)), (3, Rect::new(9.0, 0.0, 10.5, 2.0))];
        let t = world(&boxes, &[guide], &[]);
        let mut s = only(&[(Kind::Spacing, Reach::Document(5.0), 1), (Kind::Guide, Reach::Document(5.0), guide_priority)]);
        s.choose = choose;
        s.spacing_row = row;
        s.spacing_gaps = gaps;
        s.spacing_reach = SpacingReach { times: 4.0, at_least: 4.0 };
        s.marks = MarkRule::HitsOnly;
        (t, s)
    }

    #[test]
    fn the_middle_between_two_neighbours_that_fails_where_the_subject_lands_leaves_a_guide_in_reach_to_be_met() {
        let subject = Rect::new(10.0, 0.0, 12.0, 2.0);
        for (row, gaps) in
            [(SpacingRow::Beyond, SpacingGaps::Neighbour), (SpacingRow::Band, SpacingGaps::Row), (SpacingRow::Linked, SpacingGaps::Row)]
        {
            for choose in [Choose::Nearest, Choose::Ranked] {
                // A guide 1.2 from the subject's high side: farther than the middle (1.0), and of a worse priority than it.
                let (t, s) = middle(row, gaps, choose, 13.2, 3);
                let r = ask_held(&t, &s, subject);
                assert!((r.delta.x - 1.2).abs() < 1e-9 && r.delta.y == 0.0, "{row:?} {choose:?}: {:?} {:?}", r.delta, r.hits);
                assert_eq!(hit_kinds(&r), vec![(Kind::Guide, None, Some(0.0))], "{row:?} {choose:?}");
                // A guide as far as the middle (the subject's high side to 13): the tie goes to the gap, which is dropped where it lands, and the guide is met.
                let (t, s) = middle(row, gaps, choose, 13.0, 3);
                let r = ask_held(&t, &s, subject);
                assert!((r.delta.x - 1.0).abs() < 1e-9, "{row:?} {choose:?} tie: {:?} {:?}", r.delta, r.hits);
                assert_eq!(hit_kinds(&r), vec![(Kind::Guide, None, Some(0.0))], "{row:?} {choose:?} tie");
                // With nothing else in reach nothing is met, and the box stays where it was dragged, as before.
                let (t, s) = middle(row, gaps, choose, 40.0, 3);
                let r = ask_held(&t, &s, subject);
                assert_eq!((r.delta.x, r.delta.y), (0.0, 0.0), "{row:?} {choose:?} nothing in reach: {:?}", r.hits);
                assert!(r.hits.is_empty(), "{:?}", r.hits);
            }
        }
    }

    fn ask_held(t: &craft_snap::Targets, s: &Settings, subject: Rect) -> craft_snap::Snapped {
        ask(t, s, subject, 1.0, Constraint::Axis(Axis::X))
    }

    #[test]
    fn each_place_is_tried_once_and_a_cap_on_the_solves_stops_the_tries_and_says_so() {
        let subject = Rect::new(10.0, 0.0, 12.0, 2.0);
        let (t, s) = middle(SpacingRow::Beyond, SpacingGaps::Neighbour, Choose::Nearest, 13.2, 3);
        let r = ask_held(&t, &s, subject);
        // The middle (dropped) and then the guide: two combinations, one dropped, one held.
        assert_eq!(r.work.solves, 2, "{:?}", r.work);
        // One solve is the middle alone: nothing is left to try, the box stays where it was dragged and the work says the cap was met.
        let q = Query {
            constraint: Constraint::Axis(Axis::X),
            budget: craft_snap::Budget { solves: 1, ..craft_snap::Budget::default() },
            ..query(box_subject(rect_box(subject)), &s)
        };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert_eq!((r.delta.x, r.delta.y), (0.0, 0.0), "{:?}", r.hits);
        assert!(r.hits.is_empty() && r.work.truncated && r.work.cut.0 & craft_snap::Cuts::SOLVES != 0, "{:?}", r.work);
    }
}

/// A move along a ray whose reference point is off the ray is first put on it; the places it meets are on its line, and the candidates hold where it lands.
#[test]
fn a_move_along_a_ray_from_a_point_off_the_ray_meets_a_guide_where_it_lands() {
    // The box's centre is at (5, 4), off the ray through the origin along (1, 1): the box is put at (4.5, 4.5) and a guide at 4.8 pulls its middle there.
    let t = world(&[], &[4.8], &[]);
    let s = only(&[(Kind::Guide, Reach::Screen(6.0), 1)]);
    let q = Query {
        zoom: 8.0,
        constraint: Constraint::Ray { origin: kurbo::Point::ORIGIN, dir: kurbo::Vec2::new(1.0, 1.0) },
        ..query(box_subject(frame(5.0, 4.0, 2.0, 2.0, 0.0)), &s)
    };
    let r = run(&t, &q);
    r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
    assert!((r.delta.x + 0.2).abs() < 1e-9 && (r.delta.y - 0.8).abs() < 1e-9, "{:?} {:?}", r.delta, r.hits);
    assert_eq!(hit_kinds(&r), vec![(Kind::Guide, None, Some(0.0))]);
}

/// A resize whose nearest line of a kind would leave the box no width or height, or turn it inside out, is offered the next line of that kind in reach:
/// the side goes there, or stays where it was dragged when there is none.
mod resize_past_a_collapse {
    use craft_snap::{About, Edges, Object, Parts, Targets, TargetsBuilder, TurnedBox};
    use kurbo::Point;
    use proptest::prelude::*;

    use super::*;

    /// Guides across x and across y, a guide pulling from 6 screen units: 0.0833 at 72 to the unit.
    fn guides(x: &[f64], y: &[f64]) -> Targets {
        let mut b = TargetsBuilder::new(craft_snap::Limits::default());
        for g in x {
            b.line(None, Axis::X, *g, None, Kind::Guide).unwrap();
        }
        for g in y {
            b.line(None, Axis::Y, *g, None, Kind::Guide).unwrap();
        }
        b.build()
    }

    fn guides_only() -> Settings {
        only(&[(Kind::Guide, Reach::Screen(6.0), 1)])
    }

    fn resize(t: &Targets, s: &Settings, dragged: Rect, moving: Edges, constraint: Constraint) -> craft_snap::Snapped {
        let q = Query { zoom: 72.0, constraint, ..query(craft_snap::Subject::Edges { frame: rect_box(dragged), moving }, s) };
        let r = run(t, &q);
        r.check(t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        r
    }

    fn right() -> Edges {
        Edges { u_max: true, ..Edges::default() }
    }

    /// The box a resize comes to, as its sides: x0, y0, x1, y1.
    fn sides(r: &craft_snap::Snapped) -> [f64; 4] {
        let f = r.frame.expect("a resize has a box");
        [f.centre.x - 0.5 * f.size.width, f.centre.y - 0.5 * f.size.height, f.centre.x + 0.5 * f.size.width, f.centre.y + 0.5 * f.size.height]
    }

    fn close(a: [f64; 4], b: [f64; 4]) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn a_guide_that_would_leave_no_width_is_passed_over_for_the_next_one_in_reach() {
        // A box 2 by 1 at (1, 1) to (3, 2), its right side dragged to x = 1.03 with the left side fixed. The guide at 1 is the nearest (0.03) but would leave
        // the box no width; the one at 1.1 is 0.07 off, within the 0.0833 a guide pulls from.
        let drag = Rect::new(1.0, 1.0, 1.03, 2.0);
        let r = resize(&guides(&[1.0, 1.1], &[]), &guides_only(), drag, right(), Constraint::None);
        assert!(close(sides(&r), [1.0, 1.0, 1.1, 2.0]), "{:?} {:?}", sides(&r), r.hits);
        assert_eq!(hit_kinds(&r), vec![(Kind::Guide, None, Some(0.0))]);
        assert!((r.hits[0].distance - 0.07).abs() < 1e-9, "{:?}", r.hits);
        // The next line past a line that flips the box is offered as well: guides at 0.9 (the side would pass the left one) and 1.
        let r = resize(&guides(&[0.9, 1.0, 1.1], &[]), &guides_only(), drag, right(), Constraint::None);
        assert!(close(sides(&r), [1.0, 1.0, 1.1, 2.0]), "{:?} {:?}", sides(&r), r.hits);
        // With only the guide that collapses it, or a next one out of reach, the side stays where it was dragged and nothing is hit.
        for at in [vec![1.0], vec![1.0, 1.2], vec![0.9, 1.0]] {
            let r = resize(&guides(&at, &[]), &guides_only(), drag, right(), Constraint::None);
            assert!(close(sides(&r), [1.0, 1.0, 1.03, 2.0]) && r.hits.is_empty(), "{at:?}: {:?} {:?}", sides(&r), r.hits);
        }
        // A next one that is nearer than the one that collapses is the nearest in the first place.
        let r = resize(&guides(&[1.0, 1.05], &[]), &guides_only(), drag, right(), Constraint::None);
        assert!(close(sides(&r), [1.0, 1.0, 1.05, 2.0]), "{:?}", sides(&r));
    }

    #[test]
    fn it_is_the_same_on_the_other_side_and_across_y() {
        // The left side dragged to 2.97 against the right side at 3: the guide at 3 collapses it, the one at 2.9 is the next.
        let r = resize(
            &guides(&[2.9, 3.0], &[]),
            &guides_only(),
            Rect::new(2.97, 1.0, 3.0, 2.0),
            Edges { u_min: true, ..Edges::default() },
            Constraint::None,
        );
        assert!(close(sides(&r), [2.9, 1.0, 3.0, 2.0]), "{:?} {:?}", sides(&r), r.hits);
        // The top side dragged to y = 1.03, then the bottom side to 1.97 (at 2).
        let r = resize(
            &guides(&[], &[1.0, 1.1]),
            &guides_only(),
            Rect::new(1.0, 1.0, 3.0, 1.03),
            Edges { v_max: true, ..Edges::default() },
            Constraint::None,
        );
        assert!(close(sides(&r), [1.0, 1.0, 3.0, 1.1]), "{:?} {:?}", sides(&r), r.hits);
        let r = resize(
            &guides(&[], &[1.9, 2.0]),
            &guides_only(),
            Rect::new(1.0, 1.97, 3.0, 2.0),
            Edges { v_min: true, ..Edges::default() },
            Constraint::None,
        );
        assert!(close(sides(&r), [1.0, 1.9, 3.0, 2.0]), "{:?} {:?}", sides(&r), r.hits);
    }

    #[test]
    fn a_resize_that_keeps_its_proportions_passes_over_a_guide_that_would_collapse_it_too() {
        // The corner of a 2 by 1 box dragged to (1.03, 1.015) with the ratio 2 kept about the fixed corner: the guide at 1 is the nearest and would scale
        // it to nothing.
        let corner = Edges { u_max: true, v_max: true, ..Edges::default() };
        let keep = Constraint::Aspect { ratio: 2.0, about: About::Fixed };
        let r = resize(&guides(&[1.0, 1.1], &[]), &guides_only(), Rect::new(1.0, 1.0, 1.03, 1.015), corner, keep);
        assert!(close(sides(&r), [1.0, 1.0, 1.1, 1.05]), "{:?} {:?}", sides(&r), r.hits);
        let r = resize(&guides(&[1.0], &[]), &guides_only(), Rect::new(1.0, 1.0, 1.03, 1.015), corner, keep);
        assert!(r.hits.is_empty(), "{:?}", r.hits);
    }

    #[test]
    fn a_width_that_would_leave_no_size_is_passed_over_for_the_next_width_in_reach() {
        // Another shape has no width (a vertical line) and another is 0.12 wide; the box dragged to a width of 0.05 is nearer to nothing (0.05) than to
        // 0.12 (0.07), and matching nothing would collapse it.
        let boxes = [(1, Rect::new(5.0, 0.0, 5.0, 3.0)), (2, Rect::new(7.0, 0.0, 7.12, 3.0))];
        let t = world(&boxes, &[], &[]);
        let s = only(&[(Kind::Size, Reach::Screen(6.0), 1)]);
        let r = resize(&t, &s, Rect::new(1.0, 1.0, 1.05, 2.0), right(), Constraint::None);
        assert!(close(sides(&r), [1.0, 1.0, 1.12, 2.0]), "{:?} {:?}", sides(&r), r.hits);
        assert_eq!(hit_kinds(&r), vec![(Kind::Size, Some(2), None)]);
    }

    #[test]
    fn a_hundred_thousand_guides_beside_a_collapsing_one_cost_one_more_bisection_of_each_kind_and_say_nothing_is_cut() {
        // Guides every 0.001 from 1 to 101. The side dragged to 1.0004 is nearest the guide at 1 (which would leave no width) and the next, 0.0006 away,
        // is in reach. The yardstick is the same query dragged to 1.0006, whose nearest guide (1.001) is fine.
        let at: Vec<f64> = (0..100_000).map(|i| 1.0 + f64::from(i) * 0.001).collect();
        let t = guides(&at, &[]);
        let s = guides_only();
        let collapsing = resize(&t, &s, Rect::new(1.0, 1.0, 1.0004, 2.0), right(), Constraint::None);
        let yardstick = resize(&t, &s, Rect::new(1.0, 1.0, 1.0006, 2.0), right(), Constraint::None);
        assert!(
            close(sides(&collapsing), [1.0, 1.0, 1.001, 2.0]) && close(sides(&yardstick), [1.0, 1.0, 1.001, 2.0]),
            "{:?} {:?}",
            collapsing.hits,
            yardstick.hits
        );
        assert_eq!((collapsing.work.cut, collapsing.work.truncated), (craft_snap::Cuts(0), false));
        // Three points of the dragged side meet the vertical guides: each asks once more, for the guide past the one that collapses it, which is a
        // bisection of the one direction and of the 100,000 guides (1 and 17 steps). Measured: 168 against 114.
        assert_eq!((collapsing.work.bisect_steps, yardstick.work.bisect_steps), (168, 114));
        // Nothing else grows: no objects were gathered and no more lines were walked.
        assert_eq!(
            (collapsing.work.visited, collapsing.work.line_steps, collapsing.work.solves),
            (yardstick.work.visited, yardstick.work.line_steps, yardstick.work.solves)
        );
    }

    #[test]
    fn a_size_match_offers_the_nearest_length_alone_and_the_next_only_when_that_would_leave_no_size() {
        // Other shapes are 0.2 and 0.3 wide; the side is dragged to a width of 0.24, nearer to 0.2 (0.04) than to 0.3 (0.06). A lattice of 0.65 holds
        // the right side at 1.3 (a width of 0.3) and takes 1.2 (a width of 0.2) to it. The nearest length is the candidate, and it does not hold on the
        // lattice: no other length is tried, and the box is the lattice's, with nothing hit.
        let boxes = [(1, Rect::new(5.0, 0.0, 5.2, 3.0)), (2, Rect::new(7.0, 0.0, 7.3, 3.0))];
        let t = world(&boxes, &[], &[]);
        let s = only(&[(Kind::Size, Reach::Screen(6.0), 1)]);
        let q = Query {
            zoom: 72.0,
            rounding: craft_snap::Rounding::Position(kurbo::Vec2::new(0.65, 0.65)),
            ..query(craft_snap::Subject::Edges { frame: rect_box(Rect::new(1.0, 1.0, 1.24, 2.0)), moving: right() }, &s)
        };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(close(sides(&r), [1.0, 1.0, 1.3, 2.0]) && r.hits.is_empty(), "{:?} {:?}", sides(&r), r.hits);
        // Without the lattice the nearest is met.
        let r = resize(&t, &s, Rect::new(1.0, 1.0, 1.24, 2.0), right(), Constraint::None);
        assert!(close(sides(&r), [1.0, 1.0, 1.2, 2.0]), "{:?}", sides(&r));
        assert_eq!(hit_kinds(&r), vec![(Kind::Size, Some(1), None)]);
    }

    /// The V25 numbers in the own frame of a box turned by `turn`: other boxes turned alike have edges at 1 and 1.1 along the box's own x. The side of the
    /// box that moves is its own (max x) side; the one that stays is at 1. `mirror` turns the box over, which swaps which own side is the max one.
    fn turned_case(turn: f64, drag: f64, flip: bool) -> craft_snap::Snapped {
        let at = |x: f64, y: f64| {
            let (s, c) = turn.sin_cos();
            Point::new(c * x - s * y, s * x + c * y)
        };
        // (The boxes are higher up than the one resized, so that none of their own sides lies along the sides of it.)
        let obj = |id: u64, x0: f64, x1: f64| (id, TurnedBox { centre: at(0.5 * (x0 + x1), 3.5), size: kurbo::Size::new(x1 - x0, 1.0), angle: turn });
        // Edges at 1 and 1.1 for the side that moves at the max end; at 1 and 0.9 for the min side that moves, the max side being fixed at 1.
        let boxes = if flip { [obj(1, 0.4, 0.9), obj(2, 1.0, 1.5)] } else { [obj(1, 0.5, 1.0), obj(2, 1.1, 1.6)] };
        let mut b = TargetsBuilder::new(craft_snap::Limits::default());
        for (id, f) in boxes {
            b.object(craft_snap::Id(id), Object { frame: f, bounds: None, angle: None, parts: Parts::ALL }).unwrap();
        }
        let t = b.build();
        let mut s = only(&[(Kind::Edge, Reach::Screen(6.0), 1)]);
        s.frames = craft_snap::Frames { page: false, own: true };
        // The box as dragged, in its own frame (1, 1) to (drag, 2); flipped, the side that moves is the min one, the box running from `drag` to 1.
        let (x0, x1) = if flip { (drag, 1.0) } else { (1.0, drag) };
        let frame = TurnedBox { centre: at(0.5 * (x0 + x1), 1.5), size: kurbo::Size::new(x1 - x0, 1.0), angle: turn };
        let moving = if flip { Edges { u_min: true, ..Edges::default() } } else { right() };
        let q = Query { zoom: 72.0, ..query(craft_snap::Subject::Edges { frame, moving }, &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        r
    }

    #[test]
    fn it_is_the_same_in_the_own_frame_of_a_turned_box_and_for_the_other_side_of_it() {
        let x = |r: &craft_snap::Snapped, turn: f64| {
            let f = r.frame.expect("a resize has a box");
            let (s, c) = turn.sin_cos();
            // The centre and the width in the box's own frame.
            (c * f.centre.x + s * f.centre.y, f.size.width)
        };
        for turn in [0.5, 1.2, 2.9, -0.7] {
            let r = turned_case(turn, 1.03, false);
            let (centre, width) = x(&r, turn);
            assert!((width - 0.1).abs() < 1e-9 && (centre - 1.05).abs() < 1e-9, "turn {turn}: {centre} {width} {:?}", r.hits);
            assert_eq!(r.hits.len(), 1, "{:?}", r.hits);
            assert_eq!((r.hits[0].kind, r.hits[0].id), (Kind::Edge, Some(craft_snap::Id(2))));
            // Out of reach of the next edge (0.0999 away), the side stays where it was dragged, and nothing is hit.
            let r = turned_case(turn, 1.0001, false);
            let (centre, width) = x(&r, turn);
            assert!(
                (width - 0.0001).abs() < 1e-9 && (centre - 1.00005).abs() < 1e-9 && r.hits.is_empty(),
                "turn {turn} near: {centre} {width} {:?}",
                r.hits
            );
            // The side that is at the min end: dragged to 0.97 against the fixed one at 1, the edge at 1 would collapse it and the one at 0.9 is next.
            let r = turned_case(turn, 0.97, true);
            let (centre, width) = x(&r, turn);
            assert!((width - 0.1).abs() < 1e-9 && (centre - 0.95).abs() < 1e-9, "turn {turn} min side: {centre} {width} {:?}", r.hits);
            assert_eq!((r.hits.len(), r.hits[0].kind, r.hits[0].id), (1, Kind::Edge, Some(craft_snap::Id(1))));
        }
    }

    /// A fixed seed, so that a run is the same run each time; `CRAFT_SNAP_CASES` and `CRAFT_SNAP_SEED` run a longer search, and each find is a case to read.
    fn config() -> ProptestConfig {
        let number = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(default);
        ProptestConfig {
            cases: u32::try_from(number("CRAFT_SNAP_CASES", 600)).unwrap_or(600),
            rng_seed: proptest::test_runner::RngSeed::Fixed(number("CRAFT_SNAP_SEED", 0x2025)),
            ..ProptestConfig::default()
        }
    }

    proptest! {
        #![proptest_config(config())]

        /// Against the rule written out: the right side lands on the nearest line in reach (a guide from 6 screen units, a layout line from 9) that leaves
        /// the box a width (two as near are both right), and stays where it was dragged when there is none.
        #[test]
        fn the_right_side_goes_to_the_nearest_line_in_reach_that_leaves_a_width(
            guide_at in prop::collection::vec(80i32..140, 0..5), layout_at in prop::collection::vec(80i32..140, 0..5), drag in 101i32..140,
        ) {
            let to_x = |v: &Vec<i32>| -> Vec<f64> { v.iter().map(|v| f64::from(*v) * 0.01).collect() };
            let (guides_at, layout_at) = (to_x(&guide_at), to_x(&layout_at));
            let x = f64::from(drag) * 0.01 - 0.0025;
            let mut b = TargetsBuilder::new(craft_snap::Limits::default());
            for g in &guides_at {
                b.line(None, Axis::X, *g, None, Kind::Guide).unwrap();
            }
            for g in &layout_at {
                b.line(None, Axis::X, *g, None, Kind::Layout).unwrap();
            }
            let s = only(&[(Kind::Guide, Reach::Screen(6.0), 1), (Kind::Layout, Reach::Screen(9.0), 1)]);
            let r = resize(&b.build(), &s, Rect::new(1.0, 1.0, x, 2.0), right(), Constraint::None);
            let mut ok: Vec<f64> = guides_at.iter().filter(|g| (*g - x).abs() <= 6.0 / 72.0).chain(layout_at.iter().filter(|g| (*g - x).abs() <= 9.0 / 72.0)).copied().filter(|g| *g - 1.0 > 1e-6).collect();
            ok.sort_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()));
            let want: Vec<f64> = match ok.first() {
                None => vec![x],
                Some(first) => ok.iter().copied().filter(|g| ((g - x).abs() - (first - x).abs()).abs() < 1e-9).collect(),
            };
            let got = sides(&r)[2];
            prop_assert!(want.iter().any(|w| (got - w).abs() < 1e-9), "got {} want one of {:?} (guides {:?}, layout {:?}, dragged to {})", got, want, guides_at, layout_at, x);
            prop_assert_eq!(r.hits.is_empty(), ok.is_empty());
        }
    }
}
