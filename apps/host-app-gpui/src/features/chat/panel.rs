use gpui::{div, prelude::*, rgb, ClickEvent, Context, Div, Window, WindowControlArea};
use gpui_component::input::Input;

use crate::app::shell::{ui_icon, MeetingHostShell};
use crate::app::state::ConnectionState;
use crate::components::icon::IconName;
use crate::components::ui::{
    floating_badge_button, message_empty_state, message_list, square_icon_button, text_input_shell,
    UiTone,
};

impl MeetingHostShell {
    pub(crate) fn render_chat_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let is_connected = matches!(self.connection_state, ConnectionState::Connected);
        let chat_title = format!("{} 条消息", self.chat_messages.len());

        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(0x040913))
            .child(self.render_chat_header(is_connected, &chat_title, cx))
            .child(self.render_message_stream_panel(cx))
            .child(self.render_chat_input_bar(window, is_connected, cx))
    }

    fn render_chat_header(
        &self,
        is_connected: bool,
        chat_title: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("chat-panel-drag-strip")
            .h_10()
            .px_4()
            .border_b_1()
            .border_color(rgb(0x182132))
            .bg(rgb(0x070f1b))
            .flex()
            .items_center()
            .justify_between()
            .window_control_area(WindowControlArea::Drag)
            .on_click(cx.listener(|_view, event: &ClickEvent, window, _cx| {
                if event.standard_click() && event.click_count() >= 2 {
                    window.titlebar_double_click();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(ui_icon(IconName::AudioLines, 14.0, 0x16d9c0))
                    .child(div().text_lg().text_color(rgb(0xd7deec)).child("会话记录"))
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x111928))
                            .text_xs()
                            .text_color(rgb(0x6f7c91))
                            .child(chat_title.to_string()),
                    ),
            )
            .children(if is_connected {
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .text_sm()
                        .text_color(rgb(0x8b95a9))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(div().size_1p5().rounded_full().bg(rgb(0x16d9c0)))
                                .child("LIVE"),
                        )
                        .child("PCM 16kHz / Opus"),
                )
            } else {
                None
            })
    }

    fn render_message_stream_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let mut message_stream = message_list(&self.chat_scroll);

        if self.chat_messages.is_empty() {
            message_stream =
                message_stream.child(message_empty_state("暂无消息，连接后会显示实时会话记录。"));
        } else {
            message_stream =
                message_stream.children(self.chat_messages.iter().rev().enumerate().map(
                    |(display_index, message)| {
                        let message_index = self
                            .chat_messages
                            .len()
                            .saturating_sub(display_index)
                            .saturating_sub(1);
                        self.render_chat_message(message_index, message, cx)
                    },
                ));
        }

        let mut message_stream_panel = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(message_stream);

        if self.pending_chat_messages > 0 && !self.follow_latest_chat_messages {
            let pending_chat_label = if self.pending_chat_messages > 99 {
                "99+ 条新消息".to_string()
            } else {
                format!("{} 条新消息", self.pending_chat_messages)
            };

            message_stream_panel = message_stream_panel.child(
                floating_badge_button(
                    ui_icon(IconName::ChevronDown, 12.0, 0x8fa8ca),
                    pending_chat_label,
                    UiTone::new(0x31425d, 0x111d30, 0xc6d4e7),
                    true,
                )
                .id("jump-to-latest-chat")
                .absolute()
                .right_5()
                .bottom_4()
                .on_click(
                    cx.listener(|view, _event, _window, cx| view.jump_to_latest_chat_messages(cx)),
                ),
            );
        }

        message_stream_panel
    }

    fn render_chat_input_bar(
        &mut self,
        window: &mut Window,
        is_connected: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h_16()
            .px_4()
            .border_t_1()
            .border_color(rgb(0x182132))
            .bg(rgb(0x070f1b))
            .flex()
            .items_center()
            .gap_2()
            .child(self.render_text_input_box(window, is_connected, cx))
            .child(self.render_send_button(is_connected, cx))
    }

    fn render_text_input_box(
        &mut self,
        _window: &mut Window,
        is_connected: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let border_hex = if self.chat_input_focused {
            if is_connected {
                0x16d9c0
            } else {
                0x4b5f82
            }
        } else if is_connected {
            0x145a58
        } else {
            0x283449
        };

        text_input_shell(
            border_hex,
            Input::new(&self.chat_input_state)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .text_color(rgb(0xe8efff))
                .suffix(div().text_sm().text_color(rgb(0x4f5e76)).child("Enter")),
        )
        .id("text-draft-input")
    }

    fn render_send_button(&self, is_connected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let can_send_text = is_connected && self.chat_input_has_text(cx);

        if can_send_text {
            square_icon_button(
                ui_icon(IconName::Send, 14.0, 0x9af9ef),
                UiTone::new(0x115f58, 0x0f5a54, 0x9af9ef),
                true,
            )
            .id("send-text-button")
            .on_click(cx.listener(|view, _event, window, cx| view.send_text_draft(window, cx)))
        } else {
            square_icon_button(
                ui_icon(IconName::Send, 14.0, 0x556178),
                UiTone::new(0x2a3448, 0x111928, 0x556178),
                false,
            )
            .id("send-text-button")
        }
    }
}
