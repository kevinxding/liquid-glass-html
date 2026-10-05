#!/usr/bin/env python3
"""Smoke-test copied crates against unpatched and patched GPUI in a fresh consumer."""
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOG = ROOT / "artifacts/reuse-check.txt"


def run(args, *, expected=0, cwd=ROOT):
    result = subprocess.run(args, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    with LOG.open("a") as log:
        log.write("$ " + " ".join(map(str, args)) + "\n" + result.stdout + "\n")
    if result.returncode != expected:
        raise RuntimeError(f"Unexpected exit {result.returncode}: {args}\n{result.stdout[-6000:]}")
    return result.stdout


def main():
    LOG.parent.mkdir(exist_ok=True)
    LOG.write_text("Standalone crate and GPUI hook smoke checks\n")
    run(["python3", "scripts/gpui-compat.py", "--check", "vendor/gpui-ce"])
    with tempfile.TemporaryDirectory(prefix="gpui-reuse-", dir="/private/tmp") as temp:
        temp = Path(temp)
        for name in ["gpui-smooth", "gpui-glass"]:
            shutil.copytree(ROOT / "crates" / name, temp / name, ignore=shutil.ignore_patterns("target", "Cargo.lock"))
        gpui = temp / "upstream"
        shutil.copytree(ROOT / "vendor/gpui-ce", gpui, symlinks=True, ignore=shutil.ignore_patterns("target", ".git"))
        run(["git", "apply", "--reverse", str(ROOT / "patches/gpui-backdrop.patch")], cwd=gpui)
        guard = ["python3", str(ROOT / "scripts/gpui-compat.py")]
        run([*guard, "--check", str(gpui)], expected=1)
        source_override = ["--config", f"patch.crates-io.gpui-ce.path='{gpui / 'crates/gpui'}'"]
        target = ["--target-dir", str(ROOT / "target")]
        manifest = temp / "gpui-smooth/Cargo.toml"
        print("Checking standalone geometry without GPUI...", flush=True)
        run(["cargo", "test", "-q", "--offline", "--manifest-path", str(manifest), "--no-default-features", *target, *source_override])
        tree = run(["cargo", "tree", "--offline", "--manifest-path", str(manifest), "--no-default-features", "--edges", "normal", *source_override])
        assert tree.count("gpui-smooth v") == 1 and "gpui-ce v" not in tree, tree

        consumer = temp / "consumer"
        (consumer / "src").mkdir(parents=True)
        common = '''[package]
name = "reuse-consumer"
version = "0.0.0"
edition = "2024"
[dependencies]
gpui = { package = "gpui-ce", version = "=0.2.2", default-features = false }
gpui-smooth = { path = "../gpui-smooth", features = ["gpui"] }
'''
        patches = f'''[patch.crates-io]
gpui-ce = {{ path = "{gpui / 'crates/gpui'}" }}
gpui_ce_wgpu = {{ path = "{gpui / 'crates/gpui_wgpu'}" }}
'''
        (consumer / "Cargo.toml").write_text(common + patches)
        smooth_code = '''use gpui::{prelude::*, px, rgb};
use gpui_smooth::{smooth, ShapeStyle, SmoothShape};
fn main() {
    let _card = smooth(SmoothShape::rounded_rect(24., 0.6),
        ShapeStyle::new(rgb(0x396958)).stroke(px(1.), rgb(0xffffff)).dash(vec![px(4.), px(2.)]))
        .id("card").w(px(300.)).h(px(120.)).p_4().child("Reusable")
        .on_click(|_, _, _| {});
}
'''
        (consumer / "src/main.rs").write_text(smooth_code)
        print("Checking copied smoothing adapter against unpatched GPUI...", flush=True)
        check = ["cargo", "check", "-q", "--offline", "--manifest-path", str(consumer / "Cargo.toml"), *target]
        run(check)

        print("Checking explicit patch setup, idempotence, and source drift rejection...", flush=True)
        run([*guard, "--apply", str(gpui)])
        run([*guard, "--apply", str(gpui)])
        changed = gpui / "crates/gpui/src/scene.rs"
        original = changed.read_bytes()
        changed.write_bytes(original + b"\n// simulated future source change\n")
        run([*guard, "--check", str(gpui)], expected=1)
        changed.write_bytes(original)
        run([*guard, "--check", str(gpui)])

        (consumer / "Cargo.toml").write_text(common + 'gpui-glass = { path = "../gpui-glass" }\n' + patches)
        (consumer / "src/main.rs").write_text(smooth_code.replace('fn main() {', '''fn main() {
    let renderer = std::sync::Arc::new(gpui_glass::GlassRenderer::default());
    let _glass = gpui_glass::glass(renderer, 1, gpui_glass::GlassParams::default(), gpui_glass::Shape::Capsule)
        .id("glass").w(px(120.)).h(px(44.)).child("Glass")
        .on_click(|_, _, _| {});
'''))
        print("Checking copied glass crate against the prepared GPUI source...", flush=True)
        run(check)
    with LOG.open("a") as log:
        log.write("REUSE_CHECK_OK: pure geometry, unpatched GPUI adapter, patch idempotence/drift guard, copied glass consumer\n")
    print("REUSE_CHECK_OK", flush=True)


if __name__ == "__main__":
    main()
