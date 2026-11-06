//! 클립보드 텍스트 설정

use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

const CF_UNICODETEXT: u32 = 13;

pub fn set_text(owner: Option<HWND>, text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    unsafe {
        // 다른 프로그램이 클립보드를 열고 있으면 잠시 재시도
        let mut opened = false;
        for _ in 0..20 {
            if OpenClipboard(owner).is_ok() {
                opened = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if !opened {
            return false;
        }
        let mut ok = false;
        if EmptyClipboard().is_ok()
            && let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes)
        {
            let p = GlobalLock(h) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                let _ = GlobalUnlock(h);
                ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0))).is_ok();
            }
            if !ok {
                let _ = GlobalFree(Some(h));
            }
        }
        let _ = CloseClipboard();
        ok
    }
}
