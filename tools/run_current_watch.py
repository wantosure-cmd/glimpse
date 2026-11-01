"""Native filesystem/UI regression bound to an explicit delivered executable.

Only marked generated copies are mutated. Existing app instances and old reports
are left alone; a live process is never restarted on an observation timeout.
"""
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
import uuid
from pathlib import Path

from package_local import production_inputs

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    exe = args.executable.resolve(strict=True)
    before = production_inputs()
    initial = sha(exe)
    task = ROOT / 'target' / ('watch-ui-' + uuid.uuid4().hex)
    media = task / 'media'
    media.mkdir(parents=True)
    (task / '.watch-smoke-owned').write_bytes(b'generated watch UI fixture')
    work = task / 'work'
    work.mkdir()
    evidence = work / 'artifacts'
    evidence.mkdir()
    original = ROOT / 'test-media/ui-demo/封面.jpg'
    video = ROOT / 'test-media/test-h264-aac.mp4'
    shutil.copyfile(original, task / 'original.jpg')
    for number in range(40):
        shutil.copyfile(original, media / f'{number:03d}.jpg')
    originals = [original, task / 'original.jpg', video]

    def hashes():
        return [dict(Path=str(p), Hash=sha(p).upper()) for p in originals]

    def save(name, value):
        (evidence / name).write_text(json.dumps(value, indent=2), encoding='utf8')

    sources = hashes()
    save('p4-watch-ui-originals-before.json', sources)
    started = time.time()
    with (evidence / 'stdout.log').open('wb') as stdout, (evidence / 'stderr.log').open('wb') as stderr:
        app = subprocess.Popen([str(exe), '--watch-smoke', '--instance-scope', task.name,
                                '--open', str(media), '--data-dir', str(task / 'data'),
                                '--quit-after', '150000'], cwd=work, stdout=stdout, stderr=stderr)
        save('running.json', dict(pid=app.pid, started_utc=started, exe_sha256=initial))
        try:
            code = app.wait(timeout=165)
        except subprocess.TimeoutExpired:
            save('observation-timeout.json', dict(pid=app.pid, started_utc=started))
            raise
    save('p4-watch-ui-run.json', dict(exit_code=code, started_utc=started, ended_utc=time.time(),
                                     exe_path=str(exe), exe_sha256=initial.upper(), fixture=str(task)))
    save('p4-watch-ui-originals-after.json', hashes())
    raw = evidence / 'p4-watch-ui.json'
    assert raw.is_file() and raw.stat().st_mtime >= started
    with (evidence / 'verifier.log').open('wb') as log:
        check = subprocess.run([sys.executable, '-X', 'utf8', str(ROOT / 'tools/verify_watch_smoke.py'),
                                '--evidence-dir', str(evidence)], stdout=log, stderr=subprocess.STDOUT)
    assert check.returncode == 0, str(evidence / 'verifier.log')
    result = json.loads((evidence / 'p4-watch-ui-verification.json').read_text(encoding='utf8'))
    result.update(evidence=str(evidence), production_inputs_unchanged=before == production_inputs(),
                  production_source_sha256=before, original_sources_unchanged=sources == hashes())
    assert result['checks_pass'] and result['production_inputs_unchanged']
    assert result['original_sources_unchanged'] and initial == sha(exe)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')
    print(json.dumps({k: v for k, v in result.items() if k != 'production_source_sha256'}, indent=2))


if __name__ == '__main__':
    main()
