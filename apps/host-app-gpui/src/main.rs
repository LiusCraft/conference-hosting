use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, SharedString, Window,
    WindowBounds, WindowOptions,
};
use host_core::GatewayStatus;
use host_platform::PlatformAdapter;

const WINDOW_WIDTH: f32 = 920.0;
const WINDOW_HEIGHT: f32 = 600.0;

struct MeetingHostShell {
    title: SharedString,
    platform: PlatformAdapter,
}

impl MeetingHostShell {
    fn toggle_connection(&mut self, cx: &mut Context<Self>) {
        self.platform.toggle_connection();
        cx.notify();
    }
}

impl Render for MeetingHostShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.platform.status();
        let button_label = match status {
            GatewayStatus::Idle => "Connect",
            GatewayStatus::Connected => "Disconnect",
        };

        div()
            .flex()
            .flex_col()
            .gap_4()
            .justify_center()
            .items_center()
            .size_full()
            .bg(rgb(0x0f172a))
            .text_color(rgb(0xe2e8f0))
            .text_xl()
            .child(self.title.clone())
            .child(format!("Gateway status: {}", status.as_label()))
            .child(
                div()
                    .id("connection-button")
                    .px_4()
                    .py_2()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .border_color(rgb(0x94a3b8))
                    .bg(rgb(0x1e293b))
                    .child(button_label)
                    .on_click(cx.listener(|view, _event, _window, cx| view.toggle_connection(cx))),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_| MeetingHostShell {
                    title: "AI Meeting Host Shell".into(),
                    platform: PlatformAdapter::default(),
                })
            },
        )
        .expect("open GPUI window failed");

        cx.activate(true);
    });
}
