//! 트레이 아이콘, 전역 단축키, 마우스 훅, 업데이트 확인을 담당하는 백그라운드 스레드

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::thread;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, HWND_MESSAGE,
    IsIconic, KillTimer, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    MESSAGEBOX_STYLE, MSG, MessageBoxW, PostQuitMessage, RegisterClassW, SM_CXSMICON, SM_CYSMICON, SW_RESTORE,
    SW_SHOW, SetForegroundWindow, SetTimer, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_HOTKEY, WM_TIMER, WNDCLASSW,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::config::{APP_NAME, APP_VERSION, key_to_vk};
use crate::{MainCmd, Shared, mousehook, popup, update};

pub const SERVICE_CLASS: PCWSTR = w!("GetText.ServiceWindow");
/// 설정 변경 후 단축키 재등록 요청
pub const WM_APP_RELOAD: u32 = WM_APP + 1;
/// 설정 창 열기 요청 (중복 실행 시 기존 인스턴스에 전달)
pub const WM_APP_SHOW: u32 = WM_APP + 2;
/// 업데이트 확인 요청 (wparam 1 이면 수동 확인)
pub const WM_APP_CHECK_UPDATE: u32 = WM_APP + 3;
/// 업데이트 확인 작업 완료 (wparam 1 이면 수동 확인)
const WM_APP_UPDATE_CHECKED: u32 = WM_APP + 4;
/// 업데이트 상태 변경 후 트레이 메뉴 문구 갱신 요청
pub const WM_APP_UPDATE_CHANGED: u32 = WM_APP + 5;
/// 프로그램 종료 요청 (업데이트 설치 시작 후)
pub const WM_APP_QUIT: u32 = WM_APP + 6;
/// 마우스 단축키 클릭 (마우스 훅이 보냄)
const WM_APP_MOUSE_TRIGGER: u32 = WM_APP + 7;

const KEYBOARD_HOTKEY_ID: i32 = 1;
const TIMER_FIRST_CHECK: usize = 1;
const TIMER_DAILY_CHECK: usize = 2;
/// 로그인 직후 네트워크가 준비될 때까지 첫 확인을 미룸
const FIRST_CHECK_DELAY_MS: u32 = 10_000;
const DAILY_CHECK_MS: u32 = 24 * 60 * 60 * 1000;

struct State {
    shared: Arc<Shared>,
    tx: Sender<MainCmd>,
    hwnd: HWND,
    keyboard_registered: bool,
    update_item: MenuItem,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn spawn(shared: Arc<Shared>, tx: Sender<MainCmd>) {
    thread::Builder::new()
        .name("gettext-service".into())
        .spawn(move || run(shared, tx))
        .expect("서비스 스레드 생성 실패");
}

fn run(shared: Arc<Shared>, tx: Sender<MainCmd>) {
    let hwnd = match create_message_window() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("service window creation failed: {e}");
            std::process::exit(1);
        }
    };
    shared.service_hwnd.store(hwnd.0 as isize, Ordering::SeqCst);

    let open_item = MenuItem::new("설정 열기", true, None);
    let update_item = MenuItem::new(UPDATE_MENU_CHECK, true, None);
    let quit_item = MenuItem::new("종료", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&open_item, &update_item, &PredefinedMenuItem::separator(), &quit_item]);
    let update_id = update_item.id().clone();

    let tray = build_tray(menu);

    STATE.with(|s| {
        *s.borrow_mut() = Some(State { shared, tx, hwnd, keyboard_registered: false, update_item });
    });
    reload_hotkeys();

    unsafe {
        SetTimer(Some(hwnd), TIMER_FIRST_CHECK, FIRST_CHECK_DELAY_MS, None);
        SetTimer(Some(hwnd), TIMER_DAILY_CHECK, DAILY_CHECK_MS, None);
    }

    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if ret.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                open_settings();
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *open_item.id() {
                open_settings();
            } else if ev.id == update_id {
                on_update_menu();
            } else if ev.id == *quit_item.id() {
                quit(tray);
            }
        }
    }
    quit(tray);
}

fn build_tray(menu: Menu) -> Option<TrayIcon> {
    let size = unsafe { (GetSystemMetrics(SM_CXSMICON) as u32, GetSystemMetrics(SM_CYSMICON) as u32) };
    let icon = Icon::from_resource(1, Some(size)).or_else(|_| Icon::from_resource(1, None)).ok();
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(format!("{APP_NAME} {APP_VERSION}"));
    if let Some(icon) = icon {
        builder = builder.with_icon(icon);
    }
    match builder.build() {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("tray icon creation failed: {e}");
            None
        }
    }
}

fn create_message_window() -> windows::core::Result<HWND> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: SERVICE_CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            SERVICE_CLASS,
            w!("GetText Service"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance.into()),
            None,
        )
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY if wparam.0 as i32 == KEYBOARD_HOTKEY_ID => {
            show_popup();
            LRESULT(0)
        }
        WM_APP_MOUSE_TRIGGER => {
            show_popup();
            LRESULT(0)
        }
        WM_APP_RELOAD => {
            reload_hotkeys();
            LRESULT(0)
        }
        WM_APP_SHOW => {
            open_settings();
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_FIRST_CHECK {
                unsafe {
                    let _ = KillTimer(Some(hwnd), TIMER_FIRST_CHECK);
                }
            }
            start_update_check(false);
            LRESULT(0)
        }
        WM_APP_CHECK_UPDATE => {
            start_update_check(wparam.0 == 1);
            LRESULT(0)
        }
        WM_APP_UPDATE_CHECKED => {
            on_update_checked(wparam.0 == 1);
            LRESULT(0)
        }
        WM_APP_UPDATE_CHANGED => {
            refresh_update_menu();
            LRESULT(0)
        }
        WM_APP_QUIT => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// 현재 커서 위치의 텍스트로 팝업 표시.
/// 훅 좌표는 고해상도 환경의 DPI 미지원 프로그램에서 어긋날 수 있어 커서 위치를 다시 조회함
fn show_popup() {
    let Some(config) = with_shared(|sh| sh.config.lock().unwrap().clone()) else { return };
    let mut pt = POINT::default();
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        return;
    }
    popup::spawn(config, pt);
}

const UPDATE_MENU_CHECK: &str = "업데이트 확인";

fn with_shared<R>(f: impl FnOnce(&Arc<Shared>) -> R) -> Option<R> {
    STATE.with(|s| s.borrow().as_ref().map(|st| f(&st.shared)))
}

/// 백그라운드 스레드에서 최신 릴리스 확인. 자동 확인은 설정이 꺼져 있으면 생략
fn start_update_check(manual: bool) {
    let Some(shared) = with_shared(|sh| sh.clone()) else { return };
    if !manual && !shared.config.lock().unwrap().check_updates {
        return;
    }
    {
        let mut st = shared.update.lock().unwrap();
        // 다운로드 중이거나 이미 확인 중이면 중복 실행하지 않음
        if st.checking || st.progress.is_some() {
            return;
        }
        st.checking = true;
    }
    thread::spawn(move || {
        let result = update::check();
        shared.update.lock().unwrap().check_result = Some(result);
        shared.post_service(WM_APP_UPDATE_CHECKED, manual as usize);
    });
}

/// 확인 결과 반영. 새 버전이면 트레이 메뉴를 바꾸고 설정 창으로 안내 (자동 확인은 실행 중 한 번만)
fn on_update_checked(manual: bool) {
    let Some(shared) = with_shared(|sh| sh.clone()) else { return };
    let result = {
        let mut st = shared.update.lock().unwrap();
        st.checking = false;
        st.check_result.take()
    };
    let Some(result) = result else { return };

    match result {
        Err(e) => {
            if manual {
                message_box(format!("업데이트 정보를 가져오지 못했습니다.\n{e}"), MB_ICONWARNING);
            }
        }
        Ok(None) => {
            shared.update.lock().unwrap().available = None;
            refresh_update_menu();
            if manual {
                message_box(format!("최신 버전({APP_VERSION})을 사용 중입니다."), MB_ICONINFORMATION);
            }
        }
        Ok(Some(info)) => {
            let skipped = shared.config.lock().unwrap().skipped_version.as_deref() == Some(info.tag.as_str());
            if !manual && skipped {
                return;
            }
            let notify = {
                let mut st = shared.update.lock().unwrap();
                st.available = Some(info);
                st.error = None;
                let notify = manual || !st.notified;
                st.notified = true;
                notify
            };
            refresh_update_menu();
            shared.repaint();
            if notify {
                open_settings();
            }
        }
    }
}

fn refresh_update_menu() {
    STATE.with(|s| {
        let guard = s.borrow();
        let Some(st) = guard.as_ref() else { return };
        let text = match &st.shared.update.lock().unwrap().available {
            Some(info) => format!("업데이트 v{} 설치...", info.version_text()),
            None => UPDATE_MENU_CHECK.to_string(),
        };
        st.update_item.set_text(text);
    });
}

/// 트레이 업데이트 항목: 설치 가능한 버전이 있으면 설정 창, 없으면 즉시 확인
fn on_update_menu() {
    let pending = with_shared(|sh| sh.update.lock().unwrap().available.is_some()).unwrap_or(false);
    if pending {
        open_settings();
    } else {
        start_update_check(true);
    }
}

/// 메시지 상자는 모달 루프 동안 트레이 이벤트 처리를 막지 않도록 별도 스레드에서 표시
fn message_box(text: String, icon: MESSAGEBOX_STYLE) {
    thread::spawn(move || unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(format!("{APP_NAME} 업데이트")),
            MB_OK | icon | MB_TOPMOST | MB_SETFOREGROUND,
        );
    });
}

/// 키보드 단축키 재등록과 마우스 훅 설정 적용. 실패 사유는 공유 상태에 기록
fn reload_hotkeys() {
    STATE.with(|s| {
        let mut guard = s.borrow_mut();
        let Some(st) = guard.as_mut() else { return };

        if st.keyboard_registered {
            unsafe {
                let _ = UnregisterHotKey(Some(st.hwnd), KEYBOARD_HOTKEY_ID);
            }
            st.keyboard_registered = false;
        }

        let config = st.shared.config.lock().unwrap().clone();
        let mut status = crate::HotkeyStatus::default();

        let kb = &config.keyboard;
        if kb.enabled() {
            match (kb.validate(), key_to_vk(&kb.key)) {
                (Err(e), _) => status.keyboard = Some(e),
                (Ok(()), None) => status.keyboard = Some("키보드 단축키의 키가 올바르지 않습니다.".into()),
                (Ok(()), Some(vk)) => {
                    let mut mods = MOD_NOREPEAT;
                    if kb.win {
                        mods |= MOD_WIN;
                    }
                    if kb.ctrl {
                        mods |= MOD_CONTROL;
                    }
                    if kb.alt {
                        mods |= MOD_ALT;
                    }
                    if kb.shift {
                        mods |= MOD_SHIFT;
                    }
                    match unsafe { RegisterHotKey(Some(st.hwnd), KEYBOARD_HOTKEY_ID, mods, vk) } {
                        Ok(()) => st.keyboard_registered = true,
                        Err(_) => {
                            status.keyboard = Some("다른 프로그램이나 Windows 가 이미 사용 중인 단축키입니다.".into())
                        }
                    }
                }
            }
        }

        let mouse_result = match config.mouse.validate() {
            Ok(()) => mousehook::apply(st.hwnd, WM_APP_MOUSE_TRIGGER, &config.mouse, config.normalized_excluded()),
            Err(e) => {
                mousehook::stop();
                Err(e)
            }
        };
        if let Err(e) = mouse_result {
            status.mouse = Some(e);
        }

        *st.shared.status.lock().unwrap() = status;
        st.shared.repaint();
    });
}

fn open_settings() {
    STATE.with(|s| {
        let guard = s.borrow();
        let Some(st) = guard.as_ref() else { return };
        let ui = st.shared.ui_hwnd.load(Ordering::SeqCst);
        if ui != 0 {
            focus_window(HWND(ui as *mut _));
        } else {
            let _ = st.tx.send(MainCmd::OpenSettings);
        }
    });
}

pub fn focus_window(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

fn quit(tray: Option<TrayIcon>) -> ! {
    STATE.with(|s| {
        if let Some(st) = s.borrow().as_ref()
            && st.keyboard_registered
        {
            unsafe {
                let _ = UnregisterHotKey(Some(st.hwnd), KEYBOARD_HOTKEY_ID);
            }
        }
    });
    mousehook::stop();
    // 트레이 아이콘을 먼저 제거하지 않으면 종료 후에도 아이콘이 남음
    drop(tray);
    std::process::exit(0);
}
