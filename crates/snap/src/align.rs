//! Lines: guides, frames, layout lines, other objects' edges and centres, and lattices, in every direction a subject has a side in.
//!
//! Every side and kind gives at most one candidate in a direction: the nearest line of that kind, found by bisection in a sorted array. With
//! alignment anywhere (`align_reach: None`) nothing is gathered: the sorted arrays hold every object's lines, so how many objects there are
//! and how far away they lie change only the logarithm.

use std::f64::consts::{FRAC_PI_2, PI};

use kurbo::Vec2;

use crate::budget::{Cuts, Work};
use crate::ctx::{Cand, Ctx, Probe, grown};
use crate::frame::{Id, Side};
use crate::index::{DirTable, Entry, canon, log_steps};
use crate::kind::{Kind, Reach};
use crate::query::Lattice;
use crate::settings::{LineSpan, Matching};
use crate::targets::{side_kind, side_lines};

/// One line found near a side.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Found {
    pub kind: Kind,
    pub id: Option<Id>,
    pub c: f64,
    pub count: u32,
    /// The extent along the line (empty, `lo > hi`, for a line that runs on for ever).
    pub lo: f64,
    pub hi: f64,
    /// Under `LineSpan::EachSide`, the span of the nearest target on the other side of the subject, when the entry has one apart from `lo` to `hi`.
    pub also: Option<(f64, f64)>,
}

/// How the subject travels for a line: along the line's normal (its two degrees free), or along a direction (a constraint left one).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Free {
    Normal,
    Along(Vec2),
}

/// Whether the side `side` may meet a line of kind `kind` that is the side `target` of its box (or none) under the matching rule.
fn matches(cx: &Ctx<'_>, kind: Kind, side: Side, target: Option<Side>) -> bool {
    match cx.s.matching {
        Matching::AnyToAny => true,
        Matching::LikeToLike => !kind.has_side() || ((kind == Kind::Edge) == (side != Side::Mid)),
        Matching::SameSide => !kind.has_side() || target.is_none_or(|t| t == side),
    }
}

/// The entries of a sorted set that are at `s`, charged to `line_steps`, walking out from its insertion point.
fn entries_on<'e>(cx: &mut Ctx<'_>, entries: &'e [Entry], s: f64) -> Vec<&'e Entry> {
    let i = entries.partition_point(|e| e.offset < s);
    let cap = u64::from(cx.budget().line_steps);
    let mut out = Vec::new();
    let (below, above) = entries.split_at(i.min(entries.len()));
    for e in below.iter().rev().take_while(|e| cx.co.same(e.offset, s)).chain(above.iter().take_while(|e| cx.co.same(e.offset, s))) {
        if !Work::charge(&mut cx.work.line_steps, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::LINE_STEPS) {
            break;
        }
        out.push(e);
    }
    out
}

/// The lines of each kind in one table: the nearest to the offset `s` in the direction `angle`, or, with `on`, all those at `s`.
fn from_table(cx: &mut Ctx<'_>, table: &DirTable, angle: f64, s: f64, side: Side, on: Option<(f64, f64)>, out: &mut Vec<Found>) {
    cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(table.dirs.len()));
    let Some(dir) = table.find(angle, &cx.co) else { return };
    for set in &dir.sets {
        if !cx.rule(set.kind).on || !matches(cx, set.kind, side, set.side) {
            continue;
        }
        cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(set.entries.len()));
        let span = cx.s.line_span;
        let found = |e: &Entry, ext: Option<(f64, f64)>| {
            let (lo, hi, also) = match ext {
                Some(x) if span == LineSpan::Nearest => {
                    let (lo, hi) = set.nearest_span(e, x);
                    (lo, hi, None)
                }
                Some(x) if span == LineSpan::EachSide => match set.each_side_spans(e, x) {
                    (Some((lo, hi)), also) => (lo, hi, also),
                    (None, Some((lo, hi))) => (lo, hi, None),
                    (None, None) => (e.lo, e.hi, None),
                },
                _ => (e.lo, e.hi, None),
            };
            Found { kind: set.kind, id: e.id, c: e.offset, count: e.count, lo, hi, also }
        };
        if let Some(ext) = on {
            let list = entries_on(cx, &set.entries, s);
            if span == LineSpan::EachSide {
                // Each entry of members costs five bisections: where the members that start before the subject end, the first of them that
                // touches it, where those that start no later than it ends, the first of them that reaches past it, and the end of the first
                // group that starts after it.
                let steps = list.iter().map(|e| 5 * log_steps(e.mn as usize)).fold(0u64, u64::saturating_add);
                cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(steps);
            }
            out.extend(list.into_iter().map(|e| found(e, Some(ext))));
        } else {
            out.extend(set.nearest(s).and_then(|i| set.entries.get(i)).map(|e| found(e, None)));
        }
    }
}

/// The nearest line of each kind of the lattices (or, with `on`, the one at `s`), when the direction is a page axis.
fn from_lattices(cx: &Ctx<'_>, angle: f64, s: f64, side: Side, on: bool, out: &mut Vec<Found>) {
    let axis_x = angle.abs() <= cx.co.angle;
    let axis_y = (angle - FRAC_PI_2).abs() <= cx.co.angle;
    if !(axis_x || axis_y) {
        return;
    }
    for lat in cx.q.lattices {
        let (origin, pitch, o) = if axis_x { (lat.origin.x, lat.pitch.x, cx.o.x) } else { (lat.origin.y, lat.pitch.y, cx.o.y) };
        if !(pitch.is_finite() && pitch > 0.0 && origin.is_finite() && cx.rule(lat.kind).on && matches(cx, lat.kind, side, None)) {
            continue;
        }
        let n = ((o + s - origin) / pitch).round();
        if !(n.is_finite() && n.abs() <= 4_503_599_627_370_496.0) {
            continue;
        }
        let c = origin + n * pitch - o;
        if !on || cx.co.same(c, s) {
            out.push(Found { kind: lat.kind, id: None, c, count: 1, lo: f64::INFINITY, hi: f64::NEG_INFINITY, also: None });
        }
    }
}

/// The objects within the reach across of the subject, nearest first, at most `candidates`: made on the first use.
fn near_objects(cx: &mut Ctx<'_>, reach: f64) -> Vec<u32> {
    if let Some(n) = &cx.near {
        return n.clone();
    }
    if !cx.work.gather() {
        cx.near = Some(Vec::new());
        return Vec::new();
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

/// Which lines of the objects near the subject are wanted.
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    /// The nearest of each kind to the offset.
    Nearest,
    /// Every line at the offset.
    On,
    /// The nearest of each kind strictly past the offset (the same-place rule apart), above it when `true`, else below.
    Past(bool),
}

/// The lines of the objects near the subject, tested directly: the nearest of each kind, all those at `s`, or the nearest strictly past it.
fn from_near_objects(cx: &mut Ctx<'_>, reach: f64, angle: f64, s: f64, side: Side, pick: Pick, out: &mut Vec<Found>) {
    let near = near_objects(cx, reach);
    let (t, co, o) = (cx.t, cx.co, cx.o);
    let mut best: Vec<Found> = Vec::new();
    for o_idx in near {
        let Some(obj) = t.objects.get(o_idx as usize) else { continue };
        if !(obj.parts.bounds_lines || obj.parts.own_lines) {
            continue;
        }
        for l in side_lines(&obj.frame, obj.bounds, o, &co, obj.parts.own_lines) {
            let (a, flipped) = canon(l.angle, co.angle);
            let x = (a - angle).rem_euclid(PI);
            let kind = side_kind(l.side);
            if x.min(PI - x) > co.angle || !cx.rule(kind).on || !matches(cx, kind, side, Some(crate::frame::canon_side(l.side, flipped))) {
                continue;
            }
            let (c, lo, hi) = if flipped { (-l.offset, -l.hi, -l.lo) } else { (l.offset, l.lo, l.hi) };
            let found = Found { kind, id: Some(obj.id), c, count: 1, lo, hi, also: None };
            match pick {
                Pick::On => {
                    if co.same(c, s) {
                        best.push(found);
                    }
                    continue;
                }
                Pick::Past(up) if !((if up { c > s } else { c < s }) && !co.same(c, s)) => continue,
                Pick::Past(_) | Pick::Nearest => {}
            }
            match best.iter_mut().find(|f| f.kind == kind) {
                Some(f) if (c - s).abs() < (f.c - s).abs() || ((c - s).abs() == (f.c - s).abs() && found.id < f.id) => *f = found,
                Some(_) => {}
                None => best.push(found),
            }
        }
    }
    out.extend(best);
}

/// The nearest line of each kind to the offset `s` of a side of the subject in the direction `angle` (with `on`: every line at `s`):
/// lattices, the app's lines and the objects' lines.
pub(crate) fn lines_find(cx: &mut Ctx<'_>, angle: f64, s: f64, side: Side, on: Option<(f64, f64)>) -> Vec<Found> {
    let mut out = Vec::new();
    from_lattices(cx, angle, s, side, on.is_some(), &mut out);
    let t = cx.t;
    from_table(cx, &t.app, angle, s, side, on, &mut out);
    match cx.s.align_reach {
        Some(r) if !matches!(r, Reach::Any) => {
            let reach = r.document(cx.zoom);
            from_near_objects(cx, reach, angle, s, side, if on.is_some() { Pick::On } else { Pick::Nearest }, &mut out);
        }
        _ => {
            let table = t.object_lines(&mut cx.work.made);
            from_table(cx, table, angle, s, side, on, &mut out);
        }
    }
    out
}

/// The entry of a sorted set that is the first strictly ahead of `s` (not the place `s` is on): above it when `up`, else below.
fn ahead_in<'e>(cx: &Ctx<'_>, entries: &'e [Entry], s: f64, up: bool) -> Option<&'e Entry> {
    if up {
        let i = entries.partition_point(|e| e.offset < s || cx.co.same(e.offset, s));
        entries.get(i)
    } else {
        let i = entries.partition_point(|e| e.offset < s && !cx.co.same(e.offset, s));
        entries.get(i.checked_sub(1)?)
    }
}

/// How many lines of a lattice are looked at to find the first that is ahead: the first, the next, and the one after. A line is only as exact as its
/// number, so far from the origin the next may be nearer than the same-place rule; the one after it is two pitches from the first, and a pitch is more
/// than the rule.
const LATTICE_STEPS: usize = 3;

/// The first line of a lattice on a page axis that is strictly ahead of the offset `s` of a side, past the same-place rule: above it when `up`, else
/// below. Lines no farther apart than the rule are one place with the side or with each other, so none of them is ahead, and a lattice made of them
/// has no stop. A line is only as exact as its number: the lines just coarser than the rule may be nearer than it once their positions are rounded,
/// and they are stepped past until one is not.
fn lattice_ahead(cx: &Ctx<'_>, lat: &Lattice, axis_x: bool, s: f64, up: bool) -> Option<f64> {
    let (origin, pitch, o) = if axis_x { (lat.origin.x, lat.pitch.x, cx.o.x) } else { (lat.origin.y, lat.pitch.y, cx.o.y) };
    if !(pitch.is_finite() && pitch > cx.co.tol(s, s) && origin.is_finite()) {
        return None;
    }
    let n = (o + s - origin) / pitch;
    let line = |k: f64| origin + k * pitch - o;
    let (mut k, step) = if up { (n.floor() + 1.0, 1.0) } else { (n.ceil() - 1.0, -1.0) };
    for _ in 0..LATTICE_STEPS {
        if !(k.is_finite() && k.abs() <= 4_503_599_627_370_496.0) {
            return None;
        }
        let c = line(k);
        if c.is_finite() && (if up { c > s } else { c < s }) && !cx.co.same(c, s) {
            return Some(c);
        }
        k += step;
    }
    None
}

/// The nearest line of each kind strictly ahead of the offset `s` of a side, above it when `up`, else below: for a nudge.
pub(crate) fn lines_ahead(cx: &mut Ctx<'_>, angle: f64, s: f64, side: Side, up: bool, reach: f64) -> Vec<Found> {
    let mut out = Vec::new();
    // Lattices: the next line past the one the side is on.
    for lat in cx.q.lattices {
        let (axis_x, axis_y) = (angle.abs() <= cx.co.angle, (angle - FRAC_PI_2).abs() <= cx.co.angle);
        if !(axis_x || axis_y) || !cx.rule(lat.kind).on || !matches(cx, lat.kind, side, None) {
            continue;
        }
        if let Some(c) = lattice_ahead(cx, lat, axis_x, s, up) {
            out.push(Found { kind: lat.kind, id: None, c, count: 1, lo: f64::INFINITY, hi: f64::NEG_INFINITY, also: None });
        }
    }
    let t = cx.t;
    let mut tables: Vec<&DirTable> = vec![&t.app];
    if cx.s.align_reach.is_none_or(|r| matches!(r, Reach::Any)) {
        tables.push(t.object_lines(&mut cx.work.made));
    }
    for table in tables {
        cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(table.dirs.len()));
        let Some(dir) = table.find(angle, &cx.co) else { continue };
        let sets: Vec<_> = dir.sets.iter().filter(|set| cx.rule(set.kind).on && matches(cx, set.kind, side, set.side)).collect();
        for set in sets {
            cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(set.entries.len()));
            out.extend(ahead_in(cx, &set.entries, s, up).map(|e| Found {
                kind: set.kind,
                id: e.id,
                c: e.offset,
                count: e.count,
                lo: e.lo,
                hi: e.hi,
                also: None,
            }));
        }
    }
    if let Some(r) = cx.s.align_reach.filter(|r| !matches!(r, Reach::Any)) {
        // The nearest of each kind strictly ahead, not the nearest of each kind (which may be behind the side, and hide the next one that is ahead).
        from_near_objects(cx, r.document(cx.zoom).max(reach), angle, s, side, Pick::Past(up), &mut out);
    }
    out
}

/// The nearest line of kind `kind` strictly past the offset `from` in the direction `angle`, above it when `up`, else below: what a side may meet
/// when the nearest line of its kind cannot be taken (it would leave a resized box no size).
pub(crate) fn line_past(cx: &mut Ctx<'_>, angle: f64, from: f64, side: Side, kind: Kind, up: bool) -> Option<Found> {
    if !cx.rule(kind).on {
        return None;
    }
    let mut found: Vec<Found> = Vec::new();
    let (axis_x, axis_y) = (angle.abs() <= cx.co.angle, (angle - FRAC_PI_2).abs() <= cx.co.angle);
    if axis_x || axis_y {
        for lat in cx.q.lattices.iter().filter(|l| l.kind == kind && matches(cx, kind, side, None)) {
            if let Some(c) = lattice_ahead(cx, lat, axis_x, from, up) {
                found.push(Found { kind, id: None, c, count: 1, lo: f64::INFINITY, hi: f64::NEG_INFINITY, also: None });
            }
        }
    }
    let t = cx.t;
    let mut tables: Vec<&DirTable> = vec![&t.app];
    if cx.s.align_reach.is_none_or(|r| matches!(r, Reach::Any)) {
        tables.push(t.object_lines(&mut cx.work.made));
    }
    for table in tables {
        cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(table.dirs.len()));
        let Some(dir) = table.find(angle, &cx.co) else { continue };
        let sets: Vec<_> = dir.sets.iter().filter(|set| set.kind == kind && matches(cx, kind, side, set.side)).collect();
        for set in sets {
            cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(log_steps(set.entries.len()));
            found.extend(ahead_in(cx, &set.entries, from, up).map(|e| Found {
                kind,
                id: e.id,
                c: e.offset,
                count: e.count,
                lo: e.lo,
                hi: e.hi,
                also: None,
            }));
        }
    }
    if let Some(r) = cx.s.align_reach.filter(|r| !matches!(r, Reach::Any)) {
        let mut near = Vec::new();
        from_near_objects(cx, r.document(cx.zoom), angle, from, side, Pick::Past(up), &mut near);
        found.extend(near.into_iter().filter(|f| f.kind == kind));
    }
    let distance = |f: &Found| (f.c - from).abs();
    found.into_iter().min_by(|a, b| distance(a).total_cmp(&distance(b)).then(a.id.cmp(&b.id)))
}

/// The nearest line of each kind to the offset `s` of a side of the subject in the direction `angle`.
pub(crate) fn lines_near(cx: &mut Ctx<'_>, angle: f64, s: f64, side: Side) -> Vec<Found> {
    lines_find(cx, angle, s, side, None)
}

/// The candidates of every side of one probe.
pub(crate) fn probe_cands(cx: &mut Ctx<'_>, probe: &Probe, free: Free) -> Vec<Cand> {
    let mut out = Vec::new();
    for ps in &probe.sides {
        for f in lines_near(cx, probe.angle, ps.offset, ps.side) {
            let d = f.c - ps.offset;
            let travel = match free {
                Free::Normal => Some(d),
                Free::Along(e) => Some(d / probe.normal.dot(e)),
            };
            let Some(travel) = travel.filter(|t| t.is_finite()) else { continue };
            let across = if let Free::Along(e) = free { probe.normal.dot(e) } else { 1.0 };
            if !cx.pulls(f.kind, travel.abs()) || cx.too_oblique(f.kind, 1.0, across) {
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
                s: ps.offset,
                d: travel,
                t: travel.abs(),
                pt: 0,
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
    use crate::settings::{Matching, Settings};
    use crate::targets::{Limits, Object, Parts, TargetsBuilder};

    fn query<'a>(s: &'a Settings) -> Query<'a> {
        Query {
            subject: Subject::Point { at: Point::ORIGIN, base: None },
            zoom: 1.0,
            settings: s,
            constraint: Constraint::None,
            rounding: Rounding::None,
            lattices: &[],
            view: None,
            budget: Budget::default(),
        }
    }

    fn built(objects: &[(u64, TurnedBox)], guides: &[f64]) -> crate::targets::Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for (id, f) in objects {
            b.object(Id(*id), Object { frame: *f, bounds: None, angle: None, parts: Parts::ALL }).unwrap();
        }
        for g in guides {
            b.line(None, Axis::X, *g, None, Kind::Guide).unwrap();
        }
        b.build()
    }

    #[test]
    fn the_nearest_line_of_each_kind_is_found_wherever_the_object_is() {
        let t = built(
            &[
                (1, TurnedBox::square(Point::new(100.0, 50.0), Size::new(10.0, 4.0))),
                (2, TurnedBox::square(Point::new(-300.0, 0.0), Size::new(2.0, 2.0))),
            ],
            &[7.0],
        );
        let s = Settings::default();
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        let at = 96.0 - cx.o.x;
        let mut found = lines_near(&mut cx, 0.0, at, Side::Min);
        found.sort_by_key(|a| a.kind);
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
        let kinds = |cx: &mut Ctx<'_>, side| {
            {
                let at = 10.5 - cx.o.x;
                lines_near(cx, 0.0, at, side)
            }
            .iter()
            .map(|f| f.kind)
            .collect::<Vec<_>>()
        };
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
    fn a_lattice_exactly_as_coarse_as_the_same_place_rule_has_no_line_ahead_and_one_a_hair_coarser_has() {
        let t = built(&[], &[]);
        let s = Settings::default();
        // At 0 the rule is its absolute part, with nothing added for a subject at the origin.
        let tol = Ctx::new(&t, &query(&s)).co.tol(0.0, 0.0);
        assert_eq!(tol, 1.0e-9);
        let ahead = |pitch: f64, up: bool| {
            let lat = [crate::query::Lattice { origin: Point::ORIGIN, pitch: Vec2::new(pitch, pitch), kind: Kind::Grid }];
            let q = Query { lattices: &lat, ..query(&s) };
            let mut cx = Ctx::new(&t, &q);
            lines_ahead(&mut cx, 0.0, 0.0, Side::Mid, up, 0.0).into_iter().filter(|f| f.kind == Kind::Grid).map(|f| f.c).collect::<Vec<f64>>()
        };
        for up in [true, false] {
            assert!(ahead(tol, up).is_empty(), "a pitch that is the rule: its lines are the side's place or the next one's");
            assert!(ahead(0.5 * tol, up).is_empty());
        }
        let coarser = tol.next_up();
        assert_eq!(ahead(coarser, true), vec![coarser]);
        assert_eq!(ahead(coarser, false), vec![-coarser]);
    }

    #[test]
    fn a_lattice_has_a_line_at_every_pitch_and_refuses_a_far_or_bad_one() {
        let t = built(&[], &[]);
        let s = Settings::default();
        let lat = [crate::query::Lattice { origin: Point::new(1.0, 0.0), pitch: Vec2::new(10.0, f64::NAN), kind: Kind::Grid }];
        let q = Query { lattices: &lat, ..query(&s) };
        let cx = Ctx::new(&t, &q);
        let mut out = Vec::new();
        from_lattices(&cx, 0.0, 24.0, Side::Min, false, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].c, 21.0);
        out.clear();
        from_lattices(&cx, FRAC_PI_2, 24.0, Side::Min, false, &mut out);
        assert!(out.is_empty(), "a pitch that is not a number has no lines");
        from_lattices(&cx, 0.5, 24.0, Side::Min, false, &mut out);
        assert!(out.is_empty(), "a lattice is on the page axes");
        let huge = [crate::query::Lattice { origin: Point::ORIGIN, pitch: Vec2::new(1.0e-300, 1.0), kind: Kind::Grid }];
        let q = Query { lattices: &huge, ..query(&s) };
        let cx = Ctx::new(&t, &q);
        from_lattices(&cx, 0.0, 24.0, Side::Min, false, &mut out);
        assert!(out.is_empty(), "a line number past 2^52 gives no line");
    }
    /// The line of kind `kind` strictly past `from` (a document coordinate), as a document coordinate.
    fn past(cx: &mut Ctx<'_>, from: f64, side: Side, kind: Kind, up: bool) -> Option<f64> {
        let o = cx.o.x;
        line_past(cx, 0.0, from - o, side, kind, up).map(|f| f.c + o)
    }

    #[test]
    fn the_line_past_a_line_is_the_nearest_of_its_kind_strictly_beyond_it_either_way() {
        let t = built(&[], &[0.0, 1.0, 2.0, 5.0]);
        let s = Settings::default();
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        assert_eq!(past(&mut cx, 1.0, Side::Min, Kind::Guide, true), Some(2.0));
        assert_eq!(past(&mut cx, 1.0, Side::Min, Kind::Guide, false), Some(0.0));
        assert_eq!(past(&mut cx, 2.0, Side::Min, Kind::Guide, true), Some(5.0));
        assert_eq!(past(&mut cx, 5.0, Side::Min, Kind::Guide, true), None, "nothing past the last");
        assert_eq!(past(&mut cx, 0.0, Side::Min, Kind::Guide, false), None, "nothing before the first");
        // A line the same place as `from` (within the rule) is the place `from` is on, not past it, either side of it.
        assert_eq!(past(&mut cx, 1.0 - 5.0e-10, Side::Min, Kind::Guide, true), Some(2.0));
        assert_eq!(past(&mut cx, 1.0 + 5.0e-10, Side::Min, Kind::Guide, false), Some(0.0));
        assert_eq!(past(&mut cx, 1.0 - 2.0e-9, Side::Min, Kind::Guide, true), Some(1.0), "a line farther than the rule is past it");
        // Between lines, and the kinds that have none.
        assert_eq!(past(&mut cx, 3.0, Side::Min, Kind::Guide, true), Some(5.0));
        assert_eq!(past(&mut cx, 3.0, Side::Min, Kind::Guide, false), Some(2.0));
        assert_eq!(past(&mut cx, 1.0, Side::Min, Kind::Layout, true), None);
        assert!(cx.work.bisect_steps > 0, "the bisections are counted");
    }

    #[test]
    fn the_line_past_reads_the_kind_asked_for_and_the_matching_rule_and_a_kind_that_is_off() {
        let boxes =
            [(1, TurnedBox::square(Point::new(10.0, 0.0), Size::new(4.0, 4.0))), (2, TurnedBox::square(Point::new(20.0, 0.0), Size::new(2.0, 2.0)))];
        let t = built(&boxes, &[15.0]);
        let s = Settings::default();
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        // Edges at 8, 12, 19 and 21, centres at 10 and 20, a guide at 15.
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Edge, true), Some(12.0));
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Centre, true), Some(10.0));
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Guide, true), Some(15.0));
        assert_eq!(past(&mut cx, 12.0, Side::Min, Kind::Edge, false), Some(8.0));
        // The left edges and the right edges are two sets of the kind: past 20 going down, the nearest of the two sets' lines is 19, not the 12 of the other.
        assert_eq!(past(&mut cx, 20.0, Side::Min, Kind::Edge, false), Some(19.0));
        assert_eq!(past(&mut cx, 13.0, Side::Min, Kind::Edge, true), Some(19.0), "up, the nearest of the sets' lines");
        // Under like to like an edge is met by a side that is not a middle, and a centre by a middle only.
        let like = Settings { matching: Matching::LikeToLike, ..Settings::default() };
        let q = query(&like);
        let mut cx = Ctx::new(&t, &q);
        assert_eq!(past(&mut cx, 8.0, Side::Mid, Kind::Edge, true), None);
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Edge, true), Some(12.0));
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Centre, true), None);
        assert_eq!(past(&mut cx, 8.0, Side::Mid, Kind::Centre, true), Some(10.0));
        // A kind that is off has no lines.
        let mut off = Settings::default();
        off.kinds.set(Kind::Edge, crate::kind::KindRule { on: false, ..off.kinds.rule(Kind::Edge) });
        let q = query(&off);
        let mut cx = Ctx::new(&t, &q);
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Edge, true), None);
        assert_eq!(past(&mut cx, 8.0, Side::Min, Kind::Centre, true), Some(10.0));
    }

    #[test]
    fn the_line_past_reads_the_objects_near_the_subject_when_alignment_has_a_reach() {
        let objects: Vec<(u64, TurnedBox)> =
            [10.0, 20.0, 30.0].iter().enumerate().map(|(i, x)| (i as u64, TurnedBox::square(Point::new(*x, 0.0), Size::new(2.0, 2.0)))).collect();
        let t = built(&objects, &[]);
        let s = Settings { align_reach: Some(Reach::Document(25.0)), ..Settings::default() };
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        cx.region = kurbo::Rect::new(14.0 - t.origin.x, -1.0, 16.0 - t.origin.x, 1.0);
        // Edges at 9, 11, 19, 21, 29 and 31. From 12 the nearest edge is behind (11), and the one past 12 is 19: the nearest past it, not the nearest
        // that happens to be past it.
        assert_eq!(past(&mut cx, 12.0, Side::Min, Kind::Edge, true), Some(19.0));
        assert_eq!(past(&mut cx, 12.0, Side::Min, Kind::Edge, false), Some(11.0));
        assert_eq!(past(&mut cx, 11.0, Side::Min, Kind::Edge, true), Some(19.0), "the line `from` is on is not past it");
        assert_eq!(past(&mut cx, 11.0, Side::Min, Kind::Edge, false), Some(9.0));
        assert_eq!(past(&mut cx, 31.0, Side::Min, Kind::Edge, true), None);
        assert_eq!(past(&mut cx, 12.0, Side::Min, Kind::Centre, true), Some(20.0));
        assert!(!cx.work.made.has(crate::budget::Made::OBJECT_LINES), "the table of the objects' lines is not made");
    }

    #[test]
    fn the_line_past_a_lattice_line_is_the_next_line_of_the_lattice_of_that_kind() {
        let t = built(&[], &[]);
        let mut s = Settings::default();
        s.kinds.set(Kind::Ruler, crate::kind::KindRule { on: true, ..s.kinds.rule(Kind::Ruler) });
        let lat = [
            crate::query::Lattice { origin: Point::new(0.5, 0.0), pitch: Vec2::new(2.0, 2.0), kind: Kind::Grid },
            crate::query::Lattice { origin: Point::new(0.0, 0.0), pitch: Vec2::new(0.5, 0.5), kind: Kind::Ruler },
        ];
        let q = Query { lattices: &lat, ..query(&s) };
        let mut cx = Ctx::new(&t, &q);
        // The grid has lines at 0.5, 2.5, 4.5, ...; the ruler's are every half unit, and the grid's line is not past itself.
        assert_eq!(past(&mut cx, 2.5, Side::Min, Kind::Grid, true), Some(4.5));
        assert_eq!(past(&mut cx, 2.5, Side::Min, Kind::Grid, false), Some(0.5));
        assert_eq!(past(&mut cx, 3.0, Side::Min, Kind::Grid, true), Some(4.5));
        assert_eq!(past(&mut cx, 2.5, Side::Min, Kind::Ruler, true), Some(3.0));
        // On the other axis the lattice is the same; along a turned direction it has no lines.
        let o = cx.o.y;
        assert_eq!(line_past(&mut cx, FRAC_PI_2, 2.5 - o, Side::Min, Kind::Grid, true).map(|f| f.c + o), Some(4.5 - 0.5));
        assert!(line_past(&mut cx, 0.5, 2.5, Side::Min, Kind::Grid, true).is_none());
    }

    #[test]
    fn a_lattice_line_the_rounding_of_its_number_brings_within_the_rule_is_stepped_past() {
        // From a sweep of pitches just above the same-place rule at 1e6: the side is on a line, the next is 4.5402e-9 away once its number is
        // rounded (the pitch is 4.5529e-9 and a number near 1e6 holds 1.2e-10), nearer than the rule's 4.5527e-9, so the stop is the one after.
        let t = built(&[], &[]);
        let s = Settings::default();
        let c = 1.0e6;
        let pitch = 4.552913998202368e-9;
        let lat = [crate::query::Lattice { origin: Point::new(c, c), pitch: Vec2::new(pitch, pitch), kind: Kind::Grid }];
        let q = Query { subject: Subject::Point { at: Point::new(c, c), base: None }, lattices: &lat, ..query(&s) };
        let mut cx = Ctx::new(&t, &q);
        let tol = cx.co.tol(c, c);
        for up in [true, false] {
            let o = cx.o.x;
            let found = lines_ahead(&mut cx, 0.0, c - 1.0 - o, Side::Min, up, 0.0);
            let line = found.iter().find(|f| f.kind == Kind::Grid).map(|f| f.c + o).expect("a lattice of this pitch has lines");
            let travel = if up { line - (c - 1.0) } else { (c - 1.0) - line };
            assert!(travel > tol && travel < 3.0 * pitch, "up {up}: stops {travel:e} away (the rule is {tol:e})");
        }
        // And a lattice whose neighbours are all inside the rule has no stop at all, whichever way it looks.
        let fine = [crate::query::Lattice { origin: Point::new(c, c), pitch: Vec2::new(0.9 * tol, 0.9 * tol), kind: Kind::Grid }];
        let q = Query { subject: Subject::Point { at: Point::new(c, c), base: None }, lattices: &fine, ..query(&s) };
        let mut cx = Ctx::new(&t, &q);
        let o = cx.o.x;
        assert!(lines_ahead(&mut cx, 0.0, c - 1.0 - o, Side::Min, true, 0.0).iter().all(|f| f.kind != Kind::Grid));
    }
    #[test]
    fn the_first_line_past_the_rule_is_found_in_either_direction_from_each_side_for_pitches_that_need_all_three_looks() {
        // Found by a search: pitches just above the same-place rule, far from the origin, for which the line after the side's own is within the rule too
        // (the pitch is 4.5527e-9 and a number near 1e6 holds 1.2e-10), so the third line looked at is the stop, whichever way the nudge goes.
        let t = built(&[], &[]);
        let s = Settings::default();
        for (c, pitch) in
            [(1.0e5, 1.3552858983205859e-9), (1.0e6, 4.552720418416241e-9), (1.0e6, 4.5527463689044615e-9), (3.0e6, 1.1658162076332066e-8)]
        {
            let lat = [crate::query::Lattice { origin: Point::new(c, c), pitch: Vec2::new(pitch, pitch), kind: Kind::Grid }];
            let q = Query { subject: Subject::Point { at: Point::new(c, c), base: None }, lattices: &lat, ..query(&s) };
            let mut cx = Ctx::new(&t, &q);
            let (tol, o) = (cx.co.tol(c, c), cx.o.x);
            for side in [c - 1.0, c, c + 1.0] {
                for up in [true, false] {
                    // By a scan of the lines round the side: the nearest that is farther from it than the rule, the way the nudge goes.
                    let line = |k: f64| c + k * pitch;
                    let n = ((side - c) / pitch).round();
                    let want = (-6..=6)
                        .map(|d| line(n + f64::from(d)))
                        .filter(|l| if up { l - side > tol } else { side - l > tol })
                        .min_by(|a, b| (a - side).abs().total_cmp(&(b - side).abs()))
                        .expect("a line is ahead");
                    let found = lines_ahead(&mut cx, 0.0, side - o, Side::Min, up, 0.0);
                    let got = found.iter().find(|f| f.kind == Kind::Grid).map(|f| f.c + o);
                    assert_eq!(got, Some(want), "c {c:e} pitch {pitch:e} side {side} up {up}");
                }
            }
        }
    }
    #[test]
    fn a_line_of_an_object_near_the_subject_that_is_the_same_place_as_the_offset_is_not_past_it() {
        // A shape of no width 5e-10 past the edge at 11 is the same place as it (the rule is 1e-9): not past it, going up, and not past it going down from 11
        // + 5e-10. The next edge past 11 is the one at 19.
        let objects = [
            (0, TurnedBox::square(Point::new(10.0, 0.0), Size::new(2.0, 2.0))),
            (1, TurnedBox::square(Point::new(11.0 + 5.0e-10, 0.0), Size::new(0.0, 2.0))),
            (2, TurnedBox::square(Point::new(20.0, 0.0), Size::new(2.0, 2.0))),
        ];
        let t = built(&objects, &[]);
        let s = Settings { align_reach: Some(Reach::Document(25.0)), ..Settings::default() };
        let q = query(&s);
        let mut cx = Ctx::new(&t, &q);
        cx.region = kurbo::Rect::new(14.0 - t.origin.x, -1.0, 16.0 - t.origin.x, 1.0);
        assert_eq!(past(&mut cx, 11.0, Side::Min, Kind::Edge, true), Some(19.0));
        assert_eq!(past(&mut cx, 11.0 + 5.0e-10, Side::Min, Kind::Edge, false), Some(9.0));
        // A line farther than the rule is past it.
        assert_eq!(past(&mut cx, 11.0 - 5.0e-9, Side::Min, Kind::Edge, true), Some(11.0));
    }
}
