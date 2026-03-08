mod app;
mod assets;
mod components;
mod features;
mod gateway_runtime;
mod widgets;

use app::shell::MeetingHostShell;
use app::state::{APP_TITLE, WINDOW_HEIGHT, WINDOW_WIDTH};
use assets::AppAssets;
use gateway_runtime::load_audio_device_state;
use gpui::{
    actions, point, px, rgb, size, App, AppContext, Application, Bounds, KeyBinding,
    TitlebarOptions, WindowBounds, WindowOptions,
};
use gpui_component::{Root, Theme, ThemeMode};

actions!(host_app, [CloseWindow, QuitApp]);

fn main() {
    Application::new()
        .with_assets(AppAssets::new())
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            {
                let theme = Theme::global_mut(cx);
                theme.foreground = rgb(0xe8efff).into();
                theme.muted_foreground = rgb(0x57657d).into();
                theme.caret = rgb(0x79f7ee).into();
                theme.selection = rgb(0x1a5f8f).into();
            }

            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseWindow, None),
                KeyBinding::new("ctrl-w", CloseWindow, None),
                KeyBinding::new("cmd-q", QuitApp, None),
                KeyBinding::new("ctrl-q", QuitApp, None),
            ]);
            cx.intercept_keystrokes(|event, window, cx| {
                let keystroke = &event.keystroke;
                let close_modifier_pressed = (cfg!(target_os = "macos")
                    && keystroke.modifiers.platform)
                    || (!cfg!(target_os = "macos") && keystroke.modifiers.control);
                let should_close = keystroke.key == "w" && close_modifier_pressed;
                let should_quit = keystroke.key == "q" && close_modifier_pressed;

                if should_close {
                    cx.stop_propagation();
                    window.remove_window();
                    return;
                }

                if should_quit {
                    cx.stop_propagation();
                    cx.quit();
                }
            })
            .detach();
            cx.on_action(|_: &CloseWindow, cx| {
                if let Some(window) = cx.active_window() {
                    let _ = window.update(cx, |_, window, _| {
                        window.remove_window();
                    });
                }
            });
            cx.on_action(|_: &QuitApp, cx| {
                cx.quit();
            });

            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(APP_TITLE.into()),
                        appears_transparent: cfg!(target_os = "macos"),
                        traffic_light_position: if cfg!(target_os = "macos") {
                            Some(point(px(14.0), px(9.0)))
                        } else {
                            None
                        },
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let shell =
                        cx.new(|cx| MeetingHostShell::new(window, cx, load_audio_device_state()));

                    let shell_for_close = shell.downgrade();
                    window.on_window_should_close(cx, move |_window, cx| {
                        let _ = shell_for_close.update(cx, |view, _cx| {
                            view.prepare_for_window_close();
                        });
                        true
                    });

                    cx.new(|cx| Root::new(shell, window, cx))
                },
            )
            .expect("open GPUI window failed");

            cx.activate(true);
        });
}
