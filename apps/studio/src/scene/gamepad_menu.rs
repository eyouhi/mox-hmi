use super::input::{self, GamepadState, PadButton};
use bevy::prelude::*;
use hmi_core::Screen;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MenuLevel {
    #[default]
    Root,
    Add,
}

#[derive(Debug, Clone, Copy)]
enum MenuAction {
    OpenAdd,
    Add(&'static str),
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

    if pad.pressed(PadButton::L3) && pad.just_pressed(PadButton::Create) {
        if state.open {
            close(&mut state);
        } else {
            open_root(&mut state);
        }
        return;
    }

    if !state.open {
        return;
    }

    if pad.just_pressed(PadButton::Circle) {
        if state.level == MenuLevel::Add {
            open_root(&mut state);
        } else {
            close(&mut state);
        }
        return;
    }
    if pad.just_pressed(PadButton::Create) {
        close(&mut state);
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
    state.items = vec![
        MenuEntry {
            label: add_menu_title(),
            detail: "进入组件列表".to_string(),
            action: MenuAction::OpenAdd,
        },
        MenuEntry {
            label: "新建 3D 机房画面".to_string(),
            detail: "创建并切换过去".to_string(),
            action: MenuAction::NewScreen(true),
        },
        MenuEntry {
            label: "新建 2D 空白画面".to_string(),
            detail: "创建并切换过去".to_string(),
            action: MenuAction::NewScreen(false),
        },
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
    ];
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
            detail: "放到当前视野中心".to_string(),
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
        MenuLevel::Root => "手柄快捷菜单",
        MenuLevel::Add => match crate::bridge::active_screen_snapshot() {
            Some(Screen::TwoD { .. }) => "添加 2D 图元",
            Some(Screen::ThreeD { .. }) => "添加 3D 设备",
            None => "添加组件",
        },
    };
    let hint = match state.level {
        MenuLevel::Root => "↑↓ 选择 · × 执行 · ○ 关闭（也可再按 L3+Create）",
        MenuLevel::Add => "↑↓ 选择 · × 添加到画面 · ○ 返回",
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
