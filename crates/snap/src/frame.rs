//! Ids, directions, turned boxes, and the one rule for when two numbers are the same place.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use kurbo::{Point, Rect, Size, Vec2};

/// The app's own id for a target. The crate never reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub u64);

/// A page axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    /// The other axis.
    pub fn other(self) -> Axis {
        match self {
            Axis::X => Axis::Y,
            Axis::Y => Axis::X,
        }
    }

    /// The unit vector along the axis.
    pub fn unit(self) -> Vec2 {
        match self {
            Axis::X => Vec2::new(1.0, 0.0),
            Axis::Y => Vec2::new(0.0, 1.0),
        }
    }
}

/// A place along a direction: the low side, the middle, the high side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Min,
    Mid,
    Max,
}

/// Which sides of a box snap along one of its own axes. All three false holds that axis still.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anchors {
    pub min: bool,
    pub mid: bool,
    pub max: bool,
}

impl Anchors {
    /// Every side snaps.
    pub const ALL: Anchors = Anchors { min: true, mid: true, max: true };
    /// No side snaps: the axis is held still.
    pub const NONE: Anchors = Anchors { min: false, mid: false, max: false };

    /// Whether the side is on.
    pub fn has(&self, side: Side) -> bool {
        match side {
            Side::Min => self.min,
            Side::Mid => self.mid,
            Side::Max => self.max,
        }
    }

    /// The same flags seen from the other end of the axis: low and high swapped.
    pub fn reversed(&self) -> Anchors {
        Anchors { min: self.max, mid: self.mid, max: self.min }
    }
}

/// Whether a side is one of a box's own sides or a side of the box round it (its bounds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OwnOrBounds {
    Own,
    Bounds,
}

/// When two places count as one, and two directions as one. One rule, read by the solver and by the check alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coincide {
    /// Two places are one within this much.
    pub abs: f64,
    /// Or within this times the larger magnitude of the two.
    pub rel: f64,
    /// Two directions are one within this many radians.
    pub angle: f64,
}

impl Default for Coincide {
    fn default() -> Self {
        Coincide { abs: 1.0e-9, rel: 8.0 * f64::EPSILON, angle: 1.0e-9 }
    }
}

impl Coincide {
    /// The rule for geometry whose largest coordinate is `far`: a number that big cannot say a place closer than its own spacing, so the absolute
    /// part grows by a few of them. Far from the origin a result is then not called off its line for the rounding of its own coordinates.
    pub fn at(&self, far: f64) -> Coincide {
        let extra = if far.is_finite() { 16.0 * f64::EPSILON * far.abs() } else { 0.0 };
        Coincide { abs: self.abs + extra, ..*self }
    }

    /// Whether every number is finite and not negative.
    pub fn is_valid(&self) -> bool {
        [self.abs, self.rel, self.angle].iter().all(|v| v.is_finite() && *v >= 0.0)
    }

    /// How far apart `a` and `b` may be and be the same place: `max(abs, rel * max(|a|, |b|))`.
    pub fn tol(&self, a: f64, b: f64) -> f64 {
        self.abs.max(self.rel * a.abs().max(b.abs()))
    }

    /// Whether `a` and `b` are the same place. Not a number: never.
    pub fn same(&self, a: f64, b: f64) -> bool {
        (a - b).abs() <= self.tol(a, b)
    }

    /// Whether the points are the same place on both axes.
    pub fn same_point(&self, a: Point, b: Point) -> bool {
        self.same(a.x, b.x) && self.same(a.y, b.y)
    }
}

/// The side of a box as it is along the folded direction of its line: where folding turned the normal round, low and high swap.
pub fn canon_side(side: Side, flipped: bool) -> Side {
    match (side, flipped) {
        (Side::Min, true) => Side::Max,
        (Side::Max, true) => Side::Min,
        (s, _) => s,
    }
}

/// An angle within a turn: what is left of it after whole turns (`angle % TAU`; the remainder of a float by a float is exact). An angle of a
/// million turns has the geometry of what it has left, and a quarter turn added to it is not lost in its last bit.
pub fn within_a_turn(angle: f64) -> f64 {
    angle % TAU
}

/// The unit vector at `angle`, exact at the multiples of a quarter turn (and within `1e-12` of them).
pub fn unit(angle: f64) -> Vec2 {
    let angle = within_a_turn(angle);
    let q = angle / FRAC_PI_2;
    let r = q.round();
    if (q - r).abs() * FRAC_PI_2 <= 1.0e-12 {
        return match (r as i64).rem_euclid(4) {
            0 => Vec2::new(1.0, 0.0),
            1 => Vec2::new(0.0, 1.0),
            2 => Vec2::new(-1.0, 0.0),
            _ => Vec2::new(0.0, -1.0),
        };
    }
    Vec2::new(angle.cos(), angle.sin())
}

/// `angle` folded into `[0, π)`, and whether the unit normal at `angle` is the opposite of the one at the folded angle.
pub fn fold(angle: f64) -> (f64, bool) {
    let angle = within_a_turn(angle);
    let k = (angle / PI).floor();
    let mut a = angle - k * PI;
    let mut flipped = k.rem_euclid(2.0) == 1.0;
    if a >= PI {
        a -= PI;
        flipped = !flipped;
    }
    if a < 0.0 {
        a = 0.0;
    }
    (a, flipped)
}

/// The normal of a family of parallel lines: an angle in radians, folded into `[0, π)`. A line `x = c` has direction `X` (0); a line `y = c` has
/// direction `Y` (π/2).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Direction(f64);

impl Direction {
    /// The page's x direction: the normal of the lines `x = c`.
    pub const X: Direction = Direction(0.0);
    /// The page's y direction: the normal of the lines `y = c`.
    pub const Y: Direction = Direction(FRAC_PI_2);

    /// The direction at `angle`, folded. Not a number: `None`.
    pub fn new(angle: f64) -> Option<Direction> {
        angle.is_finite().then(|| Direction(fold(angle).0))
    }

    /// The direction of lines square to the page axis.
    pub fn of_axis(axis: Axis) -> Direction {
        match axis {
            Axis::X => Direction::X,
            Axis::Y => Direction::Y,
        }
    }

    /// The angle, in `[0, π)`.
    pub fn angle(&self) -> f64 {
        self.0
    }

    /// The unit normal.
    pub fn normal(&self) -> Vec2 {
        unit(self.0)
    }

    /// The unit vector along the lines.
    pub fn tangent(&self) -> Vec2 {
        let n = self.normal();
        Vec2::new(-n.y, n.x)
    }
}

/// A box turned by `angle` (radians, counterclockwise in the app's frame) about its centre. Its own axes are `u = (cos angle, sin angle)` and
/// `v = (-sin angle, cos angle)`; `size` is along `u`, then `v`. Angle 0 is a box square to the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnedBox {
    pub centre: Point,
    pub size: Size,
    pub angle: f64,
}

impl TurnedBox {
    /// A box square to the page.
    pub fn square(centre: Point, size: Size) -> TurnedBox {
        TurnedBox { centre, size, angle: 0.0 }
    }

    /// A box square to the page, from a rectangle.
    pub fn from_rect(r: Rect) -> TurnedBox {
        TurnedBox::square(r.center(), r.size())
    }

    /// Whether every number is finite and the size is not negative.
    pub fn is_valid(&self) -> bool {
        self.centre.is_finite()
            && self.size.width.is_finite()
            && self.size.height.is_finite()
            && self.size.width >= 0.0
            && self.size.height >= 0.0
            && self.angle.is_finite()
    }

    /// The angle within a turn: the one the geometry is made from.
    pub fn turn(&self) -> f64 {
        within_a_turn(self.angle)
    }

    /// The own axis along the width.
    pub fn u(&self) -> Vec2 {
        unit(self.angle)
    }

    /// The own axis along the height.
    pub fn v(&self) -> Vec2 {
        let u = self.u();
        Vec2::new(-u.y, u.x)
    }

    /// The corners: min-min, max-min, max-max, min-max along `u` then `v`.
    pub fn corners(&self) -> [Point; 4] {
        let (u, v) = (self.u() * (0.5 * self.size.width), self.v() * (0.5 * self.size.height));
        [self.centre - u - v, self.centre + u - v, self.centre + u + v, self.centre - u + v]
    }

    /// The box round it, its bounds.
    pub fn bounds(&self) -> Rect {
        let (u, v) = (self.u(), self.v());
        let (hw, hh) = (0.5 * self.size.width, 0.5 * self.size.height);
        let ex = u.x.abs() * hw + v.x.abs() * hh;
        let ey = u.y.abs() * hw + v.y.abs() * hh;
        Rect::new(self.centre.x - ex, self.centre.y - ey, self.centre.x + ex, self.centre.y + ey)
    }

    /// How many quarter turns the angle is from a box square to the page (0 to 3), when it is within `tol` radians of one.
    pub fn quarter(&self, tol: f64) -> Option<u8> {
        let q = self.turn() / FRAC_PI_2;
        let r = q.round();
        ((q - r).abs() * FRAC_PI_2 <= tol).then(|| (r as i64).rem_euclid(4) as u8)
    }

    /// The box with its own axes, width and height as if square to the page: for a quarter turn the sizes swap, and the angle is 0.
    pub fn squared(&self, quarter: u8) -> TurnedBox {
        let size = if quarter % 2 == 1 { Size::new(self.size.height, self.size.width) } else { self.size };
        TurnedBox { centre: self.centre, size, angle: 0.0 }
    }
}

/// Maps a turned box that is a whole number of quarter turns from the page to the box square to the page that is the same shape on the page,
/// and the flags of its own axes to the flags of the page axes. Returns `None` when the box is turned by something else.
pub fn to_page_axes(b: &TurnedBox, u: Anchors, v: Anchors, tol: f64) -> Option<(TurnedBox, Anchors, Anchors)> {
    let q = b.quarter(tol)?;
    let sq = b.squared(q);
    let (x, y) = match q {
        0 => (u, v),
        1 => (v.reversed(), u),
        2 => (u.reversed(), v.reversed()),
        _ => (v, u.reversed()),
    };
    Some((sq, x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_place_is_relative_to_the_larger_magnitude() {
        let c = Coincide::default();
        assert!(c.same(1.0, 1.0 + 5.0e-10));
        assert!(!c.same(1.0, 1.0 + 5.0e-9));
        assert!(c.same(1.0e11, 1.0e11 + 1.0e-5), "a far place is the same within the relative rule");
        assert!(!c.same(1.0e11, 1.0e11 + 1.0));
        assert!(!c.same(f64::NAN, f64::NAN) && !c.same(f64::INFINITY, f64::INFINITY));
        assert!(c.is_valid() && !Coincide { abs: -1.0, ..c }.is_valid() && !Coincide { rel: f64::NAN, ..c }.is_valid());
    }

    #[test]
    fn quarter_turns_are_exact() {
        for k in 0..8 {
            let a = f64::from(k) * FRAC_PI_2;
            let u = unit(a);
            assert!(u.x == 0.0 || u.y == 0.0, "{a}: {u:?}");
            assert_eq!(u.hypot(), 1.0);
        }
        assert_eq!(unit(FRAC_PI_2), Vec2::new(0.0, 1.0));
        assert_eq!(unit(PI), Vec2::new(-1.0, 0.0));
        assert_eq!(unit(-FRAC_PI_2), Vec2::new(0.0, -1.0));
        let u = unit(0.3);
        assert!((u.x - 0.3_f64.cos()).abs() < 1e-15 && (u.y - 0.3_f64.sin()).abs() < 1e-15);
    }

    #[test]
    fn folding_a_direction_flips_the_normal_once_per_half_turn() {
        assert_eq!(fold(0.0), (0.0, false));
        assert_eq!(fold(PI), (0.0, true));
        let (a, f) = fold(PI + 0.25);
        assert!((a - 0.25).abs() < 1e-15 && f);
        let (a, f) = fold(-0.25);
        assert!((a - (PI - 0.25)).abs() < 1e-15 && f);
        let (a, f) = fold(2.0 * PI + 0.5);
        assert!((a - 0.5).abs() < 1e-12 && !f);
        let d = Direction::new(3.0 * FRAC_PI_2).unwrap();
        assert!((d.angle() - FRAC_PI_2).abs() < 1e-15);
        assert!(Direction::new(f64::NAN).is_none());
        assert_eq!(Direction::Y.normal(), Vec2::new(0.0, 1.0));
        assert_eq!(Direction::Y.tangent(), Vec2::new(-1.0, 0.0));
    }

    #[test]
    fn a_turned_box_has_its_corners_and_bounds() {
        let b = TurnedBox { centre: Point::new(10.0, 20.0), size: Size::new(4.0, 2.0), angle: FRAC_PI_2 };
        let r = b.bounds();
        assert_eq!((r.x0, r.y0, r.x1, r.y1), (9.0, 18.0, 11.0, 22.0));
        let c = b.corners();
        assert_eq!(c[0], Point::new(11.0, 18.0));
        assert_eq!(b.quarter(1e-9), Some(1));
        let t = TurnedBox { angle: 0.5, ..b };
        assert_eq!(t.quarter(1e-9), None);
        let rb = t.bounds();
        assert!(rb.width() > 2.0 && rb.height() > 2.0);
        let square = TurnedBox::from_rect(Rect::new(0.0, 0.0, 4.0, 2.0));
        assert_eq!(square.centre, Point::new(2.0, 1.0));
        assert!(square.is_valid() && !TurnedBox { size: Size::new(-1.0, 1.0), ..square }.is_valid());
    }

    #[test]
    fn a_quarter_turned_box_maps_to_the_page_axes_with_its_flags() {
        let b = TurnedBox { centre: Point::ORIGIN, size: Size::new(4.0, 2.0), angle: FRAC_PI_2 };
        let u = Anchors { min: true, mid: false, max: false };
        let v = Anchors { min: false, mid: true, max: true };
        let (sq, x, y) = to_page_axes(&b, u, v, 1e-9).unwrap();
        assert_eq!((sq.size, sq.angle), (Size::new(2.0, 4.0), 0.0));
        assert_eq!(x, v.reversed());
        assert_eq!(y, u);
        let (_, x, y) = to_page_axes(&TurnedBox { angle: PI, ..b }, u, v, 1e-9).unwrap();
        assert_eq!((x, y), (u.reversed(), v.reversed()));
        assert!(to_page_axes(&TurnedBox { angle: 0.1, ..b }, u, v, 1e-9).is_none());
    }

    #[test]
    fn two_points_are_one_place_only_if_both_their_coordinates_are() {
        let co = Coincide::default();
        assert!(co.same_point(Point::new(1.0, 2.0), Point::new(1.0 + 1e-12, 2.0)));
        assert!(!co.same_point(Point::new(1.0, 2.0), Point::new(1.0, 5.0)));
        assert!(!co.same_point(Point::new(1.0, 2.0), Point::new(7.0, 2.0)));
    }

    #[test]
    fn a_fold_is_never_outside_its_range_whatever_the_rounding_does() {
        // Angles whose quotient by pi rounds up to a whole number leave a hair under zero, and one so small that the sum is pi leaves exactly pi.
        let (a, _) = fold(-5.0e-324);
        assert_eq!(a, 0.0);
        let (a, flipped) = fold(-1.0e-323);
        assert_eq!((a, flipped), (0.0, false));
        for k in -40..40 {
            let (a, _) = fold(k as f64 * PI + 1.0e-16);
            assert!((0.0..PI).contains(&a), "{k}: {a}");
        }
    }

    #[test]
    fn the_bounds_of_a_turned_box_are_its_corners_extent() {
        let b = TurnedBox { centre: Point::new(1.0, -2.0), size: Size::new(6.0, 4.0), angle: 0.5 };
        let r = b.bounds();
        let c = b.corners();
        let (x0, x1) = (c.iter().map(|p| p.x).fold(f64::INFINITY, f64::min), c.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max));
        let (y0, y1) = (c.iter().map(|p| p.y).fold(f64::INFINITY, f64::min), c.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max));
        assert!((r.x0 - x0).abs() < 1e-12 && (r.x1 - x1).abs() < 1e-12 && (r.y0 - y0).abs() < 1e-12 && (r.y1 - y1).abs() < 1e-12, "{r:?} {c:?}");
    }

    #[test]
    fn an_angle_of_many_turns_is_what_it_has_left_after_them() {
        let b = TurnedBox { centre: Point::ORIGIN, size: Size::new(1.0, 1.0), angle: 0.0 };
        for turns in [1.0e3, 1.0e9, 1.0e12, 1.0e15] {
            let many = TurnedBox { angle: 0.3 + turns * TAU, ..b };
            let left = TurnedBox { angle: many.angle % TAU, ..b };
            assert_eq!(many.u(), left.u(), "{turns}");
            assert_eq!(many.quarter(1.0e-3), left.quarter(1.0e-3), "{turns}");
            assert_eq!(fold(many.angle), fold(left.angle), "{turns}");
            assert_eq!(many.turn(), many.angle % TAU);
        }
        // A billion quarter turns is a whole number of turns: nothing is left, and the box is square to the page.
        let square = TurnedBox { angle: 1.0e9 * FRAC_PI_2, ..b };
        assert!(square.quarter(1.0).is_some());
        assert_eq!(TurnedBox { angle: 999_999_999.0 * FRAC_PI_2, ..b }.turn(), 999_999_999.0 * FRAC_PI_2 % TAU);
    }

    #[test]
    fn the_anchors_of_the_other_end_swap_low_and_high_and_keep_the_middle() {
        let a = Anchors { min: true, mid: false, max: false };
        assert_eq!(a.reversed(), Anchors { min: false, mid: false, max: true });
        let b = Anchors { min: false, mid: true, max: true };
        assert_eq!(b.reversed(), Anchors { min: true, mid: true, max: false });
        assert_eq!(a.reversed().reversed(), a);
    }

    #[test]
    fn a_unit_vector_is_exact_within_a_trillionth_of_a_quarter_turn_and_not_beyond_it() {
        assert_eq!(unit(FRAC_PI_2 + 5.0e-13), Vec2::new(0.0, 1.0));
        assert_eq!(unit(PI - 5.0e-13), Vec2::new(-1.0, 0.0));
        assert_eq!(unit(-FRAC_PI_2), Vec2::new(0.0, -1.0));
        assert_ne!(unit(FRAC_PI_2 + 2.0e-12).x, 0.0, "two trillionths is past the tolerance");
        // A count of quarter turns of a billion is no longer exact.
        assert_ne!(unit(1.0e9 * FRAC_PI_2).y, 0.0);
    }

    #[test]
    fn a_box_is_a_quarter_turn_within_the_tolerance_in_radians_and_not_beyond_it() {
        let b = TurnedBox { centre: Point::ORIGIN, size: Size::new(1.0, 1.0), angle: FRAC_PI_2 + 0.9e-9 };
        assert_eq!(b.quarter(1.0e-9), Some(1));
        let b = TurnedBox { angle: FRAC_PI_2 + 1.5e-9, ..b };
        assert_eq!(b.quarter(1.0e-9), None);
        let b = TurnedBox { angle: -FRAC_PI_2 - 0.9e-9, ..b };
        assert_eq!(b.quarter(1.0e-9), Some(3));
    }
}
