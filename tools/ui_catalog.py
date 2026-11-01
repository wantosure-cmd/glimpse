"""Maintain compiled bilingual UI messages. Python is a developer tool, never an app dependency.
`inventory` reports Chinese messages outside test/diagnostic code; `migrate` is one-shot,
backing up every edited source. `generate` and `check` keep the committed Rust catalog
and JSON in sync. Both locales compile into static strings; ui_format! preserves format type checking.
"""
from __future__ import annotations
import argparse, functools, hashlib, json, re, shutil, string, uuid
from dataclasses import dataclass
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
CATALOG=ROOT/'assets/i18n/zh-CN.json'
RUST=ROOT/'src/ui_text.rs'
LIB={'animation','color','domain','index','input','instance','layout','library','media_path','photo_rotate','playback_control','prepare','scanner','shell_menu','shortcuts','trash','view','watch'}
FORMATS={'format','format_args','println','eprintln','print','eprint','panic','write','writeln','assert','assert_eq','assert_ne','debug_assert','debug_assert_eq','debug_assert_ne'}
@dataclass
class Token:
    kind:str; text:str; start:int; end:int

def tokens(s):
    out=[];i=0
    while i<len(s):
        if s[i].isspace():i+=1;continue
        if s.startswith('//',i):
            j=s.find('\n',i);i=len(s) if j<0 else j;continue
        if s.startswith('/*',i):
            depth=1;j=i+2
            while depth:
                if j>=len(s):raise ValueError('Unclosed block comment')
                if s.startswith('/*',j):depth+=1;j+=2
                elif s.startswith('*/',j):depth-=1;j+=2
                else:j+=1
            i=j;continue
        raw=re.match(r'(br|cr|r)(#*)"',s[i:])
        if raw:
            ending='"'+raw[2];j=s.find(ending,i+len(raw[0]));assert j>=0
            end=j+len(ending);out.append(Token('str' if raw[1]=='r' else 'other_string',s[i:end],i,end));i=end;continue
        prefix=1 if s[i:i+1]=='"' else 2 if s[i:i+2] in ('b"','c"') else 0
        if prefix:
            j=i+prefix
            while j<len(s):
                if s[j]=='\\':j+=2
                elif s[j]=='"':j+=1;break
                else:j+=1
            else:raise ValueError('Unclosed string')
            out.append(Token('str' if prefix==1 else 'other_string',s[i:j],i,j));i=j;continue
        char=re.match(r"'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.))'",s[i:])
        if char:
            j=i+len(char[0]);out.append(Token('char',s[i:j],i,j));i=j;continue
        ident=re.match(r'(?:r#)?[^\W\d]\w*',s[i:],re.UNICODE)
        if ident:
            j=i+len(ident[0]);out.append(Token('ident',s[i:j],i,j));i=j;continue
        out.append(Token('punct',s[i],i,i+1));i+=1
    return out

def pairs(ts):
    stack=[];result={};closing={')':'(',']':'[','}':'{'}
    for i,t in enumerate(ts):
        if t.kind!='punct':continue
        if t.text in ('(','[','{'):stack.append(i)
        elif t.text in closing:
            assert stack and ts[stack[-1]].text==closing[t.text],(i,t)
            j=stack.pop();result[i]=j;result[j]=i
    assert not stack
    return result

def decode(literal):
    if literal.startswith('r'):
        m=re.match(r'r(#*)"',literal);assert m;return literal[len(m[0]):-len(m[1])-1]
    s=literal[1:-1];out=[];i=0
    table={'n':'\n','r':'\r','t':'\t','0':'\0','\\':'\\','"':'"',"'":"'"}
    while i<len(s):
        if s[i]!='\\':out.append(s[i]);i+=1;continue
        i+=1;c=s[i];i+=1
        if c in table:out.append(table[c])
        elif c=='x':out.append(chr(int(s[i:i+2],16)));i+=2
        elif c=='u':
            assert s[i]=='{';j=s.index('}',i);out.append(chr(int(s[i+1:j].replace('_',''),16)));i=j+1
        elif c in '\n\r':
            while i<len(s) and s[i].isspace():i+=1
        else:raise ValueError(f'Unsupported Rust string escape {c!r}')
    return ''.join(out)

def rust_string(s):
    table={'"':'\\"','\\':'\\\\','\n':'\\n','\r':'\\r','\t':'\\t','\0':'\\0'}
    return '"'+''.join(table.get(c, '\\u{'+format(ord(c),'x')+'}' if ord(c)<32 else c) for c in s)+'"'

def chinese(s):return any(0x3400<=ord(c)<=0x9fff or 0x20000<=ord(c)<=0x3134f for c in s)
@functools.lru_cache(maxsize=1)
def technical_sources():
    return set(json.loads(CATALOG.read_text(encoding='utf-8')).get('managed_technical_sources',[])) if CATALOG.exists() else set()
def excluded(ts,links,path):
    spans=[]
    for i,t in enumerate(ts):
        if t.text=='#' and i+1<len(ts) and ts[i+1].text=='[':
            end=links[i+1];a=ts[i+2:end]
            if not (a and (a[0].text=='test' or (a[0].text=='cfg' and any(x.text=='test' for x in a) and not any(x.text=='not' for x in a)))):continue
            j=end+1
            while j<len(ts) and ts[j].text not in ('fn','mod',';'):j+=1
            while j<len(ts) and ts[j].text not in ('{',';'):j+=1
            if j<len(ts) and ts[j].text=='{':spans.append((ts[j].start,ts[links[j]].end))
        if path.name in ('browser.rs','prototype.rs') and t.text=='fn' and i+1<len(ts) and ts[i+1].text=='smoke':
            j=i+2
            while j<len(ts) and ts[j].text!='{':j+=1
            spans.append((ts[j].start,ts[links[j]].end))
    return spans

def scan(path):
    s=path.read_text(encoding='utf-8');ts=tokens(s);links=pairs(ts);skip=excluded(ts,links,path)
    items=[(i,t,decode(t.text)) for i,t in enumerate(ts) if t.kind=='str' and (chinese(decode(t.text)) or decode(t.text) in technical_sources()) and not any(a<=t.start<b for a,b in skip)]
    return s,ts,links,items

def sources():
    return [p for p in sorted((ROOT/'src').rglob('*.rs')) if p!=ROOT/'src/ui_text.rs' and not p.name.endswith('_smoke.rs')]

def validate(c):
    assert c['schema_version']==1 and re.fullmatch(r'[a-z]{2}(?:-[A-Za-z]{2,4})?',c['locale']), 'Invalid locale/schema'
    for key,item in c['messages'].items():
        assert re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*',key), f'Invalid message ID: {key}'
        assert isinstance(item['text'],str) and isinstance(item['source'],str)
        if item.get('format_template'):
            assert fields(item['text'])==fields(item['source']), f'Format capture signature changed: {key}'

def render(c):
    validate(c)
    en_path=ROOT/'assets/i18n/en.json'
    en=json.loads(en_path.read_text(encoding='utf-8')) if en_path.exists() else c
    validate(en)
    assert set(en['messages'])==set(c['messages']), 'Locale message IDs differ'
    lines=['//! Generated bilingual catalog. Edit assets/i18n JSON, not this file.',
           'use std::sync::atomic::{AtomicBool, Ordering};',
           'static ENGLISH: AtomicBool = AtomicBool::new(false);',
           'pub const DEFAULT_LOCALE: &str = "zh-CN";',
           'pub fn set_english(value: bool) { ENGLISH.store(value, Ordering::Relaxed); }',
           'pub fn english() -> bool { ENGLISH.load(Ordering::Relaxed) }',
           '#[macro_export]', 'macro_rules! ui_text {']
    for key,item in sorted(c['messages'].items()):
        translated=en['messages'][key]['text']
        def signature(text):
            return [(f,spec) for _,f,spec,_ in string.Formatter().parse(text) if f is not None]
        assert sorted(signature(item['text']))==sorted(signature(translated)), f'Placeholder mismatch: {key}'
        lines.append(f"    ({key}) => {{ if $crate::ui_text::english() {{ {rust_string(translated)} }} else {{ {rust_string(item['text'])} }} }};")
    lines+=['    ($unknown:ident) => { compile_error!(concat!("Unknown UI message ID: ", stringify!($unknown))) };','}', '#[macro_export]', 'macro_rules! ui_format {']
    for key,item in sorted(c['messages'].items()):
        translated=en['messages'][key]['text']
        lines.append(f"    ({key}, $($args:tt)*) => {{ if $crate::ui_text::english() {{ format!({rust_string(translated)} , $($args)*) }} else {{ format!({rust_string(item['text'])} , $($args)*) }} }};")
    lines+=['}', 'pub fn variants(id: &str) -> (&\'static str, &\'static str) { match id {']
    for key,item in sorted(c['messages'].items()):
        lines.append(f'    {rust_string(key)} => ({rust_string(item["text"])}, {rust_string(en["messages"][key]["text"])}),')
    lines+=['    _ => ("", ""),', '} }', '']
    return '\n'.join(lines)

def fields(text):
    names=set()
    for _,field,spec,_ in string.Formatter().parse(text):
        if field and not field.isdecimal():
            if not re.fullmatch(r'[A-Za-z_]\w*',field):raise ValueError(f'Unsupported format capture {field}')
            names.add(field)
        if spec:names.update(re.findall(r'([A-Za-z_]\w*)\$',spec))
    return names

def migrate(adopt=False):
    if adopt:
        c=json.loads(CATALOG.read_text(encoding='utf-8'));messages=c['messages']
    else:
        assert not CATALOG.exists() and not RUST.exists(),'Catalog exists; use adopt/generate/check, not repeat migration'
        c={'locale':'zh-CN','schema_version':1,'strategy':'Compile-time static Rust literals. IDs remain stable when text/translations change. User paths, protocol identifiers, icons, diagnostics fixtures and third-party raw errors are not translations.'};messages={}
    changes={};stats=[]
    for p in sources():
        s,ts,links,items=scan(p)
        if not items:continue
        edits=[];format_edits={};prefix='crate' if p.parent==ROOT/'src' and p.stem in LIB else 'media_browser'
        for i,t,text in items:
            key='msg_'+hashlib.sha256(text.encode()).hexdigest()[:12]
            item=messages.setdefault(key,{'text':text,'source':text,'references':[],'original_literals':[]})
            assert item['source']==text,'Message ID collision'
            if t.text not in item['original_literals']:item['original_literals'].append(t.text)
            item['references'].append({'file':p.relative_to(ROOT).as_posix(),'original_line':s.count('\n',0,t.start)+1})
            edits.append((t.start,t.end,f'{prefix}::ui_text!({key})'))
            # Only literals that are a formatting macro's first top-level string need capture expansion.
            for opening,closing in links.items():
                if opening>=closing or not(opening<i<closing) or opening<2:continue
                if ts[opening-1].text!='!' or ts[opening-2].text not in FORMATS:continue
                top=[];j=opening+1
                while j<closing:
                    top.append(j)
                    j=links[j]+1 if ts[j].text in ('(','[','{') else j+1
                literal=next((j for j in top if ts[j].kind=='str'),None)
                if literal!=i:continue
                item['format_template']=True
                # Top-level `name = expression` arguments already bind the name explicitly.
                explicit={ts[top[k]].text for k in range(len(top)-1) if ts[top[k]].kind=='ident' and ts[top[k+1]].text=='='}
                missing=fields(text)-explicit
                if missing:
                    assert not any(n in {'self','Self','super','crate','type','match','ref','mod'} for n in missing),f'Manual binding required: {p}:{missing}'
                    suffix=('' if ts[closing-1].text==',' else ', ')+', '.join(f'{n} = {n}' for n in sorted(missing))
                    previous=format_edits.setdefault(closing,(ts[closing].start,ts[closing].start,suffix))
                    assert previous[2]==suffix
                break
        edits+=list(format_edits.values())
        changed=s
        for a,b,value in sorted(edits,reverse=True):changed=changed[:a]+value+changed[b:]
        changes[p]=changed;stats.append({'file':p.relative_to(ROOT).as_posix(),'messages':len(items),'format_macros_with_explicit_captures':len(format_edits),'before_sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
    c['messages']=messages
    # Inspect all planned transformations and back up original bytes before changing product code.
    backup=ROOT/'target'/('ui-text-before-'+uuid.uuid4().hex);backup.mkdir()
    for p in changes:
        dest=backup/p.relative_to(ROOT);dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,dest)
    report={'backup':str(backup),'files':stats,'messages':len(messages),'default_text_unchanged':all(v['text']==v['source'] for v in messages.values())}
    (ROOT/'artifacts'/('ui-text-adoption.json' if adopt else 'ui-text-migration.json')).write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    CATALOG.parent.mkdir(parents=True,exist_ok=True);CATALOG.write_text(json.dumps(c,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');RUST.write_text(render(c),encoding='utf-8')
    for p,s in changes.items():p.write_text(s,encoding='utf-8')
    print(json.dumps({'files':len(changes),'messages':len(messages),'references':sum(x['messages'] for x in stats),'backup':str(backup)},ensure_ascii=False))

def check():
    c=json.loads(CATALOG.read_text(encoding='utf-8'));validate(c)
    assert RUST.read_text(encoding='utf-8')==render(c),'Generated Rust catalog is stale; run generate'
    remaining=[]
    for p in sources():
        s,ts,_,items=scan(p)
        remaining.extend({'file':str(p.relative_to(ROOT)),'line':s.count('\n',0,t.start)+1,'literal':t.text} for _,t,_ in items)
        for i,t in enumerate(ts[:-3]):
            if t.text=='ui_text' and ts[i+1].text=='!' and ts[i+2].text=='(':
                assert ts[i+3].text in c['messages'], f'Unknown message ID {p}:{ts[i+3].text}'
    assert not remaining,json.dumps(remaining,ensure_ascii=False,indent=2)
    print(json.dumps({'checks_pass':True,'locale':c['locale'],'messages':len(c['messages']),'unmanaged_chinese_or_owned_technical_literals':0},ensure_ascii=False))

def equivalence():
    c=json.loads(CATALOG.read_text(encoding='utf-8'));p=ROOT/'target/ui-catalog-equivalence.rs'
    lines=['#![allow(dead_code)]',f'#[path={rust_string(str(RUST))}] mod ui_text;','fn main() {','let pairs: &[(&str, &str)] = &[']
    for key,item in sorted(c['messages'].items()):
        for literal in item.get('original_literals',[rust_string(item['source'])]):lines.append(f'    ({literal}, ui_text!({key})),')
    lines+= ['];','for (old, new) in pairs { assert_eq!(old, new); }','println!("Rust literal equivalence passed: {}", pairs.len());','}']
    p.write_text('\n'.join(lines)+'\n',encoding='utf-8');print(p)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('command',choices=['inventory','migrate','adopt','generate','check','equivalence'])
    p.add_argument('--catalog',type=Path,default=CATALOG,help='Select a complete catalog at build/maintenance time, not at application startup')
    p.add_argument('--rust-output',type=Path,default=RUST,help='Generated Rust output; default is the committed production catalog')
    a=p.parse_args();CATALOG=a.catalog.resolve();RUST=a.rust_output.resolve()
    if a.command=='inventory':
        rows=[]
        for path in sources():
            s,_,_,items=scan(path)
            rows.extend({'file':str(path.relative_to(ROOT)),'line':s.count('\n',0,t.start)+1,'literal':t.text} for _,t,_ in items)
        print(json.dumps(rows,ensure_ascii=False,indent=2))
    elif a.command=='migrate':migrate()
    elif a.command=='adopt':migrate(True)
    elif a.command=='generate':RUST.write_text(render(json.loads(CATALOG.read_text(encoding='utf-8'))),encoding='utf-8')
    elif a.command=='check':check()
    else:equivalence()
