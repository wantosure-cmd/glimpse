"""Read-only native format checks on generated long Unicode/space/emoji paths."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import uuid

ROOT = Path(__file__).resolve().parent.parent

def sha(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        while b:=f.read(1024*1024): h.update(b)
    return h.hexdigest()

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--executable',type=Path,default=ROOT/'dist/browser-preview/media-browser.exe');parser.add_argument('--output',type=Path);args=parser.parse_args()
    token=uuid.uuid4().hex
    fixture=ROOT/'test-media/format-matrix-v1'/('path-compat-'+token)
    fixture.mkdir()
    (fixture/'.generated-path-fixture').write_text('Dedicated legal duplicate samples; no user library',encoding='ascii')
    base=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf-8'))
    wanted={'jpeg-exif-6','heic','h264-mov','mpeg4-mp3-avi','h263-amr-3gp'}
    cases=[]
    for case in base['cases']:
        if case['name'] not in wanted:continue
        c=copy.deepcopy(case)
        folder=fixture/c['name']
        while len(str(folder).encode('utf-16-le'))//2 < 330:
            folder=folder/('中文 空格 👩‍🚀 é 深层目录_'+str(len(folder.parts)))
        folder.mkdir(parents=True)
        src=ROOT/case['path']
        assert sha(src)==case['sha256']
        dest=folder/('照片 视频 你好 👩‍🚀 é'+src.suffix)
        shutil.copyfile(src,dest)
        assert sha(dest)==case['sha256']
        c['path']=str(dest.relative_to(ROOT))
        c['path_utf16_units']=len(str(dest).encode('utf-16-le'))//2
        assert c['path_utf16_units']>260
        cases.append(c)
    manifest=fixture/'manifest.json'
    manifest.write_text(json.dumps(dict(scope='Generated duplicate legal samples at long Unicode paths',cases=cases),ensure_ascii=False,indent=2),encoding='utf-8')
    evidence=args.output or ROOT/'artifacts'/('path-compat-'+token+'.json')
    exe=args.executable.resolve(strict=True)
    result=dict(exe_sha256=sha(exe),fixture=str(fixture),manifest=str(manifest),cases=[dict(name=c['name'],path=c['path'],path_utf16_units=c['path_utf16_units'],sha256=c['sha256']) for c in cases])
    evidence.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    print('LIVE_EVIDENCE '+str(evidence),flush=True)
    scan=subprocess.run([str(exe),'--scan-report',str(fixture),'--db',str(ROOT/'target'/('path-compat-'+token+'.sqlite3'))],cwd=ROOT,capture_output=True)
    (ROOT/'artifacts'/('path-compat-'+token+'-scan.log')).write_bytes(scan.stdout+scan.stderr)
    assert scan.returncode==0,scan.stderr
    result['scan']=json.loads(scan.stdout)
    assert result['scan']['media_count']==5 and not result['scan']['errors']
    assert result['scan']['integrity_check']=='ok'
    evidence.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    ui=ROOT/'artifacts'/('path-compat-'+token+'-ui.json')
    with (ROOT/'artifacts'/('path-compat-'+token+'-ui.log')).open('wb') as log:
        run=subprocess.run(['python','-X','utf8',str(ROOT/'tools/run_format_matrix.py'),'--executable',str(exe),'--manifest',str(manifest),'--output',str(ui)],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
    result['ui_exit_code']=run.returncode
    result['ui_evidence']=str(ui)
    result['source_hashes_unchanged']=all(sha(ROOT/c['path'])==c['sha256'] for c in cases)
    result['finished']=True
    result['checks_pass']=run.returncode==0 and result['source_hashes_unchanged']
    assert sha(exe)==result['exe_sha256']
    evidence.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k not in ('cases','scan')},ensure_ascii=False,indent=2))
    raise SystemExit(0 if result['checks_pass'] else 1)

if __name__=='__main__': main()
