"""Bind measured observations to one delivered EXE without claiming full acceptance."""
import argparse,hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('directory',type=Path);p.add_argument('--prefix',required=True);a=p.parse_args()
assert a.prefix.replace('-','').replace('_','').isalnum()
directory=a.directory.resolve(strict=True);assert directory.is_relative_to(ROOT/'dist')
def read(name):return json.loads((ROOT/'artifacts'/name).read_text(encoding='utf-8-sig'))
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
exe=directory/'media-browser-portable/media-browser.exe';digest=sha(exe)
assert digest==json.loads((directory/'delivery.json').read_text(encoding='utf-8'))['exe_sha256']
prefix=a.prefix
formats=read(prefix+'-format-matrix.json');heic=read(prefix+'-user-heic-ui.json');reference=read(prefix+'-user-heic-verification.json')
for report,count in [(formats,24),(heic,2)]:
    assert report['checks_pass'] and report['finished'] and len(report['rows'])==count
    assert report['exe_sha256'].lower()==digest
    assert all(row['passed'] and sha(ROOT/row['source'])==row['source_sha256'] for row in report['rows'])
assert reference['checks_pass'] and reference['exe_sha256'].lower()==digest
startup=read(prefix+'-startup.json');assert startup['exe_sha256'].lower()==digest and startup['runs']==20
assert len(startup['rows'])==20 and all(r['exit_code']==0 and r['input_idle'] for r in startup['rows'])
scroll=[]
for suffix in ('cold4k','revisit4k'):
    label=prefix+'-final-'+suffix
    summary=read('p2-scroll-'+label+'-summary.json');raw=read('p2-scroll-'+label+'.json');process=read('p2-scroll-'+label+'-process.json')
    assert summary['exe_sha256'].lower()==digest and summary['media']==5000 and summary['seconds']>=120
    assert process['exit_code']==0 and raw['error'] is None and len(raw['samples'])>120
    assert summary['max_scroll_points']>1000
    scroll.append(summary)
assert read('p2-scroll-'+prefix+'-final-cold4k-process.json')['data_dir']==read('p2-scroll-'+prefix+'-final-revisit4k-process.json')['data_dir']
before=read(prefix+'-perf-fixture-before.json');after=read(prefix+'-perf-fixture-after.json')
assert before['checked']==after['checked']==5000 and before['all_match'] and after['all_match'] and before['files']==after['files']
idle=read(prefix+'-browser-idle.json');assert idle['exe_sha256'].lower()==digest and idle['exit_code']==0 and idle['elapsed_seconds']>=30
scan=read(prefix+'-scan.json');assert scan['media_count']==5000 and not scan['errors'] and scan['integrity_check']=='ok'
machine_path=ROOT/'artifacts'/f'{prefix}-performance-machine.json';machine=read(machine_path.name)
assert machine['cpu']['NumberOfLogicalProcessors']==idle['logical_processors']
assert machine['memory']['TotalPhysicalMemory']>0 and machine['storage']['FriendlyName'] and machine['video']
gates=dict(callback_p95_le20ms=all(s['callback_interval_target_20ms_met'] for s in scroll),working_set_le600mib=all(s['memory_target_600mib_met'] for s in scroll),texture_accounting_le256mib=all(s['peak_texture_mib']<=256.01 for s in scroll),all_samples_4k_150percent=all(s['physical_viewport_sizes']==[[3840,2160]] and s['scale_factors']==[1.5] for s in scroll),preview_errors_zero=all(s['max_preview_failures']==0 for s in scroll),idle_total_cpu_lt1percent=idle['normalized_total_cpu_percent']<1,enumeration_le3seconds=scan['enumeration_ms']<=3000)
result=dict(evidence_valid=True,full_acceptance_pass=False,exe_sha256=digest,delivery=str(directory),format_cases_passed=26,fixture_hashes_unchanged=True,measured_proxy_targets=gates,all_measured_proxy_targets_met=all(gates.values()),startup={k:v for k,v in startup.items() if k!='rows'},scroll=scroll,idle_cpu_percent=idle['normalized_total_cpu_percent'],scan={k:scan[k] for k in ('enumeration_ms','pipeline_ms','first_media_ms','media_count','image_count','video_count')},machine_evidence='artifacts/'+prefix+'-performance-machine.json',scope='Delivered Release, fixed real codec files and HEICs, startup proxies, 5000 generated duplicate files and sampled memory/CPU; not physical display/input, OS cold boot, unique photography library, Win11 or clean GPU machine certification')
result['machine_evidence_sha256']=sha(machine_path)
output=ROOT/'artifacts'/f'{prefix}-release-recheck.json';output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k not in ('startup','scroll')},ensure_ascii=False,indent=2))
