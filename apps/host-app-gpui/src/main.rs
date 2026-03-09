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
                theme.border = rgb(0x2a3547).into();
                theme.input = rgb(0x2a3547).into();
                theme.ring = rgb(0x57e9d8).into();

                theme.secondary = rgb(0x223047).into();
                theme.secondary_hover = rgb(0x2b3d58).into();
                theme.secondary_active = rgb(0x1c2a40).into();
                theme.secondary_foreground = rgb(0xcfd8e8).into();

                theme.primary = rgb(0x0f5f59).into();
                theme.primary_hover = rgb(0x12736b).into();
                theme.primary_active = rgb(0x0b4e49).into();
                theme.primary_foreground = rgb(0xe8fffb).into();

                theme.success = rgb(0x15554e).into();
                theme.success_hover = rgb(0x1b6a61).into();
                theme.success_active = rgb(0x104640).into();
                theme.success_foreground = rgb(0xe2fff9).into();

                theme.info = rgb(0x214f77).into();
                theme.info_hover = rgb(0x2b6290).into();
                theme.info_active = rgb(0x1a4364).into();
                theme.info_foreground = rgb(0xe6f3ff).into();

                theme.warning = rgb(0x5c4720).into();
                theme.warning_hover = rgb(0x72572a).into();
                theme.warning_active = rgb(0x4b3a1a).into();
                theme.warning_foreground = rgb(0xfdecc8).into();

                theme.danger = rgb(0x7a2432).into();
                theme.danger_hover = rgb(0x913043).into();
                theme.danger_active = rgb(0x651b29).into();
                theme.danger_foreground = rgb(0xffe7ec).into();
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
