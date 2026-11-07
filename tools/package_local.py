"""Build local-test payloads only; never assign a license to the application.

Windows Rust metadata (including build dependencies) supplies crate license texts.
No user media, caches, absolute Cargo registry paths, or diagnostic reports are
copied into the source archive. This is not an offline Cargo source vendor bundle.
"""
from __future__ import annotations
import argparse
import datetime
import hashlib
import json
import shutil
import struct
import subprocess
import sys
import zipfile
from pathlib import Path
from pe_dependencies import imports

ROOT = Path(__file__).resolve().parent.parent
PRODUCT = "media-browser-local-test-27194c79-v1"


def sha(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        while block := f.read(1024 * 1024):
            h.update(block)
    return h.hexdigest()


def write_json(path: Path, value) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def production_inputs() -> dict[str, str]:
    files = list((ROOT / 'src').rglob('*.rs')) + list((ROOT / 'assets').rglob('*'))
    files += list((ROOT / 'vendor').glob('*.json'))
    files += [ROOT / n for n in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml','build.rs')]
    return {f.relative_to(ROOT).as_posix(): sha(f) for f in sorted(files) if f.is_file()}


def production_record(name: str) -> bool:
    return name.startswith(('src/', 'assets/', 'vendor/')) or name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml','build.rs')


def copy(source: Path, target: Path) -> None:
    if not source.is_file() or source.is_symlink():
        raise RuntimeError(f"Not an ordinary input file: {source}")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


def write_source_archive(path: Path) -> None:
    # Explicit source allowlist excludes downloaded media, caches and logs.
    source_files = []
    for folder in ("src", "tools", "tests", "docs", "assets", ".github"):
        source_files.extend(f for f in (ROOT / folder).rglob("*") if f.is_file()
                            and "__pycache__" not in f.parts and f.suffix not in (".pyc", ".pyo"))
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "build.rs", "README.md", ".gitignore",
                 "产品需求与技术方案.md", "给开发AI的任务书.md", "AGENTS.md"):
        source_files.append(ROOT / name)
    source_files.extend((ROOT / "vendor").glob("*.json"))
    source_files.extend(ROOT / name for name in (".gitattributes", "LICENSE", "THIRD_PARTY_NOTICES.md") if (ROOT / name).is_file())
    records = []
    with zipfile.ZipFile(path, "x", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for f in sorted(set(source_files)):
            if f.is_symlink():
                raise RuntimeError("Source archive cannot contain symlinks")
            relative = f.relative_to(ROOT).as_posix()
            z.write(f, "media-browser-source/" + relative)
            records.append(dict(file=relative, bytes=f.stat().st_size, sha256=sha(f)))
        z.writestr("media-browser-source/SOURCE-FILES.json", json.dumps(records, ensure_ascii=False, indent=2))


def write_sums(out: Path) -> None:
    sums = [f"{sha(f)}  {f.name}" for f in sorted(out.iterdir()) if f.is_file() and f.name not in ("SHA256SUMS.txt", "SHA256SUMS.txt.tmp")]
    temporary = out / "SHA256SUMS.txt.tmp"
    temporary.write_text("\n".join(sums) + "\n", encoding="utf-8")
    temporary.replace(out / "SHA256SUMS.txt")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True, help="New output directory name; no overwrite")
    parser.add_argument("--payload-only", action="store_true", help="Stage internal installer verification before final archives")
    parser.add_argument("--refresh-source", action="store_true", help="Update source/docs archive and checksums, preserving already-tested binary packages")
    parser.add_argument("--report",type=Path,help="Also include a build-specific Markdown verification report in portable payload")
    parser.add_argument("--application", type=Path, default=ROOT / 'target/release/media-browser.exe',
                        help="Explicit application from the verified Cargo target directory; version/hash checks remain required")
    parser.add_argument("--setup-stub", type=Path, default=ROOT / 'target/setup-static/release/media-browser-setup.exe',
                        help="Explicit same-version static-CRT installer stub")
    parser.add_argument("--runtime-directory", type=Path, default=ROOT / "dist/browser-preview/runtime",
                        help="Local runtime staging directory; all existing pin/hash checks still apply")
    args = parser.parse_args()
    application = args.application.resolve(strict=True)
    if (ROOT / 'assets/i18n/zh-CN.json').exists():
        catalog = subprocess.run([sys.executable, '-X', 'utf8', str(ROOT / 'tools/ui_catalog.py'), 'check'],
                                 cwd=ROOT, capture_output=True)
        if catalog.returncode:
            raise RuntimeError('UI catalog preflight failed: ' + catalog.stderr.decode('utf-8', errors='replace')[-4000:])
    if not args.tag or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_." for c in args.tag) or args.tag in (".", ".."):
        raise RuntimeError("Invalid tag")
    out = ROOT / "dist" / args.tag
    application_version = __import__('tomllib').loads((ROOT / 'Cargo.toml').read_text(encoding='utf8'))['package']['version']
    if not args.refresh_source:
        for previous in (ROOT / 'dist').glob('*/delivery.json'):
            if (old_version := json.loads(previous.read_text(encoding='utf8')).get('application_version')) and tuple(map(int, application_version.split('.'))) <= tuple(map(int, old_version.split('.'))):
                raise RuntimeError('This application version was already packaged; bump Cargo.toml and rebuild the application and installer first')
        for executable in (application, args.setup_stub.resolve(strict=True)):
            environment = dict(__import__('os').environ, GLIMPSE_VERIFY_EXE=str(executable))
            file_version = subprocess.check_output(['powershell.exe','-NoProfile','-NonInteractive','-Command',
                "(Get-Item -LiteralPath $env:GLIMPSE_VERIFY_EXE -ErrorAction Stop).VersionInfo.FileVersion"],
                env=environment,creationflags=subprocess.CREATE_NO_WINDOW).decode('utf8').strip()
            if file_version != application_version:
                raise RuntimeError(f'Binary version {file_version} differs from Cargo {application_version}: rebuild {executable.name}')
    if args.refresh_source:
        summary = json.loads((out / "delivery.json").read_text(encoding="utf-8"))
        if summary.get("product") != PRODUCT:
            raise RuntimeError("Not an owned local delivery")
        if sha(application) != summary['exe_sha256']:
            raise RuntimeError('Source-only refresh cannot switch the application build; create a new delivery tag')
        baseline = summary.get('production_source_sha256')
        if baseline is None:
            with zipfile.ZipFile(out / 'media-browser-source.zip') as archive:
                baseline = {r['file']: r['sha256'] for r in json.loads(archive.read('media-browser-source/SOURCE-FILES.json')) if production_record(r['file'])}
        if production_inputs() != baseline:
            raise RuntimeError('Production inputs changed; rebuild and create a new delivery instead of refreshing source only')
        source = out / "media-browser-source.zip"
        previous_source_sha256 = sha(source)
        partial = out / "media-browser-source.zip.partial"
        write_source_archive(partial)
        partial.replace(source)
        summary.setdefault('source_archive_refreshes', []).append({
            'refreshed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'previous_source_sha256': previous_source_sha256,
            'source_sha256': sha(source),
            'unchanged_exe_sha256': summary['exe_sha256'],
            'scope': 'Source/docs/tools refresh only; production inputs, portable and installer binaries unchanged',
        })
        summary["files"] = [dict(file=e["file"], bytes=(out / e["file"]).stat().st_size,
                                 sha256=sha(out / e["file"])) for e in summary["files"]]
        write_json(out / "delivery.json", summary)
        write_sums(out)
        print("Updated source archive only; installer and portable ZIP preserved")
        return
    out.mkdir(exist_ok=False)
    portable = out / "media-browser-portable"
    portable.mkdir()
    copy(application, portable / "media-browser.exe")
    copy(ROOT / "tools/measure_user_load.ps1", portable / "measure-load.ps1")
    copy(ROOT / "vendor/runtime/vcruntime140.dll", portable / "vcruntime140.dll")
    app_imports = imports(portable / "media-browser.exe")
    for name in app_imports:
        if name.lower().startswith(("vcruntime", "msvcp", "concrt")) and not (portable / name.lower()).is_file():
            raise RuntimeError("Application CRT import missing beside exe: " + name)
    runtime_directory = args.runtime_directory.resolve(strict=True)
    if not runtime_directory.is_relative_to(ROOT) or runtime_directory.is_symlink():
        raise RuntimeError("Runtime staging directory must be inside the project")
    for f in sorted(runtime_directory.iterdir()):
        if f.is_file():
            copy(f, portable / "runtime" / f.name)
    # Compare runtime inputs to pinned records, not merely to another staging copy.
    expected = json.loads((ROOT / "vendor/runtime-manifest.json").read_text(encoding="utf-8-sig"))
    for e in expected:
        f = portable / "runtime" / e["file"]
        if f.stat().st_size != e["bytes"] or sha(f).lower() != e["sha256"].lower():
            raise RuntimeError(f"Pinned runtime mismatch: {e['file']}")
    for f in (ROOT / "vendor/runtime").iterdir():
        if f.is_file() and sha(f) != sha(portable / "runtime" / f.name):
            raise RuntimeError(f"Current runtime differs from preview: {f.name}")

    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
        "--filter-platform", "x86_64-pc-windows-msvc"], cwd=ROOT))
    ids = {n["id"] for n in metadata["resolve"]["nodes"]}
    crates = []
    missing = []
    for p in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if p["id"] not in ids or not p.get("source"):
            continue
        crate_root = Path(p["manifest_path"]).parent
        if p["name"] == "epaint_default_fonts" and (ROOT / "assets/emoji-font-manifest.json").exists():
            font_manifest = json.loads((ROOT / "assets/emoji-font-manifest.json").read_text(encoding="utf8"))
            if p["version"] != font_manifest["version"]:
                raise RuntimeError("Emoji font crate version differs from pinned resources")
            for resource in font_manifest["fonts"] + font_manifest["license_and_credit_texts"]:
                font_file = crate_root / resource["crate_file"]
                if font_file.stat().st_size != resource["bytes"] or sha(font_file) != resource["sha256"]:
                    raise RuntimeError("Pinned emoji font/license resource mismatch")
            copy(ROOT / "assets/emoji-font-manifest.json", portable / "licenses/emoji-font-manifest.json")
        texts = []
        for f in sorted(crate_root.rglob("*")):
            if not f.is_file() or f.is_symlink():
                continue
            if (f.name.upper().startswith(("LICENSE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE"))
                    or (p["name"] == "epaint_default_fonts" and f.parent.name == "fonts" and f.suffix.lower() == ".txt")):
                relative = f.relative_to(crate_root)
                dest = Path("licenses/rust") / f"{p['name']}-{p['version']}" / relative
                copy(f, portable / dest)
                texts.append(dest.as_posix())
        license_file = p.get("license_file")
        if license_file:
            f = crate_root / license_file
            dest = Path("licenses/rust") / f"{p['name']}-{p['version']}" / f.relative_to(crate_root)
            if dest.as_posix() not in texts:
                copy(f, portable / dest)
                texts.append(dest.as_posix())
        provenance_path = ROOT / "vendor/upstream-license-sources.json"
        if provenance_path.exists():
            for e in json.loads(provenance_path.read_text(encoding="utf-8")):
                if e["crate"] == p["name"] and e["version"] == p["version"]:
                    f = ROOT / e["file"]
                    if sha(f) != e["sha256"]:
                        raise RuntimeError("Upstream license text hash mismatch")
                    dest = Path("licenses/rust") / f"{p['name']}-{p['version']}" / f.name
                    copy(f, portable / dest)
                    if dest.as_posix() not in texts:
                        texts.append(dest.as_posix())
        if not texts:
            missing.append(f"{p['name']}-{p['version']}")
        crates.append(dict(name=p["name"], version=p["version"], spdx=p.get("license"),
                           repository=p.get("repository"), source=p["source"], texts=texts))
    write_json(portable / "licenses/rust-dependencies.json", crates)
    write_json(portable / "licenses/license-text-gaps.json", missing)

    for name in ["runtime-manifest.json", "heif-sources.json", "p0-downloads.json", "media-tools.json"]:
        copy(ROOT / "vendor" / name, portable / "licenses" / name)
    if (ROOT / "vendor/upstream-license-sources.json").exists():
        copy(ROOT / "vendor/upstream-license-sources.json", portable / "licenses/upstream-license-sources.json")
    for tool in ("ffmpeg", "ffprobe"):
        for arg, name in [("-version", "version"), ("-L", "license"), ("-buildconf", "buildconf")]:
            p = subprocess.run([str(portable / "runtime" / f"{tool}.exe"), "-hide_banner", arg],
                               capture_output=True, check=True)
            (portable / "licenses" / f"{tool}-{name}.txt").write_bytes(p.stdout + p.stderr)
    copy(ROOT / "docs/本地测试包说明.md", portable / "使用说明.md")
    copy(ROOT / "docs/第三方依赖与许可证.md", portable / "THIRD_PARTY_NOTICES.md")
    copy(ROOT / "assets/icons/lucide/LICENSE", portable / "licenses/lucide/LICENSE")
    copy(ROOT / "assets/icons/lucide/manifest.json", portable / "licenses/lucide/manifest.json")
    if args.report:
        report=args.report.resolve(strict=True)
        if not report.is_relative_to(ROOT/'docs'):
            raise RuntimeError('Verification report must be a project document')
        copy(report,portable/'本轮验证.md')
    files = [dict(path=f.relative_to(portable).as_posix(), bytes=f.stat().st_size, sha256=sha(f))
             for f in sorted(portable.rglob("*")) if f.is_file()]
    write_json(portable / "package-files.json", files)
    files.append(dict(path="package-files.json", bytes=(portable / "package-files.json").stat().st_size,
                      sha256=sha(portable / "package-files.json")))
    stub = args.setup_stub.resolve(strict=True)
    setup_imports = imports(stub)
    if any(n.lower().startswith(("vcruntime", "msvcp", "concrt")) for n in setup_imports):
        raise RuntimeError("Installer still imports a dynamic MSVC CRT; run tools/build_setup.ps1")
    if b'requestedExecutionLevel level="asInvoker"' not in stub.read_bytes():
        raise RuntimeError("Installer is missing the embedded asInvoker manifest")
    installer = out / "media-browser-setup-local-test.exe"
    entries = []
    with installer.open("xb") as target:
        with stub.open("rb") as f:
            shutil.copyfileobj(f, target)
        for e in files:
            e = dict(e, offset=target.tell())
            entries.append(e)
            with (portable / e["path"]).open("rb") as f:
                shutil.copyfileobj(f, target)
        manifest = json.dumps(dict(product=PRODUCT, files=entries, stub_bytes=stub.stat().st_size,
                                   context_menu=False, registered=False), ensure_ascii=False).encode("utf-8")
        target.write(manifest)
        target.write(b"MBSET001" + struct.pack("<QQ", len(manifest), stub.stat().st_size))
    if args.payload_only:
        # Internal installation evidence must not impersonate a final delivery
        # or reserve the application version in the final-delivery scan.
        write_json(out / 'staging.json', dict(
            status='internal-payload-test-only', product=PRODUCT,
            application_version=application_version,
            exe_sha256=sha(portable / 'media-browser.exe'),
            setup_stub_sha256=sha(stub),
            production_source_sha256=production_inputs(),
            rust_resolved_windows_dependencies=len(crates),
            missing_license_texts=missing,
            files=[dict(file=installer.name, bytes=installer.stat().st_size, sha256=sha(installer))],
            scope='Internal payload/installation test only; no final portable/source archives or delivery acceptance'))
        print(json.dumps(dict(installer=str(installer), exe_sha256=sha(portable / "media-browser.exe"),
                              license_text_gaps=missing), ensure_ascii=False))
        return
    with zipfile.ZipFile(out / "media-browser-portable.zip", "x", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for f in sorted(portable.rglob("*")):
            if f.is_file():
                z.write(f, "media-browser-portable/" + f.relative_to(portable).as_posix())
    write_source_archive(out / "media-browser-source.zip")
    deliverables = [f for f in sorted(out.iterdir()) if f.is_file()]
    summary = dict(status="local-test-only", application_license="undecided-by-owner",
                   product=PRODUCT, application_version=__import__("tomllib").loads((ROOT / "Cargo.toml").read_text(encoding="utf8"))["package"]["version"],
                   app_imports=app_imports, installer_imports=setup_imports,
                   exe_sha256=sha(portable / "media-browser.exe"), setup_stub_sha256=sha(stub),
                   rust_resolved_windows_dependencies=len(crates), missing_license_texts=missing,
                   reference_only_license_crates=sorted({e["crate"] for e in json.loads((ROOT / "vendor/upstream-license-sources.json").read_text(encoding="utf-8")) if e.get("reference_only")}),
                   files=[dict(file=f.name, bytes=f.stat().st_size, sha256=sha(f)) for f in deliverables],
                   limitations=["No public distribution licensing clearance", "No clean Windows 10/11 offline machine verification",
                                "Source archive is not an offline Cargo/native dependency vendor bundle",
                                "Unsigned installer; no automatic update; uninstall old version before installing"])
    summary['production_source_sha256'] = production_inputs()
    write_json(out / "delivery.json", summary)
    write_sums(out)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
