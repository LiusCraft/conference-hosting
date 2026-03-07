use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use gpui::{
    canvas, div, prelude::*, px, rgb, rgba, size, Animation, AnimationExt as _, App, Application,
    Bounds, Context, ElementInputHandler, EntityInputHandler, FocusHandle, FontWeight,
    KeyDownEvent, ScrollHandle, SharedString, UTF16Selection, Window, WindowBounds, WindowOptions,
};
use host_core::{
    ClientTextMessage, GatewayStatus, HelloMessage, InboundTextMessage, AUDIO_FRAME_SAMPLES,
    AUDIO_SAMPLE_RATE_HZ,
};
use host_platform::{WsGatewayClient, WsGatewayConfig, WsGatewayEvent};
use opus::{
    Application as OpusApplication, Bitrate as OpusBitrate, Channels as OpusChannels,
    Decoder as OpusDecoder, Encoder as OpusEncoder,
};
use serde_json::{Map, Value};
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::sync::mpsc;

const WINDOW_WIDTH: f32 = 1200.0;
const WINDOW_HEIGHT: f32 = 700.0;
const MAX_CHAT_MESSAGES: usize = 240;
const OPUS_BITRATE_BPS: i32 = 16_000;
const OPUS_COMPLEXITY: i32 = 5;
const OPUS_MAX_PACKET_BYTES: usize = 4000;
const OPUS_DECODE_MAX_SAMPLES: usize = 4096;
const DOWNLINK_BUFFER_SECONDS: usize = 2;
const DEFAULT_WS_URL: &str = "ws://xrobo-io-k8.qbox.net/xiaozhi/v1?device-id=3b165ab4bb614dea85c7e880fa233803&client-id=resvpu932";
const DEFAULT_DEVICE_MAC: &str = "3b165ab4bb614dea85c7e880fa233803";
const DEFAULT_DEVICE_NAME: &str = "Host GPUI Desktop";
const DEFAULT_CLIENT_ID: &str = "resvpu932";
const DEFAULT_TOKEN: &str = "your-token1";
const HOST_INPUT_DEVICE_ENV: &str = "HOST_INPUT_DEVICE";
const HOST_OUTPUT_DEVICE_ENV: &str = "HOST_OUTPUT_DEVICE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Idle,
    Connecting,
    Connected,
    Disconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatRole {
    System,
    Client,
    User,
    Assistant,
    Tool,
    Error,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    role: ChatRole,
    title: SharedString,
    body: SharedString,
}

#[derive(Debug)]
enum GatewayCommand {
    Disconnect,
    DetectText(String),
    StartUplinkStream,
    StopUplinkStream,
}

#[derive(Debug)]
enum UiGatewayEvent {
    Connected { session_id: String },
    Disconnected,
    SystemNotice(String),
    Error(String),
    OutgoingText { kind: String, payload: String },
    IncomingText(InboundTextMessage),
    UplinkAudioFrameSent(usize),
    UplinkStreamStateChanged(bool),
    DownlinkAudioFrameReceived(usize),
}

#[derive(Debug, Clone)]
struct AudioRoutingConfig {
    input_device_name: Option<String>,
    input_from_output: bool,
    output_device_name: Option<String>,
}

impl AudioRoutingConfig {
    fn input_label(&self) -> String {
        let label = self.input_device_name.as_deref().unwrap_or("default");
        if self.input_from_output {
            format!("loopback:{label}")
        } else {
            label.to_string()
        }
    }

    fn output_label(&self) -> &str {
        self.output_device_name.as_deref().unwrap_or("default")
    }
}

struct MeetingHostShell {
    connection_state: ConnectionState,
    gateway_status: GatewayStatus,
    ws_url: SharedString,
    session_id: Option<SharedString>,
    uplink_audio_frames: usize,
    uplink_audio_bytes: usize,
    uplink_streaming: bool,
    downlink_audio_frames: usize,
    downlink_audio_bytes: usize,
    ws_command_tx: Option<mpsc::UnboundedSender<GatewayCommand>>,
    text_draft: String,
    text_input_selected_range: Range<usize>,
    text_input_selection_reversed: bool,
    text_input_marked_range: Option<Range<usize>>,
    text_input_focus: FocusHandle,
    chat_scroll: ScrollHandle,
    settings_scroll: ScrollHandle,
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    selected_input_index: Option<usize>,
    selected_input_output_index: Option<usize>,
    input_from_output: bool,
    selected_output_index: Option<usize>,
    show_input_dropdown: bool,
    show_output_dropdown: bool,
    show_settings_panel: bool,
    speaker_output_enabled: bool,
    chat_messages: Vec<ChatMessage>,
}

impl MeetingHostShell {
    fn connect_gateway(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.connection_state, ConnectionState::Idle) {
            return;
        }

        let config = build_gateway_config();
        let audio_routing = self.build_audio_routing_config();
        self.ws_url = config.server_url.clone().into();
        self.connection_state = ConnectionState::Connecting;
        self.session_id = None;
        self.uplink_streaming = false;
        self.push_chat(
            ChatRole::System,
            "System",
            format!("Connecting to {}", config.server_url),
        );
        self.push_chat(
            ChatRole::System,
            "System",
            format!(
                "Audio route selected -> input: {}, output: {}",
                audio_routing.input_label(),
                audio_routing.output_label()
            ),
        );

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        spawn_gateway_worker(config, audio_routing, command_rx, event_tx);

        self.ws_command_tx = Some(command_tx);

        cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.recv().await {
                if this
                    .update(cx, |view, cx| view.handle_gateway_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        cx.notify();
    }

    fn build_audio_routing_config(&self) -> AudioRoutingConfig {
        AudioRoutingConfig {
            input_device_name: self.selected_input_device_name_raw().map(ToOwned::to_owned),
            input_from_output: self.input_from_output,
            output_device_name: self.selected_output_device_name().map(ToOwned::to_owned),
        }
    }

    fn selected_input_device_name_raw(&self) -> Option<&str> {
        if self.input_from_output {
            self.selected_input_output_index
                .and_then(|index| self.output_devices.get(index))
                .map(String::as_str)
        } else {
            self.selected_input_index
                .and_then(|index| self.input_devices.get(index))
                .map(String::as_str)
        }
    }

    fn selected_input_device_label(&self) -> String {
        let label = self.selected_input_device_name_raw().unwrap_or("default");
        if self.input_from_output {
            format!("loopback:{label}")
        } else {
            label.to_string()
        }
    }

    fn selected_output_device_name(&self) -> Option<&str> {
        self.selected_output_index
            .and_then(|index| self.output_devices.get(index))
            .map(String::as_str)
    }

    fn toggle_input_dropdown(&mut self, cx: &mut Context<Self>) {
        self.show_input_dropdown = !self.show_input_dropdown;
        if self.show_input_dropdown {
            self.show_output_dropdown = false;
        }
        cx.notify();
    }

    fn toggle_output_dropdown(&mut self, cx: &mut Context<Self>) {
        self.show_output_dropdown = !self.show_output_dropdown;
        if self.show_output_dropdown {
            self.show_input_dropdown = false;
        }
        cx.notify();
    }

    fn open_settings_panel(&mut self, cx: &mut Context<Self>) {
        self.show_settings_panel = true;
        self.show_input_dropdown = false;
        self.show_output_dropdown = false;
        cx.notify();
    }

    fn close_settings_panel(&mut self, cx: &mut Context<Self>) {
        if !self.show_settings_panel {
            return;
        }
        self.show_settings_panel = false;
        cx.notify();
    }

    fn toggle_speaker_output(&mut self, cx: &mut Context<Self>) {
        self.speaker_output_enabled = !self.speaker_output_enabled;
        let state = if self.speaker_output_enabled {
            "Output monitoring enabled"
        } else {
            "Output monitoring paused"
        };
        self.push_chat(ChatRole::System, "System", state);
        cx.notify();
    }

    fn input_level_percent(&self) -> usize {
        if !matches!(self.connection_state, ConnectionState::Connected) || !self.uplink_streaming {
            return 0;
        }

        let pulse = self.uplink_audio_frames % 42;
        (32 + pulse).min(100)
    }

    fn output_level_percent(&self) -> usize {
        if !matches!(self.connection_state, ConnectionState::Connected)
            || !self.speaker_output_enabled
        {
            return 0;
        }

        let pulse = self.downlink_audio_frames % 46;
        (26 + pulse).min(100)
    }

    fn select_input_device_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.input_devices.get(index).is_none() {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Selected input device index is invalid",
            );
            cx.notify();
            return;
        }

        self.selected_input_index = Some(index);
        self.input_from_output = false;
        self.selected_input_output_index = None;
        self.show_input_dropdown = false;
        self.announce_audio_route_change("Input device selected", cx);
    }

    fn select_input_from_output_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.output_devices.get(index).is_none() {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Selected loopback output index is invalid",
            );
            cx.notify();
            return;
        }

        self.selected_input_output_index = Some(index);
        self.input_from_output = true;
        self.show_input_dropdown = false;
        self.announce_audio_route_change("Input source switched to output loopback", cx);
    }

    fn select_output_device_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.output_devices.get(index).is_none() {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Selected output device index is invalid",
            );
            cx.notify();
            return;
        }

        self.selected_output_index = Some(index);
        self.show_output_dropdown = false;
        self.announce_audio_route_change("Output device selected", cx);
    }

    fn announce_audio_route_change(&mut self, reason: &str, cx: &mut Context<Self>) {
        let input_name = self.selected_input_device_label();
        let output_name = self.selected_output_device_name().unwrap_or("default");
        let message = if matches!(self.connection_state, ConnectionState::Idle) {
            format!("{reason} -> input: {input_name}, output: {output_name}")
        } else {
            format!("{reason} -> input: {input_name}, output: {output_name} (reconnect to apply)")
        };

        self.push_chat(ChatRole::System, "System", message);
        cx.notify();
    }

    fn disconnect_gateway(&mut self, cx: &mut Context<Self>) {
        let Some(command_tx) = self.ws_command_tx.as_ref() else {
            return;
        };

        if command_tx.send(GatewayCommand::Disconnect).is_ok() {
            self.connection_state = ConnectionState::Disconnecting;
            self.push_chat(ChatRole::System, "System", "Disconnect requested");
        } else {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Gateway worker is unavailable, forcing idle state",
            );
            self.connection_state = ConnectionState::Idle;
            self.gateway_status = GatewayStatus::Idle;
            self.uplink_streaming = false;
            self.ws_command_tx = None;
            self.session_id = None;
        }

        cx.notify();
    }

    fn send_text_draft(&mut self, cx: &mut Context<Self>) {
        let text = self.text_draft.trim().to_string();
        if text.is_empty() {
            self.push_chat(
                ChatRole::System,
                "System",
                "Type a text message first (click the input box)",
            );
            cx.notify();
            return;
        }

        self.push_chat(ChatRole::User, "You", text.clone());
        self.send_gateway_command(GatewayCommand::DetectText(text), cx);
        self.text_draft.clear();
        self.text_input_selected_range = 0..0;
        self.text_input_selection_reversed = false;
        self.text_input_marked_range = None;
    }

    fn toggle_uplink_stream(&mut self, cx: &mut Context<Self>) {
        let command = if self.uplink_streaming {
            GatewayCommand::StopUplinkStream
        } else {
            GatewayCommand::StartUplinkStream
        };
        self.send_gateway_command(command, cx);
    }

    fn focus_text_input(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.focus(&self.text_input_focus);
    }

    fn blur_text_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_input_focus.is_focused(window) {
            window.blur();
            cx.notify();
        }
    }

    fn text_cursor_offset(&self) -> usize {
        if self.text_input_selection_reversed {
            self.text_input_selected_range.start
        } else {
            self.text_input_selected_range.end
        }
    }

    fn set_text_cursor(&mut self, offset: usize) {
        self.text_input_selected_range = offset..offset;
        self.text_input_selection_reversed = false;
    }

    fn text_draft_offset_from_utf16(&self, offset_utf16: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.text_draft.chars() {
            if utf16_count >= offset_utf16 {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn text_draft_offset_to_utf16(&self, offset_utf8: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.text_draft.chars() {
            if utf8_count >= offset_utf8 {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn text_draft_range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.text_draft_offset_to_utf16(range.start)..self.text_draft_offset_to_utf16(range.end)
    }

    fn text_draft_range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        let start = self.text_draft_offset_from_utf16(range_utf16.start);
        let end = self.text_draft_offset_from_utf16(range_utf16.end);
        if start <= end {
            start..end
        } else {
            end..start
        }
    }

    fn replace_text_draft_range(&mut self, range: Range<usize>, new_text: &str) {
        self.text_draft =
            (self.text_draft[0..range.start].to_owned() + new_text + &self.text_draft[range.end..])
                .into();

        let cursor = range.start + new_text.len();
        self.set_text_cursor(cursor);
        self.text_input_marked_range = None;
    }

    fn handle_text_input_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "enter" {
            if self.text_input_marked_range.is_some() {
                return;
            }
            self.send_text_draft(cx);
            return;
        }

        if key == "backspace" {
            if let Some(range) = self.text_input_marked_range.clone().or_else(|| {
                (!self.text_input_selected_range.is_empty())
                    .then_some(self.text_input_selected_range.clone())
            }) {
                self.replace_text_draft_range(range, "");
                cx.notify();
                return;
            }

            let cursor = self.text_cursor_offset();
            if cursor == 0 {
                return;
            }

            let previous_boundary = self.text_draft[0..cursor]
                .char_indices()
                .last()
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.replace_text_draft_range(previous_boundary..cursor, "");
            cx.notify();
            return;
        }

        if key == "escape" {
            self.text_draft.clear();
            self.set_text_cursor(0);
            self.text_input_marked_range = None;
            cx.notify();
        }
    }

    fn send_gateway_command(&mut self, command: GatewayCommand, cx: &mut Context<Self>) {
        if !matches!(self.connection_state, ConnectionState::Connected) {
            self.push_chat(ChatRole::System, "System", "Gateway is not connected yet");
            cx.notify();
            return;
        }

        let Some(command_tx) = self.ws_command_tx.as_ref() else {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Gateway worker channel not available",
            );
            self.connection_state = ConnectionState::Idle;
            self.gateway_status = GatewayStatus::Idle;
            cx.notify();
            return;
        };

        if command_tx.send(command).is_err() {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Failed to send command to gateway worker",
            );
            self.connection_state = ConnectionState::Idle;
            self.gateway_status = GatewayStatus::Idle;
            self.uplink_streaming = false;
            self.ws_command_tx = None;
            self.session_id = None;
        }

        cx.notify();
    }

    fn handle_gateway_event(&mut self, event: UiGatewayEvent, cx: &mut Context<Self>) {
        match event {
            UiGatewayEvent::Connected { session_id } => {
                self.connection_state = ConnectionState::Connected;
                self.gateway_status = GatewayStatus::Connected;
                self.session_id = Some(session_id.clone().into());
                self.push_chat(
                    ChatRole::System,
                    "System",
                    format!("Handshake finished, session_id={session_id}"),
                );
            }
            UiGatewayEvent::Disconnected => {
                self.connection_state = ConnectionState::Idle;
                self.gateway_status = GatewayStatus::Idle;
                self.uplink_streaming = false;
                self.ws_command_tx = None;
                self.session_id = None;
                self.push_chat(ChatRole::System, "System", "Gateway disconnected");
            }
            UiGatewayEvent::SystemNotice(message) => {
                self.push_chat(ChatRole::System, "System", message);
            }
            UiGatewayEvent::Error(message) => {
                self.push_chat(ChatRole::Error, "Error", message);
            }
            UiGatewayEvent::OutgoingText { kind, payload } => {
                self.push_chat(ChatRole::Client, format!("Client {kind}"), payload);
            }
            UiGatewayEvent::IncomingText(message) => {
                self.push_inbound_message(message);
            }
            UiGatewayEvent::UplinkAudioFrameSent(frame_bytes) => {
                self.uplink_audio_frames += 1;
                self.uplink_audio_bytes += frame_bytes;
            }
            UiGatewayEvent::UplinkStreamStateChanged(is_streaming) => {
                self.uplink_streaming = is_streaming;
                self.push_chat(
                    ChatRole::System,
                    "System",
                    if is_streaming {
                        "Microphone uplink Opus stream started"
                    } else {
                        "Microphone uplink Opus stream stopped"
                    },
                );
            }
            UiGatewayEvent::DownlinkAudioFrameReceived(frame_bytes) => {
                self.downlink_audio_frames += 1;
                self.downlink_audio_bytes += frame_bytes;
            }
        }

        cx.notify();
    }

    fn push_inbound_message(&mut self, message: InboundTextMessage) {
        let (role, title, body) = describe_inbound_message(&message);
        self.push_chat(role, title, body);
    }

    fn push_chat(
        &mut self,
        role: ChatRole,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
    ) {
        self.chat_messages.insert(
            0,
            ChatMessage {
                role,
                title: title.into(),
                body: body.into(),
            },
        );

        if self.chat_messages.len() > MAX_CHAT_MESSAGES {
            self.chat_messages.truncate(MAX_CHAT_MESSAGES);
        }
    }

    fn render_chat_message(&self, message: &ChatMessage) -> impl IntoElement {
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
                            .child(format!("{} | {}", message.title, message.body)),
                    ),
            ),
            ChatRole::Tool => div().flex().min_w_0().px_4().py_1().gap_2().child(
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
                            .child(message.title.clone()),
                    ),
            ),
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
                        .child("AI"),
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
                                .child(message.title.clone()),
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
                        .child("STT"),
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
                                .child(message.title.clone()),
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
                        .child("ERR"),
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
                                .child(message.title.clone()),
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

impl Render for MeetingHostShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_connected = matches!(self.connection_state, ConnectionState::Connected);
        let can_send_text = is_connected && !self.text_draft.trim().is_empty();
        let is_text_input_focused = self.text_input_focus.is_focused(window);
        let selected_input = self.selected_input_device_label();
        let selected_output = self
            .selected_output_device_name()
            .unwrap_or("default")
            .to_string();
        let connection_status = match self.connection_state {
            ConnectionState::Idle => ("DISCONNECTED", 0x202a3b, 0x7f8ba1, 0x506078),
            ConnectionState::Connecting => ("CONNECTING", 0x3a280a, 0xf4b544, 0xf4b544),
            ConnectionState::Connected => ("CONNECTED", 0x06332f, 0x16d9c0, 0x16d9c0),
            ConnectionState::Disconnecting => ("DISCONNECTING", 0x3a280a, 0xf4b544, 0xf4b544),
        };
        let text_input_border_color = if is_text_input_focused {
            rgb(0x16d9c0)
        } else if self.text_draft.is_empty() {
            rgb(0x283449)
        } else {
            rgb(0x145a58)
        };

        let connect_button = match self.connection_state {
            ConnectionState::Idle => div()
                .id("connect-button")
                .w_full()
                .h_10()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x17585d))
                .bg(rgb(0x0f3d40))
                .text_sm()
                .text_color(rgb(0x95f8ef))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .child("连接服务器")
                .on_click(cx.listener(|view, _event, _window, cx| view.connect_gateway(cx))),
            ConnectionState::Connected => div()
                .id("disconnect-button")
                .w_full()
                .h_10()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x7f2230))
                .bg(rgb(0x3a1219))
                .text_sm()
                .text_color(rgb(0xff99a6))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .child("断开连接")
                .on_click(cx.listener(|view, _event, _window, cx| view.disconnect_gateway(cx))),
            ConnectionState::Connecting => div()
                .id("connect-button")
                .w_full()
                .h_10()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x5c4720))
                .bg(rgb(0x2d2411))
                .text_sm()
                .text_color(rgb(0xf4d190))
                .font_weight(FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child("连接中..."),
            ConnectionState::Disconnecting => div()
                .id("connect-button")
                .w_full()
                .h_10()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x5c4720))
                .bg(rgb(0x2d2411))
                .text_sm()
                .text_color(rgb(0xf4d190))
                .font_weight(FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child("断开中..."),
        };

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
                    .text_sm()
                    .text_color(rgb(0xd2d9e7))
                    .text_ellipsis()
                    .child(selected_input.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x7f8ba1))
                    .child(if self.show_input_dropdown { "^" } else { "v" }),
            )
            .on_click(cx.listener(|view, _event, _window, cx| view.toggle_input_dropdown(cx)));

        let mut input_selector = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x5f6d84))
                    .child("输入源 (采集)"),
            )
            .child(input_selector_button);

        if self.show_input_dropdown {
            let mut input_dropdown = div()
                .flex()
                .flex_col()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x243145))
                .bg(rgb(0x0d1422))
                .overflow_hidden();

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

                    input_dropdown = input_dropdown.child(row.child(name.clone()));
                }
            }

            input_dropdown = input_dropdown.child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(0x7f8ba1))
                    .bg(rgb(0x0a101c))
                    .child("输出回采 (loopback)"),
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

                    input_dropdown = input_dropdown.child(row.child(format!("loopback: {name}")));
                }
            }

            input_selector = input_selector.child(input_dropdown);
        }

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
                    .text_sm()
                    .text_color(rgb(0xd2d9e7))
                    .text_ellipsis()
                    .child(selected_output.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x7f8ba1))
                    .child(if self.show_output_dropdown { "^" } else { "v" }),
            )
            .on_click(cx.listener(|view, _event, _window, cx| view.toggle_output_dropdown(cx)));

        let mut output_selector = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x5f6d84))
                    .child("输出源 (播放)"),
            )
            .child(output_selector_button);

        if self.show_output_dropdown {
            let mut output_dropdown = div()
                .flex()
                .flex_col()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x243145))
                .bg(rgb(0x0d1422))
                .overflow_hidden();

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

                    output_dropdown = output_dropdown.child(row.child(name.clone()));
                }
            }

            output_selector = output_selector.child(output_dropdown);
        }

        let mic_button = if is_connected {
            if self.uplink_streaming {
                div()
                    .id("mic-toggle")
                    .w_full()
                    .h_11()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x165e55))
                    .bg(rgb(0x0c3f3b))
                    .text_sm()
                    .text_color(rgb(0x6af3e2))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .child("采集中")
                    .on_click(
                        cx.listener(|view, _event, _window, cx| view.toggle_uplink_stream(cx)),
                    )
            } else {
                div()
                    .id("mic-toggle")
                    .w_full()
                    .h_11()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x2e384b))
                    .bg(rgb(0x131b2a))
                    .text_sm()
                    .text_color(rgb(0x8a96ab))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .child("采集已暂停")
                    .on_click(
                        cx.listener(|view, _event, _window, cx| view.toggle_uplink_stream(cx)),
                    )
            }
        } else {
            div()
                .id("mic-toggle")
                .w_full()
                .h_11()
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2e384b))
                .bg(rgb(0x131b2a))
                .text_sm()
                .text_color(rgb(0x8a96ab))
                .flex()
                .items_center()
                .child("采集中")
        };

        let speaker_button = if self.speaker_output_enabled {
            div()
                .id("speaker-toggle")
                .w_full()
                .h_11()
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x165e55))
                .bg(rgb(0x0c3f3b))
                .text_sm()
                .text_color(rgb(0x6af3e2))
                .cursor_pointer()
                .flex()
                .items_center()
                .child("播放中")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_speaker_output(cx)))
        } else {
            div()
                .id("speaker-toggle")
                .w_full()
                .h_11()
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2e384b))
                .bg(rgb(0x131b2a))
                .text_sm()
                .text_color(rgb(0x8a96ab))
                .cursor_pointer()
                .flex()
                .items_center()
                .child("播放已暂停")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_speaker_output(cx)))
        };

        let sidebar = div()
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
            .child(
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
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div().size_2().rounded_full().bg(rgb(connection_status.3)),
                                    )
                                    .child(
                                        div()
                                            .text_lg()
                                            .text_color(rgb(0xd5deee))
                                            .child("WebSocket"),
                                    ),
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
                                .child("RTT 48ms")
                                .child("50 fps"),
                        )
                    } else {
                        None
                    })
                    .child(connect_button),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .border_b_1()
                    .border_color(rgb(0x1a2232))
                    .child(div().text_sm().text_color(rgb(0x66758b)).child("音频设备"))
                    .child(input_selector)
                    .child(output_selector),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .border_b_1()
                    .border_color(rgb(0x1a2232))
                    .child(div().text_sm().text_color(rgb(0x66758b)).child("电平指示"))
                    .child(render_level_meter(
                        "INPUT",
                        self.input_level_percent(),
                        is_connected && self.uplink_streaming,
                    ))
                    .child(render_level_meter(
                        "OUTPUT",
                        self.output_level_percent(),
                        is_connected && self.speaker_output_enabled,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .border_b_1()
                    .border_color(rgb(0x1a2232))
                    .child(mic_button)
                    .child(speaker_button),
            )
            .child(
                div().mt_auto().p_4().child(
                    div()
                        .id("open-settings-button")
                        .text_sm()
                        .text_color(rgb(0x8a96ab))
                        .cursor_pointer()
                        .child("设置")
                        .on_click(
                            cx.listener(|view, _event, _window, cx| view.open_settings_panel(cx)),
                        ),
                ),
            );

        let chat_title = format!("{} 条消息", self.chat_messages.len());
        let chat_placeholder_text = if is_connected {
            "输入指令 (例: listen detect)"
        } else {
            "请先连接 WebSocket..."
        };
        let show_placeholder = self.text_draft.is_empty() && !is_text_input_focused;
        let chat_input_text = if show_placeholder {
            chat_placeholder_text.to_string()
        } else {
            self.text_draft.clone()
        };
        let text_input_focus_handle = self.text_input_focus.clone();
        let text_input_entity = cx.entity();

        let text_input_box =
            div()
                .id("text-draft-input")
                .track_focus(&self.text_input_focus)
                .on_click(cx.listener(|view, _event, window, cx| view.focus_text_input(window, cx)))
                .on_mouse_down_out(
                    cx.listener(|view, _event, window, cx| view.blur_text_input(window, cx)),
                )
                .on_key_down(cx.listener(|view, event, window, cx| {
                    view.handle_text_input_key(event, window, cx)
                }))
                .flex_1()
                .h_11()
                .px_3()
                .rounded_md()
                .border_1()
                .cursor_text()
                .border_color(text_input_border_color)
                .bg(rgb(0x090f1b))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .relative()
                        .flex()
                        .items_center()
                        .gap_0p5()
                        .child(
                            div()
                                .text_lg()
                                .text_color(if show_placeholder {
                                    rgb(0x57657d)
                                } else {
                                    rgb(0xd8dfef)
                                })
                                .text_ellipsis()
                                .child(chat_input_text),
                        )
                        .child(
                            div()
                                .id("text-input-caret")
                                .w(px(2.0))
                                .h(px(18.0))
                                .rounded_sm()
                                .bg(rgb(0x16d9c0))
                                .with_animation(
                                    "text-input-caret-blink",
                                    Animation::new(Duration::from_millis(980)).repeat(),
                                    move |this, delta| {
                                        if !is_text_input_focused {
                                            this.opacity(0.0)
                                        } else if delta < 0.52 {
                                            this.opacity(1.0)
                                        } else {
                                            this.opacity(0.0)
                                        }
                                    },
                                ),
                        )
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, cx| {
                                    window.handle_input(
                                        &text_input_focus_handle,
                                        ElementInputHandler::new(bounds, text_input_entity.clone()),
                                        cx,
                                    );
                                },
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        ),
                )
                .child(div().text_sm().text_color(rgb(0x4f5e76)).child("Enter"));

        let send_button = if can_send_text {
            div()
                .id("send-text-button")
                .size_11()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x115f58))
                .bg(rgb(0x0f5a54))
                .text_color(rgb(0x9af9ef))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .child("->")
                .on_click(cx.listener(|view, _event, _window, cx| view.send_text_draft(cx)))
        } else {
            div()
                .id("send-text-button")
                .size_11()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2a3448))
                .bg(rgb(0x111928))
                .text_color(rgb(0x556178))
                .flex()
                .items_center()
                .justify_center()
                .child("->")
        };

        let mut message_stream = div()
            .id("message-stream")
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .track_scroll(&self.chat_scroll)
            .overflow_y_scroll()
            .scrollbar_width(px(10.0))
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
            message_stream = message_stream.children(
                self.chat_messages
                    .iter()
                    .rev()
                    .map(|message| self.render_chat_message(message)),
            );
        }

        let chat_panel = div()
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(0x040913))
            .child(
                div()
                    .h_10()
                    .px_4()
                    .border_b_1()
                    .border_color(rgb(0x182132))
                    .bg(rgb(0x070f1b))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_lg().text_color(rgb(0xd7deec)).child("会话记录"))
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x111928))
                                    .text_xs()
                                    .text_color(rgb(0x6f7c91))
                                    .child(chat_title),
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
                    }),
            )
            .child(message_stream)
            .child(
                div()
                    .h_16()
                    .px_4()
                    .border_t_1()
                    .border_color(rgb(0x182132))
                    .bg(rgb(0x070f1b))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(text_input_box)
                    .child(send_button),
            );

        let status_bar = div()
            .h_8()
            .px_4()
            .bg(rgb(0x070d19))
            .border_t_1()
            .border_color(rgb(0x182132))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_sm()
                    .text_color(rgb(0x6f7b8f))
                    .child("CPU 2.3%")
                    .child("RAM 48 MB")
                    .children(if is_connected {
                        Some(
                            div()
                                .text_color(rgb(0x16d9c0))
                                .child("延迟 ~700ms (采集 20ms + 网络 50ms + ASR 200ms + LLM 300ms + TTS 130ms)"),
                        )
                    } else {
                        None
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x7d8798))
                    .child(wall_clock_label()),
            );

        let mut shell_body = div()
            .relative()
            .size_full()
            .bg(rgb(0x040811))
            .overflow_hidden()
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(chat_panel),
            )
            .child(status_bar);

        if self.show_settings_panel {
            let settings_panel = div()
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
                        .border_b_1()
                        .border_color(rgb(0x1a2435))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xe2e8f4))
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
                                .child("x")
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
                        .scrollbar_width(px(10.0))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xdce4f3))
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
                                        .child(setting_row("地址", self.ws_url.clone()))
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
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xdce4f3))
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
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xdce4f3))
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
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xdce4f3))
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
                                        .child(setting_row("输入设备", selected_input.clone()))
                                        .child(setting_row("输出设备", selected_output.clone()))
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
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xdce4f3))
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
                );

            shell_body = shell_body.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(
                        div()
                            .id("settings-backdrop")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .bg(rgba(0x020712d9))
                            .on_click(cx.listener(|view, _event, _window, cx| {
                                view.close_settings_panel(cx)
                            })),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_6()
                            .child(settings_panel),
                    ),
            );
        }

        div().size_full().bg(rgb(0x040811)).child(shell_body)
    }
}

impl EntityInputHandler for MeetingHostShell {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.text_draft_range_from_utf16(&range_utf16);
        adjusted_range.replace(self.text_draft_range_to_utf16(&range));
        Some(self.text_draft[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.text_draft_range_to_utf16(&self.text_input_selected_range),
            reversed: self.text_input_selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.text_input_marked_range
            .as_ref()
            .map(|range| self.text_draft_range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.text_input_marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.text_draft_range_from_utf16(range_utf16))
            .or(self.text_input_marked_range.clone())
            .unwrap_or_else(|| self.text_input_selected_range.clone());

        self.replace_text_draft_range(range, text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.text_draft_range_from_utf16(range_utf16))
            .or(self.text_input_marked_range.clone())
            .unwrap_or_else(|| self.text_input_selected_range.clone());

        self.text_draft =
            (self.text_draft[0..range.start].to_owned() + new_text + &self.text_draft[range.end..])
                .into();

        if !new_text.is_empty() {
            self.text_input_marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.text_input_marked_range = None;
        }

        self.text_input_selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.text_draft_range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| {
                let cursor = range.start + new_text.len();
                cursor..cursor
            });
        self.text_input_selection_reversed = false;

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<gpui::Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.text_draft_offset_to_utf16(self.text_cursor_offset()))
    }
}

fn render_level_meter(label: &str, level_percent: usize, active: bool) -> impl IntoElement {
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

fn wall_clock_label() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        % 86_400;
    let hour = elapsed / 3_600;
    let minute = (elapsed % 3_600) / 60;
    let second = elapsed % 60;
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn seed_mock_chat_messages() -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: ChatRole::Assistant,
            title: "AI 14:25:08 | 9.8s".into(),
            body: "建议把执行拆成三条线并行：内容生产、投放优化、转化闭环。每周做一次复盘，把预算从低 ROI 渠道挪到高 ROI 渠道。".into(),
        },
        ChatMessage {
            role: ChatRole::User,
            title: "STT 14:25:02".into(),
            body: "那我们每周复盘时，核心看哪些看板数据？".into(),
        },
        ChatMessage {
            role: ChatRole::Tool,
            title: "14:24:56".into(),
            body: "function_call: build_weekly_dashboard({\"metrics\": [\"CPL\", \"CVR\", \"Retention\", \"ROI\"]})".into(),
        },
        ChatMessage {
            role: ChatRole::Assistant,
            title: "AI 14:24:51 | 6.7s".into(),
            body: "可以先把线索成本、转化率、7 日留存和渠道 ROI 放到同一张周报中，方便一眼比较渠道效率。".into(),
        },
        ChatMessage {
            role: ChatRole::User,
            title: "STT 14:24:47".into(),
            body: "明白了，那执行上我们先从哪一块开始？".into(),
        },
        ChatMessage {
            role: ChatRole::User,
            title: "STT 14:24:30".into(),
            body: "好的，这些指标看起来可行。我们接下来讨论一下时间节点。".into(),
        },
        ChatMessage {
            role: ChatRole::Assistant,
            title: "AI 14:24:06 | 15.1s".into(),
            body: "核心 KPI 建议如下：第一，新增获客成本控制在 45 元以下；第二，短视频平台的自然流量增长目标 30%；第三，用户留存率从当前的 62% 提升到 70%；第四，ROI 整体目标不低于 3.5 倍。".into(),
        },
        ChatMessage {
            role: ChatRole::Tool,
            title: "14:24:03".into(),
            body: "function_call: get_kpi_template({\"domain\": \"digital_marketing\", \"quarter\": \"Q2\"})".into(),
        },
        ChatMessage {
            role: ChatRole::User,
            title: "STT 14:24:02".into(),
            body: "那关于 KPI 的设定呢？我们需要设定哪些关键指标？".into(),
        },
        ChatMessage {
            role: ChatRole::Assistant,
            title: "AI 14:23:38 | 12.5s".into(),
            body: "建议将总预算的 40% 分配给短视频平台，包括抖音和视频号。25% 用于搜索引擎优化，20% 用于邮件营销自动化升级，剩余 15% 作为 A/B 测试预算。".into(),
        },
        ChatMessage {
            role: ChatRole::User,
            title: "STT 14:23:35".into(),
            body: "这个建议不错，你能详细说一下具体的预算分配吗？".into(),
        },
        ChatMessage {
            role: ChatRole::Assistant,
            title: "AI 14:23:18 | 8.2s".into(),
            body: "好的，我来做一些补充。根据第一季度的数据，我们在社交媒体渠道的转化率提升了 23%，建议第二季度加大在短视频平台的投入。".into(),
        },
        ChatMessage {
            role: ChatRole::Tool,
            title: "14:23:16".into(),
            body: "function_call: analyze_context({\"topic\": \"Q2 marketing strategy\", \"participants\": 4})".into(),
        },
        ChatMessage {
            role: ChatRole::User,
            title: "STT 14:23:15".into(),
            body: "大家好，今天我们来讨论一下第二季度的营销策略。".into(),
        },
        ChatMessage {
            role: ChatRole::System,
            title: "System 14:23:02".into(),
            body: "开始音频采集: BlackHole 2ch (Loopback)".into(),
        },
        ChatMessage {
            role: ChatRole::System,
            title: "System 14:23:01".into(),
            body: "hello 握手成功 | PCM 16kHz Mono 16bit 20ms".into(),
        },
        ChatMessage {
            role: ChatRole::System,
            title: "System 14:23:01".into(),
            body: "WebSocket 连接已建立".into(),
        },
    ]
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT))),
                ..Default::default()
            },
            |_window, cx| {
                let audio_state = load_audio_device_state();
                cx.new(move |cx| MeetingHostShell {
                    connection_state: ConnectionState::Idle,
                    gateway_status: GatewayStatus::Idle,
                    ws_url: DEFAULT_WS_URL.into(),
                    session_id: None,
                    uplink_audio_frames: 0,
                    uplink_audio_bytes: 0,
                    uplink_streaming: false,
                    downlink_audio_frames: 0,
                    downlink_audio_bytes: 0,
                    ws_command_tx: None,
                    text_draft: String::new(),
                    text_input_selected_range: 0..0,
                    text_input_selection_reversed: false,
                    text_input_marked_range: None,
                    text_input_focus: cx.focus_handle(),
                    chat_scroll: ScrollHandle::new(),
                    settings_scroll: ScrollHandle::new(),
                    input_devices: audio_state.input_devices,
                    output_devices: audio_state.output_devices,
                    selected_input_index: audio_state.selected_input_index,
                    selected_input_output_index: audio_state.selected_input_output_index,
                    input_from_output: audio_state.input_from_output,
                    selected_output_index: audio_state.selected_output_index,
                    show_input_dropdown: false,
                    show_output_dropdown: false,
                    show_settings_panel: false,
                    speaker_output_enabled: true,
                    chat_messages: seed_mock_chat_messages(),
                })
            },
        )
        .expect("open GPUI window failed");

        cx.activate(true);
    });
}

fn build_gateway_config() -> WsGatewayConfig {
    let device_mac = env_or_default("HOST_DEVICE_MAC", DEFAULT_DEVICE_MAC);
    let device_id = env_or_default("HOST_DEVICE_ID", &device_mac);

    WsGatewayConfig::new(
        env_or_default("HOST_WS_URL", DEFAULT_WS_URL),
        device_id,
        env_or_default("HOST_DEVICE_NAME", DEFAULT_DEVICE_NAME),
        device_mac,
        env_or_default("HOST_CLIENT_ID", DEFAULT_CLIENT_ID),
        env_or_default("HOST_TOKEN", DEFAULT_TOKEN),
    )
}

fn env_or_default(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Debug)]
struct AudioDeviceState {
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    selected_input_index: Option<usize>,
    selected_input_output_index: Option<usize>,
    input_from_output: bool,
    selected_output_index: Option<usize>,
}

fn load_audio_device_state() -> AudioDeviceState {
    let host = cpal::default_host();

    let mut input_devices = list_input_device_names(&host);
    let mut output_devices = list_output_device_names(&host);
    input_devices.sort_unstable();
    output_devices.sort_unstable();

    let default_input_index = default_input_device_name(&host)
        .as_deref()
        .and_then(|name| find_device_index_by_hint(&input_devices, name));
    let default_output_index = default_output_device_name(&host)
        .as_deref()
        .and_then(|name| find_device_index_by_hint(&output_devices, name));

    let input_hint = env_optional(HOST_INPUT_DEVICE_ENV);
    let input_hint_ref = input_hint.as_deref();
    let env_input_index =
        input_hint_ref.and_then(|hint| find_device_index_by_hint(&input_devices, hint));
    let env_input_output_index = if env_input_index.is_none() {
        input_hint_ref.and_then(|hint| find_device_index_by_hint(&output_devices, hint))
    } else {
        None
    };
    let input_from_output = env_input_output_index.is_some();
    let selected_input_index = if input_from_output {
        None
    } else {
        env_input_index.or(default_input_index)
    };
    let selected_input_output_index = env_input_output_index;
    let selected_output_index = env_optional(HOST_OUTPUT_DEVICE_ENV)
        .as_deref()
        .and_then(|hint| find_device_index_by_hint(&output_devices, hint))
        .or(default_output_index);

    AudioDeviceState {
        input_devices,
        output_devices,
        selected_input_index,
        selected_input_output_index,
        input_from_output,
        selected_output_index,
    }
}

fn list_input_device_names(host: &cpal::Host) -> Vec<String> {
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn list_output_device_names(host: &cpal::Host) -> Vec<String> {
    match host.output_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn default_input_device_name(host: &cpal::Host) -> Option<String> {
    host.default_input_device()
        .and_then(|device| device.name().ok())
}

fn default_output_device_name(host: &cpal::Host) -> Option<String> {
    host.default_output_device()
        .and_then(|device| device.name().ok())
}

fn should_mirror_selected_output_to_system(output_hint: Option<&str>) -> bool {
    let Some(hint) = output_hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    let host = cpal::default_host();
    let selected_name = select_output_device(&host, Some(hint))
        .ok()
        .and_then(|device| device.name().ok());
    let default_name = default_output_device_name(&host);

    match (selected_name, default_name) {
        (Some(selected), Some(default_device)) => !selected.eq_ignore_ascii_case(&default_device),
        (Some(_), None) => true,
        _ => true,
    }
}

fn select_input_device(
    host: &cpal::Host,
    hint: Option<&str>,
    from_output_loopback: bool,
) -> Result<cpal::Device, String> {
    if !from_output_loopback {
        return select_input_device_by_hint(host, hint);
    }

    let Some(output_hint) = hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("Loopback input source requires a selected output device".to_string());
    };

    if let Some(device) = find_input_device_by_hint(host, output_hint)? {
        return Ok(device);
    }

    let output_device = select_output_device(host, Some(output_hint))?;
    if output_device.default_input_config().is_ok() {
        return Ok(output_device);
    }

    let available_inputs = list_input_device_names(host);
    Err(format!(
        "Cannot capture output `{output_hint}` as input directly. Use a loopback-capable output (e.g. BlackHole) and ensure matching input exists. Available input devices: {}",
        join_device_names(&available_inputs)
    ))
}

fn select_input_device_by_hint(
    host: &cpal::Host,
    hint: Option<&str>,
) -> Result<cpal::Device, String> {
    let Some(hint) = hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return host
            .default_input_device()
            .ok_or_else(|| "No default microphone input device found".to_string());
    };

    if let Some(device) = find_input_device_by_hint(host, hint)? {
        return Ok(device);
    }

    let available_inputs = list_input_device_names(host);
    Err(format!(
        "Input device `{hint}` not found. Available input devices: {}",
        join_device_names(&available_inputs)
    ))
}

fn find_input_device_by_hint(
    host: &cpal::Host,
    hint: &str,
) -> Result<Option<cpal::Device>, String> {
    let mut exact_match = None;
    let mut fuzzy_match = None;
    let hint_lower = hint.to_ascii_lowercase();

    for device in host
        .input_devices()
        .map_err(|error| format!("Cannot enumerate input devices: {error}"))?
    {
        let name = device
            .name()
            .unwrap_or_else(|_| "<unknown-input-device>".to_string());

        if name.eq_ignore_ascii_case(hint) {
            exact_match = Some(device);
            continue;
        }

        if fuzzy_match.is_none() && name.to_ascii_lowercase().contains(&hint_lower) {
            fuzzy_match = Some(device);
        }
    }

    Ok(exact_match.or(fuzzy_match))
}

fn select_output_device(host: &cpal::Host, hint: Option<&str>) -> Result<cpal::Device, String> {
    let Some(hint) = hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return host
            .default_output_device()
            .ok_or_else(|| "No default speaker output device found".to_string());
    };

    let mut exact_match = None;
    let mut fuzzy_match = None;
    let mut available = Vec::new();
    let hint_lower = hint.to_ascii_lowercase();

    for device in host
        .output_devices()
        .map_err(|error| format!("Cannot enumerate output devices: {error}"))?
    {
        let name = device
            .name()
            .unwrap_or_else(|_| "<unknown-output-device>".to_string());
        available.push(name.clone());

        if name.eq_ignore_ascii_case(hint) {
            exact_match = Some(device);
            continue;
        }

        if fuzzy_match.is_none() && name.to_ascii_lowercase().contains(&hint_lower) {
            fuzzy_match = Some(device);
        }
    }

    if let Some(device) = exact_match.or(fuzzy_match) {
        return Ok(device);
    }

    Err(format!(
        "Output device `{hint}` not found. Available output devices: {}",
        join_device_names(&available)
    ))
}

fn join_device_names(names: &[String]) -> String {
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    }
}

fn find_device_index_by_hint(devices: &[String], hint: &str) -> Option<usize> {
    if hint.is_empty() {
        return None;
    }

    if let Some(index) = devices
        .iter()
        .position(|name| name.eq_ignore_ascii_case(hint))
    {
        return Some(index);
    }

    let hint_lower = hint.to_ascii_lowercase();
    devices
        .iter()
        .position(|name| name.to_ascii_lowercase().contains(&hint_lower))
}

fn env_optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn spawn_gateway_worker(
    config: WsGatewayConfig,
    audio_routing: AudioRoutingConfig,
    mut command_rx: mpsc::UnboundedReceiver<GatewayCommand>,
    event_tx: mpsc::UnboundedSender<UiGatewayEvent>,
) {
    thread::spawn(move || {
        let runtime = match TokioRuntimeBuilder::new_multi_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to build tokio runtime: {error}"
                )));
                let _ = event_tx.send(UiGatewayEvent::Disconnected);
                return;
            }
        };

        runtime.block_on(async move {
            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                kind: "hello".to_string(),
                payload: to_pretty_json(&ClientTextMessage::hello(HelloMessage::new(
                    config.device_id.clone(),
                    config.device_name.clone(),
                    config.device_mac.clone(),
                    config.token.clone(),
                ))),
            });

            let mut client = match WsGatewayClient::connect(config).await {
                Ok(client) => client,
                Err(error) => {
                    let _ = event_tx.send(UiGatewayEvent::Error(format!(
                        "Gateway connection failed: {error}"
                    )));
                    let _ = event_tx.send(UiGatewayEvent::Disconnected);
                    return;
                }
            };

            let _ = event_tx.send(UiGatewayEvent::Connected {
                session_id: client.session_id().to_string(),
            });
            let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                "Audio route active -> input: {}, output: {}",
                audio_routing.input_label(),
                audio_routing.output_label()
            )));

            let mut uplink_streaming = false;
            let mut microphone_capture = None;
            let mut microphone_frame_rx = empty_audio_receiver();
            let mut downlink_player: Option<DownlinkAudioPlayer> = None;
            let mut downlink_playback_error_reported = false;
            let mut input_monitor_player: Option<DownlinkAudioPlayer> = None;
            let mut input_monitor_error_reported = false;
            let mut output_monitor_player: Option<DownlinkAudioPlayer> = None;
            let mut output_monitor_error_reported = false;
            let mirror_output_to_system =
                should_mirror_selected_output_to_system(audio_routing.output_device_name.as_deref());
            let mirror_input_to_system = true;

            loop {
                tokio::select! {
                    maybe_command = command_rx.recv() => {
                        let Some(command) = maybe_command else {
                            break;
                        };

                        let keep_running = handle_gateway_command(
                            command,
                            &mut client,
                            &event_tx,
                            &mut uplink_streaming,
                            &mut microphone_capture,
                            &mut microphone_frame_rx,
                            &audio_routing,
                        )
                        .await;
                        if !keep_running {
                            break;
                        }
                    }
                    maybe_frame = microphone_frame_rx.recv(), if uplink_streaming => {
                        let Some(frame) = maybe_frame else {
                            stop_uplink_capture(
                                &mut uplink_streaming,
                                &mut microphone_capture,
                                &mut microphone_frame_rx,
                                &event_tx,
                            );
                            let _ = event_tx.send(UiGatewayEvent::Error(
                                "Microphone capture stopped unexpectedly".to_string(),
                            ));
                            if let Err(error) = client.send_listen_stop().await {
                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                    "Failed to send listen stop after microphone ended: {error}"
                                )));
                                break;
                            }

                            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                                kind: "listen".to_string(),
                                payload: to_pretty_json(&ClientTextMessage::listen_stop()),
                            });
                            continue;
                        };

                        if mirror_input_to_system && !input_monitor_error_reported {
                            if input_monitor_player.is_none() {
                                match DownlinkAudioPlayer::new(event_tx.clone(), None) {
                                    Ok(player) => {
                                        let description = player.description().to_string();
                                        let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                                            "Input monitor ready (selected input -> system speaker): {description}"
                                        )));
                                        input_monitor_player = Some(player);
                                    }
                                    Err(error) => {
                                        input_monitor_error_reported = true;
                                        let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                            "Failed to init input monitor playback: {error}"
                                        )));
                                    }
                                }
                            }

                            if let Some(player) = input_monitor_player.as_mut() {
                                if let Err(error) = player.push_opus_packet(&frame) {
                                    if !input_monitor_error_reported {
                                        input_monitor_error_reported = true;
                                        let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                            "Failed to play input monitor packet: {error}"
                                        )));
                                    }
                                }
                            }
                        }

                        let frame_bytes = frame.len();
                        if let Err(error) = client.send_audio_frame(frame).await {
                            let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                "Failed to send streaming audio frame: {error}"
                            )));
                            break;
                        }

                        let _ = event_tx.send(UiGatewayEvent::UplinkAudioFrameSent(frame_bytes));
                    }
                    maybe_event = client.next_event() => {
                        let Some(event) = maybe_event else {
                            break;
                        };

                        match event {
                            WsGatewayEvent::Text(message) => {
                                let _ = event_tx.send(UiGatewayEvent::IncomingText(message));
                            }
                            WsGatewayEvent::DownlinkAudio(data) => {
                                if downlink_player.is_none() && !downlink_playback_error_reported {
                                    match DownlinkAudioPlayer::new(
                                        event_tx.clone(),
                                        audio_routing.output_device_name.as_deref(),
                                    ) {
                                        Ok(player) => {
                                            let description = player.description().to_string();
                                            let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                                                "Downlink playback ready: {description}"
                                            )));
                                            downlink_player = Some(player);
                                        }
                                        Err(error) => {
                                            downlink_playback_error_reported = true;
                                            let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                "Failed to init downlink playback: {error}"
                                            )));
                                        }
                                    }
                                }

                                if let Some(player) = downlink_player.as_mut() {
                                    if let Err(error) = player.push_opus_packet(&data) {
                                        if !downlink_playback_error_reported {
                                            downlink_playback_error_reported = true;
                                            let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                "Failed to decode/play downlink Opus packet: {error}"
                                            )));
                                        }
                                    }
                                }

                                if mirror_output_to_system && !output_monitor_error_reported {
                                    if output_monitor_player.is_none() {
                                        match DownlinkAudioPlayer::new(event_tx.clone(), None) {
                                            Ok(player) => {
                                                let description = player.description().to_string();
                                                let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                                                    "Output monitor ready (selected output -> system speaker): {description}"
                                                )));
                                                output_monitor_player = Some(player);
                                            }
                                            Err(error) => {
                                                output_monitor_error_reported = true;
                                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                    "Failed to init output monitor playback: {error}"
                                                )));
                                            }
                                        }
                                    }

                                    if let Some(player) = output_monitor_player.as_mut() {
                                        if let Err(error) = player.push_opus_packet(&data) {
                                            if !output_monitor_error_reported {
                                                output_monitor_error_reported = true;
                                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                    "Failed to play output monitor packet: {error}"
                                                )));
                                            }
                                        }
                                    }
                                }

                                let _ = event_tx
                                    .send(UiGatewayEvent::DownlinkAudioFrameReceived(data.len()));
                            }
                            WsGatewayEvent::MalformedText { raw, error } => {
                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                    "Malformed text frame: {error} | raw={raw}"
                                )));
                            }
                            WsGatewayEvent::Closed => {
                                break;
                            }
                            WsGatewayEvent::TransportError(error) => {
                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                    "Transport error: {error}"
                                )));
                                break;
                            }
                        }
                    }
                }
            }

            stop_uplink_capture(
                &mut uplink_streaming,
                &mut microphone_capture,
                &mut microphone_frame_rx,
                &event_tx,
            );

            if let Some(player) = downlink_player.take() {
                let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                    "Downlink playback closed: {}",
                    player.description()
                )));
            }

            if let Some(player) = input_monitor_player.take() {
                let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                    "Input monitor closed: {}",
                    player.description()
                )));
            }

            if let Some(player) = output_monitor_player.take() {
                let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                    "Output monitor closed: {}",
                    player.description()
                )));
            }

            let _ = event_tx.send(UiGatewayEvent::Disconnected);
        });
    });
}

async fn handle_gateway_command(
    command: GatewayCommand,
    client: &mut WsGatewayClient,
    event_tx: &mpsc::UnboundedSender<UiGatewayEvent>,
    uplink_streaming: &mut bool,
    microphone_capture: &mut Option<MicrophoneCapture>,
    microphone_frame_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    audio_routing: &AudioRoutingConfig,
) -> bool {
    match command {
        GatewayCommand::Disconnect => {
            stop_uplink_capture(
                uplink_streaming,
                microphone_capture,
                microphone_frame_rx,
                event_tx,
            );
            false
        }
        GatewayCommand::DetectText(text) => {
            if let Err(error) = client.send_listen_detect_text(text.clone()).await {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to send detect text: {error}"
                )));
                return false;
            }

            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                kind: "listen".to_string(),
                payload: to_pretty_json(&ClientTextMessage::listen_detect_text(text)),
            });
            true
        }
        GatewayCommand::StartUplinkStream => {
            if *uplink_streaming {
                return true;
            }

            if let Err(error) = client.send_listen_start().await {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to start uplink stream (listen start failed): {error}"
                )));
                return false;
            }

            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                kind: "listen".to_string(),
                payload: to_pretty_json(&ClientTextMessage::listen_start()),
            });

            let (capture, frame_rx) = match start_microphone_capture(
                event_tx.clone(),
                audio_routing.input_device_name.as_deref(),
                audio_routing.input_from_output,
            ) {
                Ok(capture_session) => capture_session,
                Err(error) => {
                    let _ = event_tx.send(UiGatewayEvent::Error(format!(
                        "Failed to start microphone capture: {error}"
                    )));
                    if let Err(stop_error) = client.send_listen_stop().await {
                        let _ = event_tx.send(UiGatewayEvent::Error(format!(
                            "Failed to rollback listen stop after microphone init failure: {stop_error}"
                        )));
                        return false;
                    }

                    let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                        kind: "listen".to_string(),
                        payload: to_pretty_json(&ClientTextMessage::listen_stop()),
                    });
                    return true;
                }
            };

            let capture_description = capture.description().to_string();
            *microphone_capture = Some(capture);
            *microphone_frame_rx = frame_rx;
            *uplink_streaming = true;
            let _ = event_tx.send(UiGatewayEvent::UplinkStreamStateChanged(true));
            let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                "Microphone capture ready: {capture_description}"
            )));
            true
        }
        GatewayCommand::StopUplinkStream => {
            if !*uplink_streaming {
                return true;
            }

            stop_uplink_capture(
                uplink_streaming,
                microphone_capture,
                microphone_frame_rx,
                event_tx,
            );

            if let Err(error) = client.send_listen_stop().await {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to stop uplink stream (listen stop failed): {error}"
                )));
                return false;
            }

            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                kind: "listen".to_string(),
                payload: to_pretty_json(&ClientTextMessage::listen_stop()),
            });
            true
        }
    }
}

struct MicrophoneCapture {
    _stream: Stream,
    description: String,
}

impl MicrophoneCapture {
    fn new(stream: Stream, description: String) -> Self {
        Self {
            _stream: stream,
            description,
        }
    }

    fn description(&self) -> &str {
        &self.description
    }
}

struct MicFrameBuilder {
    channels: usize,
    input_sample_rate_hz: u32,
    resample_accumulator: f64,
    frame_samples: Vec<i16>,
    encoder: OpusPacketEncoder,
    event_tx: mpsc::UnboundedSender<UiGatewayEvent>,
    encode_error_reported: bool,
}

impl MicFrameBuilder {
    fn new(
        channels: usize,
        input_sample_rate_hz: u32,
        event_tx: mpsc::UnboundedSender<UiGatewayEvent>,
    ) -> Result<Self, String> {
        Ok(Self {
            channels,
            input_sample_rate_hz,
            resample_accumulator: 0.0,
            frame_samples: Vec::with_capacity(AUDIO_FRAME_SAMPLES),
            encoder: OpusPacketEncoder::new()?,
            event_tx,
            encode_error_reported: false,
        })
    }

    fn process_f32(&mut self, data: &[f32], frame_tx: &mpsc::UnboundedSender<Vec<u8>>) {
        for frame in data.chunks(self.channels) {
            let mut mixed = 0.0_f32;
            for sample in frame {
                mixed += *sample;
            }
            mixed /= frame.len() as f32;
            self.push_sample(mixed, frame_tx);
        }
    }

    fn process_i16(&mut self, data: &[i16], frame_tx: &mpsc::UnboundedSender<Vec<u8>>) {
        for frame in data.chunks(self.channels) {
            let mut mixed = 0.0_f32;
            for sample in frame {
                mixed += *sample as f32 / i16::MAX as f32;
            }
            mixed /= frame.len() as f32;
            self.push_sample(mixed, frame_tx);
        }
    }

    fn process_u16(&mut self, data: &[u16], frame_tx: &mpsc::UnboundedSender<Vec<u8>>) {
        for frame in data.chunks(self.channels) {
            let mut mixed = 0.0_f32;
            for sample in frame {
                mixed += (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0;
            }
            mixed /= frame.len() as f32;
            self.push_sample(mixed, frame_tx);
        }
    }

    fn push_sample(&mut self, sample: f32, frame_tx: &mpsc::UnboundedSender<Vec<u8>>) {
        self.resample_accumulator += AUDIO_SAMPLE_RATE_HZ as f64;
        while self.resample_accumulator >= self.input_sample_rate_hz as f64 {
            self.resample_accumulator -= self.input_sample_rate_hz as f64;

            self.frame_samples.push(float_to_pcm16(sample));
            if self.frame_samples.len() < AUDIO_FRAME_SAMPLES {
                continue;
            }

            let packet = match self.encoder.encode_pcm16(&self.frame_samples) {
                Ok(packet) => packet,
                Err(error) => {
                    self.report_encode_error_once(error);
                    self.frame_samples.clear();
                    return;
                }
            };
            self.frame_samples.clear();

            if frame_tx.send(packet).is_err() {
                return;
            }
        }
    }

    fn report_encode_error_once(&mut self, error: String) {
        if self.encode_error_reported {
            return;
        }

        self.encode_error_reported = true;
        let _ = self.event_tx.send(UiGatewayEvent::Error(format!(
            "Opus encode error in microphone callback: {error}"
        )));
    }
}

struct OpusPacketEncoder {
    encoder: OpusEncoder,
    output_buffer: Vec<u8>,
}

impl OpusPacketEncoder {
    fn new() -> Result<Self, String> {
        let mut encoder = OpusEncoder::new(
            AUDIO_SAMPLE_RATE_HZ,
            OpusChannels::Mono,
            OpusApplication::Voip,
        )
        .map_err(|error| format!("create opus encoder failed: {error}"))?;

        encoder
            .set_bitrate(OpusBitrate::Bits(OPUS_BITRATE_BPS))
            .map_err(|error| format!("set opus bitrate failed: {error}"))?;
        encoder
            .set_complexity(OPUS_COMPLEXITY)
            .map_err(|error| format!("set opus complexity failed: {error}"))?;
        encoder
            .set_dtx(true)
            .map_err(|error| format!("enable opus DTX failed: {error}"))?;

        Ok(Self {
            encoder,
            output_buffer: vec![0_u8; OPUS_MAX_PACKET_BYTES],
        })
    }

    fn encode_pcm16(&mut self, samples: &[i16]) -> Result<Vec<u8>, String> {
        if samples.len() != AUDIO_FRAME_SAMPLES {
            return Err(format!(
                "invalid opus frame sample size: expected {}, got {}",
                AUDIO_FRAME_SAMPLES,
                samples.len()
            ));
        }

        let packet_len = self
            .encoder
            .encode(samples, &mut self.output_buffer)
            .map_err(|error| format!("opus encode failed: {error}"))?;

        Ok(self.output_buffer[..packet_len].to_vec())
    }
}

struct DownlinkAudioPlayer {
    _stream: Stream,
    description: String,
    output_sample_rate_hz: u32,
    output_channels: usize,
    output_buffer: Arc<Mutex<VecDeque<f32>>>,
    max_buffer_samples: usize,
    decoder: OpusDecoder,
    decode_buffer: Vec<i16>,
    resample_accumulator: f64,
}

impl DownlinkAudioPlayer {
    fn new(
        event_tx: mpsc::UnboundedSender<UiGatewayEvent>,
        output_device_hint: Option<&str>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = select_output_device(&host, output_device_hint)?;

        let device_name = device
            .name()
            .unwrap_or_else(|_| "Unknown speaker".to_string());
        let output_config = device
            .default_output_config()
            .map_err(|error| format!("Cannot read default speaker config: {error}"))?;

        let output_channels = usize::from(output_config.channels());
        if output_channels == 0 {
            return Err("Speaker channel count is zero".to_string());
        }

        let output_sample_rate_hz = output_config.sample_rate().0;
        let sample_format = output_config.sample_format();
        let stream_config: cpal::StreamConfig = output_config.config();

        let max_buffer_samples = usize::try_from(output_sample_rate_hz)
            .unwrap_or(48_000)
            .saturating_mul(output_channels)
            .saturating_mul(DOWNLINK_BUFFER_SECONDS);
        let output_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(max_buffer_samples)));

        let stream = build_speaker_output_stream(
            &device,
            &stream_config,
            sample_format,
            output_buffer.clone(),
            event_tx,
        )?;

        stream
            .play()
            .map_err(|error| format!("Failed to start speaker output stream: {error}"))?;

        let decoder = OpusDecoder::new(AUDIO_SAMPLE_RATE_HZ, OpusChannels::Mono)
            .map_err(|error| format!("create opus decoder failed: {error}"))?;

        let description = format!(
            "{} | {:?}, {}ch @ {}Hz <- {}Hz mono opus",
            device_name,
            sample_format,
            output_channels,
            output_sample_rate_hz,
            AUDIO_SAMPLE_RATE_HZ
        );

        Ok(Self {
            _stream: stream,
            description,
            output_sample_rate_hz,
            output_channels,
            output_buffer,
            max_buffer_samples,
            decoder,
            decode_buffer: vec![0_i16; OPUS_DECODE_MAX_SAMPLES],
            resample_accumulator: 0.0,
        })
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn push_opus_packet(&mut self, packet: &[u8]) -> Result<(), String> {
        let decoded_samples = self
            .decoder
            .decode(packet, &mut self.decode_buffer, false)
            .map_err(|error| format!("opus decode failed: {error}"))?;

        if decoded_samples == 0 {
            return Ok(());
        }

        let mut output_samples = Vec::with_capacity(decoded_samples * self.output_channels);
        for &sample in self.decode_buffer[..decoded_samples].iter() {
            let mono = pcm16_to_float(sample);
            self.resample_accumulator += self.output_sample_rate_hz as f64;

            while self.resample_accumulator >= AUDIO_SAMPLE_RATE_HZ as f64 {
                self.resample_accumulator -= AUDIO_SAMPLE_RATE_HZ as f64;
                for _ in 0..self.output_channels {
                    output_samples.push(mono);
                }
            }
        }

        if output_samples.is_empty() {
            return Ok(());
        }

        let mut buffer = self
            .output_buffer
            .lock()
            .map_err(|_| "speaker output buffer lock poisoned".to_string())?;

        let required_len = buffer.len().saturating_add(output_samples.len());
        if required_len > self.max_buffer_samples {
            let drop_count = required_len - self.max_buffer_samples;
            for _ in 0..drop_count {
                let _ = buffer.pop_front();
            }
        }

        buffer.extend(output_samples);
        Ok(())
    }
}

fn build_speaker_output_stream(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    output_buffer: Arc<Mutex<VecDeque<f32>>>,
    event_tx: mpsc::UnboundedSender<UiGatewayEvent>,
) -> Result<Stream, String> {
    let error_callback = move |error| {
        let _ = event_tx.send(UiGatewayEvent::Error(format!(
            "Speaker stream runtime error: {error}"
        )));
    };

    match sample_format {
        SampleFormat::F32 => {
            let output_buffer = output_buffer.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [f32], _| fill_output_buffer_f32(data, &output_buffer),
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let output_buffer = output_buffer.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], _| fill_output_buffer_i16(data, &output_buffer),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let output_buffer = output_buffer.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [u16], _| fill_output_buffer_u16(data, &output_buffer),
                error_callback,
                None,
            )
        }
        other => {
            return Err(format!(
                "Unsupported speaker sample format: {other:?}. Expect f32/i16/u16"
            ));
        }
    }
    .map_err(|error| format!("Failed to build speaker output stream: {error}"))
}

fn fill_output_buffer_f32(output: &mut [f32], output_buffer: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = 0.0;
        }
        return;
    };

    for sample in output.iter_mut() {
        *sample = buffer.pop_front().unwrap_or(0.0);
    }
}

fn fill_output_buffer_i16(output: &mut [i16], output_buffer: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = 0;
        }
        return;
    };

    for sample in output.iter_mut() {
        let value = buffer.pop_front().unwrap_or(0.0);
        *sample = float_to_pcm16(value);
    }
}

fn fill_output_buffer_u16(output: &mut [u16], output_buffer: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = u16::MAX / 2;
        }
        return;
    };

    for sample in output.iter_mut() {
        let value = buffer.pop_front().unwrap_or(0.0);
        *sample = float_to_u16(value);
    }
}

fn empty_audio_receiver() -> mpsc::UnboundedReceiver<Vec<u8>> {
    let (_tx, rx) = mpsc::unbounded_channel();
    rx
}

fn stop_uplink_capture(
    uplink_streaming: &mut bool,
    microphone_capture: &mut Option<MicrophoneCapture>,
    microphone_frame_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    event_tx: &mpsc::UnboundedSender<UiGatewayEvent>,
) {
    if *uplink_streaming {
        *uplink_streaming = false;
        let _ = event_tx.send(UiGatewayEvent::UplinkStreamStateChanged(false));
    }

    if let Some(capture) = microphone_capture.as_ref() {
        let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
            "Microphone capture closed: {}",
            capture.description()
        )));
    }

    *microphone_capture = None;
    *microphone_frame_rx = empty_audio_receiver();
}

fn start_microphone_capture(
    event_tx: mpsc::UnboundedSender<UiGatewayEvent>,
    input_device_hint: Option<&str>,
    input_from_output_loopback: bool,
) -> Result<(MicrophoneCapture, mpsc::UnboundedReceiver<Vec<u8>>), String> {
    let host = cpal::default_host();
    let device = select_input_device(&host, input_device_hint, input_from_output_loopback)?;

    let device_name = device
        .name()
        .unwrap_or_else(|_| "Unknown microphone".to_string());
    let input_config = device
        .default_input_config()
        .map_err(|error| format!("Cannot read default microphone config: {error}"))?;

    let channels = usize::from(input_config.channels());
    if channels == 0 {
        return Err("Microphone channel count is zero".to_string());
    }

    let sample_format = input_config.sample_format();
    let input_sample_rate_hz = input_config.sample_rate().0;
    let stream_config: cpal::StreamConfig = input_config.config();
    let (frame_tx, frame_rx) = mpsc::unbounded_channel();

    let error_tx = event_tx.clone();
    let error_callback = move |error| {
        let _ = error_tx.send(UiGatewayEvent::Error(format!(
            "Microphone stream runtime error: {error}"
        )));
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder =
                MicFrameBuilder::new(channels, input_sample_rate_hz, event_tx.clone())?;
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| frame_builder.process_f32(data, &frame_tx),
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder =
                MicFrameBuilder::new(channels, input_sample_rate_hz, event_tx.clone())?;
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| frame_builder.process_i16(data, &frame_tx),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder =
                MicFrameBuilder::new(channels, input_sample_rate_hz, event_tx.clone())?;
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| frame_builder.process_u16(data, &frame_tx),
                error_callback,
                None,
            )
        }
        other => {
            return Err(format!(
                "Unsupported microphone sample format: {other:?}. Expect f32/i16/u16"
            ));
        }
    }
    .map_err(|error| format!("Failed to build microphone input stream: {error}"))?;

    stream
        .play()
        .map_err(|error| format!("Failed to start microphone input stream: {error}"))?;

    let description = format!(
        "{} | {:?}, {}ch @ {}Hz -> {}Hz mono opus",
        device_name, sample_format, channels, input_sample_rate_hz, AUDIO_SAMPLE_RATE_HZ
    );

    Ok((MicrophoneCapture::new(stream, description), frame_rx))
}

fn float_to_pcm16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32).round() as i16
}

fn float_to_u16(sample: f32) -> u16 {
    let normalized = (sample.clamp(-1.0, 1.0) + 1.0) * 0.5;
    (normalized * u16::MAX as f32).round() as u16
}

fn pcm16_to_float(sample: i16) -> f32 {
    (sample as f32 / i16::MAX as f32).clamp(-1.0, 1.0)
}

fn describe_inbound_message(message: &InboundTextMessage) -> (ChatRole, String, String) {
    match message.message_type.as_str() {
        "hello" => {
            let session_id = read_string_field(&message.payload, "session_id").unwrap_or("-");
            (
                ChatRole::System,
                "Server hello".to_string(),
                format!("session_id={session_id}"),
            )
        }
        "stt" => (
            ChatRole::User,
            "STT".to_string(),
            read_string_field(&message.payload, "text")
                .unwrap_or("(empty stt text)")
                .to_string(),
        ),
        "llm" => (
            ChatRole::Assistant,
            "LLM".to_string(),
            read_string_field(&message.payload, "text")
                .unwrap_or("(empty llm text)")
                .to_string(),
        ),
        "tts" => {
            let state = read_string_field(&message.payload, "state").unwrap_or("unknown");
            let text = read_string_field(&message.payload, "text").unwrap_or("");
            let body = if text.is_empty() {
                format!("state={state}")
            } else {
                format!("state={state}, text={text}")
            };
            (ChatRole::Assistant, "TTS".to_string(), body)
        }
        "mcp" => (
            ChatRole::Tool,
            "Tool Call".to_string(),
            summarize_tool_message(&message.payload),
        ),
        "notify" => {
            let event_name = read_string_field(&message.payload, "event").unwrap_or("notify");
            if event_name == "intent_trace" {
                (
                    ChatRole::Tool,
                    "Intent Trace".to_string(),
                    summarize_intent_trace(&message.payload),
                )
            } else {
                (
                    ChatRole::System,
                    "Notify".to_string(),
                    compact_json(&Value::Object(message.payload.clone())),
                )
            }
        }
        _ => (
            ChatRole::System,
            format!("Server {}", message.message_type),
            compact_json(&Value::Object(message.payload.clone())),
        ),
    }
}

fn summarize_tool_message(payload: &Map<String, Value>) -> String {
    let Some(inner_payload) = payload.get("payload") else {
        return compact_json(&Value::Object(payload.clone()));
    };

    if let Some(inner) = inner_payload.as_object() {
        if let Some(method) = inner.get("method").and_then(Value::as_str) {
            return format!(
                "method={}, payload={}",
                method,
                compact_json(inner.get("params").unwrap_or(&Value::Null))
            );
        }

        if inner.get("result").is_some() || inner.get("error").is_some() {
            return compact_json(inner_payload);
        }
    }

    compact_json(inner_payload)
}

fn summarize_intent_trace(payload: &Map<String, Value>) -> String {
    let tool = read_string_field(payload, "tool").unwrap_or("-");
    let status = read_string_field(payload, "status").unwrap_or("-");
    let source = read_string_field(payload, "source").unwrap_or("-");

    let result_text = payload
        .get("result")
        .map(compact_json)
        .unwrap_or_else(|| "null".to_string());

    let error_text = payload
        .get("error")
        .map(compact_json)
        .unwrap_or_else(|| "null".to_string());

    format!(
        "tool={}, status={}, source={}, result={}, error={}",
        tool, status, source, result_text, error_text
    )
}

fn read_string_field<'a>(payload: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

fn to_pretty_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|error| {
        format!(
            "{{\"error\":\"json serialize failed\",\"detail\":\"{}\"}}",
            error
        )
    })
}
