$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$env:CARGO_HOME = Join-Path $projectRoot '.tools/cargo'
$env:RUSTUP_HOME = Join-Path $projectRoot '.tools/rustup'
$env:Path = "$env:CARGO_HOME/bin;$env:Path"
Push-Location -LiteralPath $projectRoot
try {
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) { throw 'Node.js LTS를 먼저 설치해 주세요: https://nodejs.org/' }
    $programFilesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
    if (-not $programFilesX86) { $programFilesX86 = 'C:\Program Files (x86)' }
    $vswhere = Join-Path $programFilesX86 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio Build Tools의 C++ 데스크톱 개발과 Windows SDK가 필요합니다: https://visualstudio.microsoft.com/visual-cpp-build-tools/' }
    $vc = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vc) {
        $fallback = Join-Path $programFilesX86 'Microsoft Visual Studio/2022/BuildTools'
        $compiler = Get-ChildItem -Path (Join-Path $fallback 'VC/Tools/MSVC/*/bin/Hostx64/x64/cl.exe') -ErrorAction SilentlyContinue
        if (-not $compiler) { throw 'Visual Studio Installer에서 C++ 데스크톱 개발 및 Windows SDK를 설치해 주세요.' }
    }
    $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    if (-not (Test-Path -LiteralPath $cargo)) {
        $installer = Join-Path $projectRoot '.tools/rustup-init.exe'
        New-Item -ItemType Directory -Path (Split-Path $installer) -Force | Out-Null
        & curl.exe --fail --location --connect-timeout 10 --max-time 120 --output $installer 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe'
        if ($LASTEXITCODE -ne 0) { throw 'Rust 설치 파일 다운로드 실패. 네트워크 연결을 확인한 뒤 setup.ps1을 다시 실행해 주세요.' }
        & $installer -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-msvc --default-toolchain stable
        if ($LASTEXITCODE -ne 0) { throw 'Rust 설치에 실패했습니다.' }
    }
    & rustup component add rustfmt clippy
    if ($LASTEXITCODE -ne 0) { throw 'Rust 개발 도구 설치에 실패했습니다.' }
    & cargo --version
    if ($LASTEXITCODE -ne 0) { throw 'Cargo 실행에 실패했습니다.' }
    & npm.cmd install --cache (Join-Path $projectRoot '.tools/npm-cache') --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'Tauri CLI 설치에 실패했습니다.' }
    & npm.cmd run tauri -- info
    if ($LASTEXITCODE -ne 0) { throw 'Tauri 환경 확인에 실패했습니다.' }
    Write-Host '설치 완료. npm run dev 또는 npm run build를 실행하세요.'
} finally { Pop-Location }
