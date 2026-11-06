//! GitHub Releases 기반 새 버전 확인, 설치 파일 다운로드, 설치 실행

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::config::{APP_NAME, APP_VERSION};

pub const REPOSITORY: &str = "workingdad365/gettext";
/// 릴리스 자산 중 설치 파일로 인식하는 이름 규칙: GetText-Setup-*.exe
const INSTALLER_PREFIX: &str = "GetText-Setup-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: (u32, u32, u32),
    pub tag: String,
    pub download_url: String,
    pub file_name: String,
    pub size: u64,
    pub notes: String,
}

/// 스레드 간 공유하는 업데이트 진행 상태
#[derive(Debug, Default)]
pub struct UpdateState {
    /// 설치 가능한 새 버전 (건너뛴 버전은 수동 확인 때만 채워짐)
    pub available: Option<UpdateInfo>,
    /// 확인 작업 스레드가 실행 중인지 여부
    pub checking: bool,
    /// 확인 작업 스레드가 남긴 결과 (서비스 스레드가 가져감)
    pub check_result: Option<Result<Option<UpdateInfo>, String>>,
    /// 이번 실행 중 자동 확인으로 이미 알렸는지 여부
    pub notified: bool,
    /// 다운로드 진행률 (다운로드 중이 아니면 None)
    pub progress: Option<f32>,
    /// 마지막 다운로드, 설치 오류
    pub error: Option<String>,
}

impl UpdateInfo {
    pub fn version_text(&self) -> String {
        let (a, b, c) = self.version;
        format!("{a}.{b}.{c}")
    }
}

/// 태그 문자열(v1.2.3 또는 1.2.3)을 버전으로 변환. 빠진 자리는 0, 형식이 다르면 None
pub fn parse_version(tag: &str) -> Option<(u32, u32, u32)> {
    let text = tag.trim();
    let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn current_version() -> (u32, u32, u32) {
    parse_version(APP_VERSION).unwrap_or((0, 0, 0))
}

/// 릴리스 JSON에서 버전과 설치 파일 정보 추출. 초안, 사전 릴리스, 설치 파일이 없는 릴리스는 None
pub fn parse_release(json: &str) -> Option<UpdateInfo> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let is_true = |name: &str| root.get(name).and_then(|v| v.as_bool()).unwrap_or(false);
    if is_true("draft") || is_true("prerelease") {
        return None;
    }
    let tag = root.get("tag_name")?.as_str()?.to_string();
    let version = parse_version(&tag)?;
    let notes = root.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();

    root.get("assets")?.as_array()?.iter().find_map(|asset| {
        let name = asset.get("name")?.as_str()?;
        let lower = name.to_ascii_lowercase();
        if !lower.starts_with(&INSTALLER_PREFIX.to_ascii_lowercase()) || !lower.ends_with(".exe") {
            return None;
        }
        Some(UpdateInfo {
            version,
            tag: tag.clone(),
            download_url: asset.get("browser_download_url")?.as_str()?.to_string(),
            file_name: name.to_string(),
            size: asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            notes: notes.clone(),
        })
    })
}

/// 최신 릴리스 조회. 현재보다 새 버전이면 정보 반환
pub fn check() -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let mut body = Vec::new();
    http_get(&url, |chunk, _| {
        body.extend_from_slice(chunk);
        Ok(())
    })?;
    let json = String::from_utf8_lossy(&body);
    Ok(parse_release(&json).filter(|info| info.version > current_version()))
}

/// 설치 파일을 임시 폴더에 다운로드하고 경로 반환. progress 는 0.0 - 1.0
pub fn download(info: &UpdateInfo, mut progress: impl FnMut(f32)) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(APP_NAME).join("update");
    // 이전 업데이트에서 받은 설치 파일 정리
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("임시 폴더 생성 실패: {e}"))?;
    // 자산 이름은 외부 입력이므로 경로 구분자가 섞여 있으면 거부
    if info.file_name.contains(['\\', '/', ':']) {
        return Err("설치 파일 이름이 올바르지 않습니다.".into());
    }
    let path = dir.join(&info.file_name);
    let mut file = fs::File::create(&path).map_err(|e| format!("파일 생성 실패: {e}"))?;

    let mut done = 0u64;
    http_get(&info.download_url, |chunk, total| {
        file.write_all(chunk).map_err(|e| format!("파일 쓰기 실패: {e}"))?;
        done += chunk.len() as u64;
        let total = total.unwrap_or(info.size);
        if total > 0 {
            progress((done as f32 / total as f32).min(1.0));
        }
        Ok(())
    })?;
    drop(file);

    if info.size > 0 && done != info.size {
        let _ = fs::remove_file(&path);
        return Err(format!("다운로드 크기가 다릅니다 ({done} / {} 바이트).", info.size));
    }
    Ok(path)
}

/// 설치 파일을 조용히 실행. 설치기가 실행 중인 앱을 닫고 설치 후 /RELAUNCH=1 로 다시 실행함
pub fn run_installer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH=1"])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("설치 파일 실행 실패: {e}"))
}

struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn checked(h: *mut core::ffi::c_void, what: &str) -> Result<Handle, String> {
    if h.is_null() {
        Err(format!("{what} 실패: {}", windows::core::Error::from_thread().message()))
    } else {
        Ok(Handle(h))
    }
}

/// HTTPS GET. 리다이렉트는 WinHTTP 가 따라가며, 응답 본문을 조각 단위로 sink 에 전달 (두 번째 인자는 전체 크기)
fn http_get(
    url: &str,
    mut sink: impl FnMut(&[u8], Option<u64>) -> Result<(), String>,
) -> Result<(), String> {
    let rest = url.strip_prefix("https://").ok_or("https 주소만 지원합니다.")?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let path = if path.is_empty() { "/" } else { path };

    unsafe {
        // GitHub API 는 User-Agent 헤더가 없으면 403 을 돌려줌
        let agent = HSTRING::from(format!("{APP_NAME}/{APP_VERSION}"));
        let session = checked(
            WinHttpOpen(&agent, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0),
            "WinHttpOpen",
        )?;
        WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000).map_err(|e| e.message())?;
        let connect = checked(
            WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0),
            "WinHttpConnect",
        )?;
        let request = checked(
            WinHttpOpenRequest(
                connect.0,
                w!("GET"),
                &HSTRING::from(path),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ),
            "WinHttpOpenRequest",
        )?;
        let headers: Vec<u16> = "Accept: application/vnd.github+json, application/octet-stream\r\n"
            .encode_utf16()
            .collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)
            .map_err(|e| format!("요청 전송 실패: {}", e.message()))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|e| format!("응답 수신 실패: {}", e.message()))?;

        let status = query_number(request.0, WINHTTP_QUERY_STATUS_CODE).unwrap_or(0);
        if status != 200 {
            return Err(format!("서버 응답 오류 (HTTP {status})"));
        }
        let total = query_content_length(request.0);

        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buf.as_mut_ptr().cast(), buf.len() as u32, &mut read)
                .map_err(|e| format!("응답 읽기 실패: {}", e.message()))?;
            if read == 0 {
                break;
            }
            sink(&buf[..read as usize], total)?;
        }
    }
    Ok(())
}

unsafe fn query_number(request: *mut core::ffi::c_void, level: u32) -> Option<u32> {
    let mut value = 0u32;
    let mut len = size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request,
            level | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut value as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
        .ok()?;
    }
    Some(value)
}

/// Content-Length 는 4GB 를 넘을 수 있어 문자열로 읽어 파싱
unsafe fn query_content_length(request: *mut core::ffi::c_void) -> Option<u64> {
    use windows::Win32::Networking::WinHttp::WINHTTP_QUERY_CONTENT_LENGTH;
    let mut buf = [0u16; 32];
    let mut len = (buf.len() * 2) as u32;
    unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_CONTENT_LENGTH,
            PCWSTR::null(),
            Some(buf.as_mut_ptr().cast()),
            &mut len,
            std::ptr::null_mut(),
        )
        .ok()?;
    }
    String::from_utf16_lossy(&buf[..len as usize / 2]).trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("V2.0"), Some((2, 0, 0)));
        assert_eq!(parse_version("3"), Some((3, 0, 0)));
        assert_eq!(parse_version(" v1.10.0 "), Some((1, 10, 0)));
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("v1.2-beta"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn version_ordering() {
        assert!(parse_version("1.0.10") > parse_version("1.0.9"));
        assert!(parse_version("1.1.0") > parse_version("1.0.99"));
        assert!(parse_version("2.0.0") > parse_version("1.9.9"));
        assert_eq!(current_version(), parse_version(APP_VERSION).unwrap());
    }

    fn release(tag: &str, draft: bool, prerelease: bool, asset: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{prerelease},"body":"notes",
            "assets":[{{"name":"other.zip","browser_download_url":"https://x/other.zip","size":1}},
                      {{"name":"{asset}","browser_download_url":"https://x/{asset}","size":1234}}]}}"#
        )
    }

    #[test]
    fn release_parsing() {
        let info = parse_release(&release("v1.0.1", false, false, "GetText-Setup-1.0.1.exe")).unwrap();
        assert_eq!(info.version, (1, 0, 1));
        assert_eq!(info.tag, "v1.0.1");
        assert_eq!(info.version_text(), "1.0.1");
        assert_eq!(info.file_name, "GetText-Setup-1.0.1.exe");
        assert_eq!(info.download_url, "https://x/GetText-Setup-1.0.1.exe");
        assert_eq!(info.size, 1234);
        assert_eq!(info.notes, "notes");
    }

    #[test]
    fn release_rejected() {
        assert!(parse_release(&release("v1.0.1", true, false, "GetText-Setup-1.0.1.exe")).is_none());
        assert!(parse_release(&release("v1.0.1", false, true, "GetText-Setup-1.0.1.exe")).is_none());
        assert!(parse_release(&release("v1.0.1", false, false, "GetText-1.0.1.zip")).is_none());
        assert!(parse_release(&release("latest", false, false, "GetText-Setup-1.0.1.exe")).is_none());
        assert!(parse_release("not json").is_none());
        assert!(parse_release("[]").is_none());
    }

    /// 실제 GitHub 릴리스 조회와 설치 파일 다운로드 (네트워크 필요: cargo test -- --ignored)
    #[test]
    #[ignore]
    fn live_release_download() {
        let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
        let mut body = Vec::new();
        http_get(&url, |c, _| {
            body.extend_from_slice(c);
            Ok(())
        })
        .unwrap();
        let info = parse_release(&String::from_utf8_lossy(&body)).expect("최신 릴리스에 설치 파일 없음");
        assert!(info.size > 0);

        let mut last = 0.0;
        let path = download(&info, |p| last = p).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), info.size);
        assert_eq!(last, 1.0);
        // 다시 받으면 이전 파일을 정리한 뒤 같은 경로에 저장
        let again = download(&info, |_| {}).unwrap();
        assert_eq!(again, path);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        fs::remove_file(path).unwrap();

        assert!(check().is_ok());
    }
}
