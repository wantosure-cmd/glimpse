"""Make a Windows-only acceptance kit; no private user media or app binaries."""
import argparse,hashlib,json,shutil,zipfile
from pathlib import Path
from pe_dependencies import imports
ROOT=Path(__file__).resolve().parent.parent

def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        while b:=f.read(1024*1024):h.update(b)
    return h.hexdigest()

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--tag',required=True)
    parser.add_argument('--delivery',type=Path,default=ROOT/'dist/0.1.0-local-test-20261019-cloud')
    parser.add_argument('--manifest',type=Path,default=ROOT/'test-media/format-matrix-v1/manifest.json')
    args=parser.parse_args()
    bound_delivery=args.delivery.resolve(strict=True);assert bound_delivery.is_relative_to(ROOT/'dist')
    assert args.tag and all(c.isalnum() or c in '-_.' for c in args.tag) and args.tag not in ('.','..')
    fixture_manifest=args.manifest.resolve(strict=True)
    assert fixture_manifest.is_relative_to(ROOT)
    source=json.loads(fixture_manifest.read_text(encoding='utf8'))
    baseline=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf8'))
    base_names={c['name'] for c in baseline['cases']} - {'h264-aac-flv','flv1-mp3-flv'}
    names=[c['name'] for c in source['cases']]
    assert len(base_names)==24 and len(names)==len(set(names))
    assert set(names) in (base_names,base_names|{'h264-aac-flv','flv1-mp3-flv'}), 'Preserve all24 original cases; only add both known FLVs'
    for c in source['cases']:
        assert c['name'] not in ('.','..') and all(ch.isalnum() or ch=='-' for ch in c['name'])
        src=(ROOT/c['path']).resolve(strict=True)
        assert src.is_relative_to(ROOT/'test-media/format-matrix-v1'), 'Only approved public/generated fixture files'
        assert sha(src)==c['sha256'] and src.stat().st_size==c['bytes']
        if c['name'] in base_names:
            assert c==next(old for old in baseline['cases'] if old['name']==c['name']), 'Original case changed'
    out=ROOT/'dist'/args.tag;out.mkdir(exist_ok=False)
    kit=out/'windows-acceptance-kit';kit.mkdir();(kit/'samples').mkdir()
    portable=bound_delivery/'media-browser-portable.zip'
    delivery=json.loads((portable.parent/'delivery.json').read_text(encoding='utf8'))
    cases=[]
    for c in source['cases']:
        src=ROOT/c['path'];assert sha(src)==c['sha256']
        target=kit/'samples'/c['name']/src.name;target.parent.mkdir();shutil.copyfile(src,target)
        w,h=c['expected']['width'],c['expected']['height']
        if c['kind']=='video':
            stream=next(s for s in c['ffprobe']['streams'] if s['codec_type']=='video')
            w,h=stream['width'],stream['height'];n,d=map(float,stream.get('sample_aspect_ratio','1:1').split(':'));w=round(w*n/d)
            if abs(next((s['rotation'] for s in stream.get('side_data_list',[]) if 'rotation' in s),0))%180==90:w,h=h,w
        entry=dict(name=c['name'],kind=c['kind'],file=target.relative_to(kit).as_posix(),sha256=c['sha256'],bytes=c['bytes'],width=w,height=h,expected=c['expected'])
        if c['kind']=='image' and not c['name'].startswith('heic'):
            colors=[[220,25,35],[25,200,55],[35,55,220],[225,210,25]];order={1:[0,1,2,3],2:[1,0,3,2],6:[2,0,3,1]}[c['expected']['orientation']];entry['corners']=[colors[i] for i in order]
        cases.append(entry)
    meter=ROOT/'target/diagnostics/Release/audio-meter.exe';assert not any(n.lower().startswith(('vcruntime','msvcp','concrt')) for n in imports(meter));shutil.copyfile(meter,kit/meter.name)
    shutil.copyfile(ROOT/'tools/run_windows_acceptance.ps1',kit/'run.ps1')
    manifest=dict(delivery=bound_delivery.relative_to(ROOT).as_posix(),fixture_manifest_sha256=sha(fixture_manifest),application_version=delivery['application_version'],exe_sha256=delivery['exe_sha256'],portable_sha256=sha(portable),meter_sha256=sha(meter),cases=cases,scope='Windows native format validation; real phone 10-bit/HDR and physical input are not covered; no private user media')
    (kit/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf8')
    (kit/'README.txt').write_text(f'Windows 10/11 x64 offline format check.\nCopy the matching portable ZIP and this kit to the test PC. Extract this kit, then run:\n  powershell -NoProfile -ExecutionPolicy Bypass -File run.ps1 -PortableZip C:\\path\\media-browser-portable.zip\nNo Python, Rust, FFmpeg installation or administrator account is required.\nThe script verifies/extracts the portable ZIP to a NEW unique directory, checks {len(cases)} public/generated samples and records native screenshots, audio levels, hashes and OS metadata. It never deletes samples or your media.\nFor a shorter validation run append -CaseName png (a partial run will be marked partial).\nKeep result.json and output files; results from Windows 10 do not certify Windows 11.\nDo not include private user media in public bug reports. Original upstream sample licensing remains applicable; this is a local acceptance kit, not public distribution clearance.\n',encoding='utf8')
    entries=[dict(file=f.relative_to(kit).as_posix(),bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(kit.rglob('*')) if f.is_file()]
    (kit/'KIT-FILES.json').write_text(json.dumps(entries,ensure_ascii=False,indent=2),encoding='utf8')
    archive=out/'windows-acceptance-kit.zip'
    with zipfile.ZipFile(archive,'x',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for f in sorted(kit.rglob('*')):
            if f.is_file():z.write(f,'windows-acceptance-kit/'+f.relative_to(kit).as_posix())
    print(json.dumps(dict(directory=str(out),zip_sha256=sha(archive),bytes=archive.stat().st_size,samples=len(cases)),ensure_ascii=False,indent=2))
if __name__=='__main__':main()
