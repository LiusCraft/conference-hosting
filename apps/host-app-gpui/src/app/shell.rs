use std::collections::VecDeque;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, rgb, ClickEvent, Context, Div, Entity, Render, ScrollHandle, SharedString,
    Stateful, Window, WindowControlArea,
};
use gpui_component::input::{InputEvent, InputState};
use host_core::GatewayStatus;
use tokio::sync::mpsc;

use crate::app::config::build_gateway_config;
use crate::app::state::{
    ChatMessage, ChatRole, ConnectionState, GatewayCommand, UiGatewayEvent, APP_TITLE,
    DEFAULT_WS_URL,
};
use crate::components::{
    icon::{icon, IconName},
    ui::{modal_backdrop, modal_center_layer, modal_overlay_root},
};
use crate::features::chat::{load_history_chat_messages, wall_clock_label};
use crate::gateway_runtime::{
    spawn_gateway_worker, AudioDeviceState, COMMAND_CHANNEL_CAPACITY, EVENT_CHANNEL_CAPACITY,
};

const AUDIO_EVENT_UI_REFRESH_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) struct MeetingHostShell {
    pub(crate) connection_state: ConnectionState,
    pub(crate) gateway_status: GatewayStatus,
    pub(crate) ws_url: String,
    pub(crate) session_id: Option<SharedString>,
    pub(crate) uplink_audio_frames: usize,
    pub(crate) uplink_audio_bytes: usize,
    pub(crate) uplink_streaming: bool,
    pub(crate) downlink_audio_frames: usize,
    pub(crate) downlink_audio_bytes: usize,
    pub(crate) ws_command_tx: Option<mpsc::Sender<GatewayCommand>>,
    pub(crate) chat_input_state: Entity<InputState>,
    pub(crate) ws_url_input_state: Entity<InputState>,
    pub(crate) chat_input_focused: bool,
    pub(crate) ws_url_input_focused: bool,
    pub(crate) chat_scroll: ScrollHandle,
    pub(crate) settings_scroll: ScrollHandle,
    pub(crate) input_devices: Vec<String>,
    pub(crate) output_devices: Vec<String>,
    pub(crate) selected_input_index: Option<usize>,
    pub(crate) selected_input_output_index: Option<usize>,
    pub(crate) input_from_output: bool,
    pub(crate) selected_output_index: Option<usize>,
    pub(crate) show_input_dropdown: bool,
    pub(crate) show_output_dropdown: bool,
    pub(crate) show_settings_panel: bool,
    pub(crate) speaker_output_enabled: bool,
    pub(crate) show_ai_emotion_messages: bool,
    pub(crate) chat_messages: Vec<ChatMessage>,
    pub(crate) pending_detect_requests: VecDeque<Instant>,
    pub(crate) active_tts_message_index: Option<usize>,
    pub(crate) active_intent_trace_message_index: Option<usize>,
    pub(crate) follow_latest_chat_messages: bool,
    pub(crate) pending_chat_messages: usize,
    pub(crate) last_audio_event_ui_refresh_at: Option<Instant>,
}

impl MeetingHostShell {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        audio_state: AudioDeviceState,
    ) -> Self {
        let initial_ws_url = crate::app::config::env_or_default("HOST_WS_URL", DEFAULT_WS_URL);
        let chat_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入指令 (例: listen detect)")
                .clean_on_escape()
        });
        let ws_url_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("ws://host/path?device-id=...")
                .default_value(initial_ws_url.clone())
        });

        cx.subscribe_in(
            &chat_input_state,
            window,
            |view, _state, event: &InputEvent, window, cx| {
                view.handle_chat_input_event(event, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &ws_url_input_state,
            window,
            |view, _state, event: &InputEvent, window, cx| {
                view.handle_ws_url_input_event(event, window, cx);
            },
        )
        .detach();

        Self {
            connection_state: ConnectionState::Idle,
            gateway_status: GatewayStatus::Idle,
            ws_url: initial_ws_url,
            session_id: None,
            uplink_audio_frames: 0,
            uplink_audio_bytes: 0,
            uplink_streaming: false,
            downlink_audio_frames: 0,
            downlink_audio_bytes: 0,
            ws_command_tx: None,
            chat_input_state,
            ws_url_input_state,
            chat_input_focused: false,
            ws_url_input_focused: false,
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
            last_audio_event_ui_refresh_at: None,
        }
    }

    pub(crate) fn prepare_for_window_close(&mut self) {
        self.show_input_dropdown = false;
        self.show_output_dropdown = false;
        self.show_settings_panel = false;

        if !matches!(self.connection_state, ConnectionState::Connected) {
            return;
        }

        if let Some(command_tx) = self.ws_command_tx.as_ref() {
            let _ = command_tx.try_send(GatewayCommand::Disconnect);
        }

        self.connection_state = ConnectionState::Disconnecting;
        self.uplink_streaming = false;
        self.pending_detect_requests.clear();
        self.active_tts_message_index = None;
        self.active_intent_trace_message_index = None;
        self.last_audio_event_ui_refresh_at = None;
    }

    pub(crate) fn notify_views(&self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub(crate) fn connect_gateway(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.connection_state, ConnectionState::Idle) {
            return;
        }

        self.sync_ws_url_from_input(cx);
        let config = build_gateway_config(Some(self.ws_url.as_str()));
        let audio_routing = self.build_audio_routing_config();
        self.ws_url = config.server_url.clone();
        self.connection_state = ConnectionState::Connecting;
        self.session_id = None;
        self.uplink_streaming = false;
        self.pending_detect_requests.clear();
        self.active_tts_message_index = None;
        self.active_intent_trace_message_index = None;
        self.last_audio_event_ui_refresh_at = None;
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

        self.notify_views(cx);
    }

    pub(crate) fn disconnect_gateway(&mut self, cx: &mut Context<Self>) {
        let Some(command_tx) = self.ws_command_tx.as_ref() else {
            return;
        };

        match command_tx.try_send(GatewayCommand::Disconnect) {
            Ok(_) => {
                self.connection_state = ConnectionState::Disconnecting;
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_intent_trace_message_index = None;
                self.last_audio_event_ui_refresh_at = None;
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
                self.last_audio_event_ui_refresh_at = None;
            }
        }

        self.notify_views(cx);
    }

    pub(crate) fn toggle_uplink_stream(&mut self, cx: &mut Context<Self>) {
        let command = if self.uplink_streaming {
            GatewayCommand::StopUplinkStream
        } else {
            GatewayCommand::StartUplinkStream
        };
        self.send_gateway_command(command, cx);
    }

    pub(crate) fn send_gateway_command(&mut self, command: GatewayCommand, cx: &mut Context<Self>) {
        if !matches!(self.connection_state, ConnectionState::Connected) {
            self.push_chat(ChatRole::System, "System", "Gateway is not connected yet");
            self.notify_views(cx);
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
            self.last_audio_event_ui_refresh_at = None;
            self.notify_views(cx);
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
                self.last_audio_event_ui_refresh_at = None;
            }
        }

        self.notify_views(cx);
    }

    pub(crate) fn handle_gateway_event(&mut self, event: UiGatewayEvent, cx: &mut Context<Self>) {
        let should_notify = match event {
            UiGatewayEvent::Connected { session_id } => {
                self.connection_state = ConnectionState::Connected;
                self.gateway_status = GatewayStatus::Connected;
                self.session_id = Some(session_id.clone().into());
                self.last_audio_event_ui_refresh_at = None;
                self.push_chat(
                    ChatRole::System,
                    "System",
                    format!("Handshake finished, session_id={session_id}"),
                );
                true
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
                self.last_audio_event_ui_refresh_at = None;
                self.push_chat(ChatRole::System, "System", "Gateway disconnected");
                true
            }
            UiGatewayEvent::SystemNotice(message) => {
                self.push_chat(ChatRole::System, "System", message);
                true
            }
            UiGatewayEvent::Error(message) => {
                self.push_chat(ChatRole::Error, "Error", message);
                true
            }
            UiGatewayEvent::OutgoingText { kind, payload } => {
                self.push_chat(ChatRole::Client, format!("Client {kind}"), payload);
                true
            }
            UiGatewayEvent::IncomingText(message) => {
                self.push_inbound_message(message);
                true
            }
            UiGatewayEvent::UplinkAudioFrameSent(frame_bytes) => {
                self.uplink_audio_frames += 1;
                self.uplink_audio_bytes += frame_bytes;
                self.should_refresh_audio_event_ui(Instant::now())
            }
            UiGatewayEvent::UplinkStreamStateChanged(is_streaming) => {
                self.uplink_streaming = is_streaming;
                self.last_audio_event_ui_refresh_at = None;
                self.push_chat(
                    ChatRole::System,
                    "System",
                    if is_streaming {
                        "Microphone uplink Opus stream started"
                    } else {
                        "Microphone uplink Opus stream stopped"
                    },
                );
                true
            }
            UiGatewayEvent::DownlinkAudioFrameReceived(frame_bytes) => {
                self.downlink_audio_frames += 1;
                self.downlink_audio_bytes += frame_bytes;
                self.should_refresh_audio_event_ui(Instant::now())
            }
        };

        if should_notify {
            self.notify_views(cx);
        }
    }

    fn should_refresh_audio_event_ui(&mut self, now: Instant) -> bool {
        if should_refresh_audio_event_ui(
            self.last_audio_event_ui_refresh_at,
            now,
            AUDIO_EVENT_UI_REFRESH_INTERVAL,
        ) {
            self.last_audio_event_ui_refresh_at = Some(now);
            true
        } else {
            false
        }
    }

    fn render_custom_titlebar(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        cfg!(target_os = "macos").then(|| {
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
        })
    }

    fn render_status_bar(&self) -> Div {
        let is_connected = matches!(self.connection_state, ConnectionState::Connected);

        div()
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
            )
    }
}

impl Render for MeetingHostShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_chat_follow_state();

        let custom_titlebar = self.render_custom_titlebar(cx);
        let sidebar = self.render_sidebar(cx);
        let chat_panel = self.render_chat_panel(window, cx);
        let status_bar = self.render_status_bar();

        let mut shell_body = div()
            .relative()
            .size_full()
            .bg(rgb(0x040811))
            .overflow_hidden()
            .flex()
            .flex_col()
            .min_h_0()
            .id("shell-body")
            .on_click(cx.listener(|view, _event, _window, cx| view.close_audio_dropdowns(cx)))
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

        if self.show_settings_panel {
            let settings_panel = self.render_settings_panel(window, cx);

            shell_body = shell_body.child(
                modal_overlay_root()
                    .child(modal_backdrop(0x020712d9).id("settings-backdrop"))
                    .child(modal_center_layer().child(settings_panel)),
            );
        }

        div().size_full().bg(rgb(0x040811)).child(shell_body)
    }
}

pub(crate) fn ui_icon(name: IconName, size_px: f32, color_hex: u32) -> gpui::Svg {
    icon(name, size_px, rgb(color_hex))
}

fn should_refresh_audio_event_ui(
    last_refresh_at: Option<Instant>,
    now: Instant,
    refresh_interval: Duration,
) -> bool {
    match last_refresh_at {
        None => true,
        Some(last_refresh_at) => now.duration_since(last_refresh_at) >= refresh_interval,
    }
}

#[cfg(test)]
mod tests {
    use super::{should_refresh_audio_event_ui, AUDIO_EVENT_UI_REFRESH_INTERVAL};
    use std::time::{Duration, Instant};

    #[test]
    fn audio_event_refresh_allows_first_frame() {
        let now = Instant::now();

        assert!(should_refresh_audio_event_ui(
            None,
            now,
            AUDIO_EVENT_UI_REFRESH_INTERVAL
        ));
    }

    #[test]
    fn audio_event_refresh_throttles_dense_frames() {
        let start = Instant::now();
        let within_interval = start + Duration::from_millis(50);
        let after_interval = start + AUDIO_EVENT_UI_REFRESH_INTERVAL;

        assert!(!should_refresh_audio_event_ui(
            Some(start),
            within_interval,
            AUDIO_EVENT_UI_REFRESH_INTERVAL,
        ));
        assert!(should_refresh_audio_event_ui(
            Some(start),
            after_interval,
            AUDIO_EVENT_UI_REFRESH_INTERVAL,
        ));
    }
}
