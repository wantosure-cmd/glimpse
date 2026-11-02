"""Record this local UI build without treating historical acceptance as new evidence."""
import hashlib,json,subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
def sha(p):return hashlib.file_digest(p.open('rb'),'sha256').hexdigest()
def save(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
def main():
    old=ROOT.parent/'dist/0.4.20/media-browser-portable'
    failures=[]
    records=json.loads((old/'package-files.json').read_text(encoding='utf8'))
    for r in records:
        p=(old/r['path']).resolve()
        assert p.is_relative_to(old.resolve())
        if not p.is_file() or sha(p)!=r['sha256'].lower():failures.append(r['path'])
    runtime=[]
    for r in json.loads((ROOT/'vendor/runtime-manifest.json').read_text(encoding='utf-8-sig')):
        p=ROOT/'vendor/runtime'/r['file']
        runtime.append({'path':r['file'],'matches':p.stat().st_size==r['bytes'] and sha(p)==r['sha256'].lower()})
    save(ROOT/'artifacts/v0447-receipt.json',{
        'source_repository':'https://github.com/wantosure-cmd/glimpse.git',
        'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        'source_baseline_version':'0.4.46','new_version':'0.4.47',
        'user_original_exe_sha256':sha(old/'media-browser.exe'),
        'user_original_payload_files_checked':len(records),'original_payload_mismatches':failures,
        'pinned_runtime':runtime,'transfer_manifest_available':False,
        'scope':'Source checkout and original payload identity; not full acceptance or physical-device proof'})
    assert all(r['matches'] for r in runtime)
    print(json.dumps({'old_payload_mismatches':failures,'runtime_files':len(runtime)},ensure_ascii=False))
if __name__=='__main__':main()
