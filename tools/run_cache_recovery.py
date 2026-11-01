"""Current release: restart after damage to generated caches in an owned fixture.

No desktop input or user data/cache is touched. Pixel observations use native
diagnostic screenshots, not successful process exit alone.
"""
import argparse
import hashlib
import json
import shutil
import subprocess
import time
import uuid
from pathlib import Path

from run_format_matrix import screenshot

ROOT = Path(__file__).resolve().parent.parent
DELIVERY = ROOT / 'dist/0.1.0-local-test-20261021-glimpse'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--delivery', type=Path, default=DELIVERY)
    parser.add_argument('--output', type=Path, default=ROOT/'artifacts/glimpse-cache-recovery.json')
    args = parser.parse_args()
    delivery_root = args.delivery.resolve(strict=True)
    assert delivery_root.is_relative_to(ROOT/'dist')
    delivery = json.loads((delivery_root / 'delivery.json').read_text(encoding='utf8'))
    exe = delivery_root / 'media-browser-portable/media-browser.exe'
    assert sha(exe) == delivery['exe_sha256']
    for relative, digest in delivery['production_source_sha256'].items():
        assert sha(ROOT / relative) == digest, relative
    manifest = json.loads((ROOT / 'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf8'))
    task = ROOT / 'target' / ('cache-recovery-' + uuid.uuid4().hex)
    task.mkdir()
    (task / '.cache-recovery-owned').write_text('Only generated caches may be damaged here', encoding='ascii')
    evidence = ROOT / 'artifacts' / task.name
    evidence.mkdir()
    result_path = args.output.resolve()
    result = dict(checks_pass=False, exe_sha256=sha(exe), production_inputs_match=True,
                  task=str(task), evidence=str(evidence), rows=[],
                  scope='Two dedicated copied PNG/MOV sources; fresh native processes, cache repair and rendered pixels; no physical input or system-cache test')

    def save():
        result_path.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf8')

    save()
    for name in ['png', 'h264-mov']:
        case = next(c for c in manifest['cases'] if c['name'] == name)
        source = (ROOT / case['path']).resolve(strict=True)
        assert source.is_relative_to(ROOT / 'test-media/format-matrix-v1')
        assert sha(source) == case['sha256']
        media = task / name / 'media'
        media.mkdir(parents=True)
        copied = media / source.name
        shutil.copy2(source, copied)
        data = task / name / 'data'
        cache = data / 'thumbnails'
        original_cache = None
        baseline_pixels = None
        for phase in ['baseline', 'warm_restart', 'corrupt_png', 'truncated_metadata', 'missing_png']:
            row = dict(case=name, phase=phase, passed=False, mutated=[])
            result['rows'].append(row)
            if phase in ['corrupt_png', 'truncated_metadata', 'missing_png']:
                assert (task / '.cache-recovery-owned').is_file()
                assert cache.resolve().is_relative_to(task.resolve()) and not cache.is_symlink()
                suffix = '*.json' if phase == 'truncated_metadata' else '*.png'
                for p in sorted(cache.glob(suffix)):
                    assert p.is_file() and not p.is_symlink()
                    row['mutated'].append(p.name)
                    if phase == 'missing_png':
                        p.unlink()
                    else:
                        p.write_bytes(b'{"width":' if phase == 'truncated_metadata' else b'generated damaged cache')
                assert row['mutated'], 'No actual generated cache was mutated'
            dest = evidence / (name + '-' + phase)
            dest.mkdir()
            started = time.time()
            with (dest / 'stdout.log').open('wb') as stdout, (dest / 'stderr.log').open('wb') as stderr:
                process = subprocess.Popen([str(exe), '--format-smoke', str(dest), '--open', str(media),
                                            '--data-dir', str(data), '--quit-after', '40000'],
                                           cwd=task, stdout=stdout, stderr=stderr)
                row['pid'] = process.pid
                save()
                row['exit_code'] = process.wait(timeout=60)
            assert row['exit_code'] == 0
            report = json.loads((dest / 'report.json').read_text(encoding='utf8'))
            assert report.get('error') is None
            phases = {r['phase']: r for r in report['rows']}
            assert all(not r['failures'] for r in report['rows'])
            listed, detail = phases['list_thumbnail'], phases['detail_fit']
            assert listed['textures'] and phases['returned_list']['textures']
            assert (listed['record']['width'], listed['record']['height']) == (case['expected']['width'], case['expected']['height'])
            pixels = screenshot(dest / 'list.png', listed)
            detail_pixels = screenshot(dest / 'detail.png', detail)
            if baseline_pixels is None:
                baseline_pixels = pixels
            distances = [abs(a - b) for pa, pb in zip(pixels['samples'], baseline_pixels['samples']) for a, b in zip(pa, pb)]
            assert max(distances) <= 3, 'Recovered poster differs from original rendered poster'
            current_cache = {p.name: sha(p) for p in cache.iterdir() if p.suffix in ['.png', '.json']}
            assert current_cache
            if original_cache is None:
                original_cache = current_cache
            # All damaged files must actually be replaced with the original bytes;
            # simply bypassing a damaged entry and drawing another texture is insufficient.
            assert current_cache == original_cache, (phase, current_cache.keys(), original_cache.keys())
            assert not list(cache.glob('*.tmp'))
            assert sha(source) == sha(copied) == case['sha256']
            row.update(passed=True, elapsed_seconds=time.time()-started, cache_files=len(current_cache),
                       cache_bytes_restored=True, source_unchanged=True, screenshot=pixels,
                       detail_screenshot=detail_pixels, maximum_poster_channel_difference=max(distances))
            save()
            print('PASS ' + name + ' ' + phase, flush=True)
    result['checks_pass'] = True
    save()
    print(str(result_path))


if __name__ == '__main__':
    main()
