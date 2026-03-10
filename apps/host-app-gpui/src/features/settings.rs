use gpui::{div, prelude::*, px, rgb, Context, FontWeight, SharedString, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::Disableable;
use host_core::{ListenMode, AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE_HZ};

use crate::app::persistence::{
    save_persisted_app_settings, PersistedAppSettings, PersistedUiSettings, PersistedWsSettings,
};
use crate::app::shell::{ui_button_icon, ui_icon, ButtonIconTone, MeetingHostShell};
use crate::components::icon::IconName;

impl MeetingHostShell {
    pub(crate) fn open_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_settings_drafts(window, cx);
        self.show_settings_panel = true;
        self.notify_views(cx);
    }

    pub(crate) fn close_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.show_settings_panel {
            return;
        }

        self.reset_settings_drafts(window, cx);
        self.show_settings_panel = false;
        self.notify_views(cx);
    }

    pub(crate) fn save_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.show_settings_panel {
            return;
        }

        self.sync_ws_url_from_input(cx);
        self.sync_auth_token_from_input(cx);
        self.sync_device_id_from_input(cx);
        self.sync_client_id_from_input(cx);

        self.ws_url = self.ws_url_draft.trim().to_string();
        self.auth_token = self.auth_token_draft.trim().to_string();
        self.device_id = self.device_id_draft.trim().to_string();
        self.client_id = self.client_id_draft.trim().to_string();

        self.ws_url_draft = self.ws_url.clone();
        self.auth_token_draft = self.auth_token.clone();
        self.device_id_draft = self.device_id.clone();
        self.client_id_draft = self.client_id.clone();
        self.apply_aec_enabled(self.aec_enabled_draft);
        self.apply_show_ai_emotion_messages(self.show_ai_emotion_messages_draft);
        self.apply_listen_mode(self.listen_mode_draft);
        self.aec_enabled_draft = self.aec_enabled;
        self.show_ai_emotion_messages_draft = self.show_ai_emotion_messages;
        self.listen_mode_draft = self.listen_mode;
        let mcp_changed = self.mcp_servers != self.mcp_servers_draft;
        self.mcp_servers = self.mcp_servers_draft.clone();
        self.write_settings_input_values(window, cx);

        let persisted_settings = PersistedAppSettings {
            ws: PersistedWsSettings {
                server_url: self.ws_url.clone(),
                device_id: self.device_id.clone(),
                client_id: self.client_id.clone(),
                auth_token: self.auth_token.clone(),
            },
            ui: PersistedUiSettings {
                aec_enabled: Some(self.aec_enabled),
                show_ai_emotion_messages: Some(self.show_ai_emotion_messages),
                listen_mode: Some(self.listen_mode),
            },
            mcp_servers: self.mcp_servers.clone(),
        };
        if let Err(error) = save_persisted_app_settings(&persisted_settings) {
            self.push_chat(
                crate::app::state::ChatRole::Error,
                "Error",
                format!("保存本地设置失败: {error}"),
            );
        } else {
            self.push_chat(
                crate::app::state::ChatRole::System,
                "System",
                "本地设置已保存（包含 MCP Servers）",
            );
        }

        if mcp_changed
            && matches!(
                self.connection_state,
                crate::app::state::ConnectionState::Connected
            )
        {
            self.push_chat(
                crate::app::state::ChatRole::System,
                "System",
                "MCP 配置已变更，请断开并重连以加载新配置",
            );
        }

        self.show_settings_panel = false;
        self.notify_views(cx);
    }

    fn reset_settings_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ws_url_draft = self.ws_url.clone();
        self.auth_token_draft = self.auth_token.clone();
        self.device_id_draft = self.device_id.clone();
        self.client_id_draft = self.client_id.clone();
        self.aec_enabled_draft = self.aec_enabled;
        self.show_ai_emotion_messages_draft = self.show_ai_emotion_messages;
        self.listen_mode_draft = self.listen_mode;
        self.reset_mcp_settings_drafts(window, cx);
        self.write_settings_input_values(window, cx);
    }

    fn write_settings_input_values(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ws_url = self.ws_url_draft.clone();
        let auth_token = self.auth_token_draft.clone();
        let device_id = self.device_id_draft.clone();
        let client_id = self.client_id_draft.clone();

        self.ws_url_input_state.update(cx, |input, cx| {
            input.set_value(&ws_url, window, cx);
        });
        self.auth_token_input_state.update(cx, |input, cx| {
            input.set_value(&auth_token, window, cx);
        });
        self.device_id_input_state.update(cx, |input, cx| {
            input.set_value(&device_id, window, cx);
        });
        self.client_id_input_state.update(cx, |input, cx| {
            input.set_value(&client_id, window, cx);
        });
    }

    pub(crate) fn toggle_ai_emotion_messages(&mut self, cx: &mut Context<Self>) {
        self.show_ai_emotion_messages_draft = !self.show_ai_emotion_messages_draft;
        self.notify_views(cx);
    }

    pub(crate) fn toggle_aec_enabled(&mut self, cx: &mut Context<Self>) {
        if self.has_shared_audio_route_risk() && self.aec_enabled_draft {
            self.push_chat(
                crate::app::state::ChatRole::System,
                "System",
                "输入/输出同一路由时 AEC 为强制开启",
            );
            self.notify_views(cx);
            return;
        }

        self.aec_enabled_draft = !self.aec_enabled_draft;
        self.notify_views(cx);
    }

    pub(crate) fn set_listen_mode_draft(&mut self, mode: ListenMode, cx: &mut Context<Self>) {
        if self.listen_mode_draft == mode {
            return;
        }

        self.listen_mode_draft = mode;
        self.notify_views(cx);
    }

    fn apply_show_ai_emotion_messages(&mut self, enabled: bool) {
        if self.show_ai_emotion_messages == enabled {
            return;
        }

        self.show_ai_emotion_messages = enabled;
        let state = if enabled {
            "AI emotion placeholders are now visible"
        } else {
            "AI emotion placeholders are now hidden"
        };
        self.push_chat(crate::app::state::ChatRole::System, "System", state);
    }

    fn apply_aec_enabled(&mut self, enabled: bool) {
        let force_enabled = self.has_shared_audio_route_risk();
        let next_enabled = enabled || force_enabled;

        if self.aec_enabled == next_enabled {
            if force_enabled {
                self.aec_enabled_draft = true;
            }
            return;
        }

        self.aec_enabled = next_enabled;
        self.aec_enabled_draft = next_enabled;
        let connected = matches!(
            self.connection_state,
            crate::app::state::ConnectionState::Connected
        );

        if connected {
            if let Some(command_tx) = self.ws_command_tx.as_ref() {
                if command_tx
                    .try_send(crate::app::state::GatewayCommand::SetAecEnabled(
                        self.aec_enabled,
                    ))
                    .is_err()
                {
                    self.push_chat(
                        crate::app::state::ChatRole::Error,
                        "Error",
                        "Failed to sync AEC switch to gateway worker",
                    );
                }
            }
        }

        let state = if self.aec_enabled {
            "AEC enabled"
        } else {
            "AEC disabled"
        };
        self.push_chat(crate::app::state::ChatRole::System, "System", state);
        if force_enabled {
            self.push_chat(
                crate::app::state::ChatRole::System,
                "System",
                "输入/输出同一路由时，AEC 会保持强制开启",
            );
        }
        if !connected {
            self.push_chat(
                crate::app::state::ChatRole::System,
                "System",
                "AEC switch will apply when the next gateway connection starts",
            );
        }
    }

    fn apply_listen_mode(&mut self, mode: ListenMode) {
        if self.listen_mode == mode {
            return;
        }

        self.listen_mode = mode;
        let connected = matches!(
            self.connection_state,
            crate::app::state::ConnectionState::Connected
        );

        if connected {
            if let Some(command_tx) = self.ws_command_tx.as_ref() {
                if command_tx
                    .try_send(crate::app::state::GatewayCommand::SetListenMode(
                        self.listen_mode,
                    ))
                    .is_err()
                {
                    self.push_chat(
                        crate::app::state::ChatRole::Error,
                        "Error",
                        "Failed to sync listen mode to gateway worker",
                    );
                }
            }
        }

        self.push_chat(
            crate::app::state::ChatRole::System,
            "System",
            format!("Listen mode set to {}", listen_mode_code(self.listen_mode)),
        );
        if !connected {
            self.push_chat(
                crate::app::state::ChatRole::System,
                "System",
                "Listen mode will apply when the next gateway connection starts",
            );
        }
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
        let force_aec_for_route = self.has_shared_audio_route_risk();
        let ws_url_input = self.render_ws_url_input(window, cx);
        let authorization_input = self.render_authorization_input(window, cx);
        let device_id_input = self.render_device_id_input(window, cx);
        let client_id_input = self.render_client_id_input(window, cx);
        let close_view = cx.entity().downgrade();
        let save_view = cx.entity().downgrade();
        let toggle_ai_view = cx.entity().downgrade();
        let toggle_aec_view = cx.entity().downgrade();
        let set_manual_mode_view = cx.entity().downgrade();
        let set_auto_mode_view = cx.entity().downgrade();
        let set_realtime_mode_view = cx.entity().downgrade();

        div()
            .w(px(560.0))
            .max_h(px(640.0))
            .flex()
            .flex_col()
            .rounded_xl()
            .border_1()
            .border_color(rgb(0x243045))
            .bg(rgb(0x0b1019))
            .overflow_hidden()
            .child(
                div()
                    .h_12()
                    .px_5()
                    .pr_2()
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
                            .cursor_pointer()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(ui_button_icon(IconName::X, 13.0, ButtonIconTone::Ghost))
                            .on_click(move |_, window, cx| {
                                let _ = close_view
                                    .update(cx, |view, cx| view.close_settings_panel(window, cx));
                            }),
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
                        self.render_mcp_servers_section(window, cx),
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
                                    .child(editable_setting_row("Authorization", authorization_input))
                                    .child(editable_setting_row("Device-Id", device_id_input))
                                    .child(editable_setting_row("Client-Id", client_id_input)),
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
                                    .child(setting_row("编解码", "Opus (上行/下行)"))
                                    .child(setting_row(
                                        "AEC状态",
                                        if force_aec_for_route {
                                            "启用（AEC3，强制）".to_string()
                                        } else if self.aec_enabled_draft {
                                            "启用（AEC3）".to_string()
                                        } else {
                                            "关闭".to_string()
                                        },
                                    ))
                                    .child(setting_row(
                                        "流延迟（应用）",
                                        option_ms(self.aec_stream_delay_ms),
                                    ))
                                    .child(setting_row(
                                        "回调延迟（采集）",
                                        option_ms(self.aec_capture_callback_delay_ms),
                                    ))
                                    .child(setting_row(
                                        "回调延迟（播放）",
                                        option_ms(self.aec_playback_callback_delay_ms),
                                    ))
                                    .child(setting_row(
                                        "缓冲延迟（播放队列）",
                                        option_ms(self.aec_playback_buffer_delay_ms),
                                    ))
                                    .child(setting_row(
                                        "AEC估计延迟",
                                        option_i32_ms(self.aec_processor_delay_ms),
                                    ))
                                    .child(setting_row(
                                        "ERL / ERLE",
                                        format!(
                                            "{} dB / {} dB",
                                            option_db(self.aec_erl_db),
                                            option_db(self.aec_erle_db)
                                        ),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap_3()
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
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xd7e0f0))
                                            .child("实时回声消除（AEC）"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x7d8aa0))
                                            .whitespace_normal()
                                            .child(
                                                "使用 AEC3 处理麦克风上行，自动根据采集/播放回调延迟动态调节 stream delay",
                                            ),
                                    ),
                            )
                            .child(
                                div().flex_none().child(
                                    Button::new("toggle-aec-enabled")
                                        .h_8()
                                        .px_3()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .when(force_aec_for_route, |this| {
                                            this.warning().child("强制启用")
                                        })
                                        .when(!force_aec_for_route && self.aec_enabled_draft, |this| {
                                            this.success().child("已启用")
                                        })
                                        .when(!force_aec_for_route && !self.aec_enabled_draft, |this| {
                                            this.outline().child("已关闭")
                                        })
                                        .disabled(force_aec_for_route)
                                        .on_click(move |_, _, cx| {
                                            let _ = toggle_aec_view.update(cx, |view, cx| {
                                                view.toggle_aec_enabled(cx)
                                            });
                                        }),
                                ),
                            ),
                    )
                    .when(force_aec_for_route, |this| {
                        this.child(
                            div()
                                .rounded_lg()
                                .border_1()
                                .border_color(rgb(0x6f5320))
                                .bg(rgb(0x30230f))
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(0xf4c879))
                                .whitespace_normal()
                                .child("输入/输出使用同一路由，可能出现残留回声，建议分离采集与播放设备。"),
                        )
                    })
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
                                    .child(ui_icon(IconName::Mic, 13.0, 0x16d9c0))
                                    .child("语音监听模式"),
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
                                    .py_3()
                                    .gap_3()
                                    .child(setting_row(
                                        "当前模式",
                                        format!(
                                            "{} ({})",
                                            listen_mode_title(self.listen_mode_draft),
                                            listen_mode_code(self.listen_mode_draft)
                                        ),
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Button::new("listen-mode-manual")
                                                    .h_8()
                                                    .px_3()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .when(
                                                        self.listen_mode_draft
                                                            == ListenMode::Manual,
                                                        |this| this.success().child("manual"),
                                                    )
                                                    .when(
                                                        self.listen_mode_draft
                                                            != ListenMode::Manual,
                                                        |this| this.outline().child("manual"),
                                                    )
                                                    .on_click(move |_, _, cx| {
                                                        let _ = set_manual_mode_view.update(
                                                            cx,
                                                            |view, cx| {
                                                                view.set_listen_mode_draft(
                                                                    ListenMode::Manual,
                                                                    cx,
                                                                )
                                                            },
                                                        );
                                                    }),
                                            )
                                            .child(
                                                Button::new("listen-mode-auto")
                                                    .h_8()
                                                    .px_3()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .when(
                                                        self.listen_mode_draft == ListenMode::Auto,
                                                        |this| this.success().child("auto"),
                                                    )
                                                    .when(
                                                        self.listen_mode_draft != ListenMode::Auto,
                                                        |this| this.outline().child("auto"),
                                                    )
                                                    .on_click(move |_, _, cx| {
                                                        let _ = set_auto_mode_view.update(
                                                            cx,
                                                            |view, cx| {
                                                                view.set_listen_mode_draft(
                                                                    ListenMode::Auto,
                                                                    cx,
                                                                )
                                                            },
                                                        );
                                                    }),
                                            )
                                            .child(
                                                Button::new("listen-mode-realtime")
                                                    .h_8()
                                                    .px_3()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .when(
                                                        self.listen_mode_draft
                                                            == ListenMode::Realtime,
                                                        |this| this.success().child("realtime"),
                                                    )
                                                    .when(
                                                        self.listen_mode_draft
                                                            != ListenMode::Realtime,
                                                        |this| this.outline().child("realtime"),
                                                    )
                                                    .on_click(move |_, _, cx| {
                                                        let _ = set_realtime_mode_view.update(
                                                            cx,
                                                            |view, cx| {
                                                                view.set_listen_mode_draft(
                                                                    ListenMode::Realtime,
                                                                    cx,
                                                                )
                                                            },
                                                        );
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x7d8aa0))
                                            .whitespace_normal()
                                            .child(listen_mode_description(self.listen_mode_draft)),
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
                                    .items_start()
                                    .justify_between()
                                    .gap_3()
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
                                            .flex_1()
                                            .min_w_0()
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
                                                    .whitespace_normal()
                                                    .child("当 llm 只返回 emoji 等情绪符号时，是否展示在会话列表"),
                                            ),
                                    )
                                    .child(
                                        div().flex_none().child(
                                            Button::new("toggle-ai-emotion-messages")
                                                .h_8()
                                                .px_3()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .when(self.show_ai_emotion_messages_draft, |this| {
                                                    this.success().child("显示中")
                                                })
                                                .when(!self.show_ai_emotion_messages_draft, |this| {
                                                    this.outline().child("已隐藏")
                                                })
                                                .on_click(move |_, _, cx| {
                                                    let _ = toggle_ai_view.update(cx, |view, cx| {
                                                        view.toggle_ai_emotion_messages(cx)
                                                    });
                                                }),
                                        ),
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
                    .p_4()
                    .border_t_1()
                    .border_color(rgb(0x1a2435))
                    .flex()
                    .items_end()
                    .justify_end()
                    .child(
                        div()
                            .id("save-settings-button")
                            .h_9()
                            .px_5()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(0x17675d))
                            .bg(rgb(0x0f4d45))
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xe4fffb))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("保存并关闭")
                            .on_click(move |_, window, cx| {
                                let _ = save_view
                                    .update(cx, |view, cx| view.save_settings_panel(window, cx));
                            }),
                    ),
            )
    }
}

fn setting_row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> impl IntoElement {
    div()
        .h_10()
        .border_b_1()
        .border_color(rgb(0x1b2536))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x7d8aa0))
                .child(label.into()),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xd7e0f0))
                .text_ellipsis()
                .child(value.into()),
        )
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

fn option_ms(value: Option<u32>) -> String {
    value
        .map(|value| format!("{} ms", value))
        .unwrap_or_else(|| "-".to_string())
}

fn option_i32_ms(value: Option<i32>) -> String {
    value
        .map(|value| format!("{} ms", value))
        .unwrap_or_else(|| "-".to_string())
}

fn option_db(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "-".to_string())
}

fn listen_mode_code(mode: ListenMode) -> &'static str {
    match mode {
        ListenMode::Manual => "manual",
        ListenMode::Auto => "auto",
        ListenMode::Realtime => "realtime",
    }
}

fn listen_mode_title(mode: ListenMode) -> &'static str {
    match mode {
        ListenMode::Manual => "手动触发",
        ListenMode::Auto => "唤醒词触发",
        ListenMode::Realtime => "自由对话",
    }
}

fn listen_mode_description(mode: ListenMode) -> &'static str {
    match mode {
        ListenMode::Manual => "manual: 手动触发模式，设备端可按键控制开始/停止监听。",
        ListenMode::Auto => "auto: 唤醒词触发模式，可通过唤醒词触发并打断 AI 播放。",
        ListenMode::Realtime => "realtime: 自由对话全双工，检测到语音时可实时打断 AI 说话。",
    }
}

impl MeetingHostShell {
    fn render_ws_url_input(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.render_settings_text_input(
            "settings-ws-url-input",
            &self.ws_url_draft,
            self.ws_url_input_focused,
            Input::new(&self.ws_url_input_state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff)),
        )
    }

    fn render_authorization_input(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.render_settings_text_input(
            "settings-auth-token-input",
            &self.auth_token_draft,
            self.auth_token_input_focused,
            Input::new(&self.auth_token_input_state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff)),
        )
    }

    fn render_device_id_input(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.render_settings_text_input(
            "settings-device-id-input",
            &self.device_id_draft,
            self.device_id_input_focused,
            Input::new(&self.device_id_input_state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff)),
        )
    }

    fn render_client_id_input(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.render_settings_text_input(
            "settings-client-id-input",
            &self.client_id_draft,
            self.client_id_input_focused,
            Input::new(&self.client_id_input_state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff)),
        )
    }

    fn render_settings_text_input(
        &self,
        input_id: &'static str,
        value: &str,
        focused: bool,
        input: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .id(input_id)
            .flex_1()
            .h_10()
            .px_3()
            .rounded_md()
            .border_1()
            .cursor_text()
            .border_color(rgb(settings_input_border_color(focused, value)))
            .bg(rgb(0x090f1b))
            .flex()
            .items_center()
            .justify_between()
            .child(input)
    }
}

fn settings_input_border_color(focused: bool, value: &str) -> u32 {
    if focused {
        if value.trim().is_empty() {
            0xcf4d68
        } else {
            0x16d9c0
        }
    } else if value.trim().is_empty() {
        0x7f2230
    } else {
        0x283449
    }
}
