use host_platform::WsGatewayConfig;

use crate::app::state::{
    DEFAULT_CLIENT_ID, DEFAULT_DEVICE_MAC, DEFAULT_DEVICE_NAME, DEFAULT_TOKEN, DEFAULT_WS_URL,
};

pub(crate) fn build_gateway_config(server_url_override: Option<&str>) -> WsGatewayConfig {
    let device_mac = env_or_default("HOST_DEVICE_MAC", DEFAULT_DEVICE_MAC);
    let device_id = env_or_default("HOST_DEVICE_ID", &device_mac);
    let server_url = server_url_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| env_or_default("HOST_WS_URL", DEFAULT_WS_URL));

    WsGatewayConfig::new(
        server_url,
        device_id,
        env_or_default("HOST_DEVICE_NAME", DEFAULT_DEVICE_NAME),
        device_mac,
        env_or_default("HOST_CLIENT_ID", DEFAULT_CLIENT_ID),
        env_or_default("HOST_TOKEN", DEFAULT_TOKEN),
    )
}

pub(crate) fn env_or_default(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
