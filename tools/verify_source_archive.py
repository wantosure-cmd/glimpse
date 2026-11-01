"""Extract our source ZIP safely into a fresh fixture, restore runtime and check it."""
import hashlib
import json
import subprocess
import uuid
import zipfile
from pathlib import Path, PurePosixPath
import argparse
import sys
from cargo_freshness import invalidate_application

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        while b := f.read(1024*1024):
            h.update(b)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument('directory', type=Path)
    p.add_argument('--output',type=Path,default=ROOT/'artifacts/p5-source-build.json')
    p.add_argument('--log',type=Path,default=ROOT/'artifacts-p5-source-check.log')
    p.add_argument('--keep-extracted-runtime',action='store_true',help='Keep the generated runtime copy for debugging; default removes it after a successful check')
    args = p.parse_args()
    directory = args.directory.resolve(strict=True)
    assert directory.is_relative_to(ROOT / 'dist')
    source_zip = directory / 'media-browser-source.zip'
    portable_zip = directory / 'media-browser-portable.zip'
    task = ROOT / 'target' / ('source-package-check-' + uuid.uuid4().hex)
    task.mkdir()
    (task / '.source-check-owned').write_text('generated source verification', encoding='ascii')
    with zipfile.ZipFile(source_zip) as z:
        records = json.loads(z.read('media-browser-source/SOURCE-FILES.json'))
        expected = {'media-browser-source/' + e['file']: e for e in records}
        for info in z.infolist():
            relative = PurePosixPath(info.filename)
            assert not relative.is_absolute() and '..' not in relative.parts and '\\' not in info.filename
            assert relative.parts[0] == 'media-browser-source'
            if info.is_dir():
                continue
            data = z.read(info)
            if info.filename != 'media-browser-source/SOURCE-FILES.json':
                e = expected[info.filename]
                assert len(data) == e['bytes'] and hashlib.sha256(data).hexdigest() == e['sha256']
            dest = task.joinpath(*relative.parts)
            assert dest.is_relative_to(task)
            dest.parent.mkdir(parents=True, exist_ok=True)
            with dest.open('xb') as f:
                f.write(data)
    source = task / 'media-browser-source'
    restore = subprocess.run([sys.executable, '-X', 'utf8', str(source / 'tools/restore_runtime.py'),
                              str(portable_zip), '--sha256', sha(portable_zip)], capture_output=True, timeout=90)
    assert restore.returncode == 0, restore.stderr.decode('utf-8', errors='replace')
    log = args.log
    target = ROOT / 'target/source-check-cache'
    target.mkdir(exist_ok=True)
    fresh_fingerprints = invalidate_application(target, 'debug')
    with log.open('wb') as out:
        checked = subprocess.run(['cargo', 'check', '--bins', '--locked', '--offline', '--target-dir',
                                   str(ROOT / 'target/source-check-cache')], cwd=source,
                                  stdout=out, stderr=subprocess.STDOUT, timeout=600)
    assert checked.returncode == 0, log.read_text(encoding='utf-8', errors='replace')[-4000:]
    generated_runtime_path = source / 'vendor/runtime'
    assert not generated_runtime_path.is_symlink() and not generated_runtime_path.is_junction()
    runtime = generated_runtime_path.resolve(strict=True)
    assert runtime.is_relative_to(task.resolve()) and task.resolve().is_relative_to(ROOT/'target')
    assert task.name.startswith('source-package-check-') and not runtime.is_symlink()
    with zipfile.ZipFile(portable_zip) as archive:
        payload = json.loads(archive.read('media-browser-portable/package-files.json'))
    expected_runtime = {r['path'].removeprefix('runtime/'): r for r in payload if r['path'].startswith('runtime/')}
    actual_runtime = list(runtime.iterdir())
    assert {f.name for f in actual_runtime} == set(expected_runtime)
    runtime_records = []
    for f in actual_runtime:
        assert f.is_file() and not f.is_symlink() and f.parent == runtime
        record = expected_runtime[f.name]
        assert f.stat().st_size == record['bytes'] and sha(f) == record['sha256']
        runtime_records.append(dict(file=f.name, bytes=record['bytes'], sha256=record['sha256']))
    if not args.keep_extracted_runtime:
        for f in actual_runtime:
            f.unlink()  # Only the individually hash-verified, flat generated files.
        runtime.rmdir()
    result = dict(checks_pass=True, source_zip_sha256=sha(source_zip), portable_zip_sha256=sha(portable_zip),
                  delivery_directory=str(directory),
                  packaged_exe_sha256=sha(directory/'media-browser-portable/media-browser.exe'),
                  extracted_source=str(source), source_files=len(records), check_exit_code=checked.returncode,
                  fresh_application_fingerprints=fresh_fingerprints,
                  extracted_runtime_retained=args.keep_extracted_runtime,
                  generated_runtime_files=runtime_records,
                  generated_runtime_bytes_reclaimed=(0 if args.keep_extracted_runtime else sum(r['bytes'] for r in runtime_records)),
                  restore_stdout=restore.stdout.decode('utf-8', errors='replace'),
                  scope='Fresh extracted application source, pinned portable runtime restoration and Cargo check --bins --locked --offline on current machine with existing Rust/Cargo/MSVC dependencies; not dependency-free offline compilation')
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
