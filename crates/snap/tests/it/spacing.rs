//! Equal spacing: one gap snapped, every equal gap marked, in a row of any direction.

use craft_snap::{Kind, Mark, Settings, SpacingReach};

use crate::common::*;

/// Boxes 10 wide in a row with the gaps 20, 20, 25 between them (x of their left sides 0, 30, 60, 95).
fn row() -> Vec<(u64, craft_snap::TurnedBox)> {
    [0.0, 30.0, 60.0, 95.0].iter().enumerate().map(|(i, x)| (i as u64, frame(x + 5.0, 0.0, 10.0, 10.0, 0.0))).collect()
}

/// Settings whose rows reach 200 along: the neighbours of a gap may be that far off.
fn wide() -> Settings {
    Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 200.0 }, ..Settings::default() }
}

#[test]
fn a_box_dropped_after_a_row_repeats_its_gap_and_every_equal_gap_is_marked_as_one_chain() {
    let t = scene(&row(), &[], &[]);
    let s = wide();
    // The box is 10 wide with its left side at 125 - 2 = 123: the row's last box ends at 105, the gap there is 18, near the 20 of the others.
    let f = frame(123.0 + 5.0, 0.0, 10.0, 10.0, 0.0);
    let q = query(box_subject(f), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    let after = r.frame.unwrap();
    assert!((after.centre.x - 5.0 - 125.0).abs() < 1e-9, "{:?}", after);
    let gaps: Vec<&Mark> = r.marks.iter().filter(|m| matches!(m, Mark::Gap { .. })).collect();
    assert_eq!(gaps.len(), 3, "the 20, 20 and the subject's 20: {:?}", r.marks);
    assert!(gaps.iter().all(|m| matches!(m, Mark::Gap { value, .. } if (value - 20.0).abs() < 1e-9)));
    let chains: std::collections::BTreeSet<u16> =
        gaps.iter().filter_map(|m| if let Mark::Gap { chain, .. } = m { Some(*chain) } else { None }).collect();
    assert_eq!(chains.len(), 2, "the two row gaps run on from one another, the subject's follows the 5: a break");
    assert!(gaps.iter().any(|m| matches!(m, Mark::Gap { subject: true, hit: true, .. })));
    assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing));
}

#[test]
fn a_box_between_two_neighbours_is_centred_in_the_room() {
    let t = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(95.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
    let s = wide();
    let f = frame(47.0, 0.0, 20.0, 10.0, 0.0);
    let q = query(box_subject(f), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!((r.frame.unwrap().centre.x - 50.0).abs() < 1e-9, "{:?}", r.frame);
}

#[test]
fn a_turned_row_is_marked_along_its_own_direction() {
    let a = 30f64.to_radians();
    let at = |d: f64| -> (f64, f64) { (d * a.cos(), d * a.sin()) };
    let objects: Vec<(u64, craft_snap::TurnedBox)> =
        [0.0, 30.0, 60.0].iter().enumerate().map(|(i, d)| (i as u64, frame(at(*d).0, at(*d).1, 10.0, 10.0, a))).collect();
    let t = scene(&objects, &[], &[]);
    let s = wide();
    let p = at(90.0);
    let f = frame(p.0, p.1, 10.0, 10.0, a);
    let q = query(box_subject(f), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.marks.iter().any(|m| matches!(m, Mark::Gap { direction, .. } if (direction.angle() - a).abs() < 1e-9)), "{:?}", r.marks);
}

#[test]
fn a_staircase_is_a_row_when_linked_and_not_when_banded() {
    use craft_snap::SpacingRow;
    // Two boxes before the subject: the near one overlaps the subject across, the far one overlaps only the near one, and the gap between them is 7.
    let t = scene(&[(1, frame(22.0, 10.0, 10.0, 10.0, 0.0)), (2, frame(5.0, 17.0, 10.0, 10.0, 0.0))], &[], &[]);
    let landing = |row: SpacingRow| {
        let mut s = Settings { spacing_row: row, ..wide() };
        for k in [Kind::Corner, Kind::SideMiddle, Kind::Edge, Kind::Centre] {
            s.kinds.set(k, craft_snap::KindRule { on: false, reach: craft_snap::Reach::Screen(6.0), priority: 1, scope: craft_snap::Scope::All });
        }
        // The near box ends at 27; repeating the gap of 7 puts the subject's low side at 34. It is at 32.
        let q = query(box_subject(frame(37.0, 5.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r
    };
    let banded = landing(SpacingRow::Band);
    assert_eq!(banded.delta.x, 0.0, "{:?}", banded.hits);
    let linked = landing(SpacingRow::Linked);
    assert!((linked.delta.x - 2.0).abs() < 1e-9 && linked.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?} {:?}", linked.delta, linked.hits);
    assert!(linked.marks.iter().any(|m| matches!(m, Mark::Gap { value, hit: true, .. } if (value - 7.0).abs() < 1e-9)), "{:?}", linked.marks);
}

#[test]
fn gaps_of_different_values_never_share_a_chain() {
    // Gaps of 2, then the subject's 2 and its 3, then a 3: two values are each repeated, and the runs of the two are two chains.
    let objs = [
        (1, frame(5.0, 0.0, 10.0, 10.0, 0.0)),
        (2, frame(17.0, 0.0, 10.0, 10.0, 0.0)),
        (3, frame(42.0, 0.0, 10.0, 10.0, 0.0)),
        (4, frame(55.0, 0.0, 10.0, 10.0, 0.0)),
    ];
    let t = scene(&objs, &[], &[]);
    let s = wide();
    let q = query(box_subject(frame(29.0, 0.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    let marks: Vec<(f64, u16)> =
        r.marks.iter().filter_map(|m| if let Mark::Gap { value, chain, .. } = m { Some((*value, *chain)) } else { None }).collect();
    assert_eq!(marks.len(), 4, "{marks:?}");
    let chains: std::collections::BTreeSet<u16> = marks.iter().map(|m| m.1).collect();
    assert_eq!(chains.len(), 2, "{marks:?}");
    assert!(marks.iter().all(|(v, c)| marks.iter().filter(|(_, d)| d == c).all(|(w, _)| (v - w).abs() < 1e-9)), "{marks:?}");
}

#[test]
fn a_chain_number_is_one_chain_across_both_directions_of_the_page() {
    // A row of gaps of 2 along x, and one of gaps of 3 along y, both repeated by the box.
    let objs = [
        (1, frame(5.0, 5.0, 10.0, 10.0, 0.0)),
        (2, frame(17.0, 5.0, 10.0, 10.0, 0.0)),
        (3, frame(29.0, 18.0, 10.0, 10.0, 0.0)),
        (4, frame(29.0, 31.0, 10.0, 10.0, 0.0)),
    ];
    let t = scene(&objs, &[], &[]);
    let s = wide();
    let q = query(box_subject(frame(29.0, 5.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    let marks: Vec<(f64, u16)> =
        r.marks.iter().filter_map(|m| if let Mark::Gap { value, chain, .. } = m { Some((*value, *chain)) } else { None }).collect();
    assert_eq!(marks.len(), 4, "{marks:?}");
    let chains: std::collections::BTreeSet<u16> = marks.iter().map(|m| m.1).collect();
    assert_eq!(chains.len(), 2, "one number for each of the two chains: {marks:?}");
}

#[test]
fn an_object_that_only_touches_the_band_across_is_not_in_the_row_wherever_the_scene_is() {
    // The edge snap puts the box's side on a neighbour's edge, so the neighbour touches the box's band across; whether the last bits say it overlaps by
    // a hair must not decide whether the row holds it, or the answer follows the noise of the numbers instead of the scene.
    let s = Settings::default();
    let story = |far: f64| {
        let by = kurbo::Vec2::new(far, -far * 0.5);
        let objs = [(0, frame(9.5, 10.5, 1.5, 1.5, 0.0)), (1, frame(8.0, -1.5, 2.5, 3.5, 0.7))]
            .map(|(i, f)| (i, craft_snap::TurnedBox { centre: f.centre + by, ..f }));
        let t = scene(&objs, &[], &[]);
        let f = frame(11.379139162409112 + by.x, 4.686806487458091 + by.y, 1.0770963054045937, 5.367554291708136, 0.3);
        let q = query(box_subject(f), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r.hits.iter().map(|h| (h.kind, h.id)).collect::<Vec<_>>()
    };
    let near = story(0.0);
    for far in [1.0, 10.0, 1.0e3, 1.0e4, 1.0e6] {
        assert_eq!(story(far), near, "{far}");
    }
}

#[test]
fn boxes_that_overlap_in_a_row_give_the_same_gaps_read_from_either_end() {
    // Two boxes on one centre, one taller, and a third beyond: the gap that counts is between the box that ends last and the one that follows. The
    // scene turned about the page gives the same gap, and the box lands where the turned scene's lands.
    let s = Settings { frames: craft_snap::Frames { page: false, own: true }, align_reach: None, ..Settings::default() };
    let objs = [frame(0.0, -8.5, 1.0, 1.0, 0.7), frame(0.0, -8.5, 1.0, 1.5, 0.7), frame(3.0, -10.5, 1.0, 1.0, 0.7)];
    let subject = frame(-3.5698062013741434, 0.0, 5.535752170506508, 2.7167502578062814, 0.7);
    let turn = |f: &craft_snap::TurnedBox, a: f64| {
        let (sn, c) = a.sin_cos();
        let d = f.centre.to_vec2();
        craft_snap::TurnedBox { centre: kurbo::Point::new(c * d.x - sn * d.y, sn * d.x + c * d.y), size: f.size, angle: f.angle + a }
    };
    let run_at = |a: f64| {
        let numbered: Vec<(u64, craft_snap::TurnedBox)> = objs.iter().enumerate().map(|(i, o)| (i as u64, turn(o, a))).collect();
        let t = scene(&numbered, &[], &[]);
        let q = query(box_subject(turn(&subject, a)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let gaps: Vec<f64> = r.marks.iter().filter_map(|m| if let Mark::Gap { value, .. } = m { Some(*value) } else { None }).collect();
        let back = {
            let (sn, c) = (-a).sin_cos();
            kurbo::Vec2::new(c * r.delta.x - sn * r.delta.y, sn * r.delta.x + c * r.delta.y)
        };
        (back, gaps)
    };
    let (delta, gaps) = run_at(0.0);
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    for a in [0.5, 1.2, std::f64::consts::FRAC_PI_2, 2.0, std::f64::consts::PI] {
        let (d, g) = run_at(a);
        assert!((d - delta).hypot() < 1e-6, "{a}: {d:?} against {delta:?}");
        assert!(g.len() == 1 && (g[0] - gaps[0]).abs() < 1e-6, "{a}: {g:?} against {gaps:?}");
    }
}

#[test]
fn a_row_reads_the_strip_the_reach_describes_and_not_the_box_round_it() {
    // The window the objects are gathered from is the box round the strip, and a strip across the page has corners that are not in it. An object there
    // is not a neighbour, and whether it is must not depend on which way the scene is turned.
    let s = Settings { frames: craft_snap::Frames { page: false, own: true }, align_reach: None, ..Settings::default() };
    let objs = [frame(-6.5, 1.0, 4.5, 2.0, 1.1), frame(0.0, 0.0, 1.0, 1.0, 1.1)];
    let subject = frame(7.906768168512113, -2.754056203080976, 3.806493300404496, 2.861341064299773, 1.1);
    let turn = |f: &craft_snap::TurnedBox, a: f64| {
        let (sn, c) = a.sin_cos();
        let d = f.centre.to_vec2();
        craft_snap::TurnedBox { centre: kurbo::Point::new(c * d.x - sn * d.y, sn * d.x + c * d.y), size: f.size, angle: f.angle + a }
    };
    let story = |a: f64| {
        let numbered: Vec<(u64, craft_snap::TurnedBox)> = objs.iter().enumerate().map(|(i, o)| (i as u64, turn(o, a))).collect();
        let t = scene(&numbered, &[], &[]);
        let q = query(box_subject(turn(&subject, a)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let gaps = r.marks.iter().filter(|m| matches!(m, Mark::Gap { .. })).count();
        let mut pulls: Vec<f64> = r.hits.iter().map(|h| h.distance).collect();
        pulls.sort_by(f64::total_cmp);
        (gaps, pulls)
    };
    let (gaps, pulls) = story(0.0);
    for a in [0.5, 1.0, 2.0, 3.0, 4.3, 4.5, 4.6, 4.7, 5.5, 6.0] {
        let (g, p) = story(a);
        assert_eq!(g, gaps, "{a}");
        assert!(p.len() == pulls.len() && p.iter().zip(&pulls).all(|(x, y)| (x - y).abs() < 1e-6), "{a}: {p:?} against {pulls:?}");
    }
}

#[test]
fn only_the_gap_the_snap_repeated_is_marked_when_the_marks_are_asked_to_be_the_hits() {
    // Gaps of 20 (A to B) and of 30 (C to D), and the box between B and C with a gap of 20 on one side and of 30 on the other: both are repeated.
    let objs = [
        (1, frame(5.0, 0.0, 10.0, 10.0, 0.0)),
        (2, frame(35.0, 0.0, 10.0, 10.0, 0.0)),
        (3, frame(105.0, 0.0, 10.0, 10.0, 0.0)),
        (4, frame(145.0, 0.0, 10.0, 10.0, 0.0)),
    ];
    let t = scene(&objs, &[], &[]);
    let gap_values = |marks: craft_snap::MarkRule| {
        let s = Settings { marks, ..wide() };
        let q = query(box_subject(frame(65.2, 0.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        let mut v: Vec<f64> = r.marks.iter().filter_map(|m| if let Mark::Gap { value, .. } = m { Some(*value) } else { None }).collect();
        v.sort_by(f64::total_cmp);
        let neighbour = r.hits.iter().find(|h| h.kind == Kind::Spacing).and_then(|h| h.id);
        (v, neighbour)
    };
    let (every, _) = gap_values(craft_snap::MarkRule::Coincident);
    assert_eq!(every.len(), 4, "{every:?}");
    let (hits, neighbour) = gap_values(craft_snap::MarkRule::HitsOnly);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert!((hits[0] - hits[1]).abs() < 1e-9, "the two of the one value: {hits:?}");
    // The value is that of the gap beside the box the snap measured from: B (a gap of 20) or C (a gap of 30).
    let beside = if neighbour == Some(craft_snap::Id(2)) { 20.0 } else { 30.0 };
    assert!(neighbour == Some(craft_snap::Id(2)) || neighbour == Some(craft_snap::Id(3)), "{neighbour:?}");
    assert!((hits[0] - beside).abs() < 1e-9, "{neighbour:?}: {hits:?}");
}

#[test]
fn an_object_whose_end_is_exactly_the_reach_from_the_box_is_a_neighbour_on_either_side() {
    // The box is at 300 to 310 and the row reaches 100 along (4 times its length is 40, the least is 100): the strip starts at 200. The first box ends
    // there, and its gap of 30 to the second is the gap the subject repeats from the second's end at 270.
    let s = Settings { spacing_reach: SpacingReach { times: 4.0, at_least: 100.0 }, ..Settings::default() };
    let t = scene(&[(1, frame(190.0, 0.0, 20.0, 10.0, 0.0)), (2, frame(250.0, 0.0, 40.0, 10.0, 0.0))], &[], &[]);
    let q = query(box_subject(frame(305.0, 0.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?}", r.hits);
    assert_eq!(r.marks.iter().filter(|m| matches!(m, Mark::Gap { .. })).count(), 2, "{:?}", r.marks);
    // The same the other way: the box is at -310 to -300, the second box starts at -270, and the last starts at -200, exactly the reach past the box's
    // high side (-300).
    let t = scene(&[(1, frame(-250.0, 0.0, 40.0, 10.0, 0.0)), (2, frame(-190.0, 0.0, 20.0, 10.0, 0.0))], &[], &[]);
    let q = query(box_subject(frame(-305.0, 0.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?}", r.hits);
    assert_eq!(r.marks.iter().filter(|m| matches!(m, Mark::Gap { .. })).count(), 2, "{:?}", r.marks);
}

#[test]
fn a_chained_row_reaches_an_object_whose_far_edge_is_exactly_the_reach_across_on_either_side() {
    // A staircase going away across the page: the nearest box overlaps the subject's band, the next overlaps that one, and the last overlaps the second
    // and ends across at exactly the reach (120) from the band. Its gap of 20 to the second is the gap the subject repeats from the nearest.
    let s =
        Settings { spacing_row: craft_snap::SpacingRow::Linked, spacing_reach: SpacingReach { times: 4.0, at_least: 120.0 }, ..Settings::default() };
    let stairs = [
        // Below the band (across smaller): the last ends at -120.
        [(1, frame(90.0, -3.0, 10.0, 10.0, 0.0)), (2, frame(40.0, -73.0, 20.0, 134.0, 0.0)), (3, frame(5.0, -125.0, 10.0, 10.0, 0.0))],
        // Above it: the last starts at 130, which is 120 past the band's 10.
        [(1, frame(90.0, 13.0, 10.0, 10.0, 0.0)), (2, frame(40.0, 83.0, 20.0, 134.0, 0.0)), (3, frame(5.0, 135.0, 10.0, 10.0, 0.0))],
    ];
    for objs in stairs {
        let t = scene(&objs, &[], &[]);
        let q = query(box_subject(frame(120.0, 5.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing), "{objs:?}: {:?}", r.hits);
        assert_eq!(r.marks.iter().filter(|m| matches!(m, Mark::Gap { .. })).count(), 2, "{:?}", r.marks);
    }
}

#[test]
fn a_box_that_ends_exactly_where_the_next_starts_fills_the_gap_a_box_before_it_would_make() {
    // The first box ends at 10 and the third starts at 30, but the second, which starts inside the first, ends at 30: there is no gap of 20 between them.
    let objs = [(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(17.5, 0.0, 25.0, 10.0, 0.0)), (3, frame(35.0, 0.0, 10.0, 10.0, 0.0))];
    let t = scene(&objs, &[], &[]);
    let s = wide();
    // The box at 60.3 would repeat a gap of 20 from the third box's end at 40, if there were one.
    let q = query(box_subject(frame(65.3, 0.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(!r.hits.iter().any(|h| h.kind == Kind::Spacing) && !r.marks.iter().any(|m| matches!(m, Mark::Gap { .. })), "{:?} {:?}", r.hits, r.marks);
}

#[test]
fn a_gap_that_is_not_the_smallest_of_the_row_is_repeated_when_it_is_the_nearest() {
    // The gaps are 20, 20 and 25 (the row()). A box that is 24.6 from the last box is nearer the 25 than the 20: it takes the 25.
    let t = scene(&row(), &[], &[]);
    let s = wide();
    let q = query(box_subject(frame(129.6 + 5.0, 0.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!((r.frame.unwrap().centre.x - 5.0 - 130.0).abs() < 1e-9, "{:?}", r.frame);
    assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing), "{:?}", r.hits);
}

mod neighbour_gaps {
    use craft_snap::SpacingGaps;

    use super::*;

    /// Boxes 10 wide with the gaps 20 (first to second) and 40 (second to third).
    fn scene_of_two_gaps() -> craft_snap::Targets {
        scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0)), (3, frame(85.0, 0.0, 10.0, 10.0, 0.0))], &[], &[])
    }

    fn lands(gaps: SpacingGaps, left: f64) -> Option<f64> {
        let s = Settings { spacing_gaps: gaps, ..wide() };
        let t = scene_of_two_gaps();
        let q = query(box_subject(frame(left + 5.0, 0.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        r.hits.iter().any(|h| h.kind == Kind::Spacing).then(|| r.frame.unwrap().centre.x - 5.0)
    }

    #[test]
    fn with_the_gap_of_the_neighbour_only_the_gap_beyond_the_nearest_box_is_repeated() {
        // After the last box (ending at 90): its gap to the box before it is 40, so 130; the gap of 20 elsewhere in the row is a gap of the row only.
        assert_eq!(lands(SpacingGaps::Neighbour, 130.4).map(|x| x.round()), Some(130.0));
        assert_eq!(lands(SpacingGaps::Neighbour, 110.4), None, "20 is not the neighbour's gap");
        assert_eq!(lands(SpacingGaps::Row, 110.4).map(|x| x.round()), Some(110.0), "but it is a gap of the row");
        // Before the first box (starting at 0): its gap to the next is 20, so the box's low side lands 30 below it.
        assert_eq!(lands(SpacingGaps::Neighbour, -29.6).map(|x| x.round()), Some(-30.0));
        assert_eq!(lands(SpacingGaps::Neighbour, -49.6), None, "40 is not the first box's gap");
        assert_eq!(lands(SpacingGaps::Row, -49.6).map(|x| x.round()), Some(-50.0));
    }
}

mod rows_everywhere {
    use craft_snap::{Axis, Constraint, NudgeReach, SpacingRow, Subject};
    use kurbo::Point;

    use super::*;

    /// Only the gaps pull: every other kind is off.
    fn only_gaps() -> Settings {
        let mut s = wide();
        for k in [
            Kind::Guide,
            Kind::Corner,
            Kind::SideMiddle,
            Kind::Size,
            Kind::Angle,
            Kind::Edge,
            Kind::Centre,
            Kind::Vertex,
            Kind::Midpoint,
            Kind::Nearest,
            Kind::Intersection,
        ] {
            let mut rule = s.kinds.rule(k);
            rule.on = false;
            s.kinds.set(k, rule);
        }
        s
    }

    fn settings(row: SpacingRow) -> Settings {
        Settings { spacing_row: row, ..only_gaps() }
    }

    fn spacing_hit(r: &craft_snap::Snapped) -> bool {
        r.hits.iter().any(|h| h.kind == Kind::Spacing)
    }

    #[test]
    fn a_row_left_of_the_origin_is_read_whole_and_so_is_one_after_the_box() {
        let s = wide();
        // Two boxes with a gap of 20, and the box 20 after them: all of them at negative coordinates.
        let t = scene(&[(1, frame(-75.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(-45.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let q = query(box_subject(frame(-15.0 + 0.3, 0.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(spacing_hit(&r) && (r.frame.unwrap().centre.x + 15.0).abs() < 1e-9, "{:?} {:?}", r.hits, r.frame);
        // The box before the row, the gap repeated before its first box.
        let t = scene(&[(1, frame(55.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(85.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let q = query(box_subject(frame(25.0 + 0.3, 0.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(spacing_hit(&r) && (r.frame.unwrap().centre.x - 25.0).abs() < 1e-9, "{:?} {:?}", r.hits, r.frame);
    }

    #[test]
    fn a_shape_that_is_turned_is_a_neighbour_in_a_row_along_the_page() {
        // The first box is turned, so the box round it is 7.5 wide; the gap to the second is 21.25 and the box 21.25 after the second repeats it.
        let turned = frame(5.0, 0.0, 6.0, 6.0, 0.3);
        let b = turned.bounds();
        let gap = 30.0 - b.x1;
        let t = scene(&[(1, turned), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let s = wide();
        let q = query(box_subject(frame(40.0 + gap + 5.0 + 0.3, 0.0, 10.0, 10.0, 0.0)), &s);
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(spacing_hit(&r), "{:?}", r.hits);
        assert!((r.frame.unwrap().centre.x - (40.0 + gap + 5.0)).abs() < 1e-9, "{:?}", r.frame);
    }

    #[test]
    fn a_box_that_touches_the_subject_is_its_neighbour_on_either_side_in_either_kind_of_row() {
        for row in [SpacingRow::Band, SpacingRow::Linked] {
            let s = settings(row);
            // Gaps of 3 at the left and the subject touching the second box: it repeats the gap of 3 and moves 3 on.
            let t = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(18.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
            let q = query(box_subject(frame(28.0, 0.0, 10.0, 10.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!(spacing_hit(&r) && (r.frame.unwrap().centre.x - 31.0).abs() < 1e-9, "{row:?}: {:?} {:?}", r.hits, r.frame);
            // The same on the other side: the subject touching the first box of a row after it.
            let t = scene(&[(1, frame(105.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(118.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
            let q = query(box_subject(frame(95.0, 0.0, 10.0, 10.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            assert!(spacing_hit(&r) && (r.frame.unwrap().centre.x - 92.0).abs() < 1e-9, "{row:?}: {:?} {:?}", r.hits, r.frame);
        }
    }

    #[test]
    fn a_box_that_touches_another_is_not_stepped_over_to_make_a_gap_beyond_it() {
        // 0 to 10, a box touching it from 10 to 30, and one from 50 to 60: the gaps are 0 and 20, and not 40.
        let t =
            scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(20.0, 0.0, 20.0, 10.0, 0.0)), (3, frame(55.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let s = wide();
        let at = |left: f64| {
            let q = query(box_subject(frame(left + 5.0, 0.0, 10.0, 10.0, 0.0)), &s);
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            r
        };
        let forty = at(100.3);
        assert!(!spacing_hit(&forty), "a gap of 40 is not in the row: {:?}", forty.hits);
        let twenty = at(80.3);
        assert!(spacing_hit(&twenty) && (twenty.frame.unwrap().centre.x - 5.0 - 80.0).abs() < 1e-9, "{:?}", twenty.hits);
    }

    #[test]
    fn a_box_moving_along_a_ray_repeats_a_gap_by_the_travel_the_ray_needs() {
        // The row's gap is 20 along x, the ray is 45 degrees up: the box has to travel 2.83 along it to move 2 across. Only the gaps pull.
        let s = only_gaps();
        let t = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let q = craft_snap::Query {
            constraint: Constraint::Ray { origin: Point::new(58.0, 0.0), dir: kurbo::Vec2::new(1.0, 1.0) },
            ..query(box_subject(frame(63.0, 5.0, 10.0, 10.0, 0.0)), &s)
        };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap();
        assert!(spacing_hit(&r), "{:?}", r.hits);
        assert!((r.frame.unwrap().centre.x - 65.0).abs() < 1e-9 && (r.frame.unwrap().centre.y - 7.0).abs() < 1e-9, "{:?}", r.frame);
    }

    #[test]
    fn a_nudge_stops_where_the_box_would_repeat_a_gap_ahead_of_it_and_behind_it() {
        let t = scene(&[(1, frame(5.0, 0.0, 10.0, 10.0, 0.0)), (2, frame(35.0, 0.0, 10.0, 10.0, 0.0))], &[], &[]);
        let s = wide();
        let nudge = |left: f64, forward: bool| {
            let q = query(
                Subject::Nudge {
                    frame: frame(left + 5.0, 0.0, 10.0, 10.0, 0.0),
                    bounds: None,
                    axis: Axis::X,
                    forward,
                    reach: NudgeReach::Anywhere,
                    fallback: 1.0,
                },
                &s,
            );
            let r = run(&t, &q);
            r.check(&t, &q).unwrap();
            r
        };
        // At 45 the next place ahead that repeats the gap of 20 is 60.
        let ahead = nudge(45.0, true);
        assert!(spacing_hit(&ahead) && (ahead.delta.x - 15.0).abs() < 1e-9, "{:?} {:?}", ahead.hits, ahead.delta);
        // At 75 the place behind is 60.
        let behind = nudge(75.0, false);
        assert!(spacing_hit(&behind) && (behind.delta.x + 15.0).abs() < 1e-9, "{:?} {:?}", behind.hits, behind.delta);
        // On the place already, the next press goes to the next one, and with none the fallback.
        let on = nudge(60.0, true);
        assert!(!spacing_hit(&on) && (on.delta.x - 1.0).abs() < 1e-9, "{:?} {:?}", on.hits, on.delta);
    }
}

#[test]
fn a_chained_row_reads_the_boxes_after_the_subject_wherever_the_origin_of_the_targets_falls() {
    // Five boxes whose median is at 18, left of the subject: the origin is there and the subject's high side is a positive distance from it.
    let s = Settings { spacing_row: craft_snap::SpacingRow::Linked, ..wide() };
    let objs = [
        (1, frame(-100.0, 0.0, 10.0, 10.0, 0.0)),
        (2, frame(5.0, 0.0, 10.0, 10.0, 0.0)),
        (3, frame(18.0, 0.0, 10.0, 10.0, 0.0)),
        (4, frame(65.0, 0.0, 10.0, 10.0, 0.0)),
        (5, frame(78.0, 0.0, 10.0, 10.0, 0.0)),
    ];
    let t = scene(&objs, &[], &[]);
    let q = query(box_subject(frame(52.3, 0.0, 10.0, 10.0, 0.0)), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    // The gap of 3 between the last two repeats before them: the box's high side lands 3 before the first of them, at 57.
    assert!(r.hits.iter().any(|h| h.kind == Kind::Spacing) && (r.frame.unwrap().centre.x - 52.0).abs() < 1e-9, "{:?} {:?}", r.hits, r.frame);
}

/// The row of an app that reads equal gaps from a strip (`SpacingRow::Beyond`): the neighbour on each side and the nearest object wholly beyond it.
mod beyond {
    use craft_snap::{Axis, Constraint, Id, Reach, SpacingGaps, SpacingRow, TurnedBox};
    use kurbo::Rect;

    use super::*;

    /// Settings with only equal gaps on, along a row, in inches at 96 to the inch: a gap pulls from 6 screen units, the neighbours of a gap may be four
    /// lengths off (or four inches), and a neighbour's gap to what is beyond it is the only gap repeated.
    fn settings(row: SpacingRow) -> Settings {
        let mut s = Settings {
            spacing_row: row,
            spacing_gaps: SpacingGaps::Neighbour,
            spacing_reach: SpacingReach { times: 4.0, at_least: 4.0 },
            ..Settings::default()
        };
        for k in [
            Kind::Corner,
            Kind::SideMiddle,
            Kind::Edge,
            Kind::Centre,
            Kind::Size,
            Kind::Vertex,
            Kind::Midpoint,
            Kind::Nearest,
            Kind::Intersection,
            Kind::Guide,
            Kind::Grid,
        ] {
            s.kinds.set(k, craft_snap::KindRule { on: false, reach: Reach::Screen(6.0), priority: 1, scope: craft_snap::Scope::All });
        }
        s.kinds.set(Kind::Spacing, craft_snap::KindRule { on: true, reach: Reach::Screen(6.0), priority: 6, scope: craft_snap::Scope::All });
        s
    }

    /// A box from its sides: x0, y0, x1, y1.
    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> TurnedBox {
        frame(0.5 * (x0 + x1), 0.5 * (y0 + y1), x1 - x0, y1 - y0, 0.0)
    }

    /// Where a subject lands along x among these boxes (the first is object 1), and what it hit.
    fn along_x(boxes: &[TurnedBox], subject: TurnedBox, row: SpacingRow) -> craft_snap::Snapped {
        let objs: Vec<(u64, TurnedBox)> = boxes.iter().enumerate().map(|(i, b)| (i as u64 + 1, *b)).collect();
        let t = scene(&objs, &[], &[]);
        let s = settings(row);
        let q = craft_snap::Query { zoom: 96.0, constraint: Constraint::Axis(Axis::X), ..query(box_subject(subject), &s) };
        let r = run(&t, &q);
        r.check(&t, &q).unwrap_or_else(|b| panic!("{b:?}\n{r:?}"));
        r
    }

    fn spacing_hit(r: &craft_snap::Snapped) -> Option<Id> {
        r.hits.iter().find(|h| h.kind == Kind::Spacing).and_then(|h| h.id)
    }

    #[test]
    fn a_staircase_that_misses_the_subjects_strip_is_not_a_row() {
        // The subject x 4.05 to 5.05, y 0 to 1. L (id 1) overlaps it across; B (id 2) overlaps L across and misses the subject's strip. The gap L to B is 1
        // and the subject's to L 1.05.
        let boxes = [bx(2.0, 0.5, 3.0, 1.5), bx(0.0, 1.2, 1.0, 2.2)];
        let subject = bx(4.05, 0.0, 5.05, 1.0);
        let beyond = along_x(&boxes, subject, SpacingRow::Beyond);
        assert_eq!(beyond.delta.x, 0.0, "{:?}", beyond.hits);
        assert_eq!(spacing_hit(&beyond), None);
        assert_eq!(along_x(&boxes, subject, SpacingRow::Band).delta.x, 0.0);
        // Linked follows the staircase out across the strip and finds it.
        let linked = along_x(&boxes, subject, SpacingRow::Linked);
        assert!((linked.delta.x + 0.05).abs() < 1e-9 && spacing_hit(&linked) == Some(Id(1)), "{:?} {:?}", linked.delta, linked.hits);
    }

    #[test]
    fn the_object_wholly_beyond_the_neighbour_is_the_gap_repeated_and_the_next_one_is_not_looked_at() {
        // Low side: the neighbour L (id 1) x 2.5 to 3.5 is 0.5 from the subject (x 4 to 5); Bl (id 2) is wholly before L, 0.55 from it; one more (id 3) is
        // 0.95 before that. The subject repeats L's gap to Bl: it moves 0.05 away.
        let boxes = [bx(2.5, 0.0, 3.5, 1.0), bx(0.95, 0.0, 1.95, 1.0), bx(-1.0, 0.0, 0.0, 1.0)];
        let r = along_x(&boxes, bx(4.0, 0.0, 5.0, 1.0), SpacingRow::Beyond);
        assert!((r.delta.x - 0.05).abs() < 1e-9 && spacing_hit(&r) == Some(Id(1)), "{:?} {:?}", r.delta, r.hits);
        assert!(
            r.marks.iter().any(|m| matches!(m, Mark::Gap { value, subject: true, hit: true, .. } if (value - 0.55).abs() < 1e-9)),
            "{:?}",
            r.marks
        );
        // The same on the high side: the neighbour H (id 1) x 5.5 to 6.5 is 0.5 from the subject; the object wholly after it (id 2) is 0.45 from it.
        let boxes = [bx(5.5, 0.0, 6.5, 1.0), bx(6.95, 0.0, 7.95, 1.0), bx(9.0, 0.0, 10.0, 1.0)];
        let r = along_x(&boxes, bx(4.0, 0.0, 5.0, 1.0), SpacingRow::Beyond);
        assert!((r.delta.x - 0.05).abs() < 1e-9 && spacing_hit(&r) == Some(Id(1)), "{:?} {:?}", r.delta, r.hits);
        // The middle between the two neighbours, as ever: 0.5 from one and 0.4 from the other, the subject goes 0.05 towards the nearer.
        let boxes = [bx(2.5, 0.0, 3.5, 1.0), bx(5.4, 0.0, 6.4, 1.0)];
        let r = along_x(&boxes, bx(4.0, 0.0, 5.0, 1.0), SpacingRow::Beyond);
        assert!((r.delta.x + 0.05).abs() < 1e-9, "{:?} {:?}", r.delta, r.hits);
    }

    #[test]
    fn an_object_that_overlaps_the_neighbour_along_the_row_is_not_beyond_it_and_is_not_stepped_onto() {
        // Low side, from a real case: the subject x 5.159 to 6.911, y 0 to 1; #3 (id 1) is its neighbour; #4 (id 2) overlaps #3 along the row and #3's band;
        // #5 (id 3) overlaps #4's band but not #3's. Nothing is wholly beyond #3 that overlaps it across: no gap to repeat.
        let boxes = [bx(3.277, 0.082, 4.726, 0.811), bx(2.181, 0.536, 3.524, 1.027), bx(2.488, 0.875, 2.836, 1.709)];
        let subject = bx(5.159, 0.0, 6.911, 1.0);
        let beyond = along_x(&boxes, subject, SpacingRow::Beyond);
        assert_eq!((beyond.delta.x, spacing_hit(&beyond)), (0.0, None), "{:?}", beyond.hits);
        // Linked steps onto #4 and joins #5 to #3 across it: a hit of 0.0082 that Beyond does not make.
        let linked = along_x(&boxes, subject, SpacingRow::Linked);
        assert!(spacing_hit(&linked).is_some() && linked.delta.x != 0.0, "{:?} {:?}", linked.delta, linked.hits);
        // The other way: the neighbour H (id 1, x 9.5 to 10.5, 0.5 from the subject) has a box wholly beyond it (id 3, x 10.95 to 11.95) that overlaps its
        // band, and one that overlaps it along the row (id 2, x 9.8 to 10.1) and sits above the beyond box's band. Linked steps onto the second and loses
        // the third; Beyond skips the second and finds the third: the gap 0.45 against the subject's 0.5, a move of 0.05.
        let boxes = [bx(9.5, 0.0, 10.5, 1.0), bx(9.8, 0.8, 10.1, 1.6), bx(10.95, 0.0, 11.95, 0.4)];
        let subject = bx(8.0, 0.0, 9.0, 1.0);
        let beyond = along_x(&boxes, subject, SpacingRow::Beyond);
        assert!((beyond.delta.x - 0.05).abs() < 1e-9 && spacing_hit(&beyond) == Some(Id(1)), "{:?} {:?}", beyond.delta, beyond.hits);
        let linked = along_x(&boxes, subject, SpacingRow::Linked);
        assert_eq!(spacing_hit(&linked), None, "{:?}", linked.hits);
    }

    #[test]
    fn an_object_touching_the_neighbour_is_no_gap_to_repeat() {
        // A gap of nothing is not a gap (it is not drawn, and a hit with no mark is not made): the neighbour's edge is still met by the alignment of sides.
        let boxes = [bx(2.5, 0.0, 3.5, 1.0), bx(1.5, 0.0, 2.5, 1.0)];
        let r = along_x(&boxes, bx(4.0, 0.0, 5.0, 1.0), SpacingRow::Beyond);
        assert_eq!((r.delta.x, spacing_hit(&r)), (0.0, None), "{:?}", r.hits);
    }

    #[test]
    fn a_gap_that_fails_where_the_subject_lands_leaves_the_neighbours_other_gaps_to_try() {
        // From the property below, which failed on it once in a while. The middle between the neighbours (0.0479 away) is a place where another box
        // becomes the neighbour, so it is dropped when it is read again there; the gap of the high neighbour to the box beyond it (0.0498 the other way)
        // holds, and is what the rule written out asks for. The crate used to keep only the nearest candidate of the direction and so had nothing to try.
        let r4 = |x: f64, y: f64, w: f64, h: f64| Rect::new(x, y, x + w, y + h);
        let boxes = [
            r4(9.133484291898712, 0.7741148520080406, 0.9707174529557605, 0.20000000000000007),
            r4(5.160760281496745, 0.0, 1.6290643702856897, 0.9141254572049335),
            r4(11.164832549934763, 0.0, 0.3000000000000007, 1.1501505008391264),
            r4(5.184633889740058, 0.0, 0.6893326820905292, 1.564771442712707),
        ];
        let subject = r4(6.789047921389958, 0.0, 1.3336490751376804, 1.1690145435144503);
        let want = expected(&boxes, subject, 96.0);
        assert!((want + 0.0498).abs() < 1e-4, "the rule asks for {want}");
        let frames: Vec<TurnedBox> = boxes.iter().map(|r| bx(r.x0, r.y0, r.x1, r.y1)).collect();
        for row in [SpacingRow::Beyond, SpacingRow::Band, SpacingRow::Linked] {
            let r = along_x(&frames, bx(subject.x0, subject.y0, subject.x1, subject.y1), row);
            if row == SpacingRow::Beyond {
                assert!((r.delta.x - want).abs() < 1e-9, "want {want}, got {:?} {:?}", r.delta, r.hits);
                assert_eq!(spacing_hit(&r), Some(Id(1)), "{:?}", r.hits);
                // The middle place is tried and dropped, and then the gap beyond: each candidate once, so two combinations are solved.
                assert_eq!(r.work.solves, 2, "{:?}", r.work);
            }
        }
    }

    /// The moves of the subject, along x, that the rule asks for (the middle between the neighbours, and each neighbour's gap repeated to the object
    /// wholly beyond it), written out over the boxes that meet the strip as wide across as the subject and four lengths (or four inches) along.
    fn asked(boxes: &[Rect], subject: Rect) -> Vec<f64> {
        let overlaps = |a: (f64, f64), b: (f64, f64)| a.0 < b.1 && b.0 < a.1;
        let reach = (4.0 * subject.width()).max(4.0);
        let window = Rect::new(subject.x0 - reach, subject.y0, subject.x1 + reach, subject.y1);
        let meets = |r: &Rect| r.x0 <= window.x1 && window.x0 <= r.x1 && r.y0 <= window.y1 && window.y0 <= r.y1;
        let gathered: Vec<(usize, Rect)> = boxes.iter().copied().enumerate().filter(|(_, r)| meets(r)).collect();
        let across = (subject.y0, subject.y1);
        let beside: Vec<&(usize, Rect)> = gathered.iter().filter(|(_, r)| overlaps((r.y0, r.y1), across)).collect();
        let low = beside.iter().filter(|(_, r)| r.x1 <= subject.x0).max_by(|a, b| a.1.x1.total_cmp(&b.1.x1));
        let high = beside.iter().filter(|(_, r)| r.x0 >= subject.x1).min_by(|a, b| a.1.x0.total_cmp(&b.1.x0));
        let mut moves: Vec<f64> = Vec::new();
        if let (Some((_, l)), Some((_, h))) = (low, high) {
            moves.push(((h.x0 - subject.x1) - (subject.x0 - l.x1)) / 2.0);
        }
        if let Some((i, l)) = low {
            let gl = subject.x0 - l.x1;
            let b = gathered
                .iter()
                .filter(|(j, r)| j != i && overlaps((r.y0, r.y1), (l.y0, l.y1)) && r.x1 <= l.x0)
                .max_by(|a, b| a.1.x1.total_cmp(&b.1.x1));
            if let Some((_, b)) = b {
                moves.push((l.x0 - b.x1) - gl);
            }
        }
        if let Some((i, h)) = high {
            let gh = h.x0 - subject.x1;
            let b = gathered
                .iter()
                .filter(|(j, r)| j != i && overlaps((r.y0, r.y1), (h.y0, h.y1)) && r.x0 >= h.x1)
                .min_by(|a, b| a.1.x0.total_cmp(&b.1.x0));
            if let Some((_, b)) = b {
                moves.push(gh - (b.x0 - h.x1));
            }
        }
        moves
    }

    /// Where the rule lands the subject: the nearest of the moves it asks within the pull, that still asks for itself where it lands (the neighbours
    /// are read again there, as the crate does for every snap, so that what it draws is true).
    fn expected(boxes: &[Rect], subject: Rect, zoom: f64) -> f64 {
        let pull = 6.0 / zoom;
        let mut moves: Vec<f64> = asked(boxes, subject).into_iter().filter(|m| m.abs() <= pull).collect();
        moves.sort_by(|a, b| a.abs().total_cmp(&b.abs()));
        let moved = |m: f64| Rect::new(subject.x0 + m, subject.y0, subject.x1 + m, subject.y1);
        moves.into_iter().find(|m| asked(boxes, moved(*m)).iter().any(|k| k.abs() < 1e-9)).unwrap_or(0.0)
    }

    use proptest::prelude::*;

    fn rect() -> impl Strategy<Value = Rect> {
        (0.0f64..14.0, -0.5f64..2.0, 0.3f64..2.0, 0.2f64..1.6).prop_map(|(x, y, w, h)| Rect::new(x, y, x + w, y + h))
    }

    /// A fixed seed, so that a run is the same run each time; `CRAFT_SNAP_CASES` and `CRAFT_SNAP_SEED` run a longer search, and each find is a case to
    /// read (and to pin as a named test).
    fn config() -> ProptestConfig {
        let number = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(default);
        ProptestConfig {
            cases: u32::try_from(number("CRAFT_SNAP_CASES", 4000)).unwrap_or(4000),
            rng_seed: proptest::test_runner::RngSeed::Fixed(number("CRAFT_SNAP_SEED", 0xbe_704d)),
            ..ProptestConfig::default()
        }
    }

    proptest! {
        #![proptest_config(config())]

        /// The row of this mode lands the subject where the rule written out above does, over boxes of every kind of overlap across and along.
        #[test]
        fn the_beyond_row_lands_where_the_rule_written_out_does(
            boxes in prop::collection::vec(rect(), 0..8),
            subject in (6.0f64..9.0, 0.0f64..0.5, 0.5f64..2.0, 0.5f64..1.5).prop_map(|(x, y, w, h)| Rect::new(x, y, x + w, y + h)),
            nudge in prop::sample::select(vec![0.0f64, 0.02, -0.02, 0.05]),
        ) {
            // Where a pull is within reach of the subject some of the time: the subject is also put a little off the middle of its neighbours.
            let subject = Rect::new(subject.x0 + nudge, subject.y0, subject.x1 + nudge, subject.y1);
            let frames: Vec<TurnedBox> = boxes.iter().map(|r| bx(r.x0, r.y0, r.x1, r.y1)).collect();
            let r = along_x(&frames, bx(subject.x0, subject.y0, subject.x1, subject.y1), SpacingRow::Beyond);
            let want = expected(&boxes, subject, 96.0);
            prop_assert!((r.delta.x - want).abs() < 1e-9, "want {want}, got {:?} {:?}", r.delta, r.hits);
        }
    }
}
