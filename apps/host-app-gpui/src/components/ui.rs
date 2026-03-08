use gpui::{
    deferred, div, prelude::*, px, rgb, rgba, Div, FontWeight, ScrollHandle, SharedString,
    Stateful, Svg,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiTone {
    pub border_hex: u32,
    pub background_hex: u32,
    pub foreground_hex: u32,
}

impl UiTone {
    pub const fn new(border_hex: u32, background_hex: u32, foreground_hex: u32) -> Self {
        Self {
            border_hex,
            background_hex,
            foreground_hex,
        }
    }
}

pub fn button_surface(tone: UiTone, interactive: bool) -> Div {
    let mut button = div()
        .rounded_md()
        .border_1()
        .border_color(rgb(tone.border_hex))
        .bg(rgb(tone.background_hex))
        .text_sm()
        .text_color(rgb(tone.foreground_hex));

    if interactive {
        button = button.cursor_pointer();
    }

    button
}

pub fn hero_action_button(
    icon: Svg,
    label: impl Into<SharedString>,
    tone: UiTone,
    interactive: bool,
) -> Div {
    button_surface(tone, interactive)
        .w_full()
        .h_10()
        .font_weight(FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .justify_center()
        .child(icon_label(icon, label))
}

pub fn line_action_button(
    icon: Svg,
    label: impl Into<SharedString>,
    tone: UiTone,
    interactive: bool,
) -> Div {
    button_surface(tone, interactive)
        .w_full()
        .h_11()
        .px_3()
        .flex()
        .items_center()
        .child(icon_label(icon, label))
}

pub fn icon_label(icon: Svg, label: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(icon)
        .child(label.into())
}

pub fn square_icon_button(icon: Svg, tone: UiTone, interactive: bool) -> Div {
    button_surface(tone, interactive)
        .size_11()
        .flex()
        .items_center()
        .justify_center()
        .child(icon)
}

pub fn floating_badge_button(
    icon: Svg,
    label: impl Into<SharedString>,
    tone: UiTone,
    interactive: bool,
) -> Div {
    button_surface(tone, interactive)
        .h_11()
        .px_4()
        .rounded_full()
        .font_weight(FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .size_5()
                .rounded_full()
                .border_1()
                .border_color(rgb(0x3b4e6d))
                .bg(rgb(0x1a2a42))
                .text_xs()
                .text_color(rgb(0x8fa8ca))
                .flex()
                .items_center()
                .justify_center()
                .child(icon),
        )
        .child(label.into())
}

pub fn text_input_shell(border_hex: u32, content: impl IntoElement) -> Div {
    div()
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
        .child(content)
}

pub fn message_list(scroll: &ScrollHandle) -> Stateful<Div> {
    div()
        .id("message-stream")
        .flex()
        .flex_col()
        .gap_1()
        .flex_1()
        .min_h_0()
        .track_scroll(scroll)
        .overflow_y_scroll()
        .scrollbar_width(px(10.0))
        .pr_2()
        .py_3()
        .bg(rgb(0x050912))
}

pub fn message_empty_state(text: impl Into<SharedString>) -> Div {
    div()
        .px_4()
        .py_3()
        .text_sm()
        .text_color(rgb(0x7b8798))
        .child(text.into())
}

pub fn dropdown_overlay_panel(content: impl IntoElement) -> impl IntoElement {
    deferred(
        div()
            .absolute()
            .top_full()
            .mt_1()
            .left_0()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x243145))
            .bg(rgb(0x0d1422))
            .occlude()
            .overflow_hidden()
            .child(content),
    )
    .with_priority(120)
}

pub fn level_meter(label: &str, level_percent: usize, active: bool) -> Div {
    let bars = 20usize;
    let active_bars = (level_percent.min(100) * bars) / 100;

    let mut row = div().flex().items_center().gap_0p5().h_4();
    for index in 0..bars {
        let bar_color = if !active || index >= active_bars {
            0x161f2e
        } else if index >= bars * 9 / 10 {
            0x8f2236
        } else if index >= bars * 7 / 10 {
            0xb7792a
        } else {
            0x12a596
        };

        row = row.child(div().w_1().h_full().rounded_sm().bg(rgb(bar_color)));
    }

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x65748b))
                .child(label.to_string()),
        )
        .child(row)
}

pub fn key_value_row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
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

pub fn modal_surface(width_px: f32, max_height_px: f32) -> Div {
    div()
        .w(px(width_px))
        .max_h(px(max_height_px))
        .flex()
        .flex_col()
        .rounded_xl()
        .border_1()
        .border_color(rgb(0x243045))
        .bg(rgb(0x0b1019))
        .overflow_hidden()
}

pub fn modal_overlay_root() -> Div {
    div().absolute().top_0().left_0().size_full().occlude()
}

pub fn modal_backdrop(backdrop_hex_with_alpha: u32) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(rgba(backdrop_hex_with_alpha))
}

pub fn modal_center_layer() -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
}
