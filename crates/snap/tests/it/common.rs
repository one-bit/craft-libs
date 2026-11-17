//! Scenes, queries and the helpers the integration tests share.

use craft_snap::{
    Anchors, Axis, Budget, Constraint, Id, Kind, Limits, NoDetail, Object, Parts, Query, Rounding, Settings, Snapped, Subject, Targets,
    TargetsBuilder, TurnedBox,
};
use kurbo::{Point, Size, Vec2};

pub fn frame(x: f64, y: f64, w: f64, h: f64, angle: f64) -> TurnedBox {
    TurnedBox { centre: Point::new(x, y), size: Size::new(w, h), angle }
}

pub fn object(frame: TurnedBox) -> Object {
    Object { frame, angle: None, parts: Parts::ALL }
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
    Query { subject, zoom: 1.0, settings, constraint: Constraint::None, rounding: Rounding::None, lattices: &[], budget: Budget::default() }
}

pub fn run(t: &Targets, q: &Query<'_>) -> Snapped {
    craft_snap::snap(t, q, &mut NoDetail)
}

pub fn box_subject<'a>(f: TurnedBox) -> Subject<'a> {
    Subject::Box { frame: f, u: Anchors::ALL, v: Anchors::ALL, own: &[] }
}

pub fn vec2(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}
