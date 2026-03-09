use gpui::{div, prelude::*, rgb, ClickEvent, Context, Div, Window, WindowControlArea};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::Disableable;

use crate::app::shell::{ui_button_icon, ButtonIconTone, MeetingHostShell};
use crate::app::state::{ChatRole, ConnectionState};
use crate::components::icon::IconName;

const LIVE_CHAT_RENDER_LIMIT: usize = 80;

impl MeetingHostShell {
    pub(crate) fn render_chat_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let is_connected = matches!(self.connection_state, ConnectionState::Connected);
        let ai_reply_count = self
            .chat_messages
            .iter()
            .filter(|message| message.role == ChatRole::Assistant)
            .count();
        let chat_title = format!("{} 条消息", ai_reply_count);

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
                    .child(ui_button_icon(
                        IconName::AudioLines,
                        14.0,
                        ButtonIconTone::Info,
                    ))
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
        let total_messages = self.chat_messages.len();
        let live_window_start = if self.follow_latest_chat_messages
            && !self.render_full_chat_history
            && total_messages > LIVE_CHAT_RENDER_LIMIT
        {
            total_messages - LIVE_CHAT_RENDER_LIMIT
        } else {
            0
        };
        let hidden_message_count = live_window_start;

        let mut message_stream = div()
            .id("message-stream")
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .track_scroll(&self.chat_scroll)
            .overflow_y_scroll()
            .scrollbar_width(gpui::px(10.0))
            .pr_2()
            .py_3()
            .bg(rgb(0x050912));

        if self.chat_messages.is_empty() {
            message_stream = message_stream.child(
                div()
                    .px_4()
                    .py_3()
                    .text_sm()
                    .text_color(rgb(0x7b8798))
                    .child("暂无消息，连接后会显示实时会话记录。"),
            );
        } else {
            if hidden_message_count > 0 {
                let view = cx.entity().downgrade();
                message_stream = message_stream.child(
                    div()
                        .mx_4()
                        .mb_2()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(0x223148))
                        .bg(rgb(0x0c1421))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_xs().text_color(rgb(0x7f8ca3)).child(format!(
                            "实时模式仅渲染最近 {} 条消息（已折叠 {} 条）",
                            LIVE_CHAT_RENDER_LIMIT, hidden_message_count
                        )))
                        .child(
                            Button::new("expand-full-chat-history")
                                .outline()
                                .h_7()
                                .px_3()
                                .text_xs()
                                .child("查看全部")
                                .on_click(move |_, _, cx| {
                                    let _ = view
                                        .update(cx, |view, cx| view.expand_full_chat_history(cx));
                                }),
                        ),
                );
            }

            message_stream = message_stream.children(
                self.chat_messages[live_window_start..]
                    .iter()
                    .rev()
                    .enumerate()
                    .map(|(display_index, message)| {
                        let message_index = total_messages
                            .saturating_sub(display_index)
                            .saturating_sub(1);
                        self.render_chat_message(message_index, message, cx)
                    }),
            );
        }

        let mut message_stream_panel = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(message_stream);

        if self.has_pending_chat_messages && !self.follow_latest_chat_messages {
            let pending_chat_label = if self.pending_chat_messages > 99 {
                "99+ 条新消息".to_string()
            } else if self.pending_chat_messages > 0 {
                format!("{} 条新消息", self.pending_chat_messages)
            } else {
                "有新消息".to_string()
            };
            let view = cx.entity().downgrade();

            message_stream_panel = message_stream_panel.child(
                Button::new("jump-to-latest-chat")
                    .info()
                    .h_11()
                    .px_4()
                    .rounded_full()
                    .absolute()
                    .right_5()
                    .bottom_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(ui_button_icon(
                                IconName::ChevronDown,
                                12.0,
                                ButtonIconTone::Info,
                            ))
                            .child(pending_chat_label),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |view, cx| view.jump_to_latest_chat_messages(cx));
                    }),
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

        div()
            .id("text-draft-input")
            .flex_1()
            .h_11()
            .px_3()
            .rounded_md()
            .border_1()
            .cursor_text()
            .border_color(rgb(border_hex))
            .bg(rgb(0x090f1b))
            .flex()
            .items_center()
            .justify_between()
            .child(
                Input::new(&self.chat_input_state)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .text_color(rgb(0xe8efff))
                    .suffix(div().text_sm().text_color(rgb(0x4f5e76)).child("Enter")),
            )
    }

    fn render_send_button(&self, is_connected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let can_send_text = is_connected && self.chat_input_has_text(cx);
        let view = cx.entity().downgrade();

        if can_send_text {
            Button::new("send-text-button")
                .primary()
                .size_11()
                .p_0()
                .child(ui_button_icon(
                    IconName::Send,
                    14.0,
                    ButtonIconTone::Primary,
                ))
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.send_text_draft(window, cx));
                })
        } else {
            Button::new("send-text-button")
                .outline()
                .disabled(true)
                .size_11()
                .p_0()
                .child(ui_button_icon(
                    IconName::Send,
                    14.0,
                    ButtonIconTone::Disabled,
                ))
        }
    }
}
