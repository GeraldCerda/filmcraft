set -euo pipefail
if [ "$(uname -s)" != Darwin ]; then
    printf 'Run this installer on your Mac.\n' >&2
    exit 1
fi
source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
app_path="${1:-/Applications/FilmCraft.app}"
if [ ! -d "$app_path" ] && [ "$app_path" = /Applications/FilmCraft.app ] && [ -d "$HOME/Applications/FilmCraft.app" ]; then
    app_path="$HOME/Applications/FilmCraft.app"
fi
if [ ! -d "$app_path/Contents/MacOS" ]; then
    printf 'FilmCraft was not found. Run bash packaging/macos/install-custom.sh followed by the full path to your FilmCraft.app.\n' >&2
    exit 1
fi
app_path="$(cd "$(dirname "$app_path")" && pwd)/$(basename "$app_path")"
executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app_path/Contents/Info.plist")"
case "$executable" in
    FilmCraft|filmcraft) ;;
    *) printf 'The selected app is not a recognized FilmCraft bundle.\n' >&2; exit 1 ;;
esac
if ! xcode-select -p >/dev/null 2>&1; then
    xcode-select --install
    printf 'Finish installing Apple Command Line Tools, then run this installer again.\n'
    exit 1
fi
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustup >/dev/null 2>&1; then
    mkdir -p "$source_dir/target/custom-installer"
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$source_dir/target/custom-installer/rustup-init.sh"
    bash "$source_dir/target/custom-installer/rustup-init.sh" -y --profile minimal --default-toolchain 1.95.0
fi
rustup toolchain install 1.95.0 --profile minimal
build_dir="${FILMCRAFT_BUILD_DIR:-$source_dir/target/custom-mac}"
mkdir -p "$build_dir"
build_dir="$(cd "$build_dir" && pwd)"
printf 'Building the customized FilmCraft app in %s\n' "$build_dir"
cd "$source_dir"
CARGO_TARGET_DIR="$build_dir" cargo +1.95.0 build --locked --release -p filmcraft
if /usr/bin/pgrep -x "$executable" >/dev/null 2>&1; then
    printf 'Save your work and quit FilmCraft, then press Return.\n'
    read -r response
    if /usr/bin/pgrep -x "$executable" >/dev/null 2>&1; then
        printf 'FilmCraft is still running. Quit it before installing.\n' >&2
        exit 1
    fi
fi
app_parent="$(dirname "$app_path")"
if [ ! -w "$app_parent" ]; then
    printf 'Your Mac requires administrator permission to replace %s\n' "$app_path"
    sudo -v
    elevated=1
else
    elevated=0
fi
run_install() {
    if [ "$elevated" = 1 ]; then sudo "$@"; else "$@"; fi
}
stage_parent="$(run_install mktemp -d "$app_parent/.FilmCraft-install.XXXXXXXX")"
stage_app="$stage_parent/FilmCraft.app"
backup_path="$app_parent/FilmCraft-backup-$(date +%Y%m%d-%H%M%S)-$$.app"
original_moved=0
installed=0
cleanup() {
    if [ "$original_moved" = 1 ] && [ "$installed" = 0 ]; then
        if [ ! -e "$app_path" ] && [ -d "$backup_path" ]; then run_install mv "$backup_path" "$app_path"; fi
    fi
    if [ -d "$stage_parent" ]; then run_install rm -rf "$stage_parent"; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
run_install ditto "$app_path" "$stage_app"
run_install cp "$build_dir/release/filmcraft" "$stage_app/Contents/MacOS/$executable"
run_install chmod 755 "$stage_app/Contents/MacOS/$executable"
run_install /usr/libexec/PlistBuddy -c 'Set :CFBundleShortVersionString 0.7.2' "$stage_app/Contents/Info.plist"
run_install /usr/libexec/PlistBuddy -c 'Set :CFBundleVersion 0.7.2' "$stage_app/Contents/Info.plist"
run_install /usr/libexec/PlistBuddy -c 'Delete :FilmCraftVersion' "$stage_app/Contents/Info.plist" 2>/dev/null || true
run_install /usr/libexec/PlistBuddy -c 'Add :FilmCraftVersion string 0.7.2-custom-sequence-1' "$stage_app/Contents/Info.plist"
run_install codesign --force --deep --sign - --entitlements "$source_dir/packaging/macos/entitlements.plist" "$stage_app"
run_install codesign --verify --deep --strict "$stage_app"
if [ -e "$backup_path" ]; then printf 'Backup path already exists. No app replaced.\n' >&2; exit 1; fi
original_moved=1
run_install mv "$app_path" "$backup_path"
run_install mv "$stage_app" "$app_path"
installed=1
printf 'Installed: %s\nBackup: %s\n' "$app_path" "$backup_path"
open "$app_path"
