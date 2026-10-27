use splinely_document::*;

fn step(h: &mut History, p: &mut Project, i: usize) {
    h.execute(
        p,
        Command::AddNode(Node::circle(format!("c{i}"), 0.0, 0.0, 1.0)),
    )
    .unwrap();
    h.set_last_label(format!("step {i}"));
}

#[test]
fn step_limit_drops_only_the_oldest_steps() {
    let mut p = Project::default();
    let mut h = History::default();
    h.set_limits(HistoryLimits {
        max_steps: Some(3),
        max_bytes: None,
    });
    for i in 0..5 {
        step(&mut h, &mut p, i);
    }
    assert_eq!(h.undo_len(), 3);
    assert_eq!(h.take_dropped(), 2);
    assert_eq!(h.take_dropped(), 0);
    assert_eq!(h.labels().0, vec!["step 2", "step 3", "step 4"]);
    // The retained steps still undo correctly, back to the state after step 1.
    while h.undo(&mut p) {}
    let ids: Vec<&str> = p.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["c0", "c1"]);
    assert_eq!(h.redo_len(), 3);
    assert!(h.redo(&mut p));
    assert_eq!(p.nodes.len(), 3);
}

#[test]
fn byte_limit_keeps_the_newest_step_and_counts_redo() {
    let mut p = Project::default();
    let mut h = History::default();
    h.set_limits(HistoryLimits {
        max_steps: None,
        max_bytes: Some(250),
    });
    for i in 0..3 {
        step(&mut h, &mut p, i);
        h.set_last_size(100);
    }
    assert_eq!(h.undo_len(), 2);
    assert_eq!(h.bytes(), 200);
    assert_eq!(h.take_dropped(), 1);
    // One oversized step is still kept on its own.
    step(&mut h, &mut p, 3);
    h.set_last_size(1000);
    assert_eq!(h.undo_len(), 1);
    assert_eq!(h.bytes(), 1000);
    assert!(h.undo(&mut p));
    assert_eq!(h.bytes(), 1000, "undone steps still occupy memory");
    assert!(h.redo(&mut p));
    assert_eq!(h.undo_len(), 1);
}

#[test]
fn lowering_limits_trims_immediately_and_default_is_unlimited() {
    let mut p = Project::default();
    let mut h = History::default();
    for i in 0..20 {
        step(&mut h, &mut p, i);
    }
    assert_eq!(h.undo_len(), 20);
    assert_eq!(h.take_dropped(), 0);
    h.set_limits(HistoryLimits {
        max_steps: Some(5),
        max_bytes: None,
    });
    assert_eq!(h.undo_len(), 5);
    assert_eq!(h.take_dropped(), 15);
    assert_eq!(h.labels().0.first(), Some(&"step 15"));
    h.clear();
    assert_eq!((h.undo_len(), h.redo_len(), h.bytes()), (0, 0, 0));
}
