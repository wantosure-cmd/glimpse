"""Compile the current bilingual macros, historical literals and negative fixtures."""
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
base=json.loads((ROOT/'assets/i18n/zh-CN.json').read_text(encoding='utf-8'))
english=json.loads((ROOT/'assets/i18n/en.json').read_text(encoding='utf-8'))
play=next(k for k,v in base['messages'].items() if v['source']=='播放')
opened=next(k for k,v in base['messages'].items() if v['source']=='无法打开目录：{e}')
lines=['#![allow(dead_code)]','#[path='+catalog.rust_string(str(production))+'] mod ui_text;','fn main() {','let e="sample";']
for enabled,locale in [(False,base),(True,english),(False,base)]:
    lines += ['ui_text::set_english('+str(enabled).lower()+');']
    for key,item in sorted(locale['messages'].items()):
        lines += [f'assert_eq!(ui_text!({key}), {catalog.rust_string(item["text"])});']
    expected=locale['messages'][opened]['text'].replace('{e}','sample')
    undo=locale['messages']['msg_media_delete_undo']['text'].replace('{count}','2').replace('{seconds}','3')
    lines += [f'assert_eq!(ui_format!({opened}, e=e), {catalog.rust_string(expected)});',
              f'assert_eq!(ui_format!(msg_media_delete_undo, count=2, seconds=3), {catalog.rust_string(undo)});']
lines += ['}']
probe=out/'translation.rs';probe.write_text('\n'.join(lines)+'\n',encoding='utf-8')
run(['rustc','--edition','2024','--crate-name','ui_translation_probe',probe,'-o',out/'translation.exe']);run([out/'translation.exe'])
bad=copy.deepcopy(base);bad['messages'][opened]['text']='Cannot open directory'
badfile=out/'bad-parameters.json';badfile.write_text(json.dumps(bad,ensure_ascii=False),encoding='utf-8');bad_output=out/'must-not-be-generated.rs'
rejected=run([sys.executable,'-X','utf8',ROOT/'tools/ui_catalog.py','generate','--catalog',badfile,'--rust-output',bad_output],expected=1)
assert b'Format capture signature changed' in rejected.stderr and not bad_output.exists()
unknown=out/'unknown.rs';unknown.write_text('#[path='+catalog.rust_string(str(production))+'] mod ui_text; fn main(){let _=ui_text!(this_message_does_not_exist);}',encoding='utf8')
rejected_id=run(['rustc','--edition','2024',unknown,'-o',out/'unknown.exe'],expected=1)
assert b'Unknown UI message ID' in rejected_id.stderr and not (out/'unknown.exe').exists()
assert sha(production)==before,'Translation fixture changed the shipped Chinese catalog'
result={'checks_pass':True,'messages':len(base['messages']),'references':sum(len(v.get('references',[])) for v in base['messages'].values()),'locale':'zh-CN','fixture':str(out),'literal_equivalence':eq.stdout.decode().strip(),'runtime_language_switch_probe':True,'all_messages_zh_en_zh_checked':True,'named_format_arguments_checked':True,'unknown_message_id_rejected':True,'missing_named_parameter_rejected':True,'production_catalog_unchanged':True,'production_catalog_sha256':before,'catalog_sha256':sha(ROOT/'assets/i18n/zh-CN.json'),'scope':'Historical literal/source byte equality and current catalog emission; all messages switched zh/en/zh in compiled Rust, named format arguments and negative generation/compile checks. Not native UI or physical input proof.'}
(ROOT/'artifacts/ui-text-catalog-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
