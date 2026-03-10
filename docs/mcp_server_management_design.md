# MCP Server 管理与 WS 桥接设计方案

## 1. 背景

当前桌面端已经具备 WebSocket 主链路（`hello` 握手、`listen` 控制、双向音频），并在聊天面板可展示 `mcp/tool/intent_trace` 类事件。

在当前 Rust 实现中，MCP 第一版聚合闭环已落地：

1. 客户端设置面板已支持 MCP Server 管理（新增/编辑/删除/启用/禁用/刷新）。
2. 连接 WS 时 `hello` 已声明 `features.mcp=true`。
3. 已可处理平台下发 `mcp` JSON-RPC 请求：`initialize`、`tools/list`、`tools/call`。

本文档在保留原始设计思路的同时，补充当前实现映射与已知边界，供后续迭代使用。

---

## 2. 设计依据

本方案对齐以下参考：

- 官方 Rust MCP SDK：`modelcontextprotocol/rust-sdk`（crate: `rmcp`）
- 灵矽端侧 MCP 文档：`https://linx.qiniu.com/docs/xrobot/mcp/hardware-mcp`

关键协议结论：

- `hello` 阶段先声明 `features.mcp`（能力声明）。
- 具体工具发现与调用通过 `type: "mcp"` 的 JSON-RPC 消息进行，而不是仅靠 `hello` 直接携带完整工具细节。

---

## 3. 目标与边界

## 3.1 目标

- 提供 MCP Server 管理页面（新增/编辑/删除/启用/禁用/刷新）。
- 支持三类 transport：
  - `stdio`
  - `sse`
  - `stream`（streamable HTTP）
- 建立本地 MCP 聚合层，对外提供统一工具视图。
- 在 WS 会话中完成 MCP 标准请求处理闭环。

## 3.2 非目标（本期不做）

- 不实现 prompts/resources/sampling 的完整透传。
- 不做跨设备云端配置同步。
- 不做复杂 RBAC 权限体系，仅保留最小安全校验与脱敏。

---

## 4. 总体架构

```text
┌──────────────────────────────────────────────────────────┐
│                  host-app-gpui (桌面端)                 │
│                                                          │
│  ┌─────────────────┐    ┌────────────────────────────┐  │
│  │ MCP 管理页面     │ -> │ MCP 配置存储 (本地文件)     │  │
│  └─────────────────┘    └────────────────────────────┘  │
│            │                               │             │
│            v                               v             │
│      ┌──────────────────────────────────────────────┐    │
│      │ MCP 聚合器 (rmcp client pool + 工具路由表)   │    │
│      └──────────────────────────────────────────────┘    │
│                        │                                 │
│                        v                                 │
│      ┌──────────────────────────────────────────────┐    │
│      │ WS 网关桥接 (hello + mcp JSON-RPC handler)   │    │
│      └──────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
                         │
                         v
                灵矽 AI 平台 (MCP 客户端)

上游 MCP Server (被聚合): stdio / sse / stream
```

角色定义：

- 对上游 MCP Server：本客户端是 MCP Client。
- 对灵矽平台：本客户端表现为支持 MCP 的设备端。

---

## 5. 协议设计

## 5.1 hello 能力声明

在现有 `hello` 的 `features.notify.intent_trace=true` 基础上，增加 `features.mcp=true`。

示例：

```json
{
  "type": "hello",
  "device_id": "<device_id>",
  "device_name": "<device_name>",
  "device_mac": "<device_mac>",
  "token": "<token>",
  "features": {
    "notify": {
      "intent_trace": true
    },
    "mcp": true
  }
}
```

说明：

- 以端侧文档优先，`mcp` 先采用布尔能力声明。
- 若后端后续明确支持，可扩展为对象能力描述（向后兼容）。

## 5.2 mcp 消息封装

沿用平台文档封装：

```json
{
  "type": "mcp",
  "session_id": "<session_id>",
  "payload": {
    "jsonrpc": "2.0",
    "method": "tools/list",
    "params": { "cursor": "" },
    "id": 2
  }
}
```

支持的方法：

1. `initialize`
2. `tools/list`
3. `tools/call`

错误响应遵循 JSON-RPC 2.0 `error` 结构。

---

## 6. MCP 聚合与路由

## 6.1 工具聚合策略

- 对所有启用的 MCP Server 拉取 `tools/list`。
- 聚合后对平台暴露统一工具列表。
- 为避免重名，采用命名空间策略：`<server_alias>.<tool_name>`。

示例：

- `calendar.get_events`（来自 `calendar` server）
- `iot.set_light`（来自 `iot` server）

## 6.2 路由表

维护内存路由：

- key: `public_tool_name`
- value: `{ server_id, origin_tool_name, transport, server_session_handle }`

`tools/call` 到来时：

1. 根据 `public_tool_name` 查路由。
2. 转发到对应上游 server（还原原始 tool name）。
3. 将结果转换为 JSON-RPC result/error 回包。

## 6.3 故障隔离

- 某个 server 异常不影响整体 WS 连接。
- `tools/list` 返回可用子集。
- 调用失败时返回可诊断错误（不吞错）。

---

## 7. 管理页面设计

入口：设置面板新增分区 `MCP Servers`。

列表字段：

- 名称（alias）
- transport（stdio/sse/stream）
- endpoint 摘要（命令或 URL）
- 启用状态
- 最近探测状态（成功/失败）
- 工具数量

操作：

- 新增
- 编辑
- 删除
- 启用/禁用
- 立即刷新工具

表单字段：

### stdio

- `command`（必填）
- `args`（可选）
- `env`（可选）
- `cwd`（可选）

### sse

- `url`（必填）
- `headers`（可选）

### stream

- `url`（必填）
- `headers`（可选）
- `auth`（可选，统一转为 header）

校验规则：

- alias 不可重复
- 必填字段不能为空
- URL 必须可解析
- 变更保存前进行最小合法性检查

---

## 8. 配置持久化

新增本地配置结构（建议并入应用统一配置）：

- WS 连接配置（现有）
- MCP Server 列表（新增）

保存行为：

- 用户点击“保存并关闭”时落盘。
- 应用启动时优先读取本地配置；缺失字段再回退 env/default。

安全要求：

- token、authorization、*_token 字段写日志必须脱敏。

---

## 9. 运行时时序

## 9.1 首次连接时序

1. 读取本地 MCP 配置。
2. 初始化 MCP 聚合器（按启用项建连，带超时）。
3. 建立 WS 连接。
4. 发送 `hello`（含 `features.mcp`）。
5. 处理平台下发：
   - `initialize`
   - `tools/list`
   - `tools/call`

## 9.2 tools/list 时序

1. 平台发 `mcp.tools/list`。
2. 本地返回聚合工具列表。
3. 若存在不可用 server，仅返回可用子集并记录系统通知。

## 9.3 tools/call 时序

1. 平台发 `mcp.tools/call(name, arguments)`。
2. 路由到上游 server。
3. 返回 result 或 error。

---

## 10. rmcp 接入策略（已落地）

当前实现已接入 `rmcp` 作为统一上游 SDK：

- `stdio`：`TokioChildProcess`
- `stream`：`StreamableHttpClientTransport`
- `sse`：当前按 Streamable HTTP 链路接入（与 `stream` 共享客户端实现）

当前实现特性：

- 统一连接超时（`connect_timeout_ms`）与请求超时（`request_timeout_ms`）。
- 对连接失败、超时、调用失败做结构化错误回包（JSON-RPC error）。
- 按 server 维度故障隔离，返回可用工具子集，不阻断 WS 主链路。

---

## 11. 数据模型草案

```text
McpServerConfig {
  id: String,
  alias: String,
  enabled: bool,
  transport: McpTransportConfig,
  request_timeout_ms: u64,
  connect_timeout_ms: u64
}

McpTransportConfig =
  Stdio { command, args[], env{key:value}, cwd? }
  Sse    { url, headers{key:value} }
  Stream { url, headers{key:value}, auth? }

McpToolDescriptor {
  public_name: String,
  origin_name: String,
  server_id: String,
  description: String,
  input_schema: Json
}
```

---

## 12. 里程碑（当前状态）

## Phase A（协议闭环）- 已完成

- MCP 管理页基础交互已落地
- `hello` 已增加 `features.mcp`
- `mcp` JSON-RPC 框架已可处理核心方法

## Phase B（真实上游）- 主体完成

- `rmcp` 已引入
- `stdio`/`stream` 已打通
- `sse` 已按当前 SDK 统一链路接入
- 真实 `tools/list` 与 `tools/call` 已可联调

## Phase C（增强）- 进行中

- 健康检查与自动重连策略待补齐
- 分页/缓存策略当前为简化实现（含工具数量上限保护）
- 调用指标与审计视图尚未完整落地

---

## 13. 验收状态（2026-03-11）

1. 设置面板可管理三类 MCP Server，并支持本地持久化（已完成）。
2. 连接 WS 后可在 `hello` 中看到 `features.mcp`（已完成）。
3. 平台发 `initialize/tools/list/tools/call` 可获得响应（已完成）。
4. 单个上游 server 失败时主链路保持可用，错误可在 UI/日志定位（已完成）。
5. 敏感字段脱敏已在关键信息展示链路落地（已完成，后续继续扩展覆盖面）。

---

## 14. 风险与应对

风险：

- 上游 server 行为不一致（不同 transport 差异大）
- 工具数量过多导致 `tools/list` payload 过大
- 首连时过多 server 并发初始化导致耗时上升

应对：

- 统一错误映射与能力探测
- 支持分页/截断与最大工具数保护
- 并发初始化 + 单服务超时 + 可用子集降级

---

## 15. 与现有工程的对应关系（已落地）

当前已落地模块：

- `crates/host-core`：`hello` features 与 `mcp` envelope、JSON-RPC 数据结构
- `crates/host-platform`：WS 文本/二进制事件收发与会话握手
- `apps/host-app-gpui/src/mcp/mod.rs`：MCP 聚合、上游连接、工具路由、错误映射
- `apps/host-app-gpui/src/features/mcp_servers.rs`：MCP 管理页交互与本地探测
- `apps/host-app-gpui/src/app/persistence.rs`：MCP 配置持久化
- `apps/host-app-gpui/src/gateway_runtime.rs`：WS `mcp` 消息处理与回包下发

## 16. 当前边界与后续补齐

- `tools/list` 当前采用全量刷新策略，后续可增加增量缓存与分页。
- `sse` 与 `stream` 当前复用同一 HTTP 客户端链路，针对 legacy SSE 的专门兼容仍可增强。
- 缺少独立审计视图与统计面板，当前以聊天面板与日志为主。
- 连接健康检查与自动重连策略仍需进一步完善。
