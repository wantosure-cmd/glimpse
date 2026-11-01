"""Package integrity and exact-build navigation/codec/input evidence; no install acceptance claim."""
from pathlib import Path
import json,hashlib,zipfile,struct
from package_local import production_inputs
ROOT=Path(__file__).resolve().parent.parent
DIST=ROOT/'dist/0.1.0-local-test-20261021-compact-sizing'
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
ui=json.loads((ROOT/'artifacts/compact-release-verification.json').read_text(encoding='utf-8'))
assert ui['checks_pass'] and ui['exit_code']==0 and ui['sources_unchanged']
assert ui['exe_sha256']==delivery['exe_sha256']==sha(DIST/'media-browser-portable/media-browser.exe')
raw=json.loads(Path(ui['raw']).read_text(encoding='utf-8'));assert raw['checks_pass'] and raw['error'] is None
names={r['phase'] for r in raw['rows']}
assert {'folder_click_locates','sticky_collapsed','locate_expands_ancestors','opening_motion','side_next','side_previous','filmstrip_three_steps','closing_motion','video_opening_motion','video_ready','video_closing_motion'}<=names
assert len({r['generation'] for r in raw['rows']})==1
assert all(r['unknown_cards']==0 for r in raw['rows'])
assert next(r for r in raw['rows'] if r['phase']=='maximum_single_column')['columns']==1
codec=json.loads((ROOT/'artifacts/compact-release-codec.json').read_text(encoding='utf-8'))
assert codec['checks_pass'] and codec['finished'] and len(codec['rows'])==24 and codec['exe_sha256']==delivery['exe_sha256']
assert all(r['passed'] and sha(ROOT/r['source'])==r['source_sha256'] for r in codec['rows'])
headers=json.loads((ROOT/'artifacts/compact-release-header-probes.json').read_text(encoding='utf-8'))
assert headers['checks_pass'] and headers['exe_sha256']==delivery['exe_sha256'] and len(headers['rows'])==5
assert all(r['source_unchanged'] and sha(Path(r['file']))==r['source_sha256'] for r in headers['rows'])
real_heic=json.loads((ROOT/'artifacts/compact-release-real-heic-decode.json').read_text(encoding='utf-8'))
assert real_heic['checks_pass'] and real_heic['exe_sha256']==delivery['exe_sha256'] and len(real_heic['rows'])==2
assert all(r['source_unchanged'] for r in real_heic['rows'])
binding=json.loads((ROOT/'artifacts/compact-release-build-binding.json').read_text(encoding='utf-8'))
assert binding['release_sha256']==binding['preview_sha256']==delivery['exe_sha256'] and binding['production_source_sha256']==delivery['production_source_sha256']
inputs=json.loads((ROOT/'artifacts/compact-release-interaction-recheck.json').read_text(encoding='utf-8'))
assert inputs['checks_pass'] and inputs['exe_sha256']==delivery['exe_sha256']
assert all(r['exit_code']==0 and r['verifier_exit_code']==0 and r['source_unchanged'] for r in inputs['rows'])
recycle=json.loads((ROOT/'artifacts/compact-final-trash-ui-status.json').read_text(encoding='utf-8'))
assert recycle['checks_pass'] is False and recycle['originals_unchanged'] and recycle['copies_unchanged']
assert recycle['exe_sha256']=='997b9ee437902ad634c083e4139133981340956972bdf7d0d2e927d74685f73e'
startup=json.loads((ROOT/'artifacts/compact-release-startup.json').read_text(encoding='utf-8-sig'))
assert startup['exe_sha256'].lower()==delivery['exe_sha256'] and startup['runs']==20 and all(r['exit_code']==0 for r in startup['rows'])
source=json.loads((ROOT/'artifacts/compact-release-source-check.json').read_text(encoding='utf-8'))
assert source['checks_pass'] and source['source_zip_sha256']==sha(DIST/'media-browser-source.zip')
source_release=json.loads((ROOT/'artifacts/compact-release-source-release.json').read_text(encoding='utf-8'))
assert source_release['checks_pass'] and source_release['source_zip_sha256']==sha(DIST/'media-browser-source.zip')
assert source_release['packaged_release_unchanged_sha256']==delivery['exe_sha256']
performance=json.loads((ROOT/'artifacts/p2-scroll-compact-release-final-4k-summary.json').read_text(encoding='utf-8-sig'))
assert performance['exe_sha256'].lower()==delivery['exe_sha256'] and performance['media']==5000 and performance['seconds']>=120
assert performance['callback_interval_target_20ms_met'] and performance['memory_target_600mib_met'] and performance['max_preview_failures']==0
before=json.loads((ROOT/'artifacts/compact-release-fixture-before.json').read_text(encoding='utf-8-sig'));after=json.loads((ROOT/'artifacts/compact-release-fixture-after.json').read_text(encoding='utf-8-sig'))
assert before['checked']==after['checked']==5000 and before['all_match'] and after['all_match'] and before['files']==after['files']
library_log=(ROOT/'artifacts/compact-final-lib-after-fixture-fix.log').read_text(encoding='utf-8',errors='replace')
assert '59 passed; 6 failed; 1 ignored' in library_log
result=dict(package_integrity_pass=True,exe_sha256=delivery['exe_sha256'],source_files=len(sources),portable_files=len(manifest),installer_files=len(payload['files']),native_navigation_motion_pass=True,codec_cases=24,read_only_heif_header_cases=5,real_user_heic_decode_cases=2,input_playback_pass=True,source_check_pass=True,independent_source_release_pass=True,current_recycle_acceptance_pass=False,intermediate_recycle_failed_sources_preserved=True,startup_runs=20,native_install_retest=False,library_suite_pass=False,library_tests_passed=59,library_tests_failed=6,full_acceptance_pass=False,scope='Local test package byte integrity, current-build controlled native regressions and fresh source extraction/Cargo check; not clean Windows offline install, physical input, system recycle/registry/IPC/replace gates or Win11 acceptance')
(ROOT/'artifacts/compact-release-delivery-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
