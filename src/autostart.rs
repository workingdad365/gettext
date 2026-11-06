//! Windows 시작 시 자동 실행 (HKCU Run 키)

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::HSTRING;

use crate::config::APP_NAME;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 자동 실행으로 시작되었음을 알리는 인자 (최초 실행 설정 창 생략용)
pub const AUTOSTART_ARG: &str = "--autostart";

fn win_err(e: WIN32_ERROR) -> String {
    windows::core::Error::from_hresult(e.to_hresult()).message()
}

fn read_value(subkey: &str, name: &str) -> Option<String> {
    let subkey = HSTRING::from(subkey);
    let name = HSTRING::from(name);
    unsafe {
        let mut size = 0u32;
        let rc = RegGetValueW(HKEY_CURRENT_USER, &subkey, &name, RRF_RT_REG_SZ, None, None, Some(&mut size));
        if rc != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        let rc = RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        );
        if rc != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

fn write_value(subkey: &str, name: &str, data: &str) -> Result<(), String> {
    let wide: Vec<u16> = data.encode_utf16().chain(std::iter::once(0)).collect();
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(name),
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
        )
    };
    if rc == ERROR_SUCCESS { Ok(()) } else { Err(win_err(rc)) }
}

fn delete_value(subkey: &str, name: &str) -> Result<(), String> {
    let rc = unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(subkey), &HSTRING::from(name))
    };
    if rc == ERROR_SUCCESS || rc == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(win_err(rc)) }
}

pub fn build_command(exe: &str) -> String {
    format!("\"{exe}\" {AUTOSTART_ARG}")
}

fn current_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(build_command(&exe.to_string_lossy()))
}

pub fn is_enabled() -> bool {
    read_value(RUN_KEY, APP_NAME).is_some()
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let cmd = current_command().ok_or("실행 파일 경로를 확인할 수 없습니다.")?;
        write_value(RUN_KEY, APP_NAME, &cmd)
    } else {
        delete_value(RUN_KEY, APP_NAME)
    }
}

/// 실행 파일이 이동된 경우 등록된 경로를 현재 경로로 갱신
pub fn refresh_path() {
    if let (Some(registered), Some(cmd)) = (read_value(RUN_KEY, APP_NAME), current_command())
        && !registered.eq_ignore_ascii_case(&cmd)
    {
        let _ = write_value(RUN_KEY, APP_NAME, &cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::RegDeleteTreeW;

    const TEST_KEY: &str = r"Software\GetText.UnitTest";

    #[test]
    fn command_quotes_path() {
        assert_eq!(
            build_command(r"C:\Program Files\GetText\GetText.exe"),
            r#""C:\Program Files\GetText\GetText.exe" --autostart"#
        );
    }

    #[test]
    fn registry_roundtrip() {
        let name = "value";
        assert_eq!(read_value(TEST_KEY, name), None);
        write_value(TEST_KEY, name, "\"C:\\경로 테스트\\a.exe\" --autostart").unwrap();
        assert_eq!(
            read_value(TEST_KEY, name).as_deref(),
            Some("\"C:\\경로 테스트\\a.exe\" --autostart")
        );
        delete_value(TEST_KEY, name).unwrap();
        assert_eq!(read_value(TEST_KEY, name), None);
        // 없는 값 삭제는 성공으로 처리
        delete_value(TEST_KEY, name).unwrap();
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(TEST_KEY));
        }
    }
}
