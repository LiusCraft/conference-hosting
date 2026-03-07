use std::collections::VecDeque;
use std::ops::Range;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod assets;
mod components;

use assets::AppAssets;
use components::{
    icon::{icon, IconName},
    ui::{
        dropdown_overlay_panel, floating_badge_button, hero_action_button, key_value_row,
        level_meter, line_action_button, message_empty_state, message_list, modal_backdrop,
        modal_center_layer, modal_overlay_root, modal_surface, square_icon_button,
        text_input_caret, text_input_content_row, text_input_shell, text_input_value, UiTone,
    },
};
use gateway_runtime::{
    load_audio_device_state, spawn_gateway_worker, COMMAND_CHANNEL_CAPACITY, EVENT_CHANNEL_CAPACITY,
};
use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, App, Application, Bounds, ClickEvent, Context,
    ElementInputHandler, EntityInputHandler, FocusHandle, FontWeight, KeyDownEvent, ScrollHandle,
    SharedString, TitlebarOptions, UTF16Selection, Window, WindowBounds, WindowControlArea,
    WindowOptions,
};
use host_core::{GatewayStatus, InboundTextMessage, AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE_HZ};
use host_platform::WsGatewayConfig;
use serde_json::{Map, Value};
use tokio::sync::mpsc;

mod gateway_runtime;

const WINDOW_WIDTH: f32 = 1200.0;
const WINDOW_HEIGHT: f32 = 760.0;
const APP_TITLE: &str = "AI Meeting Host v0.1.0-alpha";
const MAX_CHAT_MESSAGES: usize = 240;
const DEFAULT_WS_URL: &str = "wss://xrobo-io.qiniuapi.com/v1/ws/";
const DEFAULT_DEVICE_MAC: &str = "unknown-device";
const DEFAULT_DEVICE_NAME: &str = "host-user";
const DEFAULT_CLIENT_ID: &str = "resvpu932";
const DEFAULT_TOKEN: &str = "your-token1";
const CHAT_BOTTOM_EPSILON_PX: f32 = 2.0;
const TTS_APPEND_REUSE_WINDOW_MS: u64 = 5_000;

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
    Trace,
    Error,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    role: ChatRole,
    title: SharedString,
    body: SharedString,
    created_at_unix_ms: u64,
    response_latency_ms: Option<u64>,
    trace_turn_key: Option<SharedString>,
    trace_collapsed: bool,
}

#[derive(Debug)]
enum GatewayCommand {
    Disconnect,
    DetectText(String),
    StartUplinkStream,
    StopUplinkStream,
    SetSpeakerOutputEnabled(bool),
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
    speaker_output_enabled: bool,
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
    ws_command_tx: Option<mpsc::Sender<GatewayCommand>>,
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
    show_ai_emotion_messages: bool,
    chat_messages: Vec<ChatMessage>,
    pending_detect_requests: VecDeque<Instant>,
    active_tts_message_index: Option<usize>,
    active_intent_trace_message_index: Option<usize>,
    follow_latest_chat_messages: bool,
    pending_chat_messages: usize,
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
        self.pending_detect_requests.clear();
        self.active_tts_message_index = None;
        self.active_intent_trace_message_index = None;
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

        let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
        let (event_tx, mut event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

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
            speaker_output_enabled: self.speaker_output_enabled,
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

    fn close_audio_dropdowns(&mut self, cx: &mut Context<Self>) {
        if !self.show_input_dropdown && !self.show_output_dropdown {
            return;
        }

        self.show_input_dropdown = false;
        self.show_output_dropdown = false;
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
            "Output playback enabled"
        } else {
            "Output playback paused"
        };

        if matches!(self.connection_state, ConnectionState::Connected) {
            if let Some(command_tx) = self.ws_command_tx.as_ref() {
                if command_tx
                    .try_send(GatewayCommand::SetSpeakerOutputEnabled(
                        self.speaker_output_enabled,
                    ))
                    .is_err()
                {
                    self.push_chat(
                        ChatRole::Error,
                        "Error",
                        "Failed to sync output playback switch to gateway worker",
                    );
                }
            }
        }

        self.push_chat(ChatRole::System, "System", state);
        cx.notify();
    }

    fn toggle_ai_emotion_messages(&mut self, cx: &mut Context<Self>) {
        self.show_ai_emotion_messages = !self.show_ai_emotion_messages;
        let state = if self.show_ai_emotion_messages {
            "AI emotion placeholders are now visible"
        } else {
            "AI emotion placeholders are now hidden"
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

        match command_tx.try_send(GatewayCommand::Disconnect) {
            Ok(_) => {
                self.connection_state = ConnectionState::Disconnecting;
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_intent_trace_message_index = None;
                self.push_chat(ChatRole::System, "System", "Disconnect requested");
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.push_chat(
                    ChatRole::Error,
                    "Error",
                    "Gateway worker queue is full, disconnect delayed",
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
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
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_intent_trace_message_index = None;
            }
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
        self.active_tts_message_index = None;
        self.active_intent_trace_message_index = None;
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
            self.text_draft[0..range.start].to_owned() + new_text + &self.text_draft[range.end..];

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

        let track_detect_request = matches!(&command, GatewayCommand::DetectText(_));

        let Some(command_tx) = self.ws_command_tx.as_ref() else {
            self.push_chat(
                ChatRole::Error,
                "Error",
                "Gateway worker channel not available",
            );
            self.connection_state = ConnectionState::Idle;
            self.gateway_status = GatewayStatus::Idle;
            self.pending_detect_requests.clear();
            self.active_tts_message_index = None;
            self.active_intent_trace_message_index = None;
            cx.notify();
            return;
        };

        match command_tx.try_send(command) {
            Ok(_) => {
                if track_detect_request {
                    self.pending_detect_requests.push_back(Instant::now());
                }
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.push_chat(
                    ChatRole::Error,
                    "Error",
                    "Gateway worker queue is full, command dropped",
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
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
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_intent_trace_message_index = None;
            }
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
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_intent_trace_message_index = None;
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
        if self.handle_tts_inbound_message(&message) {
            return;
        }

        if self.handle_llm_inbound_message(&message) {
            return;
        }

        if self.handle_intent_trace_inbound_message(&message) {
            return;
        }

        let Some((role, title, body)) = describe_inbound_message(&message) else {
            return;
        };

        if role == ChatRole::Assistant && self.is_duplicate_assistant_message(body.as_str()) {
            return;
        }

        let response_latency_ms = if role == ChatRole::Assistant {
            self.consume_response_latency(&message)
        } else {
            None
        };

        self.push_chat_with_metadata(role, title, body, response_latency_ms);
    }

    fn handle_intent_trace_inbound_message(&mut self, message: &InboundTextMessage) -> bool {
        if message.message_type != "notify" {
            return false;
        }

        if read_string_field(&message.payload, "event") != Some("intent_trace") {
            return false;
        }

        let trace_turn_key = extract_intent_trace_turn_key(&message.payload);
        self.upsert_intent_trace_item(trace_turn_key.as_deref(), &message.payload);
        true
    }

    fn upsert_intent_trace_item(&mut self, turn_key: Option<&str>, payload: &Map<String, Value>) {
        if self.append_to_active_intent_trace_message(turn_key, payload) {
            return;
        }

        let first_line = format_intent_trace_line(payload, 1);
        self.push_chat_with_metadata(ChatRole::Trace, "调用链路", first_line, None);
        if let Some(message) = self.chat_messages.first_mut() {
            message.trace_turn_key = turn_key.map(|key| key.to_string().into());
        }
        self.active_intent_trace_message_index = Some(0);
    }

    fn append_to_active_intent_trace_message(
        &mut self,
        turn_key: Option<&str>,
        payload: &Map<String, Value>,
    ) -> bool {
        let Some(index) = self.active_intent_trace_message_index else {
            return false;
        };

        let Some(message) = self.chat_messages.get_mut(index) else {
            self.active_intent_trace_message_index = None;
            return false;
        };

        let message_turn_key = message.trace_turn_key.as_ref().map(SharedString::as_ref);
        if message.role != ChatRole::Trace || !is_same_intent_trace_turn(message_turn_key, turn_key)
        {
            self.active_intent_trace_message_index = None;
            return false;
        }

        let step_index = message
            .body
            .as_ref()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            .saturating_add(1);
        let next_line = format_intent_trace_line(payload, step_index);
        let updated_body = format!("{}\n{}", message.body, next_line);
        message.body = updated_body.into();

        if step_index == 2 && !message.trace_collapsed {
            message.trace_collapsed = true;
        }

        if self.follow_latest_chat_messages {
            self.chat_scroll.scroll_to_bottom();
        }

        true
    }

    fn handle_llm_inbound_message(&mut self, message: &InboundTextMessage) -> bool {
        if message.message_type != "llm" {
            return false;
        }

        let Some(text) = read_string_field(&message.payload, "text")
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            return true;
        };

        if is_llm_emotion_placeholder(text) {
            if !self.show_ai_emotion_messages || self.is_duplicate_assistant_message(text) {
                return true;
            }

            self.push_chat(ChatRole::Assistant, "AI Emotion", text.to_string());
            return true;
        }

        if self.is_duplicate_assistant_message(text) {
            return true;
        }

        self.active_intent_trace_message_index = None;
        let response_latency_ms = self.consume_response_latency(message);
        self.push_chat_with_metadata(
            ChatRole::Assistant,
            "AI",
            text.to_string(),
            response_latency_ms,
        );

        true
    }

    fn handle_tts_inbound_message(&mut self, message: &InboundTextMessage) -> bool {
        if message.message_type != "tts" {
            return false;
        }

        let state = read_string_field(&message.payload, "state").unwrap_or("unknown");
        match state {
            "start" => {
                self.active_tts_message_index = None;
            }
            "sentence_start" => {
                let Some(text) = read_string_field(&message.payload, "text")
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                else {
                    return true;
                };

                self.active_intent_trace_message_index = None;
                let response_latency_ms = self.consume_response_latency(message);
                self.upsert_tts_sentence_text(text, response_latency_ms);
            }
            "stop" => {
                self.active_tts_message_index = None;
                self.active_intent_trace_message_index = None;
                if read_bool_field(&message.payload, "is_aborted") == Some(true) {
                    self.push_chat(ChatRole::System, "System", "TTS playback aborted");
                }
            }
            _ => {}
        }

        true
    }

    fn upsert_tts_sentence_text(&mut self, text: &str, response_latency_ms: Option<u64>) {
        if self.append_to_active_tts_message(text, response_latency_ms) {
            return;
        }

        let now_ms = unix_now_millis();
        if let Some(latest_message) = self.chat_messages.first_mut() {
            let message_is_recent = now_ms.saturating_sub(latest_message.created_at_unix_ms)
                <= TTS_APPEND_REUSE_WINDOW_MS;
            if latest_message.role == ChatRole::Assistant
                && latest_message.title.as_ref() == "AI"
                && message_is_recent
            {
                latest_message.body = text.to_string().into();
                if latest_message.response_latency_ms.is_none() {
                    latest_message.response_latency_ms = response_latency_ms;
                }
                self.active_tts_message_index = Some(0);
                if self.follow_latest_chat_messages {
                    self.chat_scroll.scroll_to_bottom();
                }
                return;
            }
        }

        self.push_chat_with_metadata(
            ChatRole::Assistant,
            "AI",
            text.to_string(),
            response_latency_ms,
        );
        self.active_tts_message_index = Some(0);
    }

    fn append_to_active_tts_message(
        &mut self,
        text: &str,
        response_latency_ms: Option<u64>,
    ) -> bool {
        let Some(index) = self.active_tts_message_index else {
            return false;
        };

        let Some(message) = self.chat_messages.get_mut(index) else {
            self.active_tts_message_index = None;
            return false;
        };

        if message.role != ChatRole::Assistant || message.title.as_ref() != "AI" {
            self.active_tts_message_index = None;
            return false;
        }

        if !message.body.as_ref().ends_with(text) {
            let mut merged = message.body.to_string();
            merged.push_str(text);
            message.body = merged.into();
        }

        if message.response_latency_ms.is_none() {
            message.response_latency_ms = response_latency_ms;
        }

        if self.follow_latest_chat_messages {
            self.chat_scroll.scroll_to_bottom();
        }

        true
    }

    fn consume_response_latency(&mut self, message: &InboundTextMessage) -> Option<u64> {
        if !is_assistant_text_message(message) {
            return None;
        }

        self.pending_detect_requests
            .pop_front()
            .map(|request_started_at| duration_to_millis(request_started_at.elapsed()))
    }

    fn is_duplicate_assistant_message(&self, body: &str) -> bool {
        let Some(latest_message) = self.chat_messages.first() else {
            return false;
        };

        latest_message.role == ChatRole::Assistant && latest_message.body.as_ref() == body
    }

    fn chat_message_header(&self, message: &ChatMessage) -> String {
        let timestamp = format_message_timestamp(message.created_at_unix_ms);
        match message.response_latency_ms {
            Some(response_latency_ms) => format!(
                "{} {} | {}",
                message.title,
                timestamp,
                format_response_latency(response_latency_ms)
            ),
            None => format!("{} {}", message.title, timestamp),
        }
    }

    fn is_chat_scrolled_to_bottom(&self) -> bool {
        let max_offset = self.chat_scroll.max_offset().height;
        let current_offset = self.chat_scroll.offset().y;
        let distance_to_bottom = (current_offset + max_offset).abs();
        distance_to_bottom <= px(CHAT_BOTTOM_EPSILON_PX)
    }

    fn sync_chat_follow_state(&mut self) {
        if self.is_chat_scrolled_to_bottom() {
            self.follow_latest_chat_messages = true;
            self.pending_chat_messages = 0;
        } else if self.chat_scroll.max_offset().height > px(0.0) {
            self.follow_latest_chat_messages = false;
        }
    }

    fn jump_to_latest_chat_messages(&mut self, cx: &mut Context<Self>) {
        self.follow_latest_chat_messages = true;
        self.pending_chat_messages = 0;
        self.chat_scroll.scroll_to_bottom();
        cx.notify();
    }

    fn toggle_trace_message_collapse(&mut self, message_index: usize, cx: &mut Context<Self>) {
        let Some(message) = self.chat_messages.get_mut(message_index) else {
            return;
        };

        if message.role != ChatRole::Trace {
            return;
        }

        let step_count = message
            .body
            .as_ref()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        if step_count <= 1 {
            return;
        }

        message.trace_collapsed = !message.trace_collapsed;
        cx.notify();
    }

    fn push_chat(
        &mut self,
        role: ChatRole,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
    ) {
        self.push_chat_with_metadata(role, title, body, None);
    }

    fn push_chat_with_metadata(
        &mut self,
        role: ChatRole,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
        response_latency_ms: Option<u64>,
    ) {
        self.sync_chat_follow_state();

        if let Some(index) = self.active_tts_message_index {
            self.active_tts_message_index = index.checked_add(1);
        }

        if let Some(index) = self.active_intent_trace_message_index {
            self.active_intent_trace_message_index = index.checked_add(1);
        }

        self.chat_messages.insert(
            0,
            ChatMessage {
                role,
                title: title.into(),
                body: body.into(),
                created_at_unix_ms: unix_now_millis(),
                response_latency_ms,
                trace_turn_key: None,
                trace_collapsed: false,
            },
        );

        if self.chat_messages.len() > MAX_CHAT_MESSAGES {
            self.chat_messages.truncate(MAX_CHAT_MESSAGES);
        }

        if let Some(index) = self.active_tts_message_index {
            if index >= self.chat_messages.len() {
                self.active_tts_message_index = None;
            }
        }

        if let Some(index) = self.active_intent_trace_message_index {
            if index >= self.chat_messages.len() {
                self.active_intent_trace_message_index = None;
            }
        }

        if self.follow_latest_chat_messages {
            self.pending_chat_messages = 0;
            self.chat_scroll.scroll_to_bottom();
        } else {
            self.pending_chat_messages = self.pending_chat_messages.saturating_add(1);
        }
    }

    fn render_chat_message(
        &self,
        message_index: usize,
        message: &ChatMessage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let header = self.chat_message_header(message);

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
                    trace_content = trace_content.child(
                        div()
                            .id(("toggle-trace-message", message_index))
                            .mt_1()
                            .h_7()
                            .px_3()
                            .rounded_md()
                            .border_1()
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_color(rgb(0x5d4a1c))
                            .bg(rgb(0x1b180b))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xe7c87d))
                            .child(if is_collapsed {
                                "展开调用详情"
                            } else {
                                "收起调用详情"
                            })
                            .on_click(cx.listener(move |view, _event, _window, cx| {
                                view.toggle_trace_message_collapse(message_index, cx)
                            })),
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

impl Render for MeetingHostShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_chat_follow_state();

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
        let text_input_border_hex = if is_text_input_focused {
            0x16d9c0
        } else if self.text_draft.is_empty() {
            0x283449
        } else {
            0x145a58
        };

        let custom_titlebar = cfg!(target_os = "macos").then(|| {
            div()
                .id("custom-titlebar")
                .h_8()
                .px_4()
                .bg(rgb(0x070f1b))
                .border_b_1()
                .border_color(rgb(0x182132))
                .flex()
                .items_center()
                .window_control_area(WindowControlArea::Drag)
                .on_click(cx.listener(|_view, event: &ClickEvent, window, _cx| {
                    if event.standard_click() && event.click_count() >= 2 {
                        window.titlebar_double_click();
                    }
                }))
                .child(div().w_16())
                .child(
                    div()
                        .flex_1()
                        .text_center()
                        .text_base()
                        .text_color(rgb(0x7f8ba1))
                        .child(APP_TITLE),
                )
                .child(div().w_16())
        });

        let connect_button = match self.connection_state {
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
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .flex_1()
                    .child(ui_icon(audio_device_icon(&selected_input), 13.0, 0x4fd7c5))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xd2d9e7))
                            .text_ellipsis()
                            .child(selected_input.clone()),
                    ),
            )
            .child(div().child(ui_icon(IconName::ChevronDown, 12.0, 0x7f8ba1)))
            .on_click(cx.listener(|view, _event, _window, cx| view.toggle_input_dropdown(cx)));

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

            input_selector_field =
                input_selector_field.child(dropdown_overlay_panel(input_dropdown));
        }

        let input_selector = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(input_selector_label)
            .child(input_selector_field);

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
                    .child(ui_icon(audio_device_icon(&selected_output), 13.0, 0x4fd7c5))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xd2d9e7))
                            .text_ellipsis()
                            .child(selected_output.clone()),
                    ),
            )
            .child(div().child(ui_icon(IconName::ChevronDown, 12.0, 0x7f8ba1)))
            .on_click(cx.listener(|view, _event, _window, cx| view.toggle_output_dropdown(cx)));

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

            output_selector_field =
                output_selector_field.child(dropdown_overlay_panel(output_dropdown));
        }

        let output_selector = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(output_selector_label)
            .child(output_selector_field);

        let mic_button = if is_connected {
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
        };

        let speaker_button = if self.speaker_output_enabled {
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
                                    .child(
                                        div().size_2().rounded_full().bg(rgb(connection_status.3)),
                                    )
                                    .child(if is_connected {
                                        ui_icon(IconName::Wifi, 14.0, 0x16d9c0)
                                    } else {
                                        ui_icon(IconName::WifiOff, 14.0, 0x7f8ba1)
                                    })
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
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(rgb(0x8a96ab))
                        .cursor_pointer()
                        .child(ui_icon(IconName::Settings, 14.0, 0x8a96ab))
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
            text_input_shell(text_input_border_hex)
                .id("text-draft-input")
                .track_focus(&self.text_input_focus)
                .on_click(cx.listener(|view, _event, window, cx| view.focus_text_input(window, cx)))
                .on_mouse_down_out(
                    cx.listener(|view, _event, window, cx| view.blur_text_input(window, cx)),
                )
                .on_key_down(cx.listener(|view, event, window, cx| {
                    view.handle_text_input_key(event, window, cx)
                }))
                .child(
                    text_input_content_row()
                        .child(text_input_value(chat_input_text, show_placeholder))
                        .child(text_input_caret(
                            is_text_input_focused,
                            "text-input-caret-blink",
                            0x16d9c0,
                            "text-input-caret",
                        ))
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
            square_icon_button(
                ui_icon(IconName::Send, 14.0, 0x9af9ef),
                UiTone::new(0x115f58, 0x0f5a54, 0x9af9ef),
                true,
            )
            .id("send-text-button")
            .on_click(cx.listener(|view, _event, _window, cx| view.send_text_draft(cx)))
        } else {
            square_icon_button(
                ui_icon(IconName::Send, 14.0, 0x556178),
                UiTone::new(0x2a3448, 0x111928, 0x556178),
                false,
            )
            .id("send-text-button")
        };

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
            .child(message_stream_panel)
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(ui_icon(IconName::Cpu, 12.0, 0x6f7b8f))
                            .child("CPU 2.3%"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(ui_icon(IconName::HardDrive, 12.0, 0x6f7b8f))
                            .child("RAM 48 MB"),
                    )
                    .children(if is_connected {
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_color(rgb(0x16d9c0))
                                .child(ui_icon(IconName::Zap, 12.0, 0x16d9c0))
                                .child("延迟 ~700ms (采集 20ms + 网络 50ms + ASR 200ms + LLM 300ms + TTS 130ms)"),
                        )
                    } else {
                        None
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(rgb(0x7d8798))
                    .child(ui_icon(IconName::Clock, 12.0, 0x7d8798))
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
            .children(custom_titlebar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(chat_panel),
            )
            .child(status_bar);

        if self.show_input_dropdown || self.show_output_dropdown {
            shell_body = shell_body.child(
                modal_backdrop(0x00000000)
                    .id("audio-dropdown-dismiss")
                    .on_click(
                        cx.listener(|view, _event, _window, cx| view.close_audio_dropdowns(cx)),
                    ),
            );
        }

        if self.show_settings_panel {
            let settings_panel = modal_surface(560.0, 640.0)
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
                                .border_1()
                                .border_color(rgb(0x2a3548))
                                .bg(rgb(0x131b2a))
                                .text_color(rgb(0x8a96ab))
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(ui_icon(IconName::X, 13.0, 0x8a96ab))
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
                                        .items_center()
                                        .justify_between()
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
                                                        .child("当 llm 只返回 emoji 等情绪符号时，是否展示在会话列表"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("toggle-ai-emotion-messages")
                                                .h_8()
                                                .px_3()
                                                .rounded_md()
                                                .border_1()
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .border_color(if self.show_ai_emotion_messages {
                                                    rgb(0x165e55)
                                                } else {
                                                    rgb(0x36445a)
                                                })
                                                .bg(if self.show_ai_emotion_messages {
                                                    rgb(0x0c3f3b)
                                                } else {
                                                    rgb(0x131b2a)
                                                })
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(if self.show_ai_emotion_messages {
                                                    rgb(0x6af3e2)
                                                } else {
                                                    rgb(0x9aa6ba)
                                                })
                                                .child(if self.show_ai_emotion_messages {
                                                    "显示中"
                                                } else {
                                                    "已隐藏"
                                                })
                                                .on_click(cx.listener(|view, _event, _window, cx| {
                                                    view.toggle_ai_emotion_messages(cx)
                                                })),
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
                );

            shell_body = shell_body.child(
                modal_overlay_root()
                    .child(modal_backdrop(0x020712d9).id("settings-backdrop").on_click(
                        cx.listener(|view, _event, _window, cx| view.close_settings_panel(cx)),
                    ))
                    .child(modal_center_layer().child(settings_panel)),
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
            self.text_draft[0..range.start].to_owned() + new_text + &self.text_draft[range.end..];

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
    level_meter(label, level_percent, active)
}

fn ui_icon(name: IconName, size_px: f32, color_hex: u32) -> gpui::Svg {
    icon(name, size_px, rgb(color_hex))
}

fn audio_device_icon(name: &str) -> IconName {
    let lower = name.to_ascii_lowercase();
    if lower.contains("blackhole") || lower.contains("loopback") || lower.contains("virtual") {
        IconName::Cable
    } else if lower.contains("airpods") || lower.contains("headphone") {
        IconName::Headphones
    } else {
        IconName::MonitorSpeaker
    }
}

fn setting_row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> impl IntoElement {
    key_value_row(label, value)
}

fn wall_clock_label() -> String {
    let now = utc_datetime_parts_from_millis(unix_now_millis());
    format_hms(now.3, now.4, now.5)
}

fn load_history_chat_messages() -> Vec<ChatMessage> {
    let mut chat_messages = Vec::new();

    let persisted_history_records: Vec<ChatMessage> = Vec::new();
    insert_history_chat_messages(&mut chat_messages, persisted_history_records);

    chat_messages
}

fn insert_history_chat_messages(
    chat_messages: &mut Vec<ChatMessage>,
    mut history_messages: Vec<ChatMessage>,
) {
    if history_messages.is_empty() {
        return;
    }

    history_messages.sort_unstable_by_key(|message| message.created_at_unix_ms);
    history_messages.reverse();

    chat_messages.extend(history_messages);
    chat_messages
        .sort_unstable_by(|left, right| right.created_at_unix_ms.cmp(&left.created_at_unix_ms));

    if chat_messages.len() > MAX_CHAT_MESSAGES {
        chat_messages.truncate(MAX_CHAT_MESSAGES);
    }
}

fn main() {
    Application::new()
        .with_assets(AppAssets::new())
        .run(|cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(APP_TITLE.into()),
                        appears_transparent: cfg!(target_os = "macos"),
                        traffic_light_position: if cfg!(target_os = "macos") {
                            Some(point(px(14.0), px(9.0)))
                        } else {
                            None
                        },
                    }),
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
                        show_ai_emotion_messages: false,
                        chat_messages: load_history_chat_messages(),
                        pending_detect_requests: VecDeque::new(),
                        active_tts_message_index: None,
                        active_intent_trace_message_index: None,
                        follow_latest_chat_messages: true,
                        pending_chat_messages: 0,
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

fn describe_inbound_message(message: &InboundTextMessage) -> Option<(ChatRole, String, String)> {
    match message.message_type.as_str() {
        "hello" => None,
        "stt" => read_string_field(&message.payload, "text")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| (ChatRole::User, "STT".to_string(), text.to_string())),
        "llm" | "tts" => None,
        "audio" => None,
        "mcp" => Some((
            ChatRole::Tool,
            "Tool Call".to_string(),
            summarize_tool_message(&message.payload),
        )),
        "notify" => {
            let event_name = read_string_field(&message.payload, "event").unwrap_or("notify");
            if event_name == "intent_trace" {
                None
            } else {
                Some((
                    ChatRole::System,
                    "Notify".to_string(),
                    compact_json(&Value::Object(message.payload.clone())),
                ))
            }
        }
        _ => Some((
            ChatRole::System,
            format!("Server {}", message.message_type),
            compact_json(&Value::Object(message.payload.clone())),
        )),
    }
}

fn is_assistant_text_message(message: &InboundTextMessage) -> bool {
    match message.message_type.as_str() {
        "llm" => read_string_field(&message.payload, "text")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| !is_llm_emotion_placeholder(text))
            .unwrap_or(false),
        "tts" => {
            read_string_field(&message.payload, "state") == Some("sentence_start")
                && read_string_field(&message.payload, "text")
                    .map(str::trim)
                    .map(|text| !text.is_empty())
                    .unwrap_or(false)
        }
        _ => false,
    }
}

fn is_llm_emotion_placeholder(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut has_emoji = false;
    let mut emoji_unit_count = 0usize;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            continue;
        }

        if !is_emoji_component(ch) {
            return false;
        }

        if ch != '\u{200d}' && ch != '\u{fe0f}' {
            emoji_unit_count = emoji_unit_count.saturating_add(1);
        }

        if (ch as u32) != 0xfe0f && (ch as u32) != 0x200d {
            has_emoji = true;
        }

        if emoji_unit_count > 8 {
            return false;
        }
    }

    has_emoji && emoji_unit_count > 0
}

fn is_emoji_component(ch: char) -> bool {
    let code_point = ch as u32;
    matches!(
        code_point,
        0x1f300..=0x1faff
            | 0x2600..=0x27bf
            | 0x1f1e6..=0x1f1ff
            | 0xfe0f
            | 0x200d
    )
}

fn extract_intent_trace_turn_key(payload: &Map<String, Value>) -> Option<String> {
    payload.get("turn_id").and_then(|turn_id| match turn_id {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| format!("turn:{trimmed}"))
        }
        Value::Number(number) => number
            .as_u64()
            .map(|value| format!("turn:{value}"))
            .or_else(|| {
                number
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .map(|value| format!("turn:{value}"))
            }),
        _ => None,
    })
}

fn is_same_intent_trace_turn(current_turn: Option<&str>, incoming_turn: Option<&str>) -> bool {
    match (current_turn, incoming_turn) {
        (Some(current), Some(incoming)) => current == incoming,
        (None, None) => true,
        _ => false,
    }
}

fn format_intent_trace_line(payload: &Map<String, Value>, step_index: usize) -> String {
    let tool = read_string_field(payload, "tool").unwrap_or("unknown_tool");
    let status = read_string_field(payload, "status").unwrap_or("unknown");
    let source = read_string_field(payload, "source").unwrap_or("-");

    let mut line = format!("{step_index}. [{status}] {tool}");
    if source != "-" {
        line.push_str(&format!(" ({source})"));
    }

    if let Some(error_value) = payload.get("error").filter(|value| !value.is_null()) {
        let error_text = clamp_intent_trace_field(&compact_json(error_value), 88);
        line.push_str(&format!(" | error={error_text}"));
    } else if let Some(result_value) = payload.get("result").filter(|value| !value.is_null()) {
        let result_text = clamp_intent_trace_field(&compact_json(result_value), 88);
        line.push_str(&format!(" | result={result_text}"));
    }

    line
}

fn clamp_intent_trace_field(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...")
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

fn read_string_field<'a>(payload: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn read_bool_field(payload: &Map<String, Value>, key: &str) -> Option<bool> {
    payload.get(key).and_then(Value::as_bool)
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

fn unix_now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(duration_to_millis)
        .unwrap_or_default()
}

fn duration_to_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn utc_datetime_parts_from_millis(timestamp_ms: u64) -> (i32, u32, u32, u32, u32, u32) {
    let total_seconds = timestamp_ms / 1_000;
    let days_since_epoch = total_seconds / 86_400;
    let seconds_of_day = u32::try_from(total_seconds % 86_400).unwrap_or_default();
    let (year, month, day) =
        utc_date_from_days(i64::try_from(days_since_epoch).unwrap_or(i64::MAX));
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    (year, month, day, hour, minute, second)
}

fn utc_date_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let adjusted_year = year + if month <= 2 { 1 } else { 0 };

    (
        i32::try_from(adjusted_year).unwrap_or(i32::MAX),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn format_hms(hour: u32, minute: u32, second: u32) -> String {
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn format_message_timestamp(timestamp_ms: u64) -> String {
    let now = utc_datetime_parts_from_millis(unix_now_millis());
    let timestamp = utc_datetime_parts_from_millis(timestamp_ms);

    if (timestamp.0, timestamp.1, timestamp.2) == (now.0, now.1, now.2) {
        format_hms(timestamp.3, timestamp.4, timestamp.5)
    } else {
        format!(
            "{:04}-{:02}-{:02} {}",
            timestamp.0,
            timestamp.1,
            timestamp.2,
            format_hms(timestamp.3, timestamp.4, timestamp.5)
        )
    }
}

fn format_response_latency(response_latency_ms: u64) -> String {
    if response_latency_ms < 1_000 {
        format!("{response_latency_ms}ms")
    } else {
        format!("{:.1}s", response_latency_ms as f64 / 1_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_intent_trace_turn_key, format_response_latency, is_llm_emotion_placeholder,
    };

    #[test]
    fn emotion_placeholder_detection_handles_emoji_only_text() {
        assert!(is_llm_emotion_placeholder("😀"));
        assert!(is_llm_emotion_placeholder("😀 😄"));
        assert!(!is_llm_emotion_placeholder("hello 😀"));
        assert!(!is_llm_emotion_placeholder(""));
    }

    #[test]
    fn trace_turn_key_supports_string_and_number() {
        let string_turn = serde_json::json!({ "turn_id": "abc" });
        assert_eq!(
            extract_intent_trace_turn_key(string_turn.as_object().expect("object")),
            Some("turn:abc".to_string())
        );

        let numeric_turn = serde_json::json!({ "turn_id": 42 });
        assert_eq!(
            extract_intent_trace_turn_key(numeric_turn.as_object().expect("object")),
            Some("turn:42".to_string())
        );
    }

    #[test]
    fn response_latency_format_is_stable() {
        assert_eq!(format_response_latency(980), "980ms");
        assert_eq!(format_response_latency(1_200), "1.2s");
    }
}
