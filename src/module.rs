//! JS `#[module]` exported under `"nsh-widget/native"` and re-exported cleanly
//! by `nsh-widget`'s TSX entrypoint.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
    task::Poll,
};

use nsh_core::{
    JsonValue,
    element::{CallCx, Callback, JsError, JsObject, Local, PropEnum, Stop, System, codes, module},
};
use tokio::sync::mpsc;

use crate::{compiler::RawWidgetSnapshot, config::WidgetActionEvent, host, ir::WidgetSnapshot};

/// Native WidgetKit bridge (`import { Widget } from "nsh-widget/native"`).
#[module]
pub(crate) struct Widget;

#[module]
impl Widget {
    /// Returns `true` when running on macOS with an active App Group (from the
    /// signed entitlements, `NshWidgetAppGroup`, or `Widget.configure`).
    pub(crate) async fn available() -> bool {
        host::available()
    }

    /// Configures optional `appGroup`, `themeJson` (`cssVars`), or `assetsDir`,
    /// and ensures the Darwin notification observer is active.
    pub(crate) fn configure(options: WidgetConfigureOptions) -> Result<(), JsError> {
        host::configure(
            options.app_group,
            options.theme_json.as_deref(),
            options.assets_dir.map(PathBuf::from),
        )
        .map_err(JsError::Range)
    }

    /// Returns the active App Group identifier, or `null` when unavailable.
    pub(crate) fn app_group() -> Option<String> {
        host::resolve_app_group()
    }

    /// Compiles a raw declarative snapshot (`RawWidgetSnapshot`) with dual
    /// Light/Dark Tailwind v4 & design-token resolution into a validated
    /// `WidgetSnapshot` IR object without writing to disk.
    pub(crate) fn compile_snapshot(input: JsonValue) -> Result<JsonValue, JsError> {
        let raw: RawWidgetSnapshot = serde_json::from_value(input)
            .map_err(|e| JsError::Type(format!("Widget.compileSnapshot: {e}")))?;
        let compiled = host::compile_snapshot(&raw).map_err(JsError::Range)?;
        serde_json::to_value(&compiled)
            .map_err(|e| JsError::Error(e.to_string()).with_code(codes::HOST_ERROR))
    }

    /// Compiles a raw declarative snapshot (`RawWidgetSnapshot`), writes
    /// `snapshot.json` atomically to the shared App Group container, and asks
    /// `WidgetCenter` to reload that widget's timeline.
    pub(crate) async fn push_snapshot(input: JsonValue) -> Result<(), JsError> {
        let raw: RawWidgetSnapshot = serde_json::from_value(input)
            .map_err(|e| JsError::Type(format!("Widget.pushSnapshot: {e}")))?;
        if !host::available() {
            return Err(unavailable("Widget.pushSnapshot"));
        }
        host::push_snapshot(&raw).map_err(|msg| JsError::Error(msg).with_code(codes::HOST_ERROR))
    }

    /// Persists an already-compiled `WidgetSnapshot` IR JSON object directly and
    /// asks `WidgetCenter` to reload that widget's timeline.
    pub(crate) async fn write_raw_snapshot(snapshot: JsonValue) -> Result<(), JsError> {
        let parsed: WidgetSnapshot = serde_json::from_value(snapshot)
            .map_err(|e| JsError::Type(format!("Widget.writeRawSnapshot: {e}")))?;
        if !host::available() {
            return Err(unavailable("Widget.writeRawSnapshot"));
        }
        host::write_compiled_snapshot(&parsed)
            .map_err(|msg| JsError::Error(msg).with_code(codes::HOST_ERROR))
    }

    /// Persists raw JSON `data` for `widgetId` inside the shared container.
    pub(crate) async fn set_data(widget_id: String, data: JsonValue) -> Result<(), JsError> {
        if widget_id.trim().is_empty() {
            return Err(JsError::Range(
                "Widget.setData: `widgetId` must not be empty".into(),
            ));
        }
        if !host::available() {
            return Err(unavailable("Widget.setData"));
        }
        host::set_data(&widget_id, &data)
            .map_err(|msg| JsError::Error(msg).with_code(codes::HOST_ERROR))
    }

    /// Reads the last persisted `data` JSON for `widgetId` (`null` if missing
    /// or unavailable).
    pub(crate) async fn get_data(widget_id: String) -> Result<Option<JsonValue>, JsError> {
        if widget_id.trim().is_empty() {
            return Err(JsError::Range(
                "Widget.getData: `widgetId` must not be empty".into(),
            ));
        }
        if !host::available() {
            return Ok(None);
        }
        host::get_data(&widget_id).map_err(|msg| JsError::Error(msg).with_code(codes::HOST_ERROR))
    }

    /// Asks `WidgetCenter` to reload one widget's timeline, or all widgets when
    /// `widgetId` is omitted.
    pub(crate) async fn reload(widget_id: Option<String>) -> Result<(), JsError> {
        if !host::available() {
            return Ok(());
        }
        host::reload(widget_id.as_deref());
        Ok(())
    }

    /// Listens to interactive `<WAction>` events dispatched from desktop widgets.
    ///
    /// Returns a function that removes the listener.
    pub(crate) fn on(
        event: WidgetEventKind,
        #[args(event)] listener: Callback<WidgetActionEventJs>,
        sys: System,
        cx: CallCx<'_>,
    ) -> Result<Stop, JsError> {
        let WidgetEventKind::Action = event;
        let state = sys.locals.get::<ActionListeners>();
        let id = state.next.get();
        state.next.set(id.wrapping_add(1));

        let receiver = if !state.started.replace(true) {
            let (sub_id, rx, initial) = host::subscribe();
            state.subscriber_id.set(Some(sub_id));
            for event in initial {
                listener.post(WidgetActionEventJs::from(event));
            }
            Some(rx)
        } else {
            for event in state.unheard.take() {
                listener.post(WidgetActionEventJs::from(event));
            }
            None
        };

        state.listeners.borrow_mut().push((id, listener));
        if let Some(rx) = receiver {
            *state.receiver.borrow_mut() = Some(rx);
            cx.start(deliver(Rc::downgrade(&state)));
        }
        let weak = Rc::downgrade(&state);
        Ok(Stop::new(move || {
            if let Some(state) = weak.upgrade() {
                state.listeners.borrow_mut().retain(|(i, _)| *i != id);
            }
        }))
    }
}

fn unavailable(call: &str) -> JsError {
    JsError::Error(format!(
        "{call}: native widgets are not available (no App Group entitlement or configure({{ appGroup }}))"
    ))
    .with_code(codes::UNAVAILABLE)
}

/// Configuration options for `Widget.configure`.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[prop(exact)]
pub(crate) struct WidgetConfigureOptions {
    /// Explicit App Group identifier shared with the `.appex` extension.
    pub(crate) app_group: Option<String>,
    /// Optional shadcn/NSH theme JSON (`cssVars`) for Light/Dark semantic tokens.
    pub(crate) theme_json: Option<String>,
    /// Optional directory containing `asset:/...` files referenced by widgets.
    pub(crate) assets_dir: Option<String>,
}

/// Events accepted by `Widget.on`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PropEnum)]
pub(crate) enum WidgetEventKind {
    /// An interactive `<WAction>` button was clicked on a widget.
    #[default]
    Action,
}

/// Payload delivered to `Widget.on("action", listener)`.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
pub(crate) struct WidgetActionEventJs {
    /// Widget id.
    widget_id: String,
    /// Action name from `<WAction>`.
    action: String,
    /// Optional JSON payload attached to `<WAction>`.
    #[prop(skip_none)]
    payload: Option<JsonValue>,
    /// Unix timestamp in ms when the user clicked the action.
    timestamp: f64,
}

impl From<WidgetActionEvent> for WidgetActionEventJs {
    fn from(event: WidgetActionEvent) -> Self {
        Self {
            widget_id: event.widget_id,
            action: event.action,
            payload: event.payload,
            timestamp: event.timestamp as f64,
        }
    }
}

#[derive(Default)]
struct ActionListeners {
    listeners: RefCell<Vec<(u32, Callback<WidgetActionEventJs>)>>,
    next: Cell<u32>,
    started: Cell<bool>,
    subscriber_id: Cell<Option<u64>>,
    receiver: RefCell<Option<mpsc::UnboundedReceiver<WidgetActionEvent>>>,
    unheard: RefCell<Vec<WidgetActionEvent>>,
}

impl Local for ActionListeners {
    fn close(&self) {
        self.listeners.borrow_mut().clear();
        self.receiver.borrow_mut().take();
        if let Some(id) = self.subscriber_id.take() {
            host::unsubscribe(id);
        }
    }
}

async fn next_event(weak: &Weak<ActionListeners>) -> Option<WidgetActionEvent> {
    std::future::poll_fn(|cx| {
        let Some(state) = weak.upgrade() else {
            return Poll::Ready(None);
        };
        let mut rx = state.receiver.borrow_mut();
        match rx.as_mut() {
            Some(rx) => rx.poll_recv(cx),
            None => Poll::Ready(None),
        }
    })
    .await
}

async fn deliver(state: Weak<ActionListeners>) {
    while let Some(event) = next_event(&state).await {
        let Some(state) = state.upgrade() else {
            return;
        };
        let listeners = state.listeners.borrow();
        if listeners.is_empty() {
            state.unheard.borrow_mut().push(event);
            continue;
        }
        let payload = WidgetActionEventJs::from(event);
        for (_, listener) in &*listeners {
            listener.post(payload.clone());
        }
    }
}
