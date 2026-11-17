// Ported from CADCraft's object snaps: storytold/cadcraft at ba3196e3f737f2fed0d4d8ae9c065b20773dec16,
// crates/engine/src/snap.rs and crates/engine/src/snap/curves.rs.
// Copyright (c) 2026 ArtCraft Team and the CADCraft contributors. MIT OR Apache-2.0.
// Changed: moved from that app's own curve types onto `kurbo::BezPath`; every path is read about its own local origin, so a small
// outline far from the origin snaps as it does at the origin; every function is bounded before it works, and says so.

//! Object snaps on [`BezPath`]: the places a pointer or a dragged box can be pulled to on an outline.
//!
//! * **Endpoint:** the start and the end of every segment ([`endpoints`]).
//! * **Midpoint:** the point halfway along a segment's length ([`midpoints`]).
//! * **Nearest:** the closest point of an outline to a place ([`nearest`]).
//! * **Intersection:** where two outlines cross, on the pieces near a probe ([`pieces_near`], [`crossings`]). Curves are flattened first.
//! * **Tangent and perpendicular** from a base point ([`tangents`], [`perpendiculars`]): the samples of the condition, each change of
//!   sign narrowed by bisection, kept when the normalised condition is nearly zero there.
//!
//! Every function is bounded: a path is read to at most [`MAX_SEGMENTS`] segments, a flattened side keeps at most [`MAX_FLAT`] pieces, a
//! result holds at most [`MAX_POINTS`] points, and a curve is charged for the pieces it would flatten into before it is flattened.
//!
//! **Local origin.** A path is read about the centre of the box of its control points, so only its size is bounded and never its
//! distance from the origin. Results are given back in the caller's frame.

use kurbo::{
    BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, ParamCurveDeriv, ParamCurveNearest, PathEl, PathSeg, Point, QuadBez, Rect, Vec2, flatten,
};

/// The most segments of one path any function here reads.
pub const MAX_SEGMENTS: usize = 4_096;

/// The most straight pieces of a flattened path that are kept for each side of a crossing.
pub const MAX_FLAT: usize = 256;

/// The most points a function returns.
pub const MAX_POINTS: usize = 64;

/// The largest diagonal, in document units, of the box of one segment's control points. A segment past it, or with a control point that is
/// not a number, is left out and the answer says so.
pub const MAX_EXTENT: f64 = 1.0e9;

/// The most straight pieces one curved segment may be estimated to make; a segment that would make more is left out before it is flattened.
pub const MAX_SEGMENT_PIECES: usize = 4_096;

/// The most straight pieces, estimated, that one call to [`pieces_near`] may flatten curves into.
pub const MAX_FLATTEN_PIECES: usize = 16_384;

/// How far a flattened curve may leave the curve, in document units, unless the caller says otherwise.
pub const DEFAULT_FLATNESS: f64 = 1.0e-3;

/// Samples of a segment for [`tangents`] and [`perpendiculars`], bisection steps per root, and roots per segment.
const SAMPLES: usize = 64;
const BISECT: usize = 60;
const MAX_ROOTS: usize = 8;

/// The elements of a path that are read: three to a segment, and the move to start.
const MAX_ELEMENTS: usize = MAX_SEGMENTS * 3 + 1;

/// A path shifted so that the centre of the box of its control points is the origin, and that centre.
struct Local {
    path: BezPath,
    centre: Vec2,
    /// The larger side of the box of the control points.
    size: f64,
}

fn shifted(el: PathEl, by: Vec2) -> PathEl {
    let q = |p: Point| p - by;
    match el {
        PathEl::MoveTo(p) => PathEl::MoveTo(q(p)),
        PathEl::LineTo(p) => PathEl::LineTo(q(p)),
        PathEl::QuadTo(a, b) => PathEl::QuadTo(q(a), q(b)),
        PathEl::CurveTo(a, b, c) => PathEl::CurveTo(q(a), q(b), q(c)),
        PathEl::ClosePath => PathEl::ClosePath,
    }
}

fn el_points(el: &PathEl) -> impl Iterator<Item = Point> {
    let (a, n): ([Point; 3], usize) = match *el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => ([p, p, p], 1),
        PathEl::QuadTo(a, b) => ([a, b, b], 2),
        PathEl::CurveTo(a, b, c) => ([a, b, c], 3),
        PathEl::ClosePath => ([Point::ORIGIN; 3], 0),
    };
    a.into_iter().take(n)
}

fn local(path: &BezPath) -> Local {
    let els = || path.elements().iter().take(MAX_ELEMENTS);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in els().flat_map(el_points).filter(|p| p.is_finite()) {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    let (centre, size) = if x0 <= x1 { (Vec2::new(0.5 * (x0 + x1), 0.5 * (y0 + y1)), (x1 - x0).max(y1 - y0)) } else { (Vec2::ZERO, 0.0) };
    let centre = if centre.x.is_finite() && centre.y.is_finite() { centre } else { Vec2::ZERO };
    Local { path: BezPath::from_vec(els().map(|e| shifted(*e, centre)).collect()), centre, size: if size.is_finite() { size } else { 0.0 } }
}

/// The accuracy of arc lengths and nearest points for a path of this size.
fn accuracy(size: f64) -> f64 {
    (size * 1.0e-9).max(1.0e-300)
}

/// The box round an outline, curves read exactly: the app gives it as `Object::bounds` or `Query::bounds`. At most [`MAX_SEGMENTS`] segments are
/// read; a path with nothing in it, or a box that is not finite, gives `None`.
pub fn path_bounds(path: &BezPath) -> Option<Rect> {
    use kurbo::Shape;
    let l = local(path);
    let b = l.path.bounding_box();
    let r = Rect::new(b.x0 + l.centre.x, b.y0 + l.centre.y, b.x1 + l.centre.x, b.y1 + l.centre.y);
    let finite = l.path.elements().iter().flat_map(el_points).all(|p| p.is_finite());
    (finite && !l.path.elements().is_empty() && [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())).then_some(r)
}

/// The start and the end of every segment, a point once where one segment ends and the next begins.
pub fn endpoints(path: &BezPath) -> Vec<Point> {
    let l = local(path);
    let mut out: Vec<Point> = Vec::new();
    let tiny = 1.0e-12 * l.size.max(f64::MIN_POSITIVE);
    for seg in l.path.segments().take(MAX_SEGMENTS) {
        for p in [seg.start(), seg.end()] {
            if p.is_finite() && out.last().is_none_or(|q| q.distance(p) > tiny) {
                out.push(p);
            }
        }
    }
    out.into_iter().map(|p| p + l.centre).collect()
}

/// The point halfway along each segment, by length.
pub fn midpoints(path: &BezPath) -> Vec<Point> {
    let l = local(path);
    let acc = accuracy(l.size);
    let mut out = Vec::new();
    for seg in l.path.segments().take(MAX_SEGMENTS) {
        if !sane(&seg_points(&seg)).1 {
            continue;
        }
        let t = match seg {
            PathSeg::Line(_) => 0.5,
            _ => {
                let length = seg.arclen(acc);
                if !(length.is_finite() && length > 0.0) {
                    continue;
                }
                seg.inv_arclen(length / 2.0, acc)
            }
        };
        let p = seg.eval(t);
        if p.is_finite() {
            out.push(p + l.centre);
        }
    }
    out
}

fn seg_points(seg: &PathSeg) -> Vec<Point> {
    match seg {
        PathSeg::Line(l) => vec![l.p0, l.p1],
        PathSeg::Quad(q) => vec![q.p0, q.p1, q.p2],
        PathSeg::Cubic(c) => vec![c.p0, c.p1, c.p2, c.p3],
    }
}

/// The point of the path nearest `p`, and how far it is; `None` for a path with no segment or a `p` that is not a number.
pub fn nearest(path: &BezPath, p: Point) -> Option<(Point, f64)> {
    if !p.is_finite() {
        return None;
    }
    let l = local(path);
    let q = p - l.centre;
    let acc = accuracy(l.size.max(q.to_vec2().hypot()));
    let mut best: Option<(Point, f64)> = None;
    for seg in l.path.segments().take(MAX_SEGMENTS) {
        if !sane(&seg_points(&seg)).1 {
            continue;
        }
        let n = seg.nearest(q, acc);
        if !n.distance_sq.is_finite() {
            continue;
        }
        if best.is_none_or(|(_, d2)| n.distance_sq < d2) {
            best = Some((seg.eval(n.t), n.distance_sq));
        }
    }
    best.map(|(r, d2)| (r + l.centre, d2.sqrt()))
}

/// The straight pieces of a path that can reach a probe.
#[derive(Clone, Debug, Default)]
pub struct Pieces {
    /// The pieces, at most [`MAX_FLAT`].
    pub lines: Vec<Line>,
    /// Whether something was left out for the work it would cost, and not because it is out of reach: a segment too big or not a number, a
    /// curve that would make more than [`MAX_SEGMENT_PIECES`] pieces, the estimated pieces past the limit, or more pieces than [`MAX_FLAT`].
    pub cut: bool,
    /// The pieces the curves were estimated to make, which is what the call was charged: the work counted before any curve was flattened.
    pub charged: usize,
    /// The pieces the curves did make, before the probe left any out (never more than `charged`).
    pub made: usize,
}

/// An upper bound on the pieces a curve is flattened into, from the diagonal of the box its control points lie in, worked out before the
/// curve is flattened. A cubic or a quadratic differs from its chords by at most an eighth of the square of the step of its parameter times
/// its second derivative, which a control box of diagonal `size` bounds by `12 size`: within `flatness` that is at most
/// `1.23 sqrt(size / flatness)` pieces, and `kurbo`'s flattening makes up to about twice the fewest, so the estimate is
/// `2 sqrt(size / flatness) + 2`. A `size` or `flatness` that is not a number, or a `flatness` not above 0, gives `usize::MAX`.
pub fn estimated_pieces(size: f64, flatness: f64) -> usize {
    if !size.is_finite() || size < 0.0 || !flatness.is_finite() || flatness <= 0.0 {
        return usize::MAX;
    }
    let n = 2.0 * (size / flatness).sqrt() + 2.0;
    if n >= usize::MAX as f64 { usize::MAX } else { n.ceil() as usize }
}

/// The box round some points, and whether every number in them is a finite number and the box is not larger than [`MAX_EXTENT`].
fn sane(points: &[Point]) -> (Rect, bool) {
    let mut r = Rect::new(f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut ok = true;
    for p in points {
        ok &= p.is_finite();
        r = Rect::new(r.x0.min(p.x), r.y0.min(p.y), r.x1.max(p.x), r.y1.max(p.y));
    }
    (r, ok && r.width().hypot(r.height()) <= MAX_EXTENT)
}

/// Whether a box padded by `slack` can meet the probe (no probe: always).
fn reaches(probe: Option<&Rect>, b: Rect, slack: f64) -> bool {
    probe.is_none_or(|p| !(b.x1 + slack < p.x0 || p.x1 < b.x0 - slack || b.y1 + slack < p.y0 || p.y1 < b.y0 - slack))
}

/// What the flattening of one path needs: the probe and the pieces left of the caller's total, in the frame of the local path.
struct Flat<'a> {
    probe: Option<Rect>,
    limit: usize,
    flatness: f64,
    /// Added to every piece to bring it into the caller's frame.
    shift: Vec2,
    out: &'a mut Pieces,
}

/// The straight pieces of a path, curves flattened to within `flatness`; the pieces whose box cannot meet `probe` are left out, and at most
/// [`MAX_FLAT`] are kept. Pieces are in the frame whose origin is `frame_origin`, and so is the probe.
///
/// **The work is bounded before it is done.** A curve is flattened into a number of pieces that grows with its size, so for each curved
/// segment the box of its control points is looked at first, and the segment is left out, without being flattened, when a control point is
/// not a number, when the box is past [`MAX_EXTENT`], when it cannot meet the probe (a curve lies inside its control box), or when
/// [`estimated_pieces`] of the box is over [`MAX_SEGMENT_PIECES`] or would take the estimates over `limit` (the caller's: what is left of its
/// own total, and never more than [`MAX_FLATTEN_PIECES`]). What was left out for its cost is `cut`; what was charged is `charged`.
pub fn pieces_near(path: &BezPath, frame_origin: Point, probe: Option<&Rect>, limit: usize, flatness: f64) -> Pieces {
    let l = local(path);
    let mut out = Pieces::default();
    let shift = l.centre - frame_origin.to_vec2();
    // The probe is brought into the local frame: it is given about `frame_origin`.
    let probe = probe.map(|r| *r - shift);
    let mut at = Flat { probe, limit: limit.min(MAX_FLATTEN_PIECES), flatness, shift, out: &mut out };
    let (mut start, mut cur) = (None::<Point>, None::<Point>);
    for el in l.path.elements().iter().take(MAX_ELEMENTS).copied() {
        match el {
            PathEl::MoveTo(p) => (start, cur) = (Some(p), Some(p)),
            PathEl::LineTo(p) => {
                if let Some(a) = cur {
                    push_line(a, p, &mut at);
                }
                cur = Some(p);
            }
            PathEl::ClosePath => {
                if let (Some(a), Some(s)) = (cur, start) {
                    push_line(a, s, &mut at);
                }
                cur = start;
            }
            PathEl::QuadTo(c, p) => {
                if let Some(a) = cur {
                    flatten_segment(&[a, c, p], el, &mut at);
                }
                cur = Some(p);
            }
            PathEl::CurveTo(c1, c2, p) => {
                if let Some(a) = cur {
                    flatten_segment(&[a, c1, c2, p], el, &mut at);
                }
                cur = Some(p);
            }
        }
    }
    out
}

/// One straight piece, kept if it can reach the probe and there is room.
fn push_line(a: Point, b: Point, at: &mut Flat<'_>) {
    if !(a.is_finite() && b.is_finite()) || a.distance(b) > MAX_EXTENT {
        at.out.cut = true;
        return;
    }
    let bounds = Rect::new(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
    let slack = 1.0e-9 * (1.0 + bounds.width().max(bounds.height()));
    if !reaches(at.probe.as_ref(), bounds, slack) {
        return;
    }
    if at.out.lines.len() >= MAX_FLAT {
        at.out.cut = true;
        return;
    }
    at.out.lines.push(Line::new(a + at.shift, b + at.shift));
}

/// One curved segment with the control points `points` (`el` is its element, from the first point): flattened if its box is sane, can reach the
/// probe and is not too costly (see [`pieces_near`]).
fn flatten_segment(points: &[Point], el: PathEl, at: &mut Flat<'_>) {
    let (bounds, ok) = sane(points);
    if !ok {
        at.out.cut = true;
        return;
    }
    if !reaches(at.probe.as_ref(), bounds, 1.0e-9 * (1.0 + bounds.width().max(bounds.height()))) {
        return;
    }
    let estimate = estimated_pieces(bounds.width().hypot(bounds.height()), at.flatness);
    if estimate > MAX_SEGMENT_PIECES || at.out.charged.saturating_add(estimate) > at.limit {
        at.out.cut = true;
        return;
    }
    at.out.charged += estimate;
    let Some(first) = points.first().copied() else { return };
    let mut last = first;
    let mut lines: Vec<(Point, Point)> = Vec::new();
    flatten([PathEl::MoveTo(first), el], at.flatness, |piece| match piece {
        PathEl::MoveTo(p) => last = p,
        PathEl::LineTo(p) => {
            at.out.made += 1;
            // The estimate is an upper bound: pieces past it are not kept.
            if at.out.made <= at.out.charged && lines.len() < MAX_FLAT {
                lines.push((last, p));
            }
            last = p;
        }
        _ => {}
    });
    for (a, b) in lines {
        push_line(a, b, at);
    }
}

/// Where two finite straight pieces cross, ends included, with a slack of a billionth of their length.
fn crossing(a: &Line, b: &Line) -> Option<Point> {
    let (r, s) = (a.p1 - a.p0, b.p1 - b.p0);
    let denom = r.cross(s);
    let scale = (r.hypot() * s.hypot()).max(f64::MIN_POSITIVE);
    if !denom.is_finite() || denom.abs() < 1.0e-12 * scale {
        return None;
    }
    let q = b.p0 - a.p0;
    let (t, u) = (q.cross(s) / denom, q.cross(r) / denom);
    let slack = 1.0e-9;
    ((-slack..=1.0 + slack).contains(&t) && (-slack..=1.0 + slack).contains(&u)).then(|| a.p0 + r * t)
}

/// The points where the outline `a` crosses the outline `b`, both read in the frame of `frame_origin`. With a `probe` only the pieces that can
/// reach it are looked at, and only crossings inside it are given.
pub fn intersections(a: &BezPath, b: &BezPath, frame_origin: Point, probe: Option<Rect>, flatness: f64) -> Vec<Point> {
    let pa = pieces_near(a, frame_origin, probe.as_ref(), MAX_FLATTEN_PIECES, flatness);
    let pb = pieces_near(b, frame_origin, probe.as_ref(), MAX_FLATTEN_PIECES, flatness);
    crossings(&pa.lines, &pb.lines, probe)
}

/// The points where the pieces `pa` cross the pieces `pb` ([`pieces_near`] of two outlines, flattened once for as many pairs as are asked
/// about); with a `probe`, only those inside it. At most [`MAX_POINTS`] points, and at most `pa.len() * pb.len()` pairs are tried.
pub fn crossings(pa: &[Line], pb: &[Line], probe: Option<Rect>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for x in pa {
        for y in pb {
            let Some(p) = crossing(x, y) else { continue };
            if probe.is_some_and(|r| p.x < r.x0 || p.x > r.x1 || p.y < r.y0 || p.y > r.y1) {
                continue;
            }
            let near = 1.0e-9 * (1.0 + p.x.abs().max(p.y.abs()));
            if out.iter().all(|q| q.distance(p) > near) {
                out.push(p);
                if out.len() >= MAX_POINTS {
                    return out;
                }
            }
        }
    }
    out
}

/// The derivative of a segment at `t`.
fn derivative(seg: &PathSeg, t: f64) -> Vec2 {
    match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => QuadBez::deriv(q).eval(t).to_vec2(),
        PathSeg::Cubic(c) => CubicBez::deriv(c).eval(t).to_vec2(),
    }
}

/// The points of the path where `cond(point - base, derivative)` is zero.
fn solve(path: &BezPath, base: Point, cond: fn(Vec2, Vec2) -> f64, skip_lines: bool) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    if !base.is_finite() {
        return out;
    }
    let l = local(path);
    let base = base - l.centre;
    for seg in l.path.segments().take(MAX_SEGMENTS) {
        if (skip_lines && matches!(seg, PathSeg::Line(_))) || !sane(&seg_points(&seg)).1 {
            continue;
        }
        roots_of(&seg, base, cond, &mut out);
        if out.len() >= MAX_POINTS {
            break;
        }
    }
    out.into_iter().map(|p| p + l.centre).collect()
}

/// The roots of `cond` on one segment, appended to `out`: the sign changes of the samples, narrowed by bisection, kept when the normalised
/// condition is nearly zero there, so that a flip at a kink or a pole is not taken for a solution.
fn roots_of(seg: &PathSeg, base: Point, cond: fn(Vec2, Vec2) -> f64, out: &mut Vec<Point>) {
    let at = |t: f64| seg.eval(t);
    let g = |t: f64| cond(at(t) - base, derivative(seg, t));
    let vals: Vec<f64> = (0..=SAMPLES).map(|i| g(i as f64 / SAMPLES as f64)).collect();
    // A segment on which every point qualifies (a circle seen from its centre) has no particular solution.
    let scale = (0..=SAMPLES)
        .map(|i| {
            let t = i as f64 / SAMPLES as f64;
            (at(t) - base).hypot() * derivative(seg, t).hypot()
        })
        .fold(0.0, f64::max);
    if !(scale.is_finite() && scale > 0.0) || vals.iter().all(|v| v.abs() <= scale * 1.0e-3) {
        return;
    }
    let mut roots = 0;
    for i in 0..SAMPLES {
        let (Some(&ga), Some(&gb)) = (vals.get(i), vals.get(i + 1)) else { break };
        if !(ga.is_finite() && gb.is_finite()) || ((ga > 0.0) == (gb > 0.0) && ga != 0.0) {
            continue;
        }
        let t = bisect(&g, i as f64 / SAMPLES as f64, (i + 1) as f64 / SAMPLES as f64, ga);
        let (p, d) = (at(t), derivative(seg, t));
        let (r, dl) = ((p - base).hypot(), d.hypot());
        if !(p.is_finite() && r > 1.0e-12 * scale.sqrt() && dl > 0.0) || cond(p - base, d).abs() > 1.0e-6 * r * dl {
            continue;
        }
        if out.iter().all(|q| q.distance(p) > 1.0e-9 * (1.0 + r)) {
            out.push(p);
            roots += 1;
            if roots >= MAX_ROOTS || out.len() >= MAX_POINTS {
                break;
            }
        }
    }
}

/// The parameter in `[lo, hi]` where `g` changes sign, given its value `fa` at `lo`.
fn bisect(g: &dyn Fn(f64) -> f64, mut lo: f64, mut hi: f64, mut fa: f64) -> f64 {
    for _ in 0..BISECT {
        let m = 0.5 * (lo + hi);
        let fm = g(m);
        if fm == 0.0 {
            return m;
        }
        if (fm > 0.0) == (fa > 0.0) {
            (lo, fa) = (m, fm);
        } else {
            hi = m;
        }
    }
    0.5 * (lo + hi)
}

/// The points of the path where the line from `base` touches it (the tangent through `base`). A straight segment has none.
pub fn tangents(path: &BezPath, base: Point) -> Vec<Point> {
    solve(path, base, |r, d| r.cross(d), true)
}

/// The feet of the perpendiculars from `base` to the path.
pub fn perpendiculars(path: &BezPath, base: Point) -> Vec<Point> {
    solve(path, base, |r, d| r.dot(d), false)
}

#[cfg(test)]
mod tests {
    use kurbo::{Circle, Shape};

    use super::*;

    fn square(x: f64, y: f64, side: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((x, y));
        p.line_to((x + side, y));
        p.line_to((x + side, y + side));
        p.line_to((x, y + side));
        p.close_path();
        p
    }

    fn circle(cx: f64, cy: f64, r: f64) -> BezPath {
        Circle::new((cx, cy), r).to_path(1.0e-6)
    }

    fn near(a: Point, b: (f64, f64)) -> bool {
        (a.x - b.0).abs() < 1.0e-6 && (a.y - b.1).abs() < 1.0e-6
    }

    /// One cubic from (0, 0) to (0, side) whose control points are `side` away, to the right and above: a box of `side` by `side`.
    fn cubic(side: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.curve_to((side, 0.0), (side, side), (0.0, side));
        p
    }

    const F: f64 = DEFAULT_FLATNESS;

    #[test]
    fn a_cubic_with_control_points_around_1e12_is_left_out_before_it_is_flattened() {
        let huge = cubic(1.0e12);
        for probe in [None, Some(Rect::new(4.0e11, 4.0e11, 6.0e11, 6.0e11))] {
            let found = pieces_near(&huge, Point::ORIGIN, probe.as_ref(), MAX_FLATTEN_PIECES, F);
            assert!(found.lines.is_empty() && found.cut, "{found:?}");
            assert_eq!((found.charged, found.made), (0, 0), "nothing was estimated or flattened");
        }
        assert!(intersections(&huge, &huge, Point::ORIGIN, None, F).is_empty());
        assert!(nearest(&huge, Point::new(5.0e11, 5.0e11)).is_none());
        // One under the size limit and over the pieces a segment may make is refused on its estimate.
        let big = cubic(MAX_EXTENT / 4.0);
        let found = pieces_near(&big, Point::ORIGIN, None, MAX_FLATTEN_PIECES, F);
        assert!(found.lines.is_empty() && found.cut && found.charged == 0 && found.made == 0, "{found:?}");
        let mut mixed = BezPath::new();
        mixed.move_to((0.0, 0.0));
        mixed.quad_to((1.0e12, 1.0e12), (1.0, 1.0));
        mixed.line_to((2.0e12, 0.0));
        mixed.curve_to((f64::NAN, 0.0), (1.0, 1.0), (2.0, 2.0));
        let found = pieces_near(&mixed, Point::ORIGIN, None, MAX_FLATTEN_PIECES, F);
        assert!(found.lines.is_empty() && found.cut && found.charged == 0 && found.made == 0, "{found:?}");
    }

    #[test]
    fn a_curve_that_cannot_reach_the_probe_is_not_flattened() {
        let probe = Rect::new(900.0, 900.0, 901.0, 901.0);
        let found = pieces_near(&cubic(100.0), Point::ORIGIN, Some(&probe), MAX_FLATTEN_PIECES, F);
        assert!(found.lines.is_empty() && !found.cut && found.charged == 0 && found.made == 0, "{found:?}");
        // The curve reaches x = 75 at the middle of its parameter, at y = 50.
        let near_it = Rect::new(74.0, 49.0, 76.0, 51.0);
        let found = pieces_near(&cubic(100.0), Point::ORIGIN, Some(&near_it), MAX_FLATTEN_PIECES, F);
        assert!(!found.lines.is_empty() && found.charged > 0 && found.made > 0, "{found:?}");
    }

    #[test]
    fn the_estimate_of_the_pieces_of_a_curve_is_never_less_than_what_flattening_makes() {
        for side in [0.001, 0.01, 0.5, 1.0, 10.0, 100.0, 1_000.0, 2_500.0] {
            let found = pieces_near(&cubic(side), Point::ORIGIN, None, MAX_FLATTEN_PIECES, F);
            let estimate = estimated_pieces(side * std::f64::consts::SQRT_2, F);
            assert!(found.made > 0 && found.made <= found.charged && found.charged == estimate, "{side}: {found:?}");
            assert!(found.charged <= MAX_SEGMENT_PIECES, "{side}: {found:?}");
            let mut p = BezPath::new();
            p.move_to((0.0, 0.0));
            p.curve_to((side, side), (0.0, side), (side, 0.0));
            let found = pieces_near(&p, Point::ORIGIN, None, MAX_FLATTEN_PIECES, F);
            assert!(found.made <= found.charged, "{side}: {found:?}");
        }
        let too_big = pieces_near(&cubic(4_000.0), Point::ORIGIN, None, MAX_FLATTEN_PIECES, F);
        assert!(too_big.cut && too_big.lines.is_empty() && too_big.charged == 0 && too_big.made == 0, "{too_big:?}");
        for bad in [(f64::NAN, F), (f64::INFINITY, F), (-1.0, F), (1.0, 0.0), (1.0, f64::NAN)] {
            assert_eq!(estimated_pieces(bad.0, bad.1), usize::MAX, "{bad:?}");
        }
    }

    #[test]
    fn the_estimates_of_many_curves_are_held_to_one_total() {
        let mut p = BezPath::new();
        for i in 0..100 {
            p.move_to((f64::from(i), 0.0));
            p.curve_to((f64::from(i) + 400.0, 0.0), (f64::from(i) + 400.0, 400.0), (f64::from(i), 400.0));
        }
        let found = pieces_near(&p, Point::ORIGIN, None, MAX_FLATTEN_PIECES, F);
        assert!(found.cut, "{found:?}");
        assert!(found.charged <= MAX_FLATTEN_PIECES && found.made <= found.charged, "{found:?}");
        assert!(found.charged > MAX_FLATTEN_PIECES - 2 * estimated_pieces(400.0 * std::f64::consts::SQRT_2, F), "{found:?}");
    }

    #[test]
    fn the_box_round_an_outline_reads_curves_exactly_and_far_paths_alike() {
        let r = path_bounds(&circle(3.0, 4.0, 2.0)).unwrap();
        assert!((r.x0 - 1.0).abs() < 1e-6 && (r.x1 - 5.0).abs() < 1e-6 && (r.y0 - 2.0).abs() < 1e-6 && (r.y1 - 6.0).abs() < 1e-6, "{r:?}");
        let far = path_bounds(&circle(1.0e11 + 3.0, 4.0, 2.0)).unwrap();
        assert!((far.width() - 4.0).abs() < 1e-3 && (far.x0 - (1.0e11 + 1.0)).abs() < 1e-3, "{far:?}");
        assert!(path_bounds(&BezPath::new()).is_none());
        let mut nan = BezPath::new();
        nan.move_to((f64::NAN, 0.0));
        nan.line_to((1.0, 1.0));
        assert!(path_bounds(&nan).is_none());
    }

    #[test]
    fn endpoints_are_the_corners_once_each() {
        let e = endpoints(&square(0.0, 0.0, 2.0));
        assert_eq!(e.len(), 5);
        assert!(near(e[0], (0.0, 0.0)) && near(e[2], (2.0, 2.0)) && near(e[4], (0.0, 0.0)));
        assert!(endpoints(&BezPath::new()).is_empty());
    }

    #[test]
    fn midpoints_are_halfway_by_length() {
        let m = midpoints(&square(0.0, 0.0, 2.0));
        assert!(near(m[0], (1.0, 0.0)) && near(m[1], (2.0, 1.0)) && near(m[2], (1.0, 2.0)) && near(m[3], (0.0, 1.0)));
        let on = midpoints(&circle(0.0, 0.0, 1.0));
        assert!(!on.is_empty());
        for p in &on {
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 1.0).abs() < 1.0e-3, "on the circle: {p:?}");
        }
    }

    #[test]
    fn the_nearest_point_is_on_the_outline() {
        let (p, d) = nearest(&square(0.0, 0.0, 2.0), Point::new(1.0, -0.5)).unwrap();
        assert!(near(p, (1.0, 0.0)) && (d - 0.5).abs() < 1.0e-9);
        let (p, d) = nearest(&circle(0.0, 0.0, 1.0), Point::new(3.0, 0.0)).unwrap();
        assert!((p.x - 1.0).abs() < 1.0e-4 && p.y.abs() < 1.0e-4 && (d - 2.0).abs() < 1.0e-4, "{p:?} {d}");
        assert!(nearest(&BezPath::new(), Point::new(0.0, 0.0)).is_none());
        assert!(nearest(&square(0.0, 0.0, 1.0), Point::new(f64::NAN, 0.0)).is_none());
    }

    #[test]
    fn intersections_are_where_outlines_cross() {
        let o = Point::ORIGIN;
        let x = intersections(&square(0.0, 0.0, 2.0), &square(1.0, 1.0, 2.0), o, None, F);
        assert_eq!(x.len(), 2, "{x:?}");
        assert!(x.iter().any(|p| near(*p, (2.0, 1.0))) && x.iter().any(|p| near(*p, (1.0, 2.0))));
        let c = intersections(&square(0.0, 0.0, 2.0), &circle(1.0, 1.0, 1.2), o, None, F);
        assert_eq!(c.len(), 8, "{c:?}");
        for p in &c {
            assert!((((p.x - 1.0).powi(2) + (p.y - 1.0).powi(2)).sqrt() - 1.2).abs() < 2.0 * F);
        }
        let near_one = intersections(&square(0.0, 0.0, 2.0), &square(1.0, 1.0, 2.0), o, Some(Rect::new(1.9, 0.9, 2.1, 1.1)), F);
        assert_eq!(near_one.len(), 1);
        assert!(intersections(&square(0.0, 0.0, 1.0), &square(5.0, 5.0, 1.0), o, None, F).is_empty());
        assert!(intersections(&BezPath::new(), &square(0.0, 0.0, 1.0), o, None, F).is_empty());
    }

    #[test]
    fn a_small_outline_far_from_the_origin_snaps_as_it_does_at_the_origin() {
        for far in [1.0e6, 1.0e9, 1.0e11] {
            let (a, b) = (square(far, far, 2.0), square(far + 1.0, far + 1.0, 2.0));
            let o = Point::new(far, far);
            let probe = Rect::new(1.9, 0.9, 2.1, 1.1);
            let x = intersections(&a, &b, o, Some(probe), F);
            assert_eq!(x.len(), 1, "{far}: {x:?}");
            assert!(near(x[0], (2.0, 1.0)), "{far}: {:?}", x[0]);
            let (p, d) = nearest(&circle(far, far, 1.0), Point::new(far + 3.0, far)).unwrap();
            assert!((p.x - far - 1.0).abs() < 1.0e-3 && (p.y - far).abs() < 1.0e-3 && (d - 2.0).abs() < 1.0e-3, "{far}: {p:?} {d}");
            assert_eq!(endpoints(&a).len(), 5, "{far}");
            assert_eq!(midpoints(&a).len(), 4, "{far}");
        }
    }

    #[test]
    fn tangents_touch_and_perpendiculars_are_normals() {
        let c = circle(0.0, 0.0, 1.0);
        let base = Point::new(0.0, 2.0);
        let t = tangents(&c, base);
        assert_eq!(t.len(), 2, "{t:?}");
        for p in &t {
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 1.0).abs() < 1.0e-4);
            assert!((p.to_vec2().dot(*p - base)).abs() < 1.0e-4);
        }
        assert!(tangents(&c, Point::new(0.1, 0.1)).is_empty());
        let loose = |a: Point, b: (f64, f64)| (a.x - b.0).abs() < 1.0e-3 && (a.y - b.1).abs() < 1.0e-3;
        let feet = perpendiculars(&c, Point::new(3.0, 0.0));
        assert!(feet.iter().any(|p| loose(*p, (1.0, 0.0))) && feet.iter().any(|p| loose(*p, (-1.0, 0.0))), "{feet:?}");
        assert!(perpendiculars(&c, Point::new(0.0, 0.0)).is_empty());
        let sq = perpendiculars(&square(0.0, 0.0, 2.0), Point::new(1.0, 3.0));
        assert!(sq.iter().any(|p| near(*p, (1.0, 2.0))), "{sq:?}");
        assert!(tangents(&square(0.0, 0.0, 2.0), Point::new(5.0, 5.0)).is_empty(), "a straight side has no tangent through a point");
    }

    #[test]
    fn hostile_paths_cost_what_the_caps_say() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        for i in 0..20_000 {
            p.line_to((f64::from(i), f64::from(i % 7)));
        }
        assert!(endpoints(&p).len() <= 2 * MAX_SEGMENTS);
        assert!(midpoints(&p).len() <= MAX_SEGMENTS);
        assert!(intersections(&p, &p, Point::ORIGIN, None, F).len() <= MAX_POINTS);
        let mut nan = BezPath::new();
        nan.move_to((f64::NAN, 0.0));
        nan.line_to((1.0, f64::INFINITY));
        assert!(endpoints(&nan).is_empty() && midpoints(&nan).is_empty() && intersections(&nan, &nan, Point::ORIGIN, None, F).is_empty());
        assert!(nearest(&nan, Point::new(0.0, 0.0)).is_none());
    }
}
