#![allow(unsafe_code)]

use std::ptr;
use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};
use objc2_foundation::NSData;
use tauri::webview::PlatformWebview;

const PNG_FILE_TYPE: usize = 4;

fn view_of(platform: &PlatformWebview) -> Result<&AnyObject, String> {
    unsafe { platform.inner().cast::<AnyObject>().as_ref() }
        .ok_or_else(|| "the web view has no native view".to_owned())
}

pub fn keep_rendering_when_hidden(platform: &PlatformWebview) -> Result<(), String> {
    let view = view_of(platform)?;
    unsafe {
        let can_ignore_occlusion: bool = msg_send![
            view,
            respondsToSelector: sel!(_setWindowOcclusionDetectionEnabled:)
        ];
        if !can_ignore_occlusion {
            return Err(
                "this system's web view has no _setWindowOcclusionDetectionEnabled: selector"
                    .to_owned(),
            );
        }
        let () = msg_send![view, _setWindowOcclusionDetectionEnabled: false];
        let () = msg_send![view, viewDidUnhide];
    }
    Ok(())
}

pub fn window_number(platform: &PlatformWebview) -> Result<isize, String> {
    let window = unsafe { platform.ns_window().cast::<AnyObject>().as_ref() }
        .ok_or_else(|| "the web view has no native window".to_owned())?;
    Ok(unsafe { msg_send![window, windowNumber] })
}

pub fn snapshot(platform: &PlatformWebview, reply: Sender<Result<Vec<u8>, String>>) {
    let view = match view_of(platform) {
        Ok(view) => view,
        Err(error) => {
            let _ = reply.send(Err(error));
            return;
        }
    };
    let completion = RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
        let _ = reply.send(png_of(image));
    });
    unsafe {
        let () = msg_send![
            view,
            takeSnapshotWithConfiguration: ptr::null::<AnyObject>(),
            completionHandler: &*completion
        ];
    }
}

fn png_of(image: *mut AnyObject) -> Result<Vec<u8>, String> {
    let image = unsafe { image.as_ref() }.ok_or("the web view returned no snapshot")?;
    unsafe {
        let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
        let tiff = tiff.as_ref().ok_or("the snapshot has no TIFF data")?;
        let bitmap: *mut AnyObject = msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
        let bitmap = bitmap
            .as_ref()
            .ok_or("the snapshot's TIFF data is not a bitmap")?;
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
