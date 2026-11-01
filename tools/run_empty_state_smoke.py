"""Generated empty/filtered directories and native screenshots, no OS input."""
from pathlib import Path
import argparse,hashlib,json,subprocess,uuid
from PIL import Image
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',required=True);a=p.parse_args()
assert a.prefix.replace('-','').isalnum()
exe=a.exe.resolve(strict=True);out=ROOT/'artifacts'/(a.prefix+'.json');assert not out.exists()
task=ROOT/'target'/('empty-state-owned-'+uuid.uuid4().hex);task.mkdir();(task/'.owned-fixture').write_text('Generated empty folders and synthetic PNG; no private sources')
rows=[]
for name,filtered in [('empty',False),('filtered',True)]:
 work=task/name/'work';media=task/name/'media';data=task/name/'data'
 for path in [work,media,data]:path.mkdir(parents=True)
 source=media/'generated.png'
 if filtered:Image.new('RGB',(320,240),(50,110,170)).save(source)
 before=hashlib.sha256(source.read_bytes()).hexdigest() if filtered else None
 config=dict(settings=dict(filter='Videos' if filtered else 'All',sort='Name',descending=False,type_groups=False))
 (data/'settings.json').write_text(json.dumps(config))
 with (work/'stdout.log').open('xb') as stdout,(work/'stderr.log').open('xb') as stderr:
  proc=subprocess.Popen([str(exe),'--open',str(media),'--data-dir',str(data),'--instance-scope',a.prefix+'-'+name,'--browser-smoke','--quit-after','4800'],cwd=work,stdout=stdout,stderr=stderr)
  code=proc.wait(timeout=30)
 assert code==0,(name,code)
 screenshot=work/'artifacts/p2-list.png';assert screenshot.is_file(),(name,'Missing native screenshot; do not assume rendering succeeded')
 img=Image.open(screenshot).convert('RGB');w,h=img.size
 region=img.crop((w//4,h//3,3*w//4,3*h//4))
 bright=sum(1 for px in region.getdata() if min(px)>80)
 assert bright>100,(name,'Expected readable centered empty-state text')
 if filtered:assert hashlib.sha256(source.read_bytes()).hexdigest()==before
 else:assert not list(media.iterdir())
 rows.append(dict(name=name,pid=proc.pid,screenshot=str(screenshot),center_bright_pixels=bright,sources_unchanged=True))
result=dict(checks_pass=True,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),rows=rows,scope='Real native screenshots of generated empty and image-only/video-filtered roots. Pixel contrast check plus manual screenshot review; no physical input or real-library verification.')
out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False))
