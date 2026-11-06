//! 전역 저수준 마우스 훅 (WH_MOUSE_LL).
//! 단축키 조합의 버튼 누름/뗌을 가로채 다른 프로그램에 전달하지 않고, 뗄 때 알림 창에 메시지를 보냄

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock, mpsc};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, GetWindowThreadProcessId, HC_ACTION, MSG, MSLLHOOKSTRUCT,
    PM_NOREMOVE, PeekMessageW, PostMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WindowFromPoint,
};
use windows::core::PWSTR;

use crate::config::{MouseButton, MouseHotkey};

struct HookConfig {
    hotkey: MouseHotkey,
    excluded: Vec<String>,
    notify: isize,
    msg: u32,
}

static CONFIG: RwLock<Option<HookConfig>> = RwLock::new(None);
/// 단축키 누름을 가로챘는지 여부 (뗌도 함께 가로채기 위함)
static DOWN_HOOKED: AtomicBool = AtomicBool::new(false);
static THREAD: Mutex<Option<(u32, JoinHandle<()>)>> = Mutex::new(None);

/// 훅 설정 적용. 단축키를 쓰지 않으면 훅을 제거하여 마우스 입력 지연을 없앰
pub fn apply(notify: HWND, msg: u32, hotkey: &MouseHotkey, excluded: Vec<String>) -> Result<(), String> {
    if !hotkey.enabled() {
        stop();
        return Ok(());
    }
    *CONFIG.write().unwrap() =
        Some(HookConfig { hotkey: hotkey.clone(), excluded, notify: notify.0 as isize, msg });

    let mut guard = THREAD.lock().unwrap();
    if guard.as_ref().is_some_and(|(_, h)| !h.is_finished()) {
        return Ok(());
    }
    let (tx, rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("gettext-mousehook".into())
        .spawn(move || hook_thread(tx))
        .map_err(|e| format!("마우스 훅 스레드 생성 실패: {e}"))?;
    match rx.recv() {
        Ok(Ok(tid)) => {
            *guard = Some((tid, handle));
            Ok(())
        }
        Ok(Err(e)) => {
            let _ = handle.join();
            Err(e)
        }
        Err(_) => Err("마우스 훅 스레드가 비정상 종료되었습니다.".into()),
    }
}

pub fn stop() {
    *CONFIG.write().unwrap() = None;
    if let Some((tid, handle)) = THREAD.lock().unwrap().take() {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        let _ = handle.join();
    }
    DOWN_HOOKED.store(false, Ordering::SeqCst);
}

fn hook_thread(ready: mpsc::Sender<Result<u32, String>>) {
    unsafe {
        // 종료 요청을 받을 메시지 큐를 먼저 생성
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

        let module = GetModuleHandleW(None).ok();
        let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), module.map(|m| m.into()), 0) {
            Ok(h) => h,
            Err(e) => {
                let _ = ready.send(Err(format!("마우스 훅 설치 실패: {}", e.message())));
                return;
            }
        };
        let _ = ready.send(Ok(GetCurrentThreadId()));

        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if handle_event(wparam.0 as u32, info.pt) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 버튼 이벤트 처리. true 면 이벤트를 가로챔
fn handle_event(msg: u32, pt: POINT) -> bool {
    let (button, down) = match msg {
        WM_LBUTTONDOWN => (MouseButton::Left, true),
        WM_LBUTTONUP => (MouseButton::Left, false),
        WM_RBUTTONDOWN => (MouseButton::Right, true),
        WM_RBUTTONUP => (MouseButton::Right, false),
        WM_MBUTTONDOWN => (MouseButton::Middle, true),
        WM_MBUTTONUP => (MouseButton::Middle, false),
        _ => return false,
    };
    let guard = CONFIG.read().unwrap();
    let Some(cfg) = guard.as_ref() else { return false };

    let hit = button == cfg.hotkey.button
        && modifiers_match(&cfg.hotkey, key_down(VK_CONTROL.0), key_down(VK_MENU.0), key_down(VK_SHIFT.0));
    if hit && down && !is_excluded(pt, &cfg.excluded) {
        DOWN_HOOKED.store(true, Ordering::SeqCst);
        return true;
    }
    if hit && !down && DOWN_HOOKED.swap(false, Ordering::SeqCst) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(cfg.notify as *mut _)),
                cfg.msg,
                WPARAM(pt.x as usize),
                LPARAM(pt.y as isize),
            );
        }
        return true;
    }
    DOWN_HOOKED.store(false, Ordering::SeqCst);
    false
}

/// 설정한 수식키와 눌린 수식키가 정확히 같은지 여부
pub fn modifiers_match(hotkey: &MouseHotkey, ctrl: bool, alt: bool, shift: bool) -> bool {
    hotkey.ctrl == ctrl && hotkey.alt == alt && hotkey.shift == shift
}

fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// 커서 아래 창의 실행 파일 이름이 제외 목록에 있는지 여부
fn is_excluded(pt: POINT, excluded: &[String]) -> bool {
    if excluded.is_empty() {
        return false;
    }
    let Some(name) = process_name_at(pt) else { return false };
    excluded.iter().any(|e| e.eq_ignore_ascii_case(&name))
}

fn process_name_at(pt: POINT) -> Option<String> {
    unsafe {
        let hwnd = WindowFromPoint(pt);
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_matching_is_exact() {
        let hk = MouseHotkey { ctrl: true, alt: false, shift: true, button: MouseButton::Left };
        assert!(modifiers_match(&hk, true, false, true));
        assert!(!modifiers_match(&hk, true, false, false));
        assert!(!modifiers_match(&hk, true, true, true));
    }

    #[test]
    fn non_button_messages_ignored() {
        assert!(!handle_event(0x0200, POINT::default())); // WM_MOUSEMOVE
    }
}
