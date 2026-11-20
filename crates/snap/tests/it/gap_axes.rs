//! The gap marks of a move that repeats a gap in each of two directions.

use craft_snap::{Choose, Kind, Mark, MarkRule, Places, Query, Reach, Settings, SpacingReach, SpacingRow};
use kurbo::Rect;

use crate::common::*;

/// Two spacing hits, one on each axis, draw the gap marks of both axes when only the hits are marked.
mod gap_marks_of_two_axes {
    use super::*;

    /// A cross: boxes to the left and right (rows along x) and above and below (along y) of a subject 4 by 4, 0.3 right of the middle of the one pair
    /// and 0.2 below the middle of the other.
    fn cross() -> (craft_snap::Targets, Rect) {
        let boxes = [
            (1, Rect::new(0.0, 9.0, 4.0, 15.0)),
            (2, Rect::new(20.0, 9.0, 24.0, 15.0)),
            (3, Rect::new(9.0, 0.0, 15.0, 4.0)),
            (4, Rect::new(9.0, 20.0, 15.0, 24.0)),
        ];
        (world(&boxes, &[], &[]), Rect::new(10.3, 10.2, 14.3, 14.2))
    }

    fn gaps_of(r: &craft_snap::Snapped) -> Vec<(f64, f64, bool)> {
        let mut v: Vec<(f64, f64, bool)> = r
            .marks
            .iter()
            .filter_map(|m| if let Mark::Gap { direction, value, hit, .. } = m { Some((direction.angle(), *value, *hit)) } else { None })
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        v
    }

    fn settings(row: SpacingRow, marks: MarkRule) -> Settings {
        let mut s = only(&[(Kind::Spacing, Reach::Screen(2.0), 6)]);
        s.choose = Choose::Ranked;
        s.places = Places::First;
        s.align_reach = Some(Reach::Document(40.0));
        s.spacing_reach = SpacingReach { times: 4.0, at_least: 4.0 };
        s.spacing_row = row;
        s.marks = marks;
        s
    }

    #[test]
    fn two_spacing_hits_one_on_each_axis_draw_the_gaps_of_both_axes_under_every_row_and_either_mark_rule() {
        let (t, subject) = cross();
        for row in [SpacingRow::Band, SpacingRow::Linked, SpacingRow::Beyond] {
            for marks in [MarkRule::Coincident, MarkRule::HitsOnly] {
                let s = settings(row, marks);
                let q = Query { zoom: 1.0, ..query(box_subject(rect_box(subject)), &s) };
                let r = run(&t, &q);
                r.check(&t, &q).unwrap_or_else(|b| panic!("{row:?} {marks:?}: {b:?}\n{r:?}"));
                // The middle on both axes: 0.3 left and 0.2 up.
                assert!((r.delta.x + 0.3).abs() < 1e-9 && (r.delta.y + 0.2).abs() < 1e-9, "{row:?} {marks:?}: {:?} {:?}", r.delta, r.hits);
                let spacing: Vec<_> = r.hits.iter().filter(|h| h.kind == Kind::Spacing).collect();
                assert_eq!(spacing.len(), 2, "one hit on each axis: {:?}", r.hits);
                let want =
                    vec![(0.0, 6.0, true), (0.0, 6.0, true), (std::f64::consts::FRAC_PI_2, 6.0, true), (std::f64::consts::FRAC_PI_2, 6.0, true)];
                assert_eq!(gaps_of(&r), want, "{row:?} {marks:?}");
            }
        }
    }

    #[test]
    fn an_axis_that_made_no_spacing_hit_draws_no_gap_under_hits_only_and_one_that_did_draws_its_own() {
        // The same cross with the boxes above and below too far apart to repeat a gap within the 2 of reach (their middle is 4.3 away): only the row
        // along x snaps.
        let boxes = [
            (1, Rect::new(0.0, 9.0, 4.0, 15.0)),
            (2, Rect::new(20.0, 9.0, 24.0, 15.0)),
            (3, Rect::new(9.0, 0.0, 15.0, 4.0)),
            (4, Rect::new(9.0, 29.0, 15.0, 33.0)),
        ];
        let t = world(&boxes, &[], &[]);
        let subject = Rect::new(10.3, 10.2, 14.3, 14.2);
        for row in [SpacingRow::Band, SpacingRow::Linked, SpacingRow::Beyond] {
            let s = settings(row, MarkRule::HitsOnly);
            let q = Query { zoom: 1.0, ..query(box_subject(rect_box(subject)), &s) };
            let r = run(&t, &q);
            r.check(&t, &q).unwrap_or_else(|b| panic!("{row:?}: {b:?}\n{r:?}"));
            assert_eq!(r.hits.iter().filter(|h| h.kind == Kind::Spacing).count(), 1, "{row:?}: {:?}", r.hits);
            assert_eq!(gaps_of(&r), vec![(0.0, 6.0, true), (0.0, 6.0, true)], "{row:?}");
        }
    }
}
