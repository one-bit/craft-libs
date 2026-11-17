//! The target set: built once per gesture, and read by every query.
//!
//! `build` only stores and sorts. Each index is made the first time a query needs it, and the work report says which ones a query made, so an
//! app that turns everything but the grid off pays nothing for its objects. A target set does not depend on the order the targets were added in.
//! (A plain set built at the press, with a span on every target, is how PhotoCraft holds its targets; see the README.)

use std::f64::consts::FRAC_PI_2;
use std::sync::OnceLock;

use kurbo::{BezPath, Point, Rect};

use crate::budget::Made;
use crate::frame::{Axis, Coincide, Direction, Id, OwnOrBounds, Side, TurnedBox, unit};
use crate::index::{Cell, DirTable, Grid, Length, RawLine, canon, sorted_lengths};
use crate::kind::Kind;

/// Why a target was not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// A number in it is not finite.
    NotFinite,
    /// The builder holds as many as its limit allows.
    OverLimit,
    /// A box with a negative size.
    NotARectangle,
    /// A line given a kind that is not a line.
    NotALine,
}

/// What the builder holds at most. A target past a limit is refused and counted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub objects: u32,
    /// The app's own lines; an object's lines count with the object.
    pub lines: u32,
    pub points: u32,
    pub paths: u32,
    /// All paths together.
    pub path_elements: u64,
    /// When two places count as one. The lines are grouped by it as they are built, so a query should use the same.
    pub coincide: Coincide,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { objects: 200_000, lines: 100_000, points: 200_000, paths: 10_000, path_elements: 2_000_000, coincide: Coincide::default() }
    }
}

/// What an object offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parts {
    /// The lines of the box round it, in the page's directions.
    pub bounds_lines: bool,
    /// The lines of its own sides, when it is turned.
    pub own_lines: bool,
    /// Its box in equal-gap rows.
    pub spacing: bool,
    /// Its own width and height, for size match.
    pub size: bool,
    /// Its angle, for rotation match.
    pub angle: bool,
    /// Its four corners as places.
    pub corners: bool,
    /// The middles of its four sides as places.
    pub side_middles: bool,
    /// Detail from the app's source (connection points, outline) when it is near.
    pub detail: bool,
}

impl Parts {
    /// Everything.
    pub const ALL: Parts =
        Parts { bounds_lines: true, own_lines: true, spacing: true, size: true, angle: true, corners: true, side_middles: true, detail: true };
    /// Nothing.
    pub const NONE: Parts = Parts {
        bounds_lines: false,
        own_lines: false,
        spacing: false,
        size: false,
        angle: false,
        corners: false,
        side_middles: false,
        detail: false,
    };

    fn bits(&self) -> u8 {
        [self.bounds_lines, self.own_lines, self.spacing, self.size, self.angle, self.corners, self.side_middles, self.detail]
            .iter()
            .fold(0, |b, x| (b << 1) | u8::from(*x))
    }
}

/// An object given to the builder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Object {
    /// Its box, turned or not. The box round it (its bounds) is worked out from this.
    pub frame: TurnedBox,
    /// Its angle for rotation match, when it differs from the frame's (a line's direction, a flipped shape's own turn).
    pub angle: Option<f64>,
    /// What it offers.
    pub parts: Parts,
}

/// What a build kept and refused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildReport {
    pub objects: u32,
    /// The app's own lines kept.
    pub lines: u32,
    pub points: u32,
    pub paths: u32,
    pub path_elements: u64,
    pub refused_not_finite: u32,
    pub refused_over_limit: u32,
    pub refused_not_a_rectangle: u32,
    /// How many directions the app's own lines have, and how many line places (after equal places are grouped).
    pub directions: u32,
    pub line_places: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ObjectEntry {
    pub id: Id,
    pub frame: TurnedBox,
    pub angle: Option<f64>,
    pub parts: Parts,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PointEntry {
    pub id: Option<Id>,
    pub at: Point,
    pub kind: Kind,
    pub number: u32,
}

/// One side of a box as a line: the angle of its normal, which side it is, and where it is, relative to the frame's origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SideLine {
    pub angle: f64,
    pub side: Side,
    pub own: OwnOrBounds,
    pub offset: f64,
    pub lo: f64,
    pub hi: f64,
}

/// The kind a side of a box has as a line.
pub(crate) fn side_kind(side: Side) -> Kind {
    if side == Side::Mid { Kind::Centre } else { Kind::Edge }
}

/// The lines of a box, relative to `origin`: the sides of the box round it, in the page's directions, and, when it is turned and `own` is
/// asked for, its own sides. A box a whole number of quarter turns from the page has the page's lines only, once.
pub(crate) fn side_lines(frame: &TurnedBox, origin: Point, co: &Coincide, own: bool) -> Vec<SideLine> {
    let c = frame.centre - origin.to_vec2();
    let rel = TurnedBox { centre: c, ..*frame };
    let (page, turned) = match frame.quarter(co.angle) {
        Some(q) => (TurnedBox { centre: c, ..frame.squared(q) }.bounds(), false),
        None => (rel.bounds(), true),
    };
    let mut out = Vec::with_capacity(12);
    let sides = |lo: f64, hi: f64| [(Side::Min, lo), (Side::Mid, 0.5 * (lo + hi)), (Side::Max, hi)];
    for (side, x) in sides(page.x0, page.x1) {
        out.push(SideLine { angle: 0.0, side, own: OwnOrBounds::Bounds, offset: x, lo: page.y0, hi: page.y1 });
    }
    for (side, y) in sides(page.y0, page.y1) {
        out.push(SideLine { angle: FRAC_PI_2, side, own: OwnOrBounds::Bounds, offset: y, lo: -page.x1, hi: -page.x0 });
    }
    if own && turned {
        let (u, v) = (rel.u(), rel.v());
        let (hw, hh) = (0.5 * frame.size.width, 0.5 * frame.size.height);
        let (cu, cv) = (u.dot(c.to_vec2()), v.dot(c.to_vec2()));
        for (side, k) in [(Side::Min, -1.0), (Side::Mid, 0.0), (Side::Max, 1.0)] {
            out.push(SideLine { angle: frame.angle, side, own: OwnOrBounds::Own, offset: cu + k * hw, lo: cv - hh, hi: cv + hh });
            out.push(SideLine { angle: frame.angle + FRAC_PI_2, side, own: OwnOrBounds::Own, offset: cv + k * hh, lo: -cu - hw, hi: -cu + hw });
        }
    }
    out
}

fn raw_of(l: &SideLine, kind: Kind, id: Option<Id>, co: &Coincide) -> RawLine {
    let (angle, flipped) = canon(l.angle, co.angle);
    // A flipped normal reads the same line with its offset changed in sign and its tangent turned round.
    if flipped {
        RawLine { angle, offset: -l.offset, lo: -l.hi, hi: -l.lo, kind, id }
    } else {
        RawLine { angle, offset: l.offset, lo: l.lo, hi: l.hi, kind, id }
    }
}

/// What the builder collects.
pub struct TargetsBuilder {
    limits: Limits,
    objects: Vec<ObjectEntry>,
    lines: Vec<RawLine>,
    points: Vec<PointEntry>,
    paths: Vec<(Id, BezPath)>,
    report: BuildReport,
}

fn push_checked<T>(v: &mut Vec<T>, item: T) -> Result<(), Refused> {
    v.try_reserve(1).map_err(|_| Refused::OverLimit)?;
    v.push(item);
    Ok(())
}

impl TargetsBuilder {
    /// A builder with these limits.
    pub fn new(limits: Limits) -> TargetsBuilder {
        TargetsBuilder { limits, objects: Vec::new(), lines: Vec::new(), points: Vec::new(), paths: Vec::new(), report: BuildReport::default() }
    }

    fn refuse<T>(&mut self, why: Refused) -> Result<T, Refused> {
        match why {
            Refused::NotFinite | Refused::NotALine => self.report.refused_not_finite += 1,
            Refused::OverLimit => self.report.refused_over_limit += 1,
            Refused::NotARectangle => self.report.refused_not_a_rectangle += 1,
        }
        Err(why)
    }

    /// An object.
    pub fn object(&mut self, id: Id, object: Object) -> Result<(), Refused> {
        let f = &object.frame;
        if !(f.centre.is_finite()
            && f.size.width.is_finite()
            && f.size.height.is_finite()
            && f.angle.is_finite()
            && object.angle.is_none_or(f64::is_finite))
        {
            return self.refuse(Refused::NotFinite);
        }
        if f.size.width < 0.0 || f.size.height < 0.0 {
            return self.refuse(Refused::NotARectangle);
        }
        if self.report.objects >= self.limits.objects {
            return self.refuse(Refused::OverLimit);
        }
        let entry = ObjectEntry { id, frame: object.frame, angle: object.angle, parts: object.parts };
        if push_checked(&mut self.objects, entry).is_err() {
            return self.refuse(Refused::OverLimit);
        }
        self.report.objects += 1;
        Ok(())
    }

    /// A line square to the page: `x = at` or `y = at`. `span` is its extent along the line (y for an `x` line, x for a `y` line), or `None`
    /// for a line that runs on for ever.
    pub fn line(&mut self, id: Option<Id>, axis: Axis, at: f64, span: Option<(f64, f64)>, kind: Kind) -> Result<(), Refused> {
        self.line_at(id, Direction::of_axis(axis), at, span.map(|(a, b)| if axis == Axis::X { (a, b) } else { (-b, -a) }), kind)
    }

    /// A line in any direction: the points `p` with `n · p = offset`, `n` the direction's unit normal. `span` is its extent along the
    /// direction's tangent `(-n.y, n.x)`, or `None` for a line that runs on for ever.
    pub fn line_at(&mut self, id: Option<Id>, direction: Direction, offset: f64, span: Option<(f64, f64)>, kind: Kind) -> Result<(), Refused> {
        if !kind.is_line() {
            return self.refuse(Refused::NotALine);
        }
        let (lo, hi) = span.unwrap_or((f64::INFINITY, f64::NEG_INFINITY));
        if !offset.is_finite() || (span.is_some() && !(lo.is_finite() && hi.is_finite())) {
            return self.refuse(Refused::NotFinite);
        }
        self.add_line(RawLine { angle: direction.angle(), offset, lo: lo.min(hi), hi: lo.max(hi), kind, id })
    }

    fn add_line(&mut self, line: RawLine) -> Result<(), Refused> {
        let (angle, flipped) = canon(line.angle, self.limits.coincide.angle);
        let l = if flipped { RawLine { angle, offset: -line.offset, lo: -line.hi, hi: -line.lo, ..line } } else { RawLine { angle, ..line } };
        self.push_canon(l)
    }

    /// Keeps a line whose angle is canonical already.
    fn push_canon(&mut self, line: RawLine) -> Result<(), Refused> {
        if self.report.lines >= self.limits.lines {
            return self.refuse(Refused::OverLimit);
        }
        if push_checked(&mut self.lines, line).is_err() {
            return self.refuse(Refused::OverLimit);
        }
        self.report.lines += 1;
        Ok(())
    }

    /// Four edge lines, and the two centre lines when `centre`. A turned frame gives lines in its own directions.
    pub fn frame(&mut self, id: Option<Id>, frame: TurnedBox, kind: Kind, centre: bool) -> Result<(), Refused> {
        if !(frame.centre.is_finite() && frame.size.width.is_finite() && frame.size.height.is_finite() && frame.angle.is_finite()) {
            return self.refuse(Refused::NotFinite);
        }
        if frame.size.width < 0.0 || frame.size.height < 0.0 || !kind.is_line() {
            return self.refuse(if kind.is_line() { Refused::NotARectangle } else { Refused::NotALine });
        }
        let co = self.limits.coincide;
        let mut lines = side_lines(&frame, Point::ORIGIN, &co, true);
        // For a turned frame the lines are its own: the page's lines of its bounds are not a frame's.
        if frame.quarter(co.angle).is_none() {
            lines.retain(|l| l.own == OwnOrBounds::Own);
        }
        lines.retain(|l| centre || l.side != Side::Mid);
        let room = self.limits.lines.saturating_sub(self.report.lines) as usize;
        if lines.len() > room {
            return self.refuse(Refused::OverLimit);
        }
        for l in lines {
            self.push_canon(raw_of(&l, kind, id, &co))?;
        }
        Ok(())
    }

    /// A place of the app's own: a connection point, an anchor.
    pub fn point(&mut self, id: Option<Id>, at: Point, kind: Kind, number: u32) -> Result<(), Refused> {
        if !at.is_finite() {
            return self.refuse(Refused::NotFinite);
        }
        if self.report.points >= self.limits.points {
            return self.refuse(Refused::OverLimit);
        }
        if push_checked(&mut self.points, PointEntry { id, at, kind, number }).is_err() {
            return self.refuse(Refused::OverLimit);
        }
        self.report.points += 1;
        Ok(())
    }

    /// An outline given up front, for an app that has its paths at hand.
    pub fn path(&mut self, id: Id, path: BezPath) -> Result<(), Refused> {
        let n = path.elements().len() as u64;
        if self.report.paths >= self.limits.paths || self.report.path_elements.saturating_add(n) > self.limits.path_elements {
            return self.refuse(Refused::OverLimit);
        }
        if push_checked(&mut self.paths, (id, path)).is_err() {
            return self.refuse(Refused::OverLimit);
        }
        self.report.paths += 1;
        self.report.path_elements += n;
        Ok(())
    }

    /// Sorts what was added and makes the set. Insertion order does not matter.
    pub fn build(self) -> Targets {
        let TargetsBuilder { limits, mut objects, lines, mut points, mut paths, mut report } = self;
        let co = limits.coincide;
        objects.sort_by(object_order);
        points.sort_by(point_order);
        paths.sort_by_key(|p| p.0);
        let origin = median_origin(&objects, &points);
        // Offsets and extents were taken about the page's origin; they are moved to the targets' own.
        let o = origin.to_vec2();
        let shifted: Vec<RawLine> = lines
            .into_iter()
            .map(|l| {
                let n = unit(l.angle);
                let t = kurbo::Vec2::new(-n.y, n.x);
                RawLine { offset: l.offset - n.dot(o), lo: l.lo - t.dot(o), hi: l.hi - t.dot(o), ..l }
            })
            .collect();
        let app = DirTable::build(shifted, &co);
        report.directions = u32::try_from(app.dirs.len()).unwrap_or(u32::MAX);
        report.line_places = u32::try_from(app.places).unwrap_or(u32::MAX);
        Targets {
            co,
            origin,
            objects,
            points,
            paths,
            app,
            object_lines: OnceLock::new(),
            object_grid: OnceLock::new(),
            point_grid: OnceLock::new(),
            widths: OnceLock::new(),
            heights: OnceLock::new(),
            angles: OnceLock::new(),
            report,
        }
    }
}

fn object_order(a: &ObjectEntry, b: &ObjectEntry) -> std::cmp::Ordering {
    let (fa, fb) = (&a.frame, &b.frame);
    a.id.cmp(&b.id)
        .then(fa.centre.x.total_cmp(&fb.centre.x))
        .then(fa.centre.y.total_cmp(&fb.centre.y))
        .then(fa.size.width.total_cmp(&fb.size.width))
        .then(fa.size.height.total_cmp(&fb.size.height))
        .then(fa.angle.total_cmp(&fb.angle))
        .then(a.angle.map(f64::to_bits).cmp(&b.angle.map(f64::to_bits)))
        .then(a.parts.bits().cmp(&b.parts.bits()))
}

fn point_order(a: &PointEntry, b: &PointEntry) -> std::cmp::Ordering {
    a.at.x.total_cmp(&b.at.x).then(a.at.y.total_cmp(&b.at.y)).then(a.kind.cmp(&b.kind)).then(a.number.cmp(&b.number)).then(a.id.cmp(&b.id))
}

/// The median of the centres of the objects and the places: the origin the targets are indexed about.
fn median_origin(objects: &[ObjectEntry], points: &[PointEntry]) -> Point {
    let mut xs: Vec<f64> = objects.iter().map(|o| o.frame.centre.x).chain(points.iter().map(|p| p.at.x)).collect();
    let mut ys: Vec<f64> = objects.iter().map(|o| o.frame.centre.y).chain(points.iter().map(|p| p.at.y)).collect();
    let mid = |v: &mut Vec<f64>| {
        let k = v.len() / 2;
        if v.is_empty() {
            return 0.0;
        }
        let (_, m, _) = v.select_nth_unstable_by(k, f64::total_cmp);
        *m
    };
    Point::new(mid(&mut xs), mid(&mut ys))
}

/// A set of targets. Build it once per gesture, without the objects that move, and ask it at every pointer move.
pub struct Targets {
    pub(crate) co: Coincide,
    pub(crate) origin: Point,
    pub(crate) objects: Vec<ObjectEntry>,
    pub(crate) points: Vec<PointEntry>,
    pub(crate) paths: Vec<(Id, BezPath)>,
    pub(crate) app: DirTable,
    object_lines: OnceLock<DirTable>,
    object_grid: OnceLock<Grid>,
    point_grid: OnceLock<Grid>,
    widths: OnceLock<Vec<Length>>,
    heights: OnceLock<Vec<Length>>,
    angles: OnceLock<(f64, Vec<Length>)>,
    report: BuildReport,
}

impl Targets {
    /// What was kept, refused for numbers that are not finite, refused past a limit, and how many directions and line places there are.
    pub fn report(&self) -> &BuildReport {
        &self.report
    }

    /// The tolerance the lines were grouped by.
    pub fn coincide(&self) -> Coincide {
        self.co
    }

    /// The point the targets are indexed about: the median of the centres of the objects and the places.
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// The table of the lines of the objects, made the first time it is asked for.
    pub(crate) fn object_lines(&self, made: &mut Made) -> &DirTable {
        let mut did = false;
        let table = self.object_lines.get_or_init(|| {
            did = true;
            let mut raw = Vec::new();
            for o in &self.objects {
                if !(o.parts.bounds_lines || o.parts.own_lines) {
                    continue;
                }
                for l in side_lines(&o.frame, self.origin, &self.co, o.parts.own_lines) {
                    let wanted = if l.own == OwnOrBounds::Own { o.parts.own_lines } else { o.parts.bounds_lines || o.parts.own_lines };
                    if wanted && raw.try_reserve(1).is_ok() {
                        raw.push(raw_of(&l, side_kind(l.side), Some(o.id), &self.co));
                    }
                }
            }
            DirTable::build(raw, &self.co)
        });
        if did {
            made.0 |= Made::OBJECT_LINES;
        }
        table
    }

    /// The bucket grid of the objects' bounds, made the first time it is asked for.
    pub(crate) fn object_grid(&self, made: &mut Made) -> &Grid {
        let mut did = false;
        let g = self.object_grid.get_or_init(|| {
            did = true;
            let cells = self
                .objects
                .iter()
                .enumerate()
                .filter_map(|(i, o)| Some(Cell { bounds: self.rel_bounds(&o.frame), id: o.id, index: u32::try_from(i).ok()? }));
            Grid::build(cells.collect())
        });
        if did {
            made.0 |= Made::BUCKETS;
        }
        g
    }

    /// The bucket grid of the app's places, made the first time it is asked for.
    pub(crate) fn point_grid(&self, made: &mut Made) -> &Grid {
        let mut did = false;
        let g = self.point_grid.get_or_init(|| {
            did = true;
            let o = self.origin.to_vec2();
            let cells = self.points.iter().enumerate().filter_map(|(i, p)| {
                let at = p.at - o;
                Some(Cell { bounds: Rect::new(at.x, at.y, at.x, at.y), id: p.id.unwrap_or(Id(u64::MAX)), index: u32::try_from(i).ok()? })
            });
            Grid::build(cells.collect())
        });
        if did {
            made.0 |= Made::POINTS;
        }
        g
    }

    /// The bounds of a box, relative to the origin.
    pub(crate) fn rel_bounds(&self, f: &TurnedBox) -> Rect {
        TurnedBox { centre: f.centre - self.origin.to_vec2(), ..*f }.bounds()
    }

    /// The own widths of the objects that offer size, sorted.
    pub(crate) fn widths(&self, made: &mut Made) -> &[Length] {
        self.lengths(&self.widths, true, made)
    }

    /// The own heights of the objects that offer size, sorted.
    pub(crate) fn heights(&self, made: &mut Made) -> &[Length] {
        self.lengths(&self.heights, false, made)
    }

    fn lengths<'a>(&'a self, cell: &'a OnceLock<Vec<Length>>, width: bool, made: &mut Made) -> &'a [Length] {
        let mut did = false;
        let v = cell.get_or_init(|| {
            did = true;
            let it = self
                .objects
                .iter()
                .filter(|o| o.parts.size)
                .map(|o| Length { length: if width { o.frame.size.width } else { o.frame.size.height }, id: o.id });
            sorted_lengths(it.collect())
        });
        if did {
            made.0 |= Made::LENGTHS;
        }
        v
    }

    /// The angles of the objects that offer one, folded into `[0, period)` and sorted. The array is made for the first period asked for;
    /// a query with another period gets `None`.
    pub(crate) fn angles(&self, period: f64, made: &mut Made) -> Option<&[Length]> {
        let mut did = false;
        let (p, v) = self.angles.get_or_init(|| {
            did = true;
            let it = self
                .objects
                .iter()
                .filter(|o| o.parts.angle)
                .map(|o| Length { length: o.angle.unwrap_or(o.frame.angle).rem_euclid(period), id: o.id });
            (period, sorted_lengths(it.collect()))
        });
        if did {
            made.0 |= Made::ANGLES;
        }
        (p.to_bits() == period.to_bits()).then_some(v.as_slice())
    }

    /// The size of an object's box, for a test.
    #[cfg(test)]
    pub(crate) fn object_size(&self, id: Id) -> Option<Size> {
        self.objects.iter().find(|o| o.id == id).map(|o| o.frame.size)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;

    fn obj(x: f64, y: f64, w: f64, h: f64, a: f64) -> Object {
        Object { frame: TurnedBox { centre: Point::new(x, y), size: Size::new(w, h), angle: a }, angle: None, parts: Parts::ALL }
    }

    fn targets(items: &[(u64, Object)]) -> Targets {
        let mut b = TargetsBuilder::new(Limits::default());
        for (id, o) in items {
            b.object(Id(*id), *o).unwrap();
        }
        b.build()
    }

    #[test]
    fn an_unturned_box_has_six_lines_and_a_turned_one_twelve() {
        let co = Coincide::default();
        let plain = side_lines(&TurnedBox::square(Point::new(5.0, 7.0), Size::new(4.0, 2.0)), Point::ORIGIN, &co, true);
        assert_eq!(plain.len(), 6);
        let x: Vec<f64> = plain.iter().filter(|l| l.angle == 0.0).map(|l| l.offset).collect();
        assert_eq!(x, [3.0, 5.0, 7.0]);
        let quarter = side_lines(&TurnedBox { centre: Point::new(5.0, 7.0), size: Size::new(4.0, 2.0), angle: FRAC_PI_2 }, Point::ORIGIN, &co, true);
        assert_eq!(quarter.len(), 6, "a quarter turn has the page's lines once");
        let xq: Vec<f64> = quarter.iter().filter(|l| l.angle == 0.0).map(|l| l.offset).collect();
        assert_eq!(xq, [4.0, 5.0, 6.0]);
        let turned = side_lines(&TurnedBox { centre: Point::new(5.0, 7.0), size: Size::new(4.0, 2.0), angle: 0.5 }, Point::ORIGIN, &co, true);
        assert_eq!(turned.len(), 12);
        assert_eq!(side_lines(&TurnedBox { centre: Point::ORIGIN, size: Size::new(1.0, 1.0), angle: 0.5 }, Point::ORIGIN, &co, false).len(), 6);
    }

    #[test]
    fn the_sides_of_a_turned_box_lie_on_its_edges() {
        let co = Coincide::default();
        let f = TurnedBox { centre: Point::new(3.0, -2.0), size: Size::new(6.0, 2.0), angle: 0.7 };
        let lines = side_lines(&f, Point::ORIGIN, &co, true);
        for l in lines.iter().filter(|l| l.own == OwnOrBounds::Own && l.side != Side::Mid) {
            let n = unit(l.angle);
            let t = kurbo::Vec2::new(-n.y, n.x);
            let on: Vec<Point> = f.corners().into_iter().filter(|p| (n.dot(p.to_vec2()) - l.offset).abs() < 1e-12).collect();
            assert_eq!(on.len(), 2, "{l:?}");
            for p in on {
                let s = t.dot(p.to_vec2());
                assert!(s >= l.lo - 1e-12 && s <= l.hi + 1e-12, "{l:?} {s}");
            }
        }
    }

    #[test]
    fn the_origin_is_the_median_and_the_set_does_not_depend_on_the_order() {
        let items: Vec<(u64, Object)> =
            (0..50).map(|i| (i, obj((i * 7 % 13) as f64, (i * 5 % 11) as f64, 1.0 + (i % 3) as f64, 1.0, (i % 4) as f64 * 0.3))).collect();
        let a = targets(&items);
        let mut rev = items.clone();
        rev.reverse();
        let b = targets(&rev);
        assert_eq!(a.origin(), b.origin());
        assert_eq!(format!("{:?}", a.objects), format!("{:?}", b.objects));
        let (mut ma, mut mb) = (Made::default(), Made::default());
        assert_eq!(format!("{:?}", a.object_lines(&mut ma).dirs), format!("{:?}", b.object_lines(&mut mb).dirs));
        assert!(ma.has(Made::OBJECT_LINES));
        let mut again = Made::default();
        let _ = a.object_lines(&mut again);
        assert!(!again.has(Made::OBJECT_LINES), "made once");
        assert_eq!(a.report().objects, 50);
        assert_eq!(a.object_size(Id(0)).map(|s| s.width), Some(1.0));
    }

    #[test]
    fn hostile_targets_are_refused_and_counted() {
        let limits = Limits { objects: 2, lines: 3, points: 1, paths: 1, path_elements: 4, ..Limits::default() };
        let mut b = TargetsBuilder::new(limits);
        assert_eq!(b.object(Id(1), obj(f64::NAN, 0.0, 1.0, 1.0, 0.0)), Err(Refused::NotFinite));
        assert_eq!(b.object(Id(1), obj(0.0, 0.0, -1.0, 1.0, 0.0)), Err(Refused::NotARectangle));
        assert_eq!(b.object(Id(1), obj(0.0, 0.0, 1.0, 1.0, f64::INFINITY)), Err(Refused::NotFinite));
        assert!(b.object(Id(1), obj(0.0, 0.0, 1.0, 1.0, 0.0)).is_ok() && b.object(Id(2), obj(0.0, 0.0, 1.0, 1.0, 0.0)).is_ok());
        assert_eq!(b.object(Id(3), obj(0.0, 0.0, 1.0, 1.0, 0.0)), Err(Refused::OverLimit));
        assert_eq!(b.line(None, Axis::X, f64::NAN, None, Kind::Guide), Err(Refused::NotFinite));
        assert_eq!(b.line(None, Axis::X, 1.0, Some((0.0, f64::INFINITY)), Kind::Guide), Err(Refused::NotFinite));
        assert_eq!(b.line(None, Axis::X, 1.0, None, Kind::Corner), Err(Refused::NotALine));
        assert!(b.line(None, Axis::X, 1.0, None, Kind::Guide).is_ok());
        assert!(b.line(None, Axis::Y, 1.0, None, Kind::Guide).is_ok());
        assert!(
            b.frame(None, TurnedBox::square(Point::ORIGIN, Size::new(1.0, 1.0)), Kind::Frame, true).is_err(),
            "six lines do not fit the one left"
        );
        assert!(b.line_at(None, Direction::new(0.3).unwrap(), 1.0, None, Kind::Guide).is_ok());
        assert_eq!(b.line_at(None, Direction::new(0.3).unwrap(), 1.0, None, Kind::Guide), Err(Refused::OverLimit));
        assert_eq!(b.point(None, Point::new(f64::INFINITY, 0.0), Kind::Point(0), 0), Err(Refused::NotFinite));
        assert!(b.point(None, Point::new(1.0, 2.0), Kind::Point(0), 0).is_ok());
        assert_eq!(b.point(None, Point::new(1.0, 2.0), Kind::Point(0), 1), Err(Refused::OverLimit));
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((1.0, 1.0));
        assert!(b.path(Id(1), p.clone()).is_ok());
        assert_eq!(b.path(Id(2), p), Err(Refused::OverLimit));
        let t = b.build();
        let r = t.report();
        assert_eq!((r.objects, r.lines, r.points, r.paths, r.path_elements), (2, 3, 1, 1, 2));
        assert_eq!((r.refused_not_finite, r.refused_not_a_rectangle), (6, 1 + 0));
        assert!(r.refused_over_limit >= 5);
    }

    #[test]
    fn app_lines_are_grouped_and_a_frame_gives_edges_and_centres() {
        let mut b = TargetsBuilder::new(Limits::default());
        for _ in 0..1000 {
            b.line(Some(Id(7)), Axis::X, 3.0, Some((0.0, 1.0)), Kind::Guide).unwrap();
        }
        b.frame(Some(Id(9)), TurnedBox::square(Point::new(50.0, 40.0), Size::new(100.0, 80.0)), Kind::Frame, true).unwrap();
        let t = b.build();
        assert_eq!(t.report().lines, 1006);
        assert_eq!(t.report().directions, 2);
        assert_eq!(t.report().line_places, 1 + 6);
        let mut b = TargetsBuilder::new(Limits::default());
        b.frame(None, TurnedBox { centre: Point::ORIGIN, size: Size::new(10.0, 4.0), angle: 0.4 }, Kind::Frame, false).unwrap();
        let t = b.build();
        assert_eq!((t.report().lines, t.report().directions), (4, 2), "a turned frame's lines are its own, without centres here");
    }

    #[test]
    fn a_far_scene_indexes_like_a_near_one() {
        let near: Vec<(u64, Object)> = (0..30).map(|i| (i, obj((i % 6) as f64 * 3.0, (i / 6) as f64 * 3.0, 2.0, 1.5, 0.0))).collect();
        let far = |d: f64| -> Vec<(u64, Object)> {
            near.iter().map(|(i, o)| (*i, obj(o.frame.centre.x + d, o.frame.centre.y + d, 2.0, 1.5, 0.0))).collect()
        };
        let a = targets(&near);
        let mut made = Made::default();
        let la = a.object_lines(&mut made).all_entries();
        for d in [1.0e6, 1.0e9, 1.0e11] {
            let b = targets(&far(d));
            let lb = b.object_lines(&mut made).all_entries();
            assert_eq!(la.len(), lb.len(), "{d}");
            for (x, y) in la.iter().zip(lb.iter()) {
                assert!(x.0 == y.0 && x.1 == y.1 && x.2.count == y.2.count, "{d}");
                assert!((x.2.offset - y.2.offset).abs() <= 1e-9 * (1.0 + x.2.offset.abs()), "{d}: {} {}", x.2.offset, y.2.offset);
            }
        }
        assert!((PI - FRAC_PI_2 - FRAC_PI_2).abs() < 1e-15);
    }
}
