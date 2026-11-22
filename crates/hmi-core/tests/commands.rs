use hmi_core::*;

fn session() -> EditorSession {
    EditorSession::new(bas_demo()).unwrap()
}

fn first_2d_node(session: &EditorSession) -> UiNode {
    match &session.project().screens[0] {
        Screen::TwoD { nodes, .. } => nodes[0].clone(),
        _ => unreachable!(),
    }
}

#[test]
fn update_move_and_undo_restore_previous_snapshot() {
    let mut session = session();
    let mut node = first_2d_node(&session);
    let original = node.rect;
    node.rect.x += 120.0;
    let result = session
        .execute(EditorCommand::Update2dNode {
            screen_id: "screen.bas_2d".into(),
            node: node.clone(),
        })
        .unwrap();
    assert!(result.document_revision > 1);
    assert_eq!(first_2d_node(&session).rect.x, original.x + 120.0);
    assert!(session.undo());
    assert_eq!(first_2d_node(&session).rect, original);
    assert!(session.redo());
    assert_eq!(first_2d_node(&session).rect.x, original.x + 120.0);
}

#[test]
fn new_command_clears_redo() {
    let mut session = session();
    let mut node = first_2d_node(&session);
    node.rect.x += 10.0;
    session
        .execute(EditorCommand::Update2dNode {
            screen_id: "screen.bas_2d".into(),
            node: node.clone(),
        })
        .unwrap();
    assert!(session.undo());
    assert!(session.can_redo());
    node.rect.x += 20.0;
    session
        .execute(EditorCommand::Update2dNode {
            screen_id: "screen.bas_2d".into(),
            node,
        })
        .unwrap();
    assert!(!session.can_redo());
}

#[test]
fn invalid_update_leaves_project_and_undo_unchanged() {
    let mut session = session();
    let before = session.project().clone();
    let mut node = first_2d_node(&session);
    node.rect.width = -1.0;
    let error = session
        .execute(EditorCommand::Update2dNode {
            screen_id: "screen.bas_2d".into(),
            node,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        HmiError::InvalidCommand(_) | HmiError::UnknownNode(_)
    ));
    assert_eq!(session.project(), &before);
    assert!(!session.can_undo());
}

#[test]
fn duplicate_creates_unique_offset_node_and_selects_it() {
    let mut session = session();
    let original = first_2d_node(&session);
    let result = session
        .execute(EditorCommand::DuplicateNode {
            screen_id: "screen.bas_2d".into(),
            node_id: original.id.clone(),
        })
        .unwrap();
    let new_id = result.affected_id.unwrap();
    let copied = match &session.project().screens[0] {
        Screen::TwoD { nodes, .. } => nodes.iter().find(|node| node.id == new_id).unwrap().clone(),
        _ => unreachable!(),
    };
    assert_ne!(copied.id, original.id);
    assert_eq!(copied.rect.x, original.rect.x + 24.0);
    assert_eq!(copied.rect.y, original.rect.y + 24.0);
}

#[test]
fn deleting_referenced_tag_is_blocked() {
    let mut session = session();
    let before = session.project().clone();
    let error = session
        .execute(EditorCommand::DeleteTag {
            tag_id: "bas.supply_fan.run".into(),
        })
        .unwrap_err();
    assert!(matches!(error, HmiError::TagInUse { .. }));
    assert_eq!(session.project(), &before);
}

#[test]
fn last_screen_and_startup_screen_rules_hold() {
    let mut session = session();
    assert!(
        session
            .execute(EditorCommand::DeleteScreen {
                screen_id: "screen.bas_2d".into(),
                new_startup: None,
            })
            .is_ok()
    );
    assert!(matches!(
        session.execute(EditorCommand::DeleteScreen {
            screen_id: "screen.bas_3d".into(),
            new_startup: None,
        }),
        Err(HmiError::InvalidCommand(_))
    ));
}

#[test]
fn tag_write_is_outside_undo_and_document_revision() {
    let mut session = session();
    let doc_revision = session.document_revision();
    let autosave = session.autosave_generation();
    session
        .write_tag("bas.supply_fan.run", TagValue::Bool(true))
        .unwrap();
    assert_eq!(session.document_revision(), doc_revision);
    assert_eq!(session.autosave_generation(), autosave);
    assert_eq!(
        session.tag_snapshot().get("bas.supply_fan.run"),
        Some(&TagValue::Bool(true))
    );
    assert!(!session.undo());
}

#[test]
fn batch_updates_are_one_undo_step() {
    let mut session = session();
    let Screen::TwoD { nodes, .. } = &session.project().screens[0] else {
        unreachable!()
    };
    let mut first = nodes[0].clone();
    let mut second = nodes[1].clone();
    first.rect.x += 10.0;
    second.rect.y += 20.0;
    let result = session
        .execute_batch(vec![
            EditorCommand::Update2dNode {
                screen_id: "screen.bas_2d".into(),
                node: first.clone(),
            },
            EditorCommand::Update2dNode {
                screen_id: "screen.bas_2d".into(),
                node: second.clone(),
            },
        ])
        .unwrap();
    assert_eq!(result.affected_ids.len(), 2);
    assert!(session.undo());
    let Screen::TwoD { nodes, .. } = &session.project().screens[0] else {
        unreachable!()
    };
    assert_ne!(nodes[0].rect, first.rect);
    assert_ne!(nodes[1].rect, second.rect);
    assert!(session.redo());
    let Screen::TwoD { nodes, .. } = &session.project().screens[0] else {
        unreachable!()
    };
    assert_eq!(nodes[0].rect, first.rect);
    assert_eq!(nodes[1].rect, second.rect);
}

#[test]
fn invalid_batch_is_atomic() {
    let mut session = session();
    let before = session.project().clone();
    let mut node = first_2d_node(&session);
    node.rect.x += 10.0;
    let mut invalid = node.clone();
    invalid.rect.width = -1.0;
    let error = session
        .execute_batch(vec![
            EditorCommand::Update2dNode {
                screen_id: "screen.bas_2d".into(),
                node,
            },
            EditorCommand::Update2dNode {
                screen_id: "screen.bas_2d".into(),
                node: invalid,
            },
        ])
        .unwrap_err();
    assert!(matches!(
        error,
        HmiError::InvalidCommand(_) | HmiError::UnknownNode(_)
    ));
    assert_eq!(session.project(), &before);
    assert!(!session.can_undo());
}
