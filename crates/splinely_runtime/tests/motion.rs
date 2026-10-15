use serde_json::json;
use splinely_document::{Action, AnimValue, Event, Property};
use splinely_runtime::{Request, Runtime};

fn x(runtime: &Runtime) -> f64 {
    runtime
        .evaluate()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == "ball")
        .unwrap()
        .transform
        .x
}
#[test]
fn motion_progress_blends_states_clips_and_explicit_actions_in_native_runtime() {
    let project = json!({"nodes":[
        {"id":"route","visible":false,"kind":{"type":"path","d":"M0 0 L100 0","bounds":[0,0,100,0]}},
        {"id":"ball","kind":{"type":"circle","cx":0,"cy":0,"radius":3},"motionPath":{"path":"route","orient":true}}
    ],"clips":[{"id":"travel","duration":1,"autoplay":true,"blendMode":"weighted","weight":0.5,"tracks":[
        {"nodeId":"ball","property":"motionProgress","keyframes":[{"time":0,"value":{"type":"number","value":1}}]}
    ]}],"stateMachines":[{"id":"mood","initial":"idle","states":[{"id":"idle"},{"id":"active","overrides":[
        {"nodeId":"ball","property":"motionProgress","value":{"type":"number","value":0.4}}
    ]}],"transitions":[{"from":"idle","to":"active","event":{"name":"go"},"blend":1}]}]});
    let mut runtime = Runtime::from_json(&project.to_string()).unwrap();
    assert!((x(&runtime) - 50.).abs() < 1e-8);
    runtime
        .event(Event {
            name: "go".into(),
            target: None,
        })
        .unwrap();
    runtime.tick(0.5).unwrap();
    assert!((x(&runtime) - 60.).abs() < 1e-8);
    runtime.tick(0.5).unwrap();
    assert!((x(&runtime) - 70.).abs() < 1e-8);
    runtime
        .execute(Request::Action {
            action: Action::SetProperty {
                node_id: "ball".into(),
                property: Property::MotionProgress,
                value: AnimValue::Number(0.9),
            },
        })
        .unwrap();
    assert!((x(&runtime) - 90.).abs() < 1e-8);
    assert!(runtime
        .execute(Request::Action {
            action: Action::SetProperty {
                node_id: "ball".into(),
                property: Property::MotionProgress,
                value: AnimValue::Number(2.)
            }
        })
        .is_err());
    assert!((x(&runtime) - 90.).abs() < 1e-8);
}
