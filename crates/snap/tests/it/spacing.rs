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
    let chains: std::collections::BTreeSet<u16> = gaps.iter().filter_map(|m| if let Mark::Gap { chain, .. } = m { Some(*chain) } else { None }).collect();
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
    let objects: Vec<(u64, craft_snap::TurnedBox)> = [0.0, 30.0, 60.0].iter().enumerate().map(|(i, d)| (i as u64, frame(at(*d).0, at(*d).1, 10.0, 10.0, a))).collect();
    let t = scene(&objects, &[], &[]);
    let s = wide();
    let p = at(90.0);
    let f = frame(p.0, p.1, 10.0, 10.0, a);
    let q = query(box_subject(f), &s);
    let r = run(&t, &q);
    r.check(&t, &q).unwrap();
    assert!(r.marks.iter().any(|m| matches!(m, Mark::Gap { direction, .. } if (direction.angle() - a).abs() < 1e-9)), "{:?}", r.marks);
}
