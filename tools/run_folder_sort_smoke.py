"""Native dropdown/reverse clicks on generated media, with explicit sibling expectations."""
from pathlib import Path
import argparse,hashlib,json,os,struct,subprocess,uuid,zipfile,zlib
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',default='v0448-folder-sort');a=p.parse_args()
assert a.prefix.replace('-','').replace('_','').isalnum()
task=ROOT/'target'/('folder-sort-owned-'+uuid.uuid4().hex)
media=task/'media';work=task/'work';media.mkdir(parents=True);work.mkdir()
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',8,8,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes((60,120,190))*8)*8))+chunk(b'IEND',b'')
spec={'folder1/10.png':18000,'folder1/2.png':12000,'folder10/10.png':15000,'folder10/2.png':5000,
      'folder2/10.png':2000,'folder2/2.png':8000,'folder2/nested10/1.png':5000}
epoch=1_000_000_000_000_000_000
for name,size in spec.items():
    f=media/name;f.parent.mkdir(parents=True,exist_ok=True);f.write_bytes(png+b'\0'*(size-len(png)))
    t=epoch+(100 if f.name=='2.png' else 50)*1_000_000_000;os.utime(f,ns=(t,t))
(media/'folder2/nested3').mkdir()
times={'':0,'folder1':20,'folder2':30,'folder10':10,'folder2/nested3':5,'folder2/nested10':999}
for name,time in sorted(times.items(),key=lambda row:len(Path(row[0]).parts),reverse=True):
    t=epoch+time*1_000_000_000;os.utime(media/name,ns=(t,t))
def identity():return {'files':{str(f.relative_to(media)):hashlib.sha256(f.read_bytes()).hexdigest() for f in media.rglob('*') if f.is_file()},
                       'folder_modified':{n:(media/n).stat().st_mtime_ns for n in times}}
before=identity();exe=a.exe.resolve(strict=True)
out=ROOT/'artifacts'/(a.prefix+'-raw.json')
with (task/'stdout.log').open('wb') as stdout,(task/'stderr.log').open('wb') as stderr:
    proc=subprocess.Popen([str(exe),'--folder-sort-smoke',str(out),'--open',str(media),
        '--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=stdout,stderr=stderr)
    code=proc.wait(timeout=65)
raw=json.loads(out.read_text(encoding='utf8'))
assert code==0 and raw['checks_pass'],raw
expected=[['','folder1','folder2','folder2/nested3','folder2/nested10','folder10'],
          ['','folder10','folder2','folder2/nested10','folder2/nested3','folder1'],
          ['','folder10','folder1','folder2','folder2/nested3','folder2/nested10'],
          ['','folder2','folder2/nested10','folder2/nested3','folder1','folder10'],
          ['','folder2','folder2/nested3','folder2/nested10','folder10','folder1'],
          ['','folder1','folder10','folder2','folder2/nested10','folder2/nested3']]
assert len(raw['rows'])==6,raw
for i,row in enumerate(raw['rows']):
    assert row['groups']==expected[i],row
    assert row['headers']==expected[i],row
    files=[]
    for folder in expected[i]:
        children=[n for n in spec if str(Path(n).parent).replace('\\','/')==folder]
        key=lambda n: int(Path(n).stem) if row['sort']=='Name' else ((media/n).stat().st_mtime_ns if row['sort']=='Modified' else spec[n])
        files.extend(sorted(children,key=key,reverse=row['descending']))
    assert row['sequence']==files,(row,files)
    stats={d['path']:d for d in row['directories']}
    assert stats['folder2']['bytes']==15000 and stats['']['bytes']==65000,stats
    assert all(stats[n]['modified_ns']==t for n,t in before['folder_modified'].items()),stats
assert before==identity(),'Fixture changed'
result={'checks_pass':True,'cases':6,'all_files_and_folder_times_unchanged':True,
        'scope':raw['scope'],'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),
        'work':str(work),'raw':str(out),'fixture':str(media),'expected':expected}
(ROOT/'artifacts'/(a.prefix+'-verification.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps(result,ensure_ascii=False,indent=2))
