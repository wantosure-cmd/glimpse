"""Package integrity for this feature build; does not claim native install acceptance."""
from pathlib import Path
import json,hashlib,zipfile,struct
from package_local import production_inputs
ROOT=Path(__file__).resolve().parent.parent
DIST=ROOT/'dist/0.1.0-local-test-20261020-folders'
def digest(b):return hashlib.sha256(b).hexdigest()
def sha(p):return digest(p.read_bytes())
delivery=json.loads((DIST/'delivery.json').read_text(encoding='utf-8'))
assert delivery['production_source_sha256']==production_inputs()
for e in delivery['files']:
 p=DIST/e['file'];assert sha(p)==e['sha256'] and p.stat().st_size==e['bytes']
for line in (DIST/'SHA256SUMS.txt').read_text().splitlines():
 expected,name=line.split('  ',1);assert Path(name).name==name and sha(DIST/name)==expected
with zipfile.ZipFile(DIST/'media-browser-portable.zip') as z:
 assert z.testzip() is None
 manifest=json.loads(z.read('media-browser-portable/package-files.json'))
 for e in manifest:
  data=z.read('media-browser-portable/'+e['path']);assert len(data)==e['bytes'] and digest(data)==e['sha256'];assert sha(DIST/'media-browser-portable'/e['path'])==e['sha256']
with zipfile.ZipFile(DIST/'media-browser-source.zip') as z:
 assert z.testzip() is None
 sources=json.loads(z.read('media-browser-source/SOURCE-FILES.json'))
 for e in sources:
  data=z.read('media-browser-source/'+e['file']);assert digest(data)==e['sha256'] and len(data)==e['bytes'];assert sha(ROOT/e['file'])==e['sha256']
installer=DIST/'media-browser-setup-local-test.exe'
with installer.open('rb') as f:
 f.seek(-24,2);magic,size,stub=struct.unpack('<8sQQ',f.read(24));assert magic==b'MBSET001';f.seek(-24-size,2);payload=json.loads(f.read(size))
 offset=stub
 for e in payload['files']:
  assert e['offset']==offset;f.seek(offset);assert digest(f.read(e['bytes']))==e['sha256'];offset+=e['bytes']
 assert offset==installer.stat().st_size-24-size
ui=json.loads((ROOT/'artifacts/folders-ui-verification.json').read_text(encoding='utf-8'))
assert ui['exe_sha256']==delivery['exe_sha256']==sha(DIST/'media-browser-portable/media-browser.exe')
assert ui['ui_flow_pass'] and ui['permanent_confirmation_cancel_pass']
result={'package_integrity_pass':True,'source_files':len(sources),'portable_files':len(manifest),'installer_files':len(payload['files']),'exe_sha256':delivery['exe_sha256'],'ui_flow_pass':True,'permanent_confirmation_cancel_pass':True,'full_acceptance_pass':False,'native_install_retest':'not verified in restricted environment','native_recycle_retest':'system refused recycle, confirmation canceled, generated sources unchanged','real_cloud_permanent_delete':'not performed'}
(ROOT/'artifacts/folders-delivery-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
