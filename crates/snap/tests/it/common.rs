//! Scenes, queries and the helpers the integration tests share.

use craft_snap::{
    Anchors, Axis, Budget, Constraint, Frames, Id, Kind, KindRule, Limits, NoDetail, Object, Parts, Query, Reach, Rounding, Scope, Settings, Snapped,
    Subject, Targets, TargetsBuilder, TurnedBox,
};
use kurbo::{Point, Rect, Size, Vec2};

pub fn frame(x: f64, y: f64, w: f64, h: f64, angle: f64) -> TurnedBox {
    TurnedBox { centre: Point::new(x, y), size: Size::new(w, h), angle }
}

pub fn object(frame: TurnedBox) -> Object {
    Object { frame, bounds: None, angle: None, parts: Parts::ALL }
}

/// A set of targets: objects with their ids, and guides at the given places on each page axis.
pub fn scene(objects: &[(u64, TurnedBox)], guides_x: &[f64], guides_y: &[f64]) -> Targets {
    let mut b = TargetsBuilder::new(Limits::default());
    for (id, f) in objects {
        b.object(Id(*id), object(*f)).unwrap();
    }
    for g in guides_x {
        b.line(Some(Id(1000)), Axis::X, *g, None, Kind::Guide).unwrap();
    }
    for g in guides_y {
        b.line(Some(Id(2000)), Axis::Y, *g, None, Kind::Guide).unwrap();
    }
    b.build()
}

pub fn query<'a>(subject: Subject<'a>, settings: &'a Settings) -> Query<'a> {
    Query {
        subject,
        zoom: 1.0,
        settings,
        constraint: Constraint::None,
        rounding: Rounding::None,
        lattices: &[],
        view: None,
        budget: Budget::default(),
    }
}

pub fn run(t: &Targets, q: &Query<'_>) -> Snapped {
    craft_snap::snap(t, q, &mut NoDetail)
}

pub fn box_subject<'a>(f: TurnedBox) -> Subject<'a> {
    Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &[] }
}

pub fn vec2(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}

/// Every kind there is, so that a test turns on only what it asks for.
pub const KINDS: [Kind; 22] = [
    Kind::Guide,
    Kind::Frame,
    Kind::Layout,
    Kind::Edge,
    Kind::Centre,
    Kind::Grid,
    Kind::Ruler,
    Kind::Line(0),
    Kind::Spacing,
    Kind::Size,
    Kind::Angle,
    Kind::AngleStep,
    Kind::Point(0),
    Kind::Point(1),
    Kind::Corner,
    Kind::SideMiddle,
    Kind::Vertex,
    Kind::Midpoint,
    Kind::Nearest,
    Kind::Intersection,
    Kind::Tangent,
    Kind::Perpendicular,
];

/// Settings with every kind off but the ones given, each with its reach and priority (the lower number wins under `Choose::Ranked`).
pub fn only(on: &[(Kind, Reach, u8)]) -> Settings {
    let mut s = Settings { frames: Frames { page: true, own: false }, ..Settings::default() };
    for k in KINDS {
        s.kinds.set(k, KindRule { on: false, reach: Reach::Screen(6.0), priority: 1, scope: Scope::All });
    }
    for (k, reach, priority) in on {
        s.kinds.set(*k, KindRule { on: true, reach: *reach, priority: *priority, scope: Scope::All });
    }
    s
}

pub fn rect_box(r: Rect) -> TurnedBox {
    TurnedBox::from_rect(r)
}

/// Targets of boxes (with their ids), guides across x and layout lines, built as an app would: the boxes give edges, centres and gaps.
pub fn world(boxes: &[(u64, Rect)], guides_x: &[f64], layout: &[(Axis, f64)]) -> craft_snap::Targets {
    let mut b = TargetsBuilder::new(craft_snap::Limits::default());
    for (id, r) in boxes {
        b.object(Id(*id), Object { frame: rect_box(*r), bounds: Some(*r), angle: Some(0.0), parts: Parts::ALL }).unwrap();
    }
    for g in guides_x {
        b.line(None, Axis::X, *g, None, Kind::Guide).unwrap();
    }
    for (axis, at) in layout {
        b.line(None, *axis, *at, None, Kind::Layout).unwrap();
    }
    b.build()
}
