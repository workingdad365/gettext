<p align="center">
  <img src="assets/gettext.png" width="96" alt="GetText">
</p>

<h1 align="center">GetText</h1>

<p align="center">화면의 어떤 텍스트든 클릭 한 번으로 꺼내 복사하는 Windows 트레이 프로그램<br>v1.0.2</p>

---

메시지 상자, 버튼, 메뉴, 툴팁, 작업 표시줄처럼 일반적으로는 선택하거나 복사할 수 없는 텍스트를
단축키 클릭 한 번으로 편집 창에 띄워, 필요한 부분만 골라 복사할 수 있다.



## 주요 기능

- **트레이 상주**: 실행하면 작업표시줄 알림 영역에 아이콘이 나타나고 백그라운드에서 동작함
- **마우스 단축키**: 기본값 `Ctrl+Shift+왼쪽 클릭`. 수식키(`Ctrl`, `Alt`, `Shift`)와 왼쪽/오른쪽/가운데 버튼 조합으로 변경 가능
- **키보드 단축키**: 선택 사항. 마우스 커서 아래의 텍스트를 가져옴
- **텍스트 추출**: UI Automation 과 MSAA 를 함께 사용해 더 구체적인 요소의 텍스트를 선택 (방식 고정 가능)
- **팝업 편집 창**
  - 요소 바로 위에 같은 크기로 표시되며 텍스트가 전체 선택된 상태로 열림
  - `Tab` 키로 이름, 값, 설명을 차례로 선택, `Esc` 로 닫기
  - 다른 곳을 클릭하면 닫히고, 내용을 고치거나 창을 옮기면 제목 표시줄이 붙은 고정 창으로 바뀜
  - 팝업 위에서 단축키를 다시 누르면 닫힘
- **명령 버튼**: 팝업 아래 버튼으로 선택한 텍스트를 검색, 번역하거나 프로그램에 넘김 (기본값: 구글 검색)
- **자동 복사**: 팝업을 열거나 텍스트를 선택하면 바로 클립보드에 복사 (기본 켜짐, 설정에서 끌 수 있음)
- **특수 공백 정리**: 줄바꿈 없는 공백, 폭 없는 공백 같은 문자를 일반 공백으로 바꾸거나 제거 (선택 사항)
- **제외 프로그램**: 원격 데스크톱, 게임처럼 단축키 클릭을 가로채면 안 되는 프로그램 지정
- **Windows 시작 시 자동 실행**, **자동 업데이트**, **중복 실행 방지**

## 설치

[Releases](https://github.com/workingdad365/gettext/releases) 에서 `GetText-Setup-<버전>.exe` 를 내려받아 실행한다.

- 관리자 권한 없이 사용자별로 설치됨 (`%LocalAppData%\Programs\GetText`)
- 설치 중 바탕 화면 바로 가기, Windows 로그인 시 자동 실행 여부를 선택할 수 있음
- 실행 중인 GetText 는 설치 전에 자동으로 종료됨
- **설정 > 앱** 에서 제거할 수 있으며, 제거해도 설정 파일은 남음

### 업데이트

- 시작 10초 뒤와 이후 하루 한 번 GitHub 최신 릴리스를 확인함
- 새 버전이 있으면 설정 창이 열리고 상단에 안내가 표시됨 (자동 확인으로는 실행 중 한 번만 알림)
  - **지금 설치**: 설치 파일을 내려받아 설치한 뒤 프로그램을 다시 실행함
  - **이 버전 건너뛰기**: 해당 버전은 자동 확인에서 더 이상 알리지 않음
- 트레이 메뉴의 **업데이트 확인** 으로 언제든 직접 확인할 수 있음 (건너뛴 버전도 표시됨)
- 설정 창 하단의 **새 버전 자동 확인** 을 끄면 자동 확인을 하지 않음

## 사용 방법

### 실행과 종료

| 동작 | 방법 |
|---|---|
| 텍스트 가져오기 | 원하는 텍스트 위에서 `Ctrl+Shift+왼쪽 클릭` (또는 설정한 단축키) |
| 설정 창 열기 | 트레이 아이콘 왼쪽 클릭, 또는 우클릭 메뉴의 **설정 열기** |
| 설정 창 닫기 | 창의 X 버튼 (프로그램은 트레이에서 계속 동작함) |
| 프로그램 종료 | 트레이 아이콘 우클릭 메뉴의 **종료** |

처음 실행할 때는 설정 창이 자동으로 열린다.

> 💡 Windows 11 은 새 트레이 아이콘을 `^` 숨김 영역에 넣는다. 아이콘을 작업표시줄로 끌어다 놓거나
> **설정 > 개인 설정 > 작업 표시줄 > 기타 시스템 트레이 아이콘** 에서 GetText 를 켜면 항상 보인다.

### 팝업 창 조작

| 키 / 동작 | 결과 |
|---|---|
| `Tab` | 전체 -> 이름 -> 값 -> 설명 -> 전체 순서로 선택 범위 이동 |
| `Ctrl+A` | 전체 선택 |
| `Ctrl+C` | 선택한 부분 복사 |
| `Alt+단축 글자` | 해당 명령 버튼 실행 (예: `Alt+G` 구글 검색) |
| `Esc`, 다른 곳 클릭 | 닫기 (고정 창은 X 버튼이나 `Esc` 로 닫음) |
| 내용 편집, 창 이동, 크기 조절 | 고정 창으로 전환 |

### 명령 버튼

설정 창의 **명령 버튼** 에서 버튼을 추가, 편집, 정렬한다. 버튼을 누르면 팝업에서 선택한 텍스트
(선택하지 않았으면 전체)를 명령에 넣어 실행한다.

| 자리 표시자 | 치환 내용 |
|---|---|
| `%s` | URL 인코딩한 텍스트 |
| `%ds` | 이중 URL 인코딩한 텍스트 |
| `%cs` | 큰따옴표를 뺀 원문 |
| `%rs` | 원문 그대로 |

| 접두사 | 동작 |
|---|---|
| (없음) | 주소면 기본 브라우저로 열고, 아니면 명령줄로 실행 |
| `popup-web!` | 기본 브라우저(Chrome, Edge, Brave, Firefox)의 작은 앱 창으로 열기 |
| `popup-chrome!`, `popup-edge!`, `popup-firefox!` | 지정한 브라우저의 앱 창으로 열기 |

예시는 다음과 같다.

```text
https://www.google.com/search?q=%s
popup-web!https://papago.naver.com/?sk=auto&tk=ko&st=%s
notepad.exe
```

창 크기(너비 x 높이)는 `popup-` 접두사로 여는 브라우저 창에만 적용된다.

## 설정 파일

설정은 `%APPDATA%\GetText\config.json` 에 저장된다. 모든 항목은 설정 창에서 바꿀 수 있다.

```json
{
  "mouse": { "ctrl": true, "alt": false, "shift": true, "button": "left" },
  "keyboard": { "win": false, "ctrl": false, "alt": false, "shift": false, "key": "" },
  "text_method": "auto",
  "auto_copy_selection": false,
  "unicode_spaces_to_ascii": false,
  "non_resizable": false,
  "font_name": "",
  "font_size": 0,
  "web_buttons": [
    { "name": "구글 검색", "command": "https://www.google.com/search?q=%s", "key": "G", "width": 0, "height": 0 }
  ],
  "buttons_per_row": 8,
  "excluded_programs": [],
  "check_updates": true,
  "skipped_version": null
}
```

| 필드 | 설명 |
|---|---|
| `mouse.button` | `left`, `right`, `middle`, `none` (사용 안 함) |
| `keyboard.key` | `A-Z`, `0-9`, `F1-F12`. 비어 있으면 사용 안 함 |
| `text_method` | `auto` (권장), `msaa`, `uia` |
| `font_size` | 팝업 글꼴 크기(pt). `0` 이면 시스템 기본 |
| `buttons_per_row` | 한 줄에 놓을 버튼 수. `0` 이면 모두 한 줄 |
| `excluded_programs` | 마우스 단축키를 무시할 실행 파일 이름 (예: `mstsc.exe`) |
| `skipped_version` | 자동 확인에서 알리지 않을 버전 태그 (예: `"v1.0.1"`) |

자동 실행은 레지스트리 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 의 `GetText` 값으로 관리한다.

## 제한 사항

- 관리자 권한으로 실행된 프로그램의 텍스트는 가져오지 못함 (Windows UIPI 정책). 필요하면 GetText 도 관리자 권한으로 실행해야 함
- 접근성 정보를 제공하지 않는 프로그램(일부 게임, 직접 그리는 UI)에서는 텍스트를 가져올 수 없음
- 마우스 단축키 클릭은 다른 프로그램에 전달되지 않음. 단축키와 겹치는 기능이 필요한 프로그램은 제외 목록에 추가해야 함
- 설정 창을 한 번 연 뒤에는 그래픽 라이브러리(wgpu)가 메모리에 남아 사용량이 늘어남

## 빌드

### 요구 사항

- Windows 10/11
- Rust 1.88 이상 (MSVC 툴체인, edition 2024)
- Visual Studio Build Tools 와 Windows SDK (exe 아이콘, 매니페스트 리소스 컴파일에 `rc.exe` 필요)
- 설치 파일 생성 시: [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`)
- 릴리스 게시 시: [GitHub CLI](https://cli.github.com) (`gh auth login` 완료 상태)

### 명령

```powershell
cargo build --release   # target\release\GetText.exe 생성
cargo test              # 유닛 테스트
```

### 설치 파일 생성과 릴리스

```powershell
.\build-installer.ps1                    # 릴리스 빌드 후 dist\GetText-Setup-<버전>.exe 생성
.\build-installer.ps1 -Install           # 생성 직후 조용히 설치 (검증용)

.\release.ps1 "릴리스 설명"              # 버전 자동 증가 (1.0.0 -> 1.0.1) 후 GitHub Release 게시
.\release.ps1 "릴리스 설명" -Bump minor  # 1.0.1 -> 1.1.0 (major 는 2.0.0)
.\release.ps1 "릴리스 설명" -NoBump      # 현재 버전 그대로 게시 (첫 릴리스, 게시 실패 후 재시도용)
.\release.ps1 "릴리스 설명" -DryRun      # 다음 버전만 출력
```

`release.ps1` 은 다음 순서로 동작한다. 커밋 전 단계에서 실패하면 버전 변경을 되돌린다.

1. 작업 트리가 깨끗한지, 원격보다 뒤처지지 않았는지, 같은 버전 릴리스가 없는지 확인
2. `Cargo.toml`, `README.md`, `installer\GetText.iss` 의 버전 표기 변경
3. `cargo test` 실행 후 `build-installer.ps1` 로 설치 파일 생성 (`Cargo.lock` 도 갱신됨)
4. `release vX.Y.Z` 커밋 생성 후 푸시
5. 설치 파일을 첨부해 GitHub Release 게시. 실행 중인 GetText 가 이 릴리스를 새 버전으로 감지함

네트워크나 대화형 화면이 필요한 테스트(실제 릴리스 조회와 다운로드, 접근성 조회)는 기본 테스트에서 제외되어 있다.

```powershell
cargo test -- --ignored
```

### 아이콘 재생성

`assets/gettext.ico`, `assets/gettext.png` 는 스크립트로 생성한다. Python 과 Pillow 가 필요하다.

```powershell
python tools/gen_icon.py
```

## 프로젝트 구조

```
gettext/
+-- assets/
|   +-- gettext.ico     # exe, 트레이 아이콘 (16-256px)
|   +-- gettext.png     # 창 아이콘
|   +-- app.manifest    # Common Controls v6, 모니터별 DPI 인식
+-- src/
|   +-- main.rs         # 진입점, 중복 실행 방지, 스레드 간 공유 상태
|   +-- config.rs       # 설정 모델, 단축키 검증, 설정 파일 저장/로드
|   +-- service.rs      # 트레이 아이콘, 키보드 단축키, 업데이트 확인 (백그라운드 Win32 스레드)
|   +-- mousehook.rs    # 전역 저수준 마우스 훅 (마우스 단축키 감지, 제외 프로그램)
|   +-- accessible.rs   # MSAA, UI Automation 텍스트 조회
|   +-- text.rs         # 텍스트 조각 조합, 특수 공백 정리, Tab 선택 순환
|   +-- popup.rs        # 팝업 편집 창 (Win32 EDIT 컨트롤, 명령 버튼)
|   +-- launch.rs       # 명령 버튼 실행 (자리 표시자, 팝업 브라우저)
|   +-- clipboard.rs    # 클립보드 복사
|   +-- autostart.rs    # 자동 실행 레지스트리 관리
|   +-- update.rs       # GitHub 릴리스 확인, 설치 파일 다운로드 (WinHTTP)
|   +-- ui.rs           # 설정 창 (egui)
+-- installer/
|   +-- GetText.iss     # Inno Setup 설치 스크립트
+-- tools/gen_icon.py   # 아이콘 생성 스크립트
+-- build.rs            # Windows 리소스(아이콘, 매니페스트, 버전 정보) 포함
+-- build-installer.ps1 # 설치 파일 생성 스크립트
+-- release.ps1         # GitHub Release 게시 스크립트
+-- Cargo.toml
```

### 동작 구조

- **서비스 스레드**: 메시지 전용 창을 만들어 트레이 아이콘, `RegisterHotKey` 키보드 단축키, 업데이트 확인을 처리함
- **마우스 훅 스레드**: `WH_MOUSE_LL` 훅으로 단축키 클릭을 가로채 서비스 스레드에 알림. 마우스 단축키를 끄면 훅을 제거함
- **팝업 스레드**: 단축키가 눌릴 때마다 생성됨. COM 을 초기화하고 접근성 API 로 텍스트를 가져온 뒤 팝업 창을 띄우며, 창이 닫히면 끝남
- **메인 스레드**: 설정 창 열기 요청이 오면 egui 창을 만들고, 창이 닫히면 다음 요청을 기다림
- **업데이트 스레드**: 업데이트 확인, 다운로드 때만 생성됨. 결과는 서비스 스레드에 메시지로 전달함

### 사용 라이브러리

| 크레이트 | 용도 |
|---|---|
| [eframe / egui](https://github.com/emilk/egui) | 설정 창 UI |
| [tray-icon](https://github.com/tauri-apps/tray-icon) | 트레이 아이콘, 우클릭 메뉴 |
| [windows](https://github.com/microsoft/windows-rs) | 접근성(MSAA, UIA), 마우스 훅, 단축키, 클립보드, 레지스트리, WinHTTP Win32 API |
| [serde / serde_json](https://serde.rs) | 설정 파일 직렬화 |
| [winresource](https://github.com/BenjaminRi/winresource) | exe 아이콘, 매니페스트, 버전 정보 리소스 |
