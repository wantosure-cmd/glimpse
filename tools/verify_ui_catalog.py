"""Real Rust literal/format checks and build-time translation preflight; no runtime locale switch."""
import copy,hashlib,json,subprocess,sys,uuid
from pathlib import Path
import ui_catalog as catalog
ROOT=Path(__file__).resolve().parent.parent
out=ROOT/'target'/('ui-catalog-tests-'+uuid.uuid4().hex);out.mkdir();(out/'.ui-catalog-owned').write_text('Owned static catalog tests',encoding='ascii')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def run(args, expected=0):
    p=subprocess.run(list(map(str,args)),cwd=ROOT,capture_output=True,timeout=90)
    assert (p.returncode==0)==(expected==0),(args,p.returncode,p.stderr.decode('utf-8',errors='replace'))
    return p
production=ROOT/'src/ui_text.rs';before=sha(production)
checked=run([sys.executable,'-X','utf8',ROOT/'tools/ui_catalog.py','check'])
run([sys.executable,'-X','utf8',ROOT/'tools/ui_catalog.py','equivalence'])
run(['rustc','--edition','2024','--crate-name','ui_catalog_equivalence',ROOT/'target/ui-catalog-equivalence.rs','-o',out/'equivalence.exe'])
eq=run([out/'equivalence.exe'])
base=json.loads((ROOT/'assets/i18n/zh-CN.json').read_text(encoding='utf-8'));english=copy.deepcopy(base);english['locale']='en-US'
play=next(k for k,v in base['messages'].items() if v['source']=='播放')
opened=next(k for k,v in base['messages'].items() if v['source']=='无法打开目录：{e}')
english['messages'][play]['text']='Play';english['messages'][opened]['text']='Cannot open directory: {e}'
file=out/'en-US.json';file.write_text(json.dumps(english,ensure_ascii=False,indent=2),encoding='utf-8');rust=out/'en-US.rs'
run([sys.executable,'-X','utf8',ROOT/'tools/ui_catalog.py','generate','--catalog',file,'--rust-output',rust])
run([sys.executable,'-X','utf8',ROOT/'tools/ui_catalog.py','check','--catalog',file,'--rust-output',rust])
probe=out/'translation.rs';probe.write_text('#![allow(dead_code)]\n#[path='+catalog.rust_string(str(rust))+'] mod ui_text;\nfn main(){assert_eq!(ui_text::DEFAULT_LOCALE,"en-US");assert_eq!(ui_text!('+play+'),"Play");let e="sample";assert_eq!(format!(ui_text!('+opened+'),e=e),"Cannot open directory: sample");}\n',encoding='utf-8')
run(['rustc','--edition','2024','--crate-name','ui_translation_probe',probe,'-o',out/'translation.exe']);run([out/'translation.exe'])
bad=copy.deepcopy(english);bad['messages'][opened]['text']='Cannot open directory'
badfile=out/'bad-parameters.json';badfile.write_text(json.dumps(bad,ensure_ascii=False),encoding='utf-8');bad_output=out/'must-not-be-generated.rs'
rejected=run([sys.executable,'-X','utf8',ROOT/'tools/ui_catalog.py','generate','--catalog',badfile,'--rust-output',bad_output],expected=1)
assert b'Format capture signature changed' in rejected.stderr and not bad_output.exists()
assert sha(production)==before,'Translation fixture changed the shipped Chinese catalog'
result={'checks_pass':True,'messages':len(base['messages']),'references':sum(len(v.get('references',[])) for v in base['messages'].values()),'locale':'zh-CN','fixture':str(out),'literal_equivalence':eq.stdout.decode().strip(),'build_time_translation_probe':True,'missing_named_parameter_rejected':True,'production_catalog_unchanged':True,'production_catalog_sha256':before,'catalog_sha256':sha(ROOT/'assets/i18n/zh-CN.json'),'scope':'Static default-message byte equality, known IDs, code generation, Rust types/format arguments, optional build-time translation generation. No runtime language switching or OS input proof.'}
(ROOT/'artifacts/ui-text-catalog-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
