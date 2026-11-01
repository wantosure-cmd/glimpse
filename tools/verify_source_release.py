"""Build the verified extracted source into a separate target; never replace delivery EXE."""
import argparse,hashlib,json,shutil,subprocess,time,zipfile
from pathlib import Path
from cargo_freshness import invalidate_application
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('source_evidence',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--log',type=Path,required=True);a=p.parse_args()
def sha(p):
    h=hashlib.sha256()
    with p.open('rb') as f:
        while b:=f.read(1024*1024):h.update(b)
    return h.hexdigest()
e=json.loads(a.source_evidence.read_text(encoding='utf-8'));assert e['checks_pass']
source=Path(e['extracted_source']).resolve(strict=True)
assert source.is_relative_to(ROOT/'target') and source.parent.name.startswith('source-package-check-')
assert (source.parent/'.source-check-owned').is_file()
records=json.loads((source/'SOURCE-FILES.json').read_text(encoding='utf-8'))
assert all(sha(source/r['file'])==r['sha256'] for r in records)
target=ROOT/'target/source-release-cache';started=time.time()
marker=target/'.source-release-owned'
if target.exists():assert marker.is_file(), 'Refusing an unowned build target'
target.mkdir(exist_ok=True);marker.write_text('Owned isolated source Release verifier',encoding='ascii')
packaged_exe=(Path(e['delivery_directory'])/'media-browser-portable/media-browser.exe'
              if e.get('delivery_directory') else ROOT/'dist/browser-preview/media-browser.exe')
packaged_exe=packaged_exe.resolve(strict=True)
assert packaged_exe.resolve(strict=True).is_relative_to(ROOT/'dist')
packaged_before=sha(packaged_exe)
if e.get('packaged_exe_sha256'):
    assert packaged_before==e['packaged_exe_sha256'], 'Packaged application differs from extraction evidence'
fresh_fingerprints=invalidate_application(target,'release')
command=['cargo','build','--release','--bin','media-browser','--locked','--offline','--jobs','4','--target-dir',str(target)]
with a.log.open('wb') as log:
    result=subprocess.run(command,cwd=source,stdout=log,stderr=subprocess.STDOUT,timeout=1800,creationflags=subprocess.CREATE_NO_WINDOW)
assert result.returncode==0,a.log.read_text(encoding='utf-8',errors='replace')[-4000:]
exe=target/'release/media-browser.exe';assert exe.is_file()
# This custom --target-dir is intentionally outside the source's usual target/release;
# stage the verified runtime beside it, as in the real directory distribution.
shutil.copyfile(source/'vendor/runtime/vcruntime140.dll',exe.parent/'vcruntime140.dll')
runtime=exe.parent/'runtime';runtime.mkdir(exist_ok=True)
for library in (source/'vendor/runtime').iterdir():
    if library.is_file():shutil.copyfile(library,runtime/library.name)
png=source.parent/'source-release-heic.png'
def run(*args):
    p=subprocess.run([str(exe),*map(str,args)],cwd=source,capture_output=True,timeout=60,creationflags=subprocess.CREATE_NO_WINDOW)
    assert p.returncode==0,(p.returncode,p.stderr.decode('utf-8',errors='replace'))
    return json.loads(p.stdout.decode('utf-8'))
fixture=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf-8'))
heic_case=next(c for c in fixture['cases'] if c['name']=='heic')
heic_source=ROOT/heic_case['path'];assert sha(heic_source)==heic_case['sha256']
heic=run('--decode-heic',heic_source,png)
assert (heic['width'],heic['height'])==(heic_case['expected']['width'],heic_case['expected']['height']) and png.stat().st_size>0
video=run('--probe-video',ROOT/'test-media/test-h264-aac.mov')
assert video['duration']>35 and len(video['seeks'])==4 and all(s['pass'] for s in video['seeks'])
assert all(sha(source/r['file'])==r['sha256'] for r in records)
assert sha(packaged_exe)==packaged_before
r={'checks_pass':True,'source_zip_sha256':e['source_zip_sha256'],'portable_zip_sha256':e['portable_zip_sha256'],'source':str(source),'source_files':len(records),'command':command,'build_exit_code':result.returncode,'fresh_application_fingerprints':fresh_fingerprints,'began_utc_epoch':started,'ended_utc_epoch':time.time(),'source_release_exe':str(exe),'source_release_exe_sha256':sha(exe),'heic':heic,'mov':video,'packaged_exe':str(packaged_exe),'packaged_release_unchanged_sha256':sha(packaged_exe),'scope':'Actual extracted application source Release build with existing offline Cargo/Rust/MSVC caches, restored native libraries, native HEIC decode and MOV null-output probe. Separate target; not byte-for-byte reproducibility, full GUI source-build test, vendored dependency bootstrap or clean-OS/Win11 validation'}
a.output.write_text(json.dumps(r,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:r[k] for k in ['checks_pass','source_zip_sha256','source_files','build_exit_code','source_release_exe_sha256','packaged_release_unchanged_sha256']},indent=2))
