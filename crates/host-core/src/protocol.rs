use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const AUDIO_SAMPLE_RATE_HZ: u32 = 16_000;
pub const AUDIO_CHANNELS_MONO: u8 = 1;
pub const AUDIO_FRAME_DURATION_MS: u16 = 20;
pub const AUDIO_PCM16_BYTES_PER_SAMPLE: usize = 2;
pub const AUDIO_FRAME_SAMPLES: usize =
    (AUDIO_SAMPLE_RATE_HZ as usize * AUDIO_FRAME_DURATION_MS as usize) / 1_000;
pub const AUDIO_FRAME_BYTES_PCM16_MONO: usize = AUDIO_FRAME_SAMPLES * AUDIO_PCM16_BYTES_PER_SAMPLE;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientTextMessage {
    Hello(HelloMessage),
    Listen(ListenMessage),
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloMessage {
    pub device_id: String,
    pub device_name: String,
    pub device_mac: String,
    pub token: String,
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
        }
    }
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
        ClientTextMessage, HelloMessage, InboundTextMessage, AUDIO_FRAME_BYTES_PCM16_MONO,
        AUDIO_FRAME_SAMPLES,
    };

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
