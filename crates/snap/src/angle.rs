// Rotation match follows DesignCraft's `match_rotation`: storytold/designcraft at 47ef1ec411d53e98a31c576d80a1d0e20ef5eb04, crates/tools/src/snap.rs.
// Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors. MIT OR Apache-2.0.
// Changed: objects' angles are in a sorted array searched with wrap-around, the period is a setting, and a returned angle is always a finite number.

//! Rotation match and angle steps, for a turn by a handle.
//!
//! * **Rotation match.** Other objects' angles, folded into one period, sit in a sorted array searched by bisection with wrap-around. A candidate
//!   pulls when its angular distance is at most `atan(reach / radius)`, so the pull is the same distance on screen at the pointer, whatever the
//!   radius. With a radius not above the reach, nothing matches: the pointer is too near the pivot to aim. (The tolerance and the fold of whole
//!   turns follow DesignCraft's `match_rotation`; see the README.)
//! * **Angle steps.** A soft lattice `(step, zero)` of kind `AngleStep` competes with a match under the settings; a hard lattice
//!   (`Constraint::Angles`) allows only angles on it, and a match off it is not taken.

use crate::budget::Refusal;
use crate::ctx::Ctx;
use crate::frame::{Coincide, Id};
use crate::kind::{Kind, Reach};
use crate::query::Constraint;
use crate::settings::Choose;
use crate::solve::{HitRec, Solution};

/// What an angle snapped to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AngleHit {
    pub kind: Kind,
    pub id: Option<Id>,
    /// What to add to the angle the pointer gives, and the angle it makes.
    pub delta: f64,
    pub to: f64,
    pub prio: u8,
}

/// `x` turned into `[-p/2, p/2)` by whole periods.
pub(crate) fn wrap(x: f64, p: f64) -> f64 {
    (x + 0.5 * p).rem_euclid(p) - 0.5 * p
}

/// The largest angular distance that a kind of the given reach pulls from, at this radius; `None` when the pointer is too near the pivot to aim.
fn limit(cx: &Ctx<'_>, kind: Kind, radius: f64) -> Option<f64> {
    let rule = cx.rule(kind);
    if !rule.on {
        return None;
    }
    match rule.reach {
        Reach::Any => Some(cx.s.angle_period / 2.0),
        r => {
            let reach = r.document(cx.zoom);
            (radius > reach).then(|| (reach / radius).atan())
        }
    }
}

/// How far an angle may be from a lattice and be on it: the tolerance of two directions, or, at an angle large enough that the spacing of the
/// numbers is wider, the relative one. One rule, read by the solver, the marks and the check.
pub(crate) fn tolerance(co: &Coincide, a: f64, zero: f64) -> f64 {
    co.angle.max(co.rel * a.abs().max(zero.abs()))
}

/// Whether the angle is on the lattice `(step, zero)` within `tol`. An angle or a lattice that is not a usable number is on none.
pub(crate) fn on_lattice(a: f64, step: f64, zero: f64, tol: f64) -> bool {
    if !(a.is_finite() && step.is_finite() && step > 0.0 && zero.is_finite()) {
        return false;
    }
    wrap(a - zero, step).abs() <= tol
}

/// The lattice angle nearest `raw`, if it is a finite number on the lattice: a huge angle or a tiny step has none.
fn lattice_hit(co: &Coincide, raw: f64, step: (f64, f64), kind: Kind, prio: u8) -> Option<AngleHit> {
    let k = ((raw - step.1) / step.0).round();
    let to = step.1 + k * step.0;
    let delta = to - raw;
    let usable = k.is_finite() && to.is_finite() && delta.is_finite() && on_lattice(to, step.0, step.1, tolerance(co, to, step.1));
    usable.then_some(AngleHit { kind, id: None, delta, to, prio })
}

/// The match candidates near the raw angle: the nearest below and above it, and across the wrap.
fn matches(cx: &mut Ctx<'_>, raw: f64, lim: f64, hard: Option<(f64, f64)>) -> Option<AngleHit> {
    let (p, co) = (cx.s.angle_period, cx.co);
    let t = cx.t;
    let Some(arr) = t.angles(p, &mut cx.work.made) else {
        // The angles were sorted for another period: no match is tried, and the work says so.
        cx.work.cut(crate::budget::Cuts::ANGLE_PERIOD);
        return None;
    };
    cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(crate::index::log_steps(arr.len()));
    let raw_f = raw.rem_euclid(p);
    let i = arr.partition_point(|l| l.length < raw_f);
    let near = [i.checked_sub(1), Some(i), Some(0), arr.len().checked_sub(1)].into_iter().flatten().filter_map(|j| arr.get(j));
    let best = near
        .map(|l| (wrap(l.length - raw_f, p), l.id, l.length + p * ((raw - l.length) / p).round()))
        .filter(|(d, _, to)| d.is_finite() && to.is_finite() && d.abs() <= lim)
        .filter(|(_, _, to)| hard.is_none_or(|(st, z)| on_lattice(*to, st, z, tolerance(&co, *to, z))))
        .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()).then(a.1.cmp(&b.1)))?;
    Some(AngleHit { kind: Kind::Angle, id: Some(best.1), delta: best.0, to: best.2, prio: cx.rule(Kind::Angle).priority })
}

/// Turns by a handle.
pub(crate) fn solve_rotate(cx: &mut Ctx<'_>, raw: f64, radius: f64, step: Option<(f64, f64)>) -> Solution {
    let hard = match cx.q.constraint {
        Constraint::Angles { step, zero } => Some((step, zero)),
        _ => None,
    };
    let mut cands: Vec<AngleHit> = Vec::new();
    if let Some(lim) = limit(cx, Kind::Angle, radius) {
        cands.extend(matches(cx, raw, lim, hard));
    }
    let co = cx.co;
    if let Some(h) = hard {
        // A hard lattice that no finite angle lies on (a huge angle, a tiny step) cannot be met: no angle, and the reason.
        match lattice_hit(&co, raw, h, Kind::AngleStep, cx.rule(Kind::AngleStep).priority) {
            Some(l) => cands.push(l),
            None => {
                cx.work.refused = Some(Refusal::NotFinite);
                return Solution::new();
            }
        }
    } else if let Some(h) = step.and_then(|st| lattice_hit(&co, raw, st, Kind::AngleStep, cx.rule(Kind::AngleStep).priority))
        && limit(cx, Kind::AngleStep, radius).is_some_and(|l| h.delta.abs() <= l)
    {
        cands.push(h);
    }
    let order = |a: &&AngleHit, b: &&AngleHit| {
        let dist = |x: &AngleHit, y: &AngleHit| {
            if co.same(x.delta.abs(), y.delta.abs()) { std::cmp::Ordering::Equal } else { x.delta.abs().total_cmp(&y.delta.abs()) }
        };
        match cx.s.choose {
            Choose::Nearest => dist(a, b).then(a.prio.cmp(&b.prio)),
            Choose::Ranked => a.prio.cmp(&b.prio).then(dist(a, b)),
        }
        .then(a.id.cmp(&b.id))
        .then(a.kind.cmp(&b.kind))
    };
    let best = cands.iter().min_by(order).copied();
    let mut sol = Solution::new();
    sol.angle = Some(best.map_or(raw, |b| b.to)).filter(|a| a.is_finite());
    if let Some(b) = best {
        sol.hits.push(HitRec::Angle(b));
    }
    sol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_wrap_into_half_a_period_either_way() {
        let p = std::f64::consts::TAU;
        assert!((wrap(0.1, p) - 0.1).abs() < 1e-15);
        assert!((wrap(p - 0.1, p) + 0.1).abs() < 1e-12);
        assert!((wrap(3.0 * p + 0.2, p) - 0.2).abs() < 1e-12);
        assert!(on_lattice(0.5 + 3.0 * 0.25, 0.25, 0.0, 1e-9) && !on_lattice(0.51, 0.25, 0.0, 1e-9));
        assert!(on_lattice(-0.25, 0.25, 0.0, 1e-9));
        assert!(!on_lattice(f64::NAN, 0.25, 0.0, 1e-9) && !on_lattice(1.0, 0.0, 0.0, 1e-9) && !on_lattice(f64::INFINITY, 0.25, 0.0, 1e9));
    }
}
