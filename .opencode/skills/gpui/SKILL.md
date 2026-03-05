---
name: gpui
description: Build Rust desktop UIs with GPUI, including app shell, state flow, actions, async patterns, and troubleshooting checklists.
---

## What I do
- Produce idiomatic GPUI structures with `Application`, `Window`, `Entity<T>`, and `Render`.
- Separate domain state from view composition and keep update paths explicit.
- Implement keyboard first interaction with `actions!`, `key_context`, and `on_action`.
- Organize async work with safe lifecycle handling via async contexts.
- Deliver runnable starter code first, then expand module by module.

## When to use me
- You are starting a Rust desktop app with GPUI.
- You are integrating existing Rust logic into a GPUI front end.
- You are debugging stale UI state, missing actions, or async update failures.

## GPUI core model
- `App`: global application context and owner of entity data.
- `Entity<T>`: state handle for shared, observable data.
- `Context<T>`: entity scoped context for updates, notifications, and events.
- `Window`: window state container and root view mount point.
- `Render`: declarative rendering entry that returns `IntoElement`.

## Default workflow
1. Define domain and page state boundaries before writing layout code.
2. Create a root view entity and implement `Render`.
3. Mount it in `Application::run` with `open_window`.
4. Model interactions as actions and bind them with `on_action`.
5. Move network, file, and audio work into async tasks.
6. Write task results back to entities and surface actionable errors.

## Coding rules
- Keep `render` pure and avoid blocking I/O inside it.
- Use explicit update points; avoid hidden mutable global state.
- Throttle or isolate high frequency updates to reduce full tree redraws.
- Always pair key bindings with a clear `key_context`.
- Handle async failure paths when app or window lifetime ends.

## Starter template

```rust
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, Window,
    WindowBounds, WindowOptions,
};

struct RootView {
    title: String,
    running: bool,
}

impl RootView {
    fn toggle(&mut self, cx: &mut Context<Self>) {
        self.running = !self.running;
        cx.notify();
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.running { "Stop" } else { "Start" };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .child(self.title.clone())
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_1()
                    .rounded_md()
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(|view, _event, _window, cx| view.toggle(cx))),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(880.0), px(560.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_| RootView {
                    title: "Meeting Host Console".to_string(),
                    running: false,
                })
            },
        )
        .expect("open window failed");
    });
}
```

## Delivery checklist
- [ ] App starts and opens a window with expected state mapping.
- [ ] Core interactions work for both click and keyboard paths.
- [ ] Async tasks do not block rendering and handle failure branches.
- [ ] Logs and events are sufficient for debugging state transitions.
- [ ] Module boundaries are ready for audio, network, and device expansion.
