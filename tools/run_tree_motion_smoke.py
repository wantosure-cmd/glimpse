"""Generate a read-only fixture and verify tree, image-side-button and wheel navigation."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys,uuid,zlib,struct,argparse
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--exe',type=Path,default=ROOT/'dist/browser-preview/media-browser.exe');parser.add_argument('--prefix',default='tree-motion');args=parser.parse_args();prefix=args.prefix;assert prefix.replace('-','').replace('_','').isalnum()
task=ROOT/'target'/('tree-motion-owned-'+uuid.uuid4().hex);task.mkdir()
media=task/'media';media.mkdir()
def png(path,w,h,color):
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    raw=(b'\0'+bytes(color)*w)*h
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b''))
for directory,count in [('',2),('123',5),('123/456',5),('123/456/789',5),('zzz',3)]:
    folder=media/directory;folder.mkdir(exist_ok=True,parents=True)
    for i in range(count):png(folder/f'{i}.png',400 if i%2==0 else 600,600 if i%2==0 else 400,[(40+i*20)%255,100,180])
(media/'empty').mkdir();(media/'123/empty').mkdir()
shutil.copyfile(ROOT/'test-media/test-h264-aac.mov',media/'zz-video.mov')
def hashes():return {str(p.relative_to(media)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(media.rglob('*')) if p.is_file()}
before=hashes();exe=args.exe.resolve(strict=True)
output=ROOT/'artifacts'/(prefix+'-'+task.name+'.json')
work=task/'work';work.mkdir()
with (task/'stdout.log').open('wb') as out,(task/'stderr.log').open('wb') as err:
    app=subprocess.Popen([str(exe),'--navigation-smoke',str(output),'--instance-scope',task.name,'--open',str(media),'--data-dir',str(task/'data')],cwd=work,stdout=out,stderr=err)
    try:code=app.wait(timeout=65)
    except subprocess.TimeoutExpired:
        (task/'timeout.json').write_text(json.dumps(dict(pid=app.pid)),encoding='utf-8');raise
raw=json.loads(output.read_text());after=hashes()
result=dict(checks_pass=False,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),pid=app.pid,exit_code=code,raw=str(output),work=str(work),source_files=len(before),sources_unchanged=before==after,error=raw['error'],rows=raw['rows'],full_acceptance_pass=False)
try:
    assert code==0 and raw['checks_pass'] and raw['error'] is None,raw
    assert before==after
    rows={r['phase']:r for r in raw['rows']}
    for name in ['folder_blank_collapsed', 'folder_blank_expanded']:
        assert name in rows, name
    for n in ['folder_click_locates','sticky_collapsed','locate_expands_ancestors','opening_motion','side_next','side_previous','filmstrip_three_steps','closing_motion','returned_to_list','maximum_single_column']:assert n in rows
    assert len({r['generation'] for r in raw['rows']})==1
    assert all(Path(r['root']).samefile(media) for r in raw['rows'])
    assert rows['opening_motion']['motion'] and rows['closing_motion']['motion']
    assert rows['maximum_single_column']['columns']==1
    assert all(r['unknown_cards']==0 for r in raw['rows'])
    for phase in ['folder_resume_after_expand','root_resume_after_expand']:
        assert phase in rows and rows[phase]['anchor_error'] <= 1, phase
    result['checks_pass']=True
finally:
    (ROOT/'artifacts'/(prefix+'-verification.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k!='rows'},ensure_ascii=False,indent=2))
