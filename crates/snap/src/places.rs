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
                && self.best.is_none_or(|b| order(&cand, &b, cx.s.choose, &cx.co).is_lt())
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
    let strips = lines
        .into_iter()
        .map(|(_, l)| {
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
