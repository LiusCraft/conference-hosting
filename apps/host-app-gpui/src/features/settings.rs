use gpui::{div, prelude::*, rgb, Context, FontWeight, SharedString, Window};
use gpui_component::input::Input;
use host_core::{AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE_HZ};

use crate::app::config::env_or_default;
use crate::app::shell::{ui_icon, MeetingHostShell};
use crate::app::state::{DEFAULT_CLIENT_ID, DEFAULT_DEVICE_MAC};
use crate::components::icon::IconName;
use crate::components::ui::{key_value_row, modal_surface, text_input_shell};

impl MeetingHostShell {
    pub(crate) fn open_settings_panel(&mut self, cx: &mut Context<Self>) {
        self.show_settings_panel = true;
        self.show_input_dropdown = false;
        self.show_output_dropdown = false;
        self.notify_views(cx);
    }

    pub(crate) fn close_settings_panel(&mut self, cx: &mut Context<Self>) {
        if !self.show_settings_panel {
            return;
        }
        self.show_settings_panel = false;
        self.notify_views(cx);
    }

    pub(crate) fn toggle_ai_emotion_messages(&mut self, cx: &mut Context<Self>) {
        self.show_ai_emotion_messages = !self.show_ai_emotion_messages;
        let state = if self.show_ai_emotion_messages {
            "AI emotion placeholders are now visible"
        } else {
            "AI emotion placeholders are now hidden"
        };
        self.push_chat(crate::app::state::ChatRole::System, "System", state);
        self.notify_views(cx);
    }

    pub(crate) fn render_settings_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_input = self.selected_input_device_label();
        let selected_output = self
            .selected_output_device_name()
            .unwrap_or("default")
            .to_string();
        let ws_url_input = self.render_ws_url_input(window, cx);

        modal_surface(560.0, 640.0)
            .child(
                div()
                    .h_12()
                    .px_5()
                    .border_b_1()
                    .border_color(rgb(0x1a2435))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xe2e8f4))
                            .child(ui_icon(IconName::Settings, 16.0, 0x16d9c0))
                            .child("设置"),
                    )
                    .child(
                        div()
                            .id("close-settings-button")
                            .size_8()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(0x2a3548))
                            .bg(rgb(0x131b2a))
                            .text_color(rgb(0x8a96ab))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(ui_icon(IconName::X, 13.0, 0x8a96ab))
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                view.close_settings_panel(cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex()
                    .flex_col()
                    .gap_5()
                    .p_5()
                    .track_scroll(&self.settings_scroll)
                    .overflow_y_scroll()
                    .scrollbar_width(gpui::px(10.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xdce4f3))
                                    .child(ui_icon(IconName::Server, 13.0, 0x16d9c0))
                                    .child("WebSocket 服务器"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(0x1d283a))
                                    .bg(rgb(0x101726))
                                    .px_3()
                                    .py_1()
                                    .child(editable_setting_row("地址", ws_url_input))
                                    .child(setting_row("协议版本", "1"))
                                    .child(setting_row("传输方式", "WebSocket Binary Frame")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xdce4f3))
                                    .child(ui_icon(IconName::ShieldCheck, 13.0, 0x16d9c0))
                                    .child("认证信息"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(0x1d283a))
                                    .bg(rgb(0x101726))
                                    .px_3()
                                    .py_1()
                                    .child(setting_row("Authorization", "Bearer ****...a3f2"))
                                    .child(setting_row(
                                        "Device-Id",
                                        env_or_default("HOST_DEVICE_ID", DEFAULT_DEVICE_MAC),
                                    ))
                                    .child(setting_row(
                                        "Client-Id",
                                        env_or_default("HOST_CLIENT_ID", DEFAULT_CLIENT_ID),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xdce4f3))
                                    .child(ui_icon(IconName::Volume2, 13.0, 0x16d9c0))
                                    .child("音频参数"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(0x1d283a))
                                    .bg(rgb(0x101726))
                                    .px_3()
                                    .py_1()
                                    .child(setting_row("格式", "PCM 16bit"))
                                    .child(setting_row(
                                        "采样率",
                                        format!("{} Hz", AUDIO_SAMPLE_RATE_HZ),
                                    ))
                                    .child(setting_row("声道", "Mono"))
                                    .child(setting_row("帧时长", "20ms"))
                                    .child(setting_row(
                                        "帧大小",
                                        format!("{} samples", AUDIO_FRAME_SAMPLES),
                                    ))
                                    .child(setting_row("发送频率", "50 帧/秒"))
                                    .child(setting_row("编解码", "Opus (上行/下行)")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xdce4f3))
                                    .child(ui_icon(IconName::Globe, 13.0, 0x16d9c0))
                                    .child("平台适配"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(0x1d283a))
                                    .bg(rgb(0x101726))
                                    .px_3()
                                    .py_1()
                                    .child(setting_row("操作系统", std::env::consts::OS))
                                    .child(setting_row("输入设备", selected_input))
                                    .child(setting_row("输出设备", selected_output))
                                    .child(setting_row(
                                        "连接状态",
                                        self.gateway_status.as_label(),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xdce4f3))
                                    .child(ui_icon(IconName::Info, 13.0, 0x16d9c0))
                                    .child("会话显示"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(0x1d283a))
                                    .bg(rgb(0x101726))
                                    .px_3()
                                    .py_2()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0xd7e0f0))
                                                    .child("显示 AI 情绪占位消息"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x7d8aa0))
                                                    .child("当 llm 只返回 emoji 等情绪符号时，是否展示在会话列表"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("toggle-ai-emotion-messages")
                                            .h_8()
                                            .px_3()
                                            .rounded_md()
                                            .border_1()
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .border_color(if self.show_ai_emotion_messages {
                                                rgb(0x165e55)
                                            } else {
                                                rgb(0x36445a)
                                            })
                                            .bg(if self.show_ai_emotion_messages {
                                                rgb(0x0c3f3b)
                                            } else {
                                                rgb(0x131b2a)
                                            })
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(if self.show_ai_emotion_messages {
                                                rgb(0x6af3e2)
                                            } else {
                                                rgb(0x9aa6ba)
                                            })
                                            .child(if self.show_ai_emotion_messages {
                                                "显示中"
                                            } else {
                                                "已隐藏"
                                            })
                                            .on_click(cx.listener(|view, _event, _window, cx| {
                                                view.toggle_ai_emotion_messages(cx)
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xdce4f3))
                                    .child(ui_icon(IconName::FileCode, 13.0, 0x16d9c0))
                                    .child("工程信息"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(rgb(0x1d283a))
                                    .bg(rgb(0x101726))
                                    .px_3()
                                    .py_1()
                                    .child(setting_row("引擎", "Rust + GPUI"))
                                    .child(setting_row("运行时", "Tokio async runtime"))
                                    .child(setting_row("通信", "tokio-tungstenite + rustls"))
                                    .child(setting_row("音频层", "cpal + opus"))
                                    .child(setting_row("版本", "0.1.0-alpha")),
                            ),
                    ),
            )
            .child(
                div()
                    .h_12()
                    .px_5()
                    .border_t_1()
                    .border_color(rgb(0x1a2435))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(
                        div()
                            .id("save-settings-button")
                            .h_9()
                            .px_4()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(0x165e55))
                            .bg(rgb(0x0c3f3b))
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x6af3e2))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("保存并关闭")
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                view.close_settings_panel(cx)
                            })),
                    ),
            )
    }
}

fn setting_row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> impl IntoElement {
    key_value_row(label, value)
}

fn editable_setting_row(
    label: impl Into<SharedString>,
    field: impl IntoElement,
) -> impl IntoElement {
    div()
        .py_3()
        .border_b_1()
        .border_color(rgb(0x1b2536))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x7d8aa0))
                .child(label.into()),
        )
        .child(field)
}

impl MeetingHostShell {
    fn render_ws_url_input(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let border_hex = if self.ws_url_input_focused {
            if self.ws_url.trim().is_empty() {
                0xcf4d68
            } else {
                0x16d9c0
            }
        } else if self.ws_url.trim().is_empty() {
            0x7f2230
        } else {
            0x283449
        };

        text_input_shell(
            border_hex,
            Input::new(&self.ws_url_input_state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff)),
        )
        .id("settings-ws-url-input")
        .h_10()
    }
}
