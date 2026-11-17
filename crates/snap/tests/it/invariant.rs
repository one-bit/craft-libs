//! The invariant: what is drawn is where the subject lands.

use craft_snap::{Anchors, Axis, Choose, Constraint, Frames, Matching, Rounding, Settings, Subject};
use kurbo::Point;
use proptest::prelude::*;

use crate::common::*;

fn angle() -> impl Strategy<Value = f64> {
    prop_oneof![Just(0.0), Just(std::f64::consts::FRAC_PI_2), Just(std::f64::consts::PI), (0.05f64..3.0)]
}

fn object_frame() -> impl Strategy<Value = (f64, f64, f64, f64, f64)> {
    (-40.0f64..40.0, -40.0f64..40.0, 1.0f64..12.0, 1.0f64..12.0, angle())
}

fn settings() -> impl Strategy<Value = Settings> {
    (any::<bool>(), 0u8..3, any::<bool>(), any::<bool>(), any::<bool>()).prop_map(|(ranked, matching, page, own, nearest)| Settings {
        choose: if ranked { Choose::Ranked } else { Choose::Nearest },
        matching: match matching {
            0 => Matching::AnyToAny,
            1 => Matching::LikeToLike,
            _ => Matching::SameSide,
        },
        frames: Frames { page: page || !own, own },
        line_span: if nearest { craft_snap::LineSpan::Nearest } else { craft_snap::LineSpan::Every },
        ..Settings::default()
    })
}

fn constraint() -> impl Strategy<Value = Constraint> {
    prop_oneof![
        3 => Just(Constraint::None),
        1 => Just(Constraint::Axis(Axis::X)),
        1 => Just(Constraint::Axis(Axis::Y)),
        1 => (-1.0f64..1.0, -1.0f64..1.0).prop_map(|(x, y)| Constraint::Ray { origin: Point::new(0.0, 0.0), dir: vec2(x + 2.0, y) }),
    ]
}

fn rounding() -> impl Strategy<Value = Rounding> {
    prop_oneof![
        3 => Just(Rounding::None),
        1 => (0.25f64..3.0).prop_map(|s| Rounding::Delta { step: vec2(s, s), from: Point::new(1.0, 2.0) }),
        1 => (0.25f64..3.0).prop_map(|s| Rounding::Position(vec2(s, s * 1.5))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn what_is_drawn_is_where_a_box_lands(
        objects in prop::collection::vec(object_frame(), 0..12),
        gx in prop::collection::vec(-40.0f64..40.0, 0..4),
        gy in prop::collection::vec(-40.0f64..40.0, 0..4),
        sub in object_frame(),
        s in settings(),
        zoom in prop_oneof![Just(0.5), Just(1.0), Just(2.0), Just(8.0)],
        c in constraint(),
        r in rounding(),
    ) {
        let objs: Vec<(u64, _)> = objects.iter().enumerate().map(|(i, o)| (i as u64, frame(o.0, o.1, o.2, o.3, o.4))).collect();
        let t = scene(&objs, &gx, &gy);
        let f = frame(sub.0, sub.1, sub.2, sub.3, sub.4);
        let q = craft_snap::Query { zoom, constraint: c, rounding: r, ..query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &[] }, &s) };
        let out = run(&t, &q);
        prop_assert_eq!(out.work.marks_dropped, 0);
        if let Err(b) = out.check(&t, &q) {
            prop_assert!(false, "{:?}\nresult: {:?}", b, out);
        }
    }

    #[test]
    fn what_is_drawn_is_where_a_point_lands(
        objects in prop::collection::vec(object_frame(), 0..12),
        gx in prop::collection::vec(-40.0f64..40.0, 0..4),
        at in (-40.0f64..40.0, -40.0f64..40.0),
        s in settings(),
        c in constraint(),
        r in rounding(),
    ) {
        let objs: Vec<(u64, _)> = objects.iter().enumerate().map(|(i, o)| (i as u64, frame(o.0, o.1, o.2, o.3, o.4))).collect();
        let t = scene(&objs, &gx, &[]);
        let q = craft_snap::Query { constraint: c, rounding: r, ..query(Subject::Point { at: Point::new(at.0, at.1), base: None }, &s) };
        let out = run(&t, &q);
        if let Err(b) = out.check(&t, &q) {
            prop_assert!(false, "{:?}\nresult: {:?}", b, out);
        }
    }
}

fn edges() -> impl Strategy<Value = craft_snap::Edges> {
    (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>())
        .prop_filter("some side moves", |(a, b, c, d)| *a || *b || *c || *d)
        .prop_map(|(a, b, c, d)| craft_snap::Edges { u_min: a, u_max: b, v_min: c, v_max: d })
}

fn resize_constraint() -> impl Strategy<Value = Constraint> {
    prop_oneof![
        3 => Just(Constraint::None),
        1 => Just(Constraint::Centre),
        1 => (0.3f64..3.0, any::<bool>()).prop_map(|(ratio, c)| Constraint::Aspect { ratio, about: if c { craft_snap::About::Centre } else { craft_snap::About::Fixed } }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn what_is_drawn_is_where_a_resize_lands(
        objects in prop::collection::vec(object_frame(), 0..12),
        gx in prop::collection::vec(-40.0f64..40.0, 0..4),
        gy in prop::collection::vec(-40.0f64..40.0, 0..4),
        sub in object_frame(),
        moving in edges(),
        s in settings(),
        zoom in prop_oneof![Just(0.5), Just(1.0), Just(2.0), Just(8.0)],
        c in resize_constraint(),
        view in prop_oneof![Just(None), (-40.0f64..0.0, -40.0f64..0.0, 0.0f64..40.0, 0.0f64..40.0).prop_map(Some)],
    ) {
        let objs: Vec<(u64, _)> = objects.iter().enumerate().map(|(i, o)| (i as u64, frame(o.0, o.1, o.2, o.3, o.4))).collect();
        let t = scene(&objs, &gx, &gy);
        let f = frame(sub.0, sub.1, sub.2, sub.3, sub.4);
        // Size match scoped to a view, or to every object.
        let mut s = s;
        let index = view.map(|(a, b, c, d)| t.in_view(kurbo::Rect::new(a, b, c, d)));
        if index.is_some() {
            let mut rule = s.kinds.rule(craft_snap::Kind::Size);
            rule.scope = craft_snap::Scope::View;
            s.kinds.set(craft_snap::Kind::Size, rule);
        }
        let q = craft_snap::Query { zoom, constraint: c, view: index.as_ref(), ..query(Subject::Edges { frame: f, moving }, &s) };
        let out = run(&t, &q);
        prop_assert_eq!(out.work.marks_dropped, 0);
        if let Err(b) = out.check(&t, &q) {
            prop_assert!(false, "{:?}\nresult: {:?}", b, out);
        }
    }
}

fn grid_frame() -> impl Strategy<Value = (f64, f64, f64, f64, f64)> {
    (-10i32..10, -6i32..6, 1i32..5, 1i32..5, prop_oneof![Just(0.0), Just(0.0), Just(std::f64::consts::FRAC_PI_2)])
        .prop_map(|(x, y, w, h, a)| (f64::from(x) * 5.0, f64::from(y) * 5.0, f64::from(w) * 2.0, f64::from(h) * 2.0, a))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    /// Scenes on a lattice, where equal gaps, shared edges and corners that coincide are common: the places, the spacing and the lines all
    /// meet, and every mark must still be true.
    #[test]
    fn what_is_drawn_is_where_it_lands_in_a_scene_of_coincidences(
        objects in prop::collection::vec(grid_frame(), 1..14),
        points in prop::collection::vec((-50.0f64..50.0, -30.0f64..30.0), 0..4),
        sub in grid_frame(),
        jitter in (-3.0f64..3.0, -3.0f64..3.0),
        s in settings(),
        c in prop_oneof![3 => Just(Constraint::None), 1 => Just(Constraint::Axis(Axis::X)), 1 => Just(Constraint::Axis(Axis::Y))],
        r in rounding(),
        first in any::<bool>(),
    ) {
        let objs: Vec<(u64, _)> = objects.iter().enumerate().map(|(i, o)| (i as u64, frame(o.0, o.1, o.2, o.3, o.4))).collect();
        let mut b = craft_snap::TargetsBuilder::new(craft_snap::Limits::default());
        for (id, f) in &objs {
            b.object(craft_snap::Id(*id), object(*f)).unwrap();
        }
        for (i, p) in points.iter().enumerate() {
            b.point(Some(craft_snap::Id(500 + i as u64)), Point::new(p.0, p.1), craft_snap::Kind::Point(0), i as u32).unwrap();
        }
        let t = b.build();
        let s = Settings {
            spacing_reach: craft_snap::SpacingReach { times: 4.0, at_least: 100.0 },
            places: if first { craft_snap::Places::First } else { craft_snap::Places::Compete { bias: 0.25 } },
            ..s
        };
        let f = frame(sub.0 + jitter.0, sub.1 + jitter.1, sub.2, sub.3, sub.4);
        let own = [Point::new(sub.0, sub.1)];
        let q = craft_snap::Query { constraint: c, rounding: r, ..query(Subject::Box { frame: f, bounds: None, u: Anchors::ALL, v: Anchors::ALL, own: &own }, &s) };
        let out = run(&t, &q);
        if let Err(b) = out.check(&t, &q) {
            prop_assert!(false, "{:?}\nresult: {:?}", b, out);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(800))]

    /// Every nudge mode lands where it says: the first target ahead, the next place or the tick, or the step as it is.
    #[test]
    fn what_is_drawn_is_where_a_nudge_lands(
        objects in prop::collection::vec(grid_frame(), 0..12),
        gx in prop::collection::vec(-50.0f64..50.0, 0..4),
        gy in prop::collection::vec(-30.0f64..30.0, 0..4),
        sub in object_frame(),
        axis_x in any::<bool>(),
        forward in any::<bool>(),
        step in 0.05f64..8.0,
        anywhere in any::<bool>(),
        fallback in 0.05f64..4.0,
        s in settings(),
    ) {
        let objs: Vec<(u64, _)> = objects.iter().enumerate().map(|(i, o)| (i as u64, frame(o.0, o.1, o.2, o.3, o.4))).collect();
        let t = scene(&objs, &gx, &gy);
        let f = frame(sub.0, sub.1, sub.2, sub.3, sub.4);
        let axis = if axis_x { Axis::X } else { Axis::Y };
        let reach = if anywhere { craft_snap::NudgeReach::Anywhere } else { craft_snap::NudgeReach::Within(step) };
        let fallback = if anywhere { fallback } else { step };
        let q = query(Subject::Nudge { frame: f, bounds: None, axis, forward, reach, fallback }, &s);
        let out = run(&t, &q);
        if let Err(b) = out.check(&t, &q) {
            prop_assert!(false, "{:?}\nresult: {:?}", b, out);
        }
        let travel = if axis_x { out.delta.x.abs() } else { out.delta.y.abs() };
        if !anywhere {
            prop_assert!(travel <= step * (1.0 + 1e-9));
        }
        prop_assert!(travel > 0.0 || fallback == 0.0);
    }
}
