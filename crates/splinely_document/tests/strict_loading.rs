use splinely_document::Project;

#[test]
fn loading_rejects_unknown_fields_instead_of_losing_them() {
    assert!(Project::from_json(r#"{"machines":[]}"#).is_err());
    let mut value = serde_json::to_value(Project::default()).unwrap();
    value["nodes"] = serde_json::json!([{
        "id":"a", "name":"A", "kind":{"type":"circle","cx":0,"cy":0,"radius":10,"unsupported":true}
    }]);
    assert!(Project::from_json(&value.to_string()).is_err());
}

fn code_of(value: serde_json::Value) -> &'static str {
    Project::from_value_strict(value).unwrap_err().code
}

#[test]
fn from_value_strict_matches_from_json_and_reports_stable_codes() {
    let project = Project::default();
    let value = serde_json::to_value(&project).unwrap();
    assert_eq!(Project::from_value_strict(value.clone()).unwrap(), project);
    assert_eq!(Project::from_json(&value.to_string()).unwrap(), project);

    assert_eq!(code_of(serde_json::json!([1, 2])), "invalid_project");
    assert_eq!(
        code_of(serde_json::json!({"nodes": "x"})),
        "invalid_project"
    );

    let mut unknown = value.clone();
    unknown["nodes"] = serde_json::json!([{
        "id":"a", "name":"A", "kind":{"type":"circle","cx":0,"cy":0,"radius":10,"unsupported":true}
    }]);
    let err = Project::from_value_strict(unknown).unwrap_err();
    assert_eq!(err.code, "unknown_field");
    assert!(
        err.message
            .starts_with("Unsupported field at /nodes/0/kind"),
        "{err}"
    );

    let mut invalid = value;
    invalid["width"] = serde_json::json!(-5.0);
    assert_eq!(code_of(invalid), "validation_error");
}

#[test]
fn unknown_fields_suggest_updating_splinely() {
    let mut value = serde_json::to_value(splinely_document::Project::default()).unwrap();
    value["futureFeature"] = serde_json::json!(true);
    let err = splinely_document::Project::from_json(&value.to_string()).unwrap_err();
    assert!(
        err.0.contains("Unsupported field at /futureFeature"),
        "{}",
        err.0
    );
    assert!(err.0.contains("newer Splinely"), "{}", err.0);
    assert!(err.0.contains(env!("CARGO_PKG_VERSION")), "{}", err.0);
}
