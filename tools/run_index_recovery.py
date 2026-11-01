"""Native managed-index corruption checks. Only generated data is damaged."""
from pathlib import Path
import argparse,hashlib,json,shutil,sqlite3,subprocess,uuid
from package_local import production_inputs
from run_format_matrix import screenshot
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',default='v031-index');a=p.parse_args()
assert a.prefix.replace('-','').isalnum()
exe=a.exe.resolve(strict=True);task=ROOT/'target'/('index-recovery-owned-'+uuid.uuid4().hex);task.mkdir()
(task/'.index-recovery-owned').write_text('Dedicated generated cache and copied public PNG only',encoding='ascii')
media=task/'media';media.mkdir();fixture=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf-8'));case=next(c for c in fixture['cases'] if c['name']=='png');source=ROOT/case['path'];copied=media/'sample.png';shutil.copyfile(source,copied)
evidence=ROOT/'artifacts'/task.name;evidence.mkdir();output=ROOT/'artifacts'/(a.prefix+'.json')
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
digest=sha(exe);inputs=production_inputs();rows=[];baseline_pixels=None;baseline_db=None
result=dict(checks_pass=False,exe_sha256=digest,production_inputs_match=True,task=str(task),evidence=str(evidence),rows=rows,scope='Actual native GUI and FBO on dedicated public PNG and generated corrupt indexes; not physical input, arbitrary database repair, disk-full or every SQLite corruption mode')
def save():output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
save()
for name in ['baseline','invalid_header','invalid_with_sidecars','corrupt_btree','warm_restart','future_schema']:
    dest=evidence/name;dest.mkdir();data=task/('data-invalid_header' if name=='warm_restart' else 'data-'+name);data.mkdir(exist_ok=True);db=data/'index.sqlite';original={}
    if name in ('invalid_header','invalid_with_sidecars'):
        db.write_bytes(b'Dedicated invalid managed index, never original media\n'*32)
        if name=='invalid_with_sidecars':
            for suffix in ('-wal','-shm','-journal'):(data/('index.sqlite'+suffix)).write_bytes(('Dedicated '+suffix).encode())
        original={f.name:f.read_bytes() for f in data.glob('index.sqlite*')}
    elif name=='corrupt_btree':
        src=sqlite3.connect(baseline_db);dst=sqlite3.connect(db);src.backup(dst);src.close();dst.close()
        raw=bytearray(db.read_bytes());raw[100]=255;db.write_bytes(raw);original={db.name:bytes(raw)}
    elif name=='future_schema':
        conn=sqlite3.connect(db);conn.executescript('CREATE TABLE preserved(value); INSERT INTO preserved VALUES(42); PRAGMA user_version=99;');conn.close();original={db.name:db.read_bytes()}
    before_backups={f.name for f in data.glob('index-recovery-*')}
    with (dest/'stdout.log').open('wb') as stdout,(dest/'stderr.log').open('wb') as stderr:
        cmd=[str(exe),'--open',str(media),'--data-dir',str(data),'--instance-scope',task.name+'-'+name.replace('_','-')]
        if name=='future_schema':cmd+=['--load-metrics',str(dest/'metrics.json'),'--quit-after','3000']
        else:cmd+=['--format-smoke',str(dest),'--quit-after','15000']
        proc=subprocess.run(cmd,cwd=task,stdout=stdout,stderr=stderr,timeout=30)
    row=dict(name=name,exit_code=proc.returncode,passed=False);rows.append(row);save();assert proc.returncode==0
    backups=[f for f in data.glob('index-recovery-*') if f.name not in before_backups]
    if name=='future_schema':
        assert not backups and db.read_bytes()==original[db.name]
        metrics=json.loads((dest/'metrics.json').read_text(encoding='utf-8'));assert max(x['media'] or 0 for x in metrics['rows'])==0
        conn=sqlite3.connect(db);assert conn.execute('SELECT value FROM preserved').fetchone()[0]==42;assert conn.execute('PRAGMA user_version').fetchone()[0]==99;conn.close()
        row.update(future_database_unchanged=True,no_unintended_rebuild=True)
    else:
        raw=json.loads((dest/'report.json').read_text(encoding='utf-8'));assert raw['error'] is None
        phases={r['phase']:r for r in raw['rows']};listed=phases['list_thumbnail'];detail=phases['detail_fit'];assert listed['textures'] and detail['textures'] and not listed['failures'] and not detail['failures']
        pixels=screenshot(dest/'list.png',listed);detail_pixels=screenshot(dest/'detail.png',detail)
        if baseline_pixels is None:baseline_pixels=pixels
        difference=max(abs(x-y) for q,r in zip(pixels['samples'],baseline_pixels['samples']) for x,y in zip(q,r));assert difference<=3
        conn=sqlite3.connect(db);assert conn.execute('PRAGMA integrity_check').fetchone()[0]=='ok';assert conn.execute('SELECT count(*) FROM media').fetchone()[0]==1;conn.close()
        if original:
            assert len(backups)==1
            assert all((backups[0]/f).read_bytes()==b for f,b in original.items())
        else:assert not backups
        row.update(media_count=1,index_integrity='ok',backup_files={f:hashlib.sha256(b).hexdigest() for f,b in original.items()},backup_path=str(backups[0]) if backups else None,maximum_poster_channel_difference=difference,list_pixels=pixels,detail_pixels=detail_pixels)
        if name=='baseline':baseline_db=db
    assert sha(source)==sha(copied)==case['sha256'];row.update(passed=True,source_unchanged=True);save();print('PASS '+name,flush=True)
# Strict CLI --db retains its prior non-recovery behavior.
strict=task/'strict-user-selected.sqlite';bad=b'Not an application-owned cache';strict.write_bytes(bad)
proc=subprocess.run([str(exe),'--scan-report',str(media),'--db',str(strict)],cwd=task,capture_output=True,timeout=15,creationflags=subprocess.CREATE_NO_WINDOW)
assert proc.returncode!=0 and strict.read_bytes()==bad
rows.append(dict(name='strict_cli',passed=True,exit_code=proc.returncode,original_database_unchanged=True))
result.update(checks_pass=True,production_inputs_match=inputs==production_inputs(),sources_unchanged=sha(copied)==sha(source)==case['sha256'])
assert result['production_inputs_match'] and sha(exe)==digest;save();print(output)
