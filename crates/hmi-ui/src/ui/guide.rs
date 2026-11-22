use crate::bridge;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GuideTab {
    Start,
    TwoD,
    ThreeD,
}

#[component]
pub fn OperationGuide() -> Element {
    if !bridge::guide_open() {
        return rsx! {};
    }

    let mut tab = use_signal(|| GuideTab::Start);
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
                        p { class: "guide-lead", "2D 与 3D 共用一套选择、框选和剪贴板规则：点击选择，⇧/⌘ 点击多选，空白处拖动框选。点击画布后键盘快捷键生效；DualSense 触摸板可进入单个节点的参数模式。" }
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
                        class: if tab() == GuideTab::Start { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-start",
                        aria_pressed: tab() == GuideTab::Start,
                        onclick: move |_| tab.set(GuideTab::Start),
                        "开始与画面"
                    }
                    button {
                        class: if tab() == GuideTab::TwoD { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-2d",
                        aria_pressed: tab() == GuideTab::TwoD,
                        onclick: move |_| tab.set(GuideTab::TwoD),
                        "2D 操作"
                    }
                    button {
                        class: if tab() == GuideTab::ThreeD { "guide-tab is-active" } else { "guide-tab" },
                        "data-testid": "guide-tab-3d",
                        aria_pressed: tab() == GuideTab::ThreeD,
                        onclick: move |_| tab.set(GuideTab::ThreeD),
                        "3D 操作"
                    }
                }

                div { class: "guide-body", "data-testid": "guide-content",
                    match tab() {
                        GuideTab::Start => rsx! { GuideStart {} },
                        GuideTab::TwoD => rsx! { GuideTwoD {} },
                        GuideTab::ThreeD => rsx! { GuideThreeD {} },
                    }
                }

                footer { class: "guide-foot",
                    span { "F1 打开 / 关闭指引" }
                    span { "DualSense Create 打开指引 · Circle 关闭" }
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
                    span { "左侧“画面”标题栏点击 +，选择“空白 2D 画面”或“3D 机房画面”。" }
                }
                li {
                    strong { "添加内容。" }
                    span { "2D 画面从 2D 图元库添加面板、文本、仪表、按钮和管线；3D 画面从 3D 设备库添加 AHU、风机、水泵、风管、水管和阀门。" }
                }
                li {
                    strong { "选择并编辑。" }
                    span { "在画布或左侧结构树选择节点；⇧/⌘ 点击可多选，空白处拖动可框选。悬停未选节点或设备时会显示微弱发光轮廓。2D 可拖动整组节点并拖角点缩放；3D 可直接拖动整组设备，或用键盘、手柄和右侧检查器精确修改。" }
                }
                li {
                    strong { "保存并运行。" }
                    span { "工程自动保存；“运行”切换到只读运行态。F1 可随时重新打开本指引。" }
                }
            }
            div { class: "guide-callout",
                strong { "3D 是怎么出现的？" }
                p { "新建 3D 画面后，Bevy 自动生成机房地面、墙体和灯光；设备节点来自左侧 3D 设备库。选中设备后会出现青色选择环，右侧可编辑 X/Y/Z、旋转、缩放和 GLB 路径。" }
            }
            div { class: "guide-callout is-soft",
                strong { "对齐反馈" }
                p { "2D 拖动、键盘轻推和手柄移动都会吸附到画板/节点中心线。吸附瞬间播放“砰”声；macOS 原生编辑器同时触发触控板线性马达，若浏览器暴露 DualSense vibrationActuator，手柄也会同步短振。" }
            }
            div { class: "guide-callout is-soft",
                strong { "标准快捷键与输入优先级" }
                p { "画布获得焦点后，⌘A 全选、⌘C/⌘V/⌘X 复制/粘贴/剪切、⌘D 原地复制、⌘Z/⌘⇧Z 撤销/重做、Delete 删除、Esc 取消选择。输入框、下拉框或可编辑文本获得焦点时这些快捷键交还给表单，场景快捷键和手柄编辑暂停；点击画布即可恢复。" }
            }
            div { class: "guide-callout is-soft",
                strong { "三类输入的能力边界" }
                p { "画布内的选择、框选、组移动、复制/剪切/粘贴、删除、撤销/重做和画面切换均可由键盘+鼠标、键盘+触控板或 DualSense 完成。手柄用 L3+□ 全选，用 L3+×/○/△ 复制/剪切/粘贴；新建画面/节点、文本、颜色和 Tag 绑定仍需键鼠或触控板。" }
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
                    tr { td { "空白处左键拖拽" } td { "区域框选；与选框相交的可见节点会被选中" } }
                    tr { td { "⇧ + 空白处拖拽" } td { "追加框选，不清除已有选择" } }
                    tr { td { "拖拽角点" } td { "缩放（仅单选时显示角点）" } }
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
                    tr { td { kbd { "⌘Z" } kbd { "⌘⇧Z" } kbd { "Ctrl+Y" } } td { "撤销 / 重做 / 重做" } }
                    tr { td { kbd { "⌘S" } } td { "立即保存工程" } }
                    tr { td { kbd { "Delete" } kbd { "Backspace" } } td { "删除所选节点（锁定节点除外）" } }
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
                    tr { td { "方向键" } td { "整组轻推 2 个单位，可精确对齐中心线" } }
                    tr { td { "Cross ×" } td { "聚焦所选节点" } }
                    tr { td { "Circle ○" } td { "取消选择" } }
                    tr { td { "Square □" } td { "原地复制所选节点" } }
                    tr { td { "Triangle △" } td { "删除所选节点" } }
                    tr { td { "L1 / R1" } td { "单选上一个 / 下一个节点" } }
                    tr { td { "按住 L3 + Square □" } td { "全选当前画面的可见节点" } }
                    tr { td { "按住 L3 + Cross × / Circle ○ / Triangle △" } td { "复制 / 剪切 / 粘贴所选节点" } }
                    tr { td { "Options" } td { "撤销" } }
                    tr { td { "按住 L3 + Options" } td { "重做" } }
                    tr { td { "按住 L3 + L1 / R1" } td { "切换上一个 / 下一个画面" } }
                    tr { td { "Create" } td { "打开 / 关闭本指引" } }
                    tr { td { "DualSense 触摸板按下" } td { "打开 / 关闭参数模式（必须单选且未锁定）" } }
                    tr { td { "参数模式：方向键 ↑ / ↓" } td { "切换 X、Y、宽度、高度、旋转和组件数值" } }
                    tr { td { "参数模式：方向键 ← / →" } td { "调整当前参数" } }
                    tr { td { "参数模式：L1 / R1" } td { "粗调当前参数，步长 ×10" } }
                    tr { td { "参数模式：Circle ○" } td { "退出参数模式，不取消选择" } }
                }
            }
            p { class: "guide-note", "框选和任意组合多选使用鼠标或触控板；手柄可用 L3+□ 全选，再用 L1/R1 单选浏览。参数模式一次只能调整一个节点；文本、颜色和 Tag 选择仍使用键鼠或触控板。连接方式：在 macOS 蓝牙设置中配对 DualSense，按一下 PS 键唤醒，再按任意键；识别后顶栏显示“DualSense”。" }
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
                    tr { td { "拖拽设备" } td { "沿地面 X/Z 平面移动所选组；拖未选设备会先选中它，松开后一次提交" } }
                    tr { td { "空白处左键拖拽" } td { "屏幕空间区域框选，与设备包围盒相交即选中" } }
                    tr { td { "Option/Alt + 左键拖拽" } td { "轨道旋转视角" } }
                    tr { td { "中键拖拽" } td { "平移观察中心" } }
                    tr { td { "鼠标滚轮" } td { "拉近 / 拉远" } }
                    tr { td { "触控板拖拽设备 / 空白" } td { "拖设备移动所选组；拖空白区域框选" } }
                    tr { td { "触控板 Option/Alt + 左键拖拽" } td { "旋转视角" } }
                    tr { td { "触控板双指滚动" } td { "平移观察中心" } }
                    tr { td { "触控板捏合 / Cmd(⌘)+滚动" } td { "拉近 / 拉远" } }
                }
            }

            h3 { "3D：键盘（先点击画布）" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { kbd { "←" } kbd { "→" } kbd { "↑" } kbd { "↓" } } td { "环绕旋转视角" } }
                    tr { td { kbd { "+" } kbd { "-" } } td { "拉近 / 拉远" } }
                    tr { td { kbd { "Home" } } td { "重置 3D 视角" } }
                    tr { td { kbd { "F" } } td { "聚焦所选设备" } }
                    tr { td { kbd { "W" } kbd { "A" } kbd { "S" } kbd { "D" } } td { "沿世界 Z / X 轴整组移动设备，每步 0.1" } }
                    tr { td { kbd { "Q" } kbd { "E" } } td { "降低 / 升高整组设备 Y，每步 0.1" } }
                    tr { td { kbd { "Shift" } " + 移动键" } td { "每步 1.0" } }
                    tr { td { kbd { "⌘A" } } td { "全选当前画面的可见设备" } }
                    tr { td { kbd { "⌘C" } kbd { "⌘V" } kbd { "⌘X" } } td { "复制 / 粘贴 / 剪切；跨同类型画面粘贴会保留相对位置" } }
                    tr { td { kbd { "⌘D" } } td { "原地复制所选设备，不改写剪贴板" } }
                    tr { td { kbd { "Delete" } } td { "删除所选设备（锁定设备除外）" } }
                    tr { td { kbd { "⌘Z" } kbd { "⌘⇧Z" } kbd { "Ctrl+Y" } } td { "撤销 / 重做 / 重做" } }
                    tr { td { kbd { "⌘S" } } td { "立即保存工程" } }
                }
            }

            h3 { "3D：PS5 DualSense" }
            table { class: "guide-table",
                thead { tr { th { scope: "col", "按键" } th { scope: "col", "结果" } } }
                tbody {
                    tr { td { "左摇杆" } td { "沿世界 X / Z 轴移动整个所选组；松开后一次提交" } }
                    tr { td { "按住 L3 + 左摇杆" } td { "切换为垂直模式：左右移动 X，上下移动 Y" } }
                    tr { td { "右摇杆" } td { "环绕旋转视角" } }
                    tr { td { "按住 L3 + 右摇杆" } td { "平移观察中心" } }
                    tr { td { "L2 / R2" } td { "拉远 / 拉近" } }
                    tr { td { "方向键" } td { "整组精确轻推 X / Z，每次 0.1" } }
                    tr { td { "按住 L3 + 方向键上下" } td { "精确调整 Y" } }
                    tr { td { "Cross ×" } td { "聚焦所选设备" } }
                    tr { td { "Circle ○" } td { "取消选择" } }
                    tr { td { "R3" } td { "重置 3D 视角" } }
                    tr { td { "Square □" } td { "原地复制所选设备" } }
                    tr { td { "Triangle △" } td { "删除所选设备" } }
                    tr { td { "L1 / R1" } td { "单选上一个 / 下一个设备" } }
                    tr { td { "按住 L3 + Square □" } td { "全选当前画面的可见设备" } }
                    tr { td { "按住 L3 + Cross × / Circle ○ / Triangle △" } td { "复制 / 剪切 / 粘贴所选设备" } }
                    tr { td { "Options" } td { "撤销" } }
                    tr { td { "按住 L3 + Options" } td { "重做" } }
                    tr { td { "按住 L3 + L1 / R1" } td { "切换上一个 / 下一个画面" } }
                    tr { td { "Create" } td { "打开 / 关闭本指引" } }
                    tr { td { "DualSense 触摸板按下" } td { "打开 / 关闭参数模式（必须单选且未锁定）" } }
                    tr { td { "参数模式：方向键 ↑ / ↓" } td { "切换 X/Y/Z、旋转、缩放和模型尺寸" } }
                    tr { td { "参数模式：方向键 ← / →" } td { "调整当前参数" } }
                    tr { td { "参数模式：L1 / R1" } td { "粗调当前参数，步长 ×10" } }
                    tr { td { "参数模式：Circle ○" } td { "退出参数模式，不取消选择" } }
                }
            }
            p { class: "guide-note", "当前 3D 编辑采用“选择 + 变换 + 属性”的工作流，不提供工业建模软件式多轴 Gizmo。拖拽设备直接移动整组，拖空白处框选，Option/Alt + 拖拽旋转视角；旋转与缩放可在右侧检查器或手柄参数模式中精确调整。框选和任意组合多选使用鼠标/触控板，手柄可用 L3+□ 全选。" }
        }
    }
}
