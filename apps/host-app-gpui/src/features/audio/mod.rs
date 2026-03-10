pub mod view;

use gpui::{div, prelude::*, rgb, App, Context, SharedString, Window};
use gpui_component::select::{SearchableVec, SelectEvent, SelectGroup, SelectItem, SelectState};

use crate::app::shell::{ui_button_icon, ButtonIconTone, MeetingHostShell};
use crate::app::state::{AudioRoutingConfig, ConnectionState, GatewayCommand};
use crate::components::icon::IconName;

pub(crate) type InputSelectState = SelectState<SearchableVec<SelectGroup<AudioInputSelectItem>>>;
pub(crate) type OutputSelectState = SelectState<Vec<AudioOutputSelectItem>>;
pub(crate) type InputSelectEvent = SelectEvent<SearchableVec<SelectGroup<AudioInputSelectItem>>>;
pub(crate) type OutputSelectEvent = SelectEvent<Vec<AudioOutputSelectItem>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InputDeviceSelection {
    Input(usize),
    LoopbackOutput(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct AudioInputSelectItem {
    value: InputDeviceSelection,
    title: SharedString,
    icon: IconName,
}

#[derive(Clone, Debug)]
pub(crate) struct AudioOutputSelectItem {
    value: usize,
    title: SharedString,
    icon: IconName,
}

impl AudioInputSelectItem {
    fn microphone(index: usize, name: &str) -> Self {
        Self {
            value: InputDeviceSelection::Input(index),
            title: SharedString::from(name.to_string()),
            icon: audio_device_icon(name),
        }
    }

    fn loopback_output(index: usize, name: &str) -> Self {
        Self {
            value: InputDeviceSelection::LoopbackOutput(index),
            title: SharedString::from(format!("loopback: {name}")),
            icon: IconName::Cable,
        }
    }
}

impl AudioOutputSelectItem {
    fn new(index: usize, name: &str) -> Self {
        Self {
            value: index,
            title: SharedString::from(name.to_string()),
            icon: audio_device_icon(name),
        }
    }
}

impl SelectItem for AudioInputSelectItem {
    type Value = InputDeviceSelection;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let icon_tone = match self.value {
            InputDeviceSelection::Input(_) => ButtonIconTone::Neutral,
            InputDeviceSelection::LoopbackOutput(_) => ButtonIconTone::Info,
        };

        div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(ui_button_icon(self.icon, 12.0, icon_tone))
            .child(div().text_ellipsis().child(self.title.clone()))
    }
}

impl SelectItem for AudioOutputSelectItem {
    type Value = usize;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(ui_button_icon(self.icon, 12.0, ButtonIconTone::Neutral))
            .child(div().text_ellipsis().child(self.title.clone()))
    }
}

pub(crate) fn build_input_select_items(
    input_devices: &[String],
    output_devices: &[String],
) -> SearchableVec<SelectGroup<AudioInputSelectItem>> {
    let mut groups = Vec::new();

    if !input_devices.is_empty() {
        groups.push(
            SelectGroup::new("输入设备").items(
                input_devices
                    .iter()
                    .enumerate()
                    .map(|(index, name)| AudioInputSelectItem::microphone(index, name)),
            ),
        );
    }

    if !output_devices.is_empty() {
        groups.push(
            SelectGroup::new("输出回采 (loopback)").items(
                output_devices
                    .iter()
                    .enumerate()
                    .map(|(index, name)| AudioInputSelectItem::loopback_output(index, name)),
            ),
        );
    }

    SearchableVec::new(groups)
}

pub(crate) fn build_output_select_items(output_devices: &[String]) -> Vec<AudioOutputSelectItem> {
    output_devices
        .iter()
        .enumerate()
        .map(|(index, name)| AudioOutputSelectItem::new(index, name))
        .collect()
}

pub(crate) fn selected_input_selection(
    input_from_output: bool,
    selected_input_index: Option<usize>,
    selected_input_output_index: Option<usize>,
) -> Option<InputDeviceSelection> {
    if input_from_output {
        selected_input_output_index.map(InputDeviceSelection::LoopbackOutput)
    } else {
        selected_input_index.map(InputDeviceSelection::Input)
    }
}

impl MeetingHostShell {
    pub(crate) fn has_shared_audio_route_risk(&self) -> bool {
        let same_named_device = self
            .selected_input_device_name_raw()
            .zip(self.selected_output_device_name())
            .map(|(input, output)| input.eq_ignore_ascii_case(output))
            .unwrap_or(false);
        let same_loopback_route = self.input_from_output
            && matches!(
                (self.selected_input_output_index, self.selected_output_index),
                (Some(input_index), Some(output_index)) if input_index == output_index
            );

        same_named_device || same_loopback_route
    }

    pub(crate) fn enforce_aec_for_shared_audio_route(&mut self) {
        if !self.has_shared_audio_route_risk() {
            return;
        }

        self.aec_enabled_draft = true;
        if self.aec_enabled {
            return;
        }

        self.aec_enabled = true;
        if matches!(self.connection_state, ConnectionState::Connected) {
            if let Some(command_tx) = self.ws_command_tx.as_ref() {
                if command_tx
                    .try_send(GatewayCommand::SetAecEnabled(true))
                    .is_err()
                {
                    self.push_chat(
                        crate::app::state::ChatRole::Error,
                        "Error",
                        "Failed to sync forced AEC state to gateway worker",
                    );
                }
            }
        }

        self.push_chat(
            crate::app::state::ChatRole::System,
            "System",
            "输入/输出使用同一路由，已强制开启 AEC 以降低回声",
        );
    }

    pub(crate) fn build_audio_routing_config(&self) -> AudioRoutingConfig {
        AudioRoutingConfig {
            input_device_name: self.selected_input_device_name_raw().map(ToOwned::to_owned),
            input_from_output: self.input_from_output,
            output_device_name: self.selected_output_device_name().map(ToOwned::to_owned),
            speaker_output_enabled: self.speaker_output_enabled,
            aec_enabled: self.aec_enabled || self.has_shared_audio_route_risk(),
        }
    }

    pub(crate) fn handle_input_select_event(
        &mut self,
        event: &InputSelectEvent,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(selected) = event;
        let Some(selected) = selected.as_ref() else {
            return;
        };

        match selected {
            InputDeviceSelection::Input(index) => self.select_input_device_index(*index, cx),
            InputDeviceSelection::LoopbackOutput(index) => {
                self.select_input_from_output_index(*index, cx)
            }
        }
    }

    pub(crate) fn handle_output_select_event(
        &mut self,
        event: &OutputSelectEvent,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(selected) = event;
        let Some(selected) = selected.as_ref() else {
            return;
        };

        self.select_output_device_index(*selected, cx);
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
        self.announce_audio_route_change("Output device selected", cx);
    }

    fn announce_audio_route_change(&mut self, reason: &str, cx: &mut Context<Self>) {
        self.enforce_aec_for_shared_audio_route();
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
    let bars = 20usize;
    let active_bars = (level_percent.min(100) * bars) / 100;

    let mut row = div().flex().items_center().gap_0p5().h_4();
    for index in 0..bars {
        let bar_color = if !active || index >= active_bars {
            0x161f2e
        } else if index >= bars * 9 / 10 {
            0x8f2236
        } else if index >= bars * 7 / 10 {
            0xb7792a
        } else {
            0x12a596
        };

        row = row.child(div().w_1().h_full().rounded_sm().bg(rgb(bar_color)));
    }

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x65748b))
                .child(label.to_string()),
        )
        .child(row)
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
