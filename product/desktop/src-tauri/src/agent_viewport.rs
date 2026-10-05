//! Quiet, process-bound embedded WebView geometry; never restores or focuses a window.
use super::*;
use serde_json::{json, Value};
use std::sync::{mpsc, atomic::AtomicU8};

const WAIT: Duration = Duration::from_secs(2);
const MAX_PIXELS: f64 = 8_000_000.0;

fn eligible(background: bool, headless: bool, minimized: bool, visible: bool) -> bool {
    (background && minimized) || (headless && !visible)
}
const QUEUED: u8 = 0;
const ADMITTED: u8 = 1;
const CANCELLED: u8 = 2;
fn can_apply(state: &AtomicU8, deadline: Instant) -> bool {
    state.load(Ordering::Acquire) == QUEUED && Instant::now() < deadline
}
fn admit(state: &AtomicU8) -> bool {
    state.compare_exchange(QUEUED, ADMITTED, Ordering::AcqRel, Ordering::Acquire).is_ok()
}
fn cancel_queued(state: &AtomicU8) -> bool {
    state.compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire).is_ok()
}
fn target_size(width: u64, height: u64, mode: i32, scale: f64, zoom: f64) -> Result<(i32, i32), String> {
    if !(640..=2560).contains(&width) || !(480..=1920).contains(&height)
        || !scale.is_finite() || scale <= 0.0 || !zoom.is_finite() || zoom <= 0.0
        || !matches!(mode, 0 | 1)
    { return Err("Unsupported dimensions or unknown controller scale/zoom/bounds mode".into()); }
    let factor = if mode == 1 { scale } else { 1.0 };
    if width as f64 * height as f64 * factor * factor > MAX_PIXELS {
        return Err("Viewport exceeds the eight-million physical-pixel limit".into());
    }
    Ok((width as i32, height as i32))
}

pub(super) fn execute(request: &Value, apply: bool) -> Result<Value, String> {
    if request["expected_pid"].as_u64() != Some(u64::from(std::process::id())) {
        return Err("Expected process does not match this token-bound app session".into());
    }
    #[cfg(not(target_os="windows"))]
    { let _ = apply; return Err("Quiet controller viewport diagnostics require Windows".into()); }
    #[cfg(target_os="windows")]
    {
        // This lock is released before dispatch or waiting for the UI thread.
        let (background,headless) = {
            let state=agent_bridge_state().lock().unwrap();
            (state.agent_background,state.agent_headless)
        };
        if !background && !headless { return Err("Viewport tools require background or headless launch".into()); }
        let app=AGENT_APP_HANDLE.get().ok_or("Application unavailable")?;
        let window=app.get_webview_window("main").ok_or("Main WebView unavailable")?;
        let owned=window.clone();
        let request=request.clone();
        let cancelled=Arc::new(AtomicU8::new(QUEUED));
        let callback_cancel=cancelled.clone();
        let deadline=Instant::now()+WAIT;
        let (send,receive)=mpsc::sync_channel(1);
        window.with_webview(move |platform| {
            let outcome=(|| -> Result<Value,String> {
                if !can_apply(&callback_cancel,deadline) { return Err("Expired queued viewport request; no changes applied".into()); }
                let minimized=owned.is_minimized().map_err(|e|e.to_string())?;
                let visible=owned.is_visible().map_err(|e|e.to_string())?;
                if !eligible(background,headless,minimized,visible) { return Err("Background must remain minimized or headless must remain hidden".into()); }
                let controller=platform.controller();
                let before=controller_snapshot(&owned,&controller)?;
                if !apply { return Ok(json!({"before":before,"applied":false,"native_window_changed":false})); }
                if before["native_client_size"].is_null() || before["native_restore_size"].is_null() {return Err("Native geometry unavailable; setter refused".into());}
                let width=request["width"].as_u64().ok_or("width required")?;
                let height=request["height"].as_u64().ok_or("height required")?;
                let mode=before["bounds_mode"].as_i64().ok_or("Bounds mode unavailable")? as i32;
                let scale=before["rasterization_scale"].as_f64().ok_or("Rasterization scale unavailable")?;
                let zoom=before["zoom_factor"].as_f64().ok_or("Zoom factor unavailable")?;
                let (width,height)=target_size(width,height,mode,scale,zoom)?;
                let mut bounds=Default::default();
                unsafe { controller.Bounds(&mut bounds) }.map_err(|e|format!("Bounds read failed: {e}"))?;
                bounds.right=bounds.left.checked_add(width).ok_or("Bounds width overflow")?;
                bounds.bottom=bounds.top.checked_add(height).ok_or("Bounds height overflow")?;
                // Recheck after all getters, immediately before the sole mutation.
                if !can_apply(&callback_cancel,deadline)
                    || !eligible(background,headless,owned.is_minimized().map_err(|e|e.to_string())?,owned.is_visible().map_err(|e|e.to_string())?)
                { return Err("Viewport request expired or quiet mode changed; no changes applied".into()); }
                // Admission and timeout cancellation compete atomically. Once admitted,
                // the native call may begin/finish after the caller's wait expires.
                if !admit(&callback_cancel) { return Err("Queued viewport request cancelled; no changes applied".into()); }
                unsafe { controller.SetBounds(bounds) }.map_err(|e|format!("Embedded bounds write failed: {e}; re-inspect before retry"))?;
                let after=controller_snapshot(&owned,&controller)?;
                let preserved=["native_minimized","native_visible","native_client_size","native_restore_size","zoom_factor","rasterization_scale","bounds_mode"].iter().all(|field|before[*field]==after[*field]);
                let actual_width=after["bounds"]["right"].as_i64().zip(after["bounds"]["left"].as_i64()).map(|(r,l)|r-l);
                let actual_height=after["bounds"]["bottom"].as_i64().zip(after["bounds"]["top"].as_i64()).map(|(b,t)|b-t);
                if !preserved || actual_width!=Some(i64::from(width)) || actual_height!=Some(i64::from(height)) {
                    return Err(format!("Embedded viewport readback did not verify requested bounds and preserved native window/scaling; re-inspect before retry; before={before}; after={after}"));
                }
                Ok(json!({"before":before,"after":after,"applied":true,"native_window_changed":false,
                    "zoom_preferences_changed":false,"foreground_process_unchanged":before["foreground_process_id"].as_u64().zip(after["foreground_process_id"].as_u64()).map(|(a,b)|a==b),
                    "css_viewport_and_readability":"NOT_PROVEN; capture fresh dump and snapshot"}))
            })();
            let _=send.send(outcome);
        }).map_err(|e|e.to_string())?;
        match receive.recv_timeout(WAIT) {
            Ok(result)=>result,
            Err(_)=>{
                let queued_cancelled=cancel_queued(&cancelled);
                Err(if queued_cancelled {
                    "Viewport UI wait timed out; queued operation atomically cancelled before admission; no setter can start"
                } else {
                    "Viewport UI wait timed out; operation was already admitted and may begin or finish later; native outcome unknown. Re-inspect before retry"
                }.into())
            }
        }
    }
}

#[cfg(target_os="windows")]
fn controller_snapshot(window:&tauri::WebviewWindow,controller:&webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller)->Result<Value,String> {
    use windows_core::Interface;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller3;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect,GetWindowPlacement,GetWindowThreadProcessId,GetForegroundWindow,IsIconic,IsWindowVisible,WINDOWPLACEMENT};
    let handle=window.window_handle().map_err(|e|e.to_string())?;
    let RawWindowHandle::Win32(handle)=handle.as_raw() else {return Err("Main window is not Win32".into());};
    let hwnd=handle.hwnd.get() as HWND;
    let mut pid=0;unsafe {GetWindowThreadProcessId(hwnd,&mut pid)};
    if pid!=std::process::id() {return Err("Native main-window ownership mismatch".into());}
    let mut errors=serde_json::Map::new();
    macro_rules! read { ($field:literal,$expr:expr,$value:expr) => {{
        match unsafe{$expr} {Ok(())=>json!($value),Err(e)=>{errors.insert($field.into(),json!({"hresult":e.code().0}));Value::Null}}
    }}; }
    let mut bounds=Default::default();
    let bounds_value=read!("bounds",controller.Bounds(&mut bounds),json!({"left":bounds.left,"top":bounds.top,"right":bounds.right,"bottom":bounds.bottom}));
    let mut zoom=0.0;
    let zoom_value=read!("zoom_factor",controller.ZoomFactor(&mut zoom),zoom);
    let (scale_value,mode_value)=match controller.cast::<ICoreWebView2Controller3>() {
        Ok(v)=>{
            let mut scale=0.0;let scale_value=read!("rasterization_scale",v.RasterizationScale(&mut scale),scale);
            let mut mode=Default::default();let mode_value=read!("bounds_mode",v.BoundsMode(&mut mode),mode.0);
            (scale_value,mode_value)
        },
        Err(e)=>{errors.insert("controller3".into(),json!({"hresult":e.code().0}));(Value::Null,Value::Null)}
    };
    let mut client:RECT=unsafe{std::mem::zeroed()};let mut placement:WINDOWPLACEMENT=unsafe{std::mem::zeroed()};placement.length=std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    let client_value=if unsafe{GetClientRect(hwnd,&mut client)}!=0 {json!({"width":client.right-client.left,"height":client.bottom-client.top})}else{Value::Null};
    let normal_value=if unsafe{GetWindowPlacement(hwnd,&mut placement)}!=0 {let r=placement.rcNormalPosition;json!({"width":r.right-r.left,"height":r.bottom-r.top})}else{Value::Null};
    let mut foreground_pid=0;unsafe{GetWindowThreadProcessId(GetForegroundWindow(),&mut foreground_pid)};
    Ok(json!({"process_id":pid,"bounds":bounds_value,"zoom_factor":zoom_value,"rasterization_scale":scale_value,
        "bounds_mode":mode_value,"errors":errors,"native_minimized":unsafe{IsIconic(hwnd)}!=0,
        "native_visible":unsafe{IsWindowVisible(hwnd)}!=0,"native_client_size":client_value,"native_restore_size":normal_value,
        "native_scale_factor":window.scale_factor().ok(),"foreground_process_id":if foreground_pid==0 {Value::Null}else{json!(foreground_pid)}}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn wp0330_viewport_policy_requires_hidden_or_minimized_mode() {
        assert!(eligible(true,false,true,true));assert!(eligible(false,true,false,false));
        assert!(!eligible(true,false,false,true));assert!(!eligible(false,false,true,false));assert!(!eligible(false,true,false,true));
    }
    #[test] fn wp0330_viewport_bounds_reject_unknown_scaling_and_pixel_overflow() {
        assert_eq!(target_size(1600,1200,0,2.0,2.0).unwrap(),(1600,1200));
        assert!(target_size(2560,1920,1,2.0,1.0).is_err());
        assert!(target_size(400,300,0,1.0,1.0).is_err());assert!(target_size(800,600,2,1.0,1.0).is_err());
        assert!(target_size(800,600,0,f64::NAN,1.0).is_err());assert!(target_size(800,600,0,1.0,0.0).is_err());
    }
    #[test] fn wp0330_viewport_admission_timeout_interleavings() {
        use std::sync::Barrier;
        // Force cancellation to win while the callback is poised before admission.
        let state=Arc::new(AtomicU8::new(QUEUED));
        let calls=Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let poised=Arc::new(Barrier::new(2));let proceed=Arc::new(Barrier::new(2));
        let worker={let state=state.clone();let calls=calls.clone();let poised=poised.clone();let proceed=proceed.clone();std::thread::spawn(move||{
            poised.wait();proceed.wait();
            if admit(&state){calls.fetch_add(1,Ordering::SeqCst);}
        })};
        poised.wait();assert!(cancel_queued(&state));proceed.wait();worker.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst),0);
        assert_eq!(state.load(Ordering::Acquire),CANCELLED);
        // Force admission to win, then timeout before invocation. This is explicitly
        // unknown at timeout, rather than incorrectly claiming no subsequent call.
        let state=Arc::new(AtomicU8::new(QUEUED));
        let admitted=Arc::new(Barrier::new(2));let invoke=Arc::new(Barrier::new(2));
        let worker={let state=state.clone();let calls=calls.clone();let admitted=admitted.clone();let invoke=invoke.clone();std::thread::spawn(move||{
            assert!(admit(&state));admitted.wait();invoke.wait();calls.fetch_add(1,Ordering::SeqCst);
        })};
        admitted.wait();assert!(!cancel_queued(&state));assert_eq!(calls.load(Ordering::SeqCst),0);
        invoke.wait();worker.join().unwrap();assert_eq!(calls.load(Ordering::SeqCst),1);
        assert_eq!(state.load(Ordering::Acquire),ADMITTED);
    }
}
