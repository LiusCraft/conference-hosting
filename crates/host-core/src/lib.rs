pub mod protocol;

pub use protocol::{
    ClientTextMessage, HelloFeatures, HelloMessage, HelloNotifyFeatures, InboundTextMessage,
    ListenMessage, ListenMode, ListenState, AUDIO_CHANNELS_MONO, AUDIO_FRAME_BYTES_PCM16_MONO,
    AUDIO_FRAME_DURATION_MS, AUDIO_FRAME_SAMPLES, AUDIO_PCM16_BYTES_PER_SAMPLE,
    AUDIO_SAMPLE_RATE_HZ,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayStatus {
    Idle,
    Connected,
}

impl GatewayStatus {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Connected => "Connected",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Idle => Self::Connected,
            Self::Connected => Self::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GatewayStatus;

    #[test]
    fn gateway_status_toggle_switches_between_states() {
        assert_eq!(GatewayStatus::Idle.toggle(), GatewayStatus::Connected);
        assert_eq!(GatewayStatus::Connected.toggle(), GatewayStatus::Idle);
    }

    #[test]
    fn gateway_status_label_is_stable() {
        assert_eq!(GatewayStatus::Idle.as_label(), "Idle");
        assert_eq!(GatewayStatus::Connected.as_label(), "Connected");
    }
}
