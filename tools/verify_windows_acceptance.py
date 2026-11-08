"""Validate the standalone kit and explicit partial or full native execution."""
import argparse,hashlib,json,zipfile
from pathlib import Path,PurePosixPath
ROOT=Path(__file__).resolve().parent.parent

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('directory',type=Path);p.add_argument('--smoke',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--expect-full',action='store_true')
    p.add_argument('--manifest',type=Path,default=ROOT/'test-media/format-matrix-v1/manifest.json')
    p.add_argument('--partial-cases',nargs='+',default=['png','h264-mp4'])
    a=p.parse_args()
    directory=a.directory.resolve(strict=True);assert directory.is_relative_to(ROOT/'dist')
    kit=directory/'windows-acceptance-kit';manifest=json.loads((kit/'manifest.json').read_text(encoding='utf8'))
    files=json.loads((kit/'KIT-FILES.json').read_text(encoding='utf8'))
    source=json.loads(a.manifest.read_text(encoding='utf8'))
    baseline=json.loads((ROOT/'test-media/format-matrix-v1/manifest.json').read_text(encoding='utf8'))
    base_names={c['name'] for c in baseline['cases']} - {'h264-aac-flv','flv1-mp3-flv'}
    source_names=[c['name'] for c in source['cases']]
    assert len(base_names)==24 and len(source_names)==len(set(source_names))
    assert set(source_names) in (base_names,base_names|{'h264-aac-flv','flv1-mp3-flv'})
    assert len(manifest['cases'])==len(source_names) and {c['name'] for c in manifest['cases']}==set(source_names)
    if len(source_names)==26 or 'fixture_manifest_sha256' in manifest:
        assert manifest['fixture_manifest_sha256']==sha(a.manifest)
    for case in manifest['cases']:
        original=next(c for c in source['cases'] if c['name']==case['name'])
        assert all(case[k]==original[k] for k in ('kind','sha256','bytes','expected'))
        if case['name'] in base_names:
            assert original==next(c for c in baseline['cases'] if c['name']==case['name'])
    expected={'windows-acceptance-kit/'+r['file']:r for r in files}
    archive=directory/'windows-acceptance-kit.zip'
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None
        assert set(z.namelist())==set(expected)|{'windows-acceptance-kit/KIT-FILES.json'}
        for name,r in expected.items():
            path=PurePosixPath(name);assert not path.is_absolute() and '..' not in path.parts
            data=z.read(name);assert hashlib.sha256(data).hexdigest()==r['sha256'] and len(data)==r['bytes']
            assert sha(kit/r['file'])==r['sha256']
    for c in manifest['cases']:assert sha(kit/c['file'])==c['sha256']
    assert sha(ROOT/'tools/run_windows_acceptance.ps1')==sha(kit/'run.ps1')
    assert sha(ROOT/'target/diagnostics/Release/audio-meter.exe')==manifest['meter_sha256']
    delivery=(ROOT/manifest.get('delivery','dist/0.1.0-local-test-20261019-cloud')).resolve(strict=True)
    assert delivery.is_relative_to(ROOT/'dist')
    assert sha(delivery/'media-browser-portable.zip')==manifest['portable_sha256']
    assert json.loads((delivery/'delivery.json').read_text(encoding='utf8'))['exe_sha256']==manifest['exe_sha256']
    smoke=json.loads(a.smoke.read_text(encoding='utf-8-sig'));assert smoke['state']=='finished' and smoke['selected_cases_pass']
    if a.expect_full:
        assert smoke['full_suite_completed'] and smoke['checks_pass']
        assert len(smoke['rows'])==len(manifest['cases'])
        assert {c['name'] for c in smoke['rows']}=={c['name'] for c in manifest['cases']}
    else:
        assert not smoke['full_suite_completed'] and not smoke['checks_pass']
    assert smoke['exe_sha256']==manifest['exe_sha256'] and smoke['portable_sha256']==manifest['portable_sha256']
    assert smoke['powershell'].startswith('5.1.')
    assert len(smoke['rows'])==len({c['name'] for c in smoke['rows']})
    if not a.expect_full:
        assert len(a.partial_cases)==len(set(a.partial_cases)) and set(a.partial_cases)<set(source_names)
        assert {c['name'] for c in smoke['rows']}==set(a.partial_cases)
    assert all(c['passed'] and c['source_unchanged'] and c['exit_code']==0 for c in smoke['rows'])
    video_names={c['name'] for c in manifest['cases'] if c['kind']=='video'}
    for video in (c for c in smoke['rows'] if c['name'] in video_names):
        assert video['audio_peak']>.0001 and video['visible_seek_pixel_difference']>1
    result=dict(checks_pass=True,exe_sha256=manifest['exe_sha256'],zip_sha256=sha(archive),samples=len(manifest['cases']),kit_files=len(files)+1,partial_smoke=str(a.smoke),partial_smoke_cases=(0 if a.expect_full else len(smoke['rows'])),full_suite_completed=a.expect_full,native_cases=len(smoke['rows']),host_os=smoke['registry_os'],scope=f'Kit archive integrity and {len(smoke["rows"])} standalone PowerShell5.1 native cases on the recorded host; full_suite_completed only describes this format suite. No second-machine, clean-OS provenance, physical input/listening or complete product acceptance claim.')
    a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf8');print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
