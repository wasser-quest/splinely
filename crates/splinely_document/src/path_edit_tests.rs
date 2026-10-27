use super::*;

fn segs(d: &str) -> Vec<Segment> {
    parse(d).unwrap()
}
fn a(i: usize) -> Handle {
    Handle::anchor(i)
}
fn d(s: &[Segment]) -> String {
    path_data(s)
}
fn close(p: Point, q: Point) -> bool {
    (p[0] - q[0]).abs() < 1e-9 && (p[1] - q[1]).abs() < 1e-9
}
fn identity(p: Point) -> Option<Point> {
    Some(p)
}

#[test]
fn parse_round_trips_and_inserts_moves_after_close() {
    assert_eq!(d(&segs("M0 0 L10 0 L10 10 Z")), "M0 0 L10 0 L10 10 Z");
    assert_eq!(d(&segs("M0 0 L10 0 Z L5 5")), "M0 0 L10 0 Z M0 0 L5 5");
    assert!(parse("M0 0 L").is_err());
}

#[test]
fn translating_an_anchor_carries_both_adjacent_controls() {
    let s = segs("M0 0 C10 0 20 10 30 10 C40 10 50 0 60 0");
    let out = translate_anchors(&s, &[a(1)], [5., 5.]).unwrap();
    assert_eq!(d(&out), "M0 0 C10 0 25 15 35 15 C45 15 50 0 60 0");
    // Open endpoints carry their single control.
    let out = translate_anchors(&s, &[a(0)], [1., 0.]).unwrap();
    assert_eq!(d(&out), "M1 0 C11 0 20 10 30 10 C40 10 50 0 60 0");
}

#[test]
fn translating_the_start_of_a_closed_curve_moves_the_coincident_end() {
    let s = segs("M0 0 C0 -10 10 -10 10 0 C10 10 0 10 0 0 Z");
    let out = translate_anchors(&s, &[a(0)], [1., 2.]).unwrap();
    assert_eq!(d(&out), "M1 2 C1 -8 10 -10 10 0 C10 10 1 12 1 2 Z");
    // Selecting the coincident end is the same node.
    assert_eq!(translate_anchors(&s, &[a(2)], [1., 2.]).unwrap(), out);
    assert_eq!(translate_anchors(&s, &[a(0), a(2)], [1., 2.]).unwrap(), out);
    // Implicit closing lines have no controls to carry.
    let poly = segs("M0 0 L10 0 L10 10 Z");
    let out = translate_anchors(&poly, &[a(0)], [1., 1.]).unwrap();
    assert_eq!(d(&out), "M1 1 L10 0 L10 10 Z");
}

#[test]
fn multi_point_translation_moves_shared_points_once() {
    let s = segs("M0 0 Q5 10 10 0");
    let out = translate_anchors(&s, &[a(0), a(1)], [1., 1.]).unwrap();
    assert_eq!(d(&out), "M1 1 Q6 11 11 1");
    // Translating every anchor equals translating the whole path.
    let s = segs("M0 0 C1 2 3 4 5 6 L7 8 Q9 10 11 12 Z M20 20 L30 30");
    let all: Vec<Handle> = anchors(&s).into_iter().map(|(h, _)| h).collect();
    let out = translate_anchors(&s, &all, [3., -2.]).unwrap();
    for ((h0, p0), (h1, p1)) in handles(&s).into_iter().zip(handles(&out)) {
        assert_eq!(h0, h1);
        assert!(close([p0[0] + 3., p0[1] - 2.], p1), "{h0:?}");
    }
    // A control in the list moves alone.
    let s = segs("M0 0 C10 0 20 10 30 10");
    let out = translate_anchors(
        &s,
        &[Handle {
            segment: 1,
            kind: HandleKind::SecondControl,
        }],
        [0., 5.],
    )
    .unwrap();
    assert_eq!(d(&out), "M0 0 C10 0 20 15 30 10");
}

#[test]
fn translation_rejects_missing_handles_and_bad_deltas() {
    let s = segs("M0 0 L10 0 Z");
    assert!(translate_anchors(&s, &[a(2)], [1., 1.]).is_err()); // Close has no anchor.
    assert!(translate_anchors(&s, &[a(9)], [1., 1.]).is_err());
    assert!(translate_anchors(
        &s,
        &[Handle {
            segment: 1,
            kind: HandleKind::FirstControl
        }],
        [1., 1.]
    )
    .is_err());
    assert!(translate_anchors(&s, &[a(0)], [f64::NAN, 0.]).is_err());
    assert!(translate_anchors(&s, &[a(0)], [2e7, 0.]).is_err());
}

#[test]
fn marquee_selects_anchors_inside_the_mapped_rect() {
    let s = segs("M0 0 L10 0 L10 10 Z");
    assert_eq!(
        anchors_in_rect(&s, [11., 11.], [5., -1.], identity),
        vec![a(1), a(2)]
    );
    assert!(anchors_in_rect(&s, [20., 20.], [30., 30.], identity).is_empty());
    // Screen mapping is applied (zoom 2 + offset 100).
    let map = |p: Point| Some([p[0] * 2. + 100., p[1] * 2. + 100.]);
    assert_eq!(
        anchors_in_rect(&s, [115., 95.], [125., 125.], map),
        vec![a(1), a(2)]
    );
    assert!(anchors_in_rect(&s, [0., 0.], [1., 1.], |_| None).is_empty());
    // A coincident closing end is the same node as its start: listed once.
    let curve = segs("M0 0 C0 -10 10 -10 10 0 C10 10 0 10 0 0 Z");
    assert_eq!(
        anchors_in_rect(&curve, [-1., -1.], [1., 1.], identity),
        vec![a(0)]
    );
    assert_eq!(canonical_anchor(&curve, a(2)), Some(a(0)));
    assert_eq!(canonical_anchor(&curve, a(1)), Some(a(1)));
    assert_eq!(canonical_anchor(&curve, a(3)), None);
}

#[test]
fn hit_testing_prefers_anchors_on_ties() {
    let control = Handle {
        segment: 1,
        kind: HandleKind::SecondControl,
    };
    // Pen click: the incoming control sits exactly on its anchor and is listed first.
    assert_eq!(pick_handle([(control, 2.), (a(1), 2.)], 8.), Some(a(1)));
    assert_eq!(pick_handle([(control, 2.), (a(1), 2.3)], 8.), Some(a(1)));
    assert_eq!(pick_handle([(control, 1.), (a(1), 3.)], 8.), Some(control));
    assert_eq!(pick_handle([(a(0), 3.), (a(1), 2.9)], 8.), Some(a(1)));
    assert_eq!(pick_handle([(a(1), 9.)], 8.), None);
    assert_eq!(pick_handle([(a(1), f64::NAN)], 8.), None);
    assert_eq!(pick_handle([(a(1), 1.)], -1.), None);
    // Same through real handles of a pen-style path.
    let s = segs("M0 0 C0 0 10 10 10 10");
    let candidates = handles(&s)
        .into_iter()
        .map(|(h, p)| (h, (p[0] - 10.).hypot(p[1] - 10.)));
    assert_eq!(pick_handle(candidates, 8.), Some(a(1)));
}

#[test]
fn deleting_interior_anchors_merges_segments_keeping_outer_controls() {
    let s = segs("M0 0 C0 10 10 10 10 0 C10 -10 20 -10 20 0");
    assert_eq!(
        d(&delete_anchors(&s, &[a(1)]).unwrap()),
        "M0 0 C0 10 20 -10 20 0"
    );
    let s = segs("M0 0 L10 0 L20 0");
    assert_eq!(d(&delete_anchors(&s, &[a(1)]).unwrap()), "M0 0 L20 0");
    assert_eq!(d(&delete_anchors(&s, &[a(0)]).unwrap()), "M10 0 L20 0");
    assert_eq!(d(&delete_anchors(&s, &[a(2)]).unwrap()), "M0 0 L10 0");
    // A line contributes a retracted outer control.
    let s = segs("M0 0 L10 0 C15 5 20 5 20 0");
    assert_eq!(
        d(&delete_anchors(&s, &[a(1)]).unwrap()),
        "M0 0 C0 0 20 5 20 0"
    );
    // A run of anchors merges into one edge.
    let s = segs("M0 0 C0 5 5 5 5 0 L10 0 C15 0 20 5 20 0");
    assert_eq!(
        d(&delete_anchors(&s, &[a(1), a(2)]).unwrap()),
        "M0 0 C0 5 20 5 20 0"
    );
}

#[test]
fn deleting_in_closed_subpaths_and_dropping_degenerate_ones() {
    let square = segs("M0 0 L10 0 L10 10 L0 10 Z");
    assert_eq!(
        d(&delete_anchors(&square, &[a(0)]).unwrap()),
        "M10 0 L10 10 L0 10 Z"
    );
    assert_eq!(
        d(&delete_anchors(&square, &[a(3), a(0)]).unwrap()),
        "M10 0 L10 10 Z"
    );
    let curve = segs("M0 0 C0 -10 10 -10 10 0 C20 0 20 10 10 10 C0 10 -5 5 0 0 Z");
    assert_eq!(
        d(&delete_anchors(&curve, &[a(1)]).unwrap()),
        "M0 0 C0 -10 20 10 10 10 C0 10 -5 5 0 0 Z"
    );
    // Deleting the start of a closed curve merges across the closing edge.
    assert_eq!(
        d(&delete_anchors(&curve, &[a(0)]).unwrap()),
        "M10 0 C20 0 20 10 10 10 C0 10 10 -10 10 0 Z"
    );
    // A subpath with fewer than two anchors disappears; others stay.
    let two = segs("M0 0 L10 0 M20 20 L30 30 L40 40");
    assert_eq!(
        d(&delete_anchors(&two, &[a(1)]).unwrap()),
        "M20 20 L30 30 L40 40"
    );
    assert!(delete_anchors(&segs("M0 0 L10 0"), &[a(0)]).is_err());
    assert!(delete_anchors(&two, &[]).is_err());
    assert!(delete_anchors(&two, &[a(7)]).is_err());
}

#[test]
fn node_types_align_or_derive_handles() {
    let s = segs("M-10 0 C-10 0 -3 4 0 0 C10 0 20 0 30 0");
    assert_eq!(node_type(&s, a(1)), Some(NodeType::Cusp));
    let (out, anchors_after) = set_node_type(&s, &[a(1)], NodeType::Smooth).unwrap();
    assert_eq!(anchors_after, vec![a(1)]);
    assert_eq!(node_type(&out, a(1)), Some(NodeType::Smooth));
    let hin = handle_point(
        &out,
        Handle {
            segment: 1,
            kind: HandleKind::SecondControl,
        },
    )
    .unwrap();
    let hout = handle_point(
        &out,
        Handle {
            segment: 2,
            kind: HandleKind::FirstControl,
        },
    )
    .unwrap();
    assert!((hin[0].hypot(hin[1]) - 5.).abs() < 1e-9);
    assert!((hout[0].hypot(hout[1]) - 10.).abs() < 1e-9);
    // Outer controls and anchors are untouched.
    assert_eq!(
        handle_point(
            &out,
            Handle {
                segment: 1,
                kind: HandleKind::FirstControl
            }
        ),
        Some([-10., 0.])
    );
    assert_eq!(handle_point(&out, a(2)), Some([30., 0.]));

    let (sym, _) = set_node_type(&s, &[a(1)], NodeType::Symmetric).unwrap();
    assert_eq!(node_type(&sym, a(1)), Some(NodeType::Symmetric));
    let hin = handle_point(
        &sym,
        Handle {
            segment: 1,
            kind: HandleKind::SecondControl,
        },
    )
    .unwrap();
    assert!((hin[0].hypot(hin[1]) - 7.5).abs() < 1e-9);

    // Cusp ("split tangents") keeps geometry: handles are already independent.
    assert_eq!(set_node_type(&s, &[a(1)], NodeType::Cusp).unwrap().0, s);
}

#[test]
fn node_types_on_corners_create_handles_from_neighbours() {
    let s = segs("M0 0 L10 0 L20 10");
    assert_eq!(node_type(&s, a(1)), Some(NodeType::Cusp));
    let (out, _) = set_node_type(&s, &[a(1)], NodeType::Smooth).unwrap();
    assert_ne!(node_type(&out, a(1)), Some(NodeType::Cusp));
    // Lines became cubics but the far controls still describe the old lines.
    assert!(matches!(out[1], Segment::Cubic { .. }) && matches!(out[2], Segment::Cubic { .. }));

    let (auto, _) = set_node_type(&s, &[a(1)], NodeType::Auto).unwrap();
    let hout = handle_point(
        &auto,
        Handle {
            segment: 2,
            kind: HandleKind::FirstControl,
        },
    )
    .unwrap();
    let hin = handle_point(
        &auto,
        Handle {
            segment: 1,
            kind: HandleKind::SecondControl,
        },
    )
    .unwrap();
    let u = [20. / 500f64.sqrt(), 10. / 500f64.sqrt()];
    let lout = 200f64.sqrt() / 3.;
    assert!(close(hout, [10. + u[0] * lout, u[1] * lout]), "{hout:?}");
    assert!(
        close(hin, [10. - u[0] * 10. / 3., -u[1] * 10. / 3.]),
        "{hin:?}"
    );

    // Endpoints: smooth/symmetric/cusp are no-ops; auto aims at the neighbour.
    for ty in [NodeType::Cusp, NodeType::Smooth, NodeType::Symmetric] {
        assert_eq!(set_node_type(&s, &[a(0)], ty).unwrap().0, s);
    }
    let (end, _) = set_node_type(&s, &[a(0)], NodeType::Auto).unwrap();
    let first = handle_point(
        &end,
        Handle {
            segment: 1,
            kind: HandleKind::FirstControl,
        },
    )
    .unwrap();
    assert!(close(first, [10. / 3., 0.]), "{first:?}");
    assert_eq!(end[2..], s[2..]);
}

#[test]
fn node_type_on_a_closed_start_reports_shifted_handles() {
    // The implicit closing line becomes an explicit cubic, shifting later segments.
    let s = segs("M0 0 L10 0 L10 10 Z M50 50 L60 50 L60 60");
    let (out, after) = set_node_type(&s, &[a(0), a(5)], NodeType::Smooth).unwrap();
    assert_eq!(after[0], a(0));
    assert_eq!(handle_point(&out, after[1]), Some([60., 50.]));
    assert_ne!(node_type(&out, after[0]), Some(NodeType::Cusp));
    assert_ne!(node_type(&out, after[1]), Some(NodeType::Cusp));
    assert!(matches!(out[4], Segment::Close));
}

#[test]
fn retracting_handles_turns_fully_retracted_edges_into_lines() {
    let s = segs("M0 0 C0 10 10 10 10 0 C10 -10 20 -10 20 0");
    let (out, _) = retract_handles(&s, &[a(1)]).unwrap();
    assert_eq!(d(&out), "M0 0 C0 10 10 0 10 0 C10 0 20 -10 20 0");
    let (out, _) = retract_handles(&segs("M0 0 C0 10 10 10 10 0"), &[a(0), a(1)]).unwrap();
    assert_eq!(d(&out), "M0 0 L10 0");
    // Lines stay lines; quadratics are elevated before retracting one side.
    let (out, _) = retract_handles(&segs("M0 0 L10 0"), &[a(1)]).unwrap();
    assert_eq!(d(&out), "M0 0 L10 0");
    let (out, _) = retract_handles(&segs("M0 0 Q15 30 30 0"), &[a(1)]).unwrap();
    assert_eq!(d(&out), "M0 0 C10 20 30 0 30 0");
}

#[test]
fn extending_handles_fills_only_retracted_sides() {
    let s = segs("M0 0 L10 0 L20 10");
    let (out, _) = extend_handles(&s, &[a(1)]).unwrap();
    assert_ne!(node_type(&out, a(1)), Some(NodeType::Cusp));
    let s = segs("M0 0 C0 5 10 5 10 0 L20 0");
    let (out, _) = extend_handles(&s, &[a(1)]).unwrap();
    assert_eq!(
        handle_point(
            &out,
            Handle {
                segment: 1,
                kind: HandleKind::SecondControl
            }
        ),
        Some([10., 5.])
    );
    let hout = handle_point(
        &out,
        Handle {
            segment: 2,
            kind: HandleKind::FirstControl,
        },
    )
    .unwrap();
    assert!(close(hout, [10. + 10. / 3., 0.]), "{hout:?}");
}

#[test]
fn breaking_splits_open_and_opens_closed_subpaths() {
    let s = segs("M0 0 L10 0 L20 0");
    assert_eq!(d(&break_at(&s, a(1)).unwrap()), "M0 0 L10 0 M10 0 L20 0");
    assert!(break_at(&s, a(0)).is_err());
    assert!(break_at(&s, a(2)).is_err());
    let tri = segs("M0 0 L10 0 L10 10 Z");
    assert_eq!(d(&break_at(&tri, a(1)).unwrap()), "M10 0 L10 10 L0 0 L10 0");
    let square = segs("M0 0 L10 0 L10 10 L0 10 Z");
    assert_eq!(
        d(&break_at_anchors(&square, &[a(2), a(0)]).unwrap()),
        "M0 0 L10 0 L10 10 M10 10 L0 10 L0 0"
    );
    let curve = segs("M0 0 C0 -10 10 -10 10 0 C10 10 0 10 0 0 Z");
    assert_eq!(
        d(&break_at(&curve, a(0)).unwrap()),
        "M0 0 C0 -10 10 -10 10 0 C10 10 0 10 0 0"
    );
}

#[test]
fn joining_endpoints_connects_merges_or_closes() {
    let two = segs("M0 0 L10 0 M20 0 L30 0");
    assert_eq!(
        d(&join_endpoints(&two, a(1), a(2)).unwrap()),
        "M0 0 L10 0 L20 0 L30 0"
    );
    assert_eq!(
        d(&join_endpoints(&two, a(0), a(2)).unwrap()),
        "M10 0 L0 0 L20 0 L30 0"
    );
    assert_eq!(
        d(&join_endpoints(&two, a(3), a(1)).unwrap()),
        "M20 0 L30 0 L10 0 L0 0"
    );
    let touching = segs("M0 0 L10 0 M10 0 L20 5");
    assert_eq!(
        d(&join_endpoints(&touching, a(1), a(2)).unwrap()),
        "M0 0 L10 0 L20 5"
    );
    let open = segs("M0 0 L10 0 L10 10");
    assert_eq!(
        d(&join_endpoints(&open, a(0), a(2)).unwrap()),
        "M0 0 L10 0 L10 10 Z"
    );
    let ring = segs("M0 0 L10 0 L10 10 L0 0");
    assert_eq!(
        d(&join_endpoints(&ring, a(3), a(0)).unwrap()),
        "M0 0 L10 0 L10 10 L0 0 Z"
    );
    let curves = segs("M0 0 C0 5 10 5 10 0 M30 0 C30 5 20 5 20 0");
    assert_eq!(
        d(&join_endpoints(&curves, a(1), a(3)).unwrap()),
        "M0 0 C0 5 10 5 10 0 L20 0 C20 5 30 5 30 0"
    );
    assert!(join_endpoints(&open, a(1), a(2)).is_err());
    assert!(join_endpoints(&open, a(2), a(2)).is_err());
    assert!(join_endpoints(&segs("M0 0 L10 0 Z M20 0 L30 0"), a(0), a(3)).is_err());
}

#[test]
fn segments_convert_between_lines_and_curves() {
    assert_eq!(
        d(&segment_to_line(&segs("M0 0 C0 5 10 5 10 0"), 1).unwrap()),
        "M0 0 L10 0"
    );
    assert_eq!(
        d(&segment_to_curve(&segs("M0 0 L30 0"), 1).unwrap()),
        "M0 0 C10 0 20 0 30 0"
    );
    assert_eq!(
        d(&segment_to_curve(&segs("M0 0 Q15 30 30 0"), 1).unwrap()),
        "M0 0 C10 20 20 20 30 0"
    );
    assert_eq!(
        d(&segment_to_curve(&segs("M0 0 L30 0 L30 30 Z"), 3).unwrap()),
        "M0 0 L30 0 L30 30 C20 20 10 10 0 0 Z"
    );
    let c = segs("M0 0 C1 2 3 4 5 6");
    assert_eq!(segment_to_curve(&c, 1).unwrap(), c);
    assert!(segment_to_line(&c, 0).is_err());
    assert!(segment_to_curve(&c, 5).is_err());

    let square = segs("M0 0 L10 0 L10 10 L0 10 Z");
    assert_eq!(edges_between(&square, &[a(0), a(1)]), vec![1]);
    assert_eq!(edges_between(&square, &[a(3), a(0)]), vec![4]);
    assert_eq!(edges_between(&square, &[a(0), a(2)]), Vec::<usize>::new());
}

#[test]
fn reversing_subpaths() {
    let s = segs("M0 0 C0 5 10 5 10 0 L20 0 M50 50 L60 60");
    let out = reverse_subpath(&s, a(1)).unwrap();
    assert_eq!(d(&out), "M20 0 L10 0 C10 5 0 5 0 0 M50 50 L60 60");
    assert_eq!(reverse_subpath(&out, a(0)).unwrap(), s);
    let tri = segs("M0 0 L10 0 L10 10 Z");
    assert_eq!(
        d(&reverse_subpath(&tri, a(0)).unwrap()),
        "M0 0 L10 10 L10 0 Z"
    );
    let curve = segs("M0 0 C0 -10 10 -10 10 0 C10 10 0 10 0 0 Z");
    assert_eq!(
        d(&reverse_subpath(&curve, a(1)).unwrap()),
        "M0 0 C0 10 10 10 10 0 C10 -10 0 -10 0 0 Z"
    );
    // Both subpaths, each once, even with several anchors selected.
    assert_eq!(
        d(&reverse_subpaths(&s, &[a(0), a(1), a(4)]).unwrap()),
        "M20 0 L10 0 C10 5 0 5 0 0 M60 60 L50 50"
    );
}

#[test]
fn copying_runs_of_points_and_pasting_them_as_subpaths() {
    let line = segs("M0 0 L10 0 L20 0 L30 0");
    assert_eq!(
        d(&copy_anchors(&line, &[a(1), a(2)]).unwrap()),
        "M10 0 L20 0"
    );
    assert_eq!(
        d(&copy_anchors(&line, &[a(0), a(1), a(3)]).unwrap()),
        "M0 0 L10 0"
    );
    assert!(copy_anchors(&line, &[a(1), a(3)]).unwrap().is_empty());
    let square = segs("M0 0 L10 0 L10 10 L0 10 Z");
    assert_eq!(
        d(&copy_anchors(&square, &[a(3), a(0)]).unwrap()),
        "M0 10 L0 0"
    );
    assert_eq!(
        d(&copy_anchors(&square, &[a(0), a(1), a(2), a(3)]).unwrap()),
        "M0 0 L10 0 L10 10 L0 10 Z"
    );
    let curve = segs("M0 0 C0 5 10 5 10 0 L20 0");
    let clip = copy_anchors(&curve, &[a(0), a(1)]).unwrap();
    assert_eq!(d(&clip), "M0 0 C0 5 10 5 10 0");

    let (out, pasted) = paste_subpaths(&line, &clip, [100., 10.]).unwrap();
    assert_eq!(
        d(&out),
        "M0 0 L10 0 L20 0 L30 0 M100 10 C100 15 110 15 110 10"
    );
    assert_eq!(pasted, vec![a(4), a(5)]);
    let (_, pasted) = paste_subpaths(&line, &d_closed(), [0., 0.]).unwrap();
    assert_eq!(pasted, vec![a(4), a(5), a(6)]);
    assert!(paste_subpaths(&line, &[], [0., 0.]).is_err());
    assert!(paste_subpaths(&line, &clip, [f64::INFINITY, 0.]).is_err());
}
fn d_closed() -> Vec<Segment> {
    segs("M0 0 L10 0 L10 10 Z")
}

#[test]
fn visible_controls_belong_to_edges_touching_the_selection() {
    let s = segs("M0 0 C1 1 2 2 10 0 C11 1 12 2 20 0 C21 1 22 2 30 0");
    let selected = BTreeSet::from([a(1)]);
    let visible: Vec<Handle> = visible_controls(&s, &selected).into_iter().collect();
    let c = |segment, kind| Handle { segment, kind };
    assert_eq!(
        visible,
        vec![
            c(1, HandleKind::FirstControl),
            c(1, HandleKind::SecondControl),
            c(2, HandleKind::FirstControl),
            c(2, HandleKind::SecondControl),
        ]
    );
    assert!(visible_controls(&s, &BTreeSet::new()).is_empty());
}

#[test]
fn malformed_structures_are_rejected_without_panicking() {
    let bad = vec![Segment::Line([1., 1.])];
    assert!(translate_anchors(&bad, &[a(0)], [1., 1.]).is_err());
    assert!(delete_anchors(&bad, &[a(0)]).is_err());
    assert!(copy_anchors(&bad, &[a(0)]).is_err());
    assert_eq!(anchors(&bad), vec![(a(0), [1., 1.])]);
    assert!(edges_between(&bad, &[a(0)]).is_empty());
    let double_close = vec![
        Segment::Move([0., 0.]),
        Segment::Line([1., 0.]),
        Segment::Close,
        Segment::Close,
    ];
    assert!(reverse_subpath(&double_close, a(0)).is_err());
}
