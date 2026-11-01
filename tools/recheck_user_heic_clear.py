"""Recheck HEIC clear-frame fidelity; preserve cold progressive-frame failures."""
from pathlib import Path
import json,shutil,sys,uuid,types,subprocess,argparse
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'tools'));import run_format_matrix as matrix
parser=argparse.ArgumentParser();parser.add_argument('--prefix',default='folders');parser.add_argument('--directory',type=Path,default=Path('dist/0.1.0-local-test-20261020-folders'));args=parser.parse_args();prefix=args.prefix;assert prefix.replace('-','').replace('_','').isalnum()
initial=ROOT/'artifacts'/(prefix+'-user-heic-ui.json')
previous=json.loads(initial.read_text(encoding='utf-8'))
backup=ROOT/'artifacts'/(prefix+'-user-heic-cold-progressive-ui.json')
assert not backup.exists();shutil.copy2(initial,backup)
nonce=uuid.uuid4().hex;seeds=[]
for row in previous['rows']:
    old=Path(row['evidence']).parts[2];name=row['name']
    cache=ROOT/'target'/('format-smoke-'+old)/name/'thumbnails'
    dest=ROOT/'target'/('format-smoke-'+nonce)/name/'thumbnails'
    assert cache.is_relative_to(ROOT/'target') and dest.is_relative_to(ROOT/'target')
    shutil.copytree(cache,dest)
    seeds.append({'case':name,'from':str(cache),'to':str(dest),'scope':'Existing app-produced fingerprinted thumbnail cache only; no source media changed'})
(ROOT/'artifacts'/(prefix+'-heic-clear-cache-provenance.json')).write_text(json.dumps({'source_evidence':str(backup),'source_exe_sha256':previous['exe_sha256'],'seeds':seeds,'scope':'Clear-fidelity recheck on app cache; cold first-visible previews are separate evidence, not clear frames'},ensure_ascii=False,indent=2),encoding='utf-8')
matrix.EXE=ROOT/args.directory/'media-browser-portable/media-browser.exe'
original=matrix.uuid.uuid4;matrix.uuid.uuid4=lambda:types.SimpleNamespace(hex=nonce)
try:
 sys.argv=['run_format_matrix.py','--manifest',str(ROOT/'test-media/format-matrix-v1/user-heic-manifest.json'),'--output',str(initial)]
 matrix.main()
finally:matrix.uuid.uuid4=original
subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools/verify_user_heic.py'),'--ui',str(initial),'--output',str(ROOT/'artifacts'/(prefix+'-user-heic-verification.json')),'--overlay-evidence',str(ROOT/'artifacts'/(prefix+'-detail-raw.json')),'--overlay-verification',str(ROOT/'artifacts'/(prefix+'-overlay-verification.json'))],cwd=ROOT,check=True)
