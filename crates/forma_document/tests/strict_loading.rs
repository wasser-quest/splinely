use forma_document::Project;

#[test]
fn loading_rejects_unknown_fields_instead_of_losing_them() {
    assert!(Project::from_json(r#"{"machines":[]}"#).is_err());
    let mut value = serde_json::to_value(Project::default()).unwrap();
    value["nodes"] = serde_json::json!([{
        "id":"a", "name":"A", "kind":{"type":"circle","cx":0,"cy":0,"radius":10,"unsupported":true}
    }]);
    assert!(Project::from_json(&value.to_string()).is_err());
}
