#![allow(unsafe_code)]

use std::ptr;
use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};
use objc2_foundation::NSData;
use tauri::webview::PlatformWebview;

const PNG_FILE_TYPE: usize = 4;

fn view_of(platform: &PlatformWebview) -> *mut AnyObject {
    platform.inner().cast()
}

pub fn keep_rendering_when_hidden(platform: &PlatformWebview) {
    let view = view_of(platform);
    unsafe {
        let can_ignore_occlusion: bool = msg_send![
            view,
            respondsToSelector: sel!(_setWindowOcclusionDetectionEnabled:)
        ];
        if can_ignore_occlusion {
            let () = msg_send![view, _setWindowOcclusionDetectionEnabled: false];
            let () = msg_send![view, viewDidUnhide];
        }
    }
}

pub fn window_number(platform: &PlatformWebview) -> isize {
    let window: *mut AnyObject = platform.ns_window().cast();
    unsafe { msg_send![window, windowNumber] }
}

pub fn snapshot(platform: &PlatformWebview, reply: Sender<Result<Vec<u8>, String>>) {
    let completion = RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
        let _ = reply.send(png_of(image));
    });
    unsafe {
        let () = msg_send![
            view_of(platform),
            takeSnapshotWithConfiguration: ptr::null::<AnyObject>(),
            completionHandler: &*completion
        ];
    }
}

fn png_of(image: *mut AnyObject) -> Result<Vec<u8>, String> {
    if image.is_null() {
        return Err("the web view returned no snapshot".to_owned());
    }
    unsafe {
        let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
        let bitmap: *mut AnyObject = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
        let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let png: *mut NSData = msg_send![
            bitmap,
            representationUsingType: PNG_FILE_TYPE,
            properties: properties
        ];
        png.as_ref()
            .map(NSData::to_vec)
            .ok_or_else(|| "the snapshot could not be encoded as PNG".to_owned())
    }
}
