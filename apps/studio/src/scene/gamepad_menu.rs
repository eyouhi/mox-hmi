use super::input::{self, GamepadState, PadButton};
use bevy::prelude::*;
use hmi_core::Screen;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MenuLevel {
    #[default]
    Root,
    Edit,
    Add,
    Tags,
    Screens,
}

#[derive(Debug, Clone)]
enum MenuAction {
    Close,
    OpenEdit,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    OpenAdd,
    Add(&'static str),
    OpenTags,
    SelectTag { id: String, name: String },
    OpenScreens,
    OpenGamepadGuide,
    NewScreen(bool),
    CycleScreen(isize),
    Back,
}

#[derive(Debug, Clone)]
struct MenuEntry {
    label: String,
    detail: String,
    action: MenuAction,
}

#[derive(Resource, Default)]
pub struct GamepadMenuState {
    open: bool,
    level: MenuLevel,
    index: usize,
    items: Vec<MenuEntry>,
}

pub fn gamepad_menu(pad: Res<GamepadState>, mut state: ResMut<GamepadMenuState>) {
    if !pad.connected {
        close(&mut state);
        return;
    }

    if !state.open && !input::scene_input_enabled() {
        close(&mut state);
        return;
    }

    if pad.just_pressed(PadButton::Options) {
        if state.open {
            close(&mut state);
        } else {
            open_root(&mut state);
        }
        return;
    }

    if !state.open && pad.just_pressed(PadButton::Triangle) {
        open_add(&mut state);
        return;
    }

    if !state.open {
        return;
    }

    if pad.just_pressed(PadButton::Circle) {
        if matches!(
            state.level,
            MenuLevel::Edit | MenuLevel::Add | MenuLevel::Tags | MenuLevel::Screens
        ) {
            open_root(&mut state);
        } else {
            close(&mut state);
        }
        return;
    }

    if pad.just_pressed(PadButton::DpadUp) || pad.just_pressed(PadButton::DpadLeft) {
        state.index = previous_index(state.index, state.items.len());
        publish(&state);
        return;
    }
    if pad.just_pressed(PadButton::DpadDown) || pad.just_pressed(PadButton::DpadRight) {
        state.index = next_index(state.index, state.items.len());
        publish(&state);
        return;
    }
    if pad.just_pressed(PadButton::Cross) {
        let Some(entry) = state.items.get(state.index).cloned() else {
            close(&mut state);
            return;
        };
        execute(entry.action, &mut state);
    }
}

fn execute(action: MenuAction, state: &mut GamepadMenuState) {
    match action {
        MenuAction::Close => close(state),
        MenuAction::OpenEdit => open_edit(state),
        MenuAction::Undo => {
            crate::bridge::undo();
            close(state);
        }
        MenuAction::Redo => {
            crate::bridge::redo();
            close(state);
        }
        MenuAction::Copy => {
            if crate::bridge::copy_selection() {
                finish_with_notice(state, "已复制所选节点".to_string());
            } else {
                finish_with_error(state, "没有可复制的节点".to_string());
            }
        }
        MenuAction::Cut => match crate::bridge::cut_selection() {
            Ok(count) => finish_with_notice(state, format!("已剪切 {count} 个节点")),
            Err(error) => finish_with_error(state, error),
        },
        MenuAction::Paste => match crate::bridge::paste_clipboard() {
            Ok(count) => finish_with_notice(state, format!("已粘贴 {count} 个节点")),
            Err(error) => finish_with_error(state, error),
        },
        MenuAction::Duplicate => match crate::bridge::duplicate_selection() {
            Ok(count) => finish_with_notice(state, format!("已原地复制 {count} 个节点")),
            Err(error) => finish_with_error(state, error),
        },
        MenuAction::Delete => match crate::bridge::delete_selection() {
            Ok(count) => finish_with_notice(state, format!("已删除 {count} 个节点")),
            Err(error) => finish_with_error(state, error),
        },
        MenuAction::SelectAll => {
            crate::bridge::select_all_nodes();
            finish_with_notice(
                state,
                format!("已选择 {} 个节点", crate::bridge::selected_nodes().len()),
            );
        }
        MenuAction::OpenAdd => {
            open_add(state);
        }
        MenuAction::Add(kind) => {
            let label = state
                .items
                .get(state.index)
                .map(|entry| entry.label.clone())
                .unwrap_or_else(|| "组件".to_string());
            match crate::bridge::add_palette_node(kind) {
                Ok(_) => crate::bridge::set_notice(
                    crate::bridge::NoticeLevel::Info,
                    format!("已添加{label}；左摇杆拖动，方向键微调"),
                ),
                Err(error) => crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error),
            }
            close(state);
        }
        MenuAction::OpenTags => {
            open_tags(state);
        }
        MenuAction::SelectTag { id, name } => {
            *crate::bridge::SELECTED_TAG.write() = Some(id);
            crate::bridge::set_notice(
                crate::bridge::NoticeLevel::Info,
                format!("数据字典已选中：{name}；可用触控板查看和编辑详情"),
            );
            close(state);
        }
        MenuAction::OpenScreens => {
            open_screens(state);
        }
        MenuAction::OpenGamepadGuide => {
            crate::bridge::set_guide_section(crate::bridge::GuideSection::Gamepad);
            crate::bridge::set_guide_open(true);
            close(state);
        }
        MenuAction::NewScreen(three_d) => {
            match crate::bridge::create_screen(three_d) {
                Ok(_) => crate::bridge::set_notice(
                    crate::bridge::NoticeLevel::Info,
                    if three_d {
                        "已新建 3D 机房画面"
                    } else {
                        "已新建 2D 空白画面"
                    },
                ),
                Err(error) => crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error),
            }
            close(state);
        }
        MenuAction::CycleScreen(direction) => {
            input::cycle_screen(direction);
            close(state);
        }
        MenuAction::Back => open_root(state),
    }
}

fn open_root(state: &mut GamepadMenuState) {
    state.open = true;
    state.level = MenuLevel::Root;
    state.index = 0;
    let selection_count = crate::bridge::selected_nodes().len();
    state.items = vec![
        MenuEntry {
            label: "继续编辑".to_string(),
            detail: "返回画布".to_string(),
            action: MenuAction::Close,
        },
        MenuEntry {
            label: "编辑所选".to_string(),
            detail: format!("{selection_count} 个节点 · L3 连按两次可直接删除"),
            action: MenuAction::OpenEdit,
        },
        MenuEntry {
            label: "撤销".to_string(),
            detail: "上一步编辑".to_string(),
            action: MenuAction::Undo,
        },
        MenuEntry {
            label: "重做".to_string(),
            detail: "恢复刚撤销的编辑".to_string(),
            action: MenuAction::Redo,
        },
        MenuEntry {
            label: add_menu_title(),
            detail: "新组件自动错开摆放".to_string(),
            action: MenuAction::OpenAdd,
        },
        MenuEntry {
            label: "数据字典".to_string(),
            detail: "浏览 Tag 与实时值".to_string(),
            action: MenuAction::OpenTags,
        },
        MenuEntry {
            label: "画面".to_string(),
            detail: "新建或切换画面".to_string(),
            action: MenuAction::OpenScreens,
        },
        MenuEntry {
            label: "手柄操作说明".to_string(),
            detail: "查看完整键位".to_string(),
            action: MenuAction::OpenGamepadGuide,
        },
    ];
    publish(state);
}

fn open_edit(state: &mut GamepadMenuState) {
    state.open = true;
    state.level = MenuLevel::Edit;
    state.index = 0;
    state.items = vec![
        MenuEntry {
            label: "复制".to_string(),
            detail: "复制到 HMI 剪贴板".to_string(),
            action: MenuAction::Copy,
        },
        MenuEntry {
            label: "剪切".to_string(),
            detail: "移动所选节点到剪贴板".to_string(),
            action: MenuAction::Cut,
        },
        MenuEntry {
            label: "粘贴".to_string(),
            detail: "粘贴并选中新节点".to_string(),
            action: MenuAction::Paste,
        },
        MenuEntry {
            label: "原地复制".to_string(),
            detail: "不改写剪贴板".to_string(),
            action: MenuAction::Duplicate,
        },
        MenuEntry {
            label: "全选".to_string(),
            detail: "选择当前画面的可见节点".to_string(),
            action: MenuAction::SelectAll,
        },
        MenuEntry {
            label: "删除".to_string(),
            detail: "可立即撤销".to_string(),
            action: MenuAction::Delete,
        },
        MenuEntry {
            label: "返回".to_string(),
            detail: "回到手柄菜单".to_string(),
            action: MenuAction::Back,
        },
    ];
    publish(state);
}

fn open_screens(state: &mut GamepadMenuState) {
    state.open = true;
    state.level = MenuLevel::Screens;
    state.index = 0;
    state.items = vec![
        MenuEntry {
            label: "上一个画面".to_string(),
            detail: "循环切换".to_string(),
            action: MenuAction::CycleScreen(-1),
        },
        MenuEntry {
            label: "下一个画面".to_string(),
            detail: "循环切换".to_string(),
            action: MenuAction::CycleScreen(1),
        },
        MenuEntry {
            label: "新建 2D 空白画面".to_string(),
            detail: "创建并切换过去".to_string(),
            action: MenuAction::NewScreen(false),
        },
        MenuEntry {
            label: "新建 3D 机房画面".to_string(),
            detail: "创建并切换过去".to_string(),
            action: MenuAction::NewScreen(true),
        },
        MenuEntry {
            label: "返回".to_string(),
            detail: "回到手柄菜单".to_string(),
            action: MenuAction::Back,
        },
    ];
    publish(state);
}

fn open_tags(state: &mut GamepadMenuState) {
    let project = crate::bridge::project();
    let snapshot = crate::bridge::tag_snapshot();
    state.open = true;
    state.level = MenuLevel::Tags;
    state.index = 0;
    let mut items = project
        .tags
        .iter()
        .map(|tag| {
            let value = snapshot
                .get(&tag.id)
                .map(|value| value.display_text())
                .unwrap_or_else(|| "—".to_string());
            MenuEntry {
                label: tag.name.clone(),
                detail: format!("{} · {value}", tag.id),
                action: MenuAction::SelectTag {
                    id: tag.id.clone(),
                    name: tag.name.clone(),
                },
            }
        })
        .collect::<Vec<_>>();
    items.push(MenuEntry {
        label: "返回".to_string(),
        detail: "回到手柄菜单".to_string(),
        action: MenuAction::Back,
    });
    state.items = items;
    publish(state);
}

fn open_add(state: &mut GamepadMenuState) {
    let Some(screen) = crate::bridge::active_screen_snapshot() else {
        close(state);
        return;
    };
    state.open = true;
    state.level = MenuLevel::Add;
    state.index = 0;
    let mut items = crate::bridge::palette_entries_for_screen(&screen)
        .iter()
        .map(|entry| MenuEntry {
            label: format!("添加{}", entry.label),
            detail: "自动错开摆放并选中".to_string(),
            action: MenuAction::Add(entry.kind),
        })
        .collect::<Vec<_>>();
    items.push(MenuEntry {
        label: "返回".to_string(),
        detail: "回到画面菜单".to_string(),
        action: MenuAction::Back,
    });
    state.items = items;
    publish(state);
}

fn publish(state: &GamepadMenuState) {
    let title = match state.level {
        MenuLevel::Root => "手柄菜单",
        MenuLevel::Edit => "编辑所选",
        MenuLevel::Add => match crate::bridge::active_screen_snapshot() {
            Some(Screen::TwoD { .. }) => "添加 2D 图元",
            Some(Screen::ThreeD { .. }) => "添加 3D 设备",
            None => "添加组件",
        },
        MenuLevel::Tags => "数据字典",
        MenuLevel::Screens => "画面",
    };
    let hint = match state.level {
        MenuLevel::Root => "↑↓ 选择 · × 执行 · △ 新建 · L3 删除 · Options 关闭",
        MenuLevel::Edit => "↑↓ 选择 · × 执行 · ○ 返回",
        MenuLevel::Add => "↑↓ 选择 · × 添加（自动错开）· ○ 返回",
        MenuLevel::Tags => "↑↓ 浏览 · × 在数据字典中选中 · ○ 返回",
        MenuLevel::Screens => "↑↓ 选择 · × 执行 · ○ 返回",
    };
    crate::bridge::set_gamepad_menu_hud(Some(crate::bridge::GamepadMenuHud {
        title: title.to_string(),
        items: state
            .items
            .iter()
            .map(|entry| crate::bridge::GamepadMenuItemHud {
                label: entry.label.clone(),
                detail: entry.detail.clone(),
            })
            .collect(),
        index: state.index,
        hint: hint.to_string(),
    }));
}

fn finish_with_notice(state: &mut GamepadMenuState, message: String) {
    crate::bridge::set_notice(crate::bridge::NoticeLevel::Info, message);
    close(state);
}

fn finish_with_error(state: &mut GamepadMenuState, error: String) {
    crate::bridge::set_notice(crate::bridge::NoticeLevel::Error, error);
    close(state);
}

fn add_menu_title() -> String {
    match crate::bridge::active_screen_snapshot() {
        Some(Screen::TwoD { .. }) => "添加 2D 图元".to_string(),
        Some(Screen::ThreeD { .. }) => "添加 3D 设备".to_string(),
        None => "添加组件".to_string(),
    }
}

fn close(state: &mut GamepadMenuState) {
    state.open = false;
    state.level = MenuLevel::Root;
    state.index = 0;
    state.items.clear();
    crate::bridge::set_gamepad_menu_hud(None);
}

fn previous_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + len - 1) % len }
}

fn next_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + 1) % len }
}
