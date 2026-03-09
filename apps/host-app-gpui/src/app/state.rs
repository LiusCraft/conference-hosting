use gpui::SharedString;
use host_core::InboundTextMessage;

use crate::mcp::McpServerProbeStatus;

pub(crate) const WINDOW_WIDTH: f32 = 1200.0;
pub(crate) const WINDOW_HEIGHT: f32 = 760.0;
pub(crate) const APP_TITLE: &str = "AI Meeting Host v0.1.0-alpha";
pub(crate) const MAX_CHAT_MESSAGES: usize = 240;
pub(crate) const DEFAULT_WS_URL: &str = "wss://xrobo-io.qiniuapi.com/v1/ws/";
pub(crate) const DEFAULT_DEVICE_MAC: &str = "unknown-device";
pub(crate) const DEFAULT_DEVICE_NAME: &str = "host-user";
pub(crate) const DEFAULT_CLIENT_ID: &str = "resvpu932";
pub(crate) const DEFAULT_TOKEN: &str = "your-token1";
pub(crate) const CHAT_BOTTOM_EPSILON_PX: f32 = 2.0;
pub(crate) const TTS_APPEND_REUSE_WINDOW_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    Idle,
    Connecting,
    Connected,
    Disconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatRole {
    System,
    Client,
    User,
    Assistant,
    Tool,
    Trace,
    Error,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatMessage {
    pub(crate) role: ChatRole,
    pub(crate) title: SharedString,
    pub(crate) body: SharedString,
    pub(crate) created_at_unix_ms: u64,
    pub(crate) response_latency_ms: Option<u64>,
    pub(crate) trace_turn_key: Option<SharedString>,
    pub(crate) trace_collapsed: bool,
}

#[derive(Debug)]
pub(crate) enum GatewayCommand {
    Disconnect,
    DetectText(String),
    StartUplinkStream,
    StopUplinkStream,
    SetSpeakerOutputEnabled(bool),
    SetAecEnabled(bool),
    RefreshMcpTools,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AecStatsSnapshot {
    pub(crate) stream_delay_ms: u32,
    pub(crate) capture_callback_delay_ms: u32,
    pub(crate) playback_callback_delay_ms: u32,
    pub(crate) playback_buffer_delay_ms: u32,
    pub(crate) processor_delay_ms: i32,
    pub(crate) erl_db: f32,
    pub(crate) erle_db: f32,
}

#[derive(Debug)]
pub(crate) enum UiGatewayEvent {
    Connected { session_id: String },
    Disconnected,
    SystemNotice(String),
    Error(String),
    OutgoingText { kind: String, payload: String },
    IncomingText(InboundTextMessage),
    UplinkAudioFrameSent(usize),
    UplinkStreamStateChanged(bool),
    DownlinkAudioFrameReceived(usize),
    NetworkRttUpdated(u32),
    AecStateChanged(bool),
    AecStats(AecStatsSnapshot),
    McpProbeStatuses(Vec<McpServerProbeStatus>),
}

#[derive(Debug, Clone)]
pub(crate) struct AudioRoutingConfig {
    pub(crate) input_device_name: Option<String>,
    pub(crate) input_from_output: bool,
    pub(crate) output_device_name: Option<String>,
    pub(crate) speaker_output_enabled: bool,
    pub(crate) aec_enabled: bool,
}

impl AudioRoutingConfig {
    pub(crate) fn input_label(&self) -> String {
        let label = self.input_device_name.as_deref().unwrap_or("default");
        if self.input_from_output {
            format!("loopback:{label}")
        } else {
            label.to_string()
        }
    }

    pub(crate) fn output_label(&self) -> &str {
        self.output_device_name.as_deref().unwrap_or("default")
    }
}
