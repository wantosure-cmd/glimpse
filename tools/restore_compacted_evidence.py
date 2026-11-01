"""Restore selected historical evidence into a new target directory, never overwrite."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import uuid
import zipfile

ROOT=Path(__file__).resolve().parent.parent

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('archive',type=Path)
    p.add_argument('--prefix',required=True,help='Original workspace-relative file/directory prefix')
    p.add_argument('--destination',type=Path)
    a=p.parse_args()
    archive=a.archive.resolve(strict=True)
    assert archive.is_relative_to(ROOT/'artifacts')
    out=(a.destination or ROOT/'target'/('restored-evidence-'+uuid.uuid4().hex)).absolute()
    assert out.is_relative_to(ROOT/'target') and '..' not in out.parts and not out.exists()
    prefix=PurePosixPath(a.prefix.replace('\\','/'))
    assert not prefix.is_absolute() and '..' not in prefix.parts
    with zipfile.ZipFile(archive) as z:
        manifest=json.loads(z.read('manifest.json'))
        rows=[r for r in manifest['entries'] if PurePosixPath(r['path'])==prefix or PurePosixPath(r['path']).is_relative_to(prefix)]
        assert rows,'No matching evidence'
        out.mkdir(parents=True)
        restored=0;skipped=0
        for row in rows:
            if 'blob' not in row:
                skipped+=1
                continue
            relative=PurePosixPath(row['path'])
            assert not relative.is_absolute() and '..' not in relative.parts and ':' not in relative.parts[0]
            data=z.read(row['blob'])
            assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
            dest=out.joinpath(*relative.parts)
            assert dest.resolve().is_relative_to(out.resolve())
            dest.parent.mkdir(parents=True,exist_ok=True)
            with dest.open('xb') as f:f.write(data)
            restored+=1
    print(json.dumps(dict(destination=str(out),restored=restored,rebuildable_binaries_not_archived=skipped),ensure_ascii=False))

if __name__=='__main__':main()
