---
name: gpui
description: Build Rust desktop UIs with GPUI + gpui-component. Prefer library components and only build domain composites.
---

## What I do
- Produce idiomatic GPUI structures with `Application`, `Window`, `Entity<T>`, and `Render`.
- Prefer `gpui-component` primitives for inputs, forms, controls, overlays, and data widgets.
- Separate domain state from view composition and keep update paths explicit.
- Implement keyboard first interaction with `actions!`, `key_context`, and `on_action`.
- Organize async work with safe lifecycle handling via async contexts.
- Deliver runnable starter code first, then expand module by module.

## Repository UI policy (must follow)
- Use `gpui-component` first for all reusable UI controls; do not hand-roll primitive widgets.
- Do not reimplement generic behaviors (text editing, caret, selection, copy/paste, context menu, dropdown, dialog, tabs, lists, table, tree, scrollbar, tooltip).
- Allowed custom UI: domain-specific composite components assembled from existing GPUI + `gpui-component` parts.
- If a needed capability is unclear, check `gpui-component` docs and source before writing custom low-level UI logic.
- Only implement a new primitive from scratch if the user explicitly approves after component/source investigation.

## When to use me
- You are starting a Rust desktop app with GPUI.
- You are integrating existing Rust logic into a GPUI front end.
- You are building screens in this repository and need consistent component usage.
- You are debugging stale UI state, missing actions, or async update failures.

## gpui-component bootstrap (always)
1. Initialize once at app startup: `gpui_component::init(cx)`.
2. Wrap window root view: `Root::new(view, window, cx)`.
3. Keep app visual consistency via `Theme`/style overrides instead of rewriting control internals.

## gpui-component component map (use these first)

Core and styling
- `Root`, `theme`, `title_bar`, `window_border`, `animation`, `icon`, `divider`, `styled`.

Inputs and forms
- `input` (`Input`, `InputState`, `NumberInput`, `OtpInput`, input events/actions).
- `form`, `setting`, `select`, `checkbox`, `radio`, `switch`, `slider`, `date_picker`, `calendar`, `color_picker`, `label`, `kbd`.

Buttons and actions
- `button`, `link`, `menu`, `popover`, `tooltip`, `alert`, `notification`.

Containers and layout
- `accordion`, `collapsible`, `group_box`, `sheet`, `sidebar`, `tab`, `dock`, `resizable`, `scroll`.

Data display
- `list`, `table`, `tree`, `text`, `badge`, `tag`, `avatar`, `description_list`, `progress`, `skeleton`, `spinner`.

Data visualization
- `chart`, `plot`.

Advanced/supporting modules
- `highlighter`, `history`, `clipboard`, `virtual_list`, `index_path`.

Canonical exported module list (from crate root)
- `accordion`, `alert`, `animation`, `avatar`, `badge`, `breadcrumb`, `button`, `chart`, `checkbox`, `clipboard`, `collapsible`, `color_picker`, `description_list`, `dialog`, `divider`, `dock`, `form`, `group_box`, `highlighter`, `history`, `input`, `kbd`, `label`, `link`, `list`, `menu`, `notification`, `plot`, `popover`, `progress`, `radio`, `resizable`, `scroll`, `select`, `setting`, `sheet`, `sidebar`, `skeleton`, `slider`, `spinner`, `switch`, `tab`, `table`, `tag`, `text`, `theme`, `tooltip`, `tree`.

## If API is unclear
1. Check docs first:
   - `https://docs.rs/gpui-component/latest/gpui_component/`
   - `https://longbridge.github.io/gpui-component/docs/`
2. Check source directly (local cargo registry):
   - `~/.cargo/registry/src/*/gpui-component-0.5.1/src/`
3. Search existing repository usage before introducing new patterns.
4. Prefer composition of existing components over custom low-level behavior.

## GPUI core model
- `App`: global application context and owner of entity data.
- `Entity<T>`: state handle for shared, observable data.
- `Context<T>`: entity scoped context for updates, notifications, and events.
- `Window`: window state container and root view mount point.
- `Render`: declarative rendering entry that returns `IntoElement`.

## Default workflow
1. Define domain and page state boundaries before writing layout code.
2. Map each UI need to an existing `gpui-component` module first.
3. Create a root view entity and implement `Render` with component composition.
4. Mount it in `Application::run` with `open_window` + `Root::new(...)`.
5. Model interactions as actions/events and bind with `on_action`/subscriptions.
6. Move network, file, and audio work into async tasks.
7. Write task results back to entities and surface actionable errors.

## Coding rules
- Keep `render` pure and avoid blocking I/O inside it.
- Use explicit update points; avoid hidden mutable global state.
- Throttle or isolate high frequency updates to reduce full tree redraws.
- Always pair key bindings with a clear `key_context`.
- Handle async failure paths when app or window lifetime ends.
- Do not replace built-in widget behavior with custom implementations when `gpui-component` already provides it.

## Starter template

```rust
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, Entity, Window,
    WindowBounds, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    Root,
};

struct RootView {
    title: String,
    command_input: Entity<InputState>,
}

impl RootView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let command_input = cx.new(|cx| InputState::new(window, cx).placeholder("Type command"));
        cx.subscribe_in(&command_input, window, |view, _state, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { secondary: false } = event {
                view.submit(window, cx);
            }
        })
        .detach();

        Self {
            title: "Meeting Host Console".to_string(),
            command_input,
        }
    }

    fn submit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let _value = self.command_input.read(cx).value();
        cx.notify();
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .child(self.title.clone())
            .child(Input::new(&self.command_input))
            .child(Button::new("submit").primary().label("Send").on_click(
                cx.listener(|view, _event, window, cx| view.submit(window, cx)),
            ))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(880.0), px(560.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| RootView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open window failed");
    });
}
```

## Delivery checklist
- [ ] App starts and opens a window with expected state mapping.
- [ ] UI primitives are from `gpui-component` (except intentional domain composite wrappers).
- [ ] Core interactions work for both click and keyboard paths.
- [ ] Async tasks do not block rendering and handle failure branches.
- [ ] Logs and events are sufficient for debugging state transitions.
- [ ] Module boundaries are ready for audio, network, and device expansion.
