//! What to draw, derived from the final geometry. The winning hits are not carried forward: after the geometry is known, each side of it is
//! looked up again among the lines, so a mark is there if and only if the geometry lies on its line.

use kurbo::{Line, Vec2};

use crate::align::lines_find;
use crate::budget::{Cuts, Work};
use crate::ctx::{Ctx, Probe};
use crate::frame::{Direction, Id};
use crate::kind::Kind;
use crate::moves::{box_probes, point_probes};
use crate::query::Subject;
use crate::result::Mark;
use crate::settings::MarkRule;
use crate::solve::{HitRec, Solution};

/// The lines one probe's sides lie on at one place.
struct Group {
    c: f64,
    kinds: Vec<Kind>,
    first: Option<Id>,
    count: u32,
    lo: f64,
    hi: f64,
    hit: bool,
}

/// The probes of the final geometry of the subject.
fn final_probes(cx: &Ctx<'_>, sol: &Solution) -> Vec<Probe> {
    match (cx.q.subject, sol.point, sol.frame) {
        (Subject::Point { .. }, Some(p), _) => point_probes(cx, p),
        (Subject::Box { u, v, .. }, _, Some(f)) => box_probes(cx, &f, u, v),
        _ => Vec::new(),
    }
}

fn is_hit(sol: &Solution, probe: &Probe, c: f64, cx: &Ctx<'_>) -> bool {
    sol.hits.iter().any(|h| match h {
        HitRec::Line(cand) => (cand.angle - probe.angle).abs() <= cx.co.angle && cx.co.same(cand.c, c),
    })
}

/// The groups of lines one probe's sides lie on.
fn groups_of(cx: &mut Ctx<'_>, sol: &Solution, probe: &Probe) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for ps in &probe.sides {
        for f in lines_find(cx, probe.angle, ps.offset, ps.side, true) {
            let at = match groups.iter().position(|g| cx.co.same(g.c, f.c)) {
                Some(i) => i,
                None => {
                    let hit = is_hit(sol, probe, f.c, cx);
                    groups.push(Group { c: f.c, kinds: Vec::new(), first: None, count: 0, lo: f64::INFINITY, hi: f64::NEG_INFINITY, hit });
                    groups.len() - 1
                }
            };
            let Some(g) = groups.get_mut(at) else { continue };
            if !g.kinds.contains(&f.kind) {
                g.kinds.push(f.kind);
            }
            g.count = g.count.saturating_add(f.count);
            g.first = match (g.first, f.id) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            g.lo = g.lo.min(f.lo).min(ps.lo);
            g.hi = g.hi.max(f.hi).max(ps.hi);
        }
    }
    groups.sort_by(|a, b| a.c.total_cmp(&b.c));
    for g in &mut groups {
        g.kinds.sort();
    }
    groups
}

fn mark_of(cx: &Ctx<'_>, probe: &Probe, g: &Group) -> Option<Mark> {
    let n = probe.normal;
    let t = Vec2::new(-n.y, n.x);
    let at = |s: f64| cx.o + n * g.c + t * s;
    let (a, b) = (at(g.lo), at(g.hi));
    let direction = Direction::new(probe.angle)?;
    let offset = direction.normal().dot(a.to_vec2());
    (a.is_finite() && b.is_finite() && offset.is_finite()).then(|| Mark::Line {
        direction,
        offset,
        segment: Line::new(a, b),
        kinds: g.kinds.clone(),
        first: g.first,
        count: g.count,
        hit: g.hit,
    })
}

/// The marks of the final geometry.
pub(crate) fn derive(cx: &mut Ctx<'_>, sol: &Solution) -> Vec<Mark> {
    let mut out = Vec::new();
    let cap = u64::from(cx.budget().marks);
    for probe in final_probes(cx, sol) {
        for g in groups_of(cx, sol, &probe) {
            if cx.s.marks == MarkRule::HitsOnly && !g.hit {
                continue;
            }
            let Some(m) = mark_of(cx, &probe, &g) else { continue };
            let mut n = out.len() as u64;
            if !Work::charge(&mut n, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::MARKS) {
                return out;
            }
            out.push(m);
        }
    }
    out
}
