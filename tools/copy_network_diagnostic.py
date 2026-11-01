"""Copy one explicitly authorized source to a private local diagnostic fixture."""
from pathlib import Path
import argparse
import datetime
import hashlib
import json
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    nonce = uuid.uuid4().hex
    folder = ROOT / 'target' / ('private-network-source-' + nonce)
    folder.mkdir()
    (folder / '.owned-diagnostic').write_text('Private read-only source copy; exclude from delivery')
    target = folder / source.name
    output = ROOT / 'artifacts' / ('network-source-copy-' + nonce + '.json')
    before = source.stat()
    report = dict(source=str(source), target=str(target), source_bytes=before.st_size,
                  source_mtime_ns=before.st_mtime_ns, copied_bytes=0, finished=False,
                  chunks=[], started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope='Private authorized local copy. Source opened rb only. No source mutation. Full copy hash is of bytes received; post-copy source guard is size/mtime plus sampled ranges.')

    def save():
        output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf8')

    def samples():
        rows = []
        with source.open('rb') as stream:
            for offset in sorted({0, 1048576, before.st_size // 2, max(0, before.st_size - 4096)}):
                stream.seek(offset)
                data = stream.read(min(4096, before.st_size - offset))
                rows.append(dict(offset=offset, bytes=len(data), sha256=hashlib.sha256(data).hexdigest()))
        return rows

    began = time.monotonic()
    digest = hashlib.sha256()
    save()
    print(str(output), flush=True)
    try:
        report['samples_before'] = samples()
        with source.open('rb') as stream, target.open('xb') as local:
            while True:
                start = time.monotonic()
                data = stream.read(1048576)
                read_ms = (time.monotonic() - start) * 1000
                if not data:
                    break
                local.write(data)
                digest.update(data)
                report['copied_bytes'] += len(data)
                report['chunks'].append(dict(bytes=len(data), read_ms=read_ms,
                                             elapsed_ms=(time.monotonic() - began) * 1000))
                save()
        after = source.stat()
        report['samples_after'] = samples()
        report['source_size_mtime_unchanged'] = (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
        report['source_sampled_ranges_unchanged'] = report['samples_before'] == report['samples_after']
        report['received_sha256'] = digest.hexdigest()
        with target.open('rb') as local:
            report['local_sha256'] = hashlib.file_digest(local, 'sha256').hexdigest()
        report['copy_verified'] = (report['copied_bytes'] == before.st_size
            and report['received_sha256'] == report['local_sha256']
            and report['source_size_mtime_unchanged'] and report['source_sampled_ranges_unchanged'])
        assert report['copy_verified'], report
    except Exception as error:
        report['error'] = repr(error)
        raise
    finally:
        report['elapsed_ms'] = (time.monotonic() - began) * 1000
        report['finished'] = True
        save()
    print(json.dumps({k: v for k, v in report.items() if k not in ('chunks', 'samples_before', 'samples_after')}, ensure_ascii=False), flush=True)


if __name__ == '__main__':
    main()
