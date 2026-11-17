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
    (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(|(ranked, like, page, own)| Settings {
        choose: if ranked { Choose::Ranked } else { Choose::Nearest },
        matching: if like { Matching::LikeToLike } else { Matching::AnyToAny },
        frames: Frames { page: page || !own, own },
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
        let q = craft_snap::Query { zoom, constraint: c, rounding: r, ..query(Subject::Box { frame: f, u: Anchors::ALL, v: Anchors::ALL, own: &[] }, &s) };
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
    ) {
        let objs: Vec<(u64, _)> = objects.iter().enumerate().map(|(i, o)| (i as u64, frame(o.0, o.1, o.2, o.3, o.4))).collect();
        let t = scene(&objs, &gx, &gy);
        let f = frame(sub.0, sub.1, sub.2, sub.3, sub.4);
        let q = craft_snap::Query { zoom, constraint: c, ..query(Subject::Edges { frame: f, moving }, &s) };
        let out = run(&t, &q);
        prop_assert_eq!(out.work.marks_dropped, 0);
        if let Err(b) = out.check(&t, &q) {
            prop_assert!(false, "{:?}\nresult: {:?}", b, out);
        }
    }
}
