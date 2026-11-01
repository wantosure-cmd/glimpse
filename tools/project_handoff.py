"""Pack and verify the complete local project for a private Windows handoff.

Includes untracked files and .git; excludes machine-specific target output except
the two standalone diagnostic helpers. Never installs software or uploads files.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
PREFIX = 'Glimpse/'
MANIFEST = 'PROJECT-TRANSFER.json'
DIRECTORIES = ('.git', 'src', 'assets', 'docs', 'tools', 'tests', 'vendor', 'test-media', 'dist', 'artifacts')
TOP_FILES = ('AGENTS.md', '.gitignore', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
             'build.rs', 'README.md', '产品需求与技术方案.md', '给开发AI的任务书.md')


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf8')


def safe_relative(value):
    path = PurePosixPath(value)
    if (not value or path.is_absolute() or '..' in path.parts or '\\' in value
            or ':' in value or '\x00' in value):
        raise ValueError('Unsafe manifest path: ' + repr(value))
    return path


def current_delivery(root):
    version = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf8'))['package']['version']
    candidates = [path for path in (root / 'dist').glob('*/delivery.json')
                  if read(path).get('application_version') == version]
    if len(candidates) != 1:
        raise ValueError('Expected exactly one delivery for Cargo version ' + version)
    path = candidates[0]
    record = read(path)
    if sha(path.parent / 'media-browser-portable/media-browser.exe') != record['exe_sha256']:
        raise ValueError('Current delivered executable differs from its manifest')
    for name, digest in record['production_source_sha256'].items():
        if sha(root / name) != digest:
            raise ValueError('Production source differs from delivered executable: ' + name)
    return path.parent, record


def inputs(root):
    result = [root / name for name in TOP_FILES]
    skipped = []
    for name in DIRECTORIES:
        base = root / name
        if not base.is_dir():
            continue
        for parent, dirs, files in os.walk(base, followlinks=False):
            retained = []
            for child in sorted(dirs):
                path = Path(parent) / child
                if path.is_symlink() or path.is_junction():
                    skipped.append(path.relative_to(root).as_posix())
                elif child not in ('__pycache__', '.pytest_cache'):
                    retained.append(child)
            dirs[:] = retained
            for child in sorted(files):
                path = Path(parent) / child
                if path.is_symlink():
                    skipped.append(path.relative_to(root).as_posix())
                elif path.suffix not in ('.pyc', '.pyo') and not path.name.endswith('.lock.tmp'):
                    result.append(path)
    for name in ('audio-meter.exe', 'opengl-probe.exe'):
        path = root / 'target/diagnostics/Release' / name
        if path.is_file():
            result.append(path)
    for path in result:
        if not path.is_file():
            raise FileNotFoundError(path)
    return sorted(set(result)), skipped


def validate_archive(path):
    with zipfile.ZipFile(path) as archive:
        names = [entry.filename for entry in archive.infolist()]
        if len(names) != len(set(names)) or any(not name.startswith(PREFIX) for name in names):
            raise ValueError('Duplicate or unexpected ZIP paths')
        for name in names:
            safe_relative(name[len(PREFIX):])
        manifest = json.loads(archive.read(PREFIX + MANIFEST))
        expected = {PREFIX + row['path'] for row in manifest['files']} | {PREFIX + MANIFEST}
        if set(names) != expected:
            raise ValueError('Archive contents differ from transfer manifest')
        for row in manifest['files']:
            safe_relative(row['path'])
            info = archive.getinfo(PREFIX + row['path'])
            if info.file_size != row['bytes']:
                raise ValueError('Wrong archive file size: ' + row['path'])
            with archive.open(info) as stream:
                if hashlib.file_digest(stream, 'sha256').hexdigest() != row['sha256']:
                    raise ValueError('Wrong archived SHA256: ' + row['path'])
    return manifest


def pack(args):
    output = args.output.resolve()
    allowed = ROOT / 'handoff'
    if not output.is_relative_to(allowed) or output.suffix.lower() != '.zip':
        raise ValueError('Write transfer ZIP under this project\'s handoff/ directory')
    partial = output.with_suffix('.zip.partial')
    manifest_path = output.with_suffix('.manifest.json')
    report_path = output.with_suffix('.verification.json')
    sums = output.with_suffix('.sha256.txt')
    if any(path.exists() for path in (output, partial, manifest_path, report_path, sums)):
        raise FileExistsError('Use a new transfer name; existing output is never overwritten')
    output.parent.mkdir(parents=True, exist_ok=True)
    delivery, metadata = current_delivery(ROOT)
    files, skipped = inputs(ROOT)
    records = []
    manifest = dict(schema=1, created_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    product='掠影 Glimpse', version=metadata['application_version'],
                    delivery=delivery.relative_to(ROOT).as_posix(), exe_sha256=metadata['exe_sha256'],
                    working_tree_included=True, git_directory_included=(ROOT / '.git').is_dir(),
                    files=records, skipped_links=skipped,
                    exclusions=['handoff/', 'target/ except diagnostics/Release helpers', '__pycache__/',
                                'machine-level Rust/MSVC/Cargo caches, Codex sessions and AppData outside this project'],
                    private_transfer=True,
                    scope='Complete current working tree, dependencies, local test media, releases and retained evidence; '
                          'not a public source distribution or an offline build-tool installer')
    with zipfile.ZipFile(partial, 'x', zipfile.ZIP_DEFLATED, compresslevel=4, allowZip64=True) as archive:
        for index, path in enumerate(files):
            before = path.stat()
            relative = path.relative_to(ROOT).as_posix()
            safe_relative(relative)
            digest = hashlib.sha256()
            with path.open('rb') as source, archive.open(PREFIX + relative, 'w', force_zip64=True) as target:
                while chunk := source.read(1024 * 1024):
                    digest.update(chunk)
                    target.write(chunk)
            after = path.stat()
            if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
                raise RuntimeError('Input changed during handoff; partial archive retained: ' + relative)
            records.append(dict(path=relative, bytes=after.st_size, sha256=digest.hexdigest()))
            if index and index % 500 == 0:
                print(f'Packed {index}/{len(files)} files', flush=True)
        archive.writestr(PREFIX + MANIFEST, json.dumps(manifest, ensure_ascii=False, indent=2))
    print('Verifying every archived file...', flush=True)
    checked = validate_archive(partial)
    if checked != manifest:
        raise ValueError('Manifest mismatch')
    # Verify against the live snapshot too, to catch changes after an earlier file was packed.
    for row in records:
        if sha(ROOT / row['path']) != row['sha256']:
            raise RuntimeError('Workspace changed while packaging: ' + row['path'])
    partial.replace(output)
    write(manifest_path, manifest)
    digest = sha(output)
    sums.write_text(digest + '  ' + output.name + '\n', encoding='utf8')
    result = dict(archive=str(output), sha256=digest, bytes=output.stat().st_size,
                  unpacked_bytes=sum(row['bytes'] for row in records), files=len(records),
                  version=manifest['version'], archive_verified=True, live_files_verified=True,
                  target_computer_verified=False, root_folder='Glimpse', exe_sha256=manifest['exe_sha256'])
    write(report_path, result)
    print(json.dumps(result, ensure_ascii=False, indent=2))


def verify(args):
    root = args.root.resolve(strict=True)
    manifest_path = (args.manifest or root / MANIFEST).resolve(strict=True)
    manifest = read(manifest_path)
    failures = []
    for row in manifest['files']:
        relative = safe_relative(row['path'])
        path = root.joinpath(*relative.parts)
        # Refuse files redirected through junctions/symlinks on the receiving computer.
        if any(p.is_symlink() or p.is_junction() for p in [path] + list(path.parents) if p != root.parent):
            failures.append(dict(path=row['path'], reason='link in file path'))
        elif not path.is_file() or path.stat().st_size != row['bytes'] or sha(path) != row['sha256']:
            failures.append(dict(path=row['path'], reason='missing, wrong size or wrong SHA256'))
    result = dict(checks_pass=not failures, version=manifest['version'], files_checked=len(manifest['files']),
                  failures=failures, exe_sha256=manifest['exe_sha256'],
                  scope='Copied file contents only; not runtime, build or target OS acceptance')
    if args.report:
        if args.report.exists():
            raise FileExistsError(args.report)
        write(args.report, result)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    if failures:
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest='command', required=True)
    packing = actions.add_parser('pack')
    packing.add_argument('--output', type=Path, required=True)
    packing.set_defaults(func=pack)
    verification = actions.add_parser('verify')
    verification.add_argument('--root', type=Path, default=ROOT)
    verification.add_argument('--manifest', type=Path)
    verification.add_argument('--report', type=Path)
    verification.set_defaults(func=verify)
    archive = actions.add_parser('verify-archive')
    archive.add_argument('archive', type=Path)
    archive.set_defaults(func=lambda args: print(json.dumps(dict(archive_verified=True,
                        files=len(validate_archive(args.archive)['files'])), ensure_ascii=False)))
    args = parser.parse_args()
    args.func(args)


if __name__ == '__main__':
    main()
