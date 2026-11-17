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
use crate::settings::{SpacingGaps, SpacingRow};
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
pub(crate) fn outline_extents(r: &kurbo::Rect, n: Vec2, origin: Point) -> ((f64, f64), (f64, f64)) {
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
    // A page row reads the box round the subject's outline when the app gave one, as it does for the objects; an own row reads its own sides.
    let ((lo, hi), (c0, c1)) = match if own { None } else { crate::moves::subject_bounds(cx, subject) } {
        Some(r) => outline_extents(&r, n, cx.o),
        None => (extent(subject, n, cx.o), extent(subject, t, cx.o)),
    };
    let r = cx.s.spacing_reach;
    let reach = (r.times * (hi - lo)).max(r.at_least);
    // A linked row follows a staircase out across the subject's strip, so its window is as wide across as it is along.
    let (w0, w1) = if cx.s.spacing_row == SpacingRow::Linked { (c0 - reach, c1 + reach) } else { (c0, c1) };
    let corners = [(lo - reach, w0), (lo - reach, w1), (hi + reach, w0), (hi + reach, w1)].map(|(a, c)| n * a + t * c);
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
        .filter(|(_, o)| !own || crate::angle::wrap(o.frame.turn() - subject.turn(), quarter).abs() <= cx.co.angle)
        .map(|(_, o)| {
            // A page row reads the box round an object's outline when there is one; an own row reads its own sides.
            let page = if own { None } else { o.bounds };
            let ((l, h), (a, b)) = match page {
                Some(r) => outline_extents(&r, n, cx.o),
                None => (extent(&o.frame, n, cx.o), extent(&o.frame, t, cx.o)),
            };
            RowObj { id: o.id, lo: l, hi: h, c0: a, c1: b }
        })
        // The window is the box round the strip, which holds more than the strip when the row runs across the page: the strip is what the reach is.
        .filter(|o| {
            let co = &cx.co;
            o.hi >= lo - reach - co.tol(o.hi, lo - reach)
                && o.lo <= hi + reach + co.tol(o.lo, hi + reach)
                && o.c1 >= w0 - co.tol(o.c1, w0)
                && o.c0 <= w1 + co.tol(o.c0, w1)
        })
        .collect::<Vec<RowObj>>();
    let objs = match cx.s.spacing_row {
        // An object that only touches the subject's band across (a snap to its edge puts it there) is not in the row, whatever the last bits say.
        SpacingRow::Band => objs.into_iter().filter(|o| overlaps_across(&cx.co, (c0, c1), o)).collect(),
        SpacingRow::Linked => linked(cx, objs, (lo, hi), (c0, c1)),
        SpacingRow::Beyond => beyond(cx, objs, (lo, hi), (c0, c1)),
    };
    Row { objs, lo, hi, c0, c1 }
}

/// Whether an object overlaps the band `a` across the row by more than the same-place rule can tell from touching.
fn overlaps_across(co: &crate::frame::Coincide, a: (f64, f64), o: &RowObj) -> bool {
    o.c0 < a.1 - co.tol(o.c0, a.1) && a.0 + co.tol(a.0, o.c1) < o.c1
}

/// A beyond row: the neighbour on each side, and the nearest object wholly beyond it, and nothing else. The neighbour is the nearest object wholly
/// before (after) the subject that overlaps the subject's band across; what is beyond it is the nearest object wholly before (after) the neighbour
/// that overlaps the neighbour's band across. All are taken from the objects that meet the strip as wide across as the subject.
fn beyond(cx: &Ctx<'_>, objs: Vec<RowObj>, (lo, hi): (f64, f64), across: (f64, f64)) -> Vec<RowObj> {
    let co = cx.co;
    // Nearest first, a tie to the lower id: the order `starts` reads a neighbour in.
    let nearer_before = |x: &&RowObj, y: &&RowObj| x.hi.total_cmp(&y.hi).then(y.id.cmp(&x.id));
    let nearer_after = |x: &&RowObj, y: &&RowObj| x.lo.total_cmp(&y.lo).then(x.id.cmp(&y.id));
    let before = objs.iter().filter(|o| o.hi <= lo + co.tol(o.hi, lo) && overlaps_across(&co, across, o)).max_by(nearer_before).copied();
    let after = objs.iter().filter(|o| o.lo >= hi - co.tol(o.lo, hi) && overlaps_across(&co, across, o)).min_by(nearer_after).copied();
    let mut out = Vec::new();
    if let Some(n) = before {
        out.push(n);
        let next = objs
            .iter()
            .filter(|o| o.id != n.id && overlaps_across(&co, (n.c0, n.c1), o) && o.hi <= n.lo + co.tol(o.hi, n.lo))
            .max_by(nearer_before)
            .copied();
        out.extend(next);
    }
    if let Some(n) = after {
        out.push(n);
        let next = objs
            .iter()
            .filter(|o| o.id != n.id && overlaps_across(&co, (n.c0, n.c1), o) && o.lo >= n.hi - co.tol(o.lo, n.hi))
            .min_by(nearer_after)
            .copied();
        out.extend(next);
    }
    out
}

/// A linked row: from each neighbour outward, the next object that overlaps the previous one across, so a staircase of objects is a row.
fn linked(cx: &Ctx<'_>, objs: Vec<RowObj>, (lo, hi): (f64, f64), across: (f64, f64)) -> Vec<RowObj> {
    let co = cx.co;
    let (mut before, mut after): (Vec<RowObj>, Vec<RowObj>) = (Vec::new(), Vec::new());
    for o in objs {
        if o.hi <= lo + co.tol(o.hi, lo) {
            before.push(o);
        } else if o.lo >= hi - co.tol(o.lo, hi) {
            after.push(o);
        }
    }
    // Nearest the subject first on each side.
    before.sort_by(|a, b| b.hi.total_cmp(&a.hi).then(a.id.cmp(&b.id)));
    after.sort_by(|a, b| a.lo.total_cmp(&b.lo).then(a.id.cmp(&b.id)));
    let overlaps = |a: (f64, f64), o: &RowObj| overlaps_across(&co, a, o);
    let mut out = Vec::new();
    for side in [before, after] {
        let mut current = across;
        for o in side {
            if overlaps(current, &o) {
                current = (o.c0, o.c1);
                out.push(o);
            }
        }
    }
    out
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
        let mut ends: Vec<(f64, Id)> = side.iter().map(|o| (o.hi, o.id)).collect();
        ends.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        for (i, a) in side.iter().enumerate() {
            let j = side.partition_point(|b| b.lo < a.hi - co.tol(b.lo, a.hi));
            let Some(b) = side.get(j.max(i + 1)) else { continue };
            let g = b.lo - a.hi;
            // A pair is a gap when each is the other's neighbour: `b` the first to start after `a` ends, and `a` the last to end before `b` starts.
            // With boxes that overlap there are more of the one than of the other, and a row read from the other end must give the same gaps.
            let k = ends.partition_point(|h| h.0 <= b.lo + co.tol(h.0, b.lo));
            // A box of no width ends where it starts, and is not its own neighbour.
            let last_end = ends.get(..k).and_then(|e| e.iter().rev().find(|h| h.1 != b.id)).map_or(a.hi, |h| h.0);
            if g > co.tol(b.lo, a.hi) && a.hi >= last_end - co.tol(a.hi, last_end) {
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
        let (travel, across) = match free {
            crate::align::Free::Normal => (d, 1.0),
            crate::align::Free::Along(e) => (d / probe.normal.dot(e), probe.normal.dot(e)),
        };
        if !(travel.is_finite() && cx.pulls(Kind::Spacing, travel.abs())) || cx.too_oblique(Kind::Spacing, 1.0, across) {
            continue;
        }
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
    // Under `HitsOnly` only the gap the snap repeated is marked: the subject's gap beside the neighbour it was measured from.
    let snapped = sol.hits.iter().find_map(|h| match h {
        HitRec::Line(c) if c.kind == Kind::Spacing => Some(c.id),
        _ => None,
    });
    let hits_only = cx.s.marks == crate::settings::MarkRule::HitsOnly;
    let values: Vec<f64> = match (hits_only, snapped) {
        (true, None) => Vec::new(),
        (true, Some(id)) => mine.iter().filter(|g| id.is_some() && (g.left == id || g.right == id)).map(|g| g.value).collect(),
        (false, _) => mine.iter().filter(|g| equal_to_other(g, &recs, &mine)).map(|g| g.value).collect(),
    };
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
    let (mut chain, mut last_right, mut last_value): (u16, Option<Option<Id>>, f64) = (0, None, f64::NAN);
    for g in &recs {
        // A chain is a run of gaps that follow one another and have one value: a different value starts a new one.
        if last_right != Some(g.left) || !co.same(last_value, g.value) {
            chain = chain.saturating_add(1);
        }
        last_right = Some(g.right);
        last_value = g.value;
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
