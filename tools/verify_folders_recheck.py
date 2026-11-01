"""Current Release evidence binding. Proxy gates never imply full OS/display acceptance."""
from pathlib import Path
import hashlib,json,zipfile,argparse
from package_local import production_inputs
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--directory',type=Path,default=Path('dist/0.1.0-local-test-20261020-folders'));parser.add_argument('--prefix',default='folders');args=parser.parse_args();prefix=args.prefix;assert prefix.replace('-','').replace('_','').isalnum();DIST=(ROOT/args.directory).resolve(strict=True);assert DIST.is_relative_to(ROOT/'dist')
def read(name):return json.loads((ROOT/'artifacts'/name).read_text(encoding='utf-8-sig'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
exe=DIST/'media-browser-portable/media-browser.exe';digest=sha(exe)
assert read(prefix+'-ui-verification.json')['exe_sha256']==digest
formats=read(prefix+'-format-matrix.json');heic=read(prefix+'-user-heic-ui.json')
for report,count in [(formats,24),(heic,2)]:
 assert report['checks_pass'] and report['finished'] and len(report['rows'])==count and report['exe_sha256']==digest
 assert all(r['passed'] and sha(ROOT/r['source'])==r['source_sha256'] for r in report['rows'])
for suffix in ['-user-heic-verification.json','-detail-verification.json','-overlay-verification.json','-subtitle-verification.json']:
 r=read(prefix+suffix);assert r['checks_pass'] and r['exe_sha256']==digest
source=read(prefix+'-source-release.json');gui=read(prefix+'-source-gui.json')
assert source['checks_pass'] and gui['checks_pass'] and gui['finished'] and len(gui['rows'])==2
assert gui['exe_sha256']==source['source_release_exe_sha256'] and sha(Path(source['source_release_exe']))==gui['exe_sha256']
assert all(r['passed'] for r in gui['rows'])
# A documentation/tools-only refresh can change ZIP bytes without changing compiled inputs.
records=json.loads((Path(source['source'])/'SOURCE-FILES.json').read_text(encoding='utf-8'))
production=production_inputs()
compiled={r['file']:r['sha256'] for r in records if r['file'] in production}
assert compiled==production
start=read(prefix+'-startup.json');assert start['exe_sha256'].lower()==digest and start['runs']==20
assert all(r['exit_code']==0 and r['input_idle'] for r in start['rows'])
scroll=[]
for suffix in ['cold4k','revisit4k']:
 label=prefix+'-final-'+suffix;s=read('p2-scroll-'+label+'-summary.json');raw=read('p2-scroll-'+label+'.json');process=read('p2-scroll-'+label+'-process.json')
 assert s['exe_sha256'].lower()==digest and s['media']==5000 and s['seconds']>=120
 assert process['exit_code']==0 and raw['error'] is None and s['max_scroll_points']>1000
 scroll.append(s)
assert read('p2-scroll-'+prefix+'-final-cold4k-process.json')['data_dir']==read('p2-scroll-'+prefix+'-final-revisit4k-process.json')['data_dir']
before=read(prefix+'-perf-fixture-before.json');after=read(prefix+'-perf-fixture-after.json')
assert before['checked']==after['checked']==5000 and before['all_match'] and after['all_match'] and before['files']==after['files']
idle=read(prefix+'-browser-idle.json');assert idle['exe_sha256'].lower()==digest and idle['exit_code']==0 and idle['elapsed_seconds']>=30
scan=read(prefix+'-scan.json');assert scan['media_count']==5000 and not scan['errors'] and scan['integrity_check']=='ok'
machine=read(prefix+'-performance-machine.json');assert machine['logical_processors']==idle['logical_processors']
gates={'callback_p95_le20ms':all(s['callback_interval_target_20ms_met'] for s in scroll),'working_set_le600mib':all(s['memory_target_600mib_met'] for s in scroll),'texture_le256mib':all(s['peak_texture_mib']<=256.01 for s in scroll),'all_samples_4k_150percent':all(s['physical_viewport_sizes']==[[3840,2160]] and s['scale_factors']==[1.5] for s in scroll),'preview_failures_zero':all(s['max_preview_failures']==0 for s in scroll),'enumeration_le3seconds':scan['enumeration_ms']<=3000,'idle_normalized_cpu_lt1percent':idle['normalized_total_cpu_percent']<1}
result={'evidence_valid':True,'full_acceptance_pass':False,'exe_sha256':digest,'format_cases_passed':26,'source_release_gui_cases':2,'source_production_inputs_equal_current_delivery':True,'all_measured_proxy_targets_met':all(gates.values()),'measured_proxy_targets':gates,'scroll':scroll,'startup':{k:v for k,v in start.items() if k!='rows'},'idle_cpu_percent':idle['normalized_total_cpu_percent'],'scan':{k:scan[k] for k in ['enumeration_ms','pipeline_ms','first_media_ms','media_count','image_count','video_count']},'machine_evidence':'artifacts/'+prefix+'-performance-machine.json','hardware_information_complete':False,'scope':'Release proxy timings on 5000 generated duplicates; no physical presentation, OS cold reboot, independent photography library, Win11, clean GPU offline or current full native recycle/install certification'}
(ROOT/'artifacts'/(prefix+'-release-recheck.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps({k:v for k,v in result.items() if k not in ('startup','scroll')},ensure_ascii=False,indent=2))
