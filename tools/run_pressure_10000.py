"""Run current release against a NEW generated 10k fixture; never private media."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent

def sha(p):
    h = hashlib.sha256()
    with p.open('rb') as f:
        while b := f.read(1024 * 1024):
            h.update(b)
    return h.hexdigest()

def main():
    token = uuid.uuid4().hex
    fixture = ROOT / 'target' / ('pressure-10000-' + token)
    fixture.mkdir()
    (fixture / '.media-browser-fixture').write_text('Dedicated generated duplicate media pressure fixture', encoding='ascii')
    sources = {'.jpg': ROOT/'test-media/ui-demo/封面.jpg', '.png': ROOT/'artifacts/heic-10bit.png', '.heic': ROOT/'test-media/upstream-example.heic', '.mp4': ROOT/'test-media/test-h264-aac.mp4'}
    hashes = {e: sha(p) for e, p in sources.items()}
    records = []
    for group in range(1, 201):
        folder = fixture / ('图集 ' + str(group))
        folder.mkdir()
        for n in range(1, 51):
            ext = '.jpg' if n <= 45 else '.png' if n <= 48 else '.heic' if n == 49 else '.mp4' if group % 10 == 0 else '.jpg'
            dest = folder / (str(n) + ext)
            shutil.copyfile(sources[ext], dest)
            assert sha(dest) == hashes[ext]
            records.append(dict(file=dest.relative_to(fixture).as_posix(), sha256=hashes[ext]))
    label = 'pressure10000-' + token[:8]
    out = ROOT/'artifacts'/('pressure10000-' + token + '.json')
    exe = ROOT/'dist/browser-preview/media-browser.exe'
    result = dict(exe_sha256=sha(exe), fixture=str(fixture), files=records, label=label, samples=10000, scope='Generated duplicate media, not distinct high-resolution photographs; UI callback timing, not display presentation')
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print('LIVE_EVIDENCE ' + str(out), flush=True)
    scan = subprocess.run([str(exe), '--scan-report', str(fixture), '--db', str(ROOT/'target'/('pressure-scan-'+token+'.sqlite3'))], cwd=ROOT, capture_output=True)
    (ROOT/'artifacts'/('pressure10000-'+token+'-scan.log')).write_bytes(scan.stdout+scan.stderr)
    assert scan.returncode == 0, scan.stderr
    result['scan_report'] = json.loads(scan.stdout)
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    cmd = ['pwsh','-NoProfile','-File',str(ROOT/'tools/measure_browser_scroll.ps1'),'-Label',label,'-DataDir',str(ROOT/'target'/('pressure-cache-'+token)),'-Seconds','120','-Fixture',str(fixture)]
    with (ROOT/'artifacts'/('pressure10000-'+token+'-run.log')).open('wb') as log:
        run = subprocess.run(cmd, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    assert run.returncode == 0, 'Inspect live process and run log before any restart'
    summarized = subprocess.run(['python','-X','utf8',str(ROOT/'tools/summarize_browser_scroll.py'),label,'10000'],cwd=ROOT,capture_output=True)
    assert summarized.returncode == 0, summarized.stderr
    result['summary'] = json.loads(summarized.stdout)
    assert sha(exe)==result['exe_sha256']
    for e in records:
        assert sha(fixture/e['file'])==e['sha256'], e['file']
    result['source_hashes_unchanged'] = True
    result['finished'] = True
    result['four_k_verified'] = result['summary']['four_k_viewport']
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k not in ('files','scan_report')}, ensure_ascii=False, indent=2))

if __name__ == '__main__':
    main()
