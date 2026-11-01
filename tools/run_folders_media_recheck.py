"""Post-matrix current-build HEIC/detail and extracted-source native checks."""
from pathlib import Path
import json,subprocess,sys,uuid
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'tools'))
import run_format_matrix as matrix
DELIVERY=ROOT/'dist/0.1.0-local-test-20261020-folders'
exe=DELIVERY/'media-browser-portable/media-browser.exe'
base=json.loads((ROOT/'artifacts/folders-format-matrix.json').read_text(encoding='utf-8'))
assert base['checks_pass'] and base['finished'] and len(base['rows'])==24 and base['exe_sha256']==matrix.sha(exe)
matrix.EXE=exe
sys.argv=['run_format_matrix.py','--manifest',str(ROOT/'test-media/format-matrix-v1/user-heic-manifest.json'),'--output',str(ROOT/'artifacts/folders-user-heic-ui.json')]
matrix.main()
source=json.loads((ROOT/'artifacts/folders-source-release.json').read_text(encoding='utf-8'));assert source['checks_pass']
manifest=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf-8'))
manifest['cases']=[c for c in manifest['cases'] if c['name'] in ('heic','h264-mov')];assert len(manifest['cases'])==2
fixture=ROOT/'target'/('source-gui-folders-'+uuid.uuid4().hex+'.json');fixture.write_text(json.dumps(manifest),encoding='utf-8')
matrix.EXE=Path(source['source_release_exe'])
assert matrix.sha(matrix.EXE)==source['source_release_exe_sha256']
sys.argv=['run_format_matrix.py','--manifest',str(fixture),'--output',str(ROOT/'artifacts/folders-source-gui.json')]
matrix.main()
raw=ROOT/'artifacts/folders-detail-raw.json'
args=[str(exe),'--detail-smoke',str(raw),'--subtitle-smoke','--open','target/detail-redesign-fixture','--data-dir','target/folders-detail-'+uuid.uuid4().hex]
with (ROOT/'artifacts/folders-detail-stdout.log').open('wb') as out,(ROOT/'artifacts/folders-detail-stderr.log').open('wb') as err:
 result=subprocess.run(args,cwd=ROOT,stdout=out,stderr=err,timeout=90)
assert result.returncode==0
for script,name in [('verify_detail_redesign.py','folders-detail-verification.json'),('verify_detail_overlay.py','folders-overlay-verification.json'),('verify_subtitle_detail.py','folders-subtitle-verification.json')]:
 subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools'/script),str(raw),'--output',str(ROOT/'artifacts'/name)],cwd=ROOT,check=True)
overlay=ROOT/'artifacts/folders-overlay-verification.json';r=json.loads(overlay.read_text(encoding='utf-8'));r['raw']='artifacts/folders-detail-raw.json';assert r['exe_sha256']==matrix.sha(exe);overlay.write_text(json.dumps(r,ensure_ascii=False,indent=2),encoding='utf-8')
subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools/verify_user_heic.py'),'--ui',str(ROOT/'artifacts/folders-user-heic-ui.json'),'--output',str(ROOT/'artifacts/folders-user-heic-verification.json'),'--overlay-evidence',str(raw),'--overlay-verification',str(overlay)],cwd=ROOT,check=True)
