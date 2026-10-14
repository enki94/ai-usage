#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
case "$(uname -s)" in
  Darwin) xcode-select -p >/dev/null 2>&1 || { echo '먼저 xcode-select --install을 실행하세요.' >&2; exit 1; } ;;
  Linux) command -v pkg-config >/dev/null 2>&1 && pkg-config --exists webkit2gtk-4.1 || { echo 'README의 Linux 개발 패키지를 먼저 설치하세요.' >&2; exit 1; } ;;
  *) echo '이 스크립트는 macOS/Linux용입니다. Windows는 setup.ps1을 사용하세요.' >&2; exit 1 ;;
esac
command -v node >/dev/null 2>&1 || { echo 'Node.js LTS가 필요합니다.' >&2; exit 1; }
command -v npm >/dev/null 2>&1 || { echo 'npm이 필요합니다.' >&2; exit 1; }
export CARGO_HOME="$project_root/.tools/cargo"
export RUSTUP_HOME="$project_root/.tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
  mkdir -p "$project_root/.tools"
  curl --fail --location --proto '=https' --tlsv1.2 --output "$project_root/.tools/rustup-init.sh" https://sh.rustup.rs
  sh "$project_root/.tools/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain stable
fi
rustup component add rustfmt clippy
npm ci --cache "$project_root/.tools/npm-cache" --no-audit --no-fund
npm run tauri -- info
echo '설치 완료. npm run dev 또는 npm run build를 실행하세요.'
