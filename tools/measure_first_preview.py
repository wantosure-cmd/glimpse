"""Observe first generated poster and probe progress, without desktop input.

Cache-file timestamps are a pipeline proxy, not display presentation timestamps.
The shared fixture consists solely of owned copies of a public/generated video.
"""
import argparse,hashlib,json,shutil,sqlite3,subprocess,time,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
def main():
 p=argparse.ArgumentParser();p.add_argument('--executable',type=Path,required=True);p.add_argument('--label',required=True);p.add_argument('--fixture',type=Path);a=p.parse_args();assert a.label.replace('-','').isalnum()
 fixture=ROOT/'target/preview-priority-580';marker=fixture/'.owned-preview-benchmark'
 source=ROOT/'test-media/format-matrix-v1/h263-amr-3gp/sample.3gp'
 if a.fixture is not None: fixture=a.fixture.resolve(strict=True); marker=None
 if marker is not None and not fixture.exists():
  fixture.mkdir();marker.write_text('Generated video copies, read-only benchmark',encoding='ascii')
  for i in range(580):shutil.copyfile(source,fixture/f'{i+1:04d}.3gp')
 if marker is not None: assert marker.is_file() and len(list(fixture.glob('*.3gp')))==580
 task=ROOT/'target'/('first-preview-'+uuid.uuid4().hex);task.mkdir();exe=a.executable.resolve(strict=True);data=task/'data';rows=[];first=None;eight=None
 started=time.monotonic()
 with (task/'stdout.log').open('wb') as stdout,(task/'stderr.log').open('wb') as stderr:
  app=subprocess.Popen([str(exe),'--open',str(fixture),'--data-dir',str(data),'--instance-scope',task.name,'--load-metrics',str(task/'draw-metrics.json'),'--quit-after','30000'],cwd=task,stdout=stdout,stderr=stderr)
  while app.poll() is None:
   elapsed=time.monotonic()-started;posters=len(list((data/'thumbnails').glob('*.png')));known=0;total=0
   try:
    with sqlite3.connect('file:'+str(data/'index.sqlite').replace('\\','/')+'?mode=ro',uri=True,timeout=.01) as db:total,known=db.execute('select count(*),count(width) from media').fetchone()
   except sqlite3.Error:pass
   rows.append(dict(seconds=elapsed,posters=posters,metadata=known,media=total))
   if posters and first is None:first=elapsed
   if posters>=8 and eight is None:eight=elapsed
   time.sleep(.2)
  code=app.wait()
 result=dict(exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),label=a.label,exit_code=code,fixture=str(fixture),data=str(data),first_poster_seconds=first,eight_posters_seconds=eight,rows=rows,scope='Cold app data, existing OS/network cache state; poster-file readiness proxy, native draw callbacks in separate draw-metrics when available; not physical display timing')
 result['native_draw_metrics']=str(task/'draw-metrics.json') if (task/'draw-metrics.json').is_file() else None
 (ROOT/'artifacts'/('first-preview-'+a.label+'.json')).write_text(json.dumps(result,indent=2),encoding='utf8');print(json.dumps({k:v for k,v in result.items() if k!='rows'},indent=2))
if __name__=='__main__':main()
