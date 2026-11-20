//! Places: the app's own points, the corners and side middles of other objects' turned boxes, and, for the few objects near the subject, the
//! places and outlines the detail source gives, with the object snaps on them (vertex, midpoint, nearest, intersection, tangent, perpendicular).
//!
//! A place pulls both degrees of a move at once. Every place weighed is charged to the `points` budget first, every path to the `pieces` budget
//! before it is read, and the objects the detail source is asked about are capped. The nearest place wins within the settings; a vertex or an
//! intersection wins a near tie by 15% of its reach, as a CAD aperture does.

use std::borrow::Cow;
use std::collections::HashSet;

use kurbo::{BezPath, Point, Rect, Vec2};

use crate::budget::{Cuts, Spend, Work};
use crate::ctx::Ctx;
use crate::frame::{Axis, Id, TurnedBox};
use crate::kind::Kind;
use crate::osnap;
use crate::query::Detail;
use crate::settings::Choose;
use crate::targets::{ObjectEntry, Targets};

/// A place of a target that was weighed, kept for the marks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Gathered {
    pub kind: Kind,
    pub id: Option<Id>,
    pub number: Option<u32>,
    pub at: Point,
}

/// A target place a subject place can be pulled to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlaceCand {
    pub g: Gathered,
    pub prio: u8,
    pub reach: f64,
    /// The place of the subject that is pulled (absolute).
    pub q: Point,
    pub dist: f64,
    /// The distance less the bonus of a vertex or an intersection: what `Nearest` compares.
    pub score: f64,
    /// For a subject that travels along one axis: the move along it that puts `q` on the place, with nothing across (to the bit).
    pub by: Option<Vec2>,
}

impl PlaceCand {
    /// What to add to the subject.
    pub fn k(&self) -> Vec2 {
        self.by.unwrap_or(self.g.at - self.q)
    }
}

/// How a subject that travels along one page axis meets a place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Way {
    /// A move held to the axis: a place within its kind's reach along the line, either way.
    Both,
    /// A nudge: the first place strictly ahead, `sign` being 1 along the axis or -1 against it, at most `limit` away.
    Ahead { sign: f64, limit: f64 },
}

/// A subject that travels along one page axis: only the places on the lines its own places travel along are met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Travel {
    pub axis: Axis,
    pub way: Way,
}

/// How many buckets of the finest grid a window along a line of travel may cover from the subject: a nudge that looks as far as it can, with no
/// target nearer, reads this many and says so. (The grid cuts a wider window to the buckets round its middle, which for a window that starts at the
/// subject is the wrong part.)
pub(crate) const TRAVEL_BUCKETS: f64 = 46.0;

/// The nine places of a turned box: its corners, the middles of its sides and its centre.
pub(crate) fn box_places(f: &TurnedBox) -> Vec<Point> {
    let c = f.corners();
    let mids = [c[0].midpoint(c[1]), c[1].midpoint(c[2]), c[2].midpoint(c[3]), c[3].midpoint(c[0])];
    c.into_iter().chain(mids).chain([f.centre]).collect()
}

struct Weigh<'a> {
    subject: &'a [Point],
    best: Option<PlaceCand>,
    travel: Option<Travel>,
}

/// The pull of the target place `g` on the subject place `q` when the subject travels along one axis: only a place on the line `q` travels along is
/// met (the same place across the axis, by the same-place rule), and the subject lands on it.
fn on_travel(cx: &Ctx<'_>, tr: Travel, g: Gathered, q: Point, prio: u8, reach: f64) -> Option<PlaceCand> {
    let (at_g, at_q, same_line) = match tr.axis {
        Axis::X => (g.at.x, q.x, cx.co.same(g.at.y, q.y)),
        Axis::Y => (g.at.y, q.y, cx.co.same(g.at.x, q.x)),
    };
    let along = at_g - at_q;
    if !(same_line && along.is_finite()) {
        return None;
    }
    let (dist, reach, bonus) = match tr.way {
        Way::Both => {
            let d = along.abs();
            if !cx.pulls(g.kind, d) {
                return None;
            }
            (d, reach, if matches!(g.kind, Kind::Vertex | Kind::Intersection) && reach.is_finite() { 0.15 * reach } else { 0.0 })
        }
        Way::Ahead { sign, limit } => {
            // Strictly ahead is past the same-place rule, so a place the subject's own place is on is not the next one.
            let t = along * sign;
            if !(t > 0.0 && !cx.co.same(at_g, at_q) && t <= limit + cx.co.tol(t, limit.min(1.0e300))) {
                return None;
            }
            (t, f64::INFINITY, 0.0)
        }
    };
    let by = match tr.axis {
        Axis::X => Vec2::new(along, 0.0),
        Axis::Y => Vec2::new(0.0, along),
    };
    Some(PlaceCand { g, prio, reach, q, dist, score: dist - bonus, by: Some(by) })
}

fn order(a: &PlaceCand, b: &PlaceCand, choose: Choose, co: &crate::frame::Coincide) -> std::cmp::Ordering {
    let by_score = if co.same(a.score, b.score) { std::cmp::Ordering::Equal } else { a.score.total_cmp(&b.score) };
    let first = match choose {
        Choose::Nearest => by_score.then(a.prio.cmp(&b.prio)),
        Choose::Ranked => a.prio.cmp(&b.prio).then(by_score),
    };
    first.then(a.g.id.cmp(&b.g.id)).then(a.g.number.cmp(&b.g.number)).then(a.g.kind.cmp(&b.g.kind)).then(a.dist.total_cmp(&b.dist))
}

impl Weigh<'_> {
    /// Weighs one target place against the subject's places: charged first, kept for the marks, and the best kept.
    fn offer(&mut self, cx: &mut Ctx<'_>, g: Gathered) {
        let cap = u64::from(cx.budget().points);
        if !g.at.is_finite() || !cx.rule(g.kind).on {
            return;
        }
        if !Work::charge(&mut cx.work.points, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::POINTS) {
            return;
        }
        cx.gathered.push(g);
        let rule = cx.rule(g.kind);
        let reach = cx.reach(g.kind);
        // A nudge stops at the first place ahead: the nearest wins whatever `choose` says, and priority only tells apart places as near as each other.
        let choose = match self.travel {
            Some(Travel { way: Way::Ahead { .. }, .. }) => Choose::Nearest,
            _ => cx.s.choose,
        };
        for q in self.subject {
            let cand = match self.travel {
                Some(tr) => on_travel(cx, tr, g, *q, rule.priority, reach),
                None => {
                    let dist = (g.at - *q).hypot();
                    let bonus = if matches!(g.kind, Kind::Vertex | Kind::Intersection) && reach.is_finite() { 0.15 * reach } else { 0.0 };
                    (dist.is_finite() && cx.pulls(g.kind, dist)).then_some(PlaceCand {
                        g,
                        prio: rule.priority,
                        reach,
                        q: *q,
                        dist,
                        score: dist - bonus,
                        by: None,
                    })
                }
            };
            if let Some(cand) = cand
                && self.best.is_none_or(|b| order(&cand, &b, choose, &cx.co).is_lt())
            {
                self.best = Some(cand);
            }
        }
    }
}

/// The paths of one object: those the detail source builds, and those the app gave the builder, which are borrowed: a path is not copied before
/// it has been charged for, and one that is refused is never copied.
fn paths_of<'t>(cx: &mut Ctx<'t>, detail: &mut dyn Detail, id: Id) -> Vec<Cow<'t, BezPath>> {
    let q = cx.q;
    let mut built = Vec::new();
    {
        let mut spend = Spend { work: &mut cx.work, budget: &q.budget };
        detail.outline(id, &mut spend, &mut built);
    }
    let t: &'t Targets = cx.t;
    let i = t.paths.partition_point(|p| p.0 < id);
    let mut out: Vec<Cow<'t, BezPath>> = built.into_iter().map(Cow::Owned).collect();
    out.extend(t.paths.iter().skip(i).take_while(|p| p.0 == id).map(|p| Cow::Borrowed(&p.1)));
    out
}

/// The object snaps on the outlines of one object, offered as places.
fn osnaps(
    cx: &mut Ctx<'_>,
    w: &mut Weigh<'_>,
    id: Id,
    paths: &[Cow<'_, BezPath>],
    base: Option<Point>,
    probe: &Rect,
    sets: &mut Vec<Vec<kurbo::Line>>,
) {
    let cap = u64::from(cx.budget().pieces);
    let flat = cx.budget().flatness;
    for path in paths {
        let cost = path.elements().len() as u64;
        if !Work::charge(&mut cx.work.pieces, cap, cost, &mut cx.work.cut, &mut cx.work.truncated, Cuts::PIECES) {
            return;
        }
        let near_from: Vec<Point> = w.subject.iter().copied().take(9).collect();
        let mut offer = |cx: &mut Ctx<'_>, kind: Kind, p: Point| w.offer(cx, Gathered { kind, id: Some(id), number: None, at: p });
        if cx.rule(Kind::Vertex).on {
            for p in osnap::endpoints(path) {
                offer(cx, Kind::Vertex, p);
            }
        }
        if cx.rule(Kind::Midpoint).on {
            for p in osnap::midpoints(path) {
                offer(cx, Kind::Midpoint, p);
            }
        }
        if cx.rule(Kind::Nearest).on {
            for q in near_from {
                if !Work::charge(&mut cx.work.pieces, cap, cost, &mut cx.work.cut, &mut cx.work.truncated, Cuts::PIECES) {
                    break;
                }
                if let Some((p, _)) = osnap::nearest(path, q) {
                    offer(cx, Kind::Nearest, p);
                }
            }
        }
        if let Some(b) = base {
            // Each solve is charged by the segments it reads: a curve costs what its sampling and bisection do, not what a line does.
            let (segments, curves) = osnap::segment_counts(path);
            let solve = curves.saturating_mul(osnap::CURVE_SOLVE_COST);
            if cx.rule(Kind::Tangent).on && Work::charge(&mut cx.work.pieces, cap, solve, &mut cx.work.cut, &mut cx.work.truncated, Cuts::PIECES) {
                osnap::tangents(path, b).into_iter().for_each(|p| offer(cx, Kind::Tangent, p));
            }
            if cx.rule(Kind::Perpendicular).on
                && Work::charge(&mut cx.work.pieces, cap, solve.saturating_add(segments), &mut cx.work.cut, &mut cx.work.truncated, Cuts::PIECES)
            {
                osnap::perpendiculars(path, b).into_iter().for_each(|p| offer(cx, Kind::Perpendicular, p));
            }
        }
        if cx.rule(Kind::Intersection).on {
            let left = cap.saturating_sub(cx.work.pieces) as usize;
            let found = osnap::pieces_near(path, cx.o, Some(probe), left, flat);
            cx.work.pieces = cx.work.pieces.saturating_add(found.charged as u64);
            cx.work.flattened = cx.work.flattened.saturating_add(found.charged as u64);
            if found.cut {
                cx.work.cut(Cuts::PIECES);
            }
            if !found.lines.is_empty() {
                sets.push(found.lines);
            }
        }
    }
}

/// The crossings of the outlines near, pair by pair, under the pairs cap and the pieces cap.
fn crossings(cx: &mut Ctx<'_>, w: &mut Weigh<'_>, sets: &[Vec<kurbo::Line>], probe: &Rect) {
    let (pair_cap, piece_cap) = (u64::from(cx.budget().pairs), u64::from(cx.budget().pieces));
    for (i, a) in sets.iter().enumerate() {
        for b in sets.iter().skip(i + 1) {
            let cost = (a.len() as u64).saturating_mul(b.len() as u64);
            if !Work::charge(&mut cx.work.pairs, pair_cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::PAIRS)
                || !Work::charge(&mut cx.work.pieces, piece_cap, cost, &mut cx.work.cut, &mut cx.work.truncated, Cuts::PIECES)
            {
                return;
            }
            for x in osnap::crossings(a, b, Some(*probe)) {
                w.offer(cx, Gathered { kind: Kind::Intersection, id: None, number: None, at: x + cx.o.to_vec2() });
            }
        }
    }
}

/// The places of one candidate object: its corners and side middles, and the detail source's places and outlines.
/// What the objects near are scanned for: how many outlines were asked of the detail source, the pieces of those outlines, the window and the
/// base point of a tangent or perpendicular.
struct Scan {
    asked: u32,
    sets: Vec<Vec<kurbo::Line>>,
    probe: Rect,
    base: Option<Point>,
}

fn from_object(cx: &mut Ctx<'_>, w: &mut Weigh<'_>, detail: &mut dyn Detail, o: &ObjectEntry, scan: &mut Scan) {
    let c = o.frame.corners();
    if o.parts.corners {
        c.iter().for_each(|p| w.offer(cx, Gathered { kind: Kind::Corner, id: Some(o.id), number: None, at: *p }));
    }
    if o.parts.side_middles {
        let mids = [c[0].midpoint(c[1]), c[1].midpoint(c[2]), c[2].midpoint(c[3]), c[3].midpoint(c[0])];
        mids.iter().for_each(|p| w.offer(cx, Gathered { kind: Kind::SideMiddle, id: Some(o.id), number: None, at: *p }));
    }
    if !o.parts.detail {
        return;
    }
    let outlines_cap = cx.budget().outlines;
    let wants_outline =
        [Kind::Vertex, Kind::Midpoint, Kind::Nearest, Kind::Intersection, Kind::Tangent, Kind::Perpendicular].iter().any(|k| cx.rule(*k).on);
    if scan.asked >= outlines_cap {
        cx.work.cut(Cuts::OUTLINES);
        return;
    }
    scan.asked += 1;
    cx.work.outlines += 1;
    let mut pts = Vec::new();
    {
        let q = cx.q;
        let mut spend = Spend { work: &mut cx.work, budget: &q.budget };
        detail.points(o.id, &mut spend, &mut pts);
    }
    for (at, kind, number) in pts {
        w.offer(cx, Gathered { kind, id: Some(o.id), number: Some(number), at });
    }
    if wants_outline {
        let paths = paths_of(cx, detail, o.id);
        osnaps(cx, w, o.id, &paths, scan.base, &scan.probe, &mut scan.sets);
    }
}

/// A window of a bucket grid, and the point its buckets are read from (`None`: its middle).
struct Strip {
    rect: Rect,
    near: Option<Point>,
}

/// The subject's places that travel along one line.
struct Line {
    across: (f64, f64),
    along: (f64, f64),
    tol: f64,
}

/// The most strips of one grid a query gathers for a subject that travels along one axis. A box offers up to nine lines of travel (a turned one nine
/// different ones), and each strip is a gather out of the 16 a query has: lines that lie nearest each other share a strip, so that the places of a turned
/// box and of what moves with it cost at most this many windows of each grid.
pub(crate) const MAX_STRIPS: usize = 4;

/// The lines of travel, sorted by where they are across the axis, joined into at most [`MAX_STRIPS`] bands: the lines are cut where the gaps between
/// neighbours are the widest (the earlier gap on a tie), so a band is lines that lie near each other.
fn bands(lines: Vec<Line>) -> Vec<Line> {
    if lines.len() <= MAX_STRIPS {
        return lines;
    }
    // The gap after line `i` is between it and the next.
    let mut gaps: Vec<(f64, usize)> =
        lines.windows(2).enumerate().filter_map(|(i, w)| if let [a, b] = w { Some((b.across.0 - a.across.1, i)) } else { None }).collect();
    gaps.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut cuts: Vec<usize> = gaps.into_iter().take(MAX_STRIPS - 1).map(|g| g.1).collect();
    cuts.sort_unstable();
    let mut out: Vec<Line> = Vec::new();
    let (mut next_cut, mut fresh) = (0, true);
    for (i, l) in lines.into_iter().enumerate() {
        match out.last_mut() {
            Some(band) if !fresh => {
                band.across = (band.across.0.min(l.across.0), band.across.1.max(l.across.1));
                band.along = (band.along.0.min(l.along.0), band.along.1.max(l.along.1));
                band.tol = band.tol.max(l.tol);
            }
            _ => out.push(l),
        }
        // A cut gap ends the band after this line.
        fresh = cuts.get(next_cut) == Some(&i);
        next_cut += usize::from(fresh);
    }
    out
}

/// The windows to gather for a subject that travels along one axis: one thin strip for each line its places travel along (places the same across the
/// axis, by the same-place rule, share one), in the order of the lines across. A move looks a reach either side of its places along the line; a nudge
/// looks ahead of them, up to `cap` from the subject (and `cut` says when that was less than it was asked).
fn travel_strips(cx: &Ctx<'_>, subject: &[Point], tr: Travel, span: f64, cap: f64) -> (Vec<Strip>, bool) {
    // Each place: where it is across the axis (in absolute numbers, for the same-place rule, and about the grid's origin) and along it.
    let mut places: Vec<(f64, f64, f64)> = subject
        .iter()
        .map(|p| {
            let r = *p - cx.o;
            match tr.axis {
                Axis::X => (p.y, r.y, r.x),
                Axis::Y => (p.x, r.x, r.y),
            }
        })
        .collect();
    places.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.total_cmp(&b.2)));
    let mut lines: Vec<(f64, Line)> = Vec::new();
    for (across_abs, across, along) in places {
        let tol = cx.co.tol(across_abs, across_abs);
        match lines.last_mut() {
            Some((first, line)) if cx.co.same(*first, across_abs) => {
                line.across = (line.across.0.min(across), line.across.1.max(across));
                line.along = (line.along.0.min(along), line.along.1.max(along));
                line.tol = line.tol.max(tol);
            }
            _ => lines.push((across_abs, Line { across: (across, across), along: (along, along), tol })),
        }
    }
    let mut cut = false;
    let strips = bands(lines.into_iter().map(|(_, l)| l).collect())
        .into_iter()
        .map(|l| {
            let (a0, a1) = (l.across.0 - 2.0 * l.tol, l.across.1 + 2.0 * l.tol);
            let (lo, hi) = l.along;
            let (along, near) = match tr.way {
                Way::Both => ((lo - span, hi + span), 0.5 * (lo + hi)),
                Way::Ahead { sign, limit } => {
                    let reach = limit.min(cap);
                    cut |= reach < limit;
                    if sign > 0.0 { ((lo, hi + reach), lo) } else { ((lo - reach, hi), hi) }
                }
            };
            let mid = 0.5 * (a0 + a1);
            match tr.axis {
                Axis::X => Strip { rect: Rect::new(along.0, a0, along.1, a1), near: Some(Point::new(near, mid)) },
                Axis::Y => Strip { rect: Rect::new(a0, along.0, a1, along.1), near: Some(Point::new(mid, near)) },
            }
        })
        .collect();
    (strips, cut)
}

/// The best place the subject's places can be pulled to, or none. The places weighed are kept in the context for the marks.
///
/// With `travel`, the subject moves along one page axis only: its places are met on the lines they travel along, in one thin window for each line
/// (each one a gather), and the best is the nearest along its line.
pub(crate) fn solve_places(
    cx: &mut Ctx<'_>,
    detail: &mut dyn Detail,
    subject: &[Point],
    base: Option<Point>,
    travel: Option<Travel>,
) -> Option<PlaceCand> {
    let rules: Vec<f64> = [
        Kind::Point(0),
        Kind::Corner,
        Kind::SideMiddle,
        Kind::Vertex,
        Kind::Midpoint,
        Kind::Nearest,
        Kind::Intersection,
        Kind::Tangent,
        Kind::Perpendicular,
    ]
    .iter()
    .map(|k| cx.reach(*k))
    .filter(|r| r.is_finite())
    .collect();
    let reach = rules.into_iter().fold(0.0, f64::max);
    // How far along its line a place may be: a kind's reach for a move, the nudge's own for a nudge.
    let span = match travel {
        Some(Travel { way: Way::Ahead { limit, .. }, .. }) => limit,
        _ => reach,
    };
    if span <= 0.0 || span.is_nan() || subject.is_empty() {
        return None;
    }
    let rel: Vec<Vec2> = subject.iter().map(|p| *p - cx.o).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for v in &rel {
        (x0, y0, x1, y1) = (x0.min(v.x), y0.min(v.y), x1.max(v.x), y1.max(v.y));
    }
    let window = Rect::new(x0 - reach, y0 - reach, x1 + reach, y1 + reach);
    let mut w = Weigh { subject, best: None, travel };
    let t = cx.t;
    let (limit, visits) = (u64::from(cx.budget().candidates) as usize, u64::from(cx.budget().window_visits));
    let wants_objects = [
        Kind::Point(0),
        Kind::Corner,
        Kind::SideMiddle,
        Kind::Vertex,
        Kind::Midpoint,
        Kind::Nearest,
        Kind::Intersection,
        Kind::Tangent,
        Kind::Perpendicular,
    ]
    .iter()
    .any(|k| cx.rule(*k).on);
    let use_points = !t.points.is_empty();
    let use_objects = wants_objects && !t.objects.is_empty();
    // The windows: the one round the subject, or a strip along each line of travel (for each grid, as far as its own buckets allow).
    let mut cut = false;
    let mut strips_for = |cx: &Ctx<'_>, cell: f64| match travel {
        None => vec![Strip { rect: window, near: None }],
        Some(tr) => {
            let (strips, clamped) = travel_strips(cx, subject, tr, span, TRAVEL_BUCKETS * cell);
            cut |= clamped;
            strips
        }
    };
    let point_cell = if use_points { t.point_grid(&mut cx.work.made).cell() } else { 1.0 };
    let object_cell = if use_objects { t.object_grid(&mut cx.work.made).cell() } else { 1.0 };
    let point_strips = if use_points { strips_for(cx, point_cell) } else { Vec::new() };
    let object_strips = if use_objects { strips_for(cx, object_cell) } else { Vec::new() };
    if cut {
        cx.work.cut(Cuts::WINDOW_VISITS);
    }
    // What pieces of outline are kept to cross: those in the window, or in any strip.
    let probe = object_strips.iter().fold(None, |u: Option<Rect>, s| Some(u.map_or(s.rect, |u| u.union(s.rect)))).unwrap_or(window);
    // The app's own places.
    for strip in &point_strips {
        if !cx.work.gather() {
            break;
        }
        let g = t.point_grid(&mut cx.work.made).gather_from(&strip.rect, strip.near, limit, visits);
        note(cx, &g);
        for c in &g.items {
            if let Some(p) = t.points.get(c.index as usize) {
                w.offer(cx, Gathered { kind: p.kind, id: p.id, number: Some(p.number), at: p.at });
            }
        }
    }
    // The objects near: their boxes' places, and what the detail source gives. An object that meets two strips is looked at once.
    let mut scan = Scan { asked: 0, sets: Vec::new(), probe, base };
    let mut seen: HashSet<u32> = HashSet::new();
    for strip in &object_strips {
        if !cx.work.gather() {
            break;
        }
        let g = t.object_grid(&mut cx.work.made).gather_from(&strip.rect, strip.near, limit, visits);
        note(cx, &g);
        for c in &g.items {
            if let Some(o) = t.objects.get(c.index as usize).filter(|_| seen.insert(c.index)) {
                from_object(cx, &mut w, detail, o, &mut scan);
            }
        }
    }
    // Outlines the app gave the builder for objects that are not in the grid's window are read only through their object.
    crossings(cx, &mut w, &scan.sets, &probe);
    w.best
}

fn note(cx: &mut Ctx<'_>, g: &crate::index::Gather<'_>) {
    cx.work.examined = cx.work.examined.saturating_add(g.examined);
    cx.work.visited = cx.work.visited.saturating_add(g.visited);
    cx.work.candidates = cx.work.candidates.saturating_add(g.items.len() as u64);
    if g.cut {
        cx.work.cut(if g.visited >= u64::from(cx.budget().window_visits) { Cuts::WINDOW_VISITS } else { Cuts::CANDIDATES });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Budget;
    use crate::query::{Constraint, Query, Rounding, Subject};
    use crate::settings::Settings;
    use crate::targets::{Limits, TargetsBuilder};

    fn strips(subject: &[Point], tr: Travel, span: f64, cap: f64) -> (Vec<Strip>, bool, f64) {
        let t = TargetsBuilder::new(Limits::default()).build();
        let s = Settings::default();
        let q = Query {
            subject: Subject::Point { at: subject.first().copied().unwrap_or(Point::ORIGIN), base: None },
            zoom: 1.0,
            settings: &s,
            constraint: Constraint::None,
            rounding: Rounding::None,
            lattices: &[],
            view: None,
            budget: Budget::default(),
        };
        let cx = Ctx::new(&t, &q);
        let tol = cx.co.tol(5.0, 5.0);
        let (strips, cut) = travel_strips(&cx, subject, tr, span, cap);
        (strips, cut, tol)
    }

    fn near(s: &Strip) -> (f64, f64) {
        s.near.map(|p| (p.x, p.y)).unwrap_or((f64::NAN, f64::NAN))
    }

    #[test]
    fn a_held_move_reads_a_reach_either_side_of_the_places_on_a_line_from_their_middle() {
        let tr = Travel { axis: Axis::X, way: Way::Both };
        // Three places on two lines: y = 4 with one at x 0, and y = 5 with two at x 0 and 2.
        let (list, cut, tol) = strips(&[Point::new(2.0, 5.0), Point::new(0.0, 4.0), Point::new(0.0, 5.0)], tr, 6.0, 1.0e9);
        assert!(!cut);
        assert_eq!(list.len(), 2);
        let r0 = list[0].rect;
        assert_eq!((r0.x0, r0.x1), (-6.0, 6.0));
        assert!((r0.y0 - (4.0 - 2.0 * tol)).abs() < 1e-12 && (r0.y1 - (4.0 + 2.0 * tol)).abs() < 1e-12, "{r0:?}");
        assert_eq!(near(&list[0]).0, 0.0);
        let r1 = list[1].rect;
        assert_eq!((r1.x0, r1.x1), (-6.0, 8.0), "from 6 before the first place to 6 past the last");
        assert!((r1.y0 - (5.0 - 2.0 * tol)).abs() < 1e-12 && (r1.y1 - (5.0 + 2.0 * tol)).abs() < 1e-12, "{r1:?}");
        assert_eq!(near(&list[1]).0, 1.0, "the middle of the places on the line");
        assert!((near(&list[1]).1 - 5.0).abs() < 1e-12);
        // Along y the strips are tall and thin the other way, in the order of the lines across.
        let tr = Travel { axis: Axis::Y, way: Way::Both };
        let (list, _, _) = strips(&[Point::new(7.0, 1.0), Point::new(3.0, 3.0), Point::new(3.0, 1.0)], tr, 2.0, 1.0e9);
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].rect.y0, list[0].rect.y1), (-1.0, 5.0));
        assert!(list[0].rect.x0 < 3.0 && list[0].rect.x1 > 3.0 && list[0].rect.x1 - list[0].rect.x0 < 1.0e-6);
        assert_eq!(near(&list[0]), (list[0].near.map_or(f64::NAN, |p| p.x), 2.0));
        assert_eq!((list[1].rect.y0, list[1].rect.y1), (-1.0, 3.0));
        assert!(list[1].rect.x0 < 7.0 && list[1].rect.x1 > 7.0);
    }

    #[test]
    fn places_that_are_one_place_across_the_axis_share_a_strip_and_two_that_are_not_do_not() {
        let tr = Travel { axis: Axis::X, way: Way::Both };
        let (list, _, tol) =
            strips(&[Point::new(0.0, 5.0), Point::new(1.0, 5.0 + 0.5 * tol_of()), Point::new(2.0, 5.0 + 4.0 * tol_of())], tr, 3.0, 1.0e9);
        assert_eq!(list.len(), 2, "5 and 5 + half the rule are one line, 5 + four times it another");
        assert!(list[0].rect.y1 >= 5.0 + 0.5 * tol_of() + tol, "{:?}", list[0].rect);
        assert_eq!((list[0].rect.x0, list[0].rect.x1), (-3.0, 4.0));
        assert_eq!((list[1].rect.x0, list[1].rect.x1), (-1.0, 5.0));
    }

    /// The same-place rule at 5 in a query on a point there.
    fn tol_of() -> f64 {
        let t = TargetsBuilder::new(Limits::default()).build();
        let s = Settings::default();
        let q = Query {
            subject: Subject::Point { at: Point::new(5.0, 5.0), base: None },
            zoom: 1.0,
            settings: &s,
            constraint: Constraint::None,
            rounding: Rounding::None,
            lattices: &[],
            view: None,
            budget: Budget::default(),
        };
        Ctx::new(&t, &q).co.tol(5.0, 5.0)
    }

    #[test]
    fn a_nudge_reads_ahead_from_its_trailing_place_or_its_leading_one_as_far_as_it_may_and_says_when_that_is_less_than_it_asked() {
        let places = [Point::new(1.0, 3.0), Point::new(3.0, 3.0)];
        // Forward along y, 10 asked, 4 allowed: from where the lowest place is to 4 past the highest, and the window was cut.
        let (up, cut, _) =
            strips(&[Point::new(3.0, 1.0), Point::new(3.0, 3.0)], Travel { axis: Axis::Y, way: Way::Ahead { sign: 1.0, limit: 10.0 } }, 10.0, 4.0);
        assert!(cut);
        assert_eq!((up[0].rect.y0, up[0].rect.y1), (1.0, 7.0));
        assert_eq!(near(&up[0]).1, 1.0, "read from the trailing place");
        // Backward: from 4 before the lowest to the highest, read from the leading place.
        let (down, cut, _) =
            strips(&[Point::new(3.0, 1.0), Point::new(3.0, 3.0)], Travel { axis: Axis::Y, way: Way::Ahead { sign: -1.0, limit: 10.0 } }, 10.0, 4.0);
        assert!(cut);
        assert_eq!((down[0].rect.y0, down[0].rect.y1), (-3.0, 3.0));
        assert_eq!(near(&down[0]).1, 3.0);
        // As far as asked, when that is not more than it may: not cut, and the window ends where it was asked to.
        let (short, cut, _) = strips(&places, Travel { axis: Axis::X, way: Way::Ahead { sign: 1.0, limit: 2.0 } }, 2.0, 4.0);
        assert!(!cut);
        assert_eq!((short[0].rect.x0, short[0].rect.x1), (1.0, 5.0));
        // A limit that is exactly what may be read is not cut either.
        let (exact, cut, _) = strips(&places, Travel { axis: Axis::X, way: Way::Ahead { sign: 1.0, limit: 4.0 } }, 4.0, 4.0);
        assert!(!cut && exact[0].rect.x1 == 7.0);
    }

    #[test]
    fn nine_lines_are_joined_into_four_strips_cut_at_the_widest_gaps() {
        let tr = Travel { axis: Axis::X, way: Way::Both };
        let ys = [4.0, 5.0, 6.0, 20.0, 21.0, 40.0, 41.0, 80.0, 81.0];
        let subject: Vec<Point> = ys.iter().map(|y| Point::new(0.0, *y)).collect();
        let (list, _, _) = strips(&subject, tr, 1.0, 1.0e9);
        let spans: Vec<(f64, f64)> = list.iter().map(|s| (s.rect.y0.round(), s.rect.y1.round())).collect();
        assert_eq!(spans, vec![(4.0, 6.0), (20.0, 21.0), (40.0, 41.0), (80.0, 81.0)]);
        // The gaps are the widest by their width, whatever the order the places came in, and the earlier of two equal gaps is cut first.
        let (rev, _, _) = strips(&subject.iter().rev().copied().collect::<Vec<_>>(), tr, 1.0, 1.0e9);
        assert_eq!(rev.iter().map(|s| (s.rect.y0.round(), s.rect.y1.round())).collect::<Vec<_>>(), spans);
        // Five lines a unit apart: four strips, the first two lines joined (the gaps are all equal and the earlier ones cut).
        let flat: Vec<Point> = (0..5).map(|i| Point::new(0.0, f64::from(i))).collect();
        let (list, _, _) = strips(&flat, tr, 1.0, 1.0e9);
        assert_eq!(
            list.iter().map(|s| (s.rect.y0.round(), s.rect.y1.round())).collect::<Vec<_>>(),
            vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 4.0)]
        );
    }
}
