use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use host_core::{ClientTextMessage, HelloMessage, AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE_HZ};
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

use crate::app::state::{AudioRoutingConfig, GatewayCommand, UiGatewayEvent};

pub const COMMAND_CHANNEL_CAPACITY: usize = 64;
pub const EVENT_CHANNEL_CAPACITY: usize = 512;

const HOST_INPUT_DEVICE_ENV: &str = "HOST_INPUT_DEVICE";
const HOST_OUTPUT_DEVICE_ENV: &str = "HOST_OUTPUT_DEVICE";
const OPUS_BITRATE_BPS: i32 = 16_000;
const OPUS_COMPLEXITY: i32 = 5;
const OPUS_MAX_PACKET_BYTES: usize = 4000;
const OPUS_DECODE_MAX_SAMPLES: usize = 4096;
const DOWNLINK_BUFFER_SECONDS: usize = 2;
const MICROPHONE_FRAME_CHANNEL_CAPACITY: usize = 64;

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
                    payload: to_redacted_pretty_json(&ClientTextMessage::hello(
                        HelloMessage::new(
                            config.device_id.clone(),
                            config.device_name.clone(),
                            config.device_mac.clone(),
                            config.token.clone(),
                        )
                        .with_intent_trace_notify(true),
                    )),
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

            let mut uplink_streaming = false;
            let mut speaker_output_enabled = audio_routing.speaker_output_enabled;
            let mut microphone_capture = None;
            let mut microphone_frame_rx = empty_audio_receiver();
            let mut downlink_player: Option<DownlinkAudioPlayer> = None;
            let mut downlink_playback_error_reported = false;
            let mut input_monitor_player: Option<DownlinkAudioPlayer> = None;
            let mut input_monitor_error_reported = false;
            let mut output_monitor_player: Option<DownlinkAudioPlayer> = None;
            let mut output_monitor_error_reported = false;
            let mirror_output_to_system =
                should_mirror_selected_output_to_system(audio_routing.output_device_name.as_deref());
            let mirror_input_to_system = true;

            loop {
                tokio::select! {
                    maybe_command = command_rx.recv() => {
                        let Some(command) = maybe_command else {
                            break;
                        };

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

                        let keep_running = handle_gateway_command(
                            command,
                            &mut client,
                            &event_tx,
                            &mut uplink_streaming,
                            &mut microphone_capture,
                            &mut microphone_frame_rx,
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
                            if let Err(error) = client.send_listen_stop().await {
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
                                    payload: to_redacted_pretty_json(&ClientTextMessage::listen_stop()),
                                })
                                .await;
                            continue;
                        };

                        if mirror_input_to_system && !input_monitor_error_reported {
                            if input_monitor_player.is_none() {
                                match DownlinkAudioPlayer::new(event_tx.clone(), None) {
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
                                let _ = event_tx.send(UiGatewayEvent::IncomingText(message)).await;
                            }
                            WsGatewayEvent::DownlinkAudio(data) => {
                                if speaker_output_enabled {
                                    if downlink_player.is_none() && !downlink_playback_error_reported {
                                        match DownlinkAudioPlayer::new(
                                            event_tx.clone(),
                                            audio_routing.output_device_name.as_deref(),
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
                                            match DownlinkAudioPlayer::new(event_tx.clone(), None) {
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

async fn handle_gateway_command(
    command: GatewayCommand,
    client: &mut WsGatewayClient,
    event_tx: &mpsc::Sender<UiGatewayEvent>,
    uplink_streaming: &mut bool,
    microphone_capture: &mut Option<MicrophoneCapture>,
    microphone_frame_rx: &mut mpsc::Receiver<Vec<u8>>,
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
            if let Err(error) = client.send_listen_detect_text(text.clone()).await {
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
                    payload: to_redacted_pretty_json(&ClientTextMessage::listen_detect_text(text)),
                })
                .await;
            true
        }
        GatewayCommand::StartUplinkStream => {
            if *uplink_streaming {
                return true;
            }

            if let Err(error) = client.send_listen_start().await {
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
                    payload: to_redacted_pretty_json(&ClientTextMessage::listen_start()),
                })
                .await;

            let (capture, frame_rx) = match start_microphone_capture(
                event_tx.clone(),
                audio_routing.input_device_name.as_deref(),
                audio_routing.input_from_output,
            ) {
                Ok(capture_session) => capture_session,
                Err(error) => {
                    let _ = event_tx
                        .send(UiGatewayEvent::Error(format!(
                            "Failed to start microphone capture: {error}"
                        )))
                        .await;
                    if let Err(stop_error) = client.send_listen_stop().await {
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
                            payload: to_redacted_pretty_json(&ClientTextMessage::listen_stop()),
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

            if let Err(error) = client.send_listen_stop().await {
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
                    payload: to_redacted_pretty_json(&ClientTextMessage::listen_stop()),
                })
                .await;
            true
        }
        GatewayCommand::SetSpeakerOutputEnabled(_) => true,
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

struct MicFrameBuilder {
    channels: usize,
    input_sample_rate_hz: u32,
    resample_accumulator: f64,
    frame_samples: Vec<i16>,
    encoder: OpusPacketEncoder,
    event_tx: mpsc::Sender<UiGatewayEvent>,
    encode_error_reported: bool,
    drop_notice_reported: bool,
}

impl MicFrameBuilder {
    fn new(
        channels: usize,
        input_sample_rate_hz: u32,
        event_tx: mpsc::Sender<UiGatewayEvent>,
    ) -> Result<Self, String> {
        Ok(Self {
            channels,
            input_sample_rate_hz,
            resample_accumulator: 0.0,
            frame_samples: Vec::with_capacity(AUDIO_FRAME_SAMPLES),
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

            self.frame_samples.push(float_to_pcm16(sample));
            if self.frame_samples.len() < AUDIO_FRAME_SAMPLES {
                continue;
            }

            let packet = match self.encoder.encode_pcm16(&self.frame_samples) {
                Ok(packet) => packet,
                Err(error) => {
                    self.report_encode_error_once(error);
                    self.frame_samples.clear();
                    return;
                }
            };
            self.frame_samples.clear();

            match frame_tx.try_send(packet) {
                Ok(_) => {}
                Err(TrySendError::Full(_)) => {
                    self.report_drop_notice_once();
                }
                Err(TrySendError::Closed(_)) => {
                    return;
                }
            }
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
}

impl DownlinkAudioPlayer {
    fn new(
        event_tx: mpsc::Sender<UiGatewayEvent>,
        output_device_hint: Option<&str>,
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

        let mut output_samples = Vec::with_capacity(decoded_samples * self.output_channels);
        for &sample in &self.decode_buffer[..decoded_samples] {
            let mono = pcm16_to_float(sample);
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
        Ok(())
    }
}

fn build_speaker_output_stream(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    output_buffer: Arc<Mutex<VecDeque<f32>>>,
    event_tx: mpsc::Sender<UiGatewayEvent>,
) -> Result<Stream, String> {
    let error_callback = move |error| {
        try_emit_event(
            &event_tx,
            UiGatewayEvent::Error(format!("Speaker stream runtime error: {error}")),
        );
    };

    match sample_format {
        SampleFormat::F32 => {
            let output_buffer = output_buffer.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [f32], _| fill_output_buffer_f32(data, &output_buffer),
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let output_buffer = output_buffer.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], _| fill_output_buffer_i16(data, &output_buffer),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let output_buffer = output_buffer.clone();
            device.build_output_stream(
                stream_config,
                move |data: &mut [u16], _| fill_output_buffer_u16(data, &output_buffer),
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

fn fill_output_buffer_f32(output: &mut [f32], output_buffer: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = 0.0;
        }
        return;
    };

    for sample in output.iter_mut() {
        *sample = buffer.pop_front().unwrap_or(0.0);
    }
}

fn fill_output_buffer_i16(output: &mut [i16], output_buffer: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = 0;
        }
        return;
    };

    for sample in output.iter_mut() {
        let value = buffer.pop_front().unwrap_or(0.0);
        *sample = float_to_pcm16(value);
    }
}

fn fill_output_buffer_u16(output: &mut [u16], output_buffer: &Arc<Mutex<VecDeque<f32>>>) {
    let Ok(mut buffer) = output_buffer.lock() else {
        for sample in output.iter_mut() {
            *sample = u16::MAX / 2;
        }
        return;
    };

    for sample in output.iter_mut() {
        let value = buffer.pop_front().unwrap_or(0.0);
        *sample = float_to_u16(value);
    }
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

fn start_microphone_capture(
    event_tx: mpsc::Sender<UiGatewayEvent>,
    input_device_hint: Option<&str>,
    input_from_output_loopback: bool,
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
            let mut frame_builder =
                MicFrameBuilder::new(channels, input_sample_rate_hz, event_tx.clone())?;
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| frame_builder.process_f32(data, &frame_tx),
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder =
                MicFrameBuilder::new(channels, input_sample_rate_hz, event_tx.clone())?;
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| frame_builder.process_i16(data, &frame_tx),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let frame_tx = frame_tx.clone();
            let mut frame_builder =
                MicFrameBuilder::new(channels, input_sample_rate_hz, event_tx.clone())?;
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| frame_builder.process_u16(data, &frame_tx),
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
    use super::{redact_sensitive_fields, to_redacted_pretty_json};
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
}
