; GetText 설치 스크립트 (Inno Setup 6)
; build-installer.ps1이 릴리스 빌드 결과(target\release\GetText.exe)를 넣고 이 스크립트를 컴파일함
; 사용자별 설치 (관리자 권한 불필요): %LocalAppData%\Programs\GetText

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif

#define AppName "GetText"
#define AppExe "GetText.exe"
#define AppPublisher "drasys"
#define AppUrl "https://github.com/workingdad365/gettext"
#define AppIdGuid "{3EB5A5E7-3E50-494A-BAE9-B355AB820E9A}"

[Setup]
AppId={{#AppIdGuid}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\GetText
DisableDirPage=yes
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=GetText-Setup-{#AppVersion}
SetupIconFile=..\assets\gettext.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
; 트레이 앱은 최상위 창이 없어 정상 종료 요청을 받지 못하므로 강제 종료 (PrepareToInstall에서 먼저 종료함)
CloseApplications=force
RestartApplications=no
ShowLanguageDialog=no

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
korean.LaunchAfterInstall=설치 후 {#AppName} 실행
korean.CreateDesktopIcon=바탕 화면 바로 가기 만들기
korean.AutoStart=Windows 로그인 시 자동 실행
english.LaunchAfterInstall=Launch {#AppName} after install
english.CreateDesktopIcon=Create a desktop shortcut
english.AutoStart=Start automatically at Windows sign-in

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked
; 업그레이드 시에는 앱 설정 창에서 바꾼 자동 실행 상태를 덮어쓰지 않도록 작업을 생략
Name: "autostart"; Description: "{cm:AutoStart}"; Check: not IsUpgrade

[Files]
Source: "{#BuildDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; 앱 설정 창의 자동 실행과 같은 키/값/형식(--autostart 인자 포함)을 사용하므로 서로 호환됨
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "GetText"; ValueData: """{app}\{#AppExe}"" --autostart"; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchAfterInstall}"; Flags: nowait postinstall skipifsilent
; 앱 내 업데이트(/SILENT /RELAUNCH=1)로 설치한 경우 앱을 바로 다시 실행
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: IsRelaunchRequested

[UninstallRun]
; 제거 전 실행 중인 앱 종료
Filename: "{sys}\taskkill.exe"; Parameters: "/IM {#AppExe} /F"; Flags: runhidden; RunOnceId: "KillApp"

[Code]
// 같은 AppId 로 이미 설치되어 있는지 여부 (Inno Setup 이 만드는 제거 정보 키 기준)
function IsUpgrade: Boolean;
begin
  Result := RegKeyExists(HKEY_CURRENT_USER,
    'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppIdGuid}_is1');
end;

// 앱 내 업데이트가 넘기는 /RELAUNCH=1 파라미터 여부
function IsRelaunchRequested: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

// 트레이 앱은 Restart Manager의 정상 종료 요청에 응답하지 않아 강제 종료까지 약 30초 대기하므로 미리 종료
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#AppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;

// 제거 시 앱이 직접 등록한 자동 실행 값도 정리 (설치 작업으로 만든 값은 uninsdeletevalue가 처리)
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'GetText');
end;
