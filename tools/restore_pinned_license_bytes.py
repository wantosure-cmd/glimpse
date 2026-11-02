"""Restore pinned license bytes changed by Windows Git CRLF checkout, offline."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[1]
def sha(b):return hashlib.sha256(b).hexdigest()
rows=json.loads((ROOT/'vendor/upstream-license-sources.json').read_text(encoding='utf8'))
fixed=[]
for row in rows:
    path=ROOT/row['file']
    assert path.resolve().is_relative_to((ROOT/'assets/licenses').resolve())
    if sha(path.read_bytes())==row['sha256']:continue
    original=path.read_bytes()
    blob=subprocess.check_output(['git','show','HEAD:'+row['file']],cwd=ROOT)
    assert sha(blob)==row['sha256'],row['file']
    assert original.replace(b'\r\n',b'\n')==blob.replace(b'\r\n',b'\n'),row['file']
    path.write_bytes(blob)
    fixed.append(row['file'])
(ROOT/'artifacts/v0447-license-checkout-restoration.json').write_text(json.dumps({'only_line_endings':True,'restored_from_git_blob':fixed},ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps({'restored':len(fixed),'pinned_files_checked':len(rows)}))
