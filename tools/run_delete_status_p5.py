"""Current immutable delete-status delivery: sequential native media checks."""
from pathlib import Path
import json,subprocess,sys,uuid
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'tools'));import run_format_matrix as matrix
prefix='deletefinal';delivery=ROOT/'dist/0.1.0-local-test-20261020-delete-status';exe=delivery/'media-browser-portable/media-browser.exe'
def matrix_run(manifest,output,program):
 matrix.EXE=program;sys.argv=['run_format_matrix.py','--manifest',str(manifest),'--output',str(ROOT/'artifacts'/output)];matrix.main()
matrix_run(ROOT/'test-media/format-matrix-v1/manifest.json',prefix+'-format-matrix.json',exe)
matrix_run(ROOT/'test-media/format-matrix-v1/user-heic-manifest.json',prefix+'-user-heic-ui.json',exe)
source=json.loads((ROOT/'artifacts'/ (prefix+'-source-release.json')).read_text(encoding='utf-8'));assert source['checks_pass']
manifest=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf-8'));manifest['cases']=[c for c in manifest['cases'] if c['name'] in ('heic','h264-mov')]
subset=ROOT/'target'/('deletefinal-source-gui-'+uuid.uuid4().hex+'.json');subset.write_text(json.dumps(manifest),encoding='utf-8')
matrix_run(subset,prefix+'-source-gui.json',Path(source['source_release_exe']))
raw=ROOT/'artifacts'/(prefix+'-detail-raw.json')
with (ROOT/'artifacts'/(prefix+'-detail-stdout.log')).open('wb') as out,(ROOT/'artifacts'/(prefix+'-detail-stderr.log')).open('wb') as err:
 r=subprocess.run([str(exe),'--detail-smoke',str(raw),'--subtitle-smoke','--open','target/detail-redesign-fixture','--data-dir','target/deletefinal-detail-'+uuid.uuid4().hex],cwd=ROOT,stdout=out,stderr=err,timeout=90)
assert r.returncode==0
for script,name in [('verify_detail_redesign.py','-detail-verification.json'),('verify_detail_overlay.py','-overlay-verification.json'),('verify_subtitle_detail.py','-subtitle-verification.json')]:
 subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools'/script),str(raw),'--output',str(ROOT/'artifacts'/(prefix+name))],cwd=ROOT,check=True)
overlay=ROOT/'artifacts'/(prefix+'-overlay-verification.json');r=json.loads(overlay.read_text(encoding='utf-8'));r['raw']='artifacts/'+prefix+'-detail-raw.json';assert r['exe_sha256']==matrix.sha(exe);overlay.write_text(json.dumps(r,ensure_ascii=False,indent=2),encoding='utf-8')
subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools/recheck_user_heic_clear.py'),'--prefix',prefix,'--directory',str(delivery)],cwd=ROOT,check=True)
