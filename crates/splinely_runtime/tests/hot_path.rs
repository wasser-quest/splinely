//! The pre-resolved, validate-once evaluation path must equal the public
//! validating API, and failed requests must leave no trace.
use splinely_runtime::{Request, Runtime};
use std::collections::BTreeMap;

fn examples() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut list: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".splinely.json"))
        .map(|p| {
            (
                p.display().to_string(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    list.sort();
    assert!(list.len() >= 5);
    list
}

#[test]
fn runtime_frames_match_public_core_evaluation_on_every_example() {
    for (name, source) in examples() {
        let mut runtime = Runtime::from_json(&source).unwrap();
        for step in 0..40 {
            let response = runtime.tick(0.037 * f64::from(step % 5 + 1)).unwrap();
            let frame = response.frame;
            // Rebuild the context a plain host could derive from the frame alone.
            let context = splinely_document::EvaluationContext {
                states: frame.states.clone(),
                clips: Some(
                    frame
                        .clips
                        .iter()
                        .filter(|c| c.active)
                        .map(|c| splinely_document::ClipPlayback {
                            clip_id: c.id.clone(),
                            time: c.time,
                        })
                        .collect(),
                ),
                clip_weights: frame
                    .clips
                    .iter()
                    .map(|c| (c.id.clone(), c.weight))
                    .collect(),
                variables: frame.variables.clone(),
                ..Default::default()
            };
            let project = runtime.project();
            let nodes = project.evaluate_with_context(frame.time, &context).unwrap();
            let bones = project.evaluate_bones(frame.time, &context).unwrap();
            assert_eq!(
                serde_json::to_string(&frame.nodes).unwrap(),
                serde_json::to_string(&nodes).unwrap(),
                "{name} step {step}"
            );
            assert_eq!(
                serde_json::to_string(&frame.bones).unwrap(),
                serde_json::to_string(&bones).unwrap(),
                "{name} step {step}"
            );
        }
    }
}

#[test]
fn failed_requests_roll_back_all_mutable_state() {
    let (_, source) = examples()
        .into_iter()
        .find(|(name, _)| name.ends_with("cactus-runner.splinely.json"))
        .unwrap();
    let mut runtime = Runtime::from_json(&source).unwrap();
    runtime.tick(0.4).unwrap();
    let before = serde_json::to_string(&runtime.evaluate().unwrap()).unwrap();
    for request in [
        r#"{"type":"tick","delta":-1}"#,
        r#"{"type":"action","action":{"type":"setProperty","nodeId":"missing","property":"x","value":{"type":"number","value":1}}}"#,
        r#"{"type":"playClip","clipId":"missing"}"#,
    ] {
        assert!(runtime.request_json(request).is_err(), "{request}");
        assert_eq!(
            serde_json::to_string(&runtime.evaluate().unwrap()).unwrap(),
            before
        );
    }
    // A clone is an independent snapshot of the mutable state.
    let mut twin = runtime.clone();
    twin.execute(Request::Seek { time: 1.0 }).unwrap();
    assert_eq!(
        serde_json::to_string(&runtime.evaluate().unwrap()).unwrap(),
        before
    );
    let _: BTreeMap<String, String> = twin.evaluate().unwrap().states;
}
