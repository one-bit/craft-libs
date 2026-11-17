//! What one query works from, the sides a subject offers in each direction, and how candidates are chosen.

use std::cmp::Ordering;

use kurbo::{Point, Rect, Vec2};

use crate::budget::{Budget, Work};
use crate::frame::{Coincide, Id, OwnOrBounds, Side};
use crate::index::{canon, id_before};
use crate::kind::{Kind, KindRule};
use crate::query::Query;
use crate::settings::{Choose, Settings};
use crate::targets::{SideLine, Targets};

/// Everything a query reads, and the work it has done so far.
pub(crate) struct Ctx<'a> {
    pub t: &'a Targets,
    pub q: &'a Query<'a>,
    pub s: &'a Settings,
    pub co: Coincide,
    pub zoom: f64,
    pub o: Point,
    pub work: Work,
    /// The objects near the subject, for lines within a reach across: made on first use.
    pub near: Option<Vec<u32>>,
    /// The box round the subject, relative to the origin.
    pub region: Rect,
}

impl<'a> Ctx<'a> {
    pub fn new(t: &'a Targets, q: &'a Query<'a>) -> Ctx<'a> {
        Ctx { t, q, s: q.settings, co: q.settings.coincide, zoom: q.zoom, o: t.origin, work: Work::default(), near: None, region: Rect::ZERO }
    }

    pub fn budget(&self) -> &Budget {
        &self.q.budget
    }

    pub fn rule(&self, kind: Kind) -> KindRule {
        self.s.kinds.rule(kind)
    }

    /// The reach of a kind in document units; 0 for a kind that is off.
    pub fn reach(&self, kind: Kind) -> f64 {
        let r = self.rule(kind);
        if r.on { r.reach.document(self.zoom) } else { 0.0 }
    }

    /// Whether a target of this kind at document distance `d` pulls.
    pub fn pulls(&self, kind: Kind, d: f64) -> bool {
        let r = self.rule(kind);
        r.on && r.reach.within(d, self.zoom)
    }

    /// A point relative to the origin.
    pub fn rel(&self, p: Point) -> Vec2 {
        p - self.o
    }
}

/// One side of the subject along one direction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbeSide {
    pub side: Side,
    pub own: OwnOrBounds,
    /// Where it is along the probe's normal, relative to the origin.
    pub offset: f64,
    /// Its extent along the probe's tangent.
    pub lo: f64,
    pub hi: f64,
}

/// The sides of the subject in one direction.
#[derive(Clone, Debug)]
pub(crate) struct Probe {
    /// The canonical angle of the normal.
    pub angle: f64,
    pub normal: Vec2,
    pub sides: Vec<ProbeSide>,
}

/// Groups side lines into probes, one for each direction (and family), keeping those `keep` allows. Each is brought to its canonical
/// direction, which changes the sign of its offsets where the normal turns round.
pub(crate) fn probes(lines: &[SideLine], co: &Coincide, keep: &dyn Fn(&SideLine) -> bool) -> Vec<Probe> {
    let mut out: Vec<(Probe, OwnOrBounds)> = Vec::new();
    for l in lines.iter().filter(|l| keep(l)) {
        let (angle, flipped) = canon(l.angle, co.angle);
        let (offset, lo, hi) = if flipped { (-l.offset, -l.hi, -l.lo) } else { (l.offset, l.lo, l.hi) };
        let side = ProbeSide { side: l.side, own: l.own, offset, lo, hi };
        match out.iter_mut().find(|(p, own)| *own == l.own && (p.angle - angle).abs() <= co.angle) {
            Some((p, _)) => p.sides.push(side),
            None => out.push((Probe { angle, normal: crate::frame::unit(angle), sides: vec![side] }, l.own)),
        }
    }
    out.into_iter().map(|(p, _)| p).collect()
}

/// A pull of one side of the subject toward a line, or toward a place across a constrained drag.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cand {
    pub kind: Kind,
    pub id: Option<Id>,
    pub prio: u8,
    /// How far the kind pulls, in document units.
    pub reach: f64,
    pub side: Side,
    pub own: OwnOrBounds,
    /// The direction's canonical angle, and the line's offset there, relative to the origin.
    pub angle: f64,
    pub c: f64,
    /// The move along the normal that puts the side on the line.
    pub d: f64,
    /// How far the subject travels for it: its distance for the settings.
    pub t: f64,
    pub count: u32,
}

/// The order of two candidates for a total order: id, then side, then kind, then offset.
fn last_resort(a: &Cand, b: &Cand) -> Ordering {
    a.id.cmp(&b.id).then(a.side.cmp(&b.side)).then(a.kind.cmp(&b.kind)).then(a.c.total_cmp(&b.c)).then(a.angle.total_cmp(&b.angle))
}

fn by_id(a: &Cand, b: &Cand) -> Ordering {
    if id_before(a.id, b.id) {
        Ordering::Less
    } else if id_before(b.id, a.id) {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// Whether `a` is a better choice than `b` under `choose`: `Nearest` by distance (priority between distances equal within `co`), `Ranked` by
/// priority (distance within one priority). Then the lower id, the earlier side and a total order, so that the answer does not depend on the
/// order the candidates came in.
pub(crate) fn better(a: &Cand, b: &Cand, choose: Choose, co: &Coincide) -> bool {
    order(a, b, choose, co) == Ordering::Less
}

fn order(a: &Cand, b: &Cand, choose: Choose, co: &Coincide) -> Ordering {
    let by_dist = |a: &Cand, b: &Cand| if co.same(a.t, b.t) { Ordering::Equal } else { a.t.total_cmp(&b.t) };
    let first = match choose {
        Choose::Nearest => by_dist(a, b).then(a.prio.cmp(&b.prio)),
        Choose::Ranked => a.prio.cmp(&b.prio).then(by_dist(a, b)),
    };
    first.then(by_id(a, b)).then(last_resort(a, b)).then(a.t.total_cmp(&b.t))
}

/// The index of the best candidate under the settings. With `Nearest`, the distances within the tolerance of the smallest are ties and go
/// to the priority, which makes the choice the same whatever order the candidates are in.
pub(crate) fn best_of(cands: &[Cand], choose: Choose, co: &Coincide) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, c) in cands.iter().enumerate() {
        match best.and_then(|b| cands.get(b)) {
            Some(b) if !better(c, b, choose, co) => {}
            _ => best = Some(i),
        }
    }
    best
}

/// The best `keep` candidates, best first.
pub(crate) fn top(mut cands: Vec<Cand>, keep: usize, choose: Choose, co: &Coincide) -> Vec<Cand> {
    let mut out = Vec::new();
    while out.len() < keep {
        let Some(i) = best_of(&cands, choose, co) else { break };
        out.push(cands.swap_remove(i));
    }
    out
}

/// The bounds of some places grown by `d`.
pub(crate) fn grown(r: Rect, d: f64) -> Rect {
    Rect::new(r.x0 - d, r.y0 - d, r.x1 + d, r.y1 + d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(t: f64, prio: u8, id: u64) -> Cand {
        Cand { kind: Kind::Guide, id: Some(Id(id)), prio, reach: 1.0, side: Side::Min, own: OwnOrBounds::Bounds, angle: 0.0, c: t, d: t, t, count: 1 }
    }

    #[test]
    fn nearest_decides_by_distance_and_priority_between_equals() {
        let co = Coincide::default();
        let list = [cand(2.0, 0, 1), cand(1.0, 5, 2), cand(1.0 + 1e-12, 3, 3)];
        let i = best_of(&list, Choose::Nearest, &co).unwrap();
        assert_eq!(list[i].id, Some(Id(3)), "the two nearest are equal within the tolerance, so the priority decides");
        let j = best_of(&list, Choose::Ranked, &co).unwrap();
        assert_eq!(list[j].id, Some(Id(1)), "the lowest priority number wins at any distance");
    }

    #[test]
    fn the_choice_does_not_depend_on_the_order() {
        let co = Coincide::default();
        let base: Vec<Cand> = (0..8).map(|i| cand(1.0 + (i % 3) as f64 * 1e-12, (i % 2) as u8, 10 - i)).collect();
        let want = base.iter().map(|c| c.id).collect::<Vec<_>>();
        for choose in [Choose::Nearest, Choose::Ranked] {
            let mut rev = base.clone();
            rev.reverse();
            let a = best_of(&base, choose, &co).and_then(|i| base.get(i)).map(|c| c.id);
            let b = best_of(&rev, choose, &co).and_then(|i| rev.get(i)).map(|c| c.id);
            assert_eq!(a, b, "{choose:?} {want:?}");
        }
        let kept = top(base.clone(), 3, Choose::Nearest, &co);
        assert_eq!(kept.len(), 3);
        assert!(top(Vec::new(), 3, Choose::Nearest, &co).is_empty());
    }

    #[test]
    fn probes_are_grouped_by_direction_and_family_and_turned_to_the_canonical_normal() {
        let co = Coincide::default();
        let mk = |angle: f64, side, own, offset: f64| SideLine { angle, side, own, offset, lo: -1.0, hi: 2.0 };
        let lines = vec![
            mk(0.0, Side::Min, OwnOrBounds::Bounds, 1.0),
            mk(0.0, Side::Max, OwnOrBounds::Bounds, 3.0),
            mk(std::f64::consts::PI, Side::Min, OwnOrBounds::Own, 5.0),
            mk(1.0, Side::Mid, OwnOrBounds::Own, 7.0),
        ];
        let p = probes(&lines, &co, &|_| true);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].sides.len(), 2);
        let turned = &p[1];
        assert!(turned.sides[0].offset == -5.0 && turned.sides[0].lo == -2.0 && turned.sides[0].hi == 1.0, "{turned:?}");
        assert_eq!(probes(&lines, &co, &|l| l.own == OwnOrBounds::Own).len(), 2);
    }
}
