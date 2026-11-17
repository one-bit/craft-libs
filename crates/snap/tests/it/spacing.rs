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
