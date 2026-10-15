use serde_json::json;
use splinely_document::*;
use splinely_runtime::*;
fn runtime() -> Runtime {
    Runtime::from_json(include_str!(
        "../../../runtime/tests/fixtures/blend-spaces.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn variable(r: &mut Runtime, value: serde_json::Value) -> splinely_runtime::Response {
    r.execute(Request::Action {
        action: Action::SetVariable {
            name: "speed".into(),
            value,
        },
    })
    .unwrap()
}
#[test]
fn shared_fixture_live_variables_bones_fades_and_atomic_requests() {
    let mut r = runtime();
    near(r.evaluate().unwrap().nodes[0].transform.x, 42.);
    near(r.evaluate().unwrap().bones[0].x, 5.);
    near(variable(&mut r, json!(0)).frame.nodes[0].transform.x, 32.);
    near(variable(&mut r, json!(2)).frame.nodes[0].transform.x, 52.);
    variable(&mut r, json!(1));
    r.execute(Request::FadeClip {
        clip_id: "offset".into(),
        weight: 0.,
        duration: 1.,
    })
    .unwrap();
    let f = r.tick(0.5).unwrap().frame;
    near(f.nodes[0].transform.x, 41.);
    near(f.bones[0].x, 4.);
    r.execute(Request::PauseClip {
        clip_id: "offset".into(),
    })
    .unwrap();
    near(r.tick(0.5).unwrap().frame.nodes[0].transform.x, 40.);
    let before = serde_json::to_value(r.evaluate().unwrap()).unwrap();
    assert!(r
        .execute(Request::Action {
            action: Action::SetVariable {
                name: "speed".into(),
                value: json!(10000001)
            }
        })
        .is_err());
    assert_eq!(serde_json::to_value(r.evaluate().unwrap()).unwrap(), before);
    assert!(r
        .execute(Request::Action {
            action: Action::SetVariable {
                name: "speed".into(),
                value: json!("bad")
            }
        })
        .is_err());
    assert_eq!(serde_json::to_value(r.evaluate().unwrap()).unwrap(), before);
    // Variables and fades do not rewrite authored defaults.
    assert_eq!(r.project().variables["speed"], json!(1));
    near(r.project().clips[2].weight, 0.5);
}
#[test]
fn one_large_tick_equals_segmented_ticks_for_additive_fade() {
    let mut a = runtime();
    let mut b = runtime();
    for r in [&mut a, &mut b] {
        r.execute(Request::FadeClip {
            clip_id: "offset".into(),
            weight: 1.,
            duration: 2.,
        })
        .unwrap();
    }
    a.tick(1.5).unwrap();
    for _ in 0..3 {
        b.tick(0.5).unwrap();
    }
    let a = a.evaluate().unwrap();
    let b = b.evaluate().unwrap();
    near(a.nodes[0].transform.x, b.nodes[0].transform.x);
    near(a.bones[0].x, b.bones[0].x);
}
