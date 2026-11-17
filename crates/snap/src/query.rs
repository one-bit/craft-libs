//! What a query is asked, and what an app gives besides its targets: lattices and detail on demand.

use kurbo::{BezPath, Point, Vec2};

use crate::budget::{Budget, Spend};
use crate::frame::{Axis, Id, TurnedBox};
use crate::kind::Kind;
use crate::settings::Settings;

/// Which of a box's sides move, along its own axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub u_min: bool,
    pub u_max: bool,
    pub v_min: bool,
    pub v_max: bool,
}

/// What is being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Subject<'a> {
    /// A pen click, a connector's end, the press of a draw. `base`: the previous point, for tangent and perpendicular.
    Point { at: Point, base: Option<Point> },
    /// A move. `u` and `v`: which of its own sides snap. `own`: places of the moving objects pulled with the box (their connection points).
    /// `bounds`: the box it aligns by on the page axes when the app knows a tighter one than the box round the frame's corners (for a turned
    /// shape, round its paths): the same as on `Object`. It moves with the subject.
    Box { frame: TurnedBox, bounds: Option<kurbo::Rect>, u: crate::frame::Anchors, v: crate::frame::Anchors, own: &'a [Point] },
    /// A resize, or the dragged corner of a draw (the fixed corner is where the press landed). Its sides move along its own axes.
    Edges { frame: TurnedBox, moving: Edges },
    /// A turn by a handle. `angle`: what the pointer gives, radians. `radius`: the pointer's distance from the pivot. `step`: a soft lattice of
    /// angles `(step, zero)` (kind `AngleStep`), when the app wants one.
    Rotate { pivot: Point, angle: f64, radius: f64, step: Option<(f64, f64)> },
    /// A key press along a page axis: to the first target strictly ahead, within `reach`, else by `fallback`. `bounds` as on `Box`.
    Nudge { frame: TurnedBox, bounds: Option<kurbo::Rect>, axis: Axis, forward: bool, reach: NudgeReach, fallback: f64 },
}

impl Subject<'_> {
    /// The largest coordinate of the subject (not a number that is not finite: those are refused before it is read).
    pub(crate) fn far(&self) -> f64 {
        let big = |p: Point| if p.x.is_finite() && p.y.is_finite() { p.x.abs().max(p.y.abs()) } else { 0.0 };
        match self {
            Subject::Point { at, .. } => big(*at),
            Subject::Box { frame, .. } | Subject::Edges { frame, .. } | Subject::Nudge { frame, .. } => {
                frame.corners().iter().map(|c| big(*c)).fold(0.0, f64::max)
            }
            Subject::Rotate { pivot, .. } => big(*pivot),
        }
    }
}

/// How far a nudge looks for a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NudgeReach {
    /// Only a target within this distance (one step) stops the key; with none it moves by `fallback`, which is the same step.
    Within(f64),
    /// The next snap place in the key's direction, however far; with none it moves by `fallback` (an app's one ruler tick, say).
    Anywhere,
}

/// What a resize keeps fixed when the proportions are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum About {
    /// The fixed corner or side.
    Fixed,
    /// The centre.
    Centre,
}

/// What limits the candidates before any target is looked at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Constraint {
    None,
    /// Only this page axis moves.
    Axis(Axis),
    /// The subject's reference point (a point, or a box's centre) stays on the ray. A box's own axis is a ray.
    Ray {
        origin: Point,
        dir: Vec2,
    },
    /// The box keeps its own width / height = `ratio`, about the fixed corner or side, or about its centre.
    Aspect {
        ratio: f64,
        about: About,
    },
    /// Opposite sides move together: the centre stays.
    Centre,
    /// The angle is `zero` plus a whole number of `step`s.
    Angles {
        step: f64,
        zero: f64,
    },
}

/// A lattice the result lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rounding {
    None,
    /// The distance moved from `from` (the reference point at the press: the point, or a box's centre) is a whole number of steps per page
    /// axis. A step of 0 or not finite leaves that axis alone.
    Delta {
        step: Vec2,
        from: Point,
    },
    /// The point, or the min corner of the box round the subject, lands on the lattice of these steps.
    Position(Vec2),
}

/// Lines at `origin + n * pitch` on each page axis, of one kind (`Grid` or `Ruler`, or an app's own). A pitch not finite or not above 0 has no
/// lines on that axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    pub origin: Point,
    pub pitch: Vec2,
    pub kind: Kind,
}

/// What a query is asked.
#[derive(Clone, Debug)]
pub struct Query<'a> {
    pub subject: Subject<'a>,
    /// Screen units per document unit. Not finite or not above 0: nothing snaps, and the work report says why.
    pub zoom: f64,
    pub settings: &'a Settings,
    pub constraint: Constraint,
    pub rounding: Rounding,
    /// Grids and ruler ticks, each with its kind.
    pub lattices: &'a [Lattice],
    /// The objects the person sees, for the kinds whose scope is the view (`KindRule::scope`): made by `Targets::in_view`. `None`: no view, and
    /// those kinds look at every object.
    pub view: Option<&'a crate::targets::ViewIndex>,
    pub budget: Budget,
}

/// Places and outlines an app builds only for the few objects near the subject.
pub trait Detail {
    /// The places object `id` offers besides its box's (connection points, anchors), each with its kind and number.
    fn points(&mut self, id: Id, spend: &mut Spend<'_>, out: &mut Vec<(Point, Kind, u32)>);
    /// The outline of object `id`, for vertex, midpoint, nearest, intersection, tangent and perpendicular snaps.
    fn outline(&mut self, id: Id, spend: &mut Spend<'_>, out: &mut Vec<BezPath>);
}

/// For apps that give everything to the builder.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDetail;

impl Detail for NoDetail {
    fn points(&mut self, _id: Id, _spend: &mut Spend<'_>, _out: &mut Vec<(Point, Kind, u32)>) {}
    fn outline(&mut self, _id: Id, _spend: &mut Spend<'_>, _out: &mut Vec<BezPath>) {}
}
