#!/bin/sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
python3 scripts/gpui-compat.py --check vendor/gpui-ce
# The libraries have standalone manifests, so they can be copied independently.
# Keep the large upstream GPUI workspace separate from these three packages.
for manifest in Cargo.toml crates/gpui-smooth/Cargo.toml crates/gpui-glass/Cargo.toml; do
    cargo fmt --manifest-path "$manifest" -- --check
    cargo test --quiet --manifest-path "$manifest" --all-features --locked \
        --target-dir "$project_dir/target" \
        --config "patch.crates-io.gpui-ce.path='$project_dir/vendor/gpui-ce/crates/gpui'" \
        --config "patch.crates-io.gpui_ce_wgpu.path='$project_dir/vendor/gpui-ce/crates/gpui_wgpu'" \
        --config "patch.crates-io.gpui_ce_platform.path='$project_dir/vendor/gpui-ce/crates/gpui_platform'"
    cargo clippy --quiet --manifest-path "$manifest" --all-targets --all-features --locked \
        --target-dir "$project_dir/target" \
        --config "patch.crates-io.gpui-ce.path='$project_dir/vendor/gpui-ce/crates/gpui'" \
        --config "patch.crates-io.gpui_ce_wgpu.path='$project_dir/vendor/gpui-ce/crates/gpui_wgpu'" \
        --config "patch.crates-io.gpui_ce_platform.path='$project_dir/vendor/gpui-ce/crates/gpui_platform'" \
        -- -D warnings
done
