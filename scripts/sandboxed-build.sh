#!/bin/sh
set -eu

script_dir="$(dirname -- "$(realpath -- "$0")")"
project_dir="$(dirname -- "$script_dir")"
project_local_dir="$project_dir/.local"
sandbox_profile="$project_local_dir/cargo-build.sb"
cargo_home="$project_local_dir/cargo-home"
target_dir="$project_local_dir/target"
tmp_dir="$project_local_dir/tmp"
cargo_path="$(rustup which cargo)"
rustc_path="$(rustup which rustc)"
developer_dir="$(xcode-select -p)"

mkdir -p "$project_local_dir" "$cargo_home" "$target_dir" "$tmp_dir"

sed \
  -e "s#__PROJECT_DIR__#$project_dir#g" \
  "$script_dir/cargo-build.sb.in" > "$sandbox_profile"

CARGO_HOME="$cargo_home" \
TMPDIR="$tmp_dir" \
DEVELOPER_DIR="$developer_dir" \
RUSTC="$rustc_path" \
"$cargo_path" fetch --locked

CARGO_HOME="$cargo_home" \
CARGO_TARGET_DIR="$target_dir" \
TMPDIR="$tmp_dir" \
DEVELOPER_DIR="$developer_dir" \
RUSTC="$rustc_path" \
sandbox-exec -f "$sandbox_profile" "$cargo_path" build --release --locked --offline

printf '%s\n' "$target_dir/release/distill"
