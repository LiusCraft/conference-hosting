use std::collections::BTreeMap;
use std::thread;

use gpui::{div, prelude::*, rgb, Context, FontWeight, SharedString, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::Disableable as _;
use tokio::sync::oneshot;

use crate::app::shell::{ui_icon, MeetingHostShell};
use crate::app::state::{ConnectionState, GatewayCommand};
use crate::components::icon::IconName;
use crate::mcp::{
    new_mcp_server_id, normalize_server_config, preview_probe_statuses,
    probe_servers_with_dedicated_runtime, validate_server_config, McpProbeSnapshot, McpProbeState,
    McpServerConfig, McpServerProbeStatus, McpTransportConfig, McpTransportKind,
    DEFAULT_MCP_CONNECT_TIMEOUT_MS, DEFAULT_MCP_REQUEST_TIMEOUT_MS,
};

impl MeetingHostShell {
    pub(crate) fn reset_mcp_settings_drafts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mcp_servers_draft = self.mcp_servers.clone();
        self.show_mcp_editor = false;
        self.mcp_probe_in_progress = false;
        self.mcp_tools_expanded_servers.clear();
        self.mcp_form_error = None;
        self.mcp_form_notice = None;
        self.reset_mcp_editor(window, cx);
        self.refresh_mcp_probe_statuses_from_draft();
    }

    fn reset_mcp_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mcp_editor_server_id = None;
        self.mcp_editor_enabled = true;
        self.mcp_editor_transport = McpTransportKind::Stdio;
        self.set_mcp_input_value(&self.mcp_alias_input_state, "", window, cx);
        self.set_mcp_input_value(&self.mcp_endpoint_input_state, "", window, cx);
        self.set_mcp_input_value(&self.mcp_args_input_state, "", window, cx);
        self.set_mcp_input_value(&self.mcp_env_headers_input_state, "", window, cx);
        self.set_mcp_input_value(&self.mcp_cwd_input_state, "", window, cx);
        self.set_mcp_input_value(&self.mcp_auth_input_state, "", window, cx);
        self.set_mcp_input_value(
            &self.mcp_request_timeout_input_state,
            &DEFAULT_MCP_REQUEST_TIMEOUT_MS.to_string(),
            window,
            cx,
        );
        self.set_mcp_input_value(
            &self.mcp_connect_timeout_input_state,
            &DEFAULT_MCP_CONNECT_TIMEOUT_MS.to_string(),
            window,
            cx,
        );
    }

    fn set_mcp_input_value(
        &self,
        state: &gpui::Entity<gpui_component::input::InputState>,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = value.to_string();
        state.update(cx, move |input, cx| {
            input.set_value(&value, window, cx);
        });
    }

    fn refresh_mcp_probe_statuses_from_draft(&mut self) {
        self.mcp_server_statuses = preview_probe_statuses(&self.mcp_servers_draft);
        self.mcp_tools_expanded_servers.retain(|server_id| {
            self.mcp_server_statuses
                .iter()
                .any(|status| status.server_id == *server_id && !status.tools.is_empty())
        });
    }

    pub(crate) fn render_mcp_servers_section(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let mcp_count_label = format!("{} 个 server", self.mcp_servers_draft.len());

        let mut servers_panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x1d283a))
            .bg(rgb(0x101726))
            .px_3()
            .py_3();

        if self.mcp_servers_draft.is_empty() {
            servers_panel = servers_panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x7d8aa0))
                    .child("尚未添加 MCP server，可通过下方表单创建"),
            );
        } else {
            servers_panel = servers_panel.children(self.mcp_servers_draft.iter().enumerate().map(
                |(row_index, server)| {
                    let server_id = server.id.clone();
                    let edit_id = server_id.clone();
                    let delete_id = server_id.clone();
                    let toggle_id = server_id.clone();
                    let refresh_id = server_id.clone();
                    let tools_toggle_id = server_id.clone();
                    let edit_view = view.clone();
                    let toggle_view = view.clone();
                    let refresh_view = view.clone();
                    let delete_view = view.clone();
                    let tools_toggle_view = view.clone();
                    let status = self.probe_status_for_server(&server_id);
                    let endpoint_summary = summarize_endpoint(server.endpoint_summary().as_str());
                    let tool_summaries = status
                        .map(|status| status.tools.clone())
                        .unwrap_or_default();
                    let tool_count = tool_summaries.len();
                    let tools_expanded = self.mcp_tools_expanded_servers.contains(&server_id);
                    let tool_summaries_for_list = tool_summaries.clone();

                    let tools_button = if tool_count == 0 {
                        Button::new(("mcp-tools-toggle", row_index))
                            .outline()
                            .h_7()
                            .px_3()
                            .text_xs()
                            .child("无 tools")
                            .into_any_element()
                    } else {
                        let label = if tools_expanded {
                            "收起 tools".to_string()
                        } else {
                            format!("查看 tools ({tool_count})")
                        };

                        Button::new(("mcp-tools-toggle", row_index))
                            .info()
                            .h_7()
                            .px_3()
                            .text_xs()
                            .child(label)
                            .on_click(move |_, _, cx| {
                                let _ = tools_toggle_view.update(cx, |shell, cx| {
                                    shell.toggle_mcp_server_tools_visibility(
                                        tools_toggle_id.as_str(),
                                        cx,
                                    )
                                });
                            })
                            .into_any_element()
                    };

                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(0x1b2536))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0xe0e8f6))
                                                .child(server.alias.clone()),
                                        )
                                        .child(mcp_badge(
                                            server.transport_kind().as_label(),
                                            0x1d314e,
                                            0xb6c8ea,
                                        ))
                                        .child(if server.enabled {
                                            mcp_badge("enabled", 0x0f4d45, 0xaff7ee)
                                        } else {
                                            mcp_badge("disabled", 0x312033, 0xf3b7c7)
                                        }),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x7a879e))
                                        .child(format_probe_label(status)),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x7f8ca2))
                                .child(endpoint_summary),
                        )
                        .child(
                            div().flex().items_center().gap_2().children([
                                Button::new(("mcp-edit-server", row_index))
                                    .outline()
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .child("编辑")
                                    .on_click(move |_, window, cx| {
                                        let _ = edit_view.update(cx, |shell, cx| {
                                            shell.edit_mcp_server(edit_id.as_str(), window, cx)
                                        });
                                    })
                                    .into_any_element(),
                                Button::new(("mcp-toggle-server", row_index))
                                    .outline()
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .child(if server.enabled { "禁用" } else { "启用" })
                                    .on_click(move |_, _, cx| {
                                        let _ = toggle_view.update(cx, |shell, cx| {
                                            shell.toggle_mcp_server_enabled(toggle_id.as_str(), cx)
                                        });
                                    })
                                    .into_any_element(),
                                Button::new(("mcp-refresh-server", row_index))
                                    .outline()
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .disabled(self.mcp_probe_in_progress)
                                    .child("刷新工具")
                                    .on_click(move |_, _, cx| {
                                        let _ = refresh_view.update(cx, |shell, cx| {
                                            shell.refresh_mcp_server(refresh_id.as_str(), cx)
                                        });
                                    })
                                    .into_any_element(),
                                Button::new(("mcp-delete-server", row_index))
                                    .danger()
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .child("删除")
                                    .on_click(move |_, _, cx| {
                                        let _ = delete_view.update(cx, |shell, cx| {
                                            shell.delete_mcp_server(delete_id.as_str(), cx)
                                        });
                                    })
                                    .into_any_element(),
                                tools_button,
                            ]),
                        )
                        .children(
                            (tools_expanded && !tool_summaries_for_list.is_empty()).then(|| {
                                div().flex().flex_col().gap_1().children(
                                    tool_summaries_for_list.into_iter().map(|tool| {
                                        div()
                                            .rounded_md()
                                            .bg(rgb(0x111a2b))
                                            .px_2()
                                            .py_1()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0xcad5ea))
                                                    .child(format!("- {}", tool.name)),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x8794aa))
                                                    .whitespace_normal()
                                                    .child(if tool.description.trim().is_empty() {
                                                        "暂无描述".to_string()
                                                    } else {
                                                        tool.description
                                                    }),
                                            )
                                    }),
                                )
                            }),
                        )
                },
            ));
        }

        let transport_switch_view = cx.entity().downgrade();
        let transport_switch_view_stdio = transport_switch_view.clone();
        let transport_switch_view_sse = transport_switch_view.clone();
        let transport_switch_view_stream = transport_switch_view.clone();
        let add_view = cx.entity().downgrade();
        let save_view = cx.entity().downgrade();
        let cancel_view = cx.entity().downgrade();
        let refresh_all_view = cx.entity().downgrade();
        let is_editing = self.mcp_editor_server_id.is_some();

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
                    .child(ui_icon(IconName::Wrench, 13.0, 0x16d9c0))
                    .child("MCP Servers")
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(0x1a2435))
                            .text_xs()
                            .text_color(rgb(0x7f8ba1))
                            .child(mcp_count_label),
                    ),
            )
            .child(servers_panel)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        Button::new("mcp-add-server")
                            .outline()
                            .h_8()
                            .px_3()
                            .text_xs()
                            .child("新增 server")
                            .on_click(move |_, window, cx| {
                                let _ = add_view
                                    .update(cx, |shell, cx| shell.begin_add_mcp_server(window, cx));
                            }),
                    )
                    .child(
                        Button::new("mcp-refresh-all")
                            .outline()
                            .h_8()
                            .px_3()
                            .text_xs()
                            .disabled(self.mcp_probe_in_progress)
                            .child(if self.mcp_probe_in_progress {
                                "刷新中..."
                            } else {
                                "立即刷新工具"
                            })
                            .on_click(move |_, _, cx| {
                                let _ = refresh_all_view
                                    .update(cx, |shell, cx| shell.refresh_all_mcp_servers(cx));
                            }),
                    ),
            )
            .children(if self.show_mcp_editor {
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(0x1d283a))
                        .bg(rgb(0x0f1625))
                        .px_3()
                        .py_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xdce4f3))
                                        .child(if is_editing {
                                            "编辑 MCP Server"
                                        } else {
                                            "新增 MCP Server"
                                        }),
                                )
                                .child(if self.mcp_editor_enabled {
                                    mcp_badge("enabled", 0x0f4d45, 0xaff7ee)
                                } else {
                                    mcp_badge("disabled", 0x312033, 0xf3b7c7)
                                }),
                        )
                        .child(
                            div().flex().items_center().gap_2().children([
                                Button::new("mcp-transport-stdio")
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(
                                        self.mcp_editor_transport == McpTransportKind::Stdio,
                                        |this| this.info(),
                                    )
                                    .when(
                                        self.mcp_editor_transport != McpTransportKind::Stdio,
                                        |this| this.outline(),
                                    )
                                    .child("stdio")
                                    .on_click(move |_, _, cx| {
                                        let _ =
                                            transport_switch_view_stdio.update(cx, |shell, cx| {
                                                shell.switch_mcp_transport(
                                                    McpTransportKind::Stdio,
                                                    cx,
                                                )
                                            });
                                    })
                                    .into_any_element(),
                                Button::new("mcp-transport-sse")
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(
                                        self.mcp_editor_transport == McpTransportKind::Sse,
                                        |this| this.info(),
                                    )
                                    .when(
                                        self.mcp_editor_transport != McpTransportKind::Sse,
                                        |this| this.outline(),
                                    )
                                    .child("sse")
                                    .on_click(move |_, _, cx| {
                                        let _ =
                                            transport_switch_view_sse.update(cx, |shell, cx| {
                                                shell
                                                    .switch_mcp_transport(McpTransportKind::Sse, cx)
                                            });
                                    })
                                    .into_any_element(),
                                Button::new("mcp-transport-stream")
                                    .h_7()
                                    .px_3()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(
                                        self.mcp_editor_transport == McpTransportKind::Stream,
                                        |this| this.info(),
                                    )
                                    .when(
                                        self.mcp_editor_transport != McpTransportKind::Stream,
                                        |this| this.outline(),
                                    )
                                    .child("stream")
                                    .on_click(move |_, _, cx| {
                                        let _ =
                                            transport_switch_view_stream.update(cx, |shell, cx| {
                                                shell.switch_mcp_transport(
                                                    McpTransportKind::Stream,
                                                    cx,
                                                )
                                            });
                                    })
                                    .into_any_element(),
                            ]),
                        )
                        .child(mcp_editable_row(
                            "Alias",
                            mcp_input_field("mcp-alias-input", &self.mcp_alias_input_state),
                        ))
                        .child(mcp_editable_row(
                            if self.mcp_editor_transport == McpTransportKind::Stdio {
                                "Command"
                            } else {
                                "URL"
                            },
                            mcp_input_field("mcp-endpoint-input", &self.mcp_endpoint_input_state),
                        ))
                        .children(Some(match self.mcp_editor_transport {
                            McpTransportKind::Stdio => div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(mcp_editable_row(
                                    "Args (逗号分隔)",
                                    mcp_input_field("mcp-args-input", &self.mcp_args_input_state),
                                ))
                                .child(mcp_editable_row(
                                    "Env (k=v, 逗号分隔)",
                                    mcp_input_field(
                                        "mcp-env-input",
                                        &self.mcp_env_headers_input_state,
                                    ),
                                ))
                                .child(mcp_editable_row(
                                    "CWD (可选)",
                                    mcp_input_field("mcp-cwd-input", &self.mcp_cwd_input_state),
                                ))
                                .into_any_element(),
                            McpTransportKind::Sse => mcp_editable_row(
                                "Headers (k=v, 逗号分隔)",
                                mcp_input_field("mcp-headers-input", &self.mcp_args_input_state),
                            )
                            .into_any_element(),
                            McpTransportKind::Stream => div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(mcp_editable_row(
                                    "Headers (k=v, 逗号分隔)",
                                    mcp_input_field(
                                        "mcp-stream-headers",
                                        &self.mcp_args_input_state,
                                    ),
                                ))
                                .child(mcp_editable_row(
                                    "Auth (可选)",
                                    mcp_input_field("mcp-stream-auth", &self.mcp_auth_input_state),
                                ))
                                .into_any_element(),
                        }))
                        .child(mcp_editable_row(
                            "Request Timeout (ms)",
                            mcp_input_field(
                                "mcp-request-timeout",
                                &self.mcp_request_timeout_input_state,
                            ),
                        ))
                        .child(mcp_editable_row(
                            "Connect Timeout (ms)",
                            mcp_input_field(
                                "mcp-connect-timeout",
                                &self.mcp_connect_timeout_input_state,
                            ),
                        ))
                        .children(self.mcp_form_notice.as_ref().map(|message| {
                            div()
                                .rounded_md()
                                .bg(rgb(0x113627))
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(0x9df4c8))
                                .whitespace_normal()
                                .child(message.clone())
                        }))
                        .children(self.mcp_form_error.as_ref().map(|error| {
                            div()
                                .rounded_md()
                                .bg(rgb(0x3a1b25))
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(0xffc7d6))
                                .whitespace_normal()
                                .child(error.clone())
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("mcp-cancel-editor")
                                        .outline()
                                        .h_8()
                                        .px_3()
                                        .text_xs()
                                        .child("重置")
                                        .on_click(move |_, window, cx| {
                                            let _ = cancel_view.update(cx, |shell, cx| {
                                                shell.cancel_mcp_editor(window, cx)
                                            });
                                        }),
                                )
                                .child(
                                    Button::new("mcp-save-editor")
                                        .primary()
                                        .h_8()
                                        .px_4()
                                        .text_xs()
                                        .child(if is_editing {
                                            "保存修改"
                                        } else {
                                            "添加 server"
                                        })
                                        .on_click(move |_, window, cx| {
                                            let _ = save_view.update(cx, |shell, cx| {
                                                shell.save_mcp_editor(window, cx)
                                            });
                                        }),
                                ),
                        )
                        .into_any_element(),
                )
            } else {
                None
            })
    }

    fn probe_status_for_server(&self, server_id: &str) -> Option<&McpServerProbeStatus> {
        self.mcp_server_statuses
            .iter()
            .find(|status| status.server_id == server_id)
    }

    pub(crate) fn begin_add_mcp_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_mcp_editor = true;
        self.mcp_form_error = None;
        self.mcp_form_notice = Some("已切换到新增模式".to_string());
        self.reset_mcp_editor(window, cx);
        self.notify_views(cx);
    }

    fn edit_mcp_server(&mut self, server_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self
            .mcp_servers_draft
            .iter()
            .find(|server| server.id == server_id)
            .cloned()
        else {
            self.mcp_form_error = Some("未找到待编辑的 MCP server".to_string());
            self.notify_views(cx);
            return;
        };

        self.show_mcp_editor = true;
        self.mcp_editor_server_id = Some(server.id.clone());
        self.mcp_editor_enabled = server.enabled;
        self.mcp_editor_transport = server.transport_kind();
        self.set_mcp_input_value(&self.mcp_alias_input_state, &server.alias, window, cx);

        match server.transport {
            McpTransportConfig::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                self.set_mcp_input_value(&self.mcp_endpoint_input_state, &command, window, cx);
                self.set_mcp_input_value(&self.mcp_args_input_state, &args.join(", "), window, cx);
                self.set_mcp_input_value(
                    &self.mcp_env_headers_input_state,
                    &join_kv_pairs(&env),
                    window,
                    cx,
                );
                self.set_mcp_input_value(
                    &self.mcp_cwd_input_state,
                    cwd.as_deref().unwrap_or(""),
                    window,
                    cx,
                );
                self.set_mcp_input_value(&self.mcp_auth_input_state, "", window, cx);
            }
            McpTransportConfig::Sse { url, headers } => {
                self.set_mcp_input_value(&self.mcp_endpoint_input_state, &url, window, cx);
                self.set_mcp_input_value(
                    &self.mcp_args_input_state,
                    &join_kv_pairs(&headers),
                    window,
                    cx,
                );
                self.set_mcp_input_value(&self.mcp_env_headers_input_state, "", window, cx);
                self.set_mcp_input_value(&self.mcp_cwd_input_state, "", window, cx);
                self.set_mcp_input_value(&self.mcp_auth_input_state, "", window, cx);
            }
            McpTransportConfig::Stream { url, headers, auth } => {
                self.set_mcp_input_value(&self.mcp_endpoint_input_state, &url, window, cx);
                self.set_mcp_input_value(
                    &self.mcp_args_input_state,
                    &join_kv_pairs(&headers),
                    window,
                    cx,
                );
                self.set_mcp_input_value(&self.mcp_env_headers_input_state, "", window, cx);
                self.set_mcp_input_value(&self.mcp_cwd_input_state, "", window, cx);
                self.set_mcp_input_value(
                    &self.mcp_auth_input_state,
                    auth.as_deref().unwrap_or(""),
                    window,
                    cx,
                );
            }
        }

        self.set_mcp_input_value(
            &self.mcp_request_timeout_input_state,
            &server.request_timeout_ms.to_string(),
            window,
            cx,
        );
        self.set_mcp_input_value(
            &self.mcp_connect_timeout_input_state,
            &server.connect_timeout_ms.to_string(),
            window,
            cx,
        );
        self.mcp_form_error = None;
        self.mcp_form_notice = Some(format!("正在编辑 `{}`", server.alias));
        self.notify_views(cx);
    }

    fn cancel_mcp_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_mcp_editor = false;
        self.mcp_form_error = None;
        self.mcp_form_notice = None;
        self.reset_mcp_editor(window, cx);
        self.notify_views(cx);
    }

    fn switch_mcp_transport(&mut self, transport: McpTransportKind, cx: &mut Context<Self>) {
        if self.mcp_editor_transport == transport {
            return;
        }

        self.mcp_editor_transport = transport;
        self.mcp_form_error = None;
        self.notify_views(cx);
    }

    fn save_mcp_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next_server = match self.build_mcp_server_from_editor(cx) {
            Ok(server) => server,
            Err(error) => {
                self.mcp_form_error = Some(error);
                self.mcp_form_notice = None;
                self.notify_views(cx);
                return;
            }
        };

        let editing_server_id = self.mcp_editor_server_id.clone();
        let validation_error = validate_server_config(
            &next_server,
            &self.mcp_servers_draft,
            editing_server_id.as_deref(),
        )
        .err();
        if let Some(error) = validation_error {
            self.mcp_form_error = Some(error);
            self.mcp_form_notice = None;
            self.notify_views(cx);
            return;
        }

        if let Some(editing_server_id) = editing_server_id {
            if let Some(existing_server) = self
                .mcp_servers_draft
                .iter_mut()
                .find(|server| server.id == editing_server_id)
            {
                *existing_server = next_server.clone();
            }
            self.mcp_form_notice = Some(format!("已更新 `{}`", next_server.alias));
        } else {
            self.mcp_servers_draft.push(next_server.clone());
            self.mcp_form_notice = Some(format!("已添加 `{}`", next_server.alias));
        }

        self.mcp_form_error = None;
        self.refresh_mcp_probe_statuses_from_draft();
        self.show_mcp_editor = false;
        self.reset_mcp_editor(window, cx);
        self.maybe_refresh_gateway_mcp_tools();
        self.notify_views(cx);
    }

    fn build_mcp_server_from_editor(
        &self,
        cx: &mut Context<Self>,
    ) -> Result<McpServerConfig, String> {
        let alias = self
            .mcp_alias_input_state
            .read(cx)
            .value()
            .trim()
            .to_string();
        if alias.is_empty() {
            return Err("alias 不能为空".to_string());
        }

        let endpoint = self
            .mcp_endpoint_input_state
            .read(cx)
            .value()
            .trim()
            .to_string();
        let args_or_headers = self.mcp_args_input_state.read(cx).value().to_string();
        let env_or_headers = self
            .mcp_env_headers_input_state
            .read(cx)
            .value()
            .to_string();
        let cwd = self.mcp_cwd_input_state.read(cx).value().trim().to_string();
        let auth = self
            .mcp_auth_input_state
            .read(cx)
            .value()
            .trim()
            .to_string();

        let request_timeout_ms = parse_timeout_ms(
            self.mcp_request_timeout_input_state
                .read(cx)
                .value()
                .as_ref(),
            "request_timeout_ms",
            DEFAULT_MCP_REQUEST_TIMEOUT_MS,
        )?;
        let connect_timeout_ms = parse_timeout_ms(
            self.mcp_connect_timeout_input_state
                .read(cx)
                .value()
                .as_ref(),
            "connect_timeout_ms",
            DEFAULT_MCP_CONNECT_TIMEOUT_MS,
        )?;

        let transport = match self.mcp_editor_transport {
            McpTransportKind::Stdio => McpTransportConfig::Stdio {
                command: endpoint,
                args: parse_comma_list(args_or_headers.as_str()),
                env: parse_kv_pairs(env_or_headers.as_str())?,
                cwd: if cwd.is_empty() { None } else { Some(cwd) },
            },
            McpTransportKind::Sse => McpTransportConfig::Sse {
                url: endpoint,
                headers: parse_kv_pairs(args_or_headers.as_str())?,
            },
            McpTransportKind::Stream => McpTransportConfig::Stream {
                url: endpoint,
                headers: parse_kv_pairs(args_or_headers.as_str())?,
                auth: if auth.is_empty() { None } else { Some(auth) },
            },
        };

        let server = McpServerConfig {
            id: self
                .mcp_editor_server_id
                .clone()
                .unwrap_or_else(|| new_mcp_server_id(alias.as_str())),
            alias,
            enabled: self.mcp_editor_enabled,
            transport,
            request_timeout_ms,
            connect_timeout_ms,
        };

        Ok(normalize_server_config(server))
    }

    fn delete_mcp_server(&mut self, server_id: &str, cx: &mut Context<Self>) {
        let previous_count = self.mcp_servers_draft.len();
        self.mcp_servers_draft
            .retain(|server| server.id != server_id);
        if previous_count == self.mcp_servers_draft.len() {
            self.mcp_form_error = Some("待删除的 server 不存在".to_string());
            self.notify_views(cx);
            return;
        }

        if self
            .mcp_editor_server_id
            .as_deref()
            .is_some_and(|editing_id| editing_id == server_id)
        {
            self.show_mcp_editor = false;
            self.mcp_editor_server_id = None;
        }
        self.mcp_tools_expanded_servers.remove(server_id);

        self.mcp_form_error = None;
        self.mcp_form_notice = Some("已删除 MCP server".to_string());
        self.refresh_mcp_probe_statuses_from_draft();
        self.maybe_refresh_gateway_mcp_tools();
        self.notify_views(cx);
    }

    fn toggle_mcp_server_enabled(&mut self, server_id: &str, cx: &mut Context<Self>) {
        let Some(server) = self
            .mcp_servers_draft
            .iter_mut()
            .find(|server| server.id == server_id)
        else {
            self.mcp_form_error = Some("未找到目标 MCP server".to_string());
            self.notify_views(cx);
            return;
        };

        server.enabled = !server.enabled;
        self.mcp_form_error = None;
        self.mcp_form_notice = Some(format!(
            "`{}` 已{}",
            server.alias,
            if server.enabled { "启用" } else { "禁用" }
        ));
        self.refresh_mcp_probe_statuses_from_draft();
        self.maybe_refresh_gateway_mcp_tools();
        self.notify_views(cx);
    }

    fn refresh_mcp_server(&mut self, server_id: &str, cx: &mut Context<Self>) {
        if self.mcp_probe_in_progress {
            self.mcp_form_notice = Some("MCP 刷新进行中，请稍候".to_string());
            self.notify_views(cx);
            return;
        }

        let alias = self
            .mcp_servers_draft
            .iter()
            .find(|server| server.id == server_id)
            .map(|server| server.alias.clone())
            .unwrap_or_else(|| "目标 server".to_string());
        self.refresh_mcp_probe_statuses_from_draft();
        self.mcp_form_error = None;
        self.mcp_form_notice = Some(format!("正在刷新 `{alias}` 的 tools..."));
        self.start_local_mcp_probe(cx);
        self.maybe_refresh_gateway_mcp_tools();
        self.notify_views(cx);
    }

    fn toggle_mcp_server_tools_visibility(&mut self, server_id: &str, cx: &mut Context<Self>) {
        if self.mcp_tools_expanded_servers.contains(server_id) {
            self.mcp_tools_expanded_servers.remove(server_id);
        } else {
            self.mcp_tools_expanded_servers
                .insert(server_id.to_string());
        }

        self.notify_views(cx);
    }

    fn refresh_all_mcp_servers(&mut self, cx: &mut Context<Self>) {
        if self.mcp_probe_in_progress {
            self.mcp_form_notice = Some("MCP 刷新进行中，请稍候".to_string());
            self.notify_views(cx);
            return;
        }

        self.refresh_mcp_probe_statuses_from_draft();
        self.mcp_form_error = None;
        self.mcp_form_notice = Some("正在刷新全部 MCP server tools...".to_string());
        self.start_local_mcp_probe(cx);
        self.maybe_refresh_gateway_mcp_tools();
        self.notify_views(cx);
    }

    fn start_local_mcp_probe(&mut self, cx: &mut Context<Self>) {
        self.mcp_probe_in_progress = true;
        let servers = self.mcp_servers_draft.clone();
        let (result_tx, result_rx) = oneshot::channel::<Result<McpProbeSnapshot, String>>();

        thread::spawn(move || {
            let _ = result_tx.send(probe_servers_with_dedicated_runtime(servers));
        });

        cx.spawn(async move |this, cx| {
            let result = match result_rx.await {
                Ok(result) => result,
                Err(_) => Err("MCP 刷新任务已中断".to_string()),
            };

            let _ = this.update(cx, |shell, cx| {
                shell.finish_local_mcp_probe(result, cx);
            });
        })
        .detach();
    }

    fn finish_local_mcp_probe(
        &mut self,
        result: Result<McpProbeSnapshot, String>,
        cx: &mut Context<Self>,
    ) {
        self.mcp_probe_in_progress = false;

        match result {
            Ok(snapshot) => {
                self.mcp_server_statuses = snapshot.statuses;
                self.mcp_tools_expanded_servers.retain(|server_id| {
                    self.mcp_server_statuses
                        .iter()
                        .any(|status| status.server_id == *server_id && !status.tools.is_empty())
                });

                let failed_servers = self
                    .mcp_server_statuses
                    .iter()
                    .filter(|status| status.state == McpProbeState::Failed)
                    .map(|status| format!("{} ({})", status.alias, status.detail))
                    .collect::<Vec<_>>();

                if failed_servers.is_empty() {
                    self.mcp_form_error = None;
                    self.mcp_form_notice = Some(format!(
                        "MCP 刷新完成，共发现 {} 个 tools",
                        snapshot.tool_count
                    ));
                } else {
                    self.mcp_form_notice = Some(format!(
                        "MCP 刷新完成，当前共 {} 个 tools",
                        snapshot.tool_count
                    ));
                    self.mcp_form_error = Some(format!(
                        "部分 server 刷新失败: {}",
                        failed_servers.join("; ")
                    ));
                }
            }
            Err(error) => {
                self.mcp_form_notice = None;
                self.mcp_form_error = Some(format!("MCP 刷新失败: {error}"));
            }
        }

        self.notify_views(cx);
    }

    fn maybe_refresh_gateway_mcp_tools(&mut self) {
        if self.mcp_servers_draft != self.mcp_servers {
            return;
        }

        if !matches!(self.connection_state, ConnectionState::Connected) {
            return;
        }

        if let Some(command_tx) = self.ws_command_tx.as_ref() {
            let _ = command_tx.try_send(GatewayCommand::RefreshMcpTools);
        }
    }
}

fn mcp_badge(label: impl Into<SharedString>, bg_color: u32, fg_color: u32) -> impl IntoElement {
    div()
        .px_2()
        .py_0p5()
        .rounded_sm()
        .bg(rgb(bg_color))
        .text_xs()
        .text_color(rgb(fg_color))
        .child(label.into())
}

fn mcp_input_field(
    input_id: &'static str,
    state: &gpui::Entity<gpui_component::input::InputState>,
) -> impl IntoElement {
    div()
        .id(input_id)
        .h_10()
        .px_3()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x2a3346))
        .bg(rgb(0x090f1b))
        .flex()
        .items_center()
        .child(
            Input::new(state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff)),
        )
}

fn mcp_editable_row(label: impl Into<SharedString>, field: impl IntoElement) -> impl IntoElement {
    div()
        .py_2()
        .border_b_1()
        .border_color(rgb(0x1b2536))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x7d8aa0))
                .child(label.into()),
        )
        .child(field)
}

fn parse_timeout_ms(raw: &str, field_name: &str, fallback: u64) -> Result<u64, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(fallback);
    }

    let value = raw
        .parse::<u64>()
        .map_err(|_| format!("{field_name} 必须是数字"))?;
    if value == 0 {
        return Err(format!("{field_name} 必须大于 0"));
    }

    Ok(value)
}

fn parse_comma_list(raw: &str) -> Vec<String> {
    raw.split([',', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_kv_pairs(raw: &str) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(values);
    }

    for item in trimmed.split([',', '\n']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }

        let pair = item
            .split_once('=')
            .or_else(|| item.split_once(':'))
            .ok_or_else(|| format!("键值对 `{item}` 缺少分隔符，格式应为 key=value"))?;
        let key = pair.0.trim();
        let value = pair.1.trim();
        if key.is_empty() || value.is_empty() {
            return Err(format!("键值对 `{item}` 不完整，格式应为 key=value"));
        }

        values.insert(key.to_string(), value.to_string());
    }

    Ok(values)
}

fn join_kv_pairs(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn summarize_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim();
    if endpoint.chars().count() <= 64 {
        endpoint.to_string()
    } else {
        let head: String = endpoint.chars().take(61).collect();
        format!("{head}...")
    }
}

fn format_probe_label(status: Option<&McpServerProbeStatus>) -> String {
    let Some(status) = status else {
        return "未探测 / tools=0".to_string();
    };

    let mut label = format!("{} / tools={}", status.state.as_label(), status.tool_count);
    if status.state == McpProbeState::Failed {
        label.push_str(&format!(" / {}", status.detail));
    }
    label
}
