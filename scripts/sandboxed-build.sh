#!/bin/sh
set -eu

script_dir="$(dirname -- "$(realpath -- "$0")")"
project_dir="$(dirname -- "$script_dir")"
project_local_dir="$project_dir/.local"
fetch_sandbox_profile="$project_local_dir/cargo-fetch.sb"
build_sandbox_profile="$project_local_dir/cargo-build.sb"
home_dir="$project_local_dir/home"
cargo_home="$project_local_dir/cargo-home"
target_dir="$project_local_dir/target"
tmp_dir="$project_local_dir/tmp"
cargo_path="$(rustup which cargo)"
rustc_path="$(rustup which rustc)"
cargo_toolchain_dir="$(dirname -- "$(dirname -- "$cargo_path")")"
rustc_toolchain_dir="$(dirname -- "$(dirname -- "$rustc_path")")"
developer_dir="$(xcode-select -p)"
xcode_contents_dir="$(dirname -- "$developer_dir")"
darwin_tmp_dir="$(getconf DARWIN_USER_TEMP_DIR)"
darwin_tmp_real_dir="$(realpath -- "$darwin_tmp_dir")"

mkdir -p "$project_local_dir" "$home_dir" "$cargo_home" "$target_dir" "$tmp_dir"

sed \
  -e "s#__PROJECT_DIR__#$project_dir#g" \
  -e "s#__HOME_DIR__#$home_dir#g" \
  -e "s#__CARGO_HOME__#$cargo_home#g" \
  -e "s#__TARGET_DIR__#$target_dir#g" \
  -e "s#__TMP_DIR__#$tmp_dir#g" \
  -e "s#__CARGO_TOOLCHAIN_DIR__#$cargo_toolchain_dir#g" \
  -e "s#__RUSTC_TOOLCHAIN_DIR__#$rustc_toolchain_dir#g" \
  -e "s#__XCODE_CONTENTS_DIR__#$xcode_contents_dir#g" \
  -e "s#__DARWIN_TMP_DIR__#$darwin_tmp_dir#g" \
  -e "s#__DARWIN_TMP_REAL_DIR__#$darwin_tmp_real_dir#g" \
  "$script_dir/cargo-fetch.sb.in" > "$fetch_sandbox_profile"

sed \
  -e "s#__PROJECT_DIR__#$project_dir#g" \
  -e "s#__HOME_DIR__#$home_dir#g" \
  -e "s#__CARGO_HOME__#$cargo_home#g" \
  -e "s#__TARGET_DIR__#$target_dir#g" \
  -e "s#__TMP_DIR__#$tmp_dir#g" \
  -e "s#__CARGO_TOOLCHAIN_DIR__#$cargo_toolchain_dir#g" \
  -e "s#__RUSTC_TOOLCHAIN_DIR__#$rustc_toolchain_dir#g" \
  -e "s#__XCODE_CONTENTS_DIR__#$xcode_contents_dir#g" \
  -e "s#__DARWIN_TMP_DIR__#$darwin_tmp_dir#g" \
  -e "s#__DARWIN_TMP_REAL_DIR__#$darwin_tmp_real_dir#g" \
  "$script_dir/cargo-build.sb.in" > "$build_sandbox_profile"

CARGO_HOME="$cargo_home" \
HOME="$home_dir" \
TMPDIR="$tmp_dir" \
DEVELOPER_DIR="$developer_dir" \
RUSTC="$rustc_path" \
sandbox-exec -f "$fetch_sandbox_profile" "$cargo_path" fetch --locked

CARGO_HOME="$cargo_home" \
CARGO_TARGET_DIR="$target_dir" \
HOME="$home_dir" \
TMPDIR="$tmp_dir" \
DEVELOPER_DIR="$developer_dir" \
RUSTC="$rustc_path" \
sandbox-exec -f "$build_sandbox_profile" "$cargo_path" build --release --locked --offline

printf '%s\n' "$target_dir/release/distill"
