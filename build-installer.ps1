# GetText 설치 파일 생성 스크립트
# 1) cargo build --release 로 target\release\GetText.exe 생성
# 2) Inno Setup(ISCC)으로 dist\GetText-Setup-<버전>.exe 생성
# 사용: pwsh ./build-installer.ps1 [-Install]
#   -Install: 생성 직후 조용히 설치 (검증용)

param(
    [switch]$Install
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

# 버전은 Cargo.toml [package] 의 version 에서 읽음
$cargoToml = Get-Content (Join-Path $root "Cargo.toml") -Raw
if ($cargoToml -notmatch '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"') { throw "Cargo.toml에서 version을 찾지 못함" }
$version = $Matches[1]

# ISCC 위치: 사용자별 설치 또는 시스템 설치
$isccCandidates = @(
    (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
)
$iscc = $isccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6이 없습니다. 설치: winget install JRSoftware.InnoSetup" }

$buildDir = Join-Path $root "target\release"
Write-Host "1/2 릴리스 빌드: $buildDir (v$version)"
Push-Location -LiteralPath $root
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build 실패 (exit $LASTEXITCODE)" }
}
finally {
    Pop-Location
}

Write-Host "2/2 설치 파일 생성"
& $iscc "/DAppVersion=$version" "/DBuildDir=$buildDir" (Join-Path $root "installer\GetText.iss")
if ($LASTEXITCODE -ne 0) { throw "ISCC 실패 (exit $LASTEXITCODE)" }

$setup = Join-Path $root "dist\GetText-Setup-$version.exe"
Write-Host "완료: $setup"

if ($Install) {
    Write-Host "조용히 설치 중..."
    $p = Start-Process $setup -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS" -Wait -PassThru
    Write-Host "설치 종료 코드: $($p.ExitCode)"
}
