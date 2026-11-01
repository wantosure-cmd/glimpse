"""Validate pressure evidence scope and publish a compact report without hiding limits."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('evidence', type=Path)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    d = json.loads(args.evidence.read_text(encoding='utf-8'))
    assert d['finished'] and d['source_hashes_unchanged']
    assert len(d['files']) == 10000 and len({e['file'] for e in d['files']}) == 10000
    counts = {}
    for e in d['files']:
        ext = Path(e['file']).suffix
        counts[ext] = counts.get(ext, 0) + 1
    assert counts == {'.jpg':9180,'.png':600,'.heic':200,'.mp4':20}, counts
    scan = d['scan_report']
    assert scan['media_count'] == 10000 and scan['image_count'] == 9980 and scan['video_count'] == 20
    assert scan['groups'] == 200 and scan['integrity_check'] == 'ok'
    assert not scan['errors'], scan['errors']
    s = d['summary']
    assert s['media']==10000 and s['seconds']>=120 and s['four_k_viewport']
    assert s['exe_sha256'].lower()==d['exe_sha256'].lower()
    result = dict(checks_pass=True, evidence=str(args.evidence), exe_sha256=d['exe_sha256'], fixture=d['fixture'], samples=counts,
                  scan={k:v for k,v in scan.items() if k not in ('directories','sample_ids')}, scrolling=s,
                  source_hashes_unchanged=True,
                  performance_targets_met=s['memory_target_600mib_met'] and s['callback_interval_target_20ms_met'] and s['max_preview_failures']==0,
                  scope='10k files indexed, 120-second programmatic 4K scroll over generated duplicates; no claim of visiting every card, 10k distinct photographs, physical input, full GPU accounting or display presentation')
    args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps(result,ensure_ascii=False,indent=2))

if __name__=='__main__':
    main()
