use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use host_core::{InboundTextMessage, JsonRpcMessage, McpEnvelopeMessage};
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::time;
use url::Url;

pub(crate) const DEFAULT_MCP_REQUEST_TIMEOUT_MS: u64 = 8_000;
pub(crate) const DEFAULT_MCP_CONNECT_TIMEOUT_MS: u64 = 3_000;

const MAX_TOOLS_PER_SERVER: usize = 16;
const MAX_TOTAL_TOOLS: usize = 128;
const JSONRPC_INVALID_REQUEST: i64 = -32_600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32_601;
const JSONRPC_INVALID_PARAMS: i64 = -32_602;
const JSONRPC_INTERNAL_ERROR: i64 = -32_603;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum McpTransportKind {
    Stdio,
    Sse,
    Stream,
}

impl McpTransportKind {
    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Sse => "sse",
            Self::Stream => "stream",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "transport", rename_all = "lowercase")]
pub(crate) enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Sse {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
    },
    Stream {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<String>,
    },
}

impl McpTransportConfig {
    pub(crate) fn kind(&self) -> McpTransportKind {
        match self {
            Self::Stdio { .. } => McpTransportKind::Stdio,
            Self::Sse { .. } => McpTransportKind::Sse,
            Self::Stream { .. } => McpTransportKind::Stream,
        }
    }

    fn endpoint_summary(&self) -> String {
        match self {
            Self::Stdio { command, .. } => command.to_string(),
            Self::Sse { url, .. } | Self::Stream { url, .. } => url.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct McpServerConfig {
    pub(crate) id: String,
    pub(crate) alias: String,
    pub(crate) enabled: bool,
    pub(crate) transport: McpTransportConfig,
    pub(crate) request_timeout_ms: u64,
    pub(crate) connect_timeout_ms: u64,
}

impl McpServerConfig {
    pub(crate) fn transport_kind(&self) -> McpTransportKind {
        self.transport.kind()
    }

    pub(crate) fn endpoint_summary(&self) -> String {
        self.transport.endpoint_summary()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpProbeState {
    Unknown,
    Success,
    Failed,
}

impl McpProbeState {
    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Unknown => "未探测",
            Self::Success => "成功",
            Self::Failed => "失败",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct McpServerProbeStatus {
    pub(crate) server_id: String,
    pub(crate) alias: String,
    pub(crate) state: McpProbeState,
    pub(crate) detail: String,
    pub(crate) tool_count: usize,
    pub(crate) tools: Vec<McpToolSummary>,
}

#[derive(Debug, Clone)]
pub(crate) struct McpToolSummary {
    pub(crate) name: String,
    pub(crate) description: String,
}

#[derive(Debug, Clone)]
pub(crate) struct McpProbeSnapshot {
    pub(crate) statuses: Vec<McpServerProbeStatus>,
    pub(crate) tool_count: usize,
}

#[derive(Debug, Clone)]
struct McpToolDescriptor {
    public_name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Clone)]
struct ToolRoute {
    server_id: String,
    alias: String,
    origin_tool_name: String,
}

pub(crate) fn new_mcp_server_id(alias: &str) -> String {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let normalized_alias = alias
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    let alias_prefix = if normalized_alias.is_empty() {
        "server"
    } else {
        normalized_alias.as_str()
    };

    format!("mcp-{alias_prefix}-{timestamp_ms}")
}

pub(crate) fn normalize_server_config(mut config: McpServerConfig) -> McpServerConfig {
    config.id = config.id.trim().to_string();
    config.alias = config.alias.trim().to_string();
    config.request_timeout_ms = config.request_timeout_ms.max(100);
    config.connect_timeout_ms = config.connect_timeout_ms.max(100);

    config.transport = match config.transport {
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            cwd,
        } => McpTransportConfig::Stdio {
            command: command.trim().to_string(),
            args: args
                .into_iter()
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect(),
            env: sanitize_kv_map(env),
            cwd: cwd
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        },
        McpTransportConfig::Sse { url, headers } => McpTransportConfig::Sse {
            url: url.trim().to_string(),
            headers: sanitize_kv_map(headers),
        },
        McpTransportConfig::Stream { url, headers, auth } => McpTransportConfig::Stream {
            url: url.trim().to_string(),
            headers: sanitize_kv_map(headers),
            auth: auth
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        },
    };

    config
}

pub(crate) fn validate_server_config(
    config: &McpServerConfig,
    existing_servers: &[McpServerConfig],
    editing_server_id: Option<&str>,
) -> Result<(), String> {
    let alias = config.alias.trim();
    if alias.is_empty() {
        return Err("alias 不能为空".to_string());
    }

    let alias_conflict = existing_servers.iter().any(|server| {
        if let Some(editing_id) = editing_server_id {
            if server.id == editing_id {
                return false;
            }
        }

        server.alias.eq_ignore_ascii_case(alias)
    });
    if alias_conflict {
        return Err(format!("alias `{alias}` 已存在"));
    }

    if config.request_timeout_ms == 0 {
        return Err("request_timeout_ms 必须大于 0".to_string());
    }
    if config.connect_timeout_ms == 0 {
        return Err("connect_timeout_ms 必须大于 0".to_string());
    }

    validate_transport_config(&config.transport)
}

fn validate_transport_config(transport: &McpTransportConfig) -> Result<(), String> {
    match transport {
        McpTransportConfig::Stdio { command, .. } => {
            if command.trim().is_empty() {
                return Err("stdio.command 不能为空".to_string());
            }
        }
        McpTransportConfig::Sse { url, .. } | McpTransportConfig::Stream { url, .. } => {
            if url.trim().is_empty() {
                return Err("url 不能为空".to_string());
            }
            Url::parse(url.trim()).map_err(|error| format!("URL 不合法: {error}"))?;
        }
    }

    Ok(())
}

fn sanitize_kv_map(source: BTreeMap<String, String>) -> BTreeMap<String, String> {
    source
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect()
}

pub(crate) struct McpBridge {
    servers: Vec<McpServerConfig>,
    tools: Vec<McpToolDescriptor>,
    routes: HashMap<String, ToolRoute>,
    upstream_clients: HashMap<String, UpstreamClient>,
    probe_statuses: Vec<McpServerProbeStatus>,
}

type UpstreamService = RunningService<RoleClient, ()>;

struct UpstreamClient {
    request_timeout_ms: u64,
    transport_kind: McpTransportKind,
    service: UpstreamService,
}

struct DiscoveredTool {
    origin_name: String,
    description: String,
    input_schema: Value,
}

pub(crate) fn preview_probe_statuses(servers: &[McpServerConfig]) -> Vec<McpServerProbeStatus> {
    servers
        .iter()
        .map(|server| {
            if !server.enabled {
                return McpServerProbeStatus {
                    server_id: server.id.clone(),
                    alias: server.alias.clone(),
                    state: McpProbeState::Unknown,
                    detail: "已禁用".to_string(),
                    tool_count: 0,
                    tools: Vec::new(),
                };
            }

            match validate_server_config(server, servers, Some(&server.id)) {
                Ok(()) => McpServerProbeStatus {
                    server_id: server.id.clone(),
                    alias: server.alias.clone(),
                    state: McpProbeState::Unknown,
                    detail: "待连接后探测".to_string(),
                    tool_count: 0,
                    tools: Vec::new(),
                },
                Err(error) => McpServerProbeStatus {
                    server_id: server.id.clone(),
                    alias: server.alias.clone(),
                    state: McpProbeState::Failed,
                    detail: error,
                    tool_count: 0,
                    tools: Vec::new(),
                },
            }
        })
        .collect()
}

fn log_mcp_error(alias: &str, stage: &str, detail: &str) {
    eprintln!("[mcp][error] alias={alias} stage={stage} detail={detail}");
}

fn log_mcp_info(alias: &str, stage: &str, detail: &str) {
    eprintln!("[mcp][info] alias={alias} stage={stage} detail={detail}");
}

pub(crate) fn probe_servers_with_dedicated_runtime(
    servers: Vec<McpServerConfig>,
) -> Result<McpProbeSnapshot, String> {
    let runtime = TokioRuntimeBuilder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("build tokio runtime failed: {error}"))?;

    let snapshot = runtime.block_on(async move {
        let mut bridge = McpBridge::new(servers);
        bridge.refresh_tools().await;
        McpProbeSnapshot {
            statuses: bridge.probe_statuses().to_vec(),
            tool_count: bridge.tool_count(),
        }
    });

    Ok(snapshot)
}

impl McpBridge {
    pub(crate) fn new(servers: Vec<McpServerConfig>) -> Self {
        Self {
            servers,
            tools: Vec::new(),
            routes: HashMap::new(),
            upstream_clients: HashMap::new(),
            probe_statuses: Vec::new(),
        }
    }

    pub(crate) async fn refresh_tools(&mut self) {
        log_mcp_info(
            "bridge",
            "refresh_start",
            &format!("servers={}", self.servers.len()),
        );
        self.tools.clear();
        self.routes.clear();
        self.probe_statuses.clear();
        self.close_upstream_clients().await;

        for server in &self.servers {
            log_mcp_info(
                &server.alias,
                "refresh_server",
                &format!(
                    "enabled={} transport={} connect_timeout_ms={} request_timeout_ms={}",
                    server.enabled,
                    server.transport_kind().as_label(),
                    server.connect_timeout_ms,
                    server.request_timeout_ms
                ),
            );

            if !server.enabled {
                log_mcp_info(&server.alias, "skip", "server disabled");
                self.probe_statuses.push(McpServerProbeStatus {
                    server_id: server.id.clone(),
                    alias: server.alias.clone(),
                    state: McpProbeState::Unknown,
                    detail: "已禁用".to_string(),
                    tool_count: 0,
                    tools: Vec::new(),
                });
                continue;
            }

            if let Err(error) = validate_server_config(server, &self.servers, Some(&server.id)) {
                log_mcp_error(&server.alias, "validate", &error);
                self.probe_statuses.push(McpServerProbeStatus {
                    server_id: server.id.clone(),
                    alias: server.alias.clone(),
                    state: McpProbeState::Failed,
                    detail: error,
                    tool_count: 0,
                    tools: Vec::new(),
                });
                continue;
            }

            log_mcp_info(
                &server.alias,
                "connect_begin",
                &format!("endpoint={}", server.endpoint_summary()),
            );

            let mut service = match time::timeout(
                Duration::from_millis(server.connect_timeout_ms),
                connect_upstream_service(server),
            )
            .await
            {
                Ok(Ok(service)) => service,
                Ok(Err(error)) => {
                    log_mcp_error(&server.alias, "connect", &error);
                    self.probe_statuses.push(McpServerProbeStatus {
                        server_id: server.id.clone(),
                        alias: server.alias.clone(),
                        state: McpProbeState::Failed,
                        detail: error,
                        tool_count: 0,
                        tools: Vec::new(),
                    });
                    continue;
                }
                Err(_) => {
                    let detail = format!("连接超时 ({}ms)", server.connect_timeout_ms);
                    log_mcp_error(&server.alias, "connect_timeout", &detail);
                    self.probe_statuses.push(McpServerProbeStatus {
                        server_id: server.id.clone(),
                        alias: server.alias.clone(),
                        state: McpProbeState::Failed,
                        detail,
                        tool_count: 0,
                        tools: Vec::new(),
                    });
                    continue;
                }
            };
            log_mcp_info(&server.alias, "connect_ok", "upstream connected");

            let discovered_tools = match fetch_upstream_tools(&service, server).await {
                Ok(tools) => tools,
                Err(error) => {
                    log_mcp_error(&server.alias, "tools_list", &error);
                    let _ = service
                        .close_with_timeout(Duration::from_millis(
                            server.request_timeout_ms.min(1500),
                        ))
                        .await;

                    self.probe_statuses.push(McpServerProbeStatus {
                        server_id: server.id.clone(),
                        alias: server.alias.clone(),
                        state: McpProbeState::Failed,
                        detail: error,
                        tool_count: 0,
                        tools: Vec::new(),
                    });
                    continue;
                }
            };
            log_mcp_info(
                &server.alias,
                "tools_list_ok",
                &format!("upstream_tools={}", discovered_tools.len()),
            );

            let mut server_tools = Vec::new();
            for tool in discovered_tools.into_iter().take(MAX_TOOLS_PER_SERVER) {
                if self.tools.len() >= MAX_TOTAL_TOOLS {
                    break;
                }

                let public_name = format!("{}.{}", server.alias, tool.origin_name);
                if self.routes.contains_key(&public_name) {
                    continue;
                }

                let description = tool.description;

                self.routes.insert(
                    public_name.clone(),
                    ToolRoute {
                        server_id: server.id.clone(),
                        alias: server.alias.clone(),
                        origin_tool_name: tool.origin_name.to_string(),
                    },
                );
                self.tools.push(McpToolDescriptor {
                    public_name: public_name.clone(),
                    description: description.clone(),
                    input_schema: tool.input_schema,
                });
                server_tools.push(McpToolSummary {
                    name: public_name,
                    description,
                });
            }

            self.probe_statuses.push(McpServerProbeStatus {
                server_id: server.id.clone(),
                alias: server.alias.clone(),
                state: McpProbeState::Success,
                detail: "探测完成".to_string(),
                tool_count: server_tools.len(),
                tools: server_tools,
            });

            if self
                .probe_statuses
                .last()
                .is_some_and(|status| status.tool_count == 0)
            {
                log_mcp_info(&server.alias, "tools_list", "上游返回 0 个工具");
            }

            self.upstream_clients.insert(
                server.id.clone(),
                UpstreamClient {
                    request_timeout_ms: server.request_timeout_ms,
                    transport_kind: server.transport_kind(),
                    service,
                },
            );

            log_mcp_info(
                &server.alias,
                "refresh_server_done",
                &format!(
                    "public_tools={}",
                    self.probe_statuses
                        .last()
                        .map(|s| s.tool_count)
                        .unwrap_or(0)
                ),
            );
        }

        log_mcp_info(
            "bridge",
            "refresh_done",
            &format!(
                "connected_servers={} public_tools={}",
                self.upstream_clients.len(),
                self.tools.len()
            ),
        );
    }

    pub(crate) fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub(crate) fn probe_statuses(&self) -> &[McpServerProbeStatus] {
        &self.probe_statuses
    }

    pub(crate) async fn handle_inbound_message(
        &mut self,
        message: &InboundTextMessage,
        default_session_id: &str,
    ) -> Option<McpEnvelopeMessage> {
        if message.message_type != "mcp" {
            return None;
        }

        let session_id = message
            .session_id()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(default_session_id)
            .to_string();

        let payload_value = message.payload.get("payload").cloned();
        let request = match payload_value {
            Some(payload_value) => match serde_json::from_value::<JsonRpcMessage>(payload_value) {
                Ok(request) => request,
                Err(error) => {
                    log_mcp_error(
                        "ws-bridge",
                        "decode_inbound",
                        &format!("invalid mcp payload: {error}"),
                    );
                    return Some(McpEnvelopeMessage::new(
                        session_id,
                        JsonRpcMessage::failure(
                            None,
                            JSONRPC_INVALID_REQUEST,
                            "invalid mcp payload",
                            Some(json!({ "detail": error.to_string() })),
                        ),
                    ));
                }
            },
            None => {
                return Some(McpEnvelopeMessage::new(
                    session_id,
                    JsonRpcMessage::failure(
                        None,
                        JSONRPC_INVALID_REQUEST,
                        "missing mcp.payload",
                        None,
                    ),
                ));
            }
        };

        Some(McpEnvelopeMessage::new(
            session_id,
            self.handle_jsonrpc_request(request).await,
        ))
    }

    async fn handle_jsonrpc_request(&mut self, request: JsonRpcMessage) -> JsonRpcMessage {
        let request_id = request.id.clone();
        if request.jsonrpc != "2.0" {
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INVALID_REQUEST,
                "jsonrpc must be 2.0",
                None,
            );
        }

        let Some(method) = request.method.as_deref() else {
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INVALID_REQUEST,
                "missing method",
                None,
            );
        };

        match method {
            "initialize" => JsonRpcMessage::success(
                request_id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": "host-app-gpui-mcp-bridge",
                        "version": "0.1.0"
                    },
                    "capabilities": {
                        "tools": {}
                    }
                }),
            ),
            "tools/list" => {
                self.refresh_tools().await;
                let tools = self
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool.public_name,
                            "description": tool.description,
                            "inputSchema": tool.input_schema,
                        })
                    })
                    .collect::<Vec<_>>();

                JsonRpcMessage::success(
                    request_id,
                    json!({
                        "tools": tools,
                        "nextCursor": Value::Null,
                    }),
                )
            }
            "tools/call" => self.handle_tools_call(request_id, request.params).await,
            _ => JsonRpcMessage::failure(
                request_id,
                JSONRPC_METHOD_NOT_FOUND,
                format!("unsupported method `{method}`"),
                None,
            ),
        }
    }

    async fn handle_tools_call(
        &self,
        request_id: Option<Value>,
        params: Option<Value>,
    ) -> JsonRpcMessage {
        let Some(params) = params else {
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INVALID_PARAMS,
                "tools/call requires params",
                None,
            );
        };

        let Some(params_object) = params.as_object() else {
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INVALID_PARAMS,
                "tools/call params must be an object",
                None,
            );
        };

        let Some(name) = params_object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INVALID_PARAMS,
                "tools/call params.name is required",
                None,
            );
        };

        let arguments = match params_object.get("arguments") {
            None | Some(Value::Null) => None,
            Some(Value::Object(object)) => Some(object.clone()),
            Some(_) => {
                return JsonRpcMessage::failure(
                    request_id,
                    JSONRPC_INVALID_PARAMS,
                    "tools/call params.arguments must be an object",
                    None,
                );
            }
        };

        let Some(route) = self.routes.get(name) else {
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INVALID_PARAMS,
                format!("tool `{name}` not found"),
                None,
            );
        };

        let Some(upstream_client) = self.upstream_clients.get(&route.server_id) else {
            log_mcp_error(&route.alias, "tools_call", "upstream server not connected");
            return JsonRpcMessage::failure(
                request_id,
                JSONRPC_INTERNAL_ERROR,
                format!("upstream server `{}` not connected", route.alias),
                None,
            );
        };

        let request = match arguments {
            Some(arguments) => {
                CallToolRequestParams::new(route.origin_tool_name.clone()).with_arguments(arguments)
            }
            None => CallToolRequestParams::new(route.origin_tool_name.clone()),
        };

        let call_result = match time::timeout(
            Duration::from_millis(upstream_client.request_timeout_ms),
            upstream_client.service.call_tool(request),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                log_mcp_error(
                    &route.alias,
                    "tools_call",
                    &format!("upstream call failed: {error}"),
                );
                return JsonRpcMessage::failure(
                    request_id,
                    JSONRPC_INTERNAL_ERROR,
                    format!(
                        "upstream call failed ({})",
                        upstream_client.transport_kind.as_label()
                    ),
                    Some(json!({
                        "server_id": route.server_id,
                        "tool": route.origin_tool_name,
                        "detail": error.to_string(),
                    })),
                );
            }
            Err(_) => {
                let detail = format!(
                    "upstream call timeout ({}ms)",
                    upstream_client.request_timeout_ms
                );
                log_mcp_error(&route.alias, "tools_call_timeout", &detail);
                return JsonRpcMessage::failure(
                    request_id,
                    JSONRPC_INTERNAL_ERROR,
                    detail,
                    Some(json!({
                        "server_id": route.server_id,
                        "tool": route.origin_tool_name,
                    })),
                );
            }
        };

        match serde_json::to_value(call_result) {
            Ok(result) => JsonRpcMessage::success(request_id, result),
            Err(error) => {
                log_mcp_error(&route.alias, "serialize_tool_result", &error.to_string());
                JsonRpcMessage::failure(
                    request_id,
                    JSONRPC_INTERNAL_ERROR,
                    "tool result serialization failed",
                    Some(json!({ "detail": error.to_string() })),
                )
            }
        }
    }

    async fn close_upstream_clients(&mut self) {
        if !self.upstream_clients.is_empty() {
            log_mcp_info(
                "bridge",
                "close_upstreams",
                &format!("count={}", self.upstream_clients.len()),
            );
        }
        for upstream_client in self.upstream_clients.values_mut() {
            let _ = upstream_client
                .service
                .close_with_timeout(Duration::from_millis(
                    upstream_client.request_timeout_ms.min(1500),
                ))
                .await;
        }
        self.upstream_clients.clear();
    }
}

async fn connect_upstream_service(server: &McpServerConfig) -> Result<UpstreamService, String> {
    match &server.transport {
        McpTransportConfig::Stdio {
            command,
            args,
            env,
            cwd,
        } => {
            let mut child = Command::new(command);
            child.args(args);
            for (key, value) in env {
                child.env(key, value);
            }
            if let Some(cwd) = cwd {
                child.current_dir(cwd);
            }

            let transport = TokioChildProcess::new(child)
                .map_err(|error| format!("stdio 进程创建失败: {error}"))?;

            ().serve(transport)
                .await
                .map_err(|error| format!("stdio 连接失败: {error}"))
        }
        McpTransportConfig::Sse { url, headers } => {
            connect_streamable_http_service(url, headers, None).await
        }
        McpTransportConfig::Stream { url, headers, auth } => {
            connect_streamable_http_service(url, headers, auth.as_deref()).await
        }
    }
}

async fn connect_streamable_http_service(
    url: &str,
    headers: &BTreeMap<String, String>,
    auth: Option<&str>,
) -> Result<UpstreamService, String> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url.to_string());
    if let Some(auth_header) = resolve_auth_header(headers, auth) {
        config = config.auth_header(auth_header);
    }

    ().serve(StreamableHttpClientTransport::from_config(config))
        .await
        .map_err(|error| format!("streamable-http 连接失败: {error}"))
}

async fn fetch_upstream_tools(
    service: &UpstreamService,
    server: &McpServerConfig,
) -> Result<Vec<DiscoveredTool>, String> {
    let list_result = match time::timeout(
        Duration::from_millis(server.request_timeout_ms),
        service.list_tools(Default::default()),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            return Err(format!(
                "tools/list 失败 ({}): {error}",
                server.transport_kind().as_label()
            ));
        }
        Err(_) => {
            return Err(format!("tools/list 超时 ({}ms)", server.request_timeout_ms));
        }
    };

    let list_json = serde_json::to_value(list_result)
        .map_err(|error| format!("tools/list 解析失败: {error}"))?;
    parse_discovered_tools(list_json)
}

fn parse_discovered_tools(list_json: Value) -> Result<Vec<DiscoveredTool>, String> {
    let tools = list_json
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "tools/list 响应缺少 tools 字段".to_string())?;

    let mut discovered = Vec::new();
    for tool in tools {
        let Some(tool_object) = tool.as_object() else {
            continue;
        };

        let Some(origin_name) = tool_object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        let description = tool_object
            .get("description")
            .and_then(Value::as_str)
            .or_else(|| tool_object.get("title").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();

        let input_schema = tool_object
            .get("inputSchema")
            .cloned()
            .or_else(|| tool_object.get("input_schema").cloned())
            .unwrap_or_else(|| json!({ "type": "object" }));

        discovered.push(DiscoveredTool {
            origin_name: origin_name.to_string(),
            description,
            input_schema,
        });
    }

    Ok(discovered)
}

fn resolve_auth_header(headers: &BTreeMap<String, String>, auth: Option<&str>) -> Option<String> {
    if let Some(auth) = auth {
        return Some(normalize_auth_header(auth));
    }

    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| normalize_auth_header(value))
}

fn normalize_auth_header(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("Bearer ") {
        rest.trim().to_string()
    } else if let Some(rest) = trimmed.strip_prefix("bearer ") {
        rest.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        new_mcp_server_id, preview_probe_statuses, validate_server_config, McpBridge,
        McpProbeState, McpServerConfig, McpTransportConfig, DEFAULT_MCP_CONNECT_TIMEOUT_MS,
        DEFAULT_MCP_REQUEST_TIMEOUT_MS,
    };
    use host_core::InboundTextMessage;
    use serde_json::json;

    fn demo_server(alias: &str) -> McpServerConfig {
        McpServerConfig {
            id: new_mcp_server_id(alias),
            alias: alias.to_string(),
            enabled: true,
            transport: McpTransportConfig::Sse {
                url: "https://mcp.example.com/sse".to_string(),
                headers: Default::default(),
            },
            request_timeout_ms: DEFAULT_MCP_REQUEST_TIMEOUT_MS,
            connect_timeout_ms: DEFAULT_MCP_CONNECT_TIMEOUT_MS,
        }
    }

    #[test]
    fn server_alias_must_be_unique() {
        let existing = vec![demo_server("calendar")];
        let duplicated = McpServerConfig {
            id: new_mcp_server_id("calendar-copy"),
            alias: "calendar".to_string(),
            enabled: true,
            transport: McpTransportConfig::Sse {
                url: "https://example.com/sse".to_string(),
                headers: Default::default(),
            },
            request_timeout_ms: DEFAULT_MCP_REQUEST_TIMEOUT_MS,
            connect_timeout_ms: DEFAULT_MCP_CONNECT_TIMEOUT_MS,
        };

        let error = validate_server_config(&duplicated, &existing, None).expect_err("must fail");
        assert!(error.contains("已存在"));
    }

    #[test]
    fn preview_probe_status_marks_enabled_server_as_unknown_before_connect() {
        let statuses = preview_probe_statuses(&[demo_server("calendar")]);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].state, McpProbeState::Unknown);
        assert!(statuses[0].tools.is_empty());
    }

    #[tokio::test]
    async fn tools_call_returns_error_for_unknown_tool() {
        let mut bridge = McpBridge::new(Vec::new());

        let message: InboundTextMessage = serde_json::from_value(json!({
            "type": "mcp",
            "session_id": "session-001",
            "payload": {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "calendar.not_exists",
                    "arguments": {}
                }
            }
        }))
        .expect("parse inbound mcp message");

        let response = bridge
            .handle_inbound_message(&message, "session-fallback")
            .await
            .expect("response exists for mcp");

        assert_eq!(response.payload.jsonrpc, "2.0");
        assert!(response.payload.error.is_some());
        assert_eq!(
            response.payload.error.as_ref().expect("error").code,
            -32_602
        );
    }
}
