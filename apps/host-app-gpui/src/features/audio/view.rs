use gpui::{div, prelude::*, rgb, ClickEvent, Context, Div, FontWeight, WindowControlArea};

use crate::app::shell::{ui_icon, MeetingHostShell};
use crate::app::state::ConnectionState;
use crate::components::icon::IconName;
use crate::components::ui::{
    dropdown_overlay_panel, hero_action_button, line_action_button, UiTone,
};

use super::{audio_device_icon, render_level_meter};

impl MeetingHostShell {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let is_connected = matches!(self.connection_state, ConnectionState::Connected);
        let selected_input = self.selected_input_device_label();
        let selected_output = self
            .selected_output_device_name()
            .unwrap_or("default")
            .to_string();

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
            .child(self.render_device_section(&selected_input, &selected_output, cx))
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
                                ui_icon(IconName::Wifi, 14.0, 0x16d9c0)
                            } else {
                                ui_icon(IconName::WifiOff, 14.0, 0x7f8ba1)
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
            .children(if is_connected {
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .text_xs()
                        .text_color(rgb(0x7f8ba1))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(ui_icon(IconName::Activity, 12.0, 0x16d9c0))
                                .child("RTT 48ms"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(ui_icon(IconName::Radio, 12.0, 0x16d9c0))
                                .child("50 fps"),
                        ),
                )
            } else {
                None
            })
            .child(self.render_connect_button(cx))
    }

    fn render_device_section(
        &self,
        selected_input: &str,
        selected_output: &str,
        cx: &mut Context<Self>,
    ) -> Div {
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
            .child(self.render_input_selector(selected_input, cx))
            .child(self.render_output_selector(selected_output, cx))
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
        div().mt_auto().p_4().child(
            div()
                .id("open-settings-button")
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(rgb(0x8a96ab))
                .cursor_pointer()
                .child(ui_icon(IconName::Settings, 14.0, 0x8a96ab))
                .child("设置")
                .on_click(cx.listener(|view, _event, _window, cx| view.open_settings_panel(cx))),
        )
    }

    fn render_connect_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.connection_state {
            ConnectionState::Idle => hero_action_button(
                ui_icon(IconName::Wifi, 14.0, 0x95f8ef),
                "连接服务器",
                UiTone::new(0x17585d, 0x0f3d40, 0x95f8ef),
                true,
            )
            .id("connect-button")
            .on_click(cx.listener(|view, _event, _window, cx| view.connect_gateway(cx))),
            ConnectionState::Connected => hero_action_button(
                ui_icon(IconName::WifiOff, 14.0, 0xff99a6),
                "断开连接",
                UiTone::new(0x7f2230, 0x3a1219, 0xff99a6),
                true,
            )
            .id("disconnect-button")
            .on_click(cx.listener(|view, _event, _window, cx| view.disconnect_gateway(cx))),
            ConnectionState::Connecting => hero_action_button(
                ui_icon(IconName::Activity, 14.0, 0xf4d190),
                "连接中...",
                UiTone::new(0x5c4720, 0x2d2411, 0xf4d190),
                false,
            )
            .id("connect-button"),
            ConnectionState::Disconnecting => hero_action_button(
                ui_icon(IconName::Activity, 14.0, 0xf4d190),
                "断开中...",
                UiTone::new(0x5c4720, 0x2d2411, 0xf4d190),
                false,
            )
            .id("connect-button"),
        }
    }

    fn render_input_selector(&self, selected_input: &str, cx: &mut Context<Self>) -> Div {
        let input_selector_button = div()
            .id("input-selector-button")
            .w_full()
            .h_10()
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x233043))
            .bg(rgb(0x0b121f))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .flex_1()
                    .child(ui_icon(audio_device_icon(selected_input), 13.0, 0x4fd7c5))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xd2d9e7))
                            .text_ellipsis()
                            .child(selected_input.to_string()),
                    ),
            )
            .child(div().child(ui_icon(IconName::ChevronDown, 12.0, 0x7f8ba1)))
            .on_click(cx.listener(|view, _event, _window, cx| {
                cx.stop_propagation();
                view.toggle_input_dropdown(cx)
            }));

        let input_selector_label = div()
            .flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(rgb(0x5f6d84))
            .child(ui_icon(IconName::Mic, 12.0, 0x5f6d84))
            .child("输入源 (采集)");

        let mut input_selector_field = div().relative().child(input_selector_button);

        if self.show_input_dropdown {
            let mut input_dropdown = div().flex().flex_col();

            if self.input_devices.is_empty() {
                input_dropdown = input_dropdown.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(rgb(0x7f8ba1))
                        .child("No input devices"),
                );
            } else {
                for (index, name) in self.input_devices.iter().enumerate() {
                    let selected =
                        !self.input_from_output && self.selected_input_index == Some(index);
                    let mut row = div()
                        .id(("input-device", index))
                        .px_3()
                        .py_2()
                        .text_sm()
                        .cursor_pointer()
                        .text_ellipsis()
                        .on_click(cx.listener(move |view, _event, _window, cx| {
                            view.select_input_device_index(index, cx)
                        }));

                    if selected {
                        row = row.bg(rgb(0x10343a)).text_color(rgb(0x6df3e2));
                    } else {
                        row = row.bg(rgb(0x0d1422)).text_color(rgb(0xcbd5e5));
                    }

                    input_dropdown = input_dropdown.child(
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .child(ui_icon(
                                    audio_device_icon(name),
                                    12.0,
                                    if selected { 0x6df3e2 } else { 0x8b97ac },
                                ))
                                .child(div().text_ellipsis().child(name.clone())),
                        ),
                    );
                }
            }

            input_dropdown = input_dropdown.child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(0x7f8ba1))
                    .bg(rgb(0x0a101c))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(ui_icon(IconName::Cable, 11.0, 0x7f8ba1))
                            .child("输出回采 (loopback)"),
                    ),
            );

            if self.output_devices.is_empty() {
                input_dropdown = input_dropdown.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(rgb(0x7f8ba1))
                        .child("No output devices"),
                );
            } else {
                for (index, name) in self.output_devices.iter().enumerate() {
                    let selected =
                        self.input_from_output && self.selected_input_output_index == Some(index);
                    let mut row = div()
                        .id(("loopback-device", index))
                        .px_3()
                        .py_2()
                        .text_sm()
                        .cursor_pointer()
                        .text_ellipsis()
                        .on_click(cx.listener(move |view, _event, _window, cx| {
                            view.select_input_from_output_index(index, cx)
                        }));

                    if selected {
                        row = row.bg(rgb(0x10343a)).text_color(rgb(0x6df3e2));
                    } else {
                        row = row.bg(rgb(0x0d1422)).text_color(rgb(0xcbd5e5));
                    }

                    input_dropdown = input_dropdown.child(
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .child(ui_icon(
                                    IconName::Cable,
                                    12.0,
                                    if selected { 0x6df3e2 } else { 0x8b97ac },
                                ))
                                .child(div().text_ellipsis().child(format!("loopback: {name}"))),
                        ),
                    );
                }
            }

            input_selector_field = input_selector_field.child(
                div()
                    .id("input-dropdown-overlay-hitbox")
                    .on_click(cx.listener(|_view, _event, _window, cx| cx.stop_propagation()))
                    .child(dropdown_overlay_panel(input_dropdown)),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(input_selector_label)
            .child(input_selector_field)
    }

    fn render_output_selector(&self, selected_output: &str, cx: &mut Context<Self>) -> Div {
        let output_selector_button = div()
            .id("output-selector-button")
            .w_full()
            .h_10()
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x233043))
            .bg(rgb(0x0b121f))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .flex_1()
                    .child(ui_icon(audio_device_icon(selected_output), 13.0, 0x4fd7c5))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xd2d9e7))
                            .text_ellipsis()
                            .child(selected_output.to_string()),
                    ),
            )
            .child(div().child(ui_icon(IconName::ChevronDown, 12.0, 0x7f8ba1)))
            .on_click(cx.listener(|view, _event, _window, cx| {
                cx.stop_propagation();
                view.toggle_output_dropdown(cx)
            }));

        let output_selector_label = div()
            .flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(rgb(0x5f6d84))
            .child(ui_icon(IconName::Volume2, 12.0, 0x5f6d84))
            .child("输出源 (播放)");

        let mut output_selector_field = div().relative().child(output_selector_button);

        if self.show_output_dropdown {
            let mut output_dropdown = div().flex().flex_col();

            if self.output_devices.is_empty() {
                output_dropdown = output_dropdown.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(rgb(0x7f8ba1))
                        .child("No output devices"),
                );
            } else {
                for (index, name) in self.output_devices.iter().enumerate() {
                    let selected = self.selected_output_index == Some(index);
                    let mut row = div()
                        .id(("output-device", index))
                        .px_3()
                        .py_2()
                        .text_sm()
                        .cursor_pointer()
                        .text_ellipsis()
                        .on_click(cx.listener(move |view, _event, _window, cx| {
                            view.select_output_device_index(index, cx)
                        }));

                    if selected {
                        row = row.bg(rgb(0x10343a)).text_color(rgb(0x6df3e2));
                    } else {
                        row = row.bg(rgb(0x0d1422)).text_color(rgb(0xcbd5e5));
                    }

                    output_dropdown = output_dropdown.child(
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .child(ui_icon(
                                    audio_device_icon(name),
                                    12.0,
                                    if selected { 0x6df3e2 } else { 0x8b97ac },
                                ))
                                .child(div().text_ellipsis().child(name.clone())),
                        ),
                    );
                }
            }

            output_selector_field = output_selector_field.child(
                div()
                    .id("output-dropdown-overlay-hitbox")
                    .on_click(cx.listener(|_view, _event, _window, cx| cx.stop_propagation()))
                    .child(dropdown_overlay_panel(output_dropdown)),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(output_selector_label)
            .child(output_selector_field)
    }

    fn render_mic_button(&self, is_connected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        if is_connected {
            if self.uplink_streaming {
                line_action_button(
                    ui_icon(IconName::Mic, 14.0, 0x6af3e2),
                    "采集中",
                    UiTone::new(0x165e55, 0x0c3f3b, 0x6af3e2),
                    true,
                )
                .id("mic-toggle")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_uplink_stream(cx)))
            } else {
                line_action_button(
                    ui_icon(IconName::MicOff, 14.0, 0x8a96ab),
                    "采集已暂停",
                    UiTone::new(0x2e384b, 0x131b2a, 0x8a96ab),
                    true,
                )
                .id("mic-toggle")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_uplink_stream(cx)))
            }
        } else {
            line_action_button(
                ui_icon(IconName::MicOff, 14.0, 0x8a96ab),
                "采集中",
                UiTone::new(0x2e384b, 0x131b2a, 0x8a96ab),
                false,
            )
            .id("mic-toggle")
        }
    }

    fn render_speaker_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.speaker_output_enabled {
            line_action_button(
                ui_icon(IconName::Volume2, 14.0, 0x6af3e2),
                "播放中",
                UiTone::new(0x165e55, 0x0c3f3b, 0x6af3e2),
                true,
            )
            .id("speaker-toggle")
            .on_click(cx.listener(|view, _event, _window, cx| view.toggle_speaker_output(cx)))
        } else {
            line_action_button(
                ui_icon(IconName::VolumeX, 14.0, 0x8a96ab),
                "播放已暂停",
                UiTone::new(0x2e384b, 0x131b2a, 0x8a96ab),
                true,
            )
            .id("speaker-toggle")
            .on_click(cx.listener(|view, _event, _window, cx| view.toggle_speaker_output(cx)))
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
