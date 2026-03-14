use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, rgb, rgba, ClickEvent, Context, Div, Entity, Render, ScrollHandle,
    SharedString, Stateful, Window, WindowControlArea,
};
use gpui_component::input::{InputEvent, InputState};
use host_core::{GatewayStatus, ListenMode};
use tokio::sync::mpsc;

use crate::app::config::{
    build_gateway_config, default_client_id, default_device_mac, env_or_default,
};
use crate::app::persistence::load_persisted_app_settings;
use crate::app::state::{
    ChatMessage, ChatRole, ConnectionState, GatewayCommand, UiGatewayEvent, APP_TITLE,
    DEFAULT_TOKEN, DEFAULT_WS_URL,
};
use crate::components::icon::{icon, IconName};
use crate::features::audio::{
    build_input_select_items, build_output_select_items, selected_input_selection,
    InputSelectEvent, InputSelectState, OutputSelectEvent, OutputSelectState,
};
use crate::features::chat::{load_history_chat_messages, wall_clock_label};
use crate::gateway_runtime::{
    spawn_gateway_worker, AudioDeviceState, COMMAND_CHANNEL_CAPACITY, EVENT_CHANNEL_CAPACITY,
};
use crate::mcp::{
    McpServerConfig, McpServerProbeStatus, McpTransportKind, DEFAULT_MCP_CONNECT_TIMEOUT_MS,
    DEFAULT_MCP_REQUEST_TIMEOUT_MS,
};

const AUDIO_EVENT_UI_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const RESPONSE_LATENCY_SAMPLE_SIZE: usize = 5;

pub(crate) struct MeetingHostShell {
    pub(crate) connection_state: ConnectionState,
    pub(crate) gateway_status: GatewayStatus,
    pub(crate) ws_url: String,
    pub(crate) ws_url_draft: String,
    pub(crate) session_id: Option<SharedString>,
    pub(crate) connected_at: Option<Instant>,
    pub(crate) network_rtt_ms: Option<u32>,
    pub(crate) uplink_audio_frames: usize,
    pub(crate) uplink_audio_bytes: usize,
    pub(crate) uplink_streaming: bool,
    pub(crate) downlink_audio_frames: usize,
    pub(crate) downlink_audio_bytes: usize,
    pub(crate) ws_command_tx: Option<mpsc::Sender<GatewayCommand>>,
    pub(crate) chat_input_state: Entity<InputState>,
    pub(crate) ws_url_input_state: Entity<InputState>,
    pub(crate) auth_token_input_state: Entity<InputState>,
    pub(crate) device_id_input_state: Entity<InputState>,
    pub(crate) client_id_input_state: Entity<InputState>,
    pub(crate) input_select_state: Entity<InputSelectState>,
    pub(crate) output_select_state: Entity<OutputSelectState>,
    pub(crate) chat_input_focused: bool,
    pub(crate) ws_url_input_focused: bool,
    pub(crate) auth_token_input_focused: bool,
    pub(crate) device_id_input_focused: bool,
    pub(crate) client_id_input_focused: bool,
    pub(crate) auth_token: String,
    pub(crate) auth_token_draft: String,
    pub(crate) device_id: String,
    pub(crate) device_id_draft: String,
    pub(crate) client_id: String,
    pub(crate) client_id_draft: String,
    pub(crate) chat_scroll: ScrollHandle,
    pub(crate) settings_scroll: ScrollHandle,
    pub(crate) input_devices: Vec<String>,
    pub(crate) output_devices: Vec<String>,
    pub(crate) selected_input_index: Option<usize>,
    pub(crate) selected_input_output_index: Option<usize>,
    pub(crate) input_from_output: bool,
    pub(crate) selected_output_index: Option<usize>,
    pub(crate) show_settings_panel: bool,
    pub(crate) speaker_output_enabled: bool,
    pub(crate) aec_enabled: bool,
    pub(crate) aec_enabled_draft: bool,
    pub(crate) aec_stream_delay_ms: Option<u32>,
    pub(crate) aec_capture_callback_delay_ms: Option<u32>,
    pub(crate) aec_playback_callback_delay_ms: Option<u32>,
    pub(crate) aec_playback_buffer_delay_ms: Option<u32>,
    pub(crate) aec_processor_delay_ms: Option<i32>,
    pub(crate) aec_erl_db: Option<f32>,
    pub(crate) aec_erle_db: Option<f32>,
    pub(crate) listen_mode: ListenMode,
    pub(crate) listen_mode_draft: ListenMode,
    pub(crate) show_ai_emotion_messages: bool,
    pub(crate) show_ai_emotion_messages_draft: bool,
    pub(crate) show_debug_logs: bool,
    pub(crate) show_debug_logs_draft: bool,
    pub(crate) mcp_servers: Vec<McpServerConfig>,
    pub(crate) mcp_servers_draft: Vec<McpServerConfig>,
    pub(crate) mcp_server_statuses: Vec<McpServerProbeStatus>,
    pub(crate) mcp_tools_expanded_servers: HashSet<String>,
    pub(crate) mcp_probe_in_progress: bool,
    pub(crate) show_mcp_editor: bool,
    pub(crate) mcp_editor_server_id: Option<String>,
    pub(crate) mcp_editor_enabled: bool,
    pub(crate) mcp_editor_transport: McpTransportKind,
    pub(crate) mcp_form_error: Option<String>,
    pub(crate) mcp_form_notice: Option<String>,
    pub(crate) mcp_alias_input_state: Entity<InputState>,
    pub(crate) mcp_endpoint_input_state: Entity<InputState>,
    pub(crate) mcp_args_input_state: Entity<InputState>,
    pub(crate) mcp_env_headers_input_state: Entity<InputState>,
    pub(crate) mcp_cwd_input_state: Entity<InputState>,
    pub(crate) mcp_auth_input_state: Entity<InputState>,
    pub(crate) mcp_request_timeout_input_state: Entity<InputState>,
    pub(crate) mcp_connect_timeout_input_state: Entity<InputState>,
    pub(crate) chat_messages: Vec<ChatMessage>,
    pub(crate) pending_detect_requests: VecDeque<Instant>,
    pub(crate) active_tts_message_index: Option<usize>,
    pub(crate) active_stt_message_index: Option<usize>,
    pub(crate) active_intent_trace_message_index: Option<usize>,
    pub(crate) follow_latest_chat_messages: bool,
    pub(crate) render_full_chat_history: bool,
    pub(crate) pending_chat_messages: usize,
    pub(crate) has_pending_chat_messages: bool,
    pub(crate) last_audio_event_ui_refresh_at: Option<Instant>,
}

impl MeetingHostShell {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        audio_state: AudioDeviceState,
    ) -> Self {
        let persisted_settings = load_persisted_app_settings();
        let initial_ws_url = prefer_non_empty(
            &persisted_settings.ws.server_url,
            env_or_default("HOST_WS_URL", DEFAULT_WS_URL),
        );
        let initial_device_mac = env_or_default("HOST_DEVICE_MAC", &default_device_mac());
        let initial_device_id = prefer_non_empty(
            &persisted_settings.ws.device_id,
            env_or_default("HOST_DEVICE_ID", &initial_device_mac),
        );
        let initial_client_id = prefer_non_empty(
            &persisted_settings.ws.client_id,
            env_or_default("HOST_CLIENT_ID", &default_client_id()),
        );
        let initial_auth_token = prefer_non_empty(
            &persisted_settings.ws.auth_token,
            env_or_default("HOST_TOKEN", DEFAULT_TOKEN),
        );
        let initial_aec_enabled = persisted_settings
            .ui
            .aec_enabled
            .unwrap_or_else(|| env_bool_or_default("HOST_ENABLE_AEC", true));
        let initial_show_ai_emotion_messages = persisted_settings
            .ui
            .show_ai_emotion_messages
            .unwrap_or(false);
        let initial_show_debug_logs = persisted_settings.ui.show_debug_logs.unwrap_or(false);
        let initial_listen_mode = persisted_settings
            .ui
            .listen_mode
            .unwrap_or(ListenMode::Manual);
        let initial_mcp_servers = persisted_settings.mcp_servers;
        let chat_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入指令 (例: listen detect)")
                .clean_on_escape()
        });
        let ws_url_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("wss://host/path?device-id=...")
                .default_value(initial_ws_url.clone())
        });
        let auth_token_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("token（自动拼接 Bearer）")
                .default_value(initial_auth_token.clone())
        });
        let device_id_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("device-id")
                .default_value(initial_device_id.clone())
        });
        let client_id_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("client-id")
                .default_value(initial_client_id.clone())
        });
        let mcp_alias_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("calendar")
                .clean_on_escape()
        });
        let mcp_endpoint_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("stdio command / https://mcp.example.com/sse")
                .clean_on_escape()
        });
        let mcp_args_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("args 或 headers, 例如: --stdio, Authorization=Bearer token")
                .clean_on_escape()
        });
        let mcp_env_headers_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("env 或 headers, 例如: LANG=zh_CN.UTF-8")
                .clean_on_escape()
        });
        let mcp_cwd_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("cwd (stdio 可选)")
                .clean_on_escape()
        });
        let mcp_auth_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("stream auth (可选，自动映射到 header)")
                .clean_on_escape()
        });
        let mcp_request_timeout_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("request_timeout_ms")
                .default_value(DEFAULT_MCP_REQUEST_TIMEOUT_MS.to_string())
        });
        let mcp_connect_timeout_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("connect_timeout_ms")
                .default_value(DEFAULT_MCP_CONNECT_TIMEOUT_MS.to_string())
        });
        let input_select_items =
            build_input_select_items(&audio_state.input_devices, &audio_state.output_devices);
        let output_select_items = build_output_select_items(&audio_state.output_devices);

        let input_select_state =
            cx.new(|cx| InputSelectState::new(input_select_items, None, window, cx));
        let output_select_state =
            cx.new(|cx| OutputSelectState::new(output_select_items, None, window, cx));

        if let Some(selection) = selected_input_selection(
            audio_state.input_from_output,
            audio_state.selected_input_index,
            audio_state.selected_input_output_index,
        ) {
            input_select_state.update(cx, |state, cx| {
                state.set_selected_value(&selection, window, cx);
            });
        }

        if let Some(output_index) = audio_state.selected_output_index {
            output_select_state.update(cx, |state, cx| {
                state.set_selected_value(&output_index, window, cx);
            });
        }

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
        cx.subscribe_in(
            &auth_token_input_state,
            window,
            |view, _state, event: &InputEvent, window, cx| {
                view.handle_auth_token_input_event(event, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &device_id_input_state,
            window,
            |view, _state, event: &InputEvent, window, cx| {
                view.handle_device_id_input_event(event, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &client_id_input_state,
            window,
            |view, _state, event: &InputEvent, window, cx| {
                view.handle_client_id_input_event(event, window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &input_select_state,
            window,
            |view, _state, event: &InputSelectEvent, _window, cx| {
                view.handle_input_select_event(event, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &output_select_state,
            window,
            |view, _state, event: &OutputSelectEvent, _window, cx| {
                view.handle_output_select_event(event, cx);
            },
        )
        .detach();

        let mut shell = Self {
            connection_state: ConnectionState::Idle,
            gateway_status: GatewayStatus::Idle,
            ws_url: initial_ws_url.clone(),
            ws_url_draft: initial_ws_url,
            session_id: None,
            connected_at: None,
            network_rtt_ms: None,
            uplink_audio_frames: 0,
            uplink_audio_bytes: 0,
            uplink_streaming: false,
            downlink_audio_frames: 0,
            downlink_audio_bytes: 0,
            ws_command_tx: None,
            chat_input_state,
            ws_url_input_state,
            auth_token_input_state,
            device_id_input_state,
            client_id_input_state,
            mcp_alias_input_state,
            mcp_endpoint_input_state,
            mcp_args_input_state,
            mcp_env_headers_input_state,
            mcp_cwd_input_state,
            mcp_auth_input_state,
            mcp_request_timeout_input_state,
            mcp_connect_timeout_input_state,
            input_select_state,
            output_select_state,
            chat_input_focused: false,
            ws_url_input_focused: false,
            auth_token_input_focused: false,
            device_id_input_focused: false,
            client_id_input_focused: false,
            auth_token: initial_auth_token.clone(),
            auth_token_draft: initial_auth_token,
            device_id: initial_device_id.clone(),
            device_id_draft: initial_device_id,
            client_id: initial_client_id.clone(),
            client_id_draft: initial_client_id,
            chat_scroll: ScrollHandle::new(),
            settings_scroll: ScrollHandle::new(),
            input_devices: audio_state.input_devices,
            output_devices: audio_state.output_devices,
            selected_input_index: audio_state.selected_input_index,
            selected_input_output_index: audio_state.selected_input_output_index,
            input_from_output: audio_state.input_from_output,
            selected_output_index: audio_state.selected_output_index,
            show_settings_panel: false,
            speaker_output_enabled: true,
            aec_enabled: initial_aec_enabled,
            aec_enabled_draft: initial_aec_enabled,
            aec_stream_delay_ms: None,
            aec_capture_callback_delay_ms: None,
            aec_playback_callback_delay_ms: None,
            aec_playback_buffer_delay_ms: None,
            aec_processor_delay_ms: None,
            aec_erl_db: None,
            aec_erle_db: None,
            listen_mode: initial_listen_mode,
            listen_mode_draft: initial_listen_mode,
            show_ai_emotion_messages: initial_show_ai_emotion_messages,
            show_ai_emotion_messages_draft: initial_show_ai_emotion_messages,
            show_debug_logs: initial_show_debug_logs,
            show_debug_logs_draft: initial_show_debug_logs,
            mcp_servers: initial_mcp_servers.clone(),
            mcp_servers_draft: initial_mcp_servers,
            mcp_server_statuses: Vec::new(),
            mcp_tools_expanded_servers: HashSet::new(),
            mcp_probe_in_progress: false,
            show_mcp_editor: false,
            mcp_editor_server_id: None,
            mcp_editor_enabled: true,
            mcp_editor_transport: McpTransportKind::Stdio,
            mcp_form_error: None,
            mcp_form_notice: None,
            chat_messages: load_history_chat_messages(),
            pending_detect_requests: VecDeque::new(),
            active_tts_message_index: None,
            active_stt_message_index: None,
            active_intent_trace_message_index: None,
            follow_latest_chat_messages: true,
            render_full_chat_history: false,
            pending_chat_messages: 0,
            has_pending_chat_messages: false,
            last_audio_event_ui_refresh_at: None,
        };

        shell.warmup_mcp_tools_cache_on_startup(cx);
        shell
    }

    pub(crate) fn prepare_for_window_close(&mut self) {
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
        self.active_stt_message_index = None;
        self.active_intent_trace_message_index = None;
        self.render_full_chat_history = false;
        self.network_rtt_ms = None;
        self.last_audio_event_ui_refresh_at = None;
        self.aec_stream_delay_ms = None;
        self.aec_capture_callback_delay_ms = None;
        self.aec_playback_callback_delay_ms = None;
        self.aec_playback_buffer_delay_ms = None;
        self.aec_processor_delay_ms = None;
        self.aec_erl_db = None;
        self.aec_erle_db = None;
    }

    pub(crate) fn notify_views(&self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub(crate) fn connect_gateway(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.connection_state, ConnectionState::Idle) {
            return;
        }

        let config = build_gateway_config(
            Some(self.ws_url.as_str()),
            Some(self.device_id.as_str()),
            Some(self.client_id.as_str()),
            Some(self.auth_token.as_str()),
            true,
        );
        self.enforce_aec_for_shared_audio_route();
        let audio_routing = self.build_audio_routing_config();
        let mcp_servers = self.mcp_servers.clone();
        self.ws_url = config.server_url.clone();
        self.device_id = config.device_id.clone();
        self.client_id = config.client_id.clone();
        self.auth_token = config.token.clone();
        self.connection_state = ConnectionState::Connecting;
        self.session_id = None;
        self.connected_at = None;
        self.network_rtt_ms = None;
        self.uplink_audio_frames = 0;
        self.uplink_audio_bytes = 0;
        self.downlink_audio_frames = 0;
        self.downlink_audio_bytes = 0;
        self.uplink_streaming = false;
        self.pending_detect_requests.clear();
        self.active_tts_message_index = None;
        self.active_stt_message_index = None;
        self.active_intent_trace_message_index = None;
        self.render_full_chat_history = false;
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

        spawn_gateway_worker(
            config,
            audio_routing,
            self.listen_mode,
            mcp_servers,
            command_rx,
            event_tx,
        );

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
                self.active_stt_message_index = None;
                self.active_intent_trace_message_index = None;
                self.render_full_chat_history = false;
                self.network_rtt_ms = None;
                self.last_audio_event_ui_refresh_at = None;
                self.aec_stream_delay_ms = None;
                self.aec_capture_callback_delay_ms = None;
                self.aec_playback_callback_delay_ms = None;
                self.aec_playback_buffer_delay_ms = None;
                self.aec_processor_delay_ms = None;
                self.aec_erl_db = None;
                self.aec_erle_db = None;
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
                self.connected_at = None;
                self.network_rtt_ms = None;
                self.uplink_audio_frames = 0;
                self.uplink_audio_bytes = 0;
                self.downlink_audio_frames = 0;
                self.downlink_audio_bytes = 0;
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_stt_message_index = None;
                self.active_intent_trace_message_index = None;
                self.render_full_chat_history = false;
                self.last_audio_event_ui_refresh_at = None;
                self.aec_stream_delay_ms = None;
                self.aec_capture_callback_delay_ms = None;
                self.aec_playback_callback_delay_ms = None;
                self.aec_playback_buffer_delay_ms = None;
                self.aec_processor_delay_ms = None;
                self.aec_erl_db = None;
                self.aec_erle_db = None;
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
            self.active_stt_message_index = None;
            self.active_intent_trace_message_index = None;
            self.render_full_chat_history = false;
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
                self.connected_at = None;
                self.network_rtt_ms = None;
                self.uplink_audio_frames = 0;
                self.uplink_audio_bytes = 0;
                self.downlink_audio_frames = 0;
                self.downlink_audio_bytes = 0;
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_stt_message_index = None;
                self.active_intent_trace_message_index = None;
                self.render_full_chat_history = false;
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
                self.connected_at = Some(Instant::now());
                self.network_rtt_ms = None;
                self.uplink_audio_frames = 0;
                self.uplink_audio_bytes = 0;
                self.downlink_audio_frames = 0;
                self.downlink_audio_bytes = 0;
                self.render_full_chat_history = false;
                self.last_audio_event_ui_refresh_at = None;
                self.aec_stream_delay_ms = None;
                self.aec_capture_callback_delay_ms = None;
                self.aec_playback_callback_delay_ms = None;
                self.aec_playback_buffer_delay_ms = None;
                self.aec_processor_delay_ms = None;
                self.aec_erl_db = None;
                self.aec_erle_db = None;
                self.push_chat(
                    ChatRole::System,
                    "System",
                    format!("Handshake finished, session_id={session_id}"),
                );

                if let Some(command_tx) = self.ws_command_tx.as_ref() {
                    if command_tx
                        .try_send(GatewayCommand::StartUplinkStream)
                        .is_err()
                    {
                        self.push_chat(
                            ChatRole::Error,
                            "Error",
                            "Connected, but failed to auto-start microphone uplink",
                        );
                    }
                }
                true
            }
            UiGatewayEvent::Disconnected => {
                self.connection_state = ConnectionState::Idle;
                self.gateway_status = GatewayStatus::Idle;
                self.uplink_streaming = false;
                self.ws_command_tx = None;
                self.session_id = None;
                self.connected_at = None;
                self.network_rtt_ms = None;
                self.uplink_audio_frames = 0;
                self.uplink_audio_bytes = 0;
                self.downlink_audio_frames = 0;
                self.downlink_audio_bytes = 0;
                self.pending_detect_requests.clear();
                self.active_tts_message_index = None;
                self.active_stt_message_index = None;
                self.active_intent_trace_message_index = None;
                self.render_full_chat_history = false;
                self.last_audio_event_ui_refresh_at = None;
                self.aec_stream_delay_ms = None;
                self.aec_capture_callback_delay_ms = None;
                self.aec_playback_callback_delay_ms = None;
                self.aec_playback_buffer_delay_ms = None;
                self.aec_processor_delay_ms = None;
                self.aec_erl_db = None;
                self.aec_erle_db = None;
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
            UiGatewayEvent::NetworkRttUpdated(rtt_ms) => {
                self.network_rtt_ms = Some(rtt_ms);
                true
            }
            UiGatewayEvent::AecStateChanged(enabled) => {
                self.aec_enabled = enabled;
                self.aec_enabled_draft = enabled;
                if !enabled {
                    self.aec_stream_delay_ms = None;
                    self.aec_capture_callback_delay_ms = None;
                    self.aec_playback_callback_delay_ms = None;
                    self.aec_playback_buffer_delay_ms = None;
                    self.aec_processor_delay_ms = None;
                    self.aec_erl_db = None;
                    self.aec_erle_db = None;
                }
                true
            }
            UiGatewayEvent::AecStats(stats) => {
                self.aec_stream_delay_ms = Some(stats.stream_delay_ms);
                self.aec_capture_callback_delay_ms = Some(stats.capture_callback_delay_ms);
                self.aec_playback_callback_delay_ms = Some(stats.playback_callback_delay_ms);
                self.aec_playback_buffer_delay_ms = Some(stats.playback_buffer_delay_ms);
                self.aec_processor_delay_ms = Some(stats.processor_delay_ms);
                self.aec_erl_db = Some(stats.erl_db);
                self.aec_erle_db = Some(stats.erle_db);
                self.should_refresh_audio_event_ui(Instant::now())
            }
            UiGatewayEvent::McpProbeStatuses(statuses) => {
                self.mcp_server_statuses = statuses;
                self.mcp_tools_expanded_servers.retain(|server_id| {
                    self.mcp_server_statuses
                        .iter()
                        .any(|status| status.server_id == *server_id && !status.tools.is_empty())
                });
                true
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

    fn session_uptime_label(&self) -> Option<String> {
        self.connected_at
            .map(|connected_at| format_elapsed_duration(connected_at.elapsed()))
    }

    fn transport_rate_label(&self, frames: usize, bytes: usize) -> String {
        let Some(connected_at) = self.connected_at else {
            return "--".to_string();
        };

        let elapsed_seconds = connected_at.elapsed().as_secs_f64();
        if elapsed_seconds < 0.2 {
            return "--".to_string();
        }

        let fps = frames as f64 / elapsed_seconds;
        let kbps = (bytes as f64 * 8.0) / elapsed_seconds / 1_000.0;
        format!("{fps:.1}fps {kbps:.1}kbps")
    }

    fn response_latency_label(&self) -> String {
        if self.connected_at.is_none() {
            return "响应 --".to_string();
        }

        let mut latest_latency_ms: Option<u64> = None;
        let mut sample_count = 0usize;
        let mut latency_sum = 0u128;

        for message in &self.chat_messages {
            if is_handshake_system_message(message) {
                break;
            }

            let Some(response_latency_ms) = message.response_latency_ms.filter(|value| *value > 0)
            else {
                continue;
            };

            if latest_latency_ms.is_none() {
                latest_latency_ms = Some(response_latency_ms);
            }

            if sample_count < RESPONSE_LATENCY_SAMPLE_SIZE {
                latency_sum = latency_sum.saturating_add(response_latency_ms as u128);
                sample_count = sample_count.saturating_add(1);
            }
        }

        let Some(latest_latency_ms) = latest_latency_ms else {
            return "响应 --".to_string();
        };

        if sample_count <= 1 {
            return format!("响应 {}", format_latency_value(latest_latency_ms));
        }

        let average_latency_ms =
            u64::try_from(latency_sum / sample_count as u128).unwrap_or(latest_latency_ms);
        format!(
            "响应 {} (近{} {})",
            format_latency_value(latest_latency_ms),
            sample_count,
            format_latency_value(average_latency_ms)
        )
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
        let has_active_session = self.connected_at.is_some();
        let session_icon_color = if has_active_session {
            0x16d9c0
        } else {
            0x6f7b8f
        };
        let session_text_color = if has_active_session {
            0x16d9c0
        } else {
            0x6f7b8f
        };
        let session_label = self
            .session_uptime_label()
            .map(|uptime| format!("会话 {uptime}"))
            .unwrap_or_else(|| "会话 未连接".to_string());
        let uplink_label = format!(
            "上行 {}",
            self.transport_rate_label(self.uplink_audio_frames, self.uplink_audio_bytes)
        );
        let downlink_label = format!(
            "下行 {}",
            self.transport_rate_label(self.downlink_audio_frames, self.downlink_audio_bytes)
        );
        let response_latency_label = self.response_latency_label();
        let network_rtt_label = self
            .network_rtt_ms
            .map(|rtt_ms| format!("RTT {rtt_ms}ms"))
            .unwrap_or_else(|| "RTT --".to_string());
        let network_rtt_icon_color = if has_active_session && self.network_rtt_ms.is_some() {
            0x16d9c0
        } else {
            0x6f7b8f
        };
        let network_rtt_text_color = if has_active_session && self.network_rtt_ms.is_some() {
            0x16d9c0
        } else {
            0x6f7b8f
        };

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
                    .child(status_metric(
                        IconName::Wifi,
                        session_icon_color,
                        session_text_color,
                        session_label,
                    ))
                    .child(status_metric(
                        IconName::Globe,
                        network_rtt_icon_color,
                        network_rtt_text_color,
                        network_rtt_label,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(ui_icon(IconName::Mic, 12.0, 0x6f7b8f))
                            .child(uplink_label),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(ui_icon(IconName::Volume2, 12.0, 0x6f7b8f))
                            .child(downlink_label),
                    )
                    .children(if has_active_session {
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_color(rgb(0x16d9c0))
                                .child(ui_icon(IconName::Zap, 12.0, 0x16d9c0))
                                .child(response_latency_label),
                        )
                    } else {
                        None
                    })
                    .children(self.aec_stream_delay_ms.map(|delay_ms| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(ui_icon(IconName::Activity, 12.0, 0x6f7b8f))
                            .child(format!("AEC {delay_ms}ms"))
                    })),
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
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .occlude()
                    .child(
                        div()
                            .id("settings-backdrop")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .bg(rgba(0x020712d9)),
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

pub(crate) fn ui_icon(name: IconName, size_px: f32, color_hex: u32) -> gpui::Svg {
    icon(name, size_px, rgb(color_hex))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonIconTone {
    Primary,
    Danger,
    Warning,
    Success,
    Info,
    Neutral,
    Ghost,
    Disabled,
}

pub(crate) fn ui_button_icon(name: IconName, size_px: f32, tone: ButtonIconTone) -> gpui::Svg {
    let color_hex = match tone {
        ButtonIconTone::Primary => 0xe8fffb,
        ButtonIconTone::Danger => 0xffe7ec,
        ButtonIconTone::Warning => 0xfdecc8,
        ButtonIconTone::Success => 0xe2fff9,
        ButtonIconTone::Info => 0xe6f3ff,
        ButtonIconTone::Neutral => 0x9aa6ba,
        ButtonIconTone::Ghost => 0x8a96ab,
        ButtonIconTone::Disabled => 0x556178,
    };

    ui_icon(name, size_px, color_hex)
}

fn status_metric(
    icon_name: IconName,
    icon_color_hex: u32,
    text_color_hex: u32,
    label: impl Into<SharedString>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_color(rgb(text_color_hex))
        .child(ui_icon(icon_name, 12.0, icon_color_hex))
        .child(label.into())
}

fn format_elapsed_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn format_latency_value(latency_ms: u64) -> String {
    if latency_ms < 1_000 {
        format!("{latency_ms}ms")
    } else {
        format!("{:.1}s", latency_ms as f64 / 1_000.0)
    }
}

fn is_handshake_system_message(message: &ChatMessage) -> bool {
    message.role == ChatRole::System
        && message.title.as_ref() == "System"
        && message
            .body
            .as_ref()
            .starts_with("Handshake finished, session_id=")
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

fn env_bool_or_default(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .as_deref()
        .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn prefer_non_empty(preferred: &str, fallback: String) -> String {
    let preferred = preferred.trim();
    if preferred.is_empty() {
        fallback
    } else {
        preferred.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_elapsed_duration, format_latency_value, should_refresh_audio_event_ui,
        AUDIO_EVENT_UI_REFRESH_INTERVAL,
    };
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

    #[test]
    fn elapsed_duration_format_supports_mm_ss_and_hh_mm_ss() {
        assert_eq!(format_elapsed_duration(Duration::from_secs(62)), "01:02");
        assert_eq!(
            format_elapsed_duration(Duration::from_secs(3_661)),
            "01:01:01"
        );
    }

    #[test]
    fn latency_value_format_supports_ms_and_seconds() {
        assert_eq!(format_latency_value(980), "980ms");
        assert_eq!(format_latency_value(1_200), "1.2s");
    }
}
