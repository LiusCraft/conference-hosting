pub mod panel;
pub mod view;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{px, Context, SharedString};
use host_core::InboundTextMessage;
use serde_json::{Map, Value};

use crate::app::shell::MeetingHostShell;
use crate::app::state::{
    ChatMessage, ChatRole, CHAT_BOTTOM_EPSILON_PX, MAX_CHAT_MESSAGES, TTS_APPEND_REUSE_WINDOW_MS,
};

const RESPONSE_LATENCY_PENDING_TIMEOUT: Duration = Duration::from_secs(120);

impl MeetingHostShell {
    pub(crate) fn push_inbound_message(&mut self, message: InboundTextMessage) {
        if self.handle_tts_inbound_message(&message) {
            return;
        }

        if self.handle_llm_inbound_message(&message) {
            return;
        }

        if self.handle_intent_trace_inbound_message(&message) {
            return;
        }

        let Some((role, title, body)) = describe_inbound_message(&message) else {
            return;
        };

        if role == ChatRole::Assistant && self.is_duplicate_assistant_message(body.as_str()) {
            return;
        }

        self.maybe_track_stt_response_latency_anchor(&message, role, title.as_str());

        if role == ChatRole::User && title == "STT" {
            self.upsert_stt_transcript(body.as_str());
            return;
        }

        let response_latency_ms = if role == ChatRole::Assistant {
            self.consume_response_latency(&message)
        } else {
            None
        };

        self.push_chat_with_metadata(role, title, body, response_latency_ms);
    }

    fn handle_intent_trace_inbound_message(&mut self, message: &InboundTextMessage) -> bool {
        if message.message_type != "notify" {
            return false;
        }

        if read_string_field(&message.payload, "event") != Some("intent_trace") {
            return false;
        }

        let trace_turn_key = extract_intent_trace_turn_key(&message.payload);
        self.upsert_intent_trace_item(trace_turn_key.as_deref(), &message.payload);
        true
    }

    fn upsert_intent_trace_item(&mut self, turn_key: Option<&str>, payload: &Map<String, Value>) {
        if self.append_to_active_intent_trace_message(turn_key, payload) {
            return;
        }

        let first_line = format_intent_trace_line(payload, 1);
        self.push_chat_with_metadata(ChatRole::Trace, "调用链路", first_line, None);
        if let Some(message) = self.chat_messages.first_mut() {
            message.trace_turn_key = turn_key.map(|key| key.to_string().into());
        }
        self.active_intent_trace_message_index = Some(0);
    }

    fn append_to_active_intent_trace_message(
        &mut self,
        turn_key: Option<&str>,
        payload: &Map<String, Value>,
    ) -> bool {
        self.sync_chat_follow_state();

        let Some(index) = self.active_intent_trace_message_index else {
            return false;
        };

        let Some(message) = self.chat_messages.get_mut(index) else {
            self.active_intent_trace_message_index = None;
            return false;
        };

        let message_turn_key = message.trace_turn_key.as_ref().map(SharedString::as_ref);
        if message.role != ChatRole::Trace || !is_same_intent_trace_turn(message_turn_key, turn_key)
        {
            self.active_intent_trace_message_index = None;
            return false;
        }

        let step_index = message
            .body
            .as_ref()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            .saturating_add(1);
        let next_line = format_intent_trace_line(payload, step_index);
        let updated_body = format!("{}\n{}", message.body, next_line);
        message.body = updated_body.into();

        if step_index == 2 && !message.trace_collapsed {
            message.trace_collapsed = true;
        }

        if self.follow_latest_chat_messages {
            self.chat_scroll.scroll_to_bottom();
        }

        true
    }

    fn handle_llm_inbound_message(&mut self, message: &InboundTextMessage) -> bool {
        if message.message_type != "llm" {
            return false;
        }

        let Some(text) = read_string_field(&message.payload, "text")
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            return true;
        };

        if is_llm_emotion_placeholder(text) {
            if !self.show_ai_emotion_messages || self.is_duplicate_assistant_message(text) {
                return true;
            }

            self.push_chat(ChatRole::Assistant, "AI Emotion", text.to_string());
            return true;
        }

        self.active_stt_message_index = None;

        if self.is_duplicate_assistant_message(text) {
            return true;
        }

        self.active_intent_trace_message_index = None;
        let response_latency_ms = self.consume_response_latency(message);
        self.push_chat_with_metadata(
            ChatRole::Assistant,
            "AI",
            text.to_string(),
            response_latency_ms,
        );

        true
    }

    fn handle_tts_inbound_message(&mut self, message: &InboundTextMessage) -> bool {
        if message.message_type != "tts" {
            return false;
        }

        let state = read_string_field(&message.payload, "state").unwrap_or("unknown");
        match state {
            "start" => {
                self.active_tts_message_index = None;
                self.active_stt_message_index = None;
            }
            "sentence_start" => {
                let Some(text) = read_string_field(&message.payload, "text")
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                else {
                    return true;
                };

                self.active_stt_message_index = None;
                self.active_intent_trace_message_index = None;
                let response_latency_ms = self.consume_response_latency(message);
                self.upsert_tts_sentence_text(text, response_latency_ms);
            }
            "stop" => {
                self.active_tts_message_index = None;
                self.active_stt_message_index = None;
                self.active_intent_trace_message_index = None;
                if read_bool_field(&message.payload, "is_aborted") == Some(true) {
                    self.push_chat(ChatRole::System, "System", "TTS playback aborted");
                }
            }
            _ => {}
        }

        true
    }

    fn upsert_stt_transcript(&mut self, transcript: &str) {
        if self.append_to_active_stt_transcript(transcript) {
            return;
        }

        self.push_chat(ChatRole::User, "STT", transcript.to_string());
        self.active_stt_message_index = Some(0);
    }

    fn append_to_active_stt_transcript(&mut self, transcript: &str) -> bool {
        self.sync_chat_follow_state();

        let Some(index) = self.active_stt_message_index else {
            return false;
        };

        let Some(message) = self.chat_messages.get_mut(index) else {
            self.active_stt_message_index = None;
            return false;
        };

        if message.role != ChatRole::User || message.title.as_ref() != "STT" {
            self.active_stt_message_index = None;
            return false;
        }

        let merged = merge_streaming_text(message.body.as_ref(), transcript);
        if merged != message.body.as_ref() {
            message.body = merged.into();
        }

        if self.follow_latest_chat_messages {
            self.chat_scroll.scroll_to_bottom();
        }

        true
    }

    fn upsert_tts_sentence_text(&mut self, text: &str, response_latency_ms: Option<u64>) {
        self.sync_chat_follow_state();

        if self.append_to_active_tts_message(text, response_latency_ms) {
            return;
        }

        let now_ms = unix_now_millis();
        if let Some(latest_message) = self.chat_messages.first_mut() {
            let message_is_recent = now_ms.saturating_sub(latest_message.created_at_unix_ms)
                <= TTS_APPEND_REUSE_WINDOW_MS;
            if latest_message.role == ChatRole::Assistant
                && latest_message.title.as_ref() == "AI"
                && message_is_recent
            {
                latest_message.body = text.to_string().into();
                if latest_message.response_latency_ms.is_none() {
                    latest_message.response_latency_ms = response_latency_ms;
                }
                self.active_tts_message_index = Some(0);
                if self.follow_latest_chat_messages {
                    self.chat_scroll.scroll_to_bottom();
                }
                return;
            }
        }

        self.push_chat_with_metadata(
            ChatRole::Assistant,
            "AI",
            text.to_string(),
            response_latency_ms,
        );
        self.active_tts_message_index = Some(0);
    }

    fn append_to_active_tts_message(
        &mut self,
        text: &str,
        response_latency_ms: Option<u64>,
    ) -> bool {
        self.sync_chat_follow_state();

        let Some(index) = self.active_tts_message_index else {
            return false;
        };

        let Some(message) = self.chat_messages.get_mut(index) else {
            self.active_tts_message_index = None;
            return false;
        };

        if message.role != ChatRole::Assistant || message.title.as_ref() != "AI" {
            self.active_tts_message_index = None;
            return false;
        }

        if !message.body.as_ref().ends_with(text) {
            let mut merged = message.body.to_string();
            merged.push_str(text);
            message.body = merged.into();
        }

        if message.response_latency_ms.is_none() {
            message.response_latency_ms = response_latency_ms;
        }

        if self.follow_latest_chat_messages {
            self.chat_scroll.scroll_to_bottom();
        }

        true
    }

    fn consume_response_latency(&mut self, message: &InboundTextMessage) -> Option<u64> {
        if !is_assistant_text_message(message) {
            return None;
        }

        let now = Instant::now();
        while self
            .pending_detect_requests
            .front()
            .is_some_and(|started_at| {
                now.duration_since(*started_at) > RESPONSE_LATENCY_PENDING_TIMEOUT
            })
        {
            self.pending_detect_requests.pop_front();
        }

        self.pending_detect_requests
            .pop_front()
            .map(|request_started_at| duration_to_millis(now.duration_since(request_started_at)))
    }

    fn maybe_track_stt_response_latency_anchor(
        &mut self,
        message: &InboundTextMessage,
        role: ChatRole,
        title: &str,
    ) {
        if role != ChatRole::User || title != "STT" {
            return;
        }

        if !should_track_stt_response_latency(&message.payload) {
            return;
        }

        self.pending_detect_requests.push_back(Instant::now());
    }

    fn is_duplicate_assistant_message(&self, body: &str) -> bool {
        let Some(latest_message) = self.chat_messages.first() else {
            return false;
        };

        latest_message.role == ChatRole::Assistant && latest_message.body.as_ref() == body
    }

    fn is_chat_scrolled_to_bottom(&self) -> bool {
        let max_offset = self.chat_scroll.max_offset().height;
        let current_offset = self.chat_scroll.offset().y;
        let distance_to_bottom = (current_offset + max_offset).abs();
        distance_to_bottom <= px(CHAT_BOTTOM_EPSILON_PX)
    }

    pub(crate) fn sync_chat_follow_state(&mut self) {
        let at_bottom = self.is_chat_scrolled_to_bottom();
        if at_bottom {
            self.follow_latest_chat_messages = true;
            self.pending_chat_messages = 0;
            self.has_pending_chat_messages = false;
        }

        if !at_bottom
            && self.follow_latest_chat_messages
            && self.chat_scroll.max_offset().height > px(0.0)
        {
            self.follow_latest_chat_messages = false;
        }
    }

    pub(crate) fn jump_to_latest_chat_messages(&mut self, cx: &mut Context<Self>) {
        self.render_full_chat_history = false;
        self.follow_latest_chat_messages = true;
        self.pending_chat_messages = 0;
        self.has_pending_chat_messages = false;
        self.chat_scroll.scroll_to_bottom();
        self.notify_views(cx);
    }

    pub(crate) fn expand_full_chat_history(&mut self, cx: &mut Context<Self>) {
        self.render_full_chat_history = true;
        self.follow_latest_chat_messages = false;
        self.pending_chat_messages = 0;
        self.has_pending_chat_messages = false;
        self.notify_views(cx);
    }

    pub(crate) fn toggle_trace_message_collapse(
        &mut self,
        message_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(message) = self.chat_messages.get_mut(message_index) else {
            return;
        };

        if message.role != ChatRole::Trace {
            return;
        }

        let step_count = message
            .body
            .as_ref()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        if step_count <= 1 {
            return;
        }

        message.trace_collapsed = !message.trace_collapsed;
        self.notify_views(cx);
    }

    pub(crate) fn push_chat(
        &mut self,
        role: ChatRole,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
    ) {
        self.push_chat_with_metadata(role, title, body, None);
    }

    fn push_chat_with_metadata(
        &mut self,
        role: ChatRole,
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
        response_latency_ms: Option<u64>,
    ) {
        self.sync_chat_follow_state();

        if role == ChatRole::Assistant {
            self.active_stt_message_index = None;
        }

        if let Some(index) = self.active_tts_message_index {
            self.active_tts_message_index = index.checked_add(1);
        }

        if let Some(index) = self.active_stt_message_index {
            self.active_stt_message_index = index.checked_add(1);
        }

        if let Some(index) = self.active_intent_trace_message_index {
            self.active_intent_trace_message_index = index.checked_add(1);
        }

        self.chat_messages.insert(
            0,
            ChatMessage {
                role,
                title: title.into(),
                body: body.into(),
                created_at_unix_ms: unix_now_millis(),
                response_latency_ms,
                trace_turn_key: None,
                trace_collapsed: false,
            },
        );

        if self.chat_messages.len() > MAX_CHAT_MESSAGES {
            self.chat_messages.truncate(MAX_CHAT_MESSAGES);
        }

        if let Some(index) = self.active_tts_message_index {
            if index >= self.chat_messages.len() {
                self.active_tts_message_index = None;
            }
        }

        if let Some(index) = self.active_stt_message_index {
            if index >= self.chat_messages.len() {
                self.active_stt_message_index = None;
            }
        }

        if let Some(index) = self.active_intent_trace_message_index {
            if index >= self.chat_messages.len() {
                self.active_intent_trace_message_index = None;
            }
        }

        if self.follow_latest_chat_messages {
            self.pending_chat_messages = 0;
            self.has_pending_chat_messages = false;
            self.chat_scroll.scroll_to_bottom();
        } else {
            self.has_pending_chat_messages = true;
            if role == ChatRole::Assistant {
                self.pending_chat_messages = self.pending_chat_messages.saturating_add(1);
            }
        }
    }
}

pub(crate) fn wall_clock_label() -> String {
    let now = utc_datetime_parts_from_millis(unix_now_millis());
    format_hms(now.3, now.4, now.5)
}

pub(crate) fn load_history_chat_messages() -> Vec<ChatMessage> {
    let mut chat_messages = Vec::new();

    let persisted_history_records: Vec<ChatMessage> = Vec::new();
    insert_history_chat_messages(&mut chat_messages, persisted_history_records);

    chat_messages
}

fn insert_history_chat_messages(
    chat_messages: &mut Vec<ChatMessage>,
    mut history_messages: Vec<ChatMessage>,
) {
    if history_messages.is_empty() {
        return;
    }

    history_messages.sort_unstable_by_key(|message| message.created_at_unix_ms);
    history_messages.reverse();

    chat_messages.extend(history_messages);
    chat_messages
        .sort_unstable_by(|left, right| right.created_at_unix_ms.cmp(&left.created_at_unix_ms));

    if chat_messages.len() > MAX_CHAT_MESSAGES {
        chat_messages.truncate(MAX_CHAT_MESSAGES);
    }
}

fn describe_inbound_message(message: &InboundTextMessage) -> Option<(ChatRole, String, String)> {
    match message.message_type.as_str() {
        "hello" => None,
        "stt" => read_string_field(&message.payload, "text")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| (ChatRole::User, "STT".to_string(), text.to_string())),
        "llm" | "tts" => None,
        "audio" => None,
        "mcp" => Some((
            ChatRole::Tool,
            "Tool Call".to_string(),
            summarize_tool_message(&message.payload),
        )),
        "notify" => {
            let event_name = read_string_field(&message.payload, "event").unwrap_or("notify");
            if event_name == "intent_trace" {
                None
            } else {
                Some((
                    ChatRole::System,
                    "Notify".to_string(),
                    compact_json(&Value::Object(message.payload.clone())),
                ))
            }
        }
        _ => Some((
            ChatRole::System,
            format!("Server {}", message.message_type),
            compact_json(&Value::Object(message.payload.clone())),
        )),
    }
}

fn is_assistant_text_message(message: &InboundTextMessage) -> bool {
    match message.message_type.as_str() {
        "llm" => read_string_field(&message.payload, "text")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| !is_llm_emotion_placeholder(text))
            .unwrap_or(false),
        "tts" => {
            read_string_field(&message.payload, "state") == Some("sentence_start")
                && read_string_field(&message.payload, "text")
                    .map(str::trim)
                    .map(|text| !text.is_empty())
                    .unwrap_or(false)
        }
        _ => false,
    }
}

fn merge_streaming_text(existing: &str, incoming: &str) -> String {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return existing.to_string();
    }

    if existing.is_empty() {
        return incoming.to_string();
    }

    if existing.ends_with(incoming) {
        return existing.to_string();
    }

    if incoming.starts_with(existing) {
        return incoming.to_string();
    }

    if existing.starts_with(incoming) {
        return existing.to_string();
    }

    let overlap_chars = longest_suffix_prefix_overlap_chars(existing, incoming);
    if overlap_chars == 0 {
        return format!("{existing}{incoming}");
    }

    let split_index = byte_index_after_char_count(incoming, overlap_chars);
    format!("{existing}{}", &incoming[split_index..])
}

fn longest_suffix_prefix_overlap_chars(left: &str, right: &str) -> usize {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let max_overlap = left_chars.len().min(right_chars.len());

    for overlap in (1..=max_overlap).rev() {
        if left_chars[left_chars.len() - overlap..] == right_chars[..overlap] {
            return overlap;
        }
    }

    0
}

fn byte_index_after_char_count(text: &str, char_count: usize) -> usize {
    text.char_indices()
        .nth(char_count)
        .map(|(byte_index, _)| byte_index)
        .unwrap_or(text.len())
}

fn should_track_stt_response_latency(payload: &Map<String, Value>) -> bool {
    if read_bool_field(payload, "is_final") == Some(false) {
        return false;
    }

    let state = read_string_field(payload, "state")
        .map(str::trim)
        .filter(|state| !state.is_empty())
        .map(str::to_ascii_lowercase);

    !matches!(
        state.as_deref(),
        Some("partial" | "interim" | "listening" | "start" | "sentence_start")
    )
}

fn is_llm_emotion_placeholder(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut has_emoji = false;
    let mut emoji_unit_count = 0usize;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            continue;
        }

        if !is_emoji_component(ch) {
            return false;
        }

        if ch != '\u{200d}' && ch != '\u{fe0f}' {
            emoji_unit_count = emoji_unit_count.saturating_add(1);
        }

        if (ch as u32) != 0xfe0f && (ch as u32) != 0x200d {
            has_emoji = true;
        }

        if emoji_unit_count > 8 {
            return false;
        }
    }

    has_emoji && emoji_unit_count > 0
}

fn is_emoji_component(ch: char) -> bool {
    let code_point = ch as u32;
    matches!(
        code_point,
        0x1f300..=0x1faff | 0x2600..=0x27bf | 0x1f1e6..=0x1f1ff | 0xfe0f | 0x200d
    )
}

fn extract_intent_trace_turn_key(payload: &Map<String, Value>) -> Option<String> {
    payload.get("turn_id").and_then(|turn_id| match turn_id {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| format!("turn:{trimmed}"))
        }
        Value::Number(number) => number
            .as_u64()
            .map(|value| format!("turn:{value}"))
            .or_else(|| {
                number
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .map(|value| format!("turn:{value}"))
            }),
        _ => None,
    })
}

fn is_same_intent_trace_turn(current_turn: Option<&str>, incoming_turn: Option<&str>) -> bool {
    match (current_turn, incoming_turn) {
        (Some(current), Some(incoming)) => current == incoming,
        (None, None) => true,
        _ => false,
    }
}

fn format_intent_trace_line(payload: &Map<String, Value>, step_index: usize) -> String {
    let tool = read_string_field(payload, "tool").unwrap_or("unknown_tool");
    let status = read_string_field(payload, "status").unwrap_or("unknown");
    let source = read_string_field(payload, "source").unwrap_or("-");

    let mut line = format!("{step_index}. [{status}] {tool}");
    if source != "-" {
        line.push_str(&format!(" ({source})"));
    }

    if let Some(error_value) = payload.get("error").filter(|value| !value.is_null()) {
        let error_text = clamp_intent_trace_field(&compact_json(error_value), 88);
        line.push_str(&format!(" | error={error_text}"));
    } else if let Some(result_value) = payload.get("result").filter(|value| !value.is_null()) {
        let result_text = clamp_intent_trace_field(&compact_json(result_value), 88);
        line.push_str(&format!(" | result={result_text}"));
    }

    line
}

fn clamp_intent_trace_field(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...")
}

fn summarize_tool_message(payload: &Map<String, Value>) -> String {
    let Some(inner_payload) = payload.get("payload") else {
        return compact_json(&Value::Object(payload.clone()));
    };

    if let Some(inner) = inner_payload.as_object() {
        if let Some(method) = inner.get("method").and_then(Value::as_str) {
            return format!(
                "method={}, payload={}",
                method,
                compact_json(inner.get("params").unwrap_or(&Value::Null))
            );
        }

        if inner.get("result").is_some() || inner.get("error").is_some() {
            return compact_json(inner_payload);
        }
    }

    compact_json(inner_payload)
}

fn read_string_field<'a>(payload: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn read_bool_field(payload: &Map<String, Value>, key: &str) -> Option<bool> {
    payload.get(key).and_then(Value::as_bool)
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

fn unix_now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(duration_to_millis)
        .unwrap_or_default()
}

fn duration_to_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn utc_datetime_parts_from_millis(timestamp_ms: u64) -> (i32, u32, u32, u32, u32, u32) {
    let total_seconds = timestamp_ms / 1_000;
    let days_since_epoch = total_seconds / 86_400;
    let seconds_of_day = u32::try_from(total_seconds % 86_400).unwrap_or_default();
    let (year, month, day) =
        utc_date_from_days(i64::try_from(days_since_epoch).unwrap_or(i64::MAX));
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    (year, month, day, hour, minute, second)
}

fn utc_date_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let adjusted_year = year + if month <= 2 { 1 } else { 0 };

    (
        i32::try_from(adjusted_year).unwrap_or(i32::MAX),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn format_hms(hour: u32, minute: u32, second: u32) -> String {
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn format_message_timestamp(timestamp_ms: u64) -> String {
    let now = utc_datetime_parts_from_millis(unix_now_millis());
    let timestamp = utc_datetime_parts_from_millis(timestamp_ms);

    if (timestamp.0, timestamp.1, timestamp.2) == (now.0, now.1, now.2) {
        format_hms(timestamp.3, timestamp.4, timestamp.5)
    } else {
        format!(
            "{:04}-{:02}-{:02} {}",
            timestamp.0,
            timestamp.1,
            timestamp.2,
            format_hms(timestamp.3, timestamp.4, timestamp.5)
        )
    }
}

fn format_response_latency(response_latency_ms: u64) -> String {
    if response_latency_ms < 1_000 {
        format!("{response_latency_ms}ms")
    } else {
        format!("{:.1}s", response_latency_ms as f64 / 1_000.0)
    }
}

pub(crate) fn chat_message_header(message: &ChatMessage) -> String {
    let timestamp = format_message_timestamp(message.created_at_unix_ms);
    match message.response_latency_ms {
        Some(response_latency_ms) => format!(
            "{} {} | {}",
            message.title,
            timestamp,
            format_response_latency(response_latency_ms)
        ),
        None => format!("{} {}", message.title, timestamp),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_intent_trace_turn_key, format_response_latency, is_llm_emotion_placeholder,
        merge_streaming_text, should_track_stt_response_latency,
    };

    #[test]
    fn emotion_placeholder_detection_handles_emoji_only_text() {
        assert!(is_llm_emotion_placeholder("😀"));
        assert!(is_llm_emotion_placeholder("😀 😄"));
        assert!(!is_llm_emotion_placeholder("hello 😀"));
        assert!(!is_llm_emotion_placeholder(""));
    }

    #[test]
    fn trace_turn_key_supports_string_and_number() {
        let string_turn = serde_json::json!({ "turn_id": "abc" });
        assert_eq!(
            extract_intent_trace_turn_key(string_turn.as_object().expect("object")),
            Some("turn:abc".to_string())
        );

        let numeric_turn = serde_json::json!({ "turn_id": 42 });
        assert_eq!(
            extract_intent_trace_turn_key(numeric_turn.as_object().expect("object")),
            Some("turn:42".to_string())
        );
    }

    #[test]
    fn response_latency_format_is_stable() {
        assert_eq!(format_response_latency(980), "980ms");
        assert_eq!(format_response_latency(1_200), "1.2s");
    }

    #[test]
    fn stt_latency_tracking_prefers_final_results() {
        let partial = serde_json::json!({ "state": "partial", "text": "你好" });
        assert!(!should_track_stt_response_latency(
            partial.as_object().expect("object"),
        ));

        let explicit_non_final = serde_json::json!({
            "is_final": false,
            "text": "你好"
        });
        assert!(!should_track_stt_response_latency(
            explicit_non_final.as_object().expect("object"),
        ));

        let final_state = serde_json::json!({ "state": "final", "text": "你好" });
        assert!(should_track_stt_response_latency(
            final_state.as_object().expect("object"),
        ));

        let state_missing = serde_json::json!({ "text": "你好" });
        assert!(should_track_stt_response_latency(
            state_missing.as_object().expect("object"),
        ));
    }

    #[test]
    fn merge_streaming_text_avoids_duplicate_overlap() {
        assert_eq!(merge_streaming_text("你好", "你好"), "你好");
        assert_eq!(merge_streaming_text("你好", "你好呀"), "你好呀");
        assert_eq!(merge_streaming_text("你好", "好呀"), "你好呀");
        assert_eq!(
            merge_streaming_text("你好，今天", "今天会议安排"),
            "你好，今天会议安排"
        );
    }
}
