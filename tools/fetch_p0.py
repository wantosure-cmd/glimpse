"""Restore the exact pinned official libmpv archive, never a newer rolling build."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time
import urllib.error
import urllib.request
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def fetch(url, name=None, expected_sha256=None):
    downloads = ROOT / 'vendor/downloads'
    downloads.mkdir(parents=True, exist_ok=True)
    path = downloads / (name or url.rsplit('/', 1)[-1])
    if not path.exists():
        request = urllib.request.Request(url, headers={'User-Agent': 'glimpse-pinned-dependencies'})
        for attempt in range(3):
            partial = path.with_name(path.name + '.' + uuid.uuid4().hex + '.part')
            print('Downloading', url, 'attempt', attempt + 1, flush=True)
            try:
                with urllib.request.urlopen(request, timeout=90) as source, partial.open('xb') as output:
                    length = source.headers.get('Content-Length')
                    shutil.copyfileobj(source, output)
                if length is not None and partial.stat().st_size != int(length):
                    raise OSError(f'Truncated download: {partial.stat().st_size} of {length} bytes')
                if expected_sha256 and digest(partial) != expected_sha256.lower():
                    raise RuntimeError('Download hash mismatch; keeping partial evidence and refusing to stage it')
                partial.replace(path)
                break
            except (OSError, urllib.error.URLError) as error:
                if isinstance(error, urllib.error.HTTPError) or attempt == 2:
                    raise
                print('Incomplete transfer; keeping partial evidence:', error, flush=True)
                time.sleep(attempt + 1)
    if expected_sha256 and digest(path) != expected_sha256.lower():
        raise RuntimeError('Cached archive hash mismatch; refusing to overwrite or change the pin')
    return path


def fetch_mpv_archive():
    entries = json.loads((ROOT / 'vendor/p0-downloads.json').read_text(encoding='utf8'))
    entry = next(row for row in entries if row['component'] == 'libmpv official GPL build')
    try:
        path = fetch(entry['source'], entry['filename'], entry['sha256'])
    except urllib.error.URLError as error:
        if (isinstance(error, urllib.error.HTTPError) and error.code not in (404, 410)) or not entry.get('official_artifact'):
            raise
        artifact = entry['official_artifact']
        path = ROOT / 'vendor/downloads' / entry['filename']
        partial = path.with_name(path.name + '.' + uuid.uuid4().hex + '.part')
        print('Release asset unavailable; restoring the SAME official Actions archive:',
              artifact['repository'], artifact['artifact_id'], flush=True)
        # gh manages authentication and signed cross-host redirects. Stream ZIP
        # bytes directly to disk; never print credentials or transcode the body.
        with partial.open('xb') as output:
            subprocess.run(['gh', 'api', f"repos/{artifact['repository']}/actions/artifacts/{artifact['artifact_id']}/zip"],
                           stdout=output, check=True)
        if digest(partial) != entry['sha256'].lower() or partial.stat().st_size != entry['bytes']:
            raise RuntimeError('Official artifact differs from pinned identity; no runtime staged')
        partial.replace(path)
    if digest(path) != entry['sha256'].lower() or path.stat().st_size != entry['bytes']:
        raise RuntimeError('libmpv archive differs from pinned identity')
    return path


def main():
    archive = fetch_mpv_archive()
    destination = ROOT / 'vendor/extracted/mpv'
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as package:
        for member in package.infolist():
            target = (destination / member.filename).resolve()
            if not target.is_relative_to(destination.resolve()) or (member.external_attr >> 16) & 0o170000 == 0o120000:
                raise RuntimeError('Archive path/symlink refused')
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            data = package.read(member)
            if target.exists():
                if digest(target) != hashlib.sha256(data).hexdigest():
                    raise RuntimeError('Refuse to overwrite different extracted dependency: ' + str(target))
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open('xb') as output:
                    output.write(data)
    print('Verified and extracted original pinned archive:', archive.name)


if __name__ == '__main__':
    main()
