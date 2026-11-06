//! 추출한 텍스트를 보여주는 팝업 편집 창 (Win32 EDIT 컨트롤).
//! 팝업마다 전용 스레드에서 실행되며, 창이 닫히면 스레드도 끝남

use std::sync::Once;
use std::thread;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, CreateFontIndirectW, DT_CALCRECT, DT_EDITCONTROL, DT_EXPANDTABS, DT_NOPREFIX, DT_SINGLELINE,
    DT_WORDBREAK, DeleteObject, DrawTextW, GetDC, GetMonitorInfoW, HBRUSH, HDC, HFONT, HGDIOBJ, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Controls::{
    EM_GETSEL, EM_SCROLLCARET, EM_SETLIMITTEXT, EM_SETMARGINS, EM_SETSEL, ShowScrollBar, TOOLTIPS_CLASSW,
    TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, INPUT, SendInput, SetFocus, VK_CONTROL, VK_ESCAPE, VK_TAB};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, BS_PUSHBUTTON, CREATESTRUCTW, CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, EC_LEFTMARGIN, EC_RIGHTMARGIN, EN_CHANGE, ES_AUTOVSCROLL, ES_MULTILINE, ES_WANTRETURN, GA_ROOT,
    GWL_STYLE, GWLP_USERDATA, GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HMENU,
    HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MINMAXINFO, MSG,
    MessageBoxW, NONCLIENTMETRICSW, PostMessageW, PostQuitMessage, RegisterClassExW, SB_VERT, SPI_GETNONCLIENTMETRICS,
    SW_HIDE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, UISF_HIDEACCEL, UIS_CLEAR,
    WA_INACTIVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_CHANGEUISTATE, WM_CHAR, WM_CLOSE, WM_COMMAND,
    WM_DESTROY, WM_EXITSIZEMOVE, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY,
    WM_NCHITTEST, WM_SETFONT, WM_SIZE, WM_SYSCHAR, WM_SYSKEYDOWN, WNDCLASSEXW, WS_BORDER, WS_CAPTION,
    WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME,
    WS_VISIBLE, WindowFromPoint,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use crate::config::{APP_NAME, Config, WebButton};
use crate::text::{Joined, measure_text, next_tab_selection, utf16_len};
use crate::{accessible, clipboard, launch};

const CLASS: PCWSTR = w!("GetText.Popup");
const ID_EDIT: i32 = 100;
const ID_BUTTON_BASE: i32 = 1000;
pub const DEFAULT_TEXT: &str = "(텍스트를 가져오지 못했습니다)";

struct State {
    config: Config,
    edit: HWND,
    buttons: Vec<HWND>,
    /// 버튼별 단축 글자
    accels: Vec<Option<char>>,
    button_sizes: Vec<SIZE>,
    button_height: i32,
    per_row: usize,
    /// Tab 키로 순환할 조각 시작 위치 (고정되면 비움)
    indexes: Vec<i32>,
    last_sel: (i32, i32),
    /// 최소 클라이언트 크기
    min_client: SIZE,
    dpi: u32,
    fonts: Vec<HFONT>,
    /// 초기화가 끝났는지 여부 (초기화 중 알림 무시)
    ready: bool,
    /// 편집, 이동, 크기 조절로 고정되면 포커스를 잃어도 닫히지 않음
    pinned: bool,
    /// 버튼 명령 실행 중 (창을 숨겨도 닫지 않음)
    launching: bool,
}

/// 커서 위치의 텍스트로 팝업 창을 띄움 (별도 스레드)
pub fn spawn(config: Config, pt: POINT) {
    let _ = thread::Builder::new().name("gettext-popup".into()).spawn(move || unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        show(config, pt);
        if com {
            CoUninitialize();
        }
    });
}

/// 버튼 문구와 단축 글자. 단축 글자가 없으면 1-9 순번 사용
pub fn button_caption(index: usize, button: &WebButton) -> (String, Option<char>) {
    let accel = button.accel().or_else(|| (index < 9).then(|| char::from_digit(index as u32 + 1, 10).unwrap()));
    let name = button.name.trim();
    let name = if name.is_empty() { format!("{}", index + 1) } else { name.replace('&', "&&") };
    let Some(a) = accel else { return (name, None) };
    match name.char_indices().find(|(_, c)| c.to_ascii_uppercase() == a) {
        Some((i, _)) => (format!("{}&{}", &name[..i], &name[i..]), accel),
        None => (format!("{name} (&{a})"), accel),
    }
}

fn is_popup(hwnd: HWND) -> bool {
    if hwnd.is_invalid() {
        return false;
    }
    let mut buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..len]) == "GetText.Popup"
}

fn register_class() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as usize as *mut _),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);
    });
}

unsafe fn show(config: Config, pt: POINT) {
    let info = accessible::from_point(config.text_method, pt);

    // 단축키를 팝업 위에서 누르면 그 팝업을 닫음
    for candidate in [info.hwnd, unsafe { WindowFromPoint(pt) }] {
        if candidate.is_invalid() {
            continue;
        }
        let root = unsafe { GetAncestor(candidate, GA_ROOT) };
        if is_popup(root) {
            unsafe {
                let _ = PostMessageW(Some(root), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
    }

    register_class();
    let joined = Joined::from_parts(&info.parts, config.unicode_spaces_to_ascii);
    let (text, indexes) = if joined.text.is_empty() {
        (DEFAULT_TEXT.to_string(), Vec::new())
    } else {
        (joined.text, joined.indexes)
    };

    let style = WS_POPUP | WS_CLIPCHILDREN | if config.non_resizable { WS_BORDER } else { WS_THICKFRAME };
    let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
    let state = Box::new(State {
        config,
        edit: HWND::default(),
        buttons: Vec::new(),
        accels: Vec::new(),
        button_sizes: Vec::new(),
        button_height: 0,
        per_row: 1,
        indexes,
        last_sel: (0, 0),
        min_client: SIZE::default(),
        dpi: 96,
        fonts: Vec::new(),
        ready: false,
        pinned: false,
        launching: false,
    });
    let state_ptr = Box::into_raw(state);

    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        // 커서 위치에 만들어야 해당 모니터의 DPI 를 얻을 수 있음
        let hwnd = match CreateWindowExW(
            ex_style,
            CLASS,
            &HSTRING::from(APP_NAME),
            style,
            pt.x,
            pt.y,
            1,
            1,
            None,
            None,
            Some(hinstance.into()),
            Some(state_ptr as *const _),
        ) {
            Ok(h) => h,
            Err(_) => {
                drop(Box::from_raw(state_ptr));
                return;
            }
        };
        let st = &mut *state_ptr;
        setup(hwnd, st, &text, pt, &info.rect, style, ex_style);
        st.ready = true;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

unsafe fn message_font(dpi: u32) -> windows::Win32::Graphics::Gdi::LOGFONTW {
    let mut ncm = NONCLIENTMETRICSW { cbSize: size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
    unsafe {
        let _ = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut _),
            0,
            dpi,
        );
    }
    ncm.lfMessageFont
}

unsafe fn measure(hdc: HDC, font: HFONT, text: &str, max_width: i32, single_line: bool) -> SIZE {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        wide.push(' ' as u16);
    }
    let mut rc = RECT { left: 0, top: 0, right: max_width, bottom: 0 };
    let format = if single_line {
        DT_CALCRECT | DT_SINGLELINE
    } else {
        DT_CALCRECT | DT_EDITCONTROL | DT_WORDBREAK | DT_NOPREFIX | DT_EXPANDTABS
    };
    unsafe {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        DrawTextW(hdc, &mut wide, &mut rc, format);
        SelectObject(hdc, old);
    }
    SIZE { cx: rc.right - rc.left, cy: rc.bottom - rc.top }
}

unsafe fn setup(hwnd: HWND, st: &mut State, text: &str, pt: POINT, acc: &RECT, style: WINDOW_STYLE, ex_style: WINDOW_EX_STYLE) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96);
        st.dpi = dpi;
        let s = |v: i32| scale(v, dpi);

        // 글꼴: 버튼은 시스템 글꼴, 편집 영역은 설정 글꼴
        let base = message_font(dpi);
        let ui_font = CreateFontIndirectW(&base);
        let mut edit_lf = base;
        let name = st.config.font_name.trim();
        if !name.is_empty() {
            edit_lf.lfFaceName = [0; 32];
            for (i, c) in name.encode_utf16().take(31).enumerate() {
                edit_lf.lfFaceName[i] = c;
            }
        }
        if st.config.font_size > 0 {
            edit_lf.lfHeight = -(st.config.font_size as i32 * dpi as i32 / 72);
        }
        let edit_font = CreateFontIndirectW(&edit_lf);
        st.fonts = vec![ui_font, edit_font];

        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            &HSTRING::from(text),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            0,
            0,
            0,
            0,
            Some(hwnd),
            Some(HMENU(ID_EDIT as usize as *mut _)),
            Some(hinstance.into()),
            None,
        )
        .unwrap_or_default();
        st.edit = edit;
        let margin = s(4);
        SendMessageW(edit, WM_SETFONT, Some(WPARAM(edit_font.0 as usize)), Some(LPARAM(0)));
        SendMessageW(edit, EM_SETLIMITTEXT, Some(WPARAM(0)), Some(LPARAM(0)));
        SendMessageW(
            edit,
            EM_SETMARGINS,
            Some(WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize)),
            Some(LPARAM(((margin as u32) | ((margin as u32) << 16)) as isize)),
        );
        let _ = SetWindowSubclass(edit, Some(edit_proc), 1, hwnd.0 as usize);

        // 명령 버튼과 툴팁
        let hdc = GetDC(Some(hwnd));
        let tooltip = if st.config.web_buttons.is_empty() {
            HWND::default()
        } else {
            let tip = CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_NOPREFIX | TTS_ALWAYSTIP),
                0,
                0,
                0,
                0,
                Some(hwnd),
                None,
                Some(hinstance.into()),
                None,
            )
            .unwrap_or_default();
            SendMessageW(tip, TTM_SETMAXTIPWIDTH, Some(WPARAM(0)), Some(LPARAM(s(480) as isize)));
            tip
        };
        let line = measure(hdc, ui_font, "Ag", i32::MAX, true);
        st.button_height = line.cy + s(10);
        for (i, b) in st.config.web_buttons.clone().iter().enumerate() {
            let (caption, accel) = button_caption(i, b);
            let size = measure(hdc, ui_font, &caption, i32::MAX, true);
            let width = (size.cx + s(20)).max(s(40));
            let btn = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("BUTTON"),
                &HSTRING::from(caption.as_str()),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                0,
                0,
                width,
                st.button_height,
                Some(hwnd),
                Some(HMENU((ID_BUTTON_BASE + i as i32) as usize as *mut _)),
                Some(hinstance.into()),
                None,
            )
            .unwrap_or_default();
            SendMessageW(btn, WM_SETFONT, Some(WPARAM(ui_font.0 as usize)), Some(LPARAM(0)));
            if !tooltip.is_invalid() {
                let mut tip_text: Vec<u16> = b.command.encode_utf16().chain(std::iter::once(0)).collect();
                let ti = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                    hwnd,
                    uId: btn.0 as usize,
                    lpszText: PWSTR(tip_text.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(tooltip, TTM_ADDTOOLW, Some(WPARAM(0)), Some(LPARAM(&ti as *const _ as isize)));
            }
            st.buttons.push(btn);
            st.accels.push(accel);
            st.button_sizes.push(SIZE { cx: width, cy: st.button_height });
        }
        st.per_row = match st.config.buttons_per_row {
            0 => st.buttons.len().max(1),
            n => n as usize,
        };
        let (area_w, area_h) = button_area(st);

        // 편집 영역 크기: 요소 너비에 맞추되 너무 좁으면 넓힘
        let pad_w = 2 * margin + s(6);
        let pad_h = s(6);
        let def = measure(hdc, edit_font, DEFAULT_TEXT, i32::MAX, false);
        let def_client = SIZE { cx: def.cx + pad_w, cy: def.cy + pad_h };
        let acc_w = (acc.right - acc.left).max(0);
        let acc_h = (acc.bottom - acc.top).max(0);
        let max_client_w = def_client.cx.max(acc_w).max(s(400));
        let ts = measure(hdc, edit_font, &measure_text(text), max_client_w - pad_w, false);
        ReleaseDC(Some(hwnd), hdc);

        let mut cw = ts.cx + pad_w;
        let ch = (ts.cy + pad_h).max(def_client.cy);
        // 요소보다 조금 좁으면 요소 너비에 맞추는 편이 보기 좋음
        if cw < acc_w && (acc_w <= s(200) || cw * 3 / 2 >= acc_w) {
            cw = acc_w;
        }
        cw = cw.max(area_w);

        let mut client = RECT { left: acc.left, top: acc.top, right: acc.left + cw, bottom: acc.top + ch + area_h };
        let inside = pt.x >= client.left && pt.x < client.right && pt.y >= client.top && pt.y < client.top + ch;
        if acc_w == 0 || acc_h == 0 || !inside {
            let (w, h) = (client.right - client.left, client.bottom - client.top);
            client = RECT { left: pt.x - w / 2, top: pt.y - ch / 2, right: pt.x - w / 2 + w, bottom: pt.y - ch / 2 + h };
        }

        st.min_client = SIZE { cx: area_w.max(s(48)), cy: def_client.cy + area_h };

        let mut rc = client;
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex_style, dpi);
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(monitor, &mut mi).as_bool() {
            let work = mi.rcWork;
            let (mut w, mut h) = (rc.right - rc.left, rc.bottom - rc.top);
            let (ww, wh) = (work.right - work.left, work.bottom - work.top);
            if w > ww || h > wh {
                let _ = ShowScrollBar(edit, SB_VERT, true);
                w = w.min(ww);
                h = h.min(wh);
            }
            let x = rc.left.clamp(work.left, work.right - w);
            let y = rc.top.clamp(work.top, work.bottom - h);
            rc = RECT { left: x, top: y, right: x + w, bottom: y + h };
        }
        let _ = SetWindowPos(hwnd, None, rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, SWP_NOZORDER | SWP_NOACTIVATE);

        // 단축 글자 밑줄을 항상 표시
        SendMessageW(hwnd, WM_CHANGEUISTATE, Some(WPARAM(((UISF_HIDEACCEL << 16) | UIS_CLEAR) as usize)), None);

        let _ = ShowWindow(hwnd, SW_SHOW);
        force_foreground(hwnd);
        let _ = SetFocus(Some(edit));
        SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
        st.last_sel = (0, utf16_len(text));
        if st.config.auto_copy_selection {
            clipboard::set_text(Some(hwnd), text);
        }
    }
}

/// 다른 프로그램이 포그라운드일 때도 팝업이 포커스를 받도록 함
unsafe fn force_foreground(hwnd: HWND) {
    unsafe {
        // 빈 입력을 보내 이 프로세스가 마지막 입력을 받은 것으로 만들면 포커스 전환이 허용됨
        let input = INPUT::default();
        SendInput(&[input], size_of::<INPUT>() as i32);
        if SetForegroundWindow(hwnd).as_bool() {
            return;
        }
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        if fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool() {
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
}

/// 버튼 영역의 (너비, 높이)
fn button_area(st: &State) -> (i32, i32) {
    if st.buttons.is_empty() {
        return (0, 0);
    }
    let rows = st.button_sizes.chunks(st.per_row);
    let width = rows.clone().map(|r| r.iter().map(|s| s.cx).sum::<i32>()).max().unwrap_or(0);
    (width, rows.count() as i32 * st.button_height)
}

unsafe fn layout(hwnd: HWND, st: &State) {
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (_, area_h) = button_area(st);
        let edit_h = (rc.bottom - area_h).max(0);
        let _ = SetWindowPos(st.edit, None, 0, 0, rc.right, edit_h, SWP_NOZORDER | SWP_NOACTIVATE);
        let mut y = edit_h;
        for (row_buttons, row_sizes) in st.buttons.chunks(st.per_row).zip(st.button_sizes.chunks(st.per_row)) {
            let mut x = 0;
            for (b, s) in row_buttons.iter().zip(row_sizes) {
                let _ = SetWindowPos(*b, None, x, y, s.cx, s.cy, SWP_NOZORDER | SWP_NOACTIVATE);
                x += s.cx;
            }
            y += st.button_height;
        }
    }
}

unsafe fn state_ptr(hwnd: HWND) -> *mut State {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State }
}

fn window_style(hwnd: HWND) -> WINDOW_STYLE {
    WINDOW_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32)
}

fn ex_style(hwnd: HWND) -> WINDOW_EX_STYLE {
    use windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE;
    WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32)
}

/// 창 테두리 두께 (현재 스타일 기준)
fn frame_rect(hwnd: HWND, dpi: u32) -> RECT {
    let mut rc = RECT::default();
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rc, window_style(hwnd), false, ex_style(hwnd), dpi);
    }
    rc
}

/// 제목 표시줄과 닫기 버튼을 붙여 고정 창으로 전환
unsafe fn pin(hwnd: HWND, p: *mut State) {
    unsafe {
        if (*p).pinned {
            return;
        }
        (*p).pinned = true;
        (*p).indexes.clear();
        let dpi = (*p).dpi;

        let mut rc = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rc);
        let f = frame_rect(hwnd, dpi);
        let client = RECT { left: rc.left - f.left, top: rc.top - f.top, right: rc.right - f.right, bottom: rc.bottom - f.bottom };

        let _ = SetWindowTextW(hwnd, &HSTRING::from(APP_NAME));
        let style = (window_style(hwnd) & !WS_BORDER) | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style.0 as isize);
        let f = frame_rect(hwnd, dpi);
        let _ = SetWindowPos(
            hwnd,
            None,
            client.left + f.left,
            client.top + f.top,
            (client.right + f.right) - (client.left + f.left),
            (client.bottom + f.bottom) - (client.top + f.top),
            SWP_NOZORDER | SWP_FRAMECHANGED,
        );
        let _ = ShowScrollBar((*p).edit, SB_VERT, true);
    }
}

unsafe fn edit_text(edit: HWND) -> Vec<u16> {
    unsafe {
        let len = GetWindowTextLengthW(edit).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(edit, &mut buf).max(0) as usize;
        buf.truncate(n);
        buf
    }
}

unsafe fn edit_sel(edit: HWND) -> (i32, i32) {
    let (mut start, mut end) = (0u32, 0u32);
    unsafe {
        SendMessageW(
            edit,
            EM_GETSEL,
            Some(WPARAM(&mut start as *mut u32 as usize)),
            Some(LPARAM(&mut end as *mut u32 as isize)),
        );
    }
    (start as i32, end as i32)
}

/// 선택 영역이 바뀌었으면 자동 복사
unsafe fn selection_changed(hwnd: HWND) {
    unsafe {
        let p = state_ptr(hwnd);
        if p.is_null() || !(*p).config.auto_copy_selection {
            return;
        }
        let sel = edit_sel((*p).edit);
        if sel.0 < sel.1 && sel != (*p).last_sel {
            let text = edit_text((*p).edit);
            let (s, e) = (sel.0 as usize, (sel.1 as usize).min(text.len()));
            if s < e {
                clipboard::set_text(Some(hwnd), &String::from_utf16_lossy(&text[s..e]));
            }
            (*p).last_sel = sel;
        }
    }
}

/// 버튼 명령 실행. 선택 영역이 있으면 선택한 부분만 넘김
unsafe fn run_button(hwnd: HWND, p: *mut State, index: usize) {
    unsafe {
        let Some(button) = (&(*p).config.web_buttons).get(index).cloned() else { return };
        let edit = (*p).edit;
        let text = edit_text(edit);
        let (s, e) = edit_sel(edit);
        let (s, e) = (s.max(0) as usize, (e.max(0) as usize).min(text.len()));
        let selected = if s < e { &text[s..e] } else { &text[..] };
        let selected = String::from_utf16_lossy(selected);

        (*p).launching = true;
        let _ = ShowWindow(hwnd, SW_HIDE);
        if let Err(e) = launch::run(&button.command, &selected, button.width, button.height) {
            MessageBoxW(
                None,
                &HSTRING::from(e),
                &HSTRING::from(APP_NAME),
                MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND,
            );
        }
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                let p = state_ptr(hwnd);
                if !p.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let st = Box::from_raw(p);
                    for f in &st.fonts {
                        let _ = DeleteObject(HGDIOBJ(f.0));
                    }
                }
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            _ => {}
        }

        let p = state_ptr(hwnd);
        if p.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE && (*p).ready && !(*p).pinned && !(*p).launching {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
                if id == ID_EDIT && code == EN_CHANGE {
                    if (*p).ready {
                        pin(hwnd, p);
                    }
                } else if id >= ID_BUTTON_BASE && code == BN_CLICKED && !(*p).launching {
                    run_button(hwnd, p, (id - ID_BUTTON_BASE) as usize);
                }
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                pin(hwnd, p);
                LRESULT(0)
            }
            WM_SIZE => {
                layout(hwnd, &*p);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
                let f = frame_rect(hwnd, (*p).dpi);
                mmi.ptMinTrackSize.x = (*p).min_client.cx + f.right - f.left;
                mmi.ptMinTrackSize.y = (*p).min_client.cy + f.bottom - f.top;
                LRESULT(0)
            }
            WM_NCHITTEST => {
                // 빈 영역을 끌어 창을 옮길 수 있도록 함
                let r = DefWindowProcW(hwnd, msg, wparam, lparam);
                if r.0 as u32 == HTCLIENT { LRESULT(HTCAPTION as isize) } else { r }
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Alt+글자에 해당하는 버튼 순번
unsafe fn accel_button(parent: HWND, vk: usize) -> Option<usize> {
    let p = unsafe { state_ptr(parent) };
    if p.is_null() {
        return None;
    }
    let c = char::from_u32(vk as u32)?.to_ascii_uppercase();
    unsafe { (*p).accels.iter().position(|a| *a == Some(c)) }
}

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    refdata: usize,
) -> LRESULT {
    let parent = HWND(refdata as *mut _);
    unsafe {
        match msg {
            WM_KEYDOWN if wparam.0 == VK_TAB.0 as usize && GetKeyState(VK_CONTROL.0 as i32) >= 0 => {
                let p = state_ptr(parent);
                if !p.is_null() {
                    let len = GetWindowTextLengthW(hwnd);
                    let (s, e) = next_tab_selection(edit_sel(hwnd), len, &(*p).indexes);
                    SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(s as usize)), Some(LPARAM(e as isize)));
                    SendMessageW(hwnd, EM_SCROLLCARET, None, None);
                    selection_changed(parent);
                }
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
                let _ = PostMessageW(Some(parent), WM_CLOSE, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            // Ctrl+A 전체 선택. Tab, Esc 문자는 입력하지 않음
            WM_CHAR if wparam.0 == 1 => {
                SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                selection_changed(parent);
                LRESULT(0)
            }
            WM_CHAR if (wparam.0 == 9 && GetKeyState(VK_CONTROL.0 as i32) >= 0) || wparam.0 == 27 => LRESULT(0),
            WM_SYSKEYDOWN => match accel_button(parent, wparam.0) {
                Some(i) => {
                    let p = state_ptr(parent);
                    let btn = (&(*p).buttons)[i];
                    let id = (ID_BUTTON_BASE + i as i32) as usize | ((BN_CLICKED as usize) << 16);
                    let _ = PostMessageW(Some(parent), WM_COMMAND, WPARAM(id), LPARAM(btn.0 as isize));
                    LRESULT(0)
                }
                None => DefSubclassProc(hwnd, msg, wparam, lparam),
            },
            WM_SYSCHAR if accel_button(parent, (wparam.0 as u8).to_ascii_uppercase() as usize).is_some() => LRESULT(0),
            WM_KEYUP | WM_LBUTTONUP => {
                let r = DefSubclassProc(hwnd, msg, wparam, lparam);
                selection_changed(parent);
                r
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(edit_proc), 1);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn btn(name: &str, key: &str) -> WebButton {
        WebButton { name: name.into(), command: "x".into(), key: key.into(), ..Default::default() }
    }

    #[test]
    fn captions() {
        assert_eq!(button_caption(0, &btn("구글 검색", "G")), ("구글 검색 (&G)".into(), Some('G')));
        assert_eq!(button_caption(0, &btn("Google", "g")), ("&Google".into(), Some('G')));
        assert_eq!(button_caption(1, &btn("Search", "")), ("Search (&2)".into(), Some('2')));
        assert_eq!(button_caption(2, &btn("", "")), ("&3".into(), Some('3')));
        assert_eq!(button_caption(9, &btn("Ten", "")), ("Ten".into(), None));
        assert_eq!(button_caption(0, &btn("A&B", "B")), ("A&&&B".into(), Some('B')));
    }
}
