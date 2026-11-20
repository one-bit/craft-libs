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
