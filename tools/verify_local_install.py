"""Real Win32 install/uninstall smoke, restricted to a fresh owned workspace fixture."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import subprocess
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def installation_metadata(installer, allow_staging=False):
    final_path = installer.parent / 'delivery.json'
    if final_path.is_file():
        return json.loads(final_path.read_text(encoding='utf8')), False
    if not allow_staging:
        raise RuntimeError('No final delivery manifest; internal payload tests require --allow-staging')
    from package_local import PRODUCT
    metadata = json.loads((installer.parent / 'staging.json').read_text(encoding='utf8'))
    assert metadata['status'] == 'internal-payload-test-only' and metadata['product'] == PRODUCT
    assert not metadata['missing_license_texts']
    return metadata, True


def sha(p):
    h = hashlib.sha256()
    with p.open("rb") as f:
        while b := f.read(1024 * 1024):
            h.update(b)
    return h.hexdigest()


def run(exe, *args, expected=0):
    p = subprocess.run([str(exe), *map(str, args)], capture_output=True, timeout=90,
                       creationflags=subprocess.CREATE_NO_WINDOW)
    assert p.returncode == expected, (args, p.returncode, p.stderr.decode(errors="replace"))
    return p


def wait_for(predicate, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.1)
    raise AssertionError("Timed out waiting for native installer outcome")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("installer", type=Path)
    parser.add_argument("--registry", action="store_true", help="Also verify owned current-user uninstall registration")
    parser.add_argument('--allow-staging', action='store_true', help='Explicitly verify an internal payload using staging.json; never a final-delivery acceptance')
    parser.add_argument("--output",type=Path,default=ROOT / "artifacts/p5-installation-smoke.json")
    args = parser.parse_args()
    installer = args.installer.resolve(strict=True)
    assert installer.is_relative_to(ROOT / "dist")
    delivery, internal_staging = installation_metadata(installer, args.allow_staging)
    if internal_staging and args.registry:
        raise RuntimeError('Internal payload smoke must not change current-user registration')
    expected_version = delivery['application_version']
    assert sha(installer) == next(e['sha256'] for e in delivery['files'] if e['file'] == installer.name)
    task = ROOT / "target" / ("installation-smoke-" + uuid.uuid4().hex)
    task.mkdir()
    (task / ".installer-smoke-owned").write_text("generated local installer test", encoding="ascii")
    root = task / "中文 空格 安装目录"
    nonempty = task / "非空目录"
    nonempty.mkdir()
    sentinel = nonempty / "保留文件.txt"
    sentinel.write_text("must remain unchanged", encoding="utf-8")
    sentinel_hash = sha(sentinel)
    denied = run(installer, "--silent", "--install-dir", nonempty, "--no-register", expected=1)
    refusal = denied.stderr.decode("utf-8")
    assert sha(sentinel) == sentinel_hash and len(list(nonempty.iterdir())) == 1
    if "请先关闭媒体浏览器" in refusal:
        args.output.write_text(json.dumps(dict(checks_pass=False, status='blocked-by-running-instance',
            installer_sha256=sha(installer), expected_application_version=expected_version,
            fixture=str(task), sentinel_unchanged=True, installation_not_started=True,
            error=refusal.strip(), scope='Main-instance exclusion observed; complete install/uninstall not executed'),
            ensure_ascii=False, indent=2), encoding='utf8')
        raise RuntimeError("Existing application/installer holds the main lock; no installation was attempted. " + refusal.strip())
    assert "安装目录非空" in refusal, refusal
    junction_target = task / "链接目标"
    junction_target.mkdir()
    junction = task / "目录联接"
    junction_result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File",
                    str(ROOT / "tools/make_install_junction.ps1"), "-FixtureRoot", str(task),
                    "-Link", str(junction), "-Target", str(junction_target)], capture_output=True, timeout=15,
                   creationflags=subprocess.CREATE_NO_WINDOW)
    assert junction_result.returncode == 0, junction_result.stderr.decode("utf-8", errors="replace")
    denied = run(installer, "--silent", "--install-dir", junction / "拒绝创建", "--no-register", expected=1)
    assert "重解析路径" in denied.stderr.decode("utf-8") and not list(junction_target.iterdir())
    started = time.time()
    registry_key = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\LargeScreenMediaBrowser.LocalTest"
    if args.registry:
        import winreg
        try:
            existing = winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry_key)
        except FileNotFoundError:
            pass
        else:
            existing.Close()
            raise RuntimeError("Existing local-test registration; refusing overwrite")
    run(installer, "--silent", "--install-dir", root, *([] if args.registry else ["--no-register"]))
    marker = root / "installation.json"
    manifest = json.loads(marker.read_text(encoding="utf-8"))
    for e in manifest["files"]:
        f = root / e["path"]
        assert f.stat().st_size == e["bytes"] and sha(f) == e["sha256"], e["path"]
    assert manifest["registered"] == args.registry and not manifest["context_menu"]
    if args.registry:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry_key) as key:
            assert winreg.QueryValueEx(key, "InstallLocation")[0] == str(root)
            assert winreg.QueryValueEx(key, "UninstallString")[0] == f'"{root / "uninstall.exe"}" --uninstall'
            assert winreg.QueryValueEx(key, "DisplayVersion")[0] == expected_version
    exe = root / "media-browser.exe"
    assert sha(exe) == delivery['exe_sha256'], 'Installed application differs from declared tested build'
    decoded = task / "decoded-heic.png"
    heic = ROOT / "test-media/upstream-example.heic"
    original_hash = sha(heic)
    run(exe, "--decode-heic", heic, decoded)
    assert decoded.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
    probe = run(exe, "--probe-video", ROOT / "test-media/test-h264-aac.mov")
    (task / "probe.json").write_bytes(probe.stdout)
    report = task / "running.json"
    fixture = task / "媒体目录"
    fixture.mkdir()
    import shutil
    shutil.copyfile(ROOT / "test-media/ui-demo/封面.jpg", fixture / "1.jpg")
    # Normal main scope exercises the same named mutex used by the installer.
    app = subprocess.Popen([str(exe), "--open", str(fixture), "--data-dir", str(task / "data"),
                            "--instance-report", str(report), "--quit-after", "12000"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        wait_for(report.exists, 15)
        assert app.poll() is None
        blocked_root = task / "运行中拒绝安装"
        denied = run(installer, "--silent", "--install-dir", blocked_root, "--no-register", expected=1)
        assert "请先关闭媒体浏览器" in denied.stderr.decode("utf-8")
        assert not blocked_root.exists()
        # Use the exact cleanup worker entry to observe the nonzero failure code;
        # installed --uninstall then also exercises the spawned-worker path below.
        ended = subprocess.Popen(["cmd.exe", "/d", "/c", "exit", "0"], creationflags=subprocess.CREATE_NO_WINDOW)
        ended.wait(timeout=5)
        denied = run(root / "uninstall.exe", "--silent", "--cleanup-root", root,
                     "--wait-pid", ended.pid, expected=1)
        assert "请先关闭媒体浏览器" in denied.stderr.decode("utf-8")
        assert marker.exists() and sha(exe) == next(e["sha256"] for e in manifest["files"] if e["path"] == "media-browser.exe")
        assert app.wait(timeout=25) == 0
    finally:
        if app.poll() is None:
            # Never force-kill: the diagnostic auto-exit is bounded.
            app.wait(timeout=25)
    # Unknown files and changed owned files are retained by the real uninstall.
    unknown = root / "保留的未知文件.txt"
    unknown.write_text("keep this", encoding="utf-8")
    modified = root / "使用说明.md"
    modified.write_text("user annotation", encoding="utf-8")
    expected_unknown, expected_modified = sha(unknown), sha(modified)
    run(root / "uninstall.exe", "--silent", "--uninstall")
    wait_for(lambda: not marker.exists() and not (root / "uninstall.exe").exists())
    assert not exe.exists() and not (root / "runtime").exists()
    assert sha(unknown) == expected_unknown and sha(modified) == expected_modified
    assert (task / "data/settings.json").is_file()
    assert sha(heic) == original_hash
    if args.registry:
        try:
            key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry_key)
        except FileNotFoundError:
            pass
        else:
            key.Close()
            raise AssertionError("Owned uninstall registration was not removed")
    result = dict(checks_pass=True, installer_sha256=sha(installer),
                  internal_staging=internal_staging, final_delivery_acceptance=False,
                  exe_sha256=next(e["sha256"] for e in manifest["files"] if e["path"] == "media-browser.exe"),
                  started_utc=started, ended_utc=time.time(), fixture=str(task), files_verified=len(manifest["files"]),
                  nonempty_refused=True, running_install_refused=True, running_uninstall_refused=True,
                  actual_directory_junction_refused=True,
                  installed_heic_decode=True, installed_mov_probe=True,
                  unknown_and_modified_preserved=True, settings_preserved=True, originals_unchanged=True,
                  uninstall_registry_verified=args.registry,
                  expected_application_version=expected_version,
                  registered_version_verified=args.registry,
                  limits=["Explorer context menu not changed by this installer smoke", "No clean Windows 11/offline OS test",
                          "Uninstaller temporary worker executable remains in user temp"])
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
