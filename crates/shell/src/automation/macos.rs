#![allow(unsafe_code)]

use std::ptr;
use std::sync::mpsc::Sender;

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};
use objc2_foundation::{NSData, NSPoint, NSRange, NSRect, NSString};
use tauri::webview::PlatformWebview;

const PNG_FILE_TYPE: usize = 4;
const NOT_FOUND: usize = isize::MAX.unsigned_abs();

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

pub fn terminate() {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let () = msg_send![app, terminate: ptr::null::<AnyObject>()];
    }
}

const KEY_DOWN: usize = 10;
const KEY_UP: usize = 11;
const LEFT_MOUSE_DOWN: usize = 1;
const LEFT_MOUSE_UP: usize = 2;

pub struct Key {
    pub code: u16,
    pub characters: &'static str,
}

pub struct Click {
    pub x: f64,
    pub y: f64,
    pub count: isize,
}

fn window_of(platform: &PlatformWebview) -> Result<&AnyObject, String> {
    unsafe { platform.ns_window().cast::<AnyObject>().as_ref() }
        .ok_or_else(|| "the web view has no native window".to_owned())
}

pub fn type_text(platform: &PlatformWebview, text: &str) -> Result<(), String> {
    let view = view_of(platform)?;
    let text = NSString::from_str(text);
    let nowhere = NSRange::new(NOT_FOUND, 0);
    unsafe {
        let () = msg_send![view, insertText: &*text, replacementRange: nowhere];
    }
    Ok(())
}

pub fn press(platform: &PlatformWebview, key: &Key) -> Result<(), String> {
    let view = view_of(platform)?;
    let number = window_number(platform)?;
    let characters = NSString::from_str(key.characters);
    for kind in [KEY_DOWN, KEY_UP] {
        unsafe {
            let event: *mut AnyObject = msg_send![
                class!(NSEvent),
                keyEventWithType: kind,
                location: NSPoint::new(0.0, 0.0),
                modifierFlags: 0usize,
                timestamp: 0.0f64,
                windowNumber: number,
                context: ptr::null::<AnyObject>(),
                characters: &*characters,
                charactersIgnoringModifiers: &*characters,
                isARepeat: false,
                keyCode: key.code
            ];
            let event = event.as_ref().ok_or("the system made no key event")?;
            if kind == KEY_DOWN {
                let () = msg_send![view, keyDown: event];
            } else {
                let () = msg_send![view, keyUp: event];
            }
        }
    }
    Ok(())
}

pub fn click(platform: &PlatformWebview, at: &Click) -> Result<(), String> {
    let view = view_of(platform)?;
    let window = window_of(platform)?;
    let number = window_number(platform)?;
    let height = unsafe {
        let content: *mut AnyObject = msg_send![window, contentView];
        let content = content.as_ref().ok_or("the window has no content view")?;
        let frame: NSRect = msg_send![content, frame];
        frame.size.height
    };
    let location = NSPoint::new(at.x, height - at.y);
    for count in 1..=at.count {
        for kind in [LEFT_MOUSE_DOWN, LEFT_MOUSE_UP] {
            unsafe {
                let event: *mut AnyObject = msg_send![
                    class!(NSEvent),
                    mouseEventWithType: kind,
                    location: location,
                    modifierFlags: 0usize,
                    timestamp: 0.0f64,
                    windowNumber: number,
                    context: ptr::null::<AnyObject>(),
                    eventNumber: 0isize,
                    clickCount: count,
                    pressure: 1.0f32
                ];
                let event = event.as_ref().ok_or("the system made no mouse event")?;
                if kind == LEFT_MOUSE_DOWN {
                    let () = msg_send![view, mouseDown: event];
                } else {
                    let () = msg_send![view, mouseUp: event];
                }
            }
        }
    }
    Ok(())
}
