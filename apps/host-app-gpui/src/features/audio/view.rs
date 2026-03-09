use gpui::{div, prelude::*, rgb, ClickEvent, Context, Div, FontWeight, WindowControlArea};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::select::Select;
use gpui_component::Disableable;

use crate::app::shell::{ui_button_icon, ui_icon, ButtonIconTone, MeetingHostShell};
use crate::app::state::ConnectionState;
use crate::components::icon::IconName;

use super::render_level_meter;

impl MeetingHostShell {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let is_connected = matches!(self.connection_state, ConnectionState::Connected);

        div()
            .w_72()
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(rgb(0x050a14))
            .border_r_1()
            .border_color(rgb(0x1a2232))
            .overflow_hidden()
            .child(self.render_connection_section(is_connected, cx))
            .child(self.render_device_section())
            .child(self.render_meter_section(is_connected))
            .child(self.render_transport_section(is_connected, cx))
            .child(self.render_settings_entry(cx))
    }

    fn render_connection_section(&self, is_connected: bool, cx: &mut Context<Self>) -> Div {
        let connection_status = self.connection_status_badge();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(rgb(0x1a2232))
            .child(
                div()
                    .id("sidebar-drag-strip")
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
                            .child(div().size_2().rounded_full().bg(rgb(connection_status.3)))
                            .child(if is_connected {
                                ui_button_icon(IconName::Wifi, 14.0, ButtonIconTone::Success)
                            } else {
                                ui_button_icon(IconName::WifiOff, 14.0, ButtonIconTone::Neutral)
                            })
                            .child(div().text_lg().text_color(rgb(0xd5deee)).child("WebSocket")),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(connection_status.1))
                            .text_xs()
                            .text_color(rgb(connection_status.2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(connection_status.0),
                    ),
            )
            .child(self.render_connect_button(cx))
    }

    fn render_device_section(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .border_b_1()
            .border_color(rgb(0x1a2232))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(rgb(0x66758b))
                    .child(ui_icon(IconName::Headphones, 13.0, 0x66758b))
                    .child("音频设备"),
            )
            .child(self.render_input_selector())
            .child(self.render_output_selector())
    }

    fn render_meter_section(&self, is_connected: bool) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(rgb(0x1a2232))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(rgb(0x66758b))
                    .child(ui_icon(IconName::AudioLines, 13.0, 0x66758b))
                    .child("电平指示"),
            )
            .child(render_level_meter(
                "INPUT",
                self.input_level_percent(),
                is_connected && self.uplink_streaming,
            ))
            .child(render_level_meter(
                "OUTPUT",
                self.output_level_percent(),
                is_connected && self.speaker_output_enabled,
            ))
    }

    fn render_transport_section(&self, is_connected: bool, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .border_b_1()
            .border_color(rgb(0x1a2232))
            .child(self.render_mic_button(is_connected, cx))
            .child(self.render_speaker_button(cx))
    }

    fn render_settings_entry(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.entity().downgrade();

        div().mt_auto().p_4().child(
            Button::new("open-settings-button")
                .ghost()
                .w_full()
                .h_9()
                .justify_start()
                .text_color(rgb(0x8a96ab))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(ui_button_icon(
                            IconName::Settings,
                            14.0,
                            ButtonIconTone::Ghost,
                        ))
                        .child("设置"),
                )
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.open_settings_panel(window, cx));
                }),
        )
    }

    fn render_connect_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();

        match self.connection_state {
            ConnectionState::Idle => Button::new("connect-button")
                .primary()
                .w_full()
                .h_10()
                .font_weight(FontWeight::SEMIBOLD)
                .child(action_button_content(
                    ui_button_icon(IconName::Wifi, 14.0, ButtonIconTone::Primary),
                    "连接服务器",
                ))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.connect_gateway(cx));
                }),
            ConnectionState::Connected => {
                let view = view.clone();
                Button::new("disconnect-button")
                    .danger()
                    .w_full()
                    .h_10()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(action_button_content(
                        ui_button_icon(IconName::WifiOff, 14.0, ButtonIconTone::Danger),
                        "断开连接",
                    ))
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |view, cx| view.disconnect_gateway(cx));
                    })
            }
            ConnectionState::Connecting => Button::new("connect-button")
                .warning()
                .w_full()
                .h_10()
                .font_weight(FontWeight::SEMIBOLD)
                .disabled(true)
                .child(action_button_content(
                    ui_button_icon(IconName::Activity, 14.0, ButtonIconTone::Warning),
                    "连接中...",
                )),
            ConnectionState::Disconnecting => Button::new("connect-button")
                .warning()
                .w_full()
                .h_10()
                .font_weight(FontWeight::SEMIBOLD)
                .disabled(true)
                .child(action_button_content(
                    ui_button_icon(IconName::Activity, 14.0, ButtonIconTone::Warning),
                    "断开中...",
                )),
        }
    }

    fn render_input_selector(&self) -> Div {
        let has_options = !self.input_devices.is_empty() || !self.output_devices.is_empty();
        let input_selector_label = div()
            .flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(rgb(0x5f6d84))
            .child(ui_icon(IconName::Mic, 12.0, 0x5f6d84))
            .child("输入源 (采集)");

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(input_selector_label)
            .child(
                div().id("input-selector-field").w_full().h_10().child(
                    Select::new(&self.input_select_state)
                        .placeholder("选择输入源")
                        .disabled(!has_options)
                        .empty(
                            div()
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(0x7f8ba1))
                                .child("No input or loopback devices"),
                        ),
                ),
            )
    }

    fn render_output_selector(&self) -> Div {
        let has_options = !self.output_devices.is_empty();
        let output_selector_label = div()
            .flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(rgb(0x5f6d84))
            .child(ui_icon(IconName::Volume2, 12.0, 0x5f6d84))
            .child("输出源 (播放)");

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(output_selector_label)
            .child(
                div().id("output-selector-field").w_full().h_10().child(
                    Select::new(&self.output_select_state)
                        .placeholder("选择输出源")
                        .disabled(!has_options)
                        .empty(
                            div()
                                .px_3()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(0x7f8ba1))
                                .child("No output devices"),
                        ),
                ),
            )
    }

    fn render_mic_button(&self, is_connected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();

        if is_connected {
            if self.uplink_streaming {
                Button::new("mic-toggle")
                    .success()
                    .w_full()
                    .h_11()
                    .px_3()
                    .justify_start()
                    .child(action_button_content(
                        ui_button_icon(IconName::Mic, 14.0, ButtonIconTone::Success),
                        "采集中",
                    ))
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |view, cx| view.toggle_uplink_stream(cx));
                    })
            } else {
                let view = view.clone();
                Button::new("mic-toggle")
                    .outline()
                    .w_full()
                    .h_11()
                    .px_3()
                    .justify_start()
                    .child(action_button_content(
                        ui_button_icon(IconName::MicOff, 14.0, ButtonIconTone::Neutral),
                        "采集已暂停",
                    ))
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |view, cx| view.toggle_uplink_stream(cx));
                    })
            }
        } else {
            Button::new("mic-toggle")
                .outline()
                .disabled(true)
                .w_full()
                .h_11()
                .px_3()
                .justify_start()
                .child(action_button_content(
                    ui_button_icon(IconName::MicOff, 14.0, ButtonIconTone::Disabled),
                    "采集已暂停",
                ))
        }
    }

    fn render_speaker_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();

        if self.speaker_output_enabled {
            Button::new("speaker-toggle")
                .success()
                .w_full()
                .h_11()
                .px_3()
                .justify_start()
                .child(action_button_content(
                    ui_button_icon(IconName::Volume2, 14.0, ButtonIconTone::Success),
                    "播放中",
                ))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.toggle_speaker_output(cx));
                })
        } else {
            let view = view.clone();
            Button::new("speaker-toggle")
                .outline()
                .w_full()
                .h_11()
                .px_3()
                .justify_start()
                .child(action_button_content(
                    ui_button_icon(IconName::VolumeX, 14.0, ButtonIconTone::Neutral),
                    "播放已暂停",
                ))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.toggle_speaker_output(cx));
                })
        }
    }

    fn connection_status_badge(&self) -> (&'static str, u32, u32, u32) {
        match self.connection_state {
            ConnectionState::Idle => ("DISCONNECTED", 0x202a3b, 0x7f8ba1, 0x506078),
            ConnectionState::Connecting => ("CONNECTING", 0x3a280a, 0xf4b544, 0xf4b544),
            ConnectionState::Connected => ("CONNECTED", 0x06332f, 0x16d9c0, 0x16d9c0),
            ConnectionState::Disconnecting => ("DISCONNECTING", 0x3a280a, 0xf4b544, 0xf4b544),
        }
    }
}

fn action_button_content(icon: gpui::Svg, label: &'static str) -> Div {
    div().flex().items_center().gap_2().child(icon).child(label)
}
