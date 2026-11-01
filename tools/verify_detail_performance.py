"""Bind current Release measurements; keep callback/proxy and presentation distinct."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent

def read(name):return json.loads((ROOT/'artifacts'/name).read_text(encoding='utf-8-sig'))
exe_hash=hashlib.sha256((ROOT/'dist/browser-preview/media-browser.exe').read_bytes()).hexdigest()
assert exe_hash==read('detail-compact-delivery-verification.json')['exe_sha256']
startup=read('detail-startup.json');assert startup['exe_sha256'].lower()==exe_hash and startup['runs']==20 and len(startup['rows'])==20
assert all(r['exit_code']==0 and r['input_idle'] for r in startup['rows'])
scroll=[]
for label in ['detail-cold4k','detail-revisit4k']:
    s=read('p2-scroll-'+label+'-summary.json');raw=read('p2-scroll-'+label+'.json');p=read('p2-scroll-'+label+'-process.json')
    assert s['exe_sha256'].lower()==exe_hash and s['media']==5000 and s['seconds']>=120
    assert p['exit_code']==0 and raw['error'] is None and len(raw['samples'])>120
    assert s['four_k_viewport'] and s['physical_viewport_sizes']==[[3840,2160]],s['physical_viewport_sizes']
    assert s['max_preview_failures']==0 and s['max_scroll_points']>1000
    assert s['callback_interval_target_20ms_met'] and s['memory_target_600mib_met'],s
    scroll.append(s)
assert read('p2-scroll-detail-cold4k-process.json')['data_dir']==read('p2-scroll-detail-revisit4k-process.json')['data_dir']
before=read('detail-perf-fixture-before.json');after=read('detail-perf-fixture-after.json')
assert before['all_match'] and after['all_match'] and before['files']==after['files'] and before['checked']==5000
idle=read('detail-browser-idle.json');assert idle['exe_sha256'].lower()==exe_hash and idle['exit_code']==0 and idle['elapsed_seconds']>=30
assert idle['normalized_total_cpu_percent']<1,idle['normalized_total_cpu_percent']
result={'checks_pass':True,'exe_sha256':exe_hash,'scope':'Current Release startup proxies, 5000 generated duplicate media at 4K, cold application cache/revisit rolling and 30s idle CPU; not visible first frame, presented FPS, unique photography library, OS cold boot or input-to-seek latency certification','startup':{k:v for k,v in startup.items() if k!='rows'},'scroll':scroll,'idle_cpu_percent':idle['normalized_total_cpu_percent'],'fixture_hashes_unchanged':True,'machine_evidence':'artifacts/detail-performance-machine.json','unverified':['System-reboot cold startup','Visible and responsive first frame','First-media display latency','Display presentation FPS','Input-to-visible-seek-frame P95','Win11 and integrated-GPU hardware']}
(ROOT/'artifacts/detail-performance-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'checks_pass':True,'exe_sha256':exe_hash,'startup_first_ui_p95_ms':startup['internal_first_ui_p95_ms'],'scroll_p95_ms':[s['callback_interval_p95_ms'] for s in scroll],'working_set_peak_mib':[s['peak_working_set_mib'] for s in scroll],'idle_cpu_percent':result['idle_cpu_percent']},ensure_ascii=False,indent=2))
