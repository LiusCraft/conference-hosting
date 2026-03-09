use host_platform::WsGatewayConfig;

use crate::app::state::{
    DEFAULT_CLIENT_ID, DEFAULT_DEVICE_MAC, DEFAULT_DEVICE_NAME, DEFAULT_TOKEN, DEFAULT_WS_URL,
};

pub(crate) fn build_gateway_config(
    server_url_override: Option<&str>,
    device_id_override: Option<&str>,
    client_id_override: Option<&str>,
    token_override: Option<&str>,
) -> WsGatewayConfig {
    let device_mac = env_or_default("HOST_DEVICE_MAC", DEFAULT_DEVICE_MAC);
    let device_id = resolve_override_or_env(device_id_override, "HOST_DEVICE_ID", &device_mac);
    let client_id =
        resolve_override_or_env(client_id_override, "HOST_CLIENT_ID", DEFAULT_CLIENT_ID);
    let token = resolve_override_or_env(token_override, "HOST_TOKEN", DEFAULT_TOKEN);
    let server_url = resolve_override_or_env(server_url_override, "HOST_WS_URL", DEFAULT_WS_URL);

    WsGatewayConfig::new(
        server_url,
        device_id,
        env_or_default("HOST_DEVICE_NAME", DEFAULT_DEVICE_NAME),
        device_mac,
        client_id,
        token,
    )
}

fn resolve_override_or_env(override_value: Option<&str>, env_key: &str, fallback: &str) -> String {
    override_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| env_or_default(env_key, fallback))
}

pub(crate) fn env_or_default(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
