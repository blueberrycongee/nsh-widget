//! Host-side WidgetKit bridge: App Group resolution, `WidgetCenter` reloads,
//! Darwin notification observation (`<appGroup>.action`), and action delivery
//! channels for `Widget.on("action", ...)`.

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use nsh_core::JsonValue;
use tokio::sync::mpsc;

use crate::{
    compiler::{RawWidgetSnapshot, WidgetCompiler, tokens_from_theme_json},
    config::WidgetActionEvent,
    ir::WidgetSnapshot,
    store::WidgetStore,
};

const MAX_PENDING_ACTIONS: usize = 64;

struct Subscriber {
    id: u64,
    sender: mpsc::UnboundedSender<WidgetActionEvent>,
}

struct HostState {
    configured_group: Option<String>,
    assets_dir: Option<PathBuf>,
    compiler: WidgetCompiler,
    subscribers: Vec<Subscriber>,
    next_subscriber: u64,
    pending_actions: Vec<WidgetActionEvent>,
    #[cfg(target_os = "macos")]
    observer: Option<macos::WidgetActionObserver>,
}

impl HostState {
    fn new() -> Self {
        Self {
            configured_group: None,
            assets_dir: None,
            compiler: WidgetCompiler::default(),
            subscribers: Vec::new(),
            next_subscriber: 0,
            pending_actions: Vec::new(),
            #[cfg(target_os = "macos")]
            observer: None,
        }
    }
}

static HOST: OnceLock<Mutex<HostState>> = OnceLock::new();

fn state() -> MutexGuard<'static, HostState> {
    HOST.get_or_init(|| Mutex::new(HostState::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Current Unix epoch time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Configures optional explicit `app_group`, `theme_json`, and `assets_dir`.
pub fn configure(
    app_group: Option<String>,
    theme_json: Option<&str>,
    assets_dir: Option<PathBuf>,
) -> Result<(), String> {
    let new_compiler = match theme_json {
        Some(json) => Some(WidgetCompiler::with_tokens(&tokens_from_theme_json(json)?)),
        None => None,
    };
    let mut guard = state();
    if let Some(group) = app_group {
        let trimmed = group.trim().to_owned();
        if trimmed.is_empty() {
            return Err("Widget.configure: `appGroup` must not be empty".into());
        }
        guard.configured_group = Some(trimmed);
        #[cfg(target_os = "macos")]
        {
            guard.observer = None;
        }
    }
    if let Some(compiler) = new_compiler {
        guard.compiler = compiler;
    }
    if let Some(dir) = assets_dir {
        guard.assets_dir = Some(dir);
    }
    drop(guard);
    ensure_observer_installed();
    Ok(())
}

/// Resolves the active App Group identifier (explicit `configure({ appGroup })`,
/// else signed `com.apple.security.application-groups[0]` or `Info.plist`
/// `NshWidgetAppGroup`).
pub fn resolve_app_group() -> Option<String> {
    if let Some(group) = state().configured_group.clone() {
        return Some(group);
    }
    #[cfg(target_os = "macos")]
    {
        macos::default_app_group()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Whether native WidgetKit integration is active in this process.
pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        resolve_app_group().is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Resolves the [`WidgetStore`] for the active App Group.
pub fn resolve_store() -> Option<WidgetStore> {
    let group = resolve_app_group()?;
    #[cfg(target_os = "macos")]
    {
        let container = macos::group_container(&group).unwrap_or_else(|| {
            macos::app_support_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("nsh-widgets")
                .join(&group)
        });
        Some(WidgetStore::new(container))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = group;
        None
    }
}

/// Compiles a [`RawWidgetSnapshot`] into a validated [`WidgetSnapshot`] using
/// the currently configured theme tokens.
pub fn compile_snapshot(raw: &RawWidgetSnapshot) -> Result<WidgetSnapshot, String> {
    let compiler = state().compiler.clone();
    compiler.compile_snapshot(raw, now_ms())
}

/// Compiles, persists, and triggers a WidgetKit timeline reload for `raw`.
pub fn push_snapshot(raw: &RawWidgetSnapshot) -> Result<(), String> {
    let snapshot = compile_snapshot(raw)?;
    write_compiled_snapshot(&snapshot)
}

/// Validates, persists, and triggers a WidgetKit timeline reload for an already
/// compiled [`WidgetSnapshot`].
pub fn write_compiled_snapshot(snapshot: &WidgetSnapshot) -> Result<(), String> {
    let store = resolve_store()
        .ok_or_else(|| "no App Group configured or entitled on this process".to_owned())?;
    let assets_dir = state().assets_dir.clone();
    store.write_snapshot(snapshot, assets_dir.as_deref())?;
    reload(Some(&snapshot.widget_id));
    Ok(())
}

/// Persists `data` into `widgets/<widget_id>/data.json`.
pub fn set_data(widget_id: &str, data: &JsonValue) -> Result<(), String> {
    let store = resolve_store()
        .ok_or_else(|| "no App Group configured or entitled on this process".to_owned())?;
    store.write_data(widget_id, data)
}

/// Reads persisted `data` from `widgets/<widget_id>/data.json`.
pub fn get_data(widget_id: &str) -> Result<Option<JsonValue>, String> {
    let Some(store) = resolve_store() else {
        return Ok(None);
    };
    store.read_data(widget_id)
}

/// Requests `WidgetCenter.shared` to reload `widget_id`, or all widgets when `None`.
pub fn reload(widget_id: Option<&str>) {
    #[cfg(target_os = "macos")]
    match widget_id {
        Some(kind) => macos::reload_kind(kind),
        None => macos::reload_all(),
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = widget_id;
    }
}

/// Ensures the Darwin notification observer is installed for the active App Group.
pub fn ensure_observer_installed() {
    #[cfg(target_os = "macos")]
    {
        let Some(group) = resolve_app_group() else {
            return;
        };
        let mut guard = state();
        let needs_install = guard
            .observer
            .as_ref()
            .is_none_or(|obs| obs.app_group() != group);
        if needs_install {
            guard.observer = macos::WidgetActionObserver::install(&group);
        }
    }
}

/// Subscribes a JS VM listener channel to incoming widget action events and
/// drains any actions queued in memory or `actions.jsonl` before subscription.
pub fn subscribe() -> (
    u64,
    mpsc::UnboundedReceiver<WidgetActionEvent>,
    Vec<WidgetActionEvent>,
) {
    ensure_observer_installed();
    let drained_disk = resolve_store()
        .map(|s| s.drain_actions())
        .unwrap_or_default();

    let mut guard = state();
    let id = guard.next_subscriber;
    guard.next_subscriber = id.wrapping_add(1);
    let (sender, receiver) = mpsc::unbounded_channel();
    guard.subscribers.push(Subscriber { id, sender });

    let mut initial = std::mem::take(&mut guard.pending_actions);
    initial.extend(drained_disk);
    (id, receiver, initial)
}

/// Unsubscribes a listener channel by `id`.
pub fn unsubscribe(id: u64) {
    state().subscribers.retain(|s| s.id != id);
}

#[cfg(target_os = "macos")]
fn dispatch_from_disk() {
    let Some(store) = resolve_store() else {
        return;
    };
    let events = store.drain_actions();
    if events.is_empty() {
        return;
    }
    let mut guard = state();
    for event in events {
        let mut sent_any = false;
        guard.subscribers.retain(|sub| {
            let ok = sub.sender.send(event.clone()).is_ok();
            sent_any |= ok;
            ok
        });
        if !sent_any {
            if guard.pending_actions.len() >= MAX_PENDING_ACTIONS {
                guard.pending_actions.remove(0);
            }
            guard.pending_actions.push(event);
        }
    }
}

/// Sets an explicit assets directory (useful when `Widget.update` references
/// `asset:/...` files).
pub fn set_assets_dir(dir: &Path) {
    state().assets_dir = Some(dir.to_path_buf());
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        path::PathBuf,
    };

    use dispatch2::DispatchQueue;

    unsafe extern "C" {
        fn nsh_widget_reload_kind(kind: *const c_char);
        fn nsh_widget_reload_all();
        fn nsh_widget_group_container(
            group_id: *const c_char,
            out_buf: *mut c_char,
            out_len: usize,
        ) -> usize;
        fn nsh_widget_default_app_group(out_buf: *mut c_char, out_len: usize) -> usize;
    }

    #[repr(C)]
    struct CFNotificationCenterOpaque {
        _private: [u8; 0],
    }
    type CFNotificationCenterRef = *mut CFNotificationCenterOpaque;
    type CFStringRef = *const c_void;
    type CFNotificationCallback = unsafe extern "C" fn(
        center: CFNotificationCenterRef,
        observer: *mut c_void,
        name: CFStringRef,
        object: *const c_void,
        user_info: *const c_void,
    );

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_CF_NOTIFICATION_SUSPENSION_DELIVER_IMMEDIATELY: isize = 4;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFNotificationCenterGetDarwinNotifyCenter() -> CFNotificationCenterRef;
        fn CFNotificationCenterAddObserver(
            center: CFNotificationCenterRef,
            observer: *const c_void,
            callback: CFNotificationCallback,
            name: CFStringRef,
            object: *const c_void,
            suspension_behavior: isize,
        );
        fn CFNotificationCenterRemoveObserver(
            center: CFNotificationCenterRef,
            observer: *const c_void,
            name: CFStringRef,
            object: *const c_void,
        );
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
    }

    pub(super) fn reload_kind(kind: &str) {
        let Ok(cstr) = CString::new(kind) else {
            return;
        };
        // SAFETY: `cstr` is NUL-terminated UTF-8 for the duration of the call.
        unsafe { nsh_widget_reload_kind(cstr.as_ptr()) };
    }

    pub(super) fn reload_all() {
        // SAFETY: FFI function takes no arguments.
        unsafe { nsh_widget_reload_all() };
    }

    pub(super) fn group_container(app_group: &str) -> Option<PathBuf> {
        let cstr = CString::new(app_group).ok()?;
        let mut buf = vec![0u8; 4096];
        // SAFETY: `cstr` is NUL-terminated and `buf` has capacity `buf.len()`.
        let len = unsafe {
            nsh_widget_group_container(cstr.as_ptr(), buf.as_mut_ptr().cast::<c_char>(), buf.len())
        };
        read_utf8_buf(&buf, len).map(PathBuf::from)
    }

    pub(super) fn default_app_group() -> Option<String> {
        let mut buf = vec![0u8; 512];
        // SAFETY: `buf` has capacity `buf.len()`.
        let len =
            unsafe { nsh_widget_default_app_group(buf.as_mut_ptr().cast::<c_char>(), buf.len()) };
        read_utf8_buf(&buf, len)
    }

    pub(super) fn app_support_dir() -> Option<PathBuf> {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
        Some(PathBuf::from(home).join("Library/Application Support"))
    }

    fn read_utf8_buf(buf: &[u8], len: usize) -> Option<String> {
        if len == 0 || len >= buf.len() {
            return None;
        }
        let s = CStr::from_bytes_until_nul(&buf[..=len])
            .ok()?
            .to_str()
            .ok()?;
        (!s.is_empty()).then(|| s.to_owned())
    }

    pub(super) struct WidgetActionObserver {
        app_group: String,
        token: *mut u8,
        name: CFStringRef,
    }

    // SAFETY: `token` is a pinned heap byte never dereferenced after registration,
    // and `CFStringRef` is immutable and thread-safe to retain/release.
    unsafe impl Send for WidgetActionObserver {}

    impl WidgetActionObserver {
        pub(super) fn app_group(&self) -> &str {
            &self.app_group
        }

        pub(super) fn install(app_group: &str) -> Option<Self> {
            let notif_name = CString::new(format!("{app_group}.action")).ok()?;
            // SAFETY: `notif_name` is a valid NUL-terminated UTF-8 string.
            let cf_name = unsafe {
                CFStringCreateWithCString(
                    std::ptr::null(),
                    notif_name.as_ptr(),
                    K_CF_STRING_ENCODING_UTF8,
                )
            };
            if cf_name.is_null() {
                return None;
            }
            let token = Box::into_raw(Box::new(0u8));
            // SAFETY: `center` is the Darwin notification center, `token` stays
            // live until `Drop` removes the observer, and `cf_name` is retained
            // until `Drop`.
            unsafe {
                let center = CFNotificationCenterGetDarwinNotifyCenter();
                CFNotificationCenterAddObserver(
                    center,
                    token.cast::<c_void>(),
                    darwin_action_callback,
                    cf_name,
                    std::ptr::null(),
                    K_CF_NOTIFICATION_SUSPENSION_DELIVER_IMMEDIATELY,
                );
            }
            Some(Self {
                app_group: app_group.to_owned(),
                token,
                name: cf_name,
            })
        }
    }

    impl Drop for WidgetActionObserver {
        fn drop(&mut self) {
            // SAFETY: removes the exact observer registered in `install` before
            // freeing `token` and releasing `self.name`.
            unsafe {
                let center = CFNotificationCenterGetDarwinNotifyCenter();
                CFNotificationCenterRemoveObserver(
                    center,
                    self.token.cast::<c_void>(),
                    self.name,
                    std::ptr::null(),
                );
                CFRelease(self.name);
                drop(Box::from_raw(self.token));
            }
        }
    }

    unsafe extern "C" fn darwin_action_callback(
        _center: CFNotificationCenterRef,
        _observer: *mut c_void,
        _name: CFStringRef,
        _object: *const c_void,
        _user_info: *const c_void,
    ) {
        DispatchQueue::main().exec_async(|| {
            super::dispatch_from_disk();
        });
    }
}
