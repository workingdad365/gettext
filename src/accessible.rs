//! 접근성 API(MSAA, UI Automation)로 화면 좌표 아래 요소의 텍스트와 위치 조회.
//! 호출 스레드는 COM 이 초기화되어 있어야 함

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_I4, VariantClear};
use windows::Win32::UI::Accessibility::{
    AccessibleObjectFromPoint, CUIAutomation, IAccessible, IUIAutomation, IUIAutomationElement,
    ROLE_SYSTEM_TITLEBAR, UIA_ValueValuePropertyId, WindowFromAccessibleObject,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use windows::core::Interface;

use crate::config::TextMethod;

const CHILDID_SELF: i32 = 0;

#[derive(Debug, Clone, Default)]
pub struct AccInfo {
    /// 요소가 속한 창
    pub hwnd: HWND,
    /// 이름, 값, 설명 등 텍스트 조각 (빈 조각 제외)
    pub parts: Vec<String>,
    /// 요소의 화면 좌표 영역
    pub rect: RECT,
}

impl AccInfo {
    fn empty(pt: POINT) -> Self {
        Self { hwnd: HWND::default(), parts: Vec::new(), rect: RECT { left: pt.x, top: pt.y, right: pt.x, bottom: pt.y } }
    }
}

pub fn from_point(method: TextMethod, pt: POINT) -> AccInfo {
    match method {
        TextMethod::Msaa => msaa(pt),
        TextMethod::Uia => uia(pt).unwrap_or_else(|| AccInfo::empty(pt)),
        TextMethod::Auto => auto(pt),
    }
}

/// MSAA: Windows 11 작업 표시줄 같은 최신 UI 를 지원하지 않음.
/// UIA: 가장 안쪽 요소를 돌려주지 않는 경우가 있음 (Chromium 의 링크 등).
/// 그래서 둘 다 조회하고, MSAA 영역이 UIA 영역 안쪽에 있으면 더 구체적인 요소로 보고 MSAA 결과 사용
fn auto(pt: POINT) -> AccInfo {
    let Some(u) = uia(pt) else { return msaa(pt) };
    let m = msaa(pt);
    if u.parts.is_empty() && !m.parts.is_empty() {
        return m;
    }
    if !m.parts.is_empty() && strictly_inside(&m.rect, &u.rect) { m } else { u }
}

/// inner 가 outer 안에 있고 적어도 한 변이 더 안쪽인지 여부
pub fn strictly_inside(inner: &RECT, outer: &RECT) -> bool {
    inner.left >= outer.left
        && inner.top >= outer.top
        && inner.right <= outer.right
        && inner.bottom <= outer.bottom
        && (inner.left > outer.left
            || inner.top > outer.top
            || inner.right < outer.right
            || inner.bottom < outer.bottom)
}

/// 텍스트 조각 추가. 줄바꿈만 있는 조각은 제외
fn push_part(parts: &mut Vec<String>, s: &str) {
    if !crate::text::normalize_part(s).is_empty() {
        parts.push(s.to_string());
    }
}

/// 해제를 자동으로 처리하는 VARIANT
struct Var(VARIANT);

impl Var {
    fn new() -> Self {
        Var(VARIANT::default())
    }

    fn i4(value: i32) -> Self {
        let mut v = VARIANT::default();
        unsafe {
            (*v.Anonymous.Anonymous).vt = VT_I4;
            (*v.Anonymous.Anonymous).Anonymous.lVal = value;
        }
        Var(v)
    }

    fn as_i4(&self) -> Option<i32> {
        unsafe {
            let inner = &*self.0.Anonymous.Anonymous;
            (inner.vt == VT_I4).then_some(inner.Anonymous.lVal)
        }
    }

    fn as_string(&self) -> Option<String> {
        unsafe {
            let inner = &*self.0.Anonymous.Anonymous;
            (inner.vt == VT_BSTR).then(|| inner.Anonymous.bstrVal.to_string())
        }
    }
}

impl Drop for Var {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.0);
        }
    }
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn msaa_query(pt: POINT) -> Option<(IAccessible, Var)> {
    let mut acc: Option<IAccessible> = None;
    let mut child = Var::new();
    unsafe { AccessibleObjectFromPoint(pt, &mut acc, &mut child.0).ok()? };
    Some((acc?, child))
}

fn msaa_window(acc: &IAccessible) -> Option<HWND> {
    let mut hwnd = HWND::default();
    unsafe { WindowFromAccessibleObject(acc, Some(&mut hwnd)).ok()? };
    Some(hwnd)
}

fn msaa(pt: POINT) -> AccInfo {
    let mut info = AccInfo::empty(pt);
    // Chromium 은 두 번째 조회에서야 올바른 요소를 돌려주는 경우가 있어 두 번 조회
    if msaa_query(pt).is_none() {
        return info;
    }
    let Some((mut acc, mut child)) = msaa_query(pt) else { return info };
    let Some(hwnd) = msaa_window(&acc) else { return info };
    info.hwnd = hwnd;
    let pid = window_pid(hwnd);

    loop {
        let mut parts = Vec::new();
        unsafe {
            let name = acc.get_accName(&child.0).ok().map(|b| b.to_string());
            if let Some(n) = &name {
                push_part(&mut parts, n);
            }
            let value = acc.get_accValue(&child.0).ok().map(|b| b.to_string());
            if let Some(v) = &value
                && value != name
            {
                push_part(&mut parts, v);
            }
            // 제목 표시줄의 설명은 의미 없는 안내 문구라 제외
            let role = acc.get_accRole(&Var::i4(CHILDID_SELF).0).ok().map(Var);
            let is_titlebar = role.and_then(|r| r.as_i4()) == Some(ROLE_SYSTEM_TITLEBAR as i32);
            if !is_titlebar
                && let Ok(desc) = acc.get_accDescription(&child.0)
            {
                let desc = Some(desc.to_string());
                if desc != name && desc != value {
                    push_part(&mut parts, desc.as_deref().unwrap_or_default());
                }
            }
        }
        if !parts.is_empty() {
            info.parts = parts;
            break;
        }

        if child.as_i4().unwrap_or(CHILDID_SELF) == CHILDID_SELF {
            // 텍스트가 없으면 같은 프로세스 안에서 부모 요소로 올라감
            let Some(parent) = (unsafe { acc.accParent() }).ok().and_then(|d| d.cast::<IAccessible>().ok()) else {
                break;
            };
            let Some(parent_hwnd) = msaa_window(&parent) else { break };
            if window_pid(parent_hwnd) != pid {
                break;
            }
            acc = parent;
        } else {
            child = Var::i4(CHILDID_SELF);
        }
    }

    let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
    if unsafe { acc.accLocation(&mut x, &mut y, &mut w, &mut h, &child.0) }.is_ok() {
        info.rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    }
    info
}

/// UIA 를 쓸 수 없으면 None
fn uia(pt: POINT) -> Option<AccInfo> {
    let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let mut info = AccInfo::empty(pt);

    unsafe {
        if automation.ElementFromPoint(pt).is_err() {
            return Some(info);
        }
        let Ok(mut element) = automation.ElementFromPoint(pt) else { return Some(info) };
        let Ok(condition) = automation.CreateTrueCondition() else { return Some(info) };
        let Ok(walker) = automation.CreateTreeWalker(&condition) else { return Some(info) };
        let Ok(pid) = element.CurrentProcessId() else { return Some(info) };

        loop {
            let mut parts = Vec::new();
            let name = element.CurrentName().ok().map(|b| b.to_string());
            if let Some(n) = &name {
                push_part(&mut parts, n);
            }
            if let Ok(value) = element.GetCurrentPropertyValue(UIA_ValueValuePropertyId) {
                let value = Var(value).as_string();
                if let Some(v) = &value
                    && value != name
                {
                    push_part(&mut parts, v);
                }
            }
            if !parts.is_empty() {
                info.parts = parts;
                break;
            }
            let Ok(parent) = walker.GetParentElement(&element) else { break };
            if parent.CurrentProcessId().ok() != Some(pid) {
                break;
            }
            element = parent;
        }

        info.hwnd = native_window(&walker, &element);
        if let Ok(rect) = element.CurrentBoundingRectangle() {
            info.rect = rect;
        }
    }
    Some(info)
}

/// 요소 자체에 창 핸들이 없으면 핸들이 있는 조상까지 올라가며 찾음
unsafe fn native_window(
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
) -> HWND {
    unsafe {
        let mut current = element.clone();
        for _ in 0..64 {
            match current.CurrentNativeWindowHandle() {
                Ok(h) if !h.is_invalid() => return h,
                Err(_) => break,
                _ => {}
            }
            match walker.GetParentElement(&current) {
                Ok(p) => current = p,
                Err(_) => break,
            }
        }
    }
    HWND::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(l: i32, t: i32, rr: i32, b: i32) -> RECT {
        RECT { left: l, top: t, right: rr, bottom: b }
    }

    #[test]
    fn inside_check() {
        assert!(strictly_inside(&r(1, 0, 10, 10), &r(0, 0, 10, 10)));
        assert!(!strictly_inside(&r(0, 0, 10, 10), &r(0, 0, 10, 10)));
        assert!(!strictly_inside(&r(-1, 0, 5, 5), &r(0, 0, 10, 10)));
        assert!(strictly_inside(&r(2, 2, 3, 3), &r(0, 0, 10, 10)));
    }

    #[test]
    fn variant_roundtrip() {
        let v = Var::i4(7);
        assert_eq!(v.as_i4(), Some(7));
        assert_eq!(v.as_string(), None);
        assert_eq!(Var::new().as_i4(), None);
    }

    /// 실제 화면 조회 (대화형 세션 필요: cargo test -- --ignored)
    #[test]
    #[ignore]
    fn live_query_does_not_crash() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            for method in TextMethod::ALL {
                let info = from_point(method, POINT { x: 10, y: 10 });
                eprintln!("{method:?}: {:?} {:?}", info.parts, info.rect);
            }
            CoUninitialize();
        }
    }
}
