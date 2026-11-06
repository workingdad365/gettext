//! 팝업 창 버튼 명령 실행.
//! 자리 표시자: %s (URL 인코딩), %ds (이중 URL 인코딩), %cs (큰따옴표 제거), %rs (원문)
//! 접두사: popup-web! (기본 브라우저), popup-chrome!, popup-edge!, popup-firefox! 는 주소를 작은 앱 창으로 엶

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Shell::{ShellExecuteW, URLIS_APPLIABLE, UrlIsW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
    Brave,
    Firefox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    /// 기본 브라우저 종류를 감지해 사용
    Default,
    Browser(Browser),
}

/// 명령의 팝업 접두사 분리
pub fn split_prefix(command: &str) -> (Option<Popup>, &str) {
    const PREFIXES: [(&str, Popup); 4] = [
        ("popup-web!", Popup::Default),
        ("popup-chrome!", Popup::Browser(Browser::Chrome)),
        ("popup-edge!", Popup::Browser(Browser::Edge)),
        ("popup-firefox!", Popup::Browser(Browser::Firefox)),
    ];
    for (prefix, popup) in PREFIXES {
        if let Some(rest) = command.strip_prefix(prefix) {
            return (Some(popup), rest);
        }
    }
    (None, command)
}

/// UTF-8 바이트 기준 URL 인코딩 (영문, 숫자, - _ . ~ 만 그대로 둠)
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 자리 표시자 치환. 한 번에 훑으므로 치환된 텍스트 안의 % 는 다시 해석하지 않음
pub fn fill_placeholders(template: &str, text: &str) -> String {
    let mut out = String::with_capacity(template.len() + text.len() * 3);
    let mut rest = template;
    while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let (value, skip) = if after.starts_with("ds") {
            (Some(url_encode(&url_encode(text))), 2)
        } else if after.starts_with("cs") {
            (Some(text.replace('"', "")), 2)
        } else if after.starts_with("rs") {
            (Some(text.to_string()), 2)
        } else if after.starts_with('s') {
            (Some(url_encode(text)), 1)
        } else {
            (None, 0)
        };
        match value {
            Some(v) => {
                out.push_str(&v);
                rest = &after[skip..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 실행할 최종 명령줄 생성. 팝업 브라우저를 알 수 없으면 주소를 그대로 실행
pub fn build_command(command: &str, text: &str, width: u32, height: u32, detected: Option<Browser>) -> String {
    let (popup, body) = split_prefix(command);
    let filled = fill_placeholders(body, text);
    let browser = match popup {
        None => return filled,
        Some(Popup::Default) => detected,
        Some(Popup::Browser(b)) => Some(b),
    };
    let Some(browser) = browser else { return filled };
    // 인자 주입 방지: 주소에는 큰따옴표가 올 수 없으므로 제거
    let url = filled.replace('"', "");
    let size = (width > 0 && height > 0).then_some((width, height));
    match browser {
        Browser::Firefox => {
            let (w, h) = size.unwrap_or((0, 0));
            if size.is_some() {
                format!("firefox.exe -width {w} -height {h} -new-window \"{url}\"")
            } else {
                format!("firefox.exe -new-window \"{url}\"")
            }
        }
        Browser::Chrome | Browser::Edge | Browser::Brave => {
            let exe = match browser {
                Browser::Chrome => "chrome.exe",
                Browser::Edge => "msedge.exe",
                _ => "brave.exe",
            };
            match size {
                Some((w, h)) => format!("{exe} --app=\"{url}\" --window-size={w},{h}"),
                None => format!("{exe} --app=\"{url}\""),
            }
        }
    }
}

/// 명령줄을 실행 파일과 인자로 분리 (첫 번째 따옴표 밖 공백 기준)
pub fn split_args(command: &str) -> (String, String) {
    let command = command.trim();
    let mut in_quotes = false;
    let mut split = command.len();
    for (i, c) in command.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                split = i;
                break;
            }
            _ => {}
        }
    }
    let file = command[..split].trim();
    let file = file
        .strip_prefix('"')
        .and_then(|f| f.strip_suffix('"'))
        .unwrap_or(file);
    (file.to_string(), command[split..].trim().to_string())
}

/// ProgId 로 브라우저 종류 판별
pub fn browser_from_prog_id(prog_id: &str) -> Option<Browser> {
    let id = prog_id.to_ascii_lowercase();
    match id.as_str() {
        "chromehtml" | "chromebhtml" | "chromedhtml" => Some(Browser::Chrome),
        "msedgehtm" | "msedgebhtml" | "msedgedhtml" => Some(Browser::Edge),
        "bravehtml" | "bravebhtml" | "bravedhtml" => Some(Browser::Brave),
        _ if id == "firefoxurl" || id.starts_with("firefoxurl-") => Some(Browser::Firefox),
        _ => None,
    }
}

/// 기본 브라우저 감지 (http 연결 프로그램)
fn detect_browser() -> Option<Browser> {
    let subkey = w!(r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\http\UserChoice");
    let mut buf = [0u16; 128];
    let mut size = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            w!("ProgId"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    browser_from_prog_id(&String::from_utf16_lossy(&buf[..len]))
}

/// 버튼 명령 실행. 호출 스레드는 COM 이 초기화되어 있어야 함
pub fn run(command: &str, text: &str, width: u32, height: u32) -> Result<(), String> {
    let detected = match split_prefix(command).0 {
        Some(Popup::Default) => detect_browser(),
        _ => None,
    };
    let cmdline = build_command(command, text, width, height, detected);
    execute(&cmdline)
}

fn execute(cmdline: &str) -> Result<(), String> {
    let trimmed = cmdline.trim();
    if trimmed.is_empty() {
        return Err("실행할 명령이 비어 있습니다.".into());
    }
    let is_url = unsafe { UrlIsW(&HSTRING::from(trimmed), URLIS_APPLIABLE) }.as_bool();
    let (file, args) = if is_url { (trimmed.to_string(), String::new()) } else { split_args(trimmed) };
    let args_h = HSTRING::from(args.as_str());
    let params = if args.is_empty() { PCWSTR::null() } else { PCWSTR(args_h.as_ptr()) };
    let ret = unsafe {
        ShellExecuteW(None, PCWSTR::null(), &HSTRING::from(file.as_str()), params, PCWSTR::null(), SW_SHOWNORMAL)
    };
    if ret.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("다음 명령을 실행하지 못했습니다.\n{trimmed}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding() {
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("가"), "%EA%B0%80");
        assert_eq!(url_encode("A-z_0.9~"), "A-z_0.9~");
        assert_eq!(url_encode("&?="), "%26%3F%3D");
    }

    #[test]
    fn placeholders() {
        assert_eq!(fill_placeholders("q=%s", "a b"), "q=a%20b");
        assert_eq!(fill_placeholders("q=%ds", "a b"), "q=a%2520b");
        assert_eq!(fill_placeholders("\"%cs\"", "say \"hi\""), "\"say hi\"");
        assert_eq!(fill_placeholders("%rs!", "a&b"), "a&b!");
        assert_eq!(fill_placeholders("100% %x %s", "t"), "100% %x t");
        assert_eq!(fill_placeholders("end%", "t"), "end%");
        // 치환된 텍스트 안의 자리 표시자는 다시 치환하지 않음
        assert_eq!(fill_placeholders("%rs %s", "%s"), "%s %25s");
    }

    #[test]
    fn prefixes() {
        assert_eq!(split_prefix("popup-web!https://x"), (Some(Popup::Default), "https://x"));
        assert_eq!(split_prefix("popup-edge!u"), (Some(Popup::Browser(Browser::Edge)), "u"));
        assert_eq!(split_prefix("https://x"), (None, "https://x"));
    }

    #[test]
    fn commands() {
        assert_eq!(
            build_command("https://www.google.com/search?q=%s", "한 글", 0, 0, None),
            "https://www.google.com/search?q=%ED%95%9C%20%EA%B8%80"
        );
        assert_eq!(
            build_command("popup-chrome!https://x/?q=%rs", "a\"b", 0, 0, None),
            "chrome.exe --app=\"https://x/?q=ab\""
        );
        assert_eq!(
            build_command("popup-web!https://x/%s", "t", 500, 400, Some(Browser::Edge)),
            "msedge.exe --app=\"https://x/t\" --window-size=500,400"
        );
        assert_eq!(
            build_command("popup-firefox!https://x", "t", 500, 400, None),
            "firefox.exe -width 500 -height 400 -new-window \"https://x\""
        );
        // 기본 브라우저를 알 수 없으면 주소 그대로
        assert_eq!(build_command("popup-web!https://x/%s", "t", 0, 0, None), "https://x/t");
    }

    #[test]
    fn args_split() {
        assert_eq!(split_args("notepad.exe a.txt"), ("notepad.exe".into(), "a.txt".into()));
        assert_eq!(
            split_args(r#""C:\Program Files\x.exe" --a "b c""#),
            (r"C:\Program Files\x.exe".into(), r#"--a "b c""#.into())
        );
        assert_eq!(split_args("  calc  "), ("calc".into(), "".into()));
    }

    #[test]
    fn prog_ids() {
        assert_eq!(browser_from_prog_id("ChromeHTML"), Some(Browser::Chrome));
        assert_eq!(browser_from_prog_id("MSEdgeHTM"), Some(Browser::Edge));
        assert_eq!(browser_from_prog_id("BraveHTML"), Some(Browser::Brave));
        assert_eq!(browser_from_prog_id("FirefoxURL-308046B0AF4A39CB"), Some(Browser::Firefox));
        assert_eq!(browser_from_prog_id("IE.HTTP"), None);
    }
}
