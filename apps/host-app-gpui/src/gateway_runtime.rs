use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use aec3::voip::VoipAec3;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use host_core::{ClientTextMessage, ListenMode, AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE_HZ};
use host_platform::{WsGatewayClient, WsGatewayConfig, WsGatewayEvent};
use opus::{
    Application as OpusApplication, Bitrate as OpusBitrate, Channels as OpusChannels,
    Decoder as OpusDecoder, Encoder as OpusEncoder,
};
use serde::Serialize;
use serde_json::Value;
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time;

use crate::app::state::{AecStatsSnapshot, AudioRoutingConfig, GatewayCommand, UiGatewayEvent};
use crate::mcp::{McpBridge, McpProbeState, McpServerConfig};

pub const COMMAND_CHANNEL_CAPACITY: usize = 64;
pub const EVENT_CHANNEL_CAPACITY: usize = 512;

const HOST_INPUT_DEVICE_ENV: &str = "HOST_INPUT_DEVICE";
const HOST_OUTPUT_DEVICE_ENV: &str = "HOST_OUTPUT_DEVICE";
const HOST_ENABLE_AEC_ENV: &str = "HOST_ENABLE_AEC";
const HOST_ENABLE_INPUT_MONITOR_ENV: &str = "HOST_ENABLE_INPUT_MONITOR";
const HOST_ENABLE_OUTPUT_MONITOR_ENV: &str = "HOST_ENABLE_OUTPUT_MONITOR";
const OPUS_BITRATE_BPS: i32 = 16_000;
const OPUS_COMPLEXITY: i32 = 3;
const OPUS_MAX_PACKET_BYTES: usize = 4000;
const OPUS_DECODE_MAX_SAMPLES: usize = 4096;
const DOWNLINK_BUFFER_SECONDS: usize = 2;
const MICROPHONE_FRAME_CHANNEL_CAPACITY: usize = 64;
const AEC_FRAME_DURATION_MS: usize = 10;
const AEC_FRAME_SAMPLES: usize = (AUDIO_SAMPLE_RATE_HZ as usize * AEC_FRAME_DURATION_MS) / 1_000;
const AEC_INITIAL_DELAY_MS: i32 = 120;
const AEC_STATS_EMIT_INTERVAL: Duration = Duration::from_millis(500);
const WS_RTT_PING_INTERVAL: Duration = Duration::from_secs(2);
const WS_RTT_PING_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug)]
pub struct AudioDeviceState {
    pub input_devices: Vec<String>,
    pub output_devices: Vec<String>,
    pub selected_input_index: Option<usize>,
    pub selected_input_output_index: Option<usize>,
    pub input_from_output: bool,
    pub selected_output_index: Option<usize>,
}

pub fn load_audio_device_state() -> AudioDeviceState {
    let host = cpal::default_host();

    let mut input_devices = list_input_device_names(&host);
    let mut output_devices = list_output_device_names(&host);
    input_devices.sort_unstable();
    output_devices.sort_unstable();

    let default_input_index = default_input_device_name(&host)
        .as_deref()
        .and_then(|name| find_device_index_by_hint(&input_devices, name));
    let default_output_index = default_output_device_name(&host)
        .as_deref()
        .and_then(|name| find_device_index_by_hint(&output_devices, name));

    let input_hint = env_optional(HOST_INPUT_DEVICE_ENV);
    let input_hint_ref = input_hint.as_deref();
    let env_input_index =
        input_hint_ref.and_then(|hint| find_device_index_by_hint(&input_devices, hint));
    let env_input_output_index = if env_input_index.is_none() {
        input_hint_ref.and_then(|hint| find_device_index_by_hint(&output_devices, hint))
    } else {
        None
    };
    let input_from_output = env_input_output_index.is_some();
    let selected_input_index = if input_from_output {
        None
    } else {
        env_input_index.or(default_input_index)
    };
    let selected_input_output_index = env_input_output_index;
    let selected_output_index = env_optional(HOST_OUTPUT_DEVICE_ENV)
        .as_deref()
        .and_then(|hint| find_device_index_by_hint(&output_devices, hint))
        .or(default_output_index);

    AudioDeviceState {
        input_devices,
        output_devices,
        selected_input_index,
        selected_input_output_index,
        input_from_output,
        selected_output_index,
    }
}

pub fn spawn_gateway_worker(
    config: WsGatewayConfig,
    audio_routing: AudioRoutingConfig,
    initial_listen_mode: ListenMode,
    mcp_servers: Vec<McpServerConfig>,
    mut command_rx: mpsc::Receiver<GatewayCommand>,
    event_tx: mpsc::Sender<UiGatewayEvent>,
) {
    thread::spawn(move || {
        let runtime = match TokioRuntimeBuilder::new_multi_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                try_emit_event(
                    &event_tx,
                    UiGatewayEvent::Error(format!("Failed to build tokio runtime: {error}")),
                );
                try_emit_event(&event_tx, UiGatewayEvent::Disconnected);
                return;
            }
        };

        runtime.block_on(async move {
            let _ = event_tx
                .send(UiGatewayEvent::OutgoingText {
                    kind: "hello".to_string(),
                    payload: to_redacted_pretty_json(&config.hello_message()),
                })
                .await;

            let mut client = match WsGatewayClient::connect(config).await {
                Ok(client) => client,
                Err(error) => {
                    let _ = event_tx
                        .send(UiGatewayEvent::Error(format!(
                            "Gateway connection failed: {error}"
                        )))
                        .await;
                    let _ = event_tx.send(UiGatewayEvent::Disconnected).await;
                    return;
                }
            };

            let _ = event_tx
                .send(UiGatewayEvent::Connected {
                    session_id: client.session_id().to_string(),
                })
                .await;
            let _ = event_tx
                .send(UiGatewayEvent::SystemNotice(format!(
                    "Audio route active -> input: {}, output: {}",
                    audio_routing.input_label(),
                    audio_routing.output_label()
                )))
                .await;

            let mut mcp_bridge = McpBridge::new(mcp_servers);
            mcp_bridge.refresh_tools().await;
            let initial_mcp_statuses = mcp_bridge.probe_statuses().to_vec();
            let _ = event_tx
                .send(UiGatewayEvent::McpProbeStatuses(initial_mcp_statuses.clone()))
                .await;
            let _ = event_tx
                .send(UiGatewayEvent::SystemNotice(format!(
                    "MCP bridge ready: {} tools available",
                    mcp_bridge.tool_count()
                )))
                .await;

            let failed_servers = initial_mcp_statuses
                .iter()
                .filter(|status| status.state == McpProbeState::Failed)
                .map(|status| format!("{} ({})", status.alias, status.detail))
                .collect::<Vec<_>>();
            if !failed_servers.is_empty() {
                eprintln!("[mcp][refresh] {}", failed_servers.join("; "));
                let _ = event_tx
                    .send(UiGatewayEvent::Error(format!(
                        "MCP 刷新失败: {}",
                        failed_servers.join("; ")
                    )))
                    .await;
            }

            let force_aec_for_shared_route = audio_routing.has_shared_input_output_route();
            if force_aec_for_shared_route {
                let _ = event_tx
                    .send(UiGatewayEvent::SystemNotice(
                        "Input/output share same route, forcing AEC enabled".to_string(),
                    ))
                    .await;
            }

            let has_aec_env_override = env_optional(HOST_ENABLE_AEC_ENV).is_some();
            let mut aec_enabled = if force_aec_for_shared_route {
                true
            } else {
                env_bool_or_default(HOST_ENABLE_AEC_ENV, audio_routing.aec_enabled)
            };

            let mut echo_canceller = if aec_enabled {
                match SharedEchoCanceller::new() {
                    Ok(echo_canceller) => {
                        let _ = event_tx
                            .send(UiGatewayEvent::SystemNotice(
                                "AEC enabled (AEC3 real-time, 16kHz mono)".to_string(),
                            ))
                            .await;
                        Some(echo_canceller)
                    }
                    Err(error) => {
                        aec_enabled = false;
                        let _ = event_tx
                            .send(UiGatewayEvent::Error(format!(
                                "AEC init failed, fallback to raw mic capture: {error}"
                            )))
                            .await;
                        None
                    }
                }
            } else {
                let _ = event_tx
                    .send(UiGatewayEvent::SystemNotice(if has_aec_env_override {
                        "AEC disabled by env HOST_ENABLE_AEC".to_string()
                    } else {
                        "AEC disabled".to_string()
                    }))
                    .await;
                None
            };

            let _ = event_tx.send(UiGatewayEvent::AecStateChanged(aec_enabled)).await;

            let mut uplink_streaming = false;
            let mut listen_mode = initial_listen_mode;
            let mut speaker_output_enabled = audio_routing.speaker_output_enabled;
            let mut microphone_capture = None;
            let mut microphone_frame_rx = empty_audio_receiver();
            let mut downlink_player: Option<DownlinkAudioPlayer> = None;
            let mut downlink_playback_error_reported = false;
            let mut input_monitor_player: Option<DownlinkAudioPlayer> = None;
            let mut input_monitor_error_reported = false;
            let mut output_monitor_player: Option<DownlinkAudioPlayer> = None;
            let mut output_monitor_error_reported = false;
            let mirror_input_to_system =
                env_bool_or_default(HOST_ENABLE_INPUT_MONITOR_ENV, false);
            let mirror_output_to_system = env_bool_or_default(HOST_ENABLE_OUTPUT_MONITOR_ENV, false)
                && should_mirror_selected_output_to_system(
                    audio_routing.output_device_name.as_deref(),
                );
            let mut aec_stats_interval = time::interval(AEC_STATS_EMIT_INTERVAL);
            aec_stats_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            let mut ws_rtt_ping_interval = time::interval(WS_RTT_PING_INTERVAL);
            ws_rtt_ping_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            let mut pending_ws_ping_sent_at: Option<Instant> = None;

            loop {
                tokio::select! {
                    maybe_command = command_rx.recv() => {
                        let Some(command) = maybe_command else {
                            break;
                        };

                        if let GatewayCommand::RefreshMcpTools = command {
                            mcp_bridge.refresh_tools().await;
                            let refreshed_statuses = mcp_bridge.probe_statuses().to_vec();
                            let _ = event_tx
                                .send(UiGatewayEvent::McpProbeStatuses(refreshed_statuses.clone()))
                                .await;
                            let _ = event_tx
                                .send(UiGatewayEvent::SystemNotice(format!(
                                    "MCP tools refreshed: {} tools available",
                                    mcp_bridge.tool_count()
                                )))
                                .await;

                            let failed_servers = refreshed_statuses
                                .iter()
                                .filter(|status| status.state == McpProbeState::Failed)
                                .map(|status| format!("{} ({})", status.alias, status.detail))
                                .collect::<Vec<_>>();
                            if !failed_servers.is_empty() {
                                eprintln!("[mcp][refresh] {}", failed_servers.join("; "));
                                let _ = event_tx
                                    .send(UiGatewayEvent::Error(format!(
                                        "MCP 刷新失败: {}",
                                        failed_servers.join("; ")
                                    )))
                                    .await;
                            }
                            continue;
                        }

                        if let GatewayCommand::SetSpeakerOutputEnabled(enabled) = command {
                            if speaker_output_enabled != enabled {
                                speaker_output_enabled = enabled;

                                if !speaker_output_enabled {
                                    close_player(
                                        &event_tx,
                                        &mut downlink_player,
                                        "Downlink playback paused",
                                        "Downlink playback closed",
                                    );
                                    close_player(
                                        &event_tx,
                                        &mut output_monitor_player,
                                        "Output monitor paused",
                                        "Output monitor closed",
                                    );
                                }

                                let _ = event_tx
                                    .send(UiGatewayEvent::SystemNotice(if speaker_output_enabled {
                                        "Output playback enabled".to_string()
                                    } else {
                                        "Output playback paused".to_string()
                                    }))
                                    .await;
                            }
                            continue;
                        }

                        if let GatewayCommand::SetAecEnabled(enabled) = command {
                            if force_aec_for_shared_route && !enabled {
                                let notice = if aec_enabled {
                                    "Input/output share same route, AEC stays enabled"
                                } else {
                                    "Shared route requires AEC, but AEC is currently unavailable"
                                };
                                let _ = event_tx
                                    .send(UiGatewayEvent::SystemNotice(notice.to_string()))
                                    .await;
                                let _ = event_tx.send(UiGatewayEvent::AecStateChanged(aec_enabled)).await;
                                continue;
                            }

                            if aec_enabled == enabled {
                                continue;
                            }

                            if enabled {
                                match SharedEchoCanceller::new() {
                                    Ok(next_echo_canceller) => {
                                        if let Err(error) = restart_microphone_capture_if_streaming(
                                            &event_tx,
                                            uplink_streaming,
                                            &mut microphone_capture,
                                            &mut microphone_frame_rx,
                                            &audio_routing,
                                            Some(next_echo_canceller.clone()),
                                            "Microphone capture restarted with AEC enabled",
                                        ) {
                                            let _ = event_tx
                                                .send(UiGatewayEvent::Error(format!(
                                                    "Failed to apply AEC enable at runtime: {error}"
                                                )))
                                                .await;
                                            continue;
                                        }

                                        echo_canceller = Some(next_echo_canceller);
                                        aec_enabled = true;
                                        downlink_player = None;
                                        downlink_playback_error_reported = false;

                                        let _ = event_tx
                                            .send(UiGatewayEvent::SystemNotice(
                                                "AEC enabled (runtime update applied)".to_string(),
                                            ))
                                            .await;
                                        let _ =
                                            event_tx.send(UiGatewayEvent::AecStateChanged(true)).await;
                                    }
                                    Err(error) => {
                                        let _ = event_tx
                                            .send(UiGatewayEvent::Error(format!(
                                                "Failed to enable AEC at runtime: {error}"
                                            )))
                                            .await;
                                    }
                                }
                            } else {
                                if let Err(error) = restart_microphone_capture_if_streaming(
                                    &event_tx,
                                    uplink_streaming,
                                    &mut microphone_capture,
                                    &mut microphone_frame_rx,
                                    &audio_routing,
                                    None,
                                    "Microphone capture restarted with AEC disabled",
                                ) {
                                    let _ = event_tx
                                        .send(UiGatewayEvent::Error(format!(
                                            "Failed to apply AEC disable at runtime: {error}"
                                        )))
                                        .await;
                                    continue;
                                }

                                echo_canceller = None;
                                aec_enabled = false;
                                downlink_player = None;
                                downlink_playback_error_reported = false;

                                let _ = event_tx
                                    .send(UiGatewayEvent::SystemNotice(
                                        "AEC disabled (runtime update applied)".to_string(),
                                    ))
                                    .await;
                                let _ = event_tx.send(UiGatewayEvent::AecStateChanged(false)).await;
                            }

                            continue;
                        }

                        if let GatewayCommand::SetListenMode(mode) = command {
                            if listen_mode != mode {
                                listen_mode = mode;
                                let _ = event_tx
                                    .send(UiGatewayEvent::SystemNotice(format!(
                                        "Listen mode updated: {}",
                                        listen_mode_code(listen_mode)
                                    )))
                                    .await;
                            }
                            continue;
                        }

                        let keep_running = handle_gateway_command(
                            command,
                            listen_mode,
                            &mut client,
                            &event_tx,
                            &mut uplink_streaming,
                            &mut microphone_capture,
                            &mut microphone_frame_rx,
                            echo_canceller.as_ref(),
                            &audio_routing,
                        )
                        .await;
                        if !keep_running {
                            break;
                        }
                    }
                    maybe_frame = microphone_frame_rx.recv(), if uplink_streaming => {
                        let Some(frame) = maybe_frame else {
                            stop_uplink_capture(
                                &mut uplink_streaming,
                                &mut microphone_capture,
                                &mut microphone_frame_rx,
                                &event_tx,
                            );
                            let _ = event_tx
                                .send(UiGatewayEvent::Error(
                                    "Microphone capture stopped unexpectedly".to_string(),
                                ))
                                .await;
                            if let Err(error) = client.send_listen_stop_with_mode(listen_mode).await {
                                let _ = event_tx
                                    .send(UiGatewayEvent::Error(format!(
                                        "Failed to send listen stop after microphone ended: {error}"
                                    )))
                                    .await;
                                break;
                            }

                            let _ = event_tx
                                .send(UiGatewayEvent::OutgoingText {
                                    kind: "listen".to_string(),
                                    payload: to_redacted_pretty_json(
                                        &ClientTextMessage::listen_stop_with_mode(listen_mode),
                                    ),
                                })
                                .await;
                            continue;
                        };

                        if mirror_input_to_system && !input_monitor_error_reported {
                            if input_monitor_player.is_none() {
                                match DownlinkAudioPlayer::new(event_tx.clone(), None, None) {
                                    Ok(player) => {
                                        let description = player.description().to_string();
                                        let _ = event_tx
                                            .send(UiGatewayEvent::SystemNotice(format!(
                                                "Input monitor ready (selected input -> system speaker): {description}"
                                            )))
                                            .await;
                                        input_monitor_player = Some(player);
                                    }
                                    Err(error) => {
                                        input_monitor_error_reported = true;
                                        let _ = event_tx
                                            .send(UiGatewayEvent::Error(format!(
                                                "Failed to init input monitor playback: {error}"
                                            )))
                                            .await;
                                    }
                                }
                            }

                            if let Some(player) = input_monitor_player.as_mut() {
                                if let Err(error) = player.push_opus_packet(&frame) {
                                    if !input_monitor_error_reported {
                                        input_monitor_error_reported = true;
                                        let _ = event_tx
                                            .send(UiGatewayEvent::Error(format!(
                                                "Failed to play input monitor packet: {error}"
                                            )))
                                            .await;
                                    }
                                }
                            }
                        }

                        let frame_bytes = frame.len();
                        if let Err(error) = client.send_audio_frame(frame).await {
                            let _ = event_tx
                                .send(UiGatewayEvent::Error(format!(
                                    "Failed to send streaming audio frame: {error}"
                                )))
                                .await;
                            break;
                        }

                        try_emit_event(&event_tx, UiGatewayEvent::UplinkAudioFrameSent(frame_bytes));
                    }
                    maybe_event = client.next_event() => {
                        let Some(event) = maybe_event else {
                            break;
                        };

                        match event {
                            WsGatewayEvent::Text(message) => {
                                if let Some(response) =
                                    mcp_bridge
                                        .handle_inbound_message(&message, client.session_id())
                                        .await
                                {
                                    if let Err(error) = client
                                        .send_mcp_jsonrpc(
                                            response.session_id.clone(),
                                            response.payload.clone(),
                                        )
                                        .await
                                    {
                                        let _ = event_tx
                                            .send(UiGatewayEvent::Error(format!(
                                                "Failed to send MCP response: {error}"
                                            )))
                                            .await;
                                        break;
                                    }

                                    let _ = event_tx
                                        .send(UiGatewayEvent::OutgoingText {
                                            kind: "mcp".to_string(),
                                            payload: to_redacted_pretty_json(
                                                &ClientTextMessage::mcp(response),
                                            ),
                                        })
                                        .await;

                                    if message
                                        .payload
                                        .get("payload")
                                        .and_then(|value| value.get("method"))
                                        .and_then(Value::as_str)
                                        == Some("tools/list")
                                    {
                                        let _ = event_tx
                                            .send(UiGatewayEvent::McpProbeStatuses(
                                                mcp_bridge.probe_statuses().to_vec(),
                                            ))
                                            .await;
                                    }
                                }

                                let _ = event_tx.send(UiGatewayEvent::IncomingText(message)).await;
                            }
                            WsGatewayEvent::DownlinkAudio(data) => {
                                if speaker_output_enabled {
                                    if downlink_player.is_none() && !downlink_playback_error_reported {
                                        match DownlinkAudioPlayer::new(
                                            event_tx.clone(),
                                            audio_routing.output_device_name.as_deref(),
                                            echo_canceller.clone(),
                                        ) {
                                            Ok(player) => {
                                                let description = player.description().to_string();
                                                let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                                                    "Downlink playback ready: {description}"
                                                ))).await;
                                                downlink_player = Some(player);
                                            }
                                            Err(error) => {
                                                downlink_playback_error_reported = true;
                                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                    "Failed to init downlink playback: {error}"
                                                ))).await;
                                            }
                                        }
                                    }

                                    if let Some(player) = downlink_player.as_mut() {
                                        if let Err(error) = player.push_opus_packet(&data) {
                                            if !downlink_playback_error_reported {
                                                downlink_playback_error_reported = true;
                                                let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                    "Failed to decode/play downlink Opus packet: {error}"
                                                ))).await;
                                            }
                                        }
                                    }

                                    if mirror_output_to_system && !output_monitor_error_reported {
                                        if output_monitor_player.is_none() {
                                            match DownlinkAudioPlayer::new(event_tx.clone(), None, None) {
                                                Ok(player) => {
                                                    let description = player.description().to_string();
                                                    let _ = event_tx.send(UiGatewayEvent::SystemNotice(format!(
                                                        "Output monitor ready (selected output -> system speaker): {description}"
                                                    ))).await;
                                                    output_monitor_player = Some(player);
                                                }
                                                Err(error) => {
                                                    output_monitor_error_reported = true;
                                                    let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                        "Failed to init output monitor playback: {error}"
                                                    ))).await;
                                                }
                                            }
                                        }

                                        if let Some(player) = output_monitor_player.as_mut() {
                                            if let Err(error) = player.push_opus_packet(&data) {
                                                if !output_monitor_error_reported {
                                                    output_monitor_error_reported = true;
                                                    let _ = event_tx.send(UiGatewayEvent::Error(format!(
                                                        "Failed to play output monitor packet: {error}"
                                                    ))).await;
                                                }
                                            }
                                        }
                                    }
                                }

                                try_emit_event(
                                    &event_tx,
                                    UiGatewayEvent::DownlinkAudioFrameReceived(data.len()),
                                );
                            }
                            WsGatewayEvent::Pong(_) => {
                                if let Some(ping_sent_at) = pending_ws_ping_sent_at.take() {
                                    let rtt_ms = duration_to_millis_saturated(ping_sent_at.elapsed());
                                    try_emit_event(&event_tx, UiGatewayEvent::NetworkRttUpdated(rtt_ms));
                                }
                            }
                            WsGatewayEvent::MalformedText { raw, error } => {
                                let _ = event_tx
                                    .send(UiGatewayEvent::Error(format!(
                                        "Malformed text frame: {error} | raw={raw}"
                                    )))
                                    .await;
                            }
                            WsGatewayEvent::Closed => {
                                break;
                            }
                            WsGatewayEvent::TransportError(error) => {
                                let _ = event_tx
                                    .send(UiGatewayEvent::Error(format!(
                                        "Transport error: {error}"
                                    )))
                                    .await;
                                break;
                            }
                        }
                    }
                    _ = aec_stats_interval.tick(), if aec_enabled => {
                        if let Some(echo_canceller) = echo_canceller.as_ref() {
                            if let Some(stats) = echo_canceller.snapshot() {
                                try_emit_event(&event_tx, UiGatewayEvent::AecStats(stats));
                            }
                        }
                    }
                    _ = ws_rtt_ping_interval.tick() => {
                        if let Some(ping_sent_at) = pending_ws_ping_sent_at {
                            if ping_sent_at.elapsed() < WS_RTT_PING_TIMEOUT {
                                continue;
                            }
                        }

                        if let Err(error) = client.send_ping(Vec::new()).await {
                            let _ = event_tx
                                .send(UiGatewayEvent::Error(format!(
                                    "Failed to send websocket ping: {error}"
                                )))
                                .await;
                            break;
                        }

                        pending_ws_ping_sent_at = Some(Instant::now());
                    }
                }
            }

            stop_uplink_capture(
                &mut uplink_streaming,
                &mut microphone_capture,
                &mut microphone_frame_rx,
                &event_tx,
            );

            close_player(
                &event_tx,
                &mut downlink_player,
                "",
                "Downlink playback closed",
            );
            close_player(
                &event_tx,
                &mut input_monitor_player,
                "",
                "Input monitor closed",
            );
            close_player(
                &event_tx,
                &mut output_monitor_player,
                "",
                "Output monitor closed",
            );

            let _ = event_tx.send(UiGatewayEvent::Disconnected).await;
        });
    });
}

fn close_player(
    event_tx: &mpsc::Sender<UiGatewayEvent>,
    player: &mut Option<DownlinkAudioPlayer>,
    pause_notice: &str,
    close_notice: &str,
) {
    if let Some(player) = player.take() {
        if !pause_notice.is_empty() {
            try_emit_event(
                event_tx,
                UiGatewayEvent::SystemNotice(format!("{pause_notice}: {}", player.description())),
            );
        }
        if !close_notice.is_empty() {
            try_emit_event(
                event_tx,
                UiGatewayEvent::SystemNotice(format!("{close_notice}: {}", player.description())),
            );
        }
    }
}

fn try_emit_event(event_tx: &mpsc::Sender<UiGatewayEvent>, event: UiGatewayEvent) {
    let _ = event_tx.try_send(event);
}

fn listen_mode_code(mode: ListenMode) -> &'static str {
    match mode {
        ListenMode::Manual => "manual",
        ListenMode::Auto => "auto",
        ListenMode::Realtime => "realtime",
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_gateway_command(
    command: GatewayCommand,
    listen_mode: ListenMode,
    client: &mut WsGatewayClient,
    event_tx: &mpsc::Sender<UiGatewayEvent>,
    uplink_streaming: &mut bool,
    microphone_capture: &mut Option<MicrophoneCapture>,
    microphone_frame_rx: &mut mpsc::Receiver<Vec<u8>>,
    echo_canceller: Option<&SharedEchoCanceller>,
    audio_routing: &AudioRoutingConfig,
) -> bool {
    match command {
        GatewayCommand::Disconnect => {
            stop_uplink_capture(
                uplink_streaming,
                microphone_capture,
                microphone_frame_rx,
                event_tx,
            );
            false
        }
        GatewayCommand::DetectText(text) => {
            if let Err(error) = client
                .send_listen_detect_text_with_mode(text.clone(), listen_mode)
                .await
            {
                let _ = event_tx
                    .send(UiGatewayEvent::Error(format!(
                        "Failed to send detect text: {error}"
                    )))
                    .await;
                return false;
            }

            let _ = event_tx
                .send(UiGatewayEvent::OutgoingText {
                    kind: "listen".to_string(),
                    payload: to_redacted_pretty_json(
                        &ClientTextMessage::listen_detect_text_with_mode(text, listen_mode),
                    ),
                })
                .await;
            true
        }
        GatewayCommand::StartUplinkStream => {
            if *uplink_streaming {
                return true;
            }

            if let Err(error) = client.send_listen_start_with_mode(listen_mode).await {
                let _ = event_tx
                    .send(UiGatewayEvent::Error(format!(
                        "Failed to start uplink stream (listen start failed): {error}"
                    )))
                    .await;
                return false;
            }

            let _ = event_tx
                .send(UiGatewayEvent::OutgoingText {
                    kind: "listen".to_string(),
                    payload: to_redacted_pretty_json(&ClientTextMessage::listen_start_with_mode(
                        listen_mode,
                    )),
                })
                .await;

            let (capture, frame_rx) = match start_microphone_capture(
                event_tx.clone(),
                audio_routing.input_device_name.as_deref(),
                audio_routing.input_from_output,
                echo_canceller.cloned(),
            ) {
                Ok(capture_session) => capture_session,
                Err(error) => {
                    let _ = event_tx
                        .send(UiGatewayEvent::Error(format!(
                            "Failed to start microphone capture: {error}"
                        )))
                        .await;
                    if let Err(stop_error) = client.send_listen_stop_with_mode(listen_mode).await {
                        let _ = event_tx
                            .send(UiGatewayEvent::Error(format!(
                                "Failed to rollback listen stop after microphone init failure: {stop_error}"
                            )))
                            .await;
                        return false;
                    }

                    let _ = event_tx
                        .send(UiGatewayEvent::OutgoingText {
                            kind: "listen".to_string(),
                            payload: to_redacted_pretty_json(
                                &ClientTextMessage::listen_stop_with_mode(listen_mode),
                            ),
                        })
                        .await;
                    return true;
                }
            };

            let capture_description = capture.description().to_string();
            *microphone_capture = Some(capture);
            *microphone_frame_rx = frame_rx;
            *uplink_streaming = true;
            let _ = event_tx
                .send(UiGatewayEvent::UplinkStreamStateChanged(true))
                .await;
            let _ = event_tx
                .send(UiGatewayEvent::SystemNotice(format!(
                    "Microphone capture ready: {capture_description}"
                )))
                .await;
            true
        }
        GatewayCommand::StopUplinkStream => {
            if !*uplink_streaming {
                return true;
            }

            stop_uplink_capture(
                uplink_streaming,
                microphone_capture,
                microphone_frame_rx,
                event_tx,
            );

            if let Err(error) = client.send_listen_stop_with_mode(listen_mode).await {
                let _ = event_tx
                    .send(UiGatewayEvent::Error(format!(
                        "Failed to stop uplink stream (listen stop failed): {error}"
                    )))
                    .await;
                return false;
            }

            let _ = event_tx
                .send(UiGatewayEvent::OutgoingText {
                    kind: "listen".to_string(),
                    payload: to_redacted_pretty_json(&ClientTextMessage::listen_stop_with_mode(
                        listen_mode,
                    )),
                })
                .await;
            true
        }
        GatewayCommand::SetSpeakerOutputEnabled(_) => true,
        GatewayCommand::SetAecEnabled(_) => true,
        GatewayCommand::SetListenMode(_) => true,
        GatewayCommand::RefreshMcpTools => true,
    }
}

struct MicrophoneCapture {
    _stream: Stream,
    description: String,
}

impl MicrophoneCapture {
    fn new(stream: Stream, description: String) -> Self {
        Self {
            _stream: stream,
            description,
        }
    }

    fn description(&self) -> &str {
        &self.description
    }
}

struct RealtimeEchoCanceller {
    processor: VoipAec3,
    render_pending: VecDeque<f32>,
    render_frame: Vec<f32>,
    capture_output: Vec<f32>,
    capture_callback_delay_ms: u32,
    playback_callback_delay_ms: u32,
    playback_buffer_delay_ms: u32,
    stream_delay_ms: u32,
    processor_delay_ms: i32,
    erl_db: f32,
    erle_db: f32,
}

// SAFETY: RealtimeEchoCanceller is always accessed behind std::sync::Mutex in
// SharedEchoCanceller, so there is no concurrent access to the internal AEC3 state.
unsafe impl Send for RealtimeEchoCanceller {}

impl RealtimeEchoCanceller {
    fn new() -> Result<Self, String> {
        let processor = VoipAec3::builder(AUDIO_SAMPLE_RATE_HZ as usize, 1, 1)
            .initial_delay_ms(AEC_INITIAL_DELAY_MS)
            .enable_high_pass(true)
            .build()
            .map_err(|error| format!("create AEC3 processor failed: {error}"))?;

        let render_frame = vec![0.0_f32; processor.render_frame_samples()];
        let capture_output = vec![0.0_f32; processor.capture_frame_samples()];

        Ok(Self {
            processor,
            render_pending: VecDeque::new(),
            render_frame,
            capture_output,
            capture_callback_delay_ms: 0,
            playback_callback_delay_ms: 0,
            playback_buffer_delay_ms: 0,
            stream_delay_ms: AEC_INITIAL_DELAY_MS.max(0) as u32,
            processor_delay_ms: 0,
            erl_db: 0.0,
            erle_db: 0.0,
        })
    }

    fn feed_render_samples(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        self.render_pending.extend(samples.iter().copied());
        let frame_samples = self.processor.render_frame_samples();

        while self.render_pending.len() >= frame_samples {
            self.render_frame.clear();
            for _ in 0..frame_samples {
                if let Some(sample) = self.render_pending.pop_front() {
                    self.render_frame.push(sample);
                }
            }

            if self
                .processor
                .handle_render_frame(&self.render_frame)
                .is_err()
            {
                break;
            }
        }
    }

    fn process_capture_frame_in_place(&mut self, capture_frame: &mut [f32]) {
        if capture_frame.len() != self.processor.capture_frame_samples() {
            return;
        }

        if let Ok(metrics) =
            self.processor
                .process_capture_frame(capture_frame, false, &mut self.capture_output)
        {
            self.processor_delay_ms = metrics.delay_ms;
            self.erl_db = metrics.echo_return_loss as f32;
            self.erle_db = metrics.echo_return_loss_enhancement as f32;
            capture_frame.copy_from_slice(&self.capture_output);
        }
    }

    fn update_capture_callback_delay_ms(&mut self, delay_ms: u32) {
        self.capture_callback_delay_ms = smooth_delay(self.capture_callback_delay_ms, delay_ms);
        self.apply_stream_delay();
    }

    fn update_playback_callback_delay_ms(&mut self, delay_ms: u32) {
        self.playback_callback_delay_ms = smooth_delay(self.playback_callback_delay_ms, delay_ms);
        self.apply_stream_delay();
    }

    fn update_playback_buffer_delay_ms(&mut self, delay_ms: u32) {
        self.playback_buffer_delay_ms = smooth_delay(self.playback_buffer_delay_ms, delay_ms);
        self.apply_stream_delay();
    }

    fn snapshot(&self) -> AecStatsSnapshot {
        AecStatsSnapshot {
            stream_delay_ms: self.stream_delay_ms,
            capture_callback_delay_ms: self.capture_callback_delay_ms,
            playback_callback_delay_ms: self.playback_callback_delay_ms,
            playback_buffer_delay_ms: self.playback_buffer_delay_ms,
            processor_delay_ms: self.processor_delay_ms,
            erl_db: self.erl_db,
            erle_db: self.erle_db,
        }
    }

    fn apply_stream_delay(&mut self) {
        let delay_ms = self
            .capture_callback_delay_ms
            .saturating_add(self.playback_callback_delay_ms)
            .saturating_add(self.playback_buffer_delay_ms)
            .clamp(0, 2_000);

        if self.stream_delay_ms.abs_diff(delay_ms) >= 1 {
            self.stream_delay_ms = delay_ms;
            self.processor.set_audio_buffer_delay(delay_ms as i32);
        }
    }
}

#[derive(Clone)]
struct SharedEchoCanceller {
    inner: Arc<Mutex<RealtimeEchoCanceller>>,
}

impl SharedEchoCanceller {
    fn new() -> Result<Self, String> {
        Ok(Self {
            inner: Arc::new(Mutex::new(RealtimeEchoCanceller::new()?)),
        })
    }

    fn feed_render_samples(&self, samples: &[f32]) {
        if let Ok(mut processor) = self.inner.lock() {
            processor.feed_render_samples(samples);
        }
    }

    fn process_capture_frame_in_place(&self, capture_frame: &mut [f32]) {
        if let Ok(mut processor) = self.inner.try_lock() {
            processor.process_capture_frame_in_place(capture_frame);
        }
    }

    fn update_capture_callback_delay_ms(&self, delay_ms: u32) {
        if let Ok(mut processor) = self.inner.try_lock() {
            processor.update_capture_callback_delay_ms(delay_ms);
        }
    }

    fn update_playback_callback_delay_ms(&self, delay_ms: u32) {
        if let Ok(mut processor) = self.inner.try_lock() {
            processor.update_playback_callback_delay_ms(delay_ms);
        }
    }

    fn update_playback_buffer_delay_ms(&self, delay_ms: u32) {
        if let Ok(mut processor) = self.inner.try_lock() {
            processor.update_playback_buffer_delay_ms(delay_ms);
        }
    }

    fn snapshot(&self) -> Option<AecStatsSnapshot> {
        self.inner
            .try_lock()
            .ok()
            .map(|processor| processor.snapshot())
    }
}

struct MicFrameBuilder {
    channels: usize,
    input_sample_rate_hz: u32,
    resample_accumulator: f64,
    capture_samples: VecDeque<f32>,
    frame_samples: Vec<i16>,
    echo_canceller: Option<SharedEchoCanceller>,
    encoder: OpusPacketEncoder,
    event_tx: mpsc::Sender<UiGatewayEvent>,
    encode_error_reported: bool,
    drop_notice_reported: bool,
}

impl MicFrameBuilder {
    fn new(
        channels: usize,
        input_sample_rate_hz: u32,
        echo_canceller: Option<SharedEchoCanceller>,
        event_tx: mpsc::Sender<UiGatewayEvent>,
    ) -> Result<Self, String> {
        Ok(Self {
            channels,
            input_sample_rate_hz,
            resample_accumulator: 0.0,
            capture_samples: VecDeque::with_capacity(AEC_FRAME_SAMPLES * 4),
            frame_samples: Vec::with_capacity(AUDIO_FRAME_SAMPLES),
            echo_canceller,
            encoder: OpusPacketEncoder::new()?,
            event_tx,
            encode_error_reported: false,
            drop_notice_reported: false,
        })
    }

    fn process_f32(&mut self, data: &[f32], frame_tx: &mpsc::Sender<Vec<u8>>) {
        for frame in data.chunks(self.channels) {
            let mut mixed = 0.0_f32;
            for sample in frame {
                mixed += *sample;
            }
            mixed /= frame.len() as f32;
            self.push_sample(mixed, frame_tx);
        }
    }

    fn update_capture_callback_delay_ms(&self, delay_ms: u32) {
        if let Some(echo_canceller) = self.echo_canceller.as_ref() {
            echo_canceller.update_capture_callback_delay_ms(delay_ms);
        }
    }

    fn process_i16(&mut self, data: &[i16], frame_tx: &mpsc::Sender<Vec<u8>>) {
        for frame in data.chunks(self.channels) {
            let mut mixed = 0.0_f32;
            for sample in frame {
                mixed += *sample as f32 / i16::MAX as f32;
            }
            mixed /= frame.len() as f32;
            self.push_sample(mixed, frame_tx);
        }
    }

    fn process_u16(&mut self, data: &[u16], frame_tx: &mpsc::Sender<Vec<u8>>) {
        for frame in data.chunks(self.channels) {
            let mut mixed = 0.0_f32;
            for sample in frame {
                mixed += (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0;
            }
            mixed /= frame.len() as f32;
            self.push_sample(mixed, frame_tx);
        }
    }

    fn push_sample(&mut self, sample: f32, frame_tx: &mpsc::Sender<Vec<u8>>) {
        self.resample_accumulator += AUDIO_SAMPLE_RATE_HZ as f64;
        while self.resample_accumulator >= self.input_sample_rate_hz as f64 {
            self.resample_accumulator -= self.input_sample_rate_hz as f64;

            self.capture_samples.push_back(sample);
            if !self.process_capture_frames(frame_tx) {
                return;
            }
        }
    }

    fn process_capture_frames(&mut self, frame_tx: &mpsc::Sender<Vec<u8>>) -> bool {
        while self.capture_samples.len() >= AEC_FRAME_SAMPLES {
            let mut capture_frame = [0.0_f32; AEC_FRAME_SAMPLES];
            for sample in &mut capture_frame {
                if let Some(value) = self.capture_samples.pop_front() {
                    *sample = value;
                }
            }

            if let Some(echo_canceller) = self.echo_canceller.as_ref() {
                echo_canceller.process_capture_frame_in_place(&mut capture_frame);
            }

            for sample in capture_frame {
                self.frame_samples.push(float_to_pcm16(sample));
                if self.frame_samples.len() < AUDIO_FRAME_SAMPLES {
                    continue;
                }

                if !self.encode_and_send_frame(frame_tx) {
                    return false;
                }
            }
        }

        true
    }

    fn encode_and_send_frame(&mut self, frame_tx: &mpsc::Sender<Vec<u8>>) -> bool {
        let packet = match self.encoder.encode_pcm16(&self.frame_samples) {
            Ok(packet) => packet,
            Err(error) => {
                self.report_encode_error_once(error);
                self.frame_samples.clear();
                return false;
            }
        };
        self.frame_samples.clear();

        match frame_tx.try_send(packet) {
            Ok(_) => true,
            Err(TrySendError::Full(_)) => {
                self.report_drop_notice_once();
                true
            }
            Err(TrySendError::Closed(_)) => false,
        }
    }

    fn report_encode_error_once(&mut self, error: String) {
        if self.encode_error_reported {
            return;
        }

        self.encode_error_reported = true;
        try_emit_event(
            &self.event_tx,
            UiGatewayEvent::Error(format!("Opus encode error in microphone callback: {error}")),
        );
    }

    fn report_drop_notice_once(&mut self) {
        if self.drop_notice_reported {
            return;
        }

        self.drop_notice_reported = true;
        try_emit_event(
            &self.event_tx,
            UiGatewayEvent::SystemNotice(
                "Microphone frame queue is full, dropping upstream audio packets".to_string(),
            ),
        );
    }
}

struct OpusPacketEncoder {
    encoder: OpusEncoder,
    output_buffer: Vec<u8>,
}

impl OpusPacketEncoder {
    fn new() -> Result<Self, String> {
        let mut encoder = OpusEncoder::new(
            AUDIO_SAMPLE_RATE_HZ,
            OpusChannels::Mono,
            OpusApplication::Voip,
        )
        .map_err(|error| format!("create opus encoder failed: {error}"))?;

        encoder
            .set_bitrate(OpusBitrate::Bits(OPUS_BITRATE_BPS))
            .map_err(|error| format!("set opus bitrate failed: {error}"))?;
        encoder
            .set_complexity(OPUS_COMPLEXITY)
            .map_err(|error| format!("set opus complexity failed: {error}"))?;
        encoder
            .set_dtx(true)
            .map_err(|error| format!("enable opus DTX failed: {error}"))?;

        Ok(Self {
            encoder,
            output_buffer: vec![0_u8; OPUS_MAX_PACKET_BYTES],
        })
    }

    fn encode_pcm16(&mut self, samples: &[i16]) -> Result<Vec<u8>, String> {
        if samples.len() != AUDIO_FRAME_SAMPLES {
            return Err(format!(
                "invalid opus frame sample size: expected {}, got {}",
                AUDIO_FRAME_SAMPLES,
                samples.len()
            ));
        }

        let packet_len = self
            .encoder
            .encode(samples, &mut self.output_buffer)
            .map_err(|error| format!("opus encode failed: {error}"))?;

        Ok(self.output_buffer[..packet_len].to_vec())
    }
}

struct DownlinkAudioPlayer {
    _stream: Stream,
    description: String,
    output_sample_rate_hz: u32,
    output_channels: usize,
    output_buffer: Arc<Mutex<VecDeque<f32>>>,
    max_buffer_samples: usize,
    decoder: OpusDecoder,
    decode_buffer: Vec<i16>,
    resample_accumulator: f64,
    echo_canceller: Option<SharedEchoCanceller>,
}

impl DownlinkAudioPlayer {
    fn new(
        event_tx: mpsc::Sender<UiGatewayEvent>,
        output_device_hint: Option<&str>,
        echo_canceller: Option<SharedEchoCanceller>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = select_output_device(&host, output_device_hint)?;

        let device_name = device
            .name()
            .unwrap_or_else(|_| "Unknown speaker".to_string());
        let output_config = device
            .default_output_config()
            .map_err(|error| format!("Cannot read default speaker config: {error}"))?;

        let output_channels = usize::from(output_config.channels());
        if output_channels == 0 {
            return Err("Speaker channel count is zero".to_string());
        }

        let output_sample_rate_hz = output_config.sample_rate().0;
        let sample_format = output_config.sample_format();
        let stream_config: cpal::StreamConfig = output_config.config();

        let max_buffer_samples = usize::try_from(output_sample_rate_hz)
            .unwrap_or(48_000)
            .saturating_mul(output_channels)
            .saturating_mul(DOWNLINK_BUFFER_SECONDS);
        let output_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(max_buffer_samples)));

        let stream = build_speaker_output_stream(
            &device,
            &stream_config,
            sample_format,
            output_buffer.clone(),
            echo_canceller.clone(),
            event_tx,
        )?;

        stream
            .play()
            .map_err(|error| format!("Failed to start speaker output stream: {error}"))?;

        let decoder = OpusDecoder::new(AUDIO_SAMPLE_RATE_HZ, OpusChannels::Mono)
            .map_err(|error| format!("create opus decoder failed: {error}"))?;

        let description = format!(
            "{} | {:?}, {}ch @ {}Hz <- {}Hz mono opus",
            device_name,
            sample_format,
            output_channels,
            output_sample_rate_hz,
            AUDIO_SAMPLE_RATE_HZ
        );

        Ok(Self {
            _stream: stream,
            description,
            output_sample_rate_hz,
            output_channels,
            output_buffer,
            max_buffer_samples,
            decoder,
            decode_buffer: vec![0_i16; OPUS_DECODE_MAX_SAMPLES],
            resample_accumulator: 0.0,
            echo_canceller,
        })
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn push_opus_packet(&mut self, packet: &[u8]) -> Result<(), String> {
        let decoded_samples = self
            .decoder
            .decode(packet, &mut self.decode_buffer, false)
            .map_err(|error| format!("opus decode failed: {error}"))?;

        if decoded_samples == 0 {
            return Ok(());
        }

        let mut decoded_mono_samples = Vec::with_capacity(decoded_samples);
        for &sample in &self.decode_buffer[..decoded_samples] {
            decoded_mono_samples.push(pcm16_to_float(sample));
        }

        if let Some(echo_canceller) = self.echo_canceller.as_ref() {
            echo_canceller.feed_render_samples(&decoded_mono_samples);
        }

        let mut output_samples = Vec::with_capacity(decoded_samples * self.output_channels);
        for mono in decoded_mono_samples {
            self.resample_accumulator += self.output_sample_rate_hz as f64;

            while self.resample_accumulator >= AUDIO_SAMPLE_RATE_HZ as f64 {
                self.resample_accumulator -= AUDIO_SAMPLE_RATE_HZ as f64;
                for _ in 0..self.output_channels {
                    output_samples.push(mono);
                }
            }
        }

        if output_samples.is_empty() {
            return Ok(());
        }

        let mut buffer = self
            .output_buffer
            .lock()
            .map_err(|_| "speaker output buffer lock poisoned".to_string())?;

        let required_len = buffer.len().saturating_add(output_samples.len());
        if required_len > self.max_buffer_samples {
            let drop_count = required_len - self.max_buffer_samples;
            for _ in 0..drop_count {
                let _ = buffer.pop_front();
            }
        }

        buffer.extend(output_samples);

        if let Some(echo_canceller) = self.echo_canceller.as_ref() {
            echo_canceller.update_playback_buffer_delay_ms(playback_buffer_delay_ms(
                buffer.len(),
                self.output_sample_rate_hz,
                self.output_channels,
            ));
        }

        Ok(())
    }
}

fn build_speaker_output_stream(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    output_buffer: Arc<Mutex<VecDeque<f32>>>,
    echo_canceller: Option<SharedEchoCanceller>,
    event_tx: mpsc::Sender<UiGatewayEvent>,
) -> Result<Stream, String> {
    let output_sample_rate_hz = stream_config.sample_rate.0;
    let output_channels = usize::from(stream_config.channels);

    let error_callback = move |error| {
        try_emit_event(
            &event_tx,
            UiGatewayEvent::Error(format!("Speaker stream runtime error: {error}")),
        );
    };

    match sample_format {
        SampleFormat::F32 => {
            let output_buffer = output_buffer.clone();
            let echo_canceller = echo_canceller.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [f32], info| {
                    let queued_samples = fill_output_buffer_f32(data, &output_buffer);
                    if let Some(echo_canceller) = echo_canceller.as_ref() {
                        echo_canceller
                            .update_playback_callback_delay_ms(output_callback_delay_ms(info));
                        echo_canceller.update_playback_buffer_delay_ms(playback_buffer_delay_ms(
                            queued_samples,
                            output_sample_rate_hz,
                            output_channels,
                        ));
                    }
                },
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let output_buffer = output_buffer.clone();
            let echo_canceller = echo_canceller.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], info| {
                    let queued_samples = fill_output_buffer_i16(data, &output_buffer);
                    if let Some(echo_canceller) = echo_canceller.as_ref() {
                        echo_canceller
                            .update_playback_callback_delay_ms(output_callback_delay_ms(info));
                        echo_canceller.update_playback_buffer_delay_ms(playback_buffer_delay_ms(
                            queued_samples,
                            output_sample_rate_hz,
                            output_channels,
                        ));
                    }
                },
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let output_buffer = output_buffer.clone();
            let echo_canceller = echo_canceller.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [u16], info| {
                    let queued_samples = fill_output_buffer_u16(data, &output_buffer);
                    if let Some(echo_canceller) = echo_canceller.as_ref() {
                        echo_canceller
                            .update_playback_callback_delay_ms(output_callback_delay_ms(info));
                        echo_canceller.update_playback_buffer_delay_ms(playback_buffer_delay_ms(
                            queued_samples,
                            output_sample_rate_hz,
                            output_channels,
                        ));
                    }
                },
                error_callback,
                None,
            )
        }
        other => {
            return Err(format!(
                "Unsupported speaker sample format: {other:?}. Expect f32/i16/u16"
            ));
        }
    }
    .map_err(|error| format!("Failed to build speaker output stream: {error}"))
}

fn fill_output_buffer_f32(output: &mut [f32], output_buffer: &Arc<Mutex<VecDeque<f32>>>) -> usize {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = 0.0;
        }
        return 0;
    };

    for sample in output.iter_mut() {
        *sample = buffer.pop_front().unwrap_or(0.0);
    }

    buffer.len()
}

fn fill_output_buffer_i16(output: &mut [i16], output_buffer: &Arc<Mutex<VecDeque<f32>>>) -> usize {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = 0;
        }
        return 0;
    };

    for sample in output.iter_mut() {
        let value = buffer.pop_front().unwrap_or(0.0);
        *sample = float_to_pcm16(value);
    }

    buffer.len()
}

fn fill_output_buffer_u16(output: &mut [u16], output_buffer: &Arc<Mutex<VecDeque<f32>>>) -> usize {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = u16::MAX / 2;
        }
        return 0;
    };

    for sample in output.iter_mut() {
        let value = buffer.pop_front().unwrap_or(0.0);
        *sample = float_to_u16(value);
    }

    buffer.len()
}

fn empty_audio_receiver() -> mpsc::Receiver<Vec<u8>> {
    let (_tx, rx) = mpsc::channel(1);
    rx
}

fn stop_uplink_capture(
    uplink_streaming: &mut bool,
    microphone_capture: &mut Option<MicrophoneCapture>,
    microphone_frame_rx: &mut mpsc::Receiver<Vec<u8>>,
    event_tx: &mpsc::Sender<UiGatewayEvent>,
) {
    if *uplink_streaming {
        *uplink_streaming = false;
        try_emit_event(event_tx, UiGatewayEvent::UplinkStreamStateChanged(false));
    }

    if let Some(capture) = microphone_capture.as_ref() {
        try_emit_event(
            event_tx,
            UiGatewayEvent::SystemNotice(format!(
                "Microphone capture closed: {}",
                capture.description()
            )),
        );
    }

    *microphone_capture = None;
    *microphone_frame_rx = empty_audio_receiver();
}

fn restart_microphone_capture_if_streaming(
    event_tx: &mpsc::Sender<UiGatewayEvent>,
    uplink_streaming: bool,
    microphone_capture: &mut Option<MicrophoneCapture>,
    microphone_frame_rx: &mut mpsc::Receiver<Vec<u8>>,
    audio_routing: &AudioRoutingConfig,
    echo_canceller: Option<SharedEchoCanceller>,
    notice: &str,
) -> Result<(), String> {
    if !uplink_streaming {
        return Ok(());
    }

    let (capture, frame_rx) = start_microphone_capture(
        event_tx.clone(),
        audio_routing.input_device_name.as_deref(),
        audio_routing.input_from_output,
        echo_canceller,
    )?;

    *microphone_capture = Some(capture);
    *microphone_frame_rx = frame_rx;
    try_emit_event(event_tx, UiGatewayEvent::SystemNotice(notice.to_string()));
    Ok(())
}

fn start_microphone_capture(
    event_tx: mpsc::Sender<UiGatewayEvent>,
    input_device_hint: Option<&str>,
    input_from_output_loopback: bool,
    echo_canceller: Option<SharedEchoCanceller>,
) -> Result<(MicrophoneCapture, mpsc::Receiver<Vec<u8>>), String> {
    let host = cpal::default_host();
    let device = select_input_device(&host, input_device_hint, input_from_output_loopback)?;

    let device_name = device
        .name()
        .unwrap_or_else(|_| "Unknown microphone".to_string());
    let input_config = device
        .default_input_config()
        .map_err(|error| format!("Cannot read default microphone config: {error}"))?;

    let channels = usize::from(input_config.channels());
    if channels == 0 {
        return Err("Microphone channel count is zero".to_string());
    }

    let sample_format = input_config.sample_format();
    let input_sample_rate_hz = input_config.sample_rate().0;
    let stream_config: cpal::StreamConfig = input_config.config();
    let (frame_tx, frame_rx) = mpsc::channel(MICROPHONE_FRAME_CHANNEL_CAPACITY);

    let error_tx = event_tx.clone();
    let error_callback = move |error| {
        try_emit_event(
            &error_tx,
            UiGatewayEvent::Error(format!("Microphone stream runtime error: {error}")),
        );
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder = MicFrameBuilder::new(
                channels,
                input_sample_rate_hz,
                echo_canceller.clone(),
                event_tx.clone(),
            )?;
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], info| {
                    frame_builder.update_capture_callback_delay_ms(input_callback_delay_ms(info));
                    frame_builder.process_f32(data, &frame_tx);
                },
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder = MicFrameBuilder::new(
                channels,
                input_sample_rate_hz,
                echo_canceller.clone(),
                event_tx.clone(),
            )?;
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], info| {
                    frame_builder.update_capture_callback_delay_ms(input_callback_delay_ms(info));
                    frame_builder.process_i16(data, &frame_tx);
                },
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder = MicFrameBuilder::new(
                channels,
                input_sample_rate_hz,
                echo_canceller.clone(),
                event_tx.clone(),
            )?;
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], info| {
                    frame_builder.update_capture_callback_delay_ms(input_callback_delay_ms(info));
                    frame_builder.process_u16(data, &frame_tx);
                },
                error_callback,
                None,
            )
        }
        other => {
            return Err(format!(
                "Unsupported microphone sample format: {other:?}. Expect f32/i16/u16"
            ));
        }
    }
    .map_err(|error| format!("Failed to build microphone input stream: {error}"))?;

    stream
        .play()
        .map_err(|error| format!("Failed to start microphone input stream: {error}"))?;

    let description = format!(
        "{} | {:?}, {}ch @ {}Hz -> {}Hz mono opus",
        device_name, sample_format, channels, input_sample_rate_hz, AUDIO_SAMPLE_RATE_HZ
    );

    Ok((MicrophoneCapture::new(stream, description), frame_rx))
}

fn list_input_device_names(host: &cpal::Host) -> Vec<String> {
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn list_output_device_names(host: &cpal::Host) -> Vec<String> {
    match host.output_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn default_input_device_name(host: &cpal::Host) -> Option<String> {
    host.default_input_device()
        .and_then(|device| device.name().ok())
}

fn default_output_device_name(host: &cpal::Host) -> Option<String> {
    host.default_output_device()
        .and_then(|device| device.name().ok())
}

fn should_mirror_selected_output_to_system(output_hint: Option<&str>) -> bool {
    let Some(hint) = output_hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    let host = cpal::default_host();
    let selected_name = select_output_device(&host, Some(hint))
        .ok()
        .and_then(|device| device.name().ok());
    let default_name = default_output_device_name(&host);

    match (selected_name, default_name) {
        (Some(selected), Some(default_device)) => !selected.eq_ignore_ascii_case(&default_device),
        (Some(_), None) => true,
        _ => true,
    }
}

fn select_input_device(
    host: &cpal::Host,
    hint: Option<&str>,
    from_output_loopback: bool,
) -> Result<cpal::Device, String> {
    if !from_output_loopback {
        return select_input_device_by_hint(host, hint);
    }

    let Some(output_hint) = hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("Loopback input source requires a selected output device".to_string());
    };

    if let Some(device) = find_input_device_by_hint(host, output_hint)? {
        return Ok(device);
    }

    let output_device = select_output_device(host, Some(output_hint))?;
    if output_device.default_input_config().is_ok() {
        return Ok(output_device);
    }

    let available_inputs = list_input_device_names(host);
    Err(format!(
        "Cannot capture output `{output_hint}` as input directly. Use a loopback-capable output (e.g. BlackHole) and ensure matching input exists. Available input devices: {}",
        join_device_names(&available_inputs)
    ))
}

fn select_input_device_by_hint(
    host: &cpal::Host,
    hint: Option<&str>,
) -> Result<cpal::Device, String> {
    let Some(hint) = hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return host
            .default_input_device()
            .ok_or_else(|| "No default microphone input device found".to_string());
    };

    if let Some(device) = find_input_device_by_hint(host, hint)? {
        return Ok(device);
    }

    let available_inputs = list_input_device_names(host);
    Err(format!(
        "Input device `{hint}` not found. Available input devices: {}",
        join_device_names(&available_inputs)
    ))
}

fn find_input_device_by_hint(
    host: &cpal::Host,
    hint: &str,
) -> Result<Option<cpal::Device>, String> {
    let mut exact_match = None;
    let mut fuzzy_match = None;
    let hint_lower = hint.to_ascii_lowercase();

    for device in host
        .input_devices()
        .map_err(|error| format!("Cannot enumerate input devices: {error}"))?
    {
        let name = device
            .name()
            .unwrap_or_else(|_| "<unknown-input-device>".to_string());

        if name.eq_ignore_ascii_case(hint) {
            exact_match = Some(device);
            continue;
        }

        if fuzzy_match.is_none() && name.to_ascii_lowercase().contains(&hint_lower) {
            fuzzy_match = Some(device);
        }
    }

    Ok(exact_match.or(fuzzy_match))
}

fn select_output_device(host: &cpal::Host, hint: Option<&str>) -> Result<cpal::Device, String> {
    let Some(hint) = hint.map(str::trim).filter(|value| !value.is_empty()) else {
        return host
            .default_output_device()
            .ok_or_else(|| "No default speaker output device found".to_string());
    };

    let mut exact_match = None;
    let mut fuzzy_match = None;
    let mut available = Vec::new();
    let hint_lower = hint.to_ascii_lowercase();

    for device in host
        .output_devices()
        .map_err(|error| format!("Cannot enumerate output devices: {error}"))?
    {
        let name = device
            .name()
            .unwrap_or_else(|_| "<unknown-output-device>".to_string());
        available.push(name.clone());

        if name.eq_ignore_ascii_case(hint) {
            exact_match = Some(device);
            continue;
        }

        if fuzzy_match.is_none() && name.to_ascii_lowercase().contains(&hint_lower) {
            fuzzy_match = Some(device);
        }
    }

    if let Some(device) = exact_match.or(fuzzy_match) {
        return Ok(device);
    }

    Err(format!(
        "Output device `{hint}` not found. Available output devices: {}",
        join_device_names(&available)
    ))
}

fn join_device_names(names: &[String]) -> String {
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    }
}

fn find_device_index_by_hint(devices: &[String], hint: &str) -> Option<usize> {
    if hint.is_empty() {
        return None;
    }

    if let Some(index) = devices
        .iter()
        .position(|name| name.eq_ignore_ascii_case(hint))
    {
        return Some(index);
    }

    let hint_lower = hint.to_ascii_lowercase();
    devices
        .iter()
        .position(|name| name.to_ascii_lowercase().contains(&hint_lower))
}

fn env_optional(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_bool_or_default(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .as_deref()
        .and_then(parse_env_bool)
        .unwrap_or(default)
}

fn parse_env_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn smooth_delay(previous_ms: u32, next_ms: u32) -> u32 {
    if previous_ms == 0 {
        return next_ms;
    }

    ((previous_ms as f32 * 0.78) + (next_ms as f32 * 0.22)).round() as u32
}

fn input_callback_delay_ms(info: &cpal::InputCallbackInfo) -> u32 {
    let timestamp = info.timestamp();
    duration_to_millis_saturated(
        timestamp
            .callback
            .duration_since(&timestamp.capture)
            .unwrap_or_default(),
    )
}

fn output_callback_delay_ms(info: &cpal::OutputCallbackInfo) -> u32 {
    let timestamp = info.timestamp();
    duration_to_millis_saturated(
        timestamp
            .callback
            .duration_since(&timestamp.playback)
            .unwrap_or_default(),
    )
}

fn playback_buffer_delay_ms(sample_count: usize, sample_rate_hz: u32, channels: usize) -> u32 {
    if sample_rate_hz == 0 || channels == 0 {
        return 0;
    }

    let frames = sample_count as f64 / channels as f64;
    let millis = frames * 1000.0 / sample_rate_hz as f64;
    millis.max(0.0).round().min(u32::MAX as f64) as u32
}

fn duration_to_millis_saturated(duration: Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
}

fn float_to_pcm16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32).round() as i16
}

fn float_to_u16(sample: f32) -> u16 {
    let normalized = (sample.clamp(-1.0, 1.0) + 1.0) * 0.5;
    (normalized * u16::MAX as f32).round() as u16
}

fn pcm16_to_float(sample: i16) -> f32 {
    (sample as f32 / i16::MAX as f32).clamp(-1.0, 1.0)
}

fn to_redacted_pretty_json<T: Serialize>(value: &T) -> String {
    let mut serialized = match serde_json::to_value(value) {
        Ok(value) => value,
        Err(error) => {
            return format!(
                "{{\"error\":\"json serialize failed\",\"detail\":\"{}\"}}",
                error
            );
        }
    };

    redact_sensitive_fields(&mut serialized);
    serde_json::to_string_pretty(&serialized).unwrap_or_else(|error| {
        format!(
            "{{\"error\":\"json pretty serialize failed\",\"detail\":\"{}\"}}",
            error
        )
    })
}

fn redact_sensitive_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, inner_value) in map.iter_mut() {
                if is_sensitive_field(key) {
                    *inner_value = Value::String("<redacted>".to_string());
                } else {
                    redact_sensitive_fields(inner_value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_sensitive_fields(item);
            }
        }
        _ => {}
    }
}

fn is_sensitive_field(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "token" | "authorization" | "auth")
        || normalized.ends_with("_token")
        || normalized.ends_with("-token")
}

#[cfg(test)]
mod tests {
    use super::{parse_env_bool, redact_sensitive_fields, to_redacted_pretty_json};
    use host_core::{ClientTextMessage, HelloMessage};

    #[test]
    fn outgoing_hello_payload_redacts_token() {
        let payload = to_redacted_pretty_json(&ClientTextMessage::hello(HelloMessage::new(
            "device-001",
            "host-desktop",
            "AA:BB",
            "token-demo",
        )));

        assert!(!payload.contains("token-demo"));
        assert!(payload.contains("\"token\": \"<redacted>\""));
        assert!(payload.contains("\"device_id\": \"device-001\""));
    }

    #[test]
    fn redaction_is_recursive_for_nested_fields() {
        let mut value = serde_json::json!({
            "payload": {
                "authorization": "Bearer abc",
                "meta": {
                    "refresh_token": "refresh-1"
                }
            }
        });

        redact_sensitive_fields(&mut value);

        assert_eq!(value["payload"]["authorization"], "<redacted>");
        assert_eq!(value["payload"]["meta"]["refresh_token"], "<redacted>");
    }

    #[test]
    fn env_bool_parser_accepts_common_values() {
        assert_eq!(parse_env_bool("1"), Some(true));
        assert_eq!(parse_env_bool("true"), Some(true));
        assert_eq!(parse_env_bool("YES"), Some(true));
        assert_eq!(parse_env_bool("on"), Some(true));

        assert_eq!(parse_env_bool("0"), Some(false));
        assert_eq!(parse_env_bool("false"), Some(false));
        assert_eq!(parse_env_bool("No"), Some(false));
        assert_eq!(parse_env_bool("off"), Some(false));

        assert_eq!(parse_env_bool(""), None);
        assert_eq!(parse_env_bool("maybe"), None);
    }
}
