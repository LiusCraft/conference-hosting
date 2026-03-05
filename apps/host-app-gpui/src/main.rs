use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, FocusHandle, FontWeight,
    KeyDownEvent, SharedString, Window, WindowBounds, WindowOptions,
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

const WINDOW_WIDTH: f32 = 920.0;
const WINDOW_HEIGHT: f32 = 600.0;
const MAX_CHAT_MESSAGES: usize = 240;
const OPUS_BITRATE_BPS: i32 = 16_000;
const OPUS_COMPLEXITY: i32 = 5;
const OPUS_MAX_PACKET_BYTES: usize = 4000;
const OPUS_DECODE_MAX_SAMPLES: usize = 4096;
const DOWNLINK_BUFFER_SECONDS: usize = 2;
const DEFAULT_WS_URL: &str = "wss://xrobo-io.qiniuapi.com/v1/ws/";
const DEFAULT_DEVICE_MAC: &str = "unknown-device";
const DEFAULT_DEVICE_NAME: &str = "host-user";
const DEFAULT_CLIENT_ID: &str = "resvpu932";
const DEFAULT_TOKEN: &str = "your-token1";
const DEFAULT_TEXT_PROMPT: &str = "Hello, this is a test text message from GPUI host.";
const HOST_INPUT_DEVICE_ENV: &str = "HOST_INPUT_DEVICE";
const HOST_OUTPUT_DEVICE_ENV: &str = "HOST_OUTPUT_DEVICE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Idle,
    Connecting,
    Connected,
    Disconnecting,
}

impl ConnectionState {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Disconnecting => "Disconnecting",
        }
    }
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
    ListenStart,
    ListenStop,
    DetectText(String),
    SendSilenceFrame,
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
    title: SharedString,
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
    text_input_focus: FocusHandle,
    input_devices: Vec<String>,
    output_devices: Vec<String>,
    selected_input_index: Option<usize>,
    selected_input_output_index: Option<usize>,
    input_from_output: bool,
    selected_output_index: Option<usize>,
    default_input_index: Option<usize>,
    default_output_index: Option<usize>,
    show_input_dropdown: bool,
    show_output_dropdown: bool,
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

    fn cycle_input_device_prev(&mut self, cx: &mut Context<Self>) {
        self.cycle_input_device(false, cx);
    }

    fn cycle_input_device_next(&mut self, cx: &mut Context<Self>) {
        self.cycle_input_device(true, cx);
    }

    fn cycle_output_device_prev(&mut self, cx: &mut Context<Self>) {
        self.cycle_output_device(false, cx);
    }

    fn cycle_output_device_next(&mut self, cx: &mut Context<Self>) {
        self.cycle_output_device(true, cx);
    }

    fn use_default_output_device(&mut self, cx: &mut Context<Self>) {
        self.selected_output_index = self.default_output_index;
        self.announce_audio_route_change("Output device reset to default", cx);
    }

    fn use_default_input_device(&mut self, cx: &mut Context<Self>) {
        self.selected_input_index = self.default_input_index;
        self.input_from_output = false;
        self.selected_input_output_index = None;
        self.announce_audio_route_change("Input device reset to default", cx);
    }

    fn use_blackhole_output_device(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self
            .output_devices
            .iter()
            .position(|name| name.to_ascii_lowercase().contains("blackhole"))
        {
            self.selected_output_index = Some(index);
            self.announce_audio_route_change("Output device switched to BlackHole", cx);
            return;
        }

        self.push_chat(
            ChatRole::Error,
            "Error",
            "Cannot find output device containing `BlackHole`",
        );
        cx.notify();
    }

    fn cycle_input_device(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.input_from_output {
            if self.output_devices.is_empty() {
                self.push_chat(
                    ChatRole::Error,
                    "Error",
                    "No output device available for loopback input",
                );
                cx.notify();
                return;
            }

            self.selected_input_output_index = Some(cycle_index(
                self.selected_input_output_index,
                self.output_devices.len(),
                forward,
            ));
            self.announce_audio_route_change("Loopback input selection updated", cx);
            return;
        }

        if self.input_devices.is_empty() {
            self.push_chat(ChatRole::Error, "Error", "No input device available");
            cx.notify();
            return;
        }

        self.selected_input_index = Some(cycle_index(
            self.selected_input_index,
            self.input_devices.len(),
            forward,
        ));
        self.announce_audio_route_change("Input device selection updated", cx);
    }

    fn cycle_output_device(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.output_devices.is_empty() {
            self.push_chat(ChatRole::Error, "Error", "No output device available");
            cx.notify();
            return;
        }

        self.selected_output_index = Some(cycle_index(
            self.selected_output_index,
            self.output_devices.len(),
            forward,
        ));
        self.announce_audio_route_change("Output device selection updated", cx);
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

    fn send_listen_start(&mut self, cx: &mut Context<Self>) {
        self.send_gateway_command(GatewayCommand::ListenStart, cx);
    }

    fn send_listen_stop(&mut self, cx: &mut Context<Self>) {
        self.send_gateway_command(GatewayCommand::ListenStop, cx);
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
    }

    fn send_silence_frame(&mut self, cx: &mut Context<Self>) {
        self.send_gateway_command(GatewayCommand::SendSilenceFrame, cx);
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
            self.send_text_draft(cx);
            return;
        }

        if key == "backspace" {
            let _ = self.text_draft.pop();
            cx.notify();
            return;
        }

        if key == "escape" {
            self.text_draft.clear();
            cx.notify();
            return;
        }

        if key == "space" {
            self.text_draft.push(' ');
            cx.notify();
            return;
        }

        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            return;
        }

        if let Some(ch) = event
            .keystroke
            .key_char
            .as_deref()
            .and_then(single_visible_char)
        {
            self.text_draft.push(ch);
            cx.notify();
            return;
        }

        if let Some(ch) = single_visible_char(event.keystroke.key.as_str()) {
            self.text_draft.push(ch);
            cx.notify();
        }
    }

    fn clear_chat(&mut self, cx: &mut Context<Self>) {
        self.chat_messages.clear();
        self.push_chat(
            ChatRole::System,
            "System",
            "Chat history cleared (audio counters preserved)",
        );
        cx.notify();
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

    fn connect_button_label(&self) -> &'static str {
        match self.connection_state {
            ConnectionState::Idle => "Connect",
            ConnectionState::Connecting => "Connecting...",
            ConnectionState::Connected => "Disconnect",
            ConnectionState::Disconnecting => "Disconnecting...",
        }
    }

    fn status_color(&self) -> u32 {
        match self.connection_state {
            ConnectionState::Idle => 0x94a3b8,
            ConnectionState::Connecting | ConnectionState::Disconnecting => 0xf59e0b,
            ConnectionState::Connected => 0x34d399,
        }
    }

    fn stream_button_label(&self) -> &'static str {
        if self.uplink_streaming {
            "Stop Uplink Stream"
        } else {
            "Start Uplink Stream"
        }
    }

    fn render_chat_message(&self, message: &ChatMessage) -> impl IntoElement {
        let is_outgoing = matches!(message.role, ChatRole::Client);
        let row = if is_outgoing {
            div().flex().justify_end()
        } else {
            div().flex().justify_start()
        };

        let (bubble_bg, border_color, title_color, body_color) = match message.role {
            ChatRole::System => (0x111827, 0x334155, 0x94a3b8, 0xe2e8f0),
            ChatRole::Client => (0x1d4ed8, 0x60a5fa, 0xbfdbfe, 0xf8fafc),
            ChatRole::User => (0x0f766e, 0x2dd4bf, 0x99f6e4, 0xf0fdfa),
            ChatRole::Assistant => (0x312e81, 0x818cf8, 0xc7d2fe, 0xf5f3ff),
            ChatRole::Tool => (0x4a044e, 0xc084fc, 0xe9d5ff, 0xfdf4ff),
            ChatRole::Error => (0x7f1d1d, 0xf87171, 0xfecaca, 0xfff1f2),
        };

        row.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .bg(rgb(bubble_bg))
                .border_color(rgb(border_color))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(title_color))
                        .child(message.title.clone()),
                )
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .text_color(rgb(body_color))
                        .child(message.body.clone()),
                ),
        )
    }
}

impl Render for MeetingHostShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connect_button = match self.connection_state {
            ConnectionState::Idle => div()
                .id("connect-button")
                .px_4()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x7dd3fc))
                .bg(rgb(0x0369a1))
                .child(self.connect_button_label())
                .on_click(cx.listener(|view, _event, _window, cx| view.connect_gateway(cx))),
            ConnectionState::Connected => div()
                .id("connect-button")
                .px_4()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0xfca5a5))
                .bg(rgb(0x7f1d1d))
                .child(self.connect_button_label())
                .on_click(cx.listener(|view, _event, _window, cx| view.disconnect_gateway(cx))),
            ConnectionState::Connecting | ConnectionState::Disconnecting => div()
                .id("connect-button")
                .px_4()
                .py_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0xf59e0b))
                .bg(rgb(0x78350f))
                .child(self.connect_button_label()),
        };

        let can_control = matches!(self.connection_state, ConnectionState::Connected);
        let listen_start_button = if can_control {
            div()
                .id("listen-start-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x6ee7b7))
                .bg(rgb(0x065f46))
                .child("Listen Start")
                .on_click(cx.listener(|view, _event, _window, cx| view.send_listen_start(cx)))
        } else {
            div()
                .id("listen-start-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0x64748b))
                .bg(rgb(0x1e293b))
                .child("Listen Start")
        };

        let listen_stop_button = if can_control {
            div()
                .id("listen-stop-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0xfcd34d))
                .bg(rgb(0x92400e))
                .child("Listen Stop")
                .on_click(cx.listener(|view, _event, _window, cx| view.send_listen_stop(cx)))
        } else {
            div()
                .id("listen-stop-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0x64748b))
                .bg(rgb(0x1e293b))
                .child("Listen Stop")
        };

        let send_text_button = if can_control {
            div()
                .id("send-text-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0xc4b5fd))
                .bg(rgb(0x4338ca))
                .child("Send Text")
                .on_click(cx.listener(|view, _event, _window, cx| view.send_text_draft(cx)))
        } else {
            div()
                .id("send-text-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0x64748b))
                .bg(rgb(0x1e293b))
                .child("Send Text")
        };

        let uplink_stream_button = if can_control {
            if self.uplink_streaming {
                div()
                    .id("uplink-stream-button")
                    .px_3()
                    .py_2()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .border_color(rgb(0xfca5a5))
                    .bg(rgb(0x7f1d1d))
                    .child(self.stream_button_label())
                    .on_click(
                        cx.listener(|view, _event, _window, cx| view.toggle_uplink_stream(cx)),
                    )
            } else {
                div()
                    .id("uplink-stream-button")
                    .px_3()
                    .py_2()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .border_color(rgb(0xfcd34d))
                    .bg(rgb(0x854d0e))
                    .child(self.stream_button_label())
                    .on_click(
                        cx.listener(|view, _event, _window, cx| view.toggle_uplink_stream(cx)),
                    )
            }
        } else {
            div()
                .id("uplink-stream-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0x64748b))
                .bg(rgb(0x1e293b))
                .child(self.stream_button_label())
        };

        let send_silence_button = if can_control {
            div()
                .id("send-silence-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x93c5fd))
                .bg(rgb(0x1e3a8a))
                .child("Send 1 Silent Opus Packet")
                .on_click(cx.listener(|view, _event, _window, cx| view.send_silence_frame(cx)))
        } else {
            div()
                .id("send-silence-button")
                .px_3()
                .py_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0x64748b))
                .bg(rgb(0x1e293b))
                .child("Send 1 Silent Opus Packet")
        };

        let clear_chat_button = div()
            .id("clear-chat-button")
            .px_3()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0x94a3b8))
            .bg(rgb(0x334155))
            .child("Clear")
            .on_click(cx.listener(|view, _event, _window, cx| view.clear_chat(cx)));

        let input_prev_button = div()
            .id("input-prev-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0x7dd3fc))
            .bg(rgb(0x0c4a6e))
            .child("In <")
            .on_click(cx.listener(|view, _event, _window, cx| view.cycle_input_device_prev(cx)));

        let input_next_button = div()
            .id("input-next-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0x7dd3fc))
            .bg(rgb(0x0c4a6e))
            .child("In >")
            .on_click(cx.listener(|view, _event, _window, cx| view.cycle_input_device_next(cx)));

        let output_prev_button = div()
            .id("output-prev-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0xa7f3d0))
            .bg(rgb(0x064e3b))
            .child("Out <")
            .on_click(cx.listener(|view, _event, _window, cx| view.cycle_output_device_prev(cx)));

        let output_next_button = div()
            .id("output-next-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0xa7f3d0))
            .bg(rgb(0x064e3b))
            .child("Out >")
            .on_click(cx.listener(|view, _event, _window, cx| view.cycle_output_device_next(cx)));

        let blackhole_button = div()
            .id("blackhole-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0xfde68a))
            .bg(rgb(0x78350f))
            .child("Use BlackHole")
            .on_click(
                cx.listener(|view, _event, _window, cx| view.use_blackhole_output_device(cx)),
            );

        let default_input_button = div()
            .id("default-input-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0xcbd5e1))
            .bg(rgb(0x334155))
            .child("Use Default In")
            .on_click(cx.listener(|view, _event, _window, cx| view.use_default_input_device(cx)));

        let default_output_button = div()
            .id("default-output-button")
            .px_2()
            .py_2()
            .border_1()
            .rounded_md()
            .cursor_pointer()
            .border_color(rgb(0xcbd5e1))
            .bg(rgb(0x334155))
            .child("Use Default Out")
            .on_click(cx.listener(|view, _event, _window, cx| view.use_default_output_device(cx)));

        let input_dropdown_button = if self.show_input_dropdown {
            div()
                .id("input-dropdown-button")
                .px_2()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x93c5fd))
                .bg(rgb(0x1d4ed8))
                .child("Input List ▲")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_input_dropdown(cx)))
        } else {
            div()
                .id("input-dropdown-button")
                .px_2()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x93c5fd))
                .bg(rgb(0x0f172a))
                .child("Input List ▼")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_input_dropdown(cx)))
        };

        let output_dropdown_button = if self.show_output_dropdown {
            div()
                .id("output-dropdown-button")
                .px_2()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x86efac))
                .bg(rgb(0x166534))
                .child("Output List ▲")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_output_dropdown(cx)))
        } else {
            div()
                .id("output-dropdown-button")
                .px_2()
                .py_2()
                .border_1()
                .rounded_md()
                .cursor_pointer()
                .border_color(rgb(0x86efac))
                .bg(rgb(0x0f172a))
                .child("Output List ▼")
                .on_click(cx.listener(|view, _event, _window, cx| view.toggle_output_dropdown(cx)))
        };

        let mut input_dropdown_panel = div()
            .id("input-dropdown-panel")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .border_1()
            .rounded_md()
            .border_color(rgb(0x0ea5e9))
            .bg(rgb(0x0b2239))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xbae6fd))
                    .child("Input Devices"),
            );

        if self.input_devices.is_empty() {
            input_dropdown_panel = input_dropdown_panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x94a3b8))
                    .child("No input devices"),
            );
        } else {
            for (index, name) in self.input_devices.iter().enumerate() {
                let selected = !self.input_from_output && self.selected_input_index == Some(index);
                let mut row = div()
                    .id(("input-device", index))
                    .px_2()
                    .py_1()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _event, _window, cx| {
                        view.select_input_device_index(index, cx)
                    }));

                if selected {
                    row = row
                        .border_color(rgb(0x7dd3fc))
                        .bg(rgb(0x1d4ed8))
                        .text_color(rgb(0xeff6ff));
                } else {
                    row = row
                        .border_color(rgb(0x334155))
                        .bg(rgb(0x0f172a))
                        .text_color(rgb(0xcbd5e1));
                }

                input_dropdown_panel = input_dropdown_panel
                    .child(row.child(div().text_xs().whitespace_normal().child(name.clone())));
            }
        }

        input_dropdown_panel = input_dropdown_panel.child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0xa7f3d0))
                .child("Output Loopback Sources"),
        );

        if self.output_devices.is_empty() {
            input_dropdown_panel = input_dropdown_panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x94a3b8))
                    .child("No output devices for loopback"),
            );
        } else {
            for (index, name) in self.output_devices.iter().enumerate() {
                let selected =
                    self.input_from_output && self.selected_input_output_index == Some(index);
                let mut row = div()
                    .id(("input-loopback-device", index))
                    .px_2()
                    .py_1()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _event, _window, cx| {
                        view.select_input_from_output_index(index, cx)
                    }));

                if selected {
                    row = row
                        .border_color(rgb(0x86efac))
                        .bg(rgb(0x166534))
                        .text_color(rgb(0xf0fdf4));
                } else {
                    row = row
                        .border_color(rgb(0x334155))
                        .bg(rgb(0x0f172a))
                        .text_color(rgb(0xcbd5e1));
                }

                input_dropdown_panel = input_dropdown_panel.child(
                    row.child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .child(format!("loopback: {name}")),
                    ),
                );
            }
        }

        let mut output_dropdown_panel = div()
            .id("output-dropdown-panel")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .border_1()
            .rounded_md()
            .border_color(rgb(0x22c55e))
            .bg(rgb(0x0b2a1a))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xbbf7d0))
                    .child("Output Devices"),
            );

        if self.output_devices.is_empty() {
            output_dropdown_panel = output_dropdown_panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x94a3b8))
                    .child("No output devices"),
            );
        } else {
            for (index, name) in self.output_devices.iter().enumerate() {
                let selected = self.selected_output_index == Some(index);
                let mut row = div()
                    .id(("output-device", index))
                    .px_2()
                    .py_1()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _event, _window, cx| {
                        view.select_output_device_index(index, cx)
                    }));

                if selected {
                    row = row
                        .border_color(rgb(0x86efac))
                        .bg(rgb(0x166534))
                        .text_color(rgb(0xf0fdf4));
                } else {
                    row = row
                        .border_color(rgb(0x334155))
                        .bg(rgb(0x0f172a))
                        .text_color(rgb(0xcbd5e1));
                }

                output_dropdown_panel = output_dropdown_panel
                    .child(row.child(div().text_xs().whitespace_normal().child(name.clone())));
            }
        }

        let mut device_dropdowns = div().flex().flex_wrap().gap_2();
        if self.show_input_dropdown {
            device_dropdowns = device_dropdowns.child(input_dropdown_panel);
        }
        if self.show_output_dropdown {
            device_dropdowns = device_dropdowns.child(output_dropdown_panel);
        }

        let selected_input = self.selected_input_device_label();
        let selected_output = self.selected_output_device_name().unwrap_or("default");
        let route_apply_hint = if matches!(self.connection_state, ConnectionState::Idle) {
            ""
        } else {
            " (reconnect to apply)"
        };

        let draft_text = if self.text_draft.is_empty() {
            "Type message here, press Enter to send...".to_string()
        } else {
            self.text_draft.clone()
        };

        let text_input_box =
            div()
                .id("text-draft-input")
                .track_focus(&self.text_input_focus)
                .on_click(cx.listener(|view, _event, window, cx| view.focus_text_input(window, cx)))
                .on_key_down(cx.listener(|view, event, window, cx| {
                    view.handle_text_input_key(event, window, cx)
                }))
                .px_3()
                .py_2()
                .flex_1()
                .border_1()
                .rounded_md()
                .cursor_text()
                .border_color(if self.text_draft.is_empty() {
                    rgb(0x475569)
                } else {
                    rgb(0x38bdf8)
                })
                .bg(rgb(0x0b1120))
                .child(
                    div()
                        .text_sm()
                        .whitespace_normal()
                        .text_color(if self.text_draft.is_empty() {
                            rgb(0x64748b)
                        } else {
                            rgb(0xe2e8f0)
                        })
                        .child(draft_text),
                );

        let mut chat_panel = div()
            .id("chat-panel")
            .flex_1()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_3()
            .overflow_y_scroll()
            .border_1()
            .rounded_md()
            .border_color(rgb(0x334155))
            .bg(rgb(0x0f172a));

        if self.chat_messages.is_empty() {
            chat_panel = chat_panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x94a3b8))
                    .child("No text messages yet. Binary audio is counted but hidden from chat."),
            );
        } else {
            chat_panel = chat_panel.children(
                self.chat_messages
                    .iter()
                    .map(|message| self.render_chat_message(message)),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .size_full()
            .px_4()
            .py_4()
            .bg(rgb(0x020617))
            .text_color(rgb(0xe2e8f0))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.title.clone()),
                            )
                            .child(div().text_sm().text_color(rgb(self.status_color())).child(
                                format!(
                                    "Connection: {} | Gateway: {}",
                                    self.connection_state.label(),
                                    self.gateway_status.as_label()
                                ),
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94a3b8))
                                    .child(format!("URL: {}", self.ws_url.as_ref())),
                            )
                            .child(div().text_xs().text_color(rgb(0x94a3b8)).child(format!(
                                    "Session: {}",
                                    self.session_id
                                        .as_ref()
                                        .map(SharedString::as_ref)
                                        .unwrap_or("-")
                                )))
                            .child(
                                div().text_xs().text_color(rgb(0x93c5fd)).child(format!(
                                    "Audio route: in=[{}], out=[{}]{}",
                                    selected_input, selected_output, route_apply_hint
                                )),
                            ),
                    )
                    .child(connect_button),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(text_input_box)
                    .child(input_prev_button)
                    .child(input_next_button)
                    .child(output_prev_button)
                    .child(output_next_button)
                    .child(blackhole_button)
                    .child(default_input_button)
                    .child(default_output_button)
                    .child(input_dropdown_button)
                    .child(output_dropdown_button)
                    .child(listen_start_button)
                    .child(listen_stop_button)
                    .child(send_text_button)
                    .child(uplink_stream_button)
                    .child(send_silence_button)
                    .child(clear_chat_button),
            )
            .child(device_dropdowns)
            .child(div().text_sm().text_color(rgb(0x93c5fd)).child(format!(
                "Audio counters -> uplink frames: {}, uplink bytes: {}, downlink frames: {}, downlink bytes: {}",
                self.uplink_audio_frames, self.uplink_audio_bytes, self.downlink_audio_frames, self.downlink_audio_bytes
            )))
            .child(chat_panel)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| {
                let audio_state = load_audio_device_state();
                let input_device_count = audio_state.input_devices.len();
                let output_device_count = audio_state.output_devices.len();
                cx.new(move |cx| MeetingHostShell {
                    title: "AI Meeting Host Shell".into(),
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
                    text_draft: DEFAULT_TEXT_PROMPT.to_string(),
                    text_input_focus: cx.focus_handle(),
                    input_devices: audio_state.input_devices,
                    output_devices: audio_state.output_devices,
                    selected_input_index: audio_state.selected_input_index,
                    selected_input_output_index: audio_state.selected_input_output_index,
                    input_from_output: audio_state.input_from_output,
                    selected_output_index: audio_state.selected_output_index,
                    default_input_index: audio_state.default_input_index,
                    default_output_index: audio_state.default_output_index,
                    show_input_dropdown: false,
                    show_output_dropdown: false,
                    chat_messages: vec![ChatMessage {
                        role: ChatRole::System,
                        title: "System".into(),
                        body: format!(
                            "Ready. Configure env vars (HOST_WS_URL/HOST_TOKEN/...) then click Connect. Input devices: {}, output devices: {}",
                            input_device_count,
                            output_device_count
                        )
                        .into(),
                    }],
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
    default_input_index: Option<usize>,
    default_output_index: Option<usize>,
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
        default_input_index,
        default_output_index,
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

fn cycle_index(current: Option<usize>, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }

    match (current, forward) {
        (Some(index), true) => (index + 1) % len,
        (Some(0), false) => len - 1,
        (Some(index), false) => index - 1,
        (None, true) => 0,
        (None, false) => len - 1,
    }
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
        GatewayCommand::ListenStart => {
            if let Err(error) = client.send_listen_start().await {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to send listen start: {error}"
                )));
                return false;
            }

            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                kind: "listen".to_string(),
                payload: to_pretty_json(&ClientTextMessage::listen_start()),
            });
            true
        }
        GatewayCommand::ListenStop => {
            stop_uplink_capture(
                uplink_streaming,
                microphone_capture,
                microphone_frame_rx,
                event_tx,
            );

            if let Err(error) = client.send_listen_stop().await {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to send listen stop: {error}"
                )));
                return false;
            }

            let _ = event_tx.send(UiGatewayEvent::OutgoingText {
                kind: "listen".to_string(),
                payload: to_pretty_json(&ClientTextMessage::listen_stop()),
            });
            true
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
        GatewayCommand::SendSilenceFrame => {
            let packet = match encode_single_silent_opus_frame() {
                Ok(packet) => packet,
                Err(error) => {
                    let _ = event_tx.send(UiGatewayEvent::Error(format!(
                        "Failed to encode silent Opus frame: {error}"
                    )));
                    return true;
                }
            };

            let packet_bytes = packet.len();
            if let Err(error) = client.send_audio_frame(packet).await {
                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                    "Failed to send binary audio frame: {error}"
                )));
                return false;
            }

            let _ = event_tx.send(UiGatewayEvent::UplinkAudioFrameSent(packet_bytes));
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

fn encode_single_silent_opus_frame() -> Result<Vec<u8>, String> {
    let mut encoder = OpusPacketEncoder::new()?;
    encoder.encode_pcm16(&vec![0_i16; AUDIO_FRAME_SAMPLES])
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

fn single_visible_char(input: &str) -> Option<char> {
    let mut chars = input.chars();
    let ch = chars.next()?;
    if chars.next().is_some() || ch.is_control() {
        return None;
    }
    Some(ch)
}

fn to_pretty_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|error| {
        format!(
            "{{\"error\":\"json serialize failed\",\"detail\":\"{}\"}}",
            error
        )
    })
}
