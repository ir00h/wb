#![allow(unsafe_code)]

use std::sync::{Arc, OnceLock};

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{class, ffi, msg_send, sel};

use tauri::AppHandle;

use crate::closing::{Closing, TerminateReply};

const TERMINATE_NOW: usize = 1;
const TERMINATE_LATER: usize = 2;
const SHOULD_TERMINATE_TYPES: &std::ffi::CStr = c"Q@:@";

static CLOSING: OnceLock<Arc<Closing>> = OnceLock::new();

extern "C-unwind" fn should_terminate(_: &AnyObject, _: Sel, _: &AnyObject) -> usize {
    match CLOSING.get().map(Closing::terminate_requested) {
        None | Some(TerminateReply::Now) => TERMINATE_NOW,
        Some(TerminateReply::Later) => TERMINATE_LATER,
    }
}

pub fn reply_to_terminate(app: &AppHandle, terminate: bool) {
    let _ = app.run_on_main_thread(move || unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let () = msg_send![app, replyToApplicationShouldTerminate: Bool::new(terminate)];
    });
}

pub fn route_quit_requests(closing: &Arc<Closing>) -> Result<(), String> {
    CLOSING
        .set(Arc::clone(closing))
        .map_err(|_| "quit requests are already routed".to_owned())?;
    let handler: extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize = should_terminate;
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let delegate: *mut AnyObject = msg_send![app, delegate];
        let delegate = delegate
            .as_ref()
            .ok_or("the application has no delegate to ask before it quits")?;
        let delegate_class: *const AnyClass = delegate.class();
        let added = ffi::class_addMethod(
            delegate_class.cast_mut(),
            sel!(applicationShouldTerminate:),
            std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize, Imp>(
                handler,
            ),
            SHOULD_TERMINATE_TYPES.as_ptr(),
        );
        if !added.as_bool() {
            return Err(
                "the application delegate already answers applicationShouldTerminate:".to_owned(),
            );
        }
    }
    Ok(())
}
