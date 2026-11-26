use crate::bridge::{self, GuideSection};
use dioxus::html::MountedData;
use dioxus::html::ScrollBehavior;
use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, Default)]
struct AppliedGuideScroll {
    section: Option<GuideSection>,
    revision: u64,
}

#[component]
pub fn OperationGuide() -> Element {
    if !bridge::guide_open() {
        return rsx! {};
    }

    let section = bridge::guide_section();
    let gamepad = bridge::gamepad_status();
    let gamepad_text = if gamepad.connected {
        if gamepad.is_dualsense() {
            "DualSense 已连接".to_string()
        } else {
            "手柄已连接".to_string()
        }
    } else {
        "手柄未连接".to_string()
    };

    let mut body_mounted = use_signal(|| None::<Rc<MountedData>>);
    let applied_scroll = use_hook(|| Rc::new(Cell::new(AppliedGuideScroll::default())));
    use_effect(move || {
        let section = bridge::guide_section();
        let request = bridge::guide_scroll_request();
        let Some(mounted) = body_mounted.read().clone() else {
            return;
        };
        let previous = applied_scroll.get();
        let reset = previous.section != Some(section);
        if !reset && previous.revision == request.revision {
            return;
        }
        applied_scroll.set(AppliedGuideScroll {
            section: Some(section),
            revision: request.revision,
        });
        let delta = if reset { 0.0 } else { request.delta };
        spawn(async move {
            let Ok(offset) = mounted.get_scroll_offset().await else {
                return;
            };
            let max_scroll = match (
                mounted.get_scroll_size().await,
                mounted.get_client_rect().await,
            ) {
                (Ok(size), Ok(rect)) => (size.height - rect.size.height).max(0.0),
                _ => f64::MAX,
            };
            let target = if reset {
                0.0
            } else {
                (offset.y + delta).clamp(0.0, max_scroll)
            };
            let _ = mounted
                .scroll(PixelsVector2D::new(0.0, target), ScrollBehavior::Instant)
                .await;
        });
    });

    rsx! {
        div {
            class: "guide-backdrop",
            "data-testid": "guide-backdrop",
            onclick: move |_| bridge::set_guide_open(false),
            section {
                class: "guide-dialog",
                "data-testid": "guide-dialog",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "guide-title",
                onclick: move |event| event.stop_propagation(),
                header { class: "guide-head",
                    div {
                        div { class: "eyebrow", "输入与操作" }
                        h2 { id: "guide-title", class: "guide-title", "MOX HMI 操作指引" }
                        p { class: "guide-lead", "键鼠遵循设计软件习惯，手柄遵循主机布局：左摇杆移动、右摇杆观察、Options 打开菜单。2D 与 3D 共用选择和剪贴板规则，空白处普通拖动用于视图导航，框选统一使用 Option/Alt + 拖动；点击画布后键盘快捷键生效。" }
                    }
                    div { class: "guide-head-actions",
                        span {
                            class: if gamepad.connected { "controller-chip is-connected" } else { "controller-chip" },
                            "data-testid": "guide-gamepad-status",
                            "{gamepad_text}"
                        }
                        button {
                            class: "icon-button",
                            "data-testid": "guide-close",
                            aria_label: "关闭操作指引",
                            autofocus: true,
                            onclick: move |_| bridge::set_guide_open(false),
                            "×"
                        }
                    }
                }

                nav { class: "guide-tabs", aria_label: "指引章节",
                    button {
                        class: if section == GuideSection::Start { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-start",
                        aria_pressed: section == GuideSection::Start,
                        onclick: move |_| bridge::set_guide_section(GuideSection::Start),
                        "开始与画面"
                    }
                    button {
                        class: if section == GuideSection::TwoD { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-2d",
                        aria_pressed: section == GuideSection::TwoD,
                        onclick: move |_| bridge::set_guide_section(GuideSection::TwoD),
                        "2D 操作"
                    }
                    button {
                        class: if section == GuideSection::ThreeD { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-3d",
                        aria_pressed: section == GuideSection::ThreeD,
                        onclick: move |_| bridge::set_guide_section(GuideSection::ThreeD),
                        "3D 操作"
                    }
                    button {
                        class: if section == GuideSection::Gamepad { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-gamepad",
                        aria_pressed: section == GuideSection::Gamepad,
                        onclick: move |_| bridge::set_guide_section(GuideSection::Gamepad),
                        "手柄操作"
                    }
                }

                div {
                    class: "guide-body",
                    "data-testid": "guide-content",
                    onmounted: move |event| body_mounted.set(Some(event.data())),
                    match section {
                        GuideSection::Start => rsx! { GuideStart {} },
                        GuideSection::TwoD => rsx! { GuideTwoD {} },
                        GuideSection::ThreeD => rsx! { GuideThreeD {} },
                        GuideSection::Gamepad => rsx! { GuideGamepad {} },
                    }
                }

                footer { class: "guide-foot",
                    span { "F1 打开 / 关闭指引" }
                    span { "手柄：L1 / R1 或 ← / → 切换章节；Create 打开 / 关闭" }
                    span { "↑ / ↓ 或左摇杆滚动 · Circle 关闭" }
                    button {
                        class: "mini-button",
                        onclick: move |_| bridge::set_guide_open(false),
                        "知道了"
                    }
                }
            }
        }
    }
}

#[component]
fn GuideStart() -> Element {
    rsx! {
        section { class: "guide-section",
            h3 { "新建 2D 或 3D 画面" }
            ol { class: "guide-steps",
                li {
                    strong { "选择画面类型。" }
                    span { "左侧“画面”标题栏点击 +，选择“空白 2D 画面”或“3D 机房画面”；手柄按 Options 打开菜单，在“画面”中新建并切换。" }
                }
                li {
                    strong { "添加内容。" }
                    span { "2D 画面从 2D 图元库添加面板、文本、仪表、按钮和管线；3D 画面从 3D 设备库添加 AHU、风机、水泵、风管、水管和阀门。" }
                }
                li {
                    strong { "选择并编辑。" }
                    span { "在画布或左侧结构树选择节点；⇧/⌘ 点击可多选；2D 空白处拖动平移、3D 空白处拖动旋转视角，两者都用 Option/Alt + 拖动框选。悬停未选节点或设备时会显示微弱发光轮廓。2D 可拖动整组节点并拖角点缩放；3D 可直接拖动整组设备，或用键盘、手柄和右侧检查器精确修改。3D 坐标统一按 X 左右、Y 前后、Z 垂直地面显示，画布左下角有实时 XYZ 方位标识。" }
                }
                li {
                    strong { "自动保存并运行。" }
                    span { "工程在编辑后自动保存，无需手动点保存；“运行”切换到只读运行态。F1 可随时重新打开本指引。" }
                }
            }
            div { class: "guide-callout",
                strong { "3D 是怎么出现的？" }
                p { "新建 3D 画面后，Bevy 只生成一个地面和灯光，不再额外生成墙体；设备节点来自左侧 3D 设备库。选中设备后会出现青色选择环，右侧可编辑 X/Y/Z（Z 垂直地面）、旋转、缩放和 GLB 路径。" }
            }
            div { class: "guide-callout",
                strong { "手柄菜单：Options" }
                p { "按 Options 打开菜单，可编辑所选、撤销/重做、添加组件、浏览数据字典、新建或切换画面，以及查看手柄说明。× 执行，○ 返回，Options 关闭；按 △ 可直接打开添加菜单，新节点会自动选中并在当前视野附近错开摆放，随后用左摇杆拖动、方向键微调。" }
            }
            div { class: "guide-callout is-soft",
                strong { "对齐与震动反馈" }
                p { "2D 拖动、键盘轻推和手柄移动都会吸附到画板/节点中心线；管线折点靠近水平/垂直方向时会自动吸附，端点还会吸附设备边缘。3D 地面拖动会吸附 X=0 / Y=0 垂直中心面，属性检查器还可一键对齐到 X/Y/Z 三个中心面。吸附瞬间播放轻提示音；macOS 原生编辑器优先通过 CoreHaptics 触发 DualSense 短振，同时保留触控板线性马达反馈；Web 端则尝试 DualSense vibrationActuator。" }
            }
            div { class: "guide-callout is-soft",
                strong { "标准快捷键与输入优先级" }
                p { "画布获得焦点后，⌘A 全选、⌘C/⌘V/⌘X 复制/粘贴/剪切、⌘D 原地复制、⌘Z/⌘⇧Z 撤销/重做、Delete 删除、Esc 取消选择。输入框、下拉框或可编辑文本获得焦点时这些快捷键交还给表单，场景快捷键和手柄编辑暂停；点击画布即可恢复。" }
            }
            div { class: "guide-callout is-soft",
                strong { "三类输入的能力边界" }
                p { "画布内的选择、组移动、复制/剪切/粘贴、全选、删除、撤销/重做、画面切换和添加节点，均可由 DualSense 完成。△ 直接打开添加菜单；删除可连按两次 L3 完成，第一次会提示确认。文本、颜色和 Tag 绑定仍需键鼠或触控板。" }
            }
        }
    }
}

#[component]
fn GuideTwoD() -> Element {
    rsx! {
        section { class: "guide-section",
            h3 { "2D：选择与视图（鼠标 / 触控板）" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "操作" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "左键单击" } td { "选择节点；拖拽已选节点会移动整个未锁定选择" } }
                    tr { td { "悬停未选节点" } td { "显示微弱的多层矩形轮廓，提示可点击范围" } }
                    tr { td { "⇧ 或 ⌘ + 单击" } td { "加入或移出选择（多选）" } }
                    tr { td { "空白处左键拖拽 / 触控板单指拖拽" } td { "平移画布" } }
                    tr { td { "Option/Alt + 拖拽" } td { "区域框选，可在节点上开始；与选框相交的可见节点会被选中" } }
                    tr { td { "⇧ + Option/Alt + 拖拽" } td { "追加框选，不清除已有选择" } }
                    tr { td { "拖拽角点" } td { "缩放（仅单选时显示角点）" } }
                    tr { td { "单选管线后拖拽圆形折点" } td { "直接调整该折点；相邻线段进入约 7° 的水平/垂直范围时自动吸附，端点靠近设备时吸附到最近的设备边缘" } }
                    tr { td { "拖动折点时按住 Shift" } td { "锁定水平或垂直拖动方向" } }
                    tr { td { "中键拖拽 / Space + 左键拖拽" } td { "平移画布" } }
                    tr { td { "鼠标滚轮" } td { "以光标为中心缩放" } }
                    tr { td { "触控板双指滚动" } td { "平移画布" } }
                    tr { td { "触控板捏合 / Cmd(⌘)+滚动" } td { "缩放画布" } }
                    tr { td { "点击空白处 / Esc" } td { "取消选择" } }
                }
            }

            h3 { "2D：键盘（先点击画布）" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { kbd { "⌘A" } } td { "全选当前画面的可见节点" } }
                    tr { td { kbd { "⌘C" } kbd { "⌘V" } kbd { "⌘X" } } td { "复制 / 粘贴 / 剪切；粘贴和剪切各是一条撤销记录" } }
                    tr { td { kbd { "⌘D" } } td { "原地复制所选节点，不改写剪贴板" } }
                    tr { td { kbd { "←" } kbd { "→" } kbd { "↑" } kbd { "↓" } } td { "整组轻推 1 个设计单位" } }
                    tr { td { kbd { "Shift" } " + 方向键" } td { "轻推 10 个单位" } }
                    tr { td { kbd { "Q" } kbd { "E" } } td { "单选时逆时针 / 顺时针旋转 1°" } }
                    tr { td { kbd { "Shift" } " + " kbd { "Q" } kbd { "E" } } td { "单选时旋转 15°" } }
                    tr { td { kbd { "F" } } td { "聚焦全部所选节点" } }
                    tr { td { kbd { "Home" } } td { "恢复 2D 最佳视角" } }
                    tr { td { kbd { "⌘Z" } kbd { "⌘⇧Z" } kbd { "Ctrl+Y" } } td { "撤销 / 重做 / 重做" } }
                    tr { td { kbd { "⌘S" } } td { "立即刷写自动保存（日常无需手动操作）" } }
                    tr { td { kbd { "Delete" } kbd { "Backspace" } } td { "删除所选节点（锁定节点除外），设备残影由吃豆人逐个吃掉" } }
                    tr { td { kbd { "Esc" } } td { "取消选择 / 关闭指引" } }
                }
            }

            h3 { "2D：PS5 DualSense" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "左摇杆" } td { "移动整个所选组；松开后合并为一次撤销记录" } }
                    tr { td { "右摇杆" } td { "平移画布" } }
                    tr { td { "L2 / R2" } td { "缩小 / 放大画布" } }
                    tr { td { "方向键" } td { "整组轻推 2 个单位；靠近中心线时不会被吸附卡住" } }
                    tr { td { "L1 / R1" } td { "单选上一个 / 下一个节点" } }
                    tr { td { "Cross ×" } td { "聚焦所选节点" } }
                    tr { td { "Circle ○" } td { "取消选择" } }
                    tr { td { "Square □" } td { "原地复制所选节点" } }
                    tr { td { "Triangle △" } td { "直接打开 2D 图元添加菜单" } }
                    tr { td { "L3 连按两次" } td { "删除所选节点；第一次会提示确认" } }
                    tr { td { "Options" } td { "打开手柄菜单：编辑、撤销/重做、添加、数据字典、画面" } }
                    tr { td { "Options → 编辑所选" } td { "复制、剪切、粘贴、全选和删除" } }
                    tr { td { "Create" } td { "打开 / 关闭本指引" } }
                    tr { td { "指引已打开：L1 / R1 或 ← / →" } td { "切换上一章 / 下一章" } }
                    tr { td { "指引已打开：↑ / ↓ 或左摇杆" } td { "滚动指引正文" } }
                    tr { td { "DualSense 触摸板按下" } td { "打开 / 关闭参数模式（必须单选且未锁定）" } }
                    tr { td { "参数模式：方向键 ↑ / ↓" } td { "切换 X、Y、宽度、高度、旋转和组件数值" } }
                    tr { td { "参数模式：方向键 ← / →" } td { "调整当前参数" } }
                    tr { td { "参数模式：L1 / R1" } td { "粗调当前参数，步长 ×10" } }
                    tr { td { "参数模式：Circle ○" } td { "退出参数模式，不取消选择" } }
                }
            }
            p { class: "guide-note", "管线的每个折点都可直接拖动；相邻线段接近水平/垂直时会自动吸附并显示琥珀色局部引导线，端点吸附设备后显示琥珀色高亮。拖任意折点仍会吸附画布和其他节点中心线。空白处普通拖拽用于平移画布，框选和任意组合多选使用 Option/Alt + 鼠标或触控板拖拽；手柄可在 Options → 编辑所选中全选，再用 L1/R1 单选浏览。连接方式：在 macOS 蓝牙设置中配对 DualSense，按一下 PS 键唤醒，再按任意键；识别后顶栏显示“DualSense”。" }
        }
    }
}

#[component]
fn GuideThreeD() -> Element {
    rsx! {
        section { class: "guide-section",
            h3 { "3D：选择与视图（鼠标 / 触控板）" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "操作" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "左键单击设备" } td { "选择设备；所有选中设备都显示青色选择环" } }
                    tr { td { "悬停未选设备" } td { "显示微弱的多层包围盒发光轮廓" } }
                    tr { td { "⇧ 或 ⌘ + 单击设备" } td { "加入或移出选择（多选）" } }
                    tr { td { "拖拽设备" } td { "沿地面 X/Y 平面移动所选组；拖未选设备会先选中它，松开后一次提交" } }
                    tr { td { "拖拽时按住 Shift" } td { "锁定主导地面轴，方便沿 X 或 Y 方向直线移动" } }
                    tr { td { "拖拽到画布中心附近" } td { "组中心吸附 X=0 / Y=0 垂直中心面，并显示琥珀色中心面" } }
                    tr { td { "空白处左键拖拽" } td { "轨道旋转视角" } }
                    tr { td { "Option/Alt + 左键拖拽" } td { "屏幕空间区域框选，与设备包围盒相交即选中；按住 Shift 追加" } }
                    tr { td { "右键拖拽" } td { "轨道旋转视角" } }
                    tr { td { "中键拖拽" } td { "平移观察中心" } }
                    tr { td { "鼠标滚轮" } td { "拉近 / 拉远" } }
                    tr { td { "触控板拖拽设备 / 空白" } td { "拖设备移动所选组；拖空白处单指旋转视角" } }
                    tr { td { "触控板 Option/Alt + 单指拖拽" } td { "屏幕空间区域框选多选；按住 Shift 追加" } }
                    tr { td { "触控板双指滚动" } td { "平移观察中心" } }
                    tr { td { "触控板捏合 / Cmd(⌘)+滚动" } td { "拉近 / 拉远" } }
                }
            }

            h3 { "3D：键盘（先点击画布）" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { kbd { "←" } kbd { "→" } kbd { "↑" } kbd { "↓" } } td { "整组轻推地面 X / Y，每步 0.1" } }
                    tr { td { kbd { "Q" } kbd { "E" } } td { "降低 / 升高整组设备，每步 0.1" } }
                    tr { td { kbd { "Shift" } " + 移动键" } td { "每步 1.0" } }
                    tr { td { kbd { "Option/Alt" } " + 方向键" } td { "环绕旋转视角" } }
                    tr { td { kbd { "W" } kbd { "A" } kbd { "S" } kbd { "D" } } td { "相对当前视角平移观察中心" } }
                    tr { td { kbd { "+" } kbd { "-" } } td { "拉近 / 拉远" } }
                    tr { td { kbd { "Home" } } td { "恢复 3D 最佳视角" } }
                    tr { td { kbd { "F" } } td { "聚焦所选设备" } }
                    tr { td { kbd { "⌘A" } } td { "全选当前画面的可见设备" } }
                    tr { td { kbd { "⌘C" } kbd { "⌘V" } kbd { "⌘X" } } td { "复制 / 粘贴 / 剪切；跨同类型画面粘贴会保留相对位置" } }
                    tr { td { kbd { "⌘D" } } td { "原地复制所选设备，不改写剪贴板" } }
                    tr { td { kbd { "Delete" } } td { "删除所选设备（锁定设备除外），设备残影由吃豆人逐个吃掉" } }
                    tr { td { kbd { "⌘Z" } kbd { "⌘⇧Z" } kbd { "Ctrl+Y" } } td { "撤销 / 重做 / 重做" } }
                    tr { td { kbd { "⌘S" } } td { "立即刷写自动保存（日常无需手动操作）" } }
                }
            }

            h3 { "3D：PS5 DualSense" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "左摇杆" } td { "相对当前镜头方向沿地面移动整个所选组，左右不会因转视角而反转；松开后一次提交" } }
                    tr { td { "右摇杆" } td { "环绕旋转视角" } }
                    tr { td { "L2 / R2" } td { "拉远 / 拉近" } }
                    tr { td { "方向键 ← / →" } td { "整组精确轻推 X，每次 0.1" } }
                    tr { td { "方向键 ↑ / ↓" } td { "整组精确调整 Z（高度），每次 0.1" } }
                    tr { td { "Cross ×" } td { "聚焦所选设备" } }
                    tr { td { "Circle ○" } td { "取消选择" } }
                    tr { td { "R3" } td { "重置 3D 视角" } }
                    tr { td { "Square □" } td { "原地复制所选设备" } }
                    tr { td { "Triangle △" } td { "直接打开 3D 设备添加菜单" } }
                    tr { td { "L3 连按两次" } td { "删除所选设备；第一次会提示确认" } }
                    tr { td { "L1 / R1" } td { "单选上一个 / 下一个设备" } }
                    tr { td { "Options" } td { "打开手柄菜单：编辑、撤销/重做、添加、数据字典、画面" } }
                    tr { td { "Options → 编辑所选" } td { "复制、剪切、粘贴、全选和删除" } }
                    tr { td { "Create" } td { "打开 / 关闭本指引" } }
                    tr { td { "指引已打开：L1 / R1 或 ← / →" } td { "切换上一章 / 下一章" } }
                    tr { td { "指引已打开：↑ / ↓ 或左摇杆" } td { "滚动指引正文" } }
                    tr { td { "DualSense 触摸板按下" } td { "打开 / 关闭参数模式（必须单选且未锁定）" } }
                    tr { td { "参数模式：方向键 ↑ / ↓" } td { "切换 X/Y/Z、旋转、缩放和模型尺寸" } }
                    tr { td { "参数模式：方向键 ← / →" } td { "调整当前参数" } }
                    tr { td { "参数模式：L1 / R1" } td { "粗调当前参数，步长 ×10" } }
                    tr { td { "参数模式：Circle ○" } td { "退出参数模式，不取消选择" } }
                }
            }
            p { class: "guide-note", "当前 3D 编辑采用“选择 + 变换 + 属性”的工作流，不提供工业建模软件式多轴 Gizmo，但画布左下角有实时 XYZ 方位标识：X 左右、Y 前后、Z 垂直地面。方向键沿地面轻推，Q/E 调整高度；右侧“3D 变换”提供垂直中心面 X=0、垂直中心面 Y=0 和水平中心面 Z=0 的一键对齐。旋转与缩放可在右侧检查器或手柄触摸板参数模式中精确调整。框选使用 Option/Alt + 鼠标/触控板拖拽，手柄可在 Options → 编辑所选中全选。" }
        }
    }
}

#[component]
fn GuideGamepad() -> Element {
    rsx! {
        section { class: "guide-section",
            h3 { "画面：移动与观察" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "左摇杆" } td { "移动所选画面内容。3D 中始终相对当前镜头方向，左右不会因视角旋转而颠倒" } }
                    tr { td { "右摇杆" } td { "2D 平移画布；3D 环绕旋转视角" } }
                    tr { td { "L2 / R2" } td { "拉远 / 拉近" } }
                    tr { td { "方向键" } td { "2D 沿 X / Y 轻推 2 个单位；3D 沿 X 和高度 Z 精确轻推 0.1" } }
                    tr { td { "L1 / R1" } td { "单选上一个 / 下一个节点或设备" } }
                    tr { td { "Cross ×" } td { "聚焦所选设备" } }
                    tr { td { "Circle ○" } td { "取消选择" } }
                    tr { td { "Square □" } td { "原地复制所选内容" } }
                    tr { td { "Triangle △" } td { "直接打开当前画面的添加菜单" } }
                    tr { td { "L3" } td { "连按两次删除所选；第一次会提示确认，2 秒内有效" } }
                    tr { td { "R3" } td { "3D 重置视角" } }
                    tr { td { "Options" } td { "打开 / 关闭手柄菜单" } }
                    tr { td { "Create" } td { "打开 / 关闭本指引" } }
                    tr { td { "DualSense 触摸板" } td { "单选且未锁定时，打开数值参数模式；方向键切换/调整参数" } }
                }
            }

            h3 { "手柄菜单：编辑与画面" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "Options" } td { "打开菜单；菜单内再按一次关闭" } }
                    tr { td { "方向键 ↑ / ↓" } td { "选择菜单项" } }
                    tr { td { "Cross ×" } td { "执行当前菜单项" } }
                    tr { td { "Circle ○" } td { "返回上一级；根菜单中关闭" } }
                    tr { td { "编辑所选" } td { "复制、剪切、粘贴、原地复制、全选和删除" } }
                    tr { td { "撤销 / 重做" } td { "位于根菜单，直接执行" } }
                    tr { td { "添加组件" } td { "进入当前画面的 2D 图元或 3D 设备列表" } }
                    tr { td { "数据字典" } td { "浏览 Tag 与实时值" } }
                    tr { td { "画面" } td { "切换上一个 / 下一个画面，或新建 2D / 3D 画面" } }
                }
            }
            p { class: "guide-note", "△ 从画布直接打开添加菜单，少一层菜单跳转；L3 连按两次删除所选，第一次会显示确认提示，2 秒内再次按下才执行，避免单次误触。剪贴板与全选仍位于“编辑所选”。左侧“组件库”标题下的提示与菜单内容一致；鼠标/触控板仍可直接点击组件按钮。" }

            h3 { "组件库：手柄添加" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "△ 或 Options → 添加组件" } td { "进入当前画面的组件列表" } }
                    tr { td { "方向键 ↑↓" } td { "选择组件；× 添加后在当前视野附近自动错开摆放并选中" } }
                    tr { td { "Circle ○" } td { "返回上一级菜单" } }
                }
            }

            h3 { "数据字典：浏览与选中 Tag" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "Options → 数据字典" } td { "打开 Tag 列表，每项显示名称、ID 和实时值" } }
                    tr { td { "方向键 ↑↓" } td { "浏览 Tag" } }
                    tr { td { "Cross ×" } td { "在左侧数据字典中选中该 Tag，并展开详情" } }
                    tr { td { "Circle ○" } td { "返回手柄菜单" } }
                }
            }
            p { class: "guide-note", "手柄负责浏览和选中。Tag 名称、单位、范围、初始值和绑定等文本/下拉编辑仍用鼠标或触控板完成，避免手柄误改工程。" }

            h3 { "随时查看说明" }
            p { class: "guide-note", "按手柄 Create 可打开本指引，再按 L1 / R1 切到“手柄操作”。Options → 手柄操作说明也提供同一入口。" }
        }
    }
}
