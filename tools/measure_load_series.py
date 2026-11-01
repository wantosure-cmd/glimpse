"""Repeat native first-preview observations with fresh private application caches."""
from pathlib import Path
import argparse,hashlib,json,statistics,subprocess,uuid,time
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--directory',type=Path,default=ROOT/'target/perf-media-5000');p.add_argument('--prefix',required=True);p.add_argument('--runs',type=int,default=5);p.add_argument('--seconds',type=int,default=10);p.add_argument('--clear-target',type=int,default=0);a=p.parse_args();assert a.prefix.replace('-','').isalnum() and 3<=a.runs<=20
exe=a.exe.resolve(strict=True);folder=a.directory.resolve(strict=True);assert (folder/'.media-browser-fixture').is_file()
assert 10<=a.seconds<=60 and 0<=a.clear_target<=32
task=ROOT/'target'/('load-series-owned-'+uuid.uuid4().hex);task.mkdir();rows=[];digest=hashlib.sha256(exe.read_bytes()).hexdigest()
for i in range(a.runs):
 work=task/f'run-{i}';work.mkdir();metrics=work/'metrics.json'
 with (work/'stdout.log').open('wb')as out,(work/'stderr.log').open('wb')as err:
  began=time.perf_counter();batch_ms=None
  proc=subprocess.Popen([str(exe),'--open',str(folder),'--data-dir',str(work/'data'),'--instance-scope',task.name+str(i),'--load-metrics',str(metrics),'--quit-after',str(a.seconds*1000)],cwd=work,stdout=out,stderr=err)
  targets=[]
  for source in sorted(folder.glob('*.jpg'))[:a.clear_target]:
   st=source.stat();canonical=str(source.resolve());canonical=canonical if canonical.startswith('\\\\?\\') else '\\\\?\\'+canonical
   key=hashlib.sha256(b'thumbnail-v4-scaled-jpeg-fast-png'+canonical.encode('utf-16-le')+st.st_size.to_bytes(8,'little')+st.st_mtime_ns.to_bytes(8,'little',signed=True)+(512).to_bytes(4,'little')).hexdigest()
   targets.append(work/'data/thumbnails'/(key+'.png'))
  while proc.poll() is None:
   if targets and batch_ms is None and all(p.is_file() for p in targets):batch_ms=(time.perf_counter()-began)*1000
   if time.perf_counter()-began>a.seconds+30:raise TimeoutError(f'Process still live: {proc.pid}; inspect before restarting')
   time.sleep(.02)
  code=proc.wait()
 assert code==0,(i,code)
 raw=json.loads(metrics.read_text(encoding='utf-8'))['rows'];assert raw and all(r['failures']==0 for r in raw)
 first=lambda predicate:next((r['elapsed_ms'] for r in raw if predicate(r)),None)
 row=dict(run=i,clear_batch_external_ms=batch_ms,clear_batch_targets=len(targets),first_preview_ms=first(lambda r:r['visible_ready']>0),first_clear_preview_ms=first(lambda r:r.get('visible_clear_ready',0)>0),all_visible_clear_ms=first(lambda r:r['visible_cards']>0 and r.get('visible_clear_ready',0)>=r['visible_cards'] and r.get('visible_pending_geometry',1)==0 and r['scan_complete']),last_pipeline=raw[-1].get('thumbnail_pipeline'),metrics=str(metrics))
 assert row['first_preview_ms'] is not None,row
 assert not a.clear_target or (len(targets)==a.clear_target and batch_ms is not None),(a.clear_target,row)
 rows.append(row);print(json.dumps(row),flush=True)
assert hashlib.sha256(exe.read_bytes()).hexdigest()==digest
result=dict(exe_sha256=digest,task=str(task),rows=rows,first_preview_median_ms=statistics.median(r['first_preview_ms']for r in rows),first_preview_max_ms=max(r['first_preview_ms']for r in rows),scope='Native callbacks, fresh application caches, warm uncontrolled OS cache, generated repeated files. Not physical presentation. First preview may be the soft image. All-visible clear requires no visible pending dimensions; only instrumented versions expose this field.')
(ROOT/'artifacts'/(a.prefix+'-load-series.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
