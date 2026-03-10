use host_platform::WsGatewayConfig;
use mac_address::get_mac_address;
use uuid::Uuid;

use crate::app::state::{DEFAULT_TOKEN, DEFAULT_WS_URL};

const DEFAULT_DEVICE_MAC_FALLBACK: &str = "unknown-device";
const DEFAULT_DEVICE_NAME_FALLBACK: &str = "host-user";

pub(crate) fn build_gateway_config(
    server_url_override: Option<&str>,
    device_id_override: Option<&str>,
    client_id_override: Option<&str>,
    token_override: Option<&str>,
    mcp_enabled: bool,
) -> WsGatewayConfig {
    let device_mac = env_or_default("HOST_DEVICE_MAC", &default_device_mac());
    let device_id = resolve_override_or_env(device_id_override, "HOST_DEVICE_ID", &device_mac);
    let generated_client_id = default_client_id();
    let client_id =
        resolve_override_or_env(client_id_override, "HOST_CLIENT_ID", &generated_client_id);
    let token = resolve_override_or_env(token_override, "HOST_TOKEN", DEFAULT_TOKEN);
    let server_url = resolve_override_or_env(server_url_override, "HOST_WS_URL", DEFAULT_WS_URL);
    let device_name = env_or_default("HOST_DEVICE_NAME", &default_device_name());

    WsGatewayConfig::new(
        server_url,
        device_id,
        device_name,
        device_mac,
        client_id,
        token,
    )
    .with_mcp_feature(mcp_enabled)
}

pub(crate) fn default_device_mac() -> String {
    get_mac_address()
        .ok()
        .flatten()
        .map(|mac| {
            mac.to_string()
                .replace(':', "")
                .replace('-', "")
                .to_ascii_lowercase()
        })
        .filter(|mac| !mac.is_empty())
        .unwrap_or_else(|| DEFAULT_DEVICE_MAC_FALLBACK.to_string())
}

pub(crate) fn default_device_name() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_DEVICE_NAME_FALLBACK.to_string())
}

pub(crate) fn default_client_id() -> String {
    Uuid::new_v4().simple().to_string()
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

#[cfg(test)]
mod tests {
    use super::default_client_id;

    #[test]
    fn default_client_id_is_alphanumeric_uuid_without_symbols() {
        let client_id = default_client_id();

        assert_eq!(client_id.len(), 32);
        assert!(client_id.chars().all(|ch| ch.is_ascii_alphanumeric()));
    }
}
