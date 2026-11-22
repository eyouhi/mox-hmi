use hmi_core::*;

#[test]
fn round_trip_preserves_the_demo() {
    let project = bas_demo();
    let json = to_json(&project).unwrap();
    let restored = from_json(&json).unwrap();
    assert_eq!(restored, project);
}

#[test]
fn rejects_wrong_schema_and_bad_binding_atomically() {
    let mut wrong_schema = serde_json::to_value(bas_demo()).unwrap();
    wrong_schema["schema_version"] = 2.into();
    assert!(matches!(
        from_json(&serde_json::to_string(&wrong_schema).unwrap()),
        Err(PersistenceError::Validation(_))
    ));

    let mut missing_tag = serde_json::to_value(bas_demo()).unwrap();
    missing_tag["screens"][0]["nodes"][0]["bindings"] = serde_json::json!([{
        "tag_id": "missing.tag",
        "target": "visible",
        "mapping": { "type": "direct" }
    }]);
    assert!(matches!(
        from_json(&serde_json::to_string(&missing_tag).unwrap()),
        Err(PersistenceError::Validation(_))
    ));
}

#[test]
fn rejects_unknown_fields_and_oversized_input() {
    assert!(matches!(
        from_json(r#"{"schema_version":1,"extra":true}"#),
        Err(PersistenceError::Json(_))
    ));
    let oversized = "x".repeat(MAX_IMPORT_BYTES + 1);
    assert!(matches!(
        from_json(&oversized),
        Err(PersistenceError::TooLarge)
    ));
}

#[test]
fn export_filename_uses_project_slug() {
    let mut project = bas_demo();
    assert_eq!(export_filename(&project), "bas-demo.moxhmi.json");
    project.id = "project.车站".into();
    project.name = "车站".into();
    assert_eq!(export_filename(&project), "project.moxhmi.json");
}
