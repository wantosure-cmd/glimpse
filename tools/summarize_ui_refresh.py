import hashlib,json,pathlib,statistics,subprocess
root=pathlib.Path('.').resolve()
def read(name): return json.loads((root/'artifacts'/name).read_text(encoding='utf-8-sig'))
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest().upper()
exe=root/'dist/browser-preview/media-browser.exe'
ui=read('p2-ui-verification.json');control=read('p3-input-verification.json');bench=read('ui-large-jpeg-benchmark.json')
assert ui['checks_pass'] and control['checks_pass']
assert ui['exe_sha256']==control['exe_sha256']==digest(exe)==digest(root/'target/release/media-browser.exe')
source_files=[*root.glob('src/**/*.rs'),root/'Cargo.toml',root/'Cargo.lock',root/'assets/ui-chinese.ttf']
assert exe.stat().st_mtime >= max(p.stat().st_mtime for p in source_files), 'executable predates source'
source=pathlib.Path(bench['source']);assert digest(source).lower()==bench['source_sha256']
rows=bench['rows'];assert len(rows)==3 and all(r['intermediate_dimensions']==[1000,750] for r in rows)
before=statistics.median(r['baseline_decode_resize_ms'] for r in rows);after=statistics.median(r['optimized_decode_resize_ms'] for r in rows)
log=(root/'artifacts-ui-unit-tests.log').read_text(encoding='utf-8-sig');assert '6 passed; 0 failed; 2 ignored' in log
summary={'checks_pass':True,'exe_sha256':digest(exe),'source_sha256':{str(p.relative_to(root)):digest(p) for p in source_files},'compiler':subprocess.check_output(['rustc','--version'],text=True).strip(),'native_ui':ui,'input':control,'jpeg_benchmark':{'scope':bench['scope'],'source_sha256':bench['source_sha256'],'baseline_median_ms':before,'optimized_median_ms':after,'speedup':before/after,'elapsed_reduction_percent':100*(1-after/before),'png_baseline_median_ms':statistics.median(r['baseline_png_ms'] for r in rows),'png_optimized_median_ms':statistics.median(r['optimized_png_ms'] for r in rows)},'limits':['Synthetic native input, not physical mouse or OS focus proof','Audio session signal, not speaker listening test','JPEG benchmark excludes GPU upload and presentation; no current 5000-file full-render performance claim','Delete UI, Explorer integration, single instance, Win11 and final installers remain incomplete']}
(root/'artifacts/ui-refresh-verification.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in summary.items() if k!='source_sha256'},ensure_ascii=False,indent=2))
