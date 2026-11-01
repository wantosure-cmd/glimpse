"""Bind observations to a delivery; preserve measured failures and proxy limits."""
import argparse, hashlib, json
from pathlib import Path
from package_local import production_inputs
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser()
p.add_argument('--delivery',type=Path,required=True)
p.add_argument('--prefix',required=True)
a=p.parse_args()
assert a.prefix.replace('-','').replace('_','').isalnum()
delivery=a.delivery.resolve(strict=True)
assert delivery.is_relative_to(ROOT/'dist')
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def read(name):return json.loads((ROOT/'artifacts'/name).read_text(encoding='utf-8-sig'))
prefix=a.prefix
manifest=json.loads((delivery/'delivery.json').read_text(encoding='utf-8'))
digest=sha(delivery/'media-browser-portable/media-browser.exe')
assert digest==manifest['exe_sha256']
assert production_inputs()==manifest['production_source_sha256']
formats=read(prefix+'-format24.json')
expected=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf-8'))
assert formats['exe_sha256']==digest and formats['checks_pass'] and formats['finished']
assert len(formats['rows'])==24 and {r['name'] for r in formats['rows']}=={r['name'] for r in expected['cases']}
assert all(r['passed'] and sha(ROOT/r['source'])==r['source_sha256'] for r in formats['rows'])
startup=read(prefix+'-startup20.json')
assert startup['exe_sha256'].lower()==digest and startup['runs']==len(startup['rows'])==20
assert all(r['exit_code']==0 and r['input_idle'] for r in startup['rows'])
scroll=[]
for label in (prefix+'-cold4k',prefix+'-revisit4k'):
    summary=read('p2-scroll-'+label+'-summary.json')
    raw=read('p2-scroll-'+label+'.json');process=read('p2-scroll-'+label+'-process.json')
    assert summary['exe_sha256'].lower()==digest and process['exe_sha256'].lower()==digest
    assert process['exit_code']==0 and raw['error'] is None and raw['media']==5000
    assert raw['requested_seconds']>=120 and summary['seconds']>=120 and len(raw['samples'])>=120
    assert summary['max_scroll_points']>1000
    scroll.append(summary)
assert read('p2-scroll-'+prefix+'-cold4k-process.json')['data_dir']==read('p2-scroll-'+prefix+'-revisit4k-process.json')['data_dir']
before=read(prefix+'-perf-fixture-before.json');after=read(prefix+'-perf-fixture-after.json')
assert before['checked']==after['checked']==5000 and before['all_match'] and after['all_match']
assert before['files']==after['files'] and before['source_sha256']==after['source_sha256']
idle=read(prefix+'-idle30.json')
assert idle['exe_sha256'].lower()==digest and idle['exit_code']==0 and idle['elapsed_seconds']>=30
scan=read(prefix+'-scan.json')
assert scan['media_count']==5000 and not scan['errors'] and scan['integrity_check']=='ok'
machine=read(prefix+'-performance-machine.json')
assert machine['logical_processors']==idle['logical_processors'] and machine['installed_memory_bytes']>0
gates=dict(callback_p95_le20ms=all(s['callback_interval_target_20ms_met'] for s in scroll),
    working_set_le600mib=all(s['memory_target_600mib_met'] for s in scroll),
    texture_accounting_le256mib=all(s['peak_texture_mib']<=256.01 for s in scroll),
    all_samples_4k_150percent=all(s['physical_viewport_sizes']==[[3840,2160]] and s['scale_factors']==[1.5] for s in scroll),
    preview_errors_zero=all(s['max_preview_failures']==0 for s in scroll),
    idle_total_cpu_lt1percent=idle['normalized_total_cpu_percent']<1,
    enumeration_le3seconds=scan['enumeration_ms']<=3000)
result=dict(evidence_valid=True,full_acceptance_pass=False,exe_sha256=digest,
    portable_zip_sha256=sha(delivery/'media-browser-portable.zip'),format_cases_passed=24,
    production_inputs_match=True,fixture_hashes_unchanged=True,measured_proxy_targets=gates,
    all_measured_proxy_targets_met=all(gates.values()),
    startup={k:v for k,v in startup.items() if k!='rows'},scroll=scroll,
    startup_timing_target_verified=False,
    startup_proxy_initial_p95_above_500ms=startup['internal_first_ui_p95_ms']>500,
    idle_cpu_percent=idle['normalized_total_cpu_percent'],
    idle_peak_working_set_mib=max(r['working_set_bytes'] for r in idle['samples'])/1024**2,
    scan={k:scan[k] for k in ('enumeration_ms','pipeline_ms','first_media_ms','media_count','image_count','video_count')},
    machine_evidence='artifacts/'+prefix+'-performance-machine.json',
    machine_evidence_sha256=sha(ROOT/'artifacts'/(prefix+'-performance-machine.json')),
    scope='Delivered Release, 24 specific codec samples, 5000 generated repeated media, warm OS cache startup proxies and callback intervals. Not physical presentation/input, OS cold boot, first-screen completion latency, seek P95, unique photography library, proven SSD model, Win11 or clean GPU system acceptance.')
followup_path=ROOT/'artifacts'/(prefix+'-startup20-recheck.json')
if followup_path.is_file():
    followup=read(followup_path.name)
    assert followup['exe_sha256'].lower()==digest and followup['runs']==len(followup['rows'])==20
    assert all(r['exit_code']==0 and r['input_idle'] for r in followup['rows'])
    result['startup_followup']={k:v for k,v in followup.items() if k!='rows'}
    result['startup_followup_note']='Additional observations after the full scroll/idle run; original outliers are retained, not replaced. Cause of intermittent delays not established; physical startup target remains unverified.'
(ROOT/'artifacts'/(prefix+'-performance-summary.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k not in ('startup','scroll')},ensure_ascii=False,indent=2))
