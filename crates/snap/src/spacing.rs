// Equal spacing follows VectorCraft's `even_spacing` and `gap_marks` (storytold/vectorcraft at 9a01d266b0438f96608c9f75b51437f912587d22,
// crates/tools/src/guides/targets.rs) and FigCraft's `spacings` (storytold/figcraft at 4548d6c00953dd051ee36af3bada376d1a80d05c,
// crates/engine/src/canvas/snap.rs). Copyright (c) 2026 ArtCraft Team and the VectorCraft and FigCraft contributors. MIT OR Apache-2.0.
// Changed: rows are gathered from the bucket grid under a cap, work along any direction (the page's, and a turned subject's own), and the
// marks are derived from the final geometry.

//! Equal spacing: snap one gap, mark every equal gap.
//!
//! The subject's gap to a neighbour becomes equal to another gap: the gap on its other side (centred between its neighbours), or any gap in the
//! row. It never moves more than the subject. Then every gap in the row that equals it is marked, with its value, joined into chains.
//!
//! A row is taken along one direction: `X` or `Y` for the box round the subject, and `u` or `v` for a turned subject's own sides. An
//! own-direction row holds only objects turned by the subject's angle modulo a quarter turn.

use kurbo::{Line, Point, Vec2};

use crate::budget::{Cuts, Work};
use crate::ctx::{Cand, Ctx, Probe};
use crate::frame::{Direction, Id, OwnOrBounds, Side, TurnedBox};
use crate::kind::Kind;
use crate::result::Mark;
use crate::settings::SpacingGaps;
use crate::solve::{HitRec, Solution};

/// One object of a row: its extent along the row's direction and across it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowObj {
    pub id: Id,
    pub lo: f64,
    pub hi: f64,
    pub c0: f64,
    pub c1: f64,
}

/// The extent of a box along a unit direction, relative to `origin`.
pub(crate) fn extent(f: &TurnedBox, n: Vec2, origin: Point) -> (f64, f64) {
    let (u, v) = (f.u(), f.v());
    let c = n.dot((f.centre - origin).to_point().to_vec2());
    let half = 0.5 * (n.dot(u).abs() * f.size.width + n.dot(v).abs() * f.size.height);
    (c - half, c + half)
}

/// The extents of the box round an outline along a page axis `n` and across it, relative to `origin`.
fn outline_extents(r: &kurbo::Rect, n: Vec2, origin: Point) -> ((f64, f64), (f64, f64)) {
    let (x, y) = ((r.x0 - origin.x, r.x1 - origin.x), (r.y0 - origin.y, r.y1 - origin.y));
    if n.x.abs() > 0.5 { (x, y) } else { (y, (-x.1, -x.0)) }
}

/// A row, the subject's extent in it, and whether the row was cut short.
pub(crate) struct Row {
    pub objs: Vec<RowObj>,
    pub lo: f64,
    pub hi: f64,
    pub c0: f64,
    pub c1: f64,
}

/// The row along `n` of the objects that overlap the subject across (the band), nearest the subject first, at most `row` of them.
pub(crate) fn gather_row(cx: &mut Ctx<'_>, subject: &TurnedBox, n: Vec2, own: bool) -> Row {
    let t = Vec2::new(-n.y, n.x);
    let ((lo, hi), (c0, c1)) = (extent(subject, n, cx.o), extent(subject, t, cx.o));
    let r = cx.s.spacing_reach;
    let reach = (r.times * (hi - lo)).max(r.at_least);
    let corners = [(lo - reach, c0), (lo - reach, c1), (hi + reach, c0), (hi + reach, c1)].map(|(a, c)| n * a + t * c);
    let window = kurbo::Rect::new(
        corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    );
    if !cx.work.gather() {
        return Row { objs: Vec::new(), lo, hi, c0, c1 };
    }
    let tt = cx.t;
    let (limit, visits) = (u64::from(cx.budget().row) as usize, u64::from(cx.budget().window_visits));
    let g = tt.object_grid(&mut cx.work.made).gather(&window, limit, visits);
    cx.work.examined = cx.work.examined.saturating_add(g.examined);
    cx.work.visited = cx.work.visited.saturating_add(g.visited);
    cx.work.row = cx.work.row.saturating_add(g.items.len() as u64);
    if g.cut {
        cx.work.cut(if g.visited >= visits { Cuts::WINDOW_VISITS } else { Cuts::ROW });
    }
    let quarter = std::f64::consts::FRAC_PI_2;
    let objs = g
        .items
        .iter()
        .filter_map(|c| tt.objects.get(c.index as usize).map(|o| (c.index, o)))
        .filter(|(_, o)| o.parts.spacing)
        .filter(|(_, o)| !own || crate::angle::wrap(o.frame.angle - subject.angle, quarter).abs() <= cx.co.angle)
        .map(|(_, o)| {
            // A page row reads the box round an object's outline when there is one; an own row reads its own sides.
            let page = if own { None } else { o.bounds };
            let ((l, h), (a, b)) = match page {
                Some(r) => outline_extents(&r, n, cx.o),
                None => (extent(&o.frame, n, cx.o), extent(&o.frame, t, cx.o)),
            };
            RowObj { id: o.id, lo: l, hi: h, c0: a, c1: b }
        })
        .filter(|o| o.c0 < c1 && c0 < o.c1)
        .collect();
    Row { objs, lo, hi, c0, c1 }
}

/// The objects wholly before and wholly after the subject, and the row's stationary gaps (each with its two objects).
pub(crate) struct Layout {
    pub before: Vec<RowObj>,
    pub after: Vec<RowObj>,
    /// `(gap, left, right)` for each pair of an object and the first object that starts at or after its end.
    pub gaps: Vec<(f64, RowObj, RowObj)>,
}

pub(crate) fn layout(cx: &Ctx<'_>, row: &Row) -> Layout {
    let co = &cx.co;
    let mut before: Vec<RowObj> = row.objs.iter().copied().filter(|o| o.hi <= row.lo + co.tol(o.hi, row.lo)).collect();
    let mut after: Vec<RowObj> = row.objs.iter().copied().filter(|o| o.lo >= row.hi - co.tol(o.lo, row.hi)).collect();
    let order = |a: &RowObj, b: &RowObj| a.lo.total_cmp(&b.lo).then(a.hi.total_cmp(&b.hi)).then(a.id.cmp(&b.id));
    before.sort_by(order);
    after.sort_by(order);
    let mut gaps = Vec::new();
    for side in [&before, &after] {
        for (i, a) in side.iter().enumerate() {
            let j = side.partition_point(|b| b.lo < a.hi - co.tol(b.lo, a.hi));
            let Some(b) = side.get(j.max(i + 1)) else { continue };
            let g = b.lo - a.hi;
            if g > co.tol(b.lo, a.hi) {
                gaps.push((g, *a, *b));
            }
        }
    }
    Layout { before, after, gaps }
}

/// The distinct values of a list of gaps, within the tolerance, smallest first.
fn distinct(cx: &Ctx<'_>, gaps: &[(f64, RowObj, RowObj)]) -> Vec<f64> {
    let mut v: Vec<f64> = gaps.iter().map(|g| g.0).collect();
    v.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::new();
    for g in v {
        if out.last().is_none_or(|l| !cx.co.same(*l, g)) {
            out.push(g);
        }
    }
    out
}

/// The positions of the subject's low side that repeat a gap: `(start, neighbour)` pairs.
fn starts(cx: &Ctx<'_>, row: &Row, lay: &Layout) -> Vec<(f64, RowObj)> {
    let len = row.hi - row.lo;
    let b = lay.before.iter().max_by(|x, y| x.hi.total_cmp(&y.hi).then(y.id.cmp(&x.id)));
    let a = lay.after.iter().min_by(|x, y| x.lo.total_cmp(&y.lo).then(x.id.cmp(&y.id)));
    let mut out = Vec::new();
    if let (Some(b), Some(a)) = (b, a)
        && a.lo - b.hi > len
    {
        out.push((0.5 * (b.hi + a.lo - len), *b));
    }
    let values = match cx.s.spacing_gaps {
        SpacingGaps::Row => distinct(cx, &lay.gaps),
        SpacingGaps::Neighbour => Vec::new(),
    };
    for g in values {
        out.extend(b.map(|b| (b.hi + g, *b)));
        out.extend(a.map(|a| (a.lo - g - len, *a)));
    }
    if cx.s.spacing_gaps == SpacingGaps::Neighbour {
        if let Some(b) = b {
            // The neighbour's gap to the object beyond it, repeated after it.
            if let Some((g, _, _)) = lay.gaps.iter().find(|(_, _, r)| r.id == b.id && r.lo == b.lo) {
                out.push((b.hi + g, *b));
            }
        }
        if let Some(a) = a
            && let Some((g, _, _)) = lay.gaps.iter().find(|(_, l, _)| l.id == a.id && l.lo == a.lo)
        {
            out.push((a.lo - g - len, *a));
        }
    }
    out
}

/// The equal-gap candidates of a box along one probe's direction.
pub(crate) fn cands(cx: &mut Ctx<'_>, frame: &TurnedBox, probe: &Probe, free: crate::align::Free) -> Vec<Cand> {
    let rule = cx.rule(Kind::Spacing);
    if !rule.on || probe.sides.is_empty() {
        return Vec::new();
    }
    let own = probe.sides.iter().any(|s| s.own == OwnOrBounds::Own);
    let row = gather_row(cx, frame, probe.normal, own);
    let lay = layout(cx, &row);
    let mut out = Vec::new();
    for (start, nb) in starts(cx, &row, &lay) {
        let d = start - row.lo;
        let travel = match free {
            crate::align::Free::Normal => Some(d),
            crate::align::Free::Along(e) => {
                let ne = probe.normal.dot(e);
                (ne.abs() > 1.0e-12).then(|| d / ne)
            }
        };
        let Some(travel) = travel.filter(|t| t.is_finite() && cx.pulls(Kind::Spacing, t.abs())) else { continue };
        out.push(Cand {
            kind: Kind::Spacing,
            id: Some(nb.id),
            prio: rule.priority,
            reach: cx.reach(Kind::Spacing),
            side: Side::Min,
            own: if own { OwnOrBounds::Own } else { OwnOrBounds::Bounds },
            angle: probe.angle,
            c: start,
            s: row.lo,
            d: travel,
            t: travel.abs(),
            pt: 0,
        });
    }
    out
}

/// The distances, strictly ahead along the page axis `n` (or against it), to the places where the box would repeat a gap in its row, with the
/// neighbour each is measured from: for a nudge.
pub(crate) fn ahead(cx: &mut Ctx<'_>, frame: &TurnedBox, n: Vec2, forward: bool) -> Vec<(f64, RowObj)> {
    if !cx.rule(Kind::Spacing).on {
        return Vec::new();
    }
    let row = gather_row(cx, frame, n, false);
    let lay = layout(cx, &row);
    let co = cx.co;
    starts(cx, &row, &lay)
        .into_iter()
        .filter_map(|(start, nb)| {
            let d = if forward { start - row.lo } else { row.lo - start };
            (d.is_finite() && d > co.tol(start, row.lo)).then_some((d, nb))
        })
        .collect()
}

/// Whether the box at its final place still repeats a gap in the row it is in then: the row is gathered again there, since moving along the
/// other direction can change which objects overlap it across.
pub(crate) fn still_holds(cx: &mut Ctx<'_>, frame: &TurnedBox, probe: &Probe) -> bool {
    let own = probe.sides.iter().any(|s| s.own == OwnOrBounds::Own);
    let row = gather_row(cx, frame, probe.normal, own);
    let lay = layout(cx, &row);
    starts(cx, &row, &lay).iter().any(|(start, _)| cx.co.same(*start, row.lo))
}

/// One gap of the final row: its value, the two objects (None for the subject), and where it runs.
struct GapRec {
    value: f64,
    /// The objects at its two ends (`None`: the subject).
    left: Option<Id>,
    right: Option<Id>,
    from: f64,
    to: f64,
    c0: f64,
    c1: f64,
    subject: bool,
}

/// The marks of the gaps: with the subject at its final position, every gap that equals one of the subject's, with chains.
pub(crate) fn gap_marks(cx: &mut Ctx<'_>, sol: &Solution, frame: &TurnedBox, probe: &Probe) -> Vec<Mark> {
    if !cx.rule(Kind::Spacing).on {
        return Vec::new();
    }
    let own = probe.sides.iter().any(|s| s.own == OwnOrBounds::Own);
    let row = gather_row(cx, frame, probe.normal, own);
    let lay = layout(cx, &row);
    let b = lay.before.iter().max_by(|x, y| x.hi.total_cmp(&y.hi).then(y.id.cmp(&x.id)));
    let a = lay.after.iter().min_by(|x, y| x.lo.total_cmp(&y.lo).then(x.id.cmp(&y.id)));
    let co = cx.co;
    let mut recs: Vec<GapRec> = lay
        .gaps
        .iter()
        .map(|(g, l, r)| GapRec {
            value: *g,
            left: Some(l.id),
            right: Some(r.id),
            from: l.hi,
            to: r.lo,
            c0: l.c0.max(r.c0),
            c1: l.c1.min(r.c1),
            subject: false,
        })
        .collect();
    let mut mine = Vec::new();
    if let Some(b) = b.filter(|b| row.lo - b.hi > co.tol(row.lo, b.hi)) {
        mine.push(GapRec {
            value: row.lo - b.hi,
            left: Some(b.id),
            right: None,
            from: b.hi,
            to: row.lo,
            c0: b.c0.max(row.c0),
            c1: b.c1.min(row.c1),
            subject: true,
        });
    }
    if let Some(a) = a.filter(|a| a.lo - row.hi > co.tol(a.lo, row.hi)) {
        mine.push(GapRec {
            value: a.lo - row.hi,
            left: None,
            right: Some(a.id),
            from: row.hi,
            to: a.lo,
            c0: a.c0.max(row.c0),
            c1: a.c1.min(row.c1),
            subject: true,
        });
    }
    let equal_to_other =
        |g: &GapRec, all: &[GapRec], others: &[GapRec]| all.iter().chain(others.iter()).any(|o| !std::ptr::eq(o, g) && co.same(o.value, g.value));
    let values: Vec<f64> = mine.iter().filter(|g| equal_to_other(g, &recs, &mine)).map(|g| g.value).collect();
    if values.is_empty() {
        return Vec::new();
    }
    recs.extend(mine);
    recs.retain(|g| values.iter().any(|v| co.same(*v, g.value)));
    recs.sort_by(|x, y| x.from.total_cmp(&y.from).then(x.to.total_cmp(&y.to)));
    let hit = sol.hits.iter().any(|h| matches!(h, HitRec::Line(c) if c.kind == Kind::Spacing));
    let direction = Direction::new(probe.angle);
    let t = Vec2::new(-probe.normal.y, probe.normal.x);
    let mut out = Vec::new();
    let (mut chain, mut last_right): (u16, Option<Option<Id>>) = (0, None);
    for g in &recs {
        if last_right != Some(g.left) {
            chain = chain.saturating_add(1);
        }
        last_right = Some(g.right);
        let across = 0.5 * (g.c0 + g.c1);
        let at = |along: f64| cx.o + probe.normal * along + t * across;
        let Some(direction) = direction else { continue };
        if g.c0.partial_cmp(&g.c1) != Some(std::cmp::Ordering::Less) {
            continue;
        }
        out.push(Mark::Gap { direction, segment: Line::new(at(g.from), at(g.to)), value: g.value, chain, subject: g.subject, hit });
    }
    out
}

/// Charges the marks of gaps against the marks cap, returning those that fit.
pub(crate) fn within_cap(cx: &mut Ctx<'_>, marks: Vec<Mark>, already: usize) -> Vec<Mark> {
    let cap = u64::from(cx.budget().marks);
    let mut out = Vec::new();
    for m in marks {
        let mut n = (already + out.len()) as u64;
        if !Work::charge(&mut n, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::MARKS) {
            break;
        }
        out.push(m);
    }
    out
}
