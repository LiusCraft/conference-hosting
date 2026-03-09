use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const AUDIO_SAMPLE_RATE_HZ: u32 = 16_000;
pub const AUDIO_CHANNELS_MONO: u8 = 1;
pub const AUDIO_FRAME_DURATION_MS: u16 = 20;
pub const AUDIO_PCM16_BYTES_PER_SAMPLE: usize = 2;
pub const AUDIO_FRAME_SAMPLES: usize =
    (AUDIO_SAMPLE_RATE_HZ as usize * AUDIO_FRAME_DURATION_MS as usize) / 1_000;
pub const AUDIO_FRAME_BYTES_PCM16_MONO: usize = AUDIO_FRAME_SAMPLES * AUDIO_PCM16_BYTES_PER_SAMPLE;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientTextMessage {
    Hello(HelloMessage),
    Listen(ListenMessage),
    Mcp(Box<McpEnvelopeMessage>),
}

impl ClientTextMessage {
    pub fn hello(message: HelloMessage) -> Self {
        Self::Hello(message)
    }

    pub fn listen_start() -> Self {
        Self::Listen(ListenMessage::start())
    }

    pub fn listen_stop() -> Self {
        Self::Listen(ListenMessage::stop())
    }

    pub fn listen_detect_text(text: impl Into<String>) -> Self {
        Self::Listen(ListenMessage::detect_text(text))
    }

    pub fn mcp(message: McpEnvelopeMessage) -> Self {
        Self::Mcp(Box::new(message))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloMessage {
    pub device_id: String,
    pub device_name: String,
    pub device_mac: String,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<HelloFeatures>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HelloFeatures {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<HelloNotifyFeatures>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloNotifyFeatures {
    pub intent_trace: bool,
}

impl HelloMessage {
    pub fn new(
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        device_mac: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            device_name: device_name.into(),
            device_mac: device_mac.into(),
            token: token.into(),
            features: None,
        }
    }

    pub fn with_intent_trace_notify(mut self, enabled: bool) -> Self {
        let mut features = self.features.unwrap_or_default();
        features.notify = Some(HelloNotifyFeatures {
            intent_trace: enabled,
        });
        self.features = Some(features);
        self
    }

    pub fn with_mcp(mut self, enabled: bool) -> Self {
        let mut features = self.features.unwrap_or_default();
        features.mcp = Some(enabled);
        self.features = Some(features);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpEnvelopeMessage {
    pub session_id: String,
    pub payload: JsonRpcMessage,
}

impl McpEnvelopeMessage {
    pub fn new(session_id: impl Into<String>, payload: JsonRpcMessage) -> Self {
        Self {
            session_id: session_id.into(),
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
}

impl JsonRpcMessage {
    pub fn request(method: impl Into<String>, params: Option<Value>, id: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: Some(method.into()),
            params,
            result: None,
            error: None,
            id,
        }
    }

    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: None,
            params: None,
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn failure(
        id: Option<Value>,
        code: i64,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: None,
            params: None,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data,
            }),
            id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListenMessage {
    pub mode: ListenMode,
    pub state: ListenState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl ListenMessage {
    pub fn start() -> Self {
        Self {
            mode: ListenMode::Manual,
            state: ListenState::Start,
            text: None,
        }
    }

    pub fn stop() -> Self {
        Self {
            mode: ListenMode::Manual,
            state: ListenState::Stop,
            text: None,
        }
    }

    pub fn detect_text(text: impl Into<String>) -> Self {
        Self {
            mode: ListenMode::Manual,
            state: ListenState::Detect,
            text: Some(text.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ListenMode {
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ListenState {
    Start,
    Stop,
    Detect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboundTextMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(flatten)]
    pub payload: Map<String, Value>,
}

impl InboundTextMessage {
    pub fn session_id(&self) -> Option<&str> {
        self.payload.get("session_id").and_then(Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClientTextMessage, HelloMessage, InboundTextMessage, JsonRpcMessage, McpEnvelopeMessage,
        AUDIO_FRAME_BYTES_PCM16_MONO, AUDIO_FRAME_SAMPLES,
    };
    use serde_json::json;

    #[test]
    fn frame_size_constants_match_16k_20ms_pcm16_mono() {
        assert_eq!(AUDIO_FRAME_SAMPLES, 320);
        assert_eq!(AUDIO_FRAME_BYTES_PCM16_MONO, 640);
    }

    #[test]
    fn hello_message_serializes_to_expected_shape() {
        let message = ClientTextMessage::hello(HelloMessage::new(
            "device-001",
            "host-desktop",
            "AA:BB:CC:DD:EE:FF",
            "token-demo",
        ));
        let value = serde_json::to_value(message).expect("serialize hello");

        assert_eq!(value["type"], "hello");
        assert_eq!(value["device_id"], "device-001");
        assert_eq!(value["device_name"], "host-desktop");
        assert_eq!(value["device_mac"], "AA:BB:CC:DD:EE:FF");
        assert_eq!(value["token"], "token-demo");
        assert!(value.get("features").is_none());
    }

    #[test]
    fn hello_message_supports_intent_trace_notify_feature() {
        let message = ClientTextMessage::hello(
            HelloMessage::new(
                "device-001",
                "host-desktop",
                "AA:BB:CC:DD:EE:FF",
                "token-demo",
            )
            .with_intent_trace_notify(true),
        );
        let value = serde_json::to_value(message).expect("serialize hello with features");

        assert_eq!(value["type"], "hello");
        assert_eq!(value["features"]["notify"]["intent_trace"], true);
    }

    #[test]
    fn hello_message_supports_multiple_features() {
        let message = ClientTextMessage::hello(
            HelloMessage::new(
                "device-001",
                "host-desktop",
                "AA:BB:CC:DD:EE:FF",
                "token-demo",
            )
            .with_intent_trace_notify(true)
            .with_mcp(true),
        );
        let value = serde_json::to_value(message).expect("serialize hello with multi-features");

        assert_eq!(value["type"], "hello");
        assert_eq!(value["features"]["notify"]["intent_trace"], true);
        assert_eq!(value["features"]["mcp"], true);
    }

    #[test]
    fn mcp_message_serializes_to_expected_shape() {
        let message = ClientTextMessage::mcp(McpEnvelopeMessage::new(
            "session-1",
            JsonRpcMessage::request("tools/list", Some(json!({ "cursor": "" })), Some(json!(2))),
        ));
        let value = serde_json::to_value(message).expect("serialize mcp message");

        assert_eq!(value["type"], "mcp");
        assert_eq!(value["session_id"], "session-1");
        assert_eq!(value["payload"]["jsonrpc"], "2.0");
        assert_eq!(value["payload"]["method"], "tools/list");
        assert_eq!(value["payload"]["params"]["cursor"], "");
        assert_eq!(value["payload"]["id"], 2);
    }

    #[test]
    fn listen_detect_text_includes_text_payload() {
        let message = ClientTextMessage::listen_detect_text("hello");
        let value = serde_json::to_value(message).expect("serialize listen detect");

        assert_eq!(value["type"], "listen");
        assert_eq!(value["mode"], "manual");
        assert_eq!(value["state"], "detect");
        assert_eq!(value["text"], "hello");
    }

    #[test]
    fn inbound_message_extracts_session_id() {
        let message: InboundTextMessage = serde_json::from_value(serde_json::json!({
            "type": "hello",
            "session_id": "session-123",
            "transport": "websocket"
        }))
        .expect("parse inbound message");

        assert_eq!(message.message_type, "hello");
        assert_eq!(message.session_id(), Some("session-123"));
    }
}
