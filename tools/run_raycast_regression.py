"""Freeze the production-input hashes and run the UI change's native regressions."""
from pathlib import Path
import subprocess, sys, json, hashlib, argparse
from package_local import production_inputs
ROOT = Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--prefix',default='raycast-final');parser.add_argument('--exe',type=Path,default=ROOT/'dist/browser-preview/media-browser.exe');args=parser.parse_args();prefix=args.prefix;assert prefix.replace('-','').isalnum()
exe = args.exe.resolve(strict=True)
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
before = production_inputs()
initial = sha(exe)
rows = []
commands = [
    ['run_design_smoke.py','--exe',str(exe),'--prefix',prefix+'-design'],
    ['run_tree_motion_smoke.py','--exe',str(exe),'--prefix',prefix+'-tree'],
    ['run_delivery_interaction_recheck.py','--exe',str(exe),'--prefix',prefix],
]
for command in commands:
    log = ROOT/'artifacts'/(prefix+'-'+Path(command[0]).stem+'.log')
    with log.open('wb') as stream:
        code = subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools'/command[0]),*command[1:]],cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT).returncode
    rows.append(dict(command=command[0],exit_code=code,log=str(log)))
    print(command[0],code,flush=True)
    if code: break
result = dict(checks_pass=len(rows)==len(commands) and all(r['exit_code']==0 for r in rows) and initial==sha(exe) and before==production_inputs(),exe_sha256=initial,production_source_sha256=before,rows=rows)
(ROOT/('artifacts/'+prefix+'-regressions.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
assert result['checks_pass'],rows
