"""Package integrity and exact-build navigation/codec/input evidence; no install acceptance claim."""
from pathlib import Path
import json,hashlib,zipfile,struct
from package_local import production_inputs
ROOT=Path(__file__).resolve().parent.parent
DIST=ROOT/'dist/0.1.0-local-test-20261020-compact-sizing'
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
ui=json.loads((ROOT/'artifacts/compact-sizing-verification.json').read_text(encoding='utf-8'))
assert ui['checks_pass'] and ui['exit_code']==0 and ui['sources_unchanged']
assert ui['exe_sha256']==delivery['exe_sha256']==sha(DIST/'media-browser-portable/media-browser.exe')
raw=json.loads(Path(ui['raw']).read_text(encoding='utf-8'));assert raw['checks_pass'] and raw['error'] is None
names={r['phase'] for r in raw['rows']}
assert {'folder_click_locates','sticky_collapsed','locate_expands_ancestors','opening_motion','side_next','side_previous','filmstrip_three_steps','closing_motion','video_opening_motion','video_ready','video_closing_motion'}<=names
assert len({r['generation'] for r in raw['rows']})==1
assert all(r['unknown_cards']==0 for r in raw['rows'])
assert next(r for r in raw['rows'] if r['phase']=='maximum_single_column')['columns']==1
codec=json.loads((ROOT/'artifacts/compact-sizing-codec.json').read_text(encoding='utf-8'))
assert codec['checks_pass'] and codec['finished'] and len(codec['rows'])==6 and codec['exe_sha256']==delivery['exe_sha256']
assert all(r['passed'] and sha(ROOT/r['source'])==r['source_sha256'] for r in codec['rows'])
headers=json.loads((ROOT/'artifacts/compact-sizing-header-probes.json').read_text(encoding='utf-8'))
assert headers['checks_pass'] and headers['exe_sha256']==delivery['exe_sha256'] and len(headers['rows'])==5
assert all(r['source_unchanged'] and sha(Path(r['file']))==r['source_sha256'] for r in headers['rows'])
inputs=json.loads((ROOT/'artifacts/compact-sizing-interaction-recheck.json').read_text(encoding='utf-8'))
assert inputs['checks_pass'] and inputs['exe_sha256']==delivery['exe_sha256']
assert all(r['exit_code']==0 and r['verifier_exit_code']==0 and r['source_unchanged'] for r in inputs['rows'])
source=json.loads((ROOT/'artifacts/compact-sizing-source-check.json').read_text(encoding='utf-8'))
assert source['checks_pass'] and source['source_zip_sha256']==sha(DIST/'media-browser-source.zip')
performance=json.loads((ROOT/'artifacts/p2-scroll-compact-sizing-final-4k-summary.json').read_text(encoding='utf-8-sig'))
assert performance['exe_sha256'].lower()==delivery['exe_sha256'] and performance['media']==5000 and performance['seconds']>=120
assert performance['callback_interval_target_20ms_met'] and performance['memory_target_600mib_met'] and performance['max_preview_failures']==0
before=json.loads((ROOT/'artifacts/compact-sizing-fixture-before.json').read_text(encoding='utf-8-sig'));after=json.loads((ROOT/'artifacts/compact-sizing-fixture-after.json').read_text(encoding='utf-8-sig'))
assert before['checked']==after['checked']==5000 and before['all_match'] and after['all_match'] and before['files']==after['files']
result=dict(package_integrity_pass=True,exe_sha256=delivery['exe_sha256'],source_files=len(sources),portable_files=len(manifest),installer_files=len(payload['files']),native_navigation_motion_pass=True,targeted_codec_cases=6,read_only_heif_header_cases=5,input_playback_pass=True,source_check_pass=True,native_install_retest=False,full_acceptance_pass=False,scope='Local test package byte integrity, current-build controlled native regressions and fresh source extraction/Cargo check; not clean Windows offline install, physical input, full codec matrix or Win11 acceptance')
(ROOT/'artifacts/compact-sizing-delivery-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
