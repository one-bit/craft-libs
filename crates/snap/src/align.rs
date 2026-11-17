//! Lines: guides, frames, layout lines, other objects' edges and centres, and lattices, in every direction a subject has a side in.
//!
//! Every side and kind gives at most one candidate in a direction: the nearest line of that kind, found by bisection in a sorted array. With
//! alignment anywhere (`align_reach: None`) nothing is gathered: the sorted arrays hold every object's lines, so how many objects there are
//! and how far away they lie change only the logarithm.

use std::f64::consts::{FRAC_PI_2, PI};

use kurbo::Vec2;

use crate::budget::Cuts;
use crate::ctx::{Cand, Ctx, Probe, grown};
use crate::frame::{Id, Side};
use crate::index::{DirTable, canon, log_steps};
use crate::kind::{Kind, Reach};
use crate::settings::Matching;
use crate::targets::{side_kind, side_lines};

/// One line found near a side.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Found {
    pub kind: Kind,
    pub id: Option<Id>,
    pub c: f64,
    pub count: u32,
}

/// How the subject travels for a line: along the line's normal (its two degrees free), or along a direction (a constraint left one).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Free {
    Normal,
    Along(Vec2),
}

/// Whether the side `side` may meet a line of kind `kind` under the matching rule.
fn matches(cx: &Ctx<'_>, kind: Kind, side: Side) -> bool {
    match cx.s.matching {
        Matching::AnyToAny => true,
        Matching::LikeToLike => !kind.has_side() || ((kind == Kind::Edge) == (side != Side::Mid)),
    }
}

/// The nearest line of each kind in one table, to the offset `s` in the direction `angle`.
fn from_table(cx: &mut Ctx<'_>, table: &DirTable, angle: f64, s: f64, side: Side, out: &mut Vec<Found>) {
    cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(table.dirs.len()));
    let Some(dir) = table.find(angle, &cx.co) else { return };
    for set in &dir.sets {
        if !cx.rule(set.kind).on || !matches(cx, set.kind, side) {
            continue;
        }
        cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(set.entries.len()));
        if let Some(e) = set.nearest(s).and_then(|i| set.entries.get(i)) {
            out.push(Found { kind: set.kind, id: e.id, c: e.offset, count: e.count });
        }
    }
}

/// The nearest line of each kind of the lattices, when the direction is a page axis.
fn from_lattices(cx: &Ctx<'_>, angle: f64, s: f64, side: Side, out: &mut Vec<Found>) {
    let axis_x = angle.abs() <= cx.co.angle;
    let axis_y = (angle - FRAC_PI_2).abs() <= cx.co.angle;
    if !(axis_x || axis_y) {
        return;
    }
    for lat in cx.q.lattices {
        let (origin, pitch, o) = if axis_x { (lat.origin.x, lat.pitch.x, cx.o.x) } else { (lat.origin.y, lat.pitch.y, cx.o.y) };
        if !(pitch.is_finite() && pitch > 0.0 && origin.is_finite() && cx.rule(lat.kind).on && matches(cx, lat.kind, side)) {
            continue;
        }
        let n = ((o + s - origin) / pitch).round();
        if n.is_finite() && n.abs() <= 4_503_599_627_370_496.0 {
            out.push(Found { kind: lat.kind, id: None, c: origin + n * pitch - o, count: 1 });
        }
    }
}

/// The objects within the reach across of the subject, nearest first, at most `candidates`: made on the first use.
fn near_objects(cx: &mut Ctx<'_>, reach: f64) -> Vec<u32> {
    if let Some(n) = &cx.near {
        return n.clone();
    }
    let t = cx.t;
    let window = grown(cx.region, reach);
    let (limit, visits) = (u64::from(cx.budget().candidates), u64::from(cx.budget().window_visits));
    let g = t.object_grid(&mut cx.work.made).gather(&window, limit as usize, visits);
    cx.work.examined = cx.work.examined.saturating_add(g.examined);
    cx.work.visited = cx.work.visited.saturating_add(g.visited);
    cx.work.candidates = cx.work.candidates.saturating_add(g.items.len() as u64);
    if g.cut {
        cx.work.cut(if g.visited >= visits { Cuts::WINDOW_VISITS } else { Cuts::CANDIDATES });
    }
    let list: Vec<u32> = g.items.iter().map(|c| c.index).collect();
    cx.near = Some(list.clone());
    list
}

/// The nearest line of each object kind among the objects near the subject, tested directly.
fn from_near_objects(cx: &mut Ctx<'_>, reach: f64, angle: f64, s: f64, side: Side, out: &mut Vec<Found>) {
    let near = near_objects(cx, reach);
    let (t, co, o) = (cx.t, cx.co, cx.o);
    let mut best: Vec<Found> = Vec::new();
    for o_idx in near {
        let Some(obj) = t.objects.get(o_idx as usize) else { continue };
        if !(obj.parts.bounds_lines || obj.parts.own_lines) {
            continue;
        }
        for l in side_lines(&obj.frame, o, &co, obj.parts.own_lines) {
            let (a, flipped) = canon(l.angle, co.angle);
            let x = (a - angle).rem_euclid(PI);
            let kind = side_kind(l.side);
            if x.min(PI - x) > co.angle || !cx.rule(kind).on || !matches(cx, kind, side) {
                continue;
            }
            let found = Found { kind, id: Some(obj.id), c: if flipped { -l.offset } else { l.offset }, count: 1 };
            match best.iter_mut().find(|f| f.kind == kind) {
                Some(f) if (found.c - s).abs() < (f.c - s).abs() || ((found.c - s).abs() == (f.c - s).abs() && found.id < f.id) => *f = found,
                Some(_) => {}
                None => best.push(found),
            }
        }
    }
    out.extend(best);
}

/// The nearest line of each kind to the offset `s` of a side of the subject in the direction `angle`: lattices, the app's lines and the
/// objects' lines.
pub(crate) fn lines_near(cx: &mut Ctx<'_>, angle: f64, s: f64, side: Side) -> Vec<Found> {
    let mut out = Vec::new();
    from_lattices(cx, angle, s, side, &mut out);
    let t = cx.t;
    from_table(cx, &t.app, angle, s, side, &mut out);
    match cx.s.align_reach {
        Some(r) if !matches!(r, Reach::Any) => {
            let reach = r.document(cx.zoom);
            from_near_objects(cx, reach, angle, s, side, &mut out);
        }
        _ => {
            let table = t.object_lines(&mut cx.work.made);
            from_table(cx, table, angle, s, side, &mut out);
        }
    }
    out
}

/// The candidates of every side of one probe.
pub(crate) fn probe_cands(cx: &mut Ctx<'_>, probe: &Probe, free: Free) -> Vec<Cand> {
    let mut out = Vec::new();
    for ps in &probe.sides {
        for f in lines_near(cx, probe.angle, ps.offset, ps.side) {
            let d = f.c - ps.offset;
            let travel = match free {
                Free::Normal => Some(d),
                Free::Along(e) => {
                    let ne = probe.normal.dot(e);
                    (ne.abs() > 1.0e-12).then(|| d / ne)
                }
            };
            let Some(travel) = travel.filter(|t| t.is_finite()) else { continue };
            if !cx.pulls(f.kind, travel.abs()) {
                continue;
            }
            out.push(Cand {
                kind: f.kind,
                id: f.id,
                prio: cx.rule(f.kind).priority,
                reach: cx.reach(f.kind),
                side: ps.side,
                own: ps.own,
                angle: probe.angle,
                c: f.c,
                d: travel,
                t: travel.abs(),
                count: f.count,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use kurbo::{Point, Size};

    use super::*;
    use crate::budget::Budget;
    use crate::frame::{Axis, TurnedBox};
    use crate::query::{Constraint, Query, Rounding, Subject};
    use crate::settings::Settings;
    use crate::targets::{Limits, Object, Parts, TargetsBuilder};

    fn query<'a>(s: &'a Settings) -> Query<'a> {
        Query { subject: Subject::Point { at: Point::ORIGIN, base: None }, zoom: 1.0, settings: s, constraint: Constraint::None, rounding: Rounding::None, lattices: &[], budget: Budget::default() }
    }

    fn built(objects: &[(u64, TurnedBox)], guides: &[f64]) -> crate::targets::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for (id, f) in objects {
            b.object(Id(*id), Object { frame: *f, angle: None, parts: Parts::ALL }).unwrap();
        }
        for g in guides {
            b.line(None, Axis::X, *g, None, Kind::Guide).unwrap();
        }
        b.build()
    }

    #[test]
    fn the_nearest_line_of_each_kind_is_found_wherever_the_object_is() {
        let t = built(&[(1, TurnedBox::square(Point::new(100.0, 50.0), Size::new(10.0, 4.0))), (2, TurnedBox::square(Point::new(-300.0, 0.0), Size::new(2.0, 2.0)))], &[7.0]);
        let s = Settings::default();
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        let at = 96.0 - cx.o.x;
        let mut found = lines_near(&mut cx, 0.0, at, Side::Min);
        found.sort_by(|a, b| a.kind.cmp(&b.kind));
        let kinds: Vec<(Kind, f64)> = found.iter().map(|f| (f.kind, f.c + cx.o.x)).collect();
        assert!(kinds.contains(&(Kind::Guide, 7.0)));
        assert!(kinds.contains(&(Kind::Edge, 95.0)), "{kinds:?}");
        assert!(kinds.contains(&(Kind::Centre, 100.0)), "{kinds:?}");
        assert!(cx.work.bisect_steps > 0 && cx.work.visited == 0, "alignment anywhere gathers nothing");
    }

    #[test]
    fn like_to_like_keeps_edges_with_edges_and_centres_with_centres() {
        let t = built(&[(1, TurnedBox::square(Point::new(10.0, 0.0), Size::new(4.0, 4.0)))], &[11.0]);
        let s = Settings { matching: Matching::LikeToLike, ..Settings::default() };
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        let kinds = |cx: &mut Ctx<'_>, side| { let at = 10.5 - cx.o.x; lines_near(cx, 0.0, at, side) }.iter().map(|f| f.kind).collect::<Vec<_>>();
        let mid = kinds(&mut cx, Side::Mid);
        assert!(mid.contains(&Kind::Centre) && !mid.contains(&Kind::Edge) && mid.contains(&Kind::Guide), "{mid:?}");
        let min = kinds(&mut cx, Side::Min);
        assert!(min.contains(&Kind::Edge) && !min.contains(&Kind::Centre) && min.contains(&Kind::Guide), "{min:?}");
    }

    #[test]
    fn a_reach_across_gathers_the_nearest_objects_and_alignment_anywhere_does_not() {
        let objects: Vec<(u64, TurnedBox)> = (0..40).map(|i| (i, TurnedBox::square(Point::new(i as f64 * 10.0, 0.0), Size::new(2.0, 2.0)))).collect();
        let t = built(&objects, &[]);
        let s = Settings { align_reach: Some(Reach::Document(25.0)), ..Settings::default() };
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        cx.region = kurbo::Rect::new(100.0 - t.origin.x, -1.0, 102.0 - t.origin.x, 1.0);
        let found = lines_near(&mut cx, 0.0, 100.0 - t.origin.x, Side::Mid);
        let centre = found.iter().find(|f| f.kind == Kind::Centre).unwrap();
        assert_eq!(centre.id, Some(Id(10)));
        assert!(cx.work.candidates >= 1 && cx.work.candidates <= 5, "only the objects within 25: {}", cx.work.candidates);
        assert!(!cx.work.made.has(crate::budget::Made::OBJECT_LINES), "the table of the objects' lines is not made");
        let anywhere = Settings::default();
        let q2 = query(&anywhere);
        let mut cx2 = Ctx::new(&t, &q2);
        let at = -0.1 - cx2.o.x;
        let far = lines_near(&mut cx2, 0.0, at, Side::Mid);
        assert!(far.iter().any(|f| f.kind == Kind::Centre && f.id == Some(Id(0))));
    }

    #[test]
    fn a_lattice_has_a_line_at_every_pitch_and_refuses_a_far_or_bad_one() {
        let t = built(&[], &[]);
        let s = Settings::default();
        let lat = [crate::query::Lattice { origin: Point::new(1.0, 0.0), pitch: Vec2::new(10.0, f64::NAN), kind: Kind::Grid }];
        let q = Query { lattices: &lat, ..query(&s) };
        let cx = Ctx::new(&t, &q);
        let mut out = Vec::new();
        from_lattices(&cx, 0.0, 24.0, Side::Min, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].c, 21.0);
        out.clear();
        from_lattices(&cx, FRAC_PI_2, 24.0, Side::Min, &mut out);
        assert!(out.is_empty(), "a pitch that is not a number has no lines");
        from_lattices(&cx, 0.5, 24.0, Side::Min, &mut out);
        assert!(out.is_empty(), "a lattice is on the page axes");
        let huge = [crate::query::Lattice { origin: Point::ORIGIN, pitch: Vec2::new(1.0e-300, 1.0), kind: Kind::Grid }];
        let q = Query { lattices: &huge, ..query(&s) };
        let cx = Ctx::new(&t, &q);
        from_lattices(&cx, 0.0, 24.0, Side::Min, &mut out);
        assert!(out.is_empty(), "a line number past 2^52 gives no line");
    }
}
