//! One bounded LS2 round trip on the platform worker's own GLib context.
//! The hub sees the installed native executable. LSCallFromApplication (the
//! luna-send-pub -a path) instead requires an impersonation privilege we lack.
use libc::{c_char, c_int, c_ulong, c_void};
use serde_json::Value;
use std::{
    ffi::{CStr, CString},
    ptr, thread,
    time::{Duration, Instant},
};

const MAX_REPLY: usize = 2 * 1024 * 1024;

// Layout from the NDK's public luna-service2/lunaservice.h.
#[repr(C)]
struct Error {
    code: c_int,
    message: *mut c_char,
    file: *const c_char,
    line: c_int,
    function: *const c_char,
    padding: *mut c_void,
    magic: c_ulong,
}

#[link(name = "luna-service2")]
unsafe extern "C" {
    fn LSErrorInit(error: *mut Error) -> bool;
    fn LSErrorFree(error: *mut Error);
    fn LSRegister(name: *const c_char, handle: *mut *mut c_void, error: *mut Error) -> bool;
    fn LSUnregister(handle: *mut c_void, error: *mut Error) -> bool;
    fn LSGmainContextAttach(handle: *mut c_void, context: *mut c_void, error: *mut Error) -> bool;
    fn LSCallOneReply(
        handle: *mut c_void,
        uri: *const c_char,
        payload: *const c_char,
        callback: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
        userdata: *mut c_void,
        token: *mut c_ulong,
        error: *mut Error,
    ) -> bool;
    fn LSCallCancel(handle: *mut c_void, token: c_ulong, error: *mut Error) -> bool;
    fn LSMessageGetPayload(message: *mut c_void) -> *const c_char;
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_main_context_new() -> *mut c_void;
    fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
    fn g_main_context_unref(context: *mut c_void);
}

impl Error {
    fn new() -> Self {
        // SAFETY: the public error structure is zero-initializable; LS2 then
        // initializes its cookie before any other error API uses it.
        unsafe {
            let mut error = std::mem::zeroed();
            LSErrorInit(&mut error);
            error
        }
    }

    fn describe(&self, stage: &str) -> String {
        let detail = if self.message.is_null() {
            String::new()
        } else {
            // SAFETY: LS2 owns this NUL-terminated string until LSErrorFree.
            unsafe { CStr::from_ptr(self.message) }
                .to_string_lossy()
                .chars()
                .take(240)
                .collect()
        };
        format!("LS2 {stage} failed ({}): {detail}", self.code)
    }
}

impl Drop for Error {
    fn drop(&mut self) {
        // SAFETY: every Error was initialized and is freed exactly once.
        unsafe { LSErrorFree(self) };
    }
}

#[derive(Default)]
struct Reply {
    result: Option<Result<Value, String>>,
}

extern "C" fn receive(_handle: *mut c_void, message: *mut c_void, userdata: *mut c_void) -> bool {
    if message.is_null() || userdata.is_null() {
        return true;
    }
    // SAFETY: LS2 invokes this only while this connection's context is iterated
    // on the worker. The heap slot outlives cancellation/unregistration.
    unsafe {
        let reply = &mut *userdata.cast::<Reply>();
        let payload = LSMessageGetPayload(message);
        reply.result = Some(if payload.is_null() {
            Err("Luna reply has no payload".into())
        } else {
            let size = libc::strnlen(payload, MAX_REPLY + 1);
            if size > MAX_REPLY {
                Err("TV response is too large".into())
            } else {
                let bytes = std::slice::from_raw_parts(payload.cast::<u8>(), size);
                serde_json::from_slice(bytes).map_err(|error| format!("Invalid Luna JSON: {error}"))
            }
        });
    }
    true
}

struct Connection {
    handle: *mut c_void,
    context: *mut c_void,
    reply: Option<Box<Reply>>,
}

impl Connection {
    fn open() -> Result<Self, String> {
        let mut error = Error::new();
        let mut connection = Self {
            handle: ptr::null_mut(),
            // SAFETY: GLib returns a new context owned by this connection.
            context: unsafe { g_main_context_new() },
            reply: None,
        };
        if connection.context.is_null() {
            return Err("Could not allocate a Luna event context".into());
        }
        // Anonymous registration still authenticates the executable and its LS2
        // role, and avoids colliding with SDL's own app-named registration.
        // SAFETY: output/error pointers are valid; no fake application ID is sent.
        if !unsafe { LSRegister(ptr::null(), &mut connection.handle, &mut error) } {
            return Err(error.describe("registration"));
        }
        // SAFETY: the registered handle and private context stay alive together.
        if !unsafe { LSGmainContextAttach(connection.handle, connection.context, &mut error) } {
            return Err(error.describe("context attachment"));
        }
        Ok(connection)
    }

    fn request(&mut self, uri: &str, payload: &Value) -> Result<Value, String> {
        let uri = CString::new(uri).map_err(|_| "Invalid Luna URI")?;
        let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Luna payload")?;
        let mut error = Error::new();
        let mut reply = Box::<Reply>::default();
        let userdata = ptr::from_mut(reply.as_mut()).cast();
        self.reply = Some(reply);
        let mut token = 0;
        // SAFETY: strings and the stable reply allocation outlive this request.
        if !unsafe {
            LSCallOneReply(
                self.handle,
                uri.as_ptr(),
                payload.as_ptr(),
                receive,
                userdata,
                &mut token,
                &mut error,
            )
        } {
            return Err(error.describe("call submission"));
        }
        let deadline = Instant::now() + Duration::from_millis(6500);
        while Instant::now() < deadline {
            // SAFETY: only this worker drives this private context. Iteration is
            // nonblocking, so a missing service cannot hold the UI or this loop.
            unsafe { g_main_context_iteration(self.context, 0) };
            if let Some(result) = self.reply.as_mut().and_then(|reply| reply.result.take()) {
                return result;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let mut cancel_error = Error::new();
        // SAFETY: handle/token belong to this live request. Even a refused cancel
        // leaves its callback allocation alive until unregistering on drop.
        if !unsafe { LSCallCancel(self.handle, token, &mut cancel_error) } {
            eprintln!("{}", cancel_error.describe("timeout cancellation"));
        }
        Err("The TV service timed out".into())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let mut error = Error::new();
            // SAFETY: unregister before freeing the context or callback storage.
            if !unsafe { LSUnregister(self.handle, &mut error) } {
                eprintln!("{}", error.describe("unregistration"));
                // A refused unregister can retain the callback. Preserve its
                // context and heap slot rather than leave dangling pointers.
                std::mem::forget(self.reply.take());
                return;
            }
        }
        if !self.context.is_null() {
            // SAFETY: no live LS2 connection can now dispatch on this context.
            unsafe { g_main_context_unref(self.context) };
        }
    }
}

pub fn call(uri: &str, payload: &Value) -> Result<Value, String> {
    Connection::open()?.request(uri, payload)
}

#[link(name = "luna-service2")]
unsafe extern "C" {
    fn LSCall(
        handle: *mut c_void,
        uri: *const c_char,
        payload: *const c_char,
        callback: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
        userdata: *mut c_void,
        token: *mut c_ulong,
        error: *mut Error,
    ) -> bool;
}

#[derive(Default)]
struct Inbox {
    events: Vec<Value>,
}

extern "C" fn collect(_handle: *mut c_void, message: *mut c_void, userdata: *mut c_void) -> bool {
    if message.is_null() || userdata.is_null() {
        return true;
    }
    // SAFETY: same contract as `receive`; the inbox lives as long as the bus.
    unsafe {
        let inbox = &mut *userdata.cast::<Inbox>();
        let payload = LSMessageGetPayload(message);
        if payload.is_null() || inbox.events.len() > 512 {
            return true;
        }
        let size = libc::strnlen(payload, MAX_REPLY + 1);
        if size <= MAX_REPLY
            && let Ok(value) =
                serde_json::from_slice(std::slice::from_raw_parts(payload.cast::<u8>(), size))
        {
            inbox.events.push(value);
        }
    }
    true
}

/// Long-lived connection for one worker thread: request/reply plus
/// subscriptions whose events are drained with `pump`.
pub struct Bus {
    connection: Connection,
    inbox: Box<Inbox>,
    subscriptions: Vec<c_ulong>,
}

impl Bus {
    pub fn open() -> Result<Self, String> {
        Ok(Self {
            connection: Connection::open()?,
            inbox: Box::default(),
            subscriptions: Vec::new(),
        })
    }

    pub fn request(&mut self, uri: &str, payload: &Value) -> Result<Value, String> {
        self.connection.request(uri, payload)
    }

    pub fn subscribe(&mut self, uri: &str, payload: &Value) -> Result<(), String> {
        let uri = CString::new(uri).map_err(|_| "Invalid Luna URI")?;
        let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Luna payload")?;
        let mut error = Error::new();
        let mut token = 0;
        let userdata = ptr::from_mut(self.inbox.as_mut()).cast();
        // SAFETY: the boxed inbox outlives the subscription (cancelled on drop).
        if !unsafe {
            LSCall(
                self.connection.handle,
                uri.as_ptr(),
                payload.as_ptr(),
                collect,
                userdata,
                &mut token,
                &mut error,
            )
        } {
            return Err(error.describe("subscription"));
        }
        self.subscriptions.push(token);
        Ok(())
    }

    pub fn unsubscribe_all(&mut self) {
        for token in self.subscriptions.drain(..) {
            let mut error = Error::new();
            // SAFETY: token came from LSCall on this live handle.
            unsafe { LSCallCancel(self.connection.handle, token, &mut error) };
        }
    }

    /// Dispatch pending LS2 messages without blocking and return new events.
    pub fn pump(&mut self) -> Vec<Value> {
        for _ in 0..64 {
            // SAFETY: only the owning worker iterates this private context.
            if unsafe { g_main_context_iteration(self.connection.context, 0) } == 0 {
                break;
            }
        }
        std::mem::take(&mut self.inbox.events)
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        self.unsubscribe_all();
        let _ = self.pump();
    }
}
