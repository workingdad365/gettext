# GetText 릴리스 스크립트
# 1) 버전 자동 증가 (Cargo.toml, README.md, installer\GetText.iss)
# 2) cargo test, build-installer.ps1 로 설치 파일 생성
# 3) 버전 변경 커밋 및 푸시
# 4) 설치 파일을 GitHub Release 로 게시 (앱의 업데이트 확인이 이 릴리스를 감지함)
# 사용 예:
#   .\release.ps1 "버그 수정"              # 1.0.0 -> 1.0.1
#   .\release.ps1 "기능 추가" -Bump minor  # 1.0.1 -> 1.1.0
#   .\release.ps1 "재게시" -NoBump         # 현재 버전 그대로 게시 (게시 실패 후 재시도용)
#   .\release.ps1 "확인" -DryRun           # 다음 버전만 출력하고 아무것도 바꾸지 않음

[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$Message,

    [ValidateSet("patch", "minor", "major")]
    [string]$Bump = "patch",

    [switch]$NoBump,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$utf8 = [System.Text.UTF8Encoding]::new($false)

function Invoke-Native([string]$what, [scriptblock]$block) {
    & $block
    if ($LASTEXITCODE -ne 0) { throw "$what 실패 (exit $LASTEXITCODE)" }
}

function Get-NextVersion([string]$current, [string]$part) {
    $v = [version]$current
    switch ($part) {
        "major" { "{0}.0.0" -f ($v.Major + 1) }
        "minor" { "{0}.{1}.0" -f $v.Major, ($v.Minor + 1) }
        default { "{0}.{1}.{2}" -f $v.Major, $v.Minor, ($v.Build + 1) }
    }
}

# 정규식 치환. 대상 패턴이 정확히 한 번 나와야 함
function Update-File([string]$path, [string]$pattern, [string]$replacement) {
    $text = [System.IO.File]::ReadAllText($path)
    $count = ([regex]::Matches($text, $pattern)).Count
    if ($count -ne 1) { throw "$path 에서 버전 표기를 찾지 못함 (일치 $count 건)" }
    [System.IO.File]::WriteAllText($path, [regex]::Replace($text, $pattern, $replacement), $utf8)
}

if ([string]::IsNullOrWhiteSpace($Message)) { throw "릴리스 메시지가 비어 있음" }
foreach ($cmd in "git", "gh", "cargo") {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) { throw "$cmd 을 찾을 수 없음" }
}

$root = $PSScriptRoot
$cargoPath = Join-Path $root "Cargo.toml"
$readmePath = Join-Path $root "README.md"
$issPath = Join-Path $root "installer\GetText.iss"
$versionFiles = @("Cargo.toml", "Cargo.lock", "README.md", "installer/GetText.iss")

$cargoToml = [System.IO.File]::ReadAllText($cargoPath)
if ($cargoToml -notmatch '(?ms)^\[package\].*?^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') {
    throw "Cargo.toml 에서 version 을 찾지 못함"
}
$current = $Matches[1]
$version = if ($NoBump) { $current } else { Get-NextVersion $current $Bump }
$tag = "v$version"

if ($DryRun) {
    Write-Host "현재 버전: $current -> 릴리스 버전: $version ($tag)"
    return
}

$notesFile = $null
$bumped = $false
$committed = $false
Push-Location -LiteralPath $root
try {
    # 사전 점검: 깨끗한 작업 트리, 원격보다 뒤처지지 않음, 같은 태그 없음
    $changes = & git status --porcelain
    if ($LASTEXITCODE -ne 0) { throw "Git 작업 상태 조회 실패" }
    if ($changes) { throw "커밋하지 않은 변경 사항이 있음. 커밋 후 다시 실행할 것" }

    $branch = (& git rev-parse --abbrev-ref HEAD).Trim()
    Invoke-Native "git fetch" { git fetch --quiet origin }
    $behind = (& git rev-list --count "HEAD..origin/$branch").Trim()
    if ($LASTEXITCODE -ne 0) { throw "origin/$branch 브랜치를 찾을 수 없음" }
    if ([int]$behind -gt 0) { throw "원격보다 $behind 커밋 뒤처져 있음. pull 후 다시 실행할 것" }

    & gh release view $tag *> $null
    if ($LASTEXITCODE -eq 0) { throw "이미 $tag 릴리스가 있음" }
    if (& git ls-remote --tags origin "refs/tags/$tag") { throw "원격에 이미 $tag 태그가 있음" }

    Write-Host "릴리스 버전: $current -> $version"

    if (-not $NoBump) {
        $bumped = $true
        Update-File $cargoPath '(?ms)(^\[package\].*?^version\s*=\s*")[0-9]+\.[0-9]+\.[0-9]+(")' "`${1}$version`${2}"
        Update-File $readmePath '(<br>v)[0-9]+\.[0-9]+\.[0-9]+(</p>)' "`${1}$version`${2}"
        Update-File $issPath '(#define AppVersion ")[0-9]+\.[0-9]+\.[0-9]+(")' "`${1}$version`${2}"
    }

    Write-Host "테스트 실행"
    Invoke-Native "cargo test" { cargo test --quiet }

    # 릴리스 빌드 과정에서 Cargo.lock 의 패키지 버전도 함께 갱신됨
    & (Join-Path $root "build-installer.ps1")
    $setup = Join-Path $root "dist\GetText-Setup-$version.exe"
    if (-not (Test-Path -LiteralPath $setup -PathType Leaf)) { throw "설치 파일이 없음: $setup" }

    if ($bumped) {
        Invoke-Native "git add" { git add -- $versionFiles }
        Invoke-Native "git commit" { git commit --quiet -m "release $tag" }
        $committed = $true
    }
    Invoke-Native "git push" { git push --quiet origin $branch }
    $commit = (& git rev-parse HEAD).Trim()

    # 한글, 따옴표와 줄바꿈을 그대로 전달하도록 UTF-8 파일로 릴리스 본문을 작성함
    $notesFile = [System.IO.Path]::GetTempFileName()
    [System.IO.File]::WriteAllText($notesFile, $Message, $utf8)

    Write-Host "릴리스 게시: $tag / $setup"
    & gh release create $tag $setup --target $commit --title $tag --notes-file $notesFile --latest
    if ($LASTEXITCODE -ne 0) {
        throw "릴리스 게시 실패 (exit $LASTEXITCODE). 버전 커밋은 푸시되었으므로 .\release.ps1 `"...`" -NoBump 로 다시 시도할 것"
    }
    Write-Host "완료: $tag"
}
catch {
    # 커밋 전 실패: 버전 표기 변경을 되돌림
    if ($bumped -and -not $committed) {
        & git checkout -- $versionFiles 2>$null
        Write-Host "버전 변경을 되돌림"
    }
    throw
}
finally {
    if ($notesFile -and (Test-Path -LiteralPath $notesFile)) {
        Remove-Item -LiteralPath $notesFile -Force
    }
    Pop-Location
}
