use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "GetText";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 마우스 단축키에 쓰는 버튼
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    /// 마우스 단축키 사용 안 함
    None,
    #[default]
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub const ALL: [MouseButton; 4] =
        [MouseButton::None, MouseButton::Left, MouseButton::Right, MouseButton::Middle];

    pub fn label(self) -> &'static str {
        match self {
            MouseButton::None => "사용 안 함",
            MouseButton::Left => "왼쪽 클릭",
            MouseButton::Right => "오른쪽 클릭",
            MouseButton::Middle => "가운데 클릭",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseHotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub button: MouseButton,
}

impl Default for MouseHotkey {
    fn default() -> Self {
        Self { ctrl: true, alt: false, shift: true, button: MouseButton::Left }
    }
}

impl MouseHotkey {
    pub fn enabled(&self) -> bool {
        self.button != MouseButton::None
    }

    pub fn display(&self) -> String {
        if !self.enabled() {
            return "(사용 안 함)".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(self.button.label());
        parts.join("+")
    }

    pub fn validate(&self) -> Result<(), String> {
        // 수식키 없는 왼쪽 클릭은 모든 클릭을 가로채므로 허용하지 않음
        if self.button == MouseButton::Left && !(self.ctrl || self.alt || self.shift) {
            return Err("왼쪽 클릭에는 Ctrl, Alt, Shift 중 하나 이상을 조합해야 합니다.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyHotkey {
    pub win: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// 트리거 키 이름 (A-Z, 0-9, F1-F12). 비어 있으면 사용 안 함
    pub key: String,
}

impl KeyHotkey {
    pub fn enabled(&self) -> bool {
        !self.key.is_empty()
    }

    pub fn display(&self) -> String {
        if !self.enabled() {
            return "(사용 안 함)".to_string();
        }
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.win {
            parts.push("Win");
        }
        parts.push(&self.key);
        parts.join("+")
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled() {
            return Ok(());
        }
        if key_to_vk(&self.key).is_none() {
            return Err("키보드 단축키의 키가 올바르지 않습니다.".to_string());
        }
        if !is_function_key(&self.key) && !(self.win || self.ctrl || self.alt) {
            return Err("문자/숫자 키에는 Win, Ctrl, Alt 중 하나 이상을 조합해야 합니다.".to_string());
        }
        Ok(())
    }
}

/// 화면 요소에서 텍스트를 가져오는 방식
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextMethod {
    /// UIA 와 MSAA 결과 중 더 안쪽 요소를 선택
    #[default]
    Auto,
    Msaa,
    Uia,
}

impl TextMethod {
    pub const ALL: [TextMethod; 3] = [TextMethod::Auto, TextMethod::Msaa, TextMethod::Uia];

    pub fn label(self) -> &'static str {
        match self {
            TextMethod::Auto => "자동 (권장)",
            TextMethod::Msaa => "MSAA",
            TextMethod::Uia => "UI Automation",
        }
    }
}

/// 팝업 창 아래에 표시되는 명령 버튼
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebButton {
    /// 버튼에 표시될 이름
    pub name: String,
    /// 실행할 명령 또는 주소 (%s 등 자리 표시자 사용)
    pub command: String,
    /// Alt 와 함께 누르는 단축 글자 (비어 있으면 순번)
    pub key: String,
    /// 팝업 브라우저 창 크기 (0 이면 브라우저 기본값)
    pub width: u32,
    pub height: u32,
}

impl WebButton {
    pub fn google() -> Self {
        Self {
            name: "구글 검색".into(),
            command: "https://www.google.com/search?q=%s".into(),
            key: "G".into(),
            width: 0,
            height: 0,
        }
    }

    /// 단축 글자 (영문, 숫자 한 글자만 유효)
    pub fn accel(&self) -> Option<char> {
        let mut chars = self.key.trim().chars();
        let c = chars.next()?.to_ascii_uppercase();
        (chars.next().is_none() && c.is_ascii_alphanumeric()).then_some(c)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.command.trim().is_empty() {
            return Err("버튼 명령을 입력하세요.".to_string());
        }
        if !self.key.trim().is_empty() && self.accel().is_none() {
            return Err("단축 글자는 영문이나 숫자 한 글자여야 합니다.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mouse: MouseHotkey,
    pub keyboard: KeyHotkey,
    pub text_method: TextMethod,
    /// 팝업을 열 때와 선택 영역이 바뀔 때 자동으로 클립보드에 복사
    pub auto_copy_selection: bool,
    /// 특수 공백은 일반 공백으로, 보이지 않는 문자는 제거
    pub unicode_spaces_to_ascii: bool,
    /// 팝업 창 크기 조절 테두리 숨김
    pub non_resizable: bool,
    /// 팝업 글꼴 이름 (비어 있으면 시스템 글꼴)
    pub font_name: String,
    /// 팝업 글꼴 크기 (pt, 0 이면 시스템 기본값)
    pub font_size: u32,
    pub web_buttons: Vec<WebButton>,
    /// 한 줄에 놓을 버튼 수 (0 이면 한 줄)
    pub buttons_per_row: u32,
    /// 마우스 단축키를 무시할 프로그램 실행 파일 이름 (예: mstsc.exe)
    pub excluded_programs: Vec<String>,
    /// 시작 직후와 하루 한 번 새 버전 자동 확인
    pub check_updates: bool,
    /// 다시 알리지 않을 버전 태그 (예: v1.0.1)
    pub skipped_version: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mouse: MouseHotkey::default(),
            keyboard: KeyHotkey::default(),
            text_method: TextMethod::Auto,
            auto_copy_selection: false,
            unicode_spaces_to_ascii: false,
            non_resizable: false,
            font_name: String::new(),
            font_size: 0,
            web_buttons: vec![WebButton::google()],
            buttons_per_row: 8,
            excluded_programs: Vec::new(),
            check_updates: true,
            skipped_version: None,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        self.mouse.validate()?;
        self.keyboard.validate()?;
        for (i, b) in self.web_buttons.iter().enumerate() {
            b.validate().map_err(|e| format!("{}번 버튼: {e}", i + 1))?;
        }
        if self.font_size > 72 {
            return Err("글꼴 크기는 72 이하여야 합니다.".to_string());
        }
        Ok(())
    }

    /// 제외 프로그램 이름 정리 (공백 제거, 빈 항목 제거, 대소문자 무시 중복 제거)
    pub fn normalized_excluded(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in &self.excluded_programs {
            let p = p.trim();
            if !p.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(p)) {
                out.push(p.to_string());
            }
        }
        out
    }

    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(APP_NAME).join("config.json")
    }

    pub fn load() -> Config {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        // 임시 파일에 쓴 뒤 교체하여 저장 중 손상 방지
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &path)
    }
}

/// 선택 가능한 키보드 트리거 키 목록
pub fn available_keys() -> Vec<String> {
    let mut keys: Vec<String> = ('A'..='Z').map(|c| c.to_string()).collect();
    keys.extend(('0'..='9').map(|c| c.to_string()));
    keys.extend((1..=12).map(|n| format!("F{n}")));
    keys
}

fn is_function_key(key: &str) -> bool {
    key.len() >= 2
        && key.starts_with(['F', 'f'])
        && key[1..].parse::<u32>().is_ok_and(|n| (1..=12).contains(&n))
}

/// 키 이름을 Windows 가상 키 코드로 변환
pub fn key_to_vk(key: &str) -> Option<u32> {
    if is_function_key(key) {
        let n: u32 = key[1..].parse().ok()?;
        return Some(0x70 + n - 1); // VK_F1 = 0x70
    }
    let mut chars = key.chars();
    let c = chars.next()?.to_ascii_uppercase();
    if chars.next().is_some() {
        return None;
    }
    match c {
        'A'..='Z' | '0'..='9' => Some(c as u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kh(win: bool, ctrl: bool, alt: bool, shift: bool, key: &str) -> KeyHotkey {
        KeyHotkey { win, ctrl, alt, shift, key: key.to_string() }
    }

    #[test]
    fn vk_mapping() {
        assert_eq!(key_to_vk("A"), Some(0x41));
        assert_eq!(key_to_vk("z"), Some(0x5A));
        assert_eq!(key_to_vk("0"), Some(0x30));
        assert_eq!(key_to_vk("F1"), Some(0x70));
        assert_eq!(key_to_vk("F12"), Some(0x7B));
        assert_eq!(key_to_vk("F13"), None);
        assert_eq!(key_to_vk(""), None);
        assert_eq!(key_to_vk("AB"), None);
        for k in available_keys() {
            assert!(key_to_vk(&k).is_some(), "{k}");
        }
    }

    #[test]
    fn default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.mouse, MouseHotkey { ctrl: true, alt: false, shift: true, button: MouseButton::Left });
        assert_eq!(cfg.mouse.display(), "Ctrl+Shift+왼쪽 클릭");
        assert!(!cfg.keyboard.enabled());
        assert_eq!(cfg.web_buttons, vec![WebButton::google()]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn mouse_validation() {
        let mut m = MouseHotkey { ctrl: false, alt: false, shift: false, button: MouseButton::Left };
        assert!(m.validate().is_err());
        m.alt = true;
        assert!(m.validate().is_ok());
        let middle = MouseHotkey { ctrl: false, alt: false, shift: false, button: MouseButton::Middle };
        assert!(middle.validate().is_ok());
        let none = MouseHotkey { button: MouseButton::None, ..middle };
        assert!(none.validate().is_ok());
        assert_eq!(none.display(), "(사용 안 함)");
    }

    #[test]
    fn keyboard_validation() {
        assert!(kh(false, false, false, false, "").validate().is_ok());
        assert!(kh(false, false, false, false, "A").validate().is_err());
        assert!(kh(false, false, false, true, "A").validate().is_err());
        assert!(kh(false, true, false, false, "A").validate().is_ok());
        assert!(kh(false, false, false, false, "F8").validate().is_ok());
        assert_eq!(kh(true, true, true, true, "Q").display(), "Ctrl+Alt+Shift+Win+Q");
    }

    #[test]
    fn web_button_validation() {
        let mut b = WebButton::google();
        assert_eq!(b.accel(), Some('G'));
        b.key = "g".into();
        assert_eq!(b.accel(), Some('G'));
        b.key = "".into();
        assert_eq!(b.accel(), None);
        assert!(b.validate().is_ok());
        b.key = "가".into();
        assert!(b.validate().is_err());
        b.key = "AB".into();
        assert!(b.validate().is_err());
        b.key = "1".into();
        b.command = "  ".into();
        assert!(b.validate().is_err());

        let mut cfg = Config::default();
        cfg.web_buttons.push(WebButton::default());
        assert_eq!(cfg.validate().unwrap_err(), "2번 버튼: 버튼 명령을 입력하세요.");
    }

    #[test]
    fn excluded_normalization() {
        let cfg = Config {
            excluded_programs: vec![" mstsc.exe ".into(), "".into(), "MSTSC.EXE".into(), "vmware.exe".into()],
            ..Default::default()
        };
        assert_eq!(cfg.normalized_excluded(), vec!["mstsc.exe", "vmware.exe"]);
    }

    #[test]
    fn json_roundtrip_and_missing_fields() {
        let mut cfg = Config::default();
        cfg.keyboard = kh(false, true, false, true, "T");
        cfg.text_method = TextMethod::Uia;
        let json = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);

        let partial: Config = serde_json::from_str(r#"{"mouse":{"button":"middle"},"text_method":"msaa"}"#).unwrap();
        assert_eq!(partial.mouse.button, MouseButton::Middle);
        // 빠진 수식키는 기본값 사용
        assert!(partial.mouse.ctrl && partial.mouse.shift);
        assert_eq!(partial.text_method, TextMethod::Msaa);
        assert!(partial.check_updates);
        assert_eq!(partial.web_buttons.len(), 1);

        let none: Config = serde_json::from_str(r#"{"mouse":{"button":"none"},"web_buttons":[]}"#).unwrap();
        assert!(!none.mouse.enabled());
        assert!(none.web_buttons.is_empty());
    }
}
