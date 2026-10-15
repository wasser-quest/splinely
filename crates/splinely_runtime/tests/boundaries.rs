use splinely_document::*;
use splinely_runtime::*;
fn event(name: &str, target: Option<&str>) -> Event {
    Event {
        name: name.into(),
        target: target.map(str::to_string),
    }
}
fn clip(id: &str, duration: f64, looping: bool) -> Clip {
    Clip {
        audio_cues: vec![],
        id: id.into(),
        name: String::new(),
        duration,
        looping,
        autoplay: true,
        blend_mode: BlendMode::Override,
        weight: 1.0,
        tracks: vec![],
    }
}
#[test]
fn decimal_loop_durations_do_not_leave_fatal_tick_residue() {
    for duration in [0.1, 0.2, 0.3] {
        let mut p = Project::default();
        p.clips.push(clip("loop", duration, true));
        let mut r = Runtime::new(p).unwrap();
        let response = r.tick(1.0).unwrap();
        assert_eq!(response.frame.time, 1.0);
        assert_eq!(
            response.host_actions.len(),
            (1.0 / duration).floor() as usize
        );
    }
}
#[test]
fn simultaneous_boundaries_are_queued_before_nested_emits() {
    let mut p = Project {
        clips: vec![clip("a", 1.0, false), clip("b", 1.0, false)],
        ..Project::default()
    };
    p.state_machines.push(StateMachine {
        id: "m".into(),
        initial: "s".into(),
        states: vec![State {
            id: "s".into(),
            on_enter: vec![],
            overrides: vec![],
            on_exit: vec![],
            announce: None,
        }],
        transitions: vec![Transition {
            from: "s".into(),
            to: "s".into(),
            event: event("clip.end", Some("a")),
            guard: None,
            actions: vec![Action::Emit {
                event: event("nested", None),
            }],
            blend: 0.0,
            after: None,
        }],
    });
    let out = Runtime::new(p).unwrap().tick(1.0).unwrap();
    let events: Vec<_> = out
        .host_actions
        .into_iter()
        .filter_map(|a| match a {
            HostAction::Event { event } => Some(event),
            _ => None,
        })
        .collect();
    assert_eq!(
        events,
        [
            event("clip.end", Some("a")),
            event("clip.end", Some("b")),
            event("nested", None)
        ]
    );
}
