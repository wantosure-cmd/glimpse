"""Verify the owned local package bytes and their tested production source inputs."""
from pathlib import Path
import hashlib, json, struct, zipfile, argparse
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--tag',default='0.1.0-local-test-20261021-raycast');parser.add_argument('--prefix',default='raycast-final');args=parser.parse_args();assert args.tag.replace('-','').replace('.','').isalnum() and args.prefix.replace('-','').isalnum()
DIST=ROOT/'dist'/args.tag
def sha(data):return hashlib.sha256(data).hexdigest()
delivery=json.loads((DIST/'delivery.json').read_text(encoding='utf-8'))
evidence=json.loads((ROOT/('artifacts/'+args.prefix+'-regressions.json')).read_text(encoding='utf-8'))
assert evidence['checks_pass']
assert delivery['exe_sha256']==evidence['exe_sha256']
for entry in delivery['files']:
    data=(DIST/entry['file']).read_bytes()
    assert sha(data)==entry['sha256'] and len(data)==entry['bytes'],entry['file']
with zipfile.ZipFile(DIST/'media-browser-portable.zip') as z:
    assert z.testzip() is None
    app=next(n for n in z.namelist() if n.endswith('/media-browser.exe'))
    assert sha(z.read(app))==evidence['exe_sha256']
portable=DIST/'media-browser-portable'
records=json.loads((portable/'package-files.json').read_text(encoding='utf-8'))
for r in records:
    data=(portable/r['path']).read_bytes()
    assert len(data)==r['bytes'] and sha(data)==r['sha256'],r['path']
with zipfile.ZipFile(DIST/'media-browser-source.zip') as z:
    assert z.testzip() is None
    source=json.loads(z.read('media-browser-source/SOURCE-FILES.json'))
    for r in source:
        data=z.read('media-browser-source/'+r['file'])
        assert len(data)==r['bytes'] and sha(data)==r['sha256'],r['file']
    source_hashes={r['file']:r['sha256'] for r in source}
    assert all(source_hashes.get(p)==v for p,v in evidence['production_source_sha256'].items())
installer=(DIST/'media-browser-setup-local-test.exe').read_bytes()
assert installer[-24:-16]==b'MBSET001'
length,stub=struct.unpack('<QQ',installer[-16:])
manifest=json.loads(installer[-24-length:-24])
for r in manifest['files']:
    data=installer[r['offset']:r['offset']+r['bytes']]
    assert sha(data)==r['sha256'] and data==(portable/r['path']).read_bytes(),r['path']
result=dict(checks_pass=True,exe_sha256=evidence['exe_sha256'],source_files=len(source),portable_files=len(records),installer_payload_files=len(manifest['files']),scope='Archive/payload/source byte checks; not a Windows install/uninstall runtime test')
(ROOT/('artifacts/'+args.prefix+'-package-verification.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result,ensure_ascii=False,indent=2))
