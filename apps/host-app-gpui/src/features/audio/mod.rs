pub mod view;

use gpui::{prelude::*, Context};

use crate::app::shell::MeetingHostShell;
use crate::app::state::{AudioRoutingConfig, ConnectionState, GatewayCommand};
use crate::components::icon::IconName;
use crate::components::ui::level_meter;

impl MeetingHostShell {
    pub(crate) fn build_audio_routing_config(&self) -> AudioRoutingConfig {
        AudioRoutingConfig {
            input_device_name: self.selected_input_device_name_raw().map(ToOwned::to_owned),
            input_from_output: self.input_from_output,
            output_device_name: self.selected_output_device_name().map(ToOwned::to_owned),
            speaker_output_enabled: self.speaker_output_enabled,
        }
    }

    fn selected_input_device_name_raw(&self) -> Option<&str> {
        if self.input_from_output {
            self.selected_input_output_index
                .and_then(|index| self.output_devices.get(index))
                .map(String::as_str)
        } else {
            self.selected_input_index
                .and_then(|index| self.input_devices.get(index))
                .map(String::as_str)
        }
    }

    pub(crate) fn selected_input_device_label(&self) -> String {
        let label = self.selected_input_device_name_raw().unwrap_or("default");
        if self.input_from_output {
            format!("loopback:{label}")
        } else {
            label.to_string()
        }
    }

    pub(crate) fn selected_output_device_name(&self) -> Option<&str> {
        self.selected_output_index
            .and_then(|index| self.output_devices.get(index))
            .map(String::as_str)
    }

    pub(crate) fn toggle_input_dropdown(&mut self, cx: &mut Context<Self>) {
        self.show_input_dropdown = !self.show_input_dropdown;
        if self.show_input_dropdown {
            self.show_output_dropdown = false;
        }
        self.notify_views(cx);
    }

    pub(crate) fn toggle_output_dropdown(&mut self, cx: &mut Context<Self>) {
        self.show_output_dropdown = !self.show_output_dropdown;
        if self.show_output_dropdown {
            self.show_input_dropdown = false;
        }
        self.notify_views(cx);
    }

    pub(crate) fn close_audio_dropdowns(&mut self, cx: &mut Context<Self>) {
        if !self.show_input_dropdown && !self.show_output_dropdown {
            return;
        }

        self.show_input_dropdown = false;
        self.show_output_dropdown = false;
        self.notify_views(cx);
    }

    pub(crate) fn toggle_speaker_output(&mut self, cx: &mut Context<Self>) {
        self.speaker_output_enabled = !self.speaker_output_enabled;
        let state = if self.speaker_output_enabled {
            "Output playback enabled"
        } else {
            "Output playback paused"
        };

        if matches!(self.connection_state, ConnectionState::Connected) {
            if let Some(command_tx) = self.ws_command_tx.as_ref() {
                if command_tx
                    .try_send(GatewayCommand::SetSpeakerOutputEnabled(
                        self.speaker_output_enabled,
                    ))
                    .is_err()
                {
                    self.push_chat(
                        crate::app::state::ChatRole::Error,
                        "Error",
                        "Failed to sync output playback switch to gateway worker",
                    );
                }
            }
        }

        self.push_chat(crate::app::state::ChatRole::System, "System", state);
        self.notify_views(cx);
    }

    pub(crate) fn input_level_percent(&self) -> usize {
        if !matches!(self.connection_state, ConnectionState::Connected) || !self.uplink_streaming {
            return 0;
        }

        let pulse = self.uplink_audio_frames % 42;
        (32 + pulse).min(100)
    }

    pub(crate) fn output_level_percent(&self) -> usize {
        if !matches!(self.connection_state, ConnectionState::Connected)
            || !self.speaker_output_enabled
        {
            return 0;
        }

        let pulse = self.downlink_audio_frames % 46;
        (26 + pulse).min(100)
    }

    pub(crate) fn select_input_device_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.input_devices.get(index).is_none() {
            self.push_chat(
                crate::app::state::ChatRole::Error,
                "Error",
                "Selected input device index is invalid",
            );
            self.notify_views(cx);
            return;
        }

        self.selected_input_index = Some(index);
        self.input_from_output = false;
        self.selected_input_output_index = None;
        self.show_input_dropdown = false;
        self.announce_audio_route_change("Input device selected", cx);
    }

    pub(crate) fn select_input_from_output_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.output_devices.get(index).is_none() {
            self.push_chat(
                crate::app::state::ChatRole::Error,
                "Error",
                "Selected loopback output index is invalid",
            );
            self.notify_views(cx);
            return;
        }

        self.selected_input_output_index = Some(index);
        self.input_from_output = true;
        self.show_input_dropdown = false;
        self.announce_audio_route_change("Input source switched to output loopback", cx);
    }

    pub(crate) fn select_output_device_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.output_devices.get(index).is_none() {
            self.push_chat(
                crate::app::state::ChatRole::Error,
                "Error",
                "Selected output device index is invalid",
            );
            self.notify_views(cx);
            return;
        }

        self.selected_output_index = Some(index);
        self.show_output_dropdown = false;
        self.announce_audio_route_change("Output device selected", cx);
    }

    fn announce_audio_route_change(&mut self, reason: &str, cx: &mut Context<Self>) {
        let input_name = self.selected_input_device_label();
        let output_name = self.selected_output_device_name().unwrap_or("default");
        let message = if matches!(self.connection_state, ConnectionState::Idle) {
            format!("{reason} -> input: {input_name}, output: {output_name}")
        } else {
            format!("{reason} -> input: {input_name}, output: {output_name} (reconnect to apply)")
        };

        self.push_chat(crate::app::state::ChatRole::System, "System", message);
        self.notify_views(cx);
    }
}

pub(crate) fn render_level_meter(
    label: &str,
    level_percent: usize,
    active: bool,
) -> impl IntoElement {
    level_meter(label, level_percent, active)
}

pub(crate) fn audio_device_icon(name: &str) -> IconName {
    let lower = name.to_ascii_lowercase();
    if lower.contains("blackhole") || lower.contains("loopback") || lower.contains("virtual") {
        IconName::Cable
    } else if lower.contains("airpods") || lower.contains("headphone") {
        IconName::Headphones
    } else {
        IconName::MonitorSpeaker
    }
}
