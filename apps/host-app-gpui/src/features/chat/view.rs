use gpui::{div, prelude::*, rgb, Context, FontWeight, SharedString};
use gpui_component::button::{Button, ButtonVariants as _};

use crate::app::shell::{ui_icon, MeetingHostShell};
use crate::app::state::{ChatMessage, ChatRole};
use crate::components::icon::IconName;

use super::chat_message_header;

impl MeetingHostShell {
    pub(crate) fn render_chat_message(
        &self,
        message_index: usize,
        message: &ChatMessage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let header = chat_message_header(message);

        match message.role {
            ChatRole::System => div().flex().justify_center().py_1().child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_full()
                    .bg(rgb(0x0e1624))
                    .border_1()
                    .border_color(rgb(0x202a3b))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x6f7c90))
                            .child(format!("{} | {}", header, message.body)),
                    ),
            ),
            ChatRole::Tool => div()
                .flex()
                .min_w_0()
                .items_start()
                .px_4()
                .py_1()
                .gap_2()
                .child(
                    div()
                        .size_5()
                        .mt_1()
                        .rounded_md()
                        .bg(rgb(0x2b1d0c))
                        .border_1()
                        .border_color(rgb(0x70451d))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui_icon(IconName::Wrench, 12.0, 0xd98a3c)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xd98a3c))
                                .whitespace_normal()
                                .child(message.body.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x7f8899))
                                .whitespace_normal()
                                .child(header.clone()),
                        ),
                ),
            ChatRole::Trace => {
                let trace_lines: Vec<SharedString> = message
                    .body
                    .as_ref()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|line| line.to_string().into())
                    .collect();
                let trace_step_count = trace_lines.len();
                let is_collapsible = trace_step_count > 1;
                let is_collapsed = is_collapsible && message.trace_collapsed;
                let trace_preview = trace_lines.last().cloned();

                let mut trace_content = div().flex().flex_col().min_w_0().flex_1().gap_1().child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xe0b24f))
                        .whitespace_normal()
                        .child(header.clone()),
                );

                if is_collapsed {
                    trace_content = trace_content
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xe8d8b0))
                                .whitespace_normal()
                                .child(format!("本轮共 {} 次工具调用", trace_step_count)),
                        )
                        .children(trace_preview.map(|preview| {
                            div()
                                .text_sm()
                                .text_color(rgb(0xc4b389))
                                .whitespace_normal()
                                .child(preview)
                        }));
                } else {
                    trace_content = trace_content.children(trace_lines.into_iter().map(|line| {
                        div()
                            .text_sm()
                            .text_color(rgb(0xe8d8b0))
                            .whitespace_normal()
                            .child(line)
                    }));
                }

                if is_collapsible {
                    let view = cx.entity().downgrade();
                    trace_content = trace_content.child(
                        Button::new(("toggle-trace-message", message_index))
                            .outline()
                            .warning()
                            .mt_1()
                            .h_7()
                            .px_3()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if is_collapsed {
                                "展开调用详情"
                            } else {
                                "收起调用详情"
                            })
                            .on_click(move |_, _, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.toggle_trace_message_collapse(message_index, cx)
                                });
                            }),
                    );
                }

                div()
                    .flex()
                    .min_w_0()
                    .items_start()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .child(
                        div()
                            .size_6()
                            .rounded_md()
                            .bg(rgb(0x1d1a08))
                            .border_1()
                            .border_color(rgb(0x8a6b1a))
                            .text_xs()
                            .text_color(rgb(0xf5c76f))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(ui_icon(IconName::Wrench, 14.0, 0xf5c76f)),
                    )
                    .child(trace_content)
            }
            ChatRole::Assistant => div()
                .flex()
                .min_w_0()
                .items_start()
                .gap_2()
                .px_4()
                .py_2()
                .child(
                    div()
                        .size_6()
                        .rounded_md()
                        .bg(rgb(0x042c2a))
                        .border_1()
                        .border_color(rgb(0x0f6b5f))
                        .text_xs()
                        .text_color(rgb(0x15d3be))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui_icon(IconName::Bot, 14.0, 0x15d3be)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x14b8a6))
                                .whitespace_normal()
                                .child(header.clone()),
                        )
                        .child(
                            div()
                                .text_base()
                                .text_color(rgb(0xdce5f6))
                                .whitespace_normal()
                                .child(message.body.clone()),
                        ),
                ),
            ChatRole::User | ChatRole::Client => div()
                .flex()
                .min_w_0()
                .items_start()
                .gap_2()
                .px_4()
                .py_2()
                .child(
                    div()
                        .size_6()
                        .rounded_md()
                        .bg(rgb(0x151b27))
                        .border_1()
                        .border_color(rgb(0x2a3446))
                        .text_xs()
                        .text_color(rgb(0x8a94a8))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui_icon(IconName::User, 14.0, 0x8a94a8)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x8b93a3))
                                .whitespace_normal()
                                .child(header.clone()),
                        )
                        .child(
                            div()
                                .text_base()
                                .text_color(rgb(0xc8d0de))
                                .whitespace_normal()
                                .child(message.body.clone()),
                        ),
                ),
            ChatRole::Error => div()
                .flex()
                .min_w_0()
                .items_start()
                .gap_2()
                .px_4()
                .py_2()
                .child(
                    div()
                        .size_6()
                        .rounded_md()
                        .bg(rgb(0x3b1116))
                        .border_1()
                        .border_color(rgb(0x8a1f2b))
                        .text_xs()
                        .text_color(rgb(0xfb7185))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(ui_icon(IconName::X, 14.0, 0xfb7185)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0xfda4af))
                                .whitespace_normal()
                                .child(header),
                        )
                        .child(
                            div()
                                .text_base()
                                .text_color(rgb(0xffd9df))
                                .whitespace_normal()
                                .child(message.body.clone()),
                        ),
                ),
        }
    }
}
