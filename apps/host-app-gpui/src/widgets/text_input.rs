use gpui::{Context, Window};
use gpui_component::input::InputEvent;

use crate::app::shell::MeetingHostShell;
use crate::app::state::{ChatRole, GatewayCommand};

impl MeetingHostShell {
    pub(crate) fn send_text_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.chat_input_state.read(cx).value().to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            self.push_chat(
                ChatRole::System,
                "System",
                "Type a text message first (click the input box)",
            );
            self.notify_views(cx);
            return;
        }

        self.push_chat(ChatRole::User, "You", text.clone());
        self.active_tts_message_index = None;
        self.active_stt_message_index = None;
        self.active_intent_trace_message_index = None;
        self.send_gateway_command(GatewayCommand::DetectText(text), cx);

        self.chat_input_state.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.notify_views(cx);
    }

    pub(crate) fn handle_chat_input_event(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.notify_views(cx);
            }
            InputEvent::PressEnter { secondary } => {
                if !secondary {
                    self.send_text_draft(window, cx);
                }
            }
            InputEvent::Focus => {
                self.chat_input_focused = true;
                self.notify_views(cx);
            }
            InputEvent::Blur => {
                self.chat_input_focused = false;
                self.notify_views(cx);
            }
        }
    }

    pub(crate) fn handle_ws_url_input_event(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.sync_ws_url_from_input(cx);
                self.notify_views(cx);
            }
            InputEvent::PressEnter { .. } => {
                self.sync_ws_url_from_input(cx);
                window.blur();
            }
            InputEvent::Focus => {
                self.ws_url_input_focused = true;
                self.notify_views(cx);
            }
            InputEvent::Blur => {
                self.ws_url_input_focused = false;
                self.sync_ws_url_from_input(cx);
                self.notify_views(cx);
            }
        }
    }

    pub(crate) fn handle_auth_token_input_event(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.sync_auth_token_from_input(cx);
                self.notify_views(cx);
            }
            InputEvent::PressEnter { .. } => {
                self.sync_auth_token_from_input(cx);
                window.blur();
            }
            InputEvent::Focus => {
                self.auth_token_input_focused = true;
                self.notify_views(cx);
            }
            InputEvent::Blur => {
                self.auth_token_input_focused = false;
                self.sync_auth_token_from_input(cx);
                self.notify_views(cx);
            }
        }
    }

    pub(crate) fn handle_device_id_input_event(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.sync_device_id_from_input(cx);
                self.notify_views(cx);
            }
            InputEvent::PressEnter { .. } => {
                self.sync_device_id_from_input(cx);
                window.blur();
            }
            InputEvent::Focus => {
                self.device_id_input_focused = true;
                self.notify_views(cx);
            }
            InputEvent::Blur => {
                self.device_id_input_focused = false;
                self.sync_device_id_from_input(cx);
                self.notify_views(cx);
            }
        }
    }

    pub(crate) fn handle_client_id_input_event(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.sync_client_id_from_input(cx);
                self.notify_views(cx);
            }
            InputEvent::PressEnter { .. } => {
                self.sync_client_id_from_input(cx);
                window.blur();
            }
            InputEvent::Focus => {
                self.client_id_input_focused = true;
                self.notify_views(cx);
            }
            InputEvent::Blur => {
                self.client_id_input_focused = false;
                self.sync_client_id_from_input(cx);
                self.notify_views(cx);
            }
        }
    }

    pub(crate) fn sync_ws_url_from_input(&mut self, cx: &mut Context<Self>) {
        self.ws_url_draft = self.ws_url_input_state.read(cx).value().to_string();
    }

    pub(crate) fn sync_auth_token_from_input(&mut self, cx: &mut Context<Self>) {
        self.auth_token_draft = self.auth_token_input_state.read(cx).value().to_string();
    }

    pub(crate) fn sync_device_id_from_input(&mut self, cx: &mut Context<Self>) {
        self.device_id_draft = self.device_id_input_state.read(cx).value().to_string();
    }

    pub(crate) fn sync_client_id_from_input(&mut self, cx: &mut Context<Self>) {
        self.client_id_draft = self.client_id_input_state.read(cx).value().to_string();
    }

    pub(crate) fn chat_input_has_text(&self, cx: &mut Context<Self>) -> bool {
        !self
            .chat_input_state
            .read(cx)
            .value()
            .as_ref()
            .trim()
            .is_empty()
    }
}
