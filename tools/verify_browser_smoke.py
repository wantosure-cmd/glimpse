import json,pathlib,hashlib
root=pathlib.Path(__file__).resolve().parent.parent
def read(name):return json.loads((root/'artifacts'/name).read_text(encoding='utf-8-sig'))
run=read('p2-browser-run.json')
assert run['exit_code']==0
assert hashlib.sha256((root/'dist/browser-preview/media-browser.exe').read_bytes()).hexdigest().upper()==run['exe_sha256']
result=read('p2-browser-smoke.json')
steps={r['step']:r for r in result['steps']}
assert (steps[0]['media'],steps[0]['shown_images'],steps[0]['shown_videos'],steps[0]['groups'])==(20,17,3,5)
assert steps[2]['detail'] is not None
assert (steps[4]['shown_images'],steps[4]['shown_videos'],steps[4]['groups'])==(0,3,2)
assert (steps[6]['media'],steps[6]['shown_images'],steps[6]['shown_videos'])==(2,1,1)
assert result['preview_failures']==1
assert result['texture_bytes']<=256*1024*1024
assert result['full_font_loaded']
assert read('p2-source-hashes-before.json')==read('p2-source-hashes-after.json')
summary={'scope':'programmatic native UI smoke; physical input and 5000-file rendering performance still require separate evidence','checks_pass':True,'exe_sha256':run['exe_sha256'],'source_files_unchanged':20,'texture_bytes':result['texture_bytes'],'full_font_loaded':result['full_font_loaded'],'expected_corrupt_image_failures':result['preview_failures']}
(root/'artifacts/p2-ui-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8')
print(json.dumps(summary,indent=2))
