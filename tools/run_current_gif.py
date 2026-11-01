"""Current delivered executable: real GIF frames, timing and bounded textures."""
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
    initial = sha(exe)
    before = production_inputs()
    task = ROOT / 'target' / ('gif-owned-' + uuid.uuid4().hex)
    media = task / 'media'
    media.mkdir(parents=True)
    work = task / 'work'
    evidence = work / 'artifacts'
    evidence.mkdir(parents=True)
    originals = [ROOT / 'test-media/gif-demo' / n for n in ['循环.gif', '有限.gif', '单次.gif']]
    for src in originals:
        shutil.copyfile(src, media / src.name)
    sources = originals + list(media.glob('*.gif'))

    def hashes():
        return [dict(Path=str(p), Hash=sha(p).upper()) for p in sources]

    def save(name, value):
        (evidence / name).write_text(json.dumps(value, indent=2), encoding='utf8')

    baseline = hashes()
    save('p3-gif-hashes-before.json', baseline)
    started = time.time()
    with (evidence / 'stdout.log').open('wb') as stdout, (evidence / 'stderr.log').open('wb') as stderr:
        app = subprocess.Popen([str(exe), '--gif-smoke', '--instance-scope', task.name,
                                '--open', str(media), '--data-dir', str(task / 'data'),
                                '--quit-after', '45000'], cwd=work, stdout=stdout, stderr=stderr)
        save('running.json', dict(pid=app.pid, started_utc=started, exe_sha256=initial))
        code = app.wait(timeout=60)
    save('p3-gif-hashes-after.json', hashes())
    save('p3-gif-run.json', dict(exit_code=code, started_utc=started, ended_utc=time.time(),
                                exe_sha256=initial.upper(), data_dir=str(task / 'data')))
    report = evidence / 'p3-gif-smoke.json'
    assert report.is_file() and report.stat().st_mtime >= started
    with (evidence / 'verifier.log').open('wb') as log:
        verified = subprocess.run([sys.executable, '-X', 'utf8', str(ROOT / 'tools/verify_gif_smoke.py'),
                                   '--evidence-dir', str(evidence), '--exe', str(exe)],
                                  stdout=log, stderr=subprocess.STDOUT)
    assert verified.returncode == 0, str(evidence / 'verifier.log')
    result = json.loads((evidence / 'p3-gif-verification.json').read_text(encoding='utf8'))
    # The legacy verifier names the original three GIFs; this wrapper additionally
    # checks the three exact copies supplied to the application.
    assert baseline == hashes() and initial == sha(exe) and before == production_inputs()
    result.update(evidence=str(evidence), copied_and_original_sources_unchanged=True,
                  production_inputs_unchanged=True, production_source_sha256=before)
    args.output.write_text(json.dumps(result, indent=2), encoding='utf8')
    print(json.dumps({k: v for k, v in result.items()
                      if k not in ('production_source_sha256', 'timing_intervals')}, indent=2))


if __name__ == '__main__':
    main()
