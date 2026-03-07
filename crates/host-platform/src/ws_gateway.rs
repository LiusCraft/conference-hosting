use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use host_core::{ClientTextMessage, HelloMessage, InboundTextMessage};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;

type WsSink = futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

#[derive(Debug, Clone)]
pub struct WsGatewayConfig {
    pub server_url: String,
    pub device_id: String,
    pub device_name: String,
    pub device_mac: String,
    pub client_id: String,
    pub token: String,
    pub hello_timeout: Duration,
}

impl WsGatewayConfig {
    pub fn new(
        server_url: impl Into<String>,
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        device_mac: impl Into<String>,
        client_id: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            server_url: server_url.into(),
            device_id: device_id.into(),
            device_name: device_name.into(),
            device_mac: device_mac.into(),
            client_id: client_id.into(),
            token: token.into(),
            hello_timeout: Duration::from_secs(5),
        }
    }

    pub fn with_hello_timeout(mut self, timeout: Duration) -> Self {
        self.hello_timeout = timeout;
        self
    }

    fn hello_message(&self) -> ClientTextMessage {
        ClientTextMessage::hello(
            HelloMessage::new(
                self.device_id.clone(),
                self.device_name.clone(),
                self.device_mac.clone(),
                self.token.clone(),
            )
            .with_intent_trace_notify(true),
        )
    }

    fn websocket_url(&self) -> Result<Url, WsGatewayError> {
        let mut url =
            Url::parse(&self.server_url).map_err(|source| WsGatewayError::InvalidUrl {
                url: self.server_url.clone(),
                source,
            })?;

        let existing_pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();

        {
            let mut query = url.query_pairs_mut();
            query.clear();

            for (key, value) in existing_pairs {
                if key == "device-id" || key == "client-id" {
                    continue;
                }

                query.append_pair(&key, &value);
            }

            query
                .append_pair("device-id", &self.device_id)
                .append_pair("client-id", &self.client_id);
        }

        Ok(url)
    }
}

#[derive(Debug)]
pub enum WsGatewayEvent {
    Text(InboundTextMessage),
    DownlinkAudio(Vec<u8>),
    MalformedText { raw: String, error: String },
    Closed,
    TransportError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum WsGatewayError {
    #[error("invalid websocket url `{url}`: {source}")]
    InvalidUrl {
        url: String,
        source: url::ParseError,
    },
    #[error("websocket transport error: {0}")]
    WebSocket(Box<tungstenite::Error>),
    #[error("json serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("hello handshake timed out after {0:?}")]
    HelloTimeout(Duration),
    #[error("hello handshake failed: {0}")]
    HelloHandshake(String),
}

impl From<tungstenite::Error> for WsGatewayError {
    fn from(value: tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(value))
    }
}

pub struct WsGatewayClient {
    session_id: String,
    sink: Arc<Mutex<WsSink>>,
    event_rx: mpsc::UnboundedReceiver<WsGatewayEvent>,
    _reader_task: JoinHandle<()>,
}

impl WsGatewayClient {
    pub async fn connect(config: WsGatewayConfig) -> Result<Self, WsGatewayError> {
        let url = config.websocket_url()?;
        let (mut socket, _) = connect_async(url.as_str()).await?;

        let hello_text = serde_json::to_string(&config.hello_message())?;
        socket.send(Message::Text(hello_text.into())).await?;

        let mut early_events = Vec::new();
        let session_id = time::timeout(
            config.hello_timeout,
            wait_for_hello(&mut socket, &mut early_events),
        )
        .await
        .map_err(|_| WsGatewayError::HelloTimeout(config.hello_timeout))??;

        let (sink, mut stream) = socket.split();
        let sink = Arc::new(Mutex::new(sink));
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        for event in early_events {
            let _ = event_tx.send(event);
        }

        let reader_task = tokio::spawn(async move {
            while let Some(incoming) = stream.next().await {
                match incoming {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<InboundTextMessage>(&text) {
                            Ok(message) => {
                                let _ = event_tx.send(WsGatewayEvent::Text(message));
                            }
                            Err(error) => {
                                let _ = event_tx.send(WsGatewayEvent::MalformedText {
                                    raw: text.to_string(),
                                    error: error.to_string(),
                                });
                            }
                        }
                    }
                    Ok(Message::Binary(binary)) => {
                        let _ = event_tx.send(WsGatewayEvent::DownlinkAudio(binary.to_vec()));
                    }
                    Ok(Message::Close(_)) => {
                        let _ = event_tx.send(WsGatewayEvent::Closed);
                        break;
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
                    Err(error) => {
                        let _ = event_tx.send(WsGatewayEvent::TransportError(error.to_string()));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            session_id,
            sink,
            event_rx,
            _reader_task: reader_task,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub async fn send_listen_start(&self) -> Result<(), WsGatewayError> {
        self.send_text(ClientTextMessage::listen_start()).await
    }

    pub async fn send_listen_stop(&self) -> Result<(), WsGatewayError> {
        self.send_text(ClientTextMessage::listen_stop()).await
    }

    pub async fn send_listen_detect_text(
        &self,
        text: impl Into<String>,
    ) -> Result<(), WsGatewayError> {
        self.send_text(ClientTextMessage::listen_detect_text(text))
            .await
    }

    pub async fn send_audio_frame(&self, frame: Vec<u8>) -> Result<(), WsGatewayError> {
        let mut sink = self.sink.lock().await;
        sink.send(Message::Binary(frame.into())).await?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Option<WsGatewayEvent> {
        self.event_rx.recv().await
    }

    async fn send_text(&self, message: ClientTextMessage) -> Result<(), WsGatewayError> {
        let payload = serde_json::to_string(&message)?;
        let mut sink = self.sink.lock().await;
        sink.send(Message::Text(payload.into())).await?;
        Ok(())
    }
}

async fn wait_for_hello(
    socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    early_events: &mut Vec<WsGatewayEvent>,
) -> Result<String, WsGatewayError> {
    while let Some(incoming) = socket.next().await {
        match incoming {
            Ok(Message::Text(text)) => match serde_json::from_str::<InboundTextMessage>(&text) {
                Ok(message) => {
                    if message.message_type == "hello" {
                        return message.session_id().map(ToOwned::to_owned).ok_or_else(|| {
                            WsGatewayError::HelloHandshake(
                                "server hello missing `session_id`".to_string(),
                            )
                        });
                    }
                    early_events.push(WsGatewayEvent::Text(message));
                }
                Err(error) => {
                    early_events.push(WsGatewayEvent::MalformedText {
                        raw: text.to_string(),
                        error: error.to_string(),
                    });
                }
            },
            Ok(Message::Binary(binary)) => {
                early_events.push(WsGatewayEvent::DownlinkAudio(binary.to_vec()));
            }
            Ok(Message::Close(frame)) => {
                let reason = frame
                    .map(|close| close.reason.to_string())
                    .unwrap_or_else(|| "server closed connection".to_string());
                return Err(WsGatewayError::HelloHandshake(reason));
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
            Err(error) => return Err(error.into()),
        }
    }

    Err(WsGatewayError::HelloHandshake(
        "connection closed before hello response".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio::time;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    use super::{WsGatewayClient, WsGatewayConfig, WsGatewayError, WsGatewayEvent};

    fn expect_text(message: Message) -> String {
        match message {
            Message::Text(text) => text.to_string(),
            other => panic!("expected text message, got {other:?}"),
        }
    }

    fn expect_binary(message: Message) -> Vec<u8> {
        match message {
            Message::Binary(binary) => binary.to_vec(),
            other => panic!("expected binary message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ws_gateway_hello_and_audio_roundtrip_works() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("read local addr");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept socket");
            let mut socket = accept_async(stream).await.expect("upgrade websocket");

            let hello_raw = expect_text(
                socket
                    .next()
                    .await
                    .expect("hello frame exists")
                    .expect("hello frame valid"),
            );
            let hello: serde_json::Value =
                serde_json::from_str(&hello_raw).expect("parse hello payload");
            assert_eq!(hello["type"], "hello");
            assert_eq!(hello["device_id"], "dev-001");
            assert_eq!(hello["device_mac"], "AA:BB:CC:DD:EE:FF");
            assert_eq!(hello["features"]["notify"]["intent_trace"], true);

            socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "hello",
                        "session_id": "session-001"
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("send hello ack");

            let listen_start_raw = expect_text(
                socket
                    .next()
                    .await
                    .expect("listen start frame exists")
                    .expect("listen start frame valid"),
            );
            let listen_start: serde_json::Value =
                serde_json::from_str(&listen_start_raw).expect("parse listen start");
            assert_eq!(listen_start["type"], "listen");
            assert_eq!(listen_start["mode"], "manual");
            assert_eq!(listen_start["state"], "start");

            let upstream_audio = expect_binary(
                socket
                    .next()
                    .await
                    .expect("upstream audio frame exists")
                    .expect("upstream audio frame valid"),
            );
            assert_eq!(upstream_audio, vec![1, 2, 3, 4]);

            socket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "stt",
                        "text": "received"
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("send stt message");
            socket
                .send(Message::Binary(vec![9, 8, 7].into()))
                .await
                .expect("send downlink audio");

            let listen_stop_raw = expect_text(
                socket
                    .next()
                    .await
                    .expect("listen stop frame exists")
                    .expect("listen stop frame valid"),
            );
            let listen_stop: serde_json::Value =
                serde_json::from_str(&listen_stop_raw).expect("parse listen stop");
            assert_eq!(listen_stop["type"], "listen");
            assert_eq!(listen_stop["state"], "stop");
        });

        let config = WsGatewayConfig::new(
            format!("ws://{addr}/xiaozhi/v1/"),
            "dev-001",
            "Host Desktop",
            "AA:BB:CC:DD:EE:FF",
            "host-client",
            "token-demo",
        );

        let mut gateway = WsGatewayClient::connect(config)
            .await
            .expect("connect gateway client");
        assert_eq!(gateway.session_id(), "session-001");

        gateway
            .send_listen_start()
            .await
            .expect("send listen start");
        gateway
            .send_audio_frame(vec![1, 2, 3, 4])
            .await
            .expect("send upstream audio");

        let first_event = time::timeout(Duration::from_secs(1), gateway.next_event())
            .await
            .expect("wait first event")
            .expect("first event exists");
        match first_event {
            WsGatewayEvent::Text(message) => {
                assert_eq!(message.message_type, "stt");
                assert_eq!(message.payload["text"], "received");
            }
            other => panic!("expected stt text event, got {other:?}"),
        }

        let second_event = time::timeout(Duration::from_secs(1), gateway.next_event())
            .await
            .expect("wait second event")
            .expect("second event exists");
        match second_event {
            WsGatewayEvent::DownlinkAudio(data) => {
                assert_eq!(data, vec![9, 8, 7]);
            }
            other => panic!("expected downlink audio event, got {other:?}"),
        }

        gateway.send_listen_stop().await.expect("send listen stop");
        drop(gateway);

        server.await.expect("mock server task finished");
    }

    #[tokio::test]
    async fn ws_gateway_connect_times_out_without_hello_response() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("read local addr");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept socket");
            let mut socket = accept_async(stream).await.expect("upgrade websocket");

            let _ = socket.next().await;
            time::sleep(Duration::from_millis(300)).await;
        });

        let config = WsGatewayConfig::new(
            format!("ws://{addr}/xiaozhi/v1/"),
            "dev-001",
            "Host Desktop",
            "AA:BB:CC:DD:EE:FF",
            "host-client",
            "token-demo",
        )
        .with_hello_timeout(Duration::from_millis(100));

        let error = match WsGatewayClient::connect(config).await {
            Ok(_) => panic!("connect should fail with timeout"),
            Err(error) => error,
        };

        match error {
            WsGatewayError::HelloTimeout(timeout) => {
                assert_eq!(timeout, Duration::from_millis(100));
            }
            other => panic!("expected hello timeout, got {other:?}"),
        }

        server.await.expect("mock server task finished");
    }

    #[test]
    fn websocket_url_replaces_existing_device_and_client_query_values() {
        let config = WsGatewayConfig::new(
            "ws://127.0.0.1:8000/xiaozhi/v1/?foo=bar&device-id=old-device&client-id=old-client",
            "new-device",
            "Host Desktop",
            "AA:BB:CC:DD:EE:FF",
            "new-client",
            "token-demo",
        );

        let url = config.websocket_url().expect("build websocket url");
        let query_pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();

        assert!(query_pairs.contains(&("foo".to_string(), "bar".to_string())));
        assert!(query_pairs.contains(&("device-id".to_string(), "new-device".to_string())));
        assert!(query_pairs.contains(&("client-id".to_string(), "new-client".to_string())));
        assert_eq!(
            query_pairs
                .iter()
                .filter(|(key, _)| key == "device-id")
                .count(),
            1
        );
        assert_eq!(
            query_pairs
                .iter()
                .filter(|(key, _)| key == "client-id")
                .count(),
            1
        );
    }
}
