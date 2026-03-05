use host_core::GatewayStatus;

mod ws_gateway;

pub use ws_gateway::{WsGatewayClient, WsGatewayConfig, WsGatewayError, WsGatewayEvent};

#[derive(Debug, Clone)]
pub struct PlatformAdapter {
    status: GatewayStatus,
}

impl Default for PlatformAdapter {
    fn default() -> Self {
        Self {
            status: GatewayStatus::Idle,
        }
    }
}

impl PlatformAdapter {
    pub fn status(&self) -> GatewayStatus {
        self.status
    }

    pub fn toggle_connection(&mut self) -> GatewayStatus {
        self.status = self.status.toggle();
        self.status
    }
}

#[cfg(test)]
mod tests {
    use host_core::GatewayStatus;

    use super::PlatformAdapter;

    #[test]
    fn adapter_toggles_gateway_status() {
        let mut adapter = PlatformAdapter::default();

        assert_eq!(adapter.status(), GatewayStatus::Idle);
        assert_eq!(adapter.toggle_connection(), GatewayStatus::Connected);
        assert_eq!(adapter.toggle_connection(), GatewayStatus::Idle);
    }
}
