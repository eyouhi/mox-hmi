use crate::{
    Binding, ButtonAction, DeviceNode, HmiError, ProjectDocument, Screen, TagDefinition,
    TagSimulation, TagSnapshot, TagValue, UiNode, ValidationIssue, Widget2D, validate_project,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const UNDO_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub affected_id: Option<String>,
    pub document_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchCommandResult {
    pub affected_ids: Vec<String>,
    pub document_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditorCommand {
    AddScreen {
        screen: Screen,
    },
    DeleteScreen {
        screen_id: String,
        new_startup: Option<String>,
    },
    RenameScreen {
        screen_id: String,
        name: String,
    },
    SetStartupScreen {
        screen_id: String,
    },
    Add2dNode {
        screen_id: String,
        node: UiNode,
    },
    Update2dNode {
        screen_id: String,
        node: UiNode,
    },
    Add3dNode {
        screen_id: String,
        node: DeviceNode,
    },
    Update3dNode {
        screen_id: String,
        node: DeviceNode,
    },
    DeleteNode {
        screen_id: String,
        node_id: String,
    },
    DuplicateNode {
        screen_id: String,
        node_id: String,
    },
    SetBindings {
        screen_id: String,
        node_id: String,
        bindings: Vec<Binding>,
    },
    AddTag {
        tag: TagDefinition,
    },
    UpdateTag {
        tag: TagDefinition,
    },
    DeleteTag {
        tag_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct EditorSession {
    project: ProjectDocument,
    undo: VecDeque<ProjectDocument>,
    redo: VecDeque<ProjectDocument>,
    simulation: TagSimulation,
    document_revision: u64,
    autosave_generation: u64,
}

impl EditorSession {
    pub fn new(project: ProjectDocument) -> Result<Self, Vec<ValidationIssue>> {
        validate_project(&project)?;
        let simulation = TagSimulation::new(&project);
        Ok(Self {
            project,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            simulation,
            document_revision: 1,
            autosave_generation: 0,
        })
    }

    pub fn project(&self) -> &ProjectDocument {
        &self.project
    }

    pub fn document_revision(&self) -> u64 {
        self.document_revision
    }

    pub fn autosave_generation(&self) -> u64 {
        self.autosave_generation
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn tag_snapshot(&self) -> &TagSnapshot {
        self.simulation.snapshot()
    }

    pub fn tick_tags(&mut self, dt_ms: u64) -> bool {
        self.simulation.tick(dt_ms)
    }

    pub fn replace_tag_snapshot(
        &mut self,
        values: std::collections::BTreeMap<String, TagValue>,
        revision: u64,
    ) -> bool {
        self.simulation.replace_snapshot(values, revision)
    }

    pub fn write_tag(&mut self, tag_id: &str, value: TagValue) -> Result<(), HmiError> {
        self.simulation.write(tag_id, value)
    }

    pub fn resume_tag_simulation(&mut self, tag_id: &str) -> Result<(), HmiError> {
        self.simulation.resume(tag_id)
    }

    pub fn replace_project(
        &mut self,
        project: ProjectDocument,
    ) -> Result<(), Vec<ValidationIssue>> {
        validate_project(&project)?;
        self.project = project;
        self.undo.clear();
        self.redo.clear();
        self.simulation.replace_definitions(&self.project.tags);
        self.document_revision += 1;
        self.autosave_generation += 1;
        Ok(())
    }

    pub fn execute(&mut self, command: EditorCommand) -> Result<CommandResult, HmiError> {
        let mut candidate = self.project.clone();
        let affected_id = apply_command(&mut candidate, command)?;
        validate_candidate(&candidate)?;
        self.commit(candidate, affected_id)
    }

    pub fn execute_batch(
        &mut self,
        commands: Vec<EditorCommand>,
    ) -> Result<BatchCommandResult, HmiError> {
        if commands.is_empty() {
            return Ok(BatchCommandResult {
                affected_ids: Vec::new(),
                document_revision: self.document_revision,
            });
        }
        let mut candidate = self.project.clone();
        let mut affected_ids = Vec::new();
        for command in commands {
            if let Some(id) = apply_command(&mut candidate, command)? {
                affected_ids.push(id);
            }
        }
        validate_candidate(&candidate)?;
        self.commit_batch(candidate, affected_ids)
    }

    fn commit(
        &mut self,
        candidate: ProjectDocument,
        affected_id: Option<String>,
    ) -> Result<CommandResult, HmiError> {
        let previous = std::mem::replace(&mut self.project, candidate);
        self.undo.push_back(previous);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.document_revision += 1;
        self.autosave_generation += 1;
        self.simulation.replace_definitions(&self.project.tags);
        Ok(CommandResult {
            affected_id,
            document_revision: self.document_revision,
        })
    }

    fn commit_batch(
        &mut self,
        candidate: ProjectDocument,
        affected_ids: Vec<String>,
    ) -> Result<BatchCommandResult, HmiError> {
        let previous = std::mem::replace(&mut self.project, candidate);
        self.undo.push_back(previous);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.document_revision += 1;
        self.autosave_generation += 1;
        self.simulation.replace_definitions(&self.project.tags);
        Ok(BatchCommandResult {
            affected_ids,
            document_revision: self.document_revision,
        })
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        let current = std::mem::replace(&mut self.project, previous);
        self.redo.push_back(current);
        if self.redo.len() > UNDO_LIMIT {
            self.redo.pop_front();
        }
        self.document_revision += 1;
        self.autosave_generation += 1;
        self.simulation.replace_definitions(&self.project.tags);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop_back() else {
            return false;
        };
        let current = std::mem::replace(&mut self.project, next);
        self.undo.push_back(current);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.document_revision += 1;
        self.autosave_generation += 1;
        self.simulation.replace_definitions(&self.project.tags);
        true
    }
}

fn validate_candidate(candidate: &ProjectDocument) -> Result<(), HmiError> {
    validate_project(candidate).map_err(|errors| {
        HmiError::InvalidCommand(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })
}

fn apply_command(
    project: &mut ProjectDocument,
    command: EditorCommand,
) -> Result<Option<String>, HmiError> {
    match command {
        EditorCommand::AddScreen { screen } => {
            if project.screens.iter().any(|item| item.id() == screen.id()) {
                return Err(HmiError::DuplicateId(screen.id().into()));
            }
            let id = screen.id().to_string();
            project.screens.push(screen);
            Ok(Some(id))
        }
        EditorCommand::DeleteScreen {
            screen_id,
            new_startup,
        } => {
            if project.screens.len() <= 1 {
                return Err(HmiError::InvalidCommand("工程至少需要保留一个画面".into()));
            }
            let index = project
                .screens
                .iter()
                .position(|screen| screen.id() == screen_id)
                .ok_or_else(|| HmiError::UnknownScreen(screen_id.clone()))?;
            let deleting_startup = project.startup_screen == screen_id;
            if let Some(next) = &new_startup {
                let exists_elsewhere = project
                    .screens
                    .iter()
                    .enumerate()
                    .any(|(screen_index, screen)| screen_index != index && screen.id() == next);
                if next == &screen_id || !exists_elsewhere {
                    return Err(HmiError::InvalidCommand(format!(
                        "新的启动画面 `{next}` 无效"
                    )));
                }
            }
            project.screens.remove(index);
            if deleting_startup {
                project.startup_screen = new_startup
                    .or_else(|| {
                        project
                            .screens
                            .first()
                            .map(|screen| screen.id().to_string())
                    })
                    .ok_or_else(|| HmiError::InvalidCommand("没有可用的启动画面".into()))?;
            }
            Ok(Some(screen_id))
        }
        EditorCommand::RenameScreen { screen_id, name } => {
            if name.trim().is_empty() || name.chars().count() > 120 {
                return Err(HmiError::InvalidCommand("画面名称必须为 1–120 字符".into()));
            }
            let screen = project
                .screens
                .iter_mut()
                .find(|screen| screen.id() == screen_id)
                .ok_or_else(|| HmiError::UnknownScreen(screen_id.clone()))?;
            match screen {
                Screen::TwoD { name: current, .. } | Screen::ThreeD { name: current, .. } => {
                    *current = name;
                }
            }
            Ok(Some(screen_id))
        }
        EditorCommand::SetStartupScreen { screen_id } => {
            if !project
                .screens
                .iter()
                .any(|screen| screen.id() == screen_id)
            {
                return Err(HmiError::UnknownScreen(screen_id));
            }
            project.startup_screen = screen_id.clone();
            Ok(Some(screen_id))
        }
        EditorCommand::Add2dNode { screen_id, node } => {
            let nodes = two_d_nodes_mut(project, &screen_id)?;
            if nodes.iter().any(|item| item.id == node.id) {
                return Err(HmiError::DuplicateId(node.id));
            }
            let id = node.id.clone();
            nodes.push(node);
            Ok(Some(id))
        }
        EditorCommand::Update2dNode { screen_id, node } => {
            let nodes = two_d_nodes_mut(project, &screen_id)?;
            let current = nodes
                .iter_mut()
                .find(|item| item.id == node.id)
                .ok_or_else(|| HmiError::UnknownNode(node.id.clone()))?;
            let id = node.id.clone();
            *current = node;
            Ok(Some(id))
        }
        EditorCommand::Add3dNode { screen_id, node } => {
            let nodes = three_d_nodes_mut(project, &screen_id)?;
            if nodes.iter().any(|item| item.id == node.id) {
                return Err(HmiError::DuplicateId(node.id));
            }
            let id = node.id.clone();
            nodes.push(node);
            Ok(Some(id))
        }
        EditorCommand::Update3dNode { screen_id, node } => {
            let nodes = three_d_nodes_mut(project, &screen_id)?;
            let current = nodes
                .iter_mut()
                .find(|item| item.id == node.id)
                .ok_or_else(|| HmiError::UnknownNode(node.id.clone()))?;
            let id = node.id.clone();
            *current = node;
            Ok(Some(id))
        }
        EditorCommand::DeleteNode { screen_id, node_id } => {
            let screen = project
                .screens
                .iter_mut()
                .find(|screen| screen.id() == screen_id)
                .ok_or_else(|| HmiError::UnknownScreen(screen_id.clone()))?;
            let position = match screen {
                Screen::TwoD { nodes, .. } => nodes.iter().position(|node| node.id == node_id),
                Screen::ThreeD { nodes, .. } => nodes.iter().position(|node| node.id == node_id),
            }
            .ok_or_else(|| HmiError::UnknownNode(node_id.clone()))?;
            match screen {
                Screen::TwoD { nodes, .. } => {
                    nodes.remove(position);
                }
                Screen::ThreeD { nodes, .. } => {
                    nodes.remove(position);
                }
            }
            Ok(Some(node_id))
        }
        EditorCommand::DuplicateNode { screen_id, node_id } => {
            let screen = project
                .screens
                .iter_mut()
                .find(|screen| screen.id() == screen_id)
                .ok_or_else(|| HmiError::UnknownScreen(screen_id.clone()))?;
            match screen {
                Screen::TwoD { nodes, .. } => {
                    let index = nodes
                        .iter()
                        .position(|node| node.id == node_id)
                        .ok_or_else(|| HmiError::UnknownNode(node_id.clone()))?;
                    let mut copy = nodes[index].clone();
                    copy.id = unique_node_id(&copy.id, nodes.iter().map(|node| node.id.as_str()))?;
                    copy.name = format!("{} 副本", copy.name);
                    copy.rect.x += 24.0;
                    copy.rect.y += 24.0;
                    let id = copy.id.clone();
                    nodes.insert(index + 1, copy);
                    Ok(Some(id))
                }
                Screen::ThreeD { nodes, .. } => {
                    let index = nodes
                        .iter()
                        .position(|node| node.id == node_id)
                        .ok_or_else(|| HmiError::UnknownNode(node_id.clone()))?;
                    let mut copy = nodes[index].clone();
                    copy.id = unique_node_id(&copy.id, nodes.iter().map(|node| node.id.as_str()))?;
                    copy.name = format!("{} 副本", copy.name);
                    copy.position[0] += 0.5;
                    let id = copy.id.clone();
                    nodes.insert(index + 1, copy);
                    Ok(Some(id))
                }
            }
        }
        EditorCommand::SetBindings {
            screen_id,
            node_id,
            bindings,
        } => {
            let screen = project
                .screens
                .iter_mut()
                .find(|screen| screen.id() == screen_id)
                .ok_or_else(|| HmiError::UnknownScreen(screen_id.clone()))?;
            match screen {
                Screen::TwoD { nodes, .. } => {
                    let node = nodes
                        .iter_mut()
                        .find(|node| node.id == node_id)
                        .ok_or_else(|| HmiError::UnknownNode(node_id.clone()))?;
                    node.bindings = bindings;
                }
                Screen::ThreeD { nodes, .. } => {
                    let node = nodes
                        .iter_mut()
                        .find(|node| node.id == node_id)
                        .ok_or_else(|| HmiError::UnknownNode(node_id.clone()))?;
                    node.bindings = bindings;
                }
            }
            Ok(Some(node_id))
        }
        EditorCommand::AddTag { tag } => {
            if project.tags.iter().any(|item| item.id == tag.id) {
                return Err(HmiError::DuplicateId(tag.id));
            }
            let id = tag.id.clone();
            project.tags.push(tag);
            Ok(Some(id))
        }
        EditorCommand::UpdateTag { tag } => {
            let current = project
                .tags
                .iter_mut()
                .find(|item| item.id == tag.id)
                .ok_or_else(|| HmiError::UnknownTag(tag.id.clone()))?;
            let id = tag.id.clone();
            *current = tag;
            Ok(Some(id))
        }
        EditorCommand::DeleteTag { tag_id } => {
            if !project.tags.iter().any(|tag| tag.id == tag_id) {
                return Err(HmiError::UnknownTag(tag_id));
            }
            let nodes = referencing_nodes(project, &tag_id);
            if !nodes.is_empty() {
                return Err(HmiError::TagInUse { tag_id, nodes });
            }
            project.tags.retain(|tag| tag.id != tag_id);
            Ok(Some(tag_id))
        }
    }
}

fn two_d_nodes_mut<'a>(
    project: &'a mut ProjectDocument,
    screen_id: &str,
) -> Result<&'a mut Vec<UiNode>, HmiError> {
    match project
        .screens
        .iter_mut()
        .find(|screen| screen.id() == screen_id)
    {
        Some(Screen::TwoD { nodes, .. }) => Ok(nodes),
        Some(Screen::ThreeD { .. }) => Err(HmiError::InvalidCommand(format!(
            "画面 `{screen_id}` 不是 2D 画面"
        ))),
        None => Err(HmiError::UnknownScreen(screen_id.into())),
    }
}

fn three_d_nodes_mut<'a>(
    project: &'a mut ProjectDocument,
    screen_id: &str,
) -> Result<&'a mut Vec<DeviceNode>, HmiError> {
    match project
        .screens
        .iter_mut()
        .find(|screen| screen.id() == screen_id)
    {
        Some(Screen::ThreeD { nodes, .. }) => Ok(nodes),
        Some(Screen::TwoD { .. }) => Err(HmiError::InvalidCommand(format!(
            "画面 `{screen_id}` 不是 3D 画面"
        ))),
        None => Err(HmiError::UnknownScreen(screen_id.into())),
    }
}

pub fn unique_node_id<'a>(
    original: &str,
    existing: impl Iterator<Item = &'a str>,
) -> Result<String, HmiError> {
    let ids: std::collections::HashSet<&str> = existing.collect();
    for index in 1..=1_000 {
        let id = format!("{original}.copy{index}");
        if !ids.contains(id.as_str()) {
            return Ok(id);
        }
    }
    Err(HmiError::InvalidCommand(format!(
        "节点 `{original}` 的副本 ID 已用尽"
    )))
}

fn referencing_nodes(project: &ProjectDocument, tag_id: &str) -> Vec<String> {
    let mut nodes = Vec::new();
    for screen in &project.screens {
        match screen {
            Screen::TwoD { nodes: items, .. } => {
                for node in items {
                    let binding_ref = node.bindings.iter().any(|binding| binding.tag_id == tag_id);
                    let action_ref = matches!(
                        &node.widget,
                        Widget2D::Button {
                            action: ButtonAction::ToggleTag { tag_id: action_tag }
                                | ButtonAction::SetTag { tag_id: action_tag, .. },
                            ..
                        } if action_tag == tag_id
                    );
                    if binding_ref || action_ref {
                        nodes.push(node.id.clone());
                    }
                }
            }
            Screen::ThreeD { nodes: items, .. } => {
                for node in items {
                    if node.bindings.iter().any(|binding| binding.tag_id == tag_id) {
                        nodes.push(node.id.clone());
                    }
                }
            }
        }
    }
    nodes
}
