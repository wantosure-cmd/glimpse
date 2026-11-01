"""Compare pinned Release observations without claiming physical FPS or cold-boot timings."""
from pathlib import Path
import argparse,hashlib,json,statistics
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--before',default='v040-controlled');p.add_argument('--after',default='v041-controlled');p.add_argument('--output',default='artifacts/v041-performance-comparison.json');a=p.parse_args()
def read(name):return json.loads((ROOT/'artifacts'/name).read_text(encoding='utf-8-sig'))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def collect(prefix):
 startup=read(prefix+'-startup20.json');assert startup['runs']==20 and all(r['exit_code']==0 for r in startup['rows'])
 digest=startup['exe_sha256'].lower();out={'exe_sha256':digest,'startup_p95_ms':startup['external_input_idle_p95_ms'],'first_ui_p95_ms':startup['internal_first_ui_p95_ms']}
 for label in ['cold4k','revisit4k']:
  r=read('p2-scroll-'+prefix+'-'+label+'-summary.json');proc=read('p2-scroll-'+prefix+'-'+label+'-process.json')
  assert r['exe_sha256'].lower()==digest and r['seconds']>=120 and proc['exit_code']==0
  assert r['media']==5000 and r['physical_viewport_sizes']==[[3840,2160]] and r['scale_factors']==[1.5] and r['max_preview_failures']==0
  out[label]={'callback_p95_ms':r['callback_interval_p95_ms'],'peak_working_set_mib':r['peak_working_set_mib'],'peak_texture_mib':r['peak_texture_mib']}
  trace=read('p2-scroll-'+prefix+'-'+label+'-load.json')['rows'];out[label]['pipeline']=trace[-1].get('thumbnail_pipeline')
 before,after=[read(prefix+'-perf-fixture-'+n+'.json')for n in ['before','after']]
 assert before['checked']==after['checked']==5000 and before['all_match'] and after['all_match'] and before['files']==after['files'] and before['source_sha256']==after['source_sha256']
 idle=read(prefix+'-idle30.json');assert idle['exit_code']==0 and idle['exe_sha256'].lower()==digest
 out['idle_cpu_percent']=idle['normalized_total_cpu_percent'];out['fixture_hashes']=before['files'];return out
before,after=collect(a.before),collect(a.after);assert before.pop('fixture_hashes')==after.pop('fixture_hashes')
large=[read(p+'-large-load-series.json')for p in ['v040','v041']];assert [r['exe_sha256']for r in large]==[before['exe_sha256'],after['exe_sha256']]
medians=[statistics.median(r['clear_batch_external_ms']for r in d['rows'])for d in large]
folder=ROOT/'target/perf-large-jpeg-32';targets=[]
for source in sorted(folder.glob('*.jpg'))[:12]:
 st=source.stat();canonical='\\\\?\\'+str(source.resolve())
 for bucket in [64,512]:
  key=hashlib.sha256(b'thumbnail-v4-scaled-jpeg-fast-png'+canonical.encode('utf-16-le')+st.st_size.to_bytes(8,'little')+st.st_mtime_ns.to_bytes(8,'little',signed=True)+bucket.to_bytes(4,'little')).hexdigest();targets.append(key+'.png')
reference=None
for d in large:
 for row in d['rows']:
  cache=Path(row['metrics']).parent/'data/thumbnails';checks={n:sha(cache/n) for n in targets}
  if reference is None:reference=checks
  assert checks==reference,'Image output changed'
result={'before':before,'after':after,'large_jpeg':{'source_pixels':48000000,'generated_files':32,'clear_batch_files':12,'runs_per_version':5,'before_median_ms':medians[0],'after_median_ms':medians[1],'reduction_percent':(1-medians[1]/medians[0])*100,'identical_soft_and_clear_pngs':len(targets)*10,'first_preview_before_ms':large[0]['first_preview_median_ms'],'first_preview_after_ms':large[1]['first_preview_median_ms']},'measured_budgets_pass':all(after[k]['callback_p95_ms']<=20 and after[k]['peak_working_set_mib']<=600 and after[k]['peak_texture_mib']<=256.01 for k in ['cold4k','revisit4k'])and after['idle_cpu_percent']<1,'full_acceptance_pass':False,'scope':'Sequential same-host Release observations. Generated duplicate media, warm uncontrolled OS cache, fresh app caches. First 12 clear PNG completion is cache generation, not physical presentation. Callback P95 is not displayed FPS; startup input-idle is a proxy. Working set is the main process. Win11, physical input/presentation, true cold boot, real camera-library diversity and whole-process-tree resource use remain unverified.'}
(ROOT/a.output).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
