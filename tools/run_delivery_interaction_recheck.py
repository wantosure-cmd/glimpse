"""Run existing native input/player diagnostics without replacing delivery binaries."""
from pathlib import Path
import hashlib,json,subprocess,sys,uuid,datetime,argparse

ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--exe',type=Path,default=ROOT/'dist/0.1.0-local-test-20261020-delete-status/media-browser-portable/media-browser.exe');parser.add_argument('--prefix',default='deletefinal');args=parser.parse_args();EXE=args.exe.resolve(strict=True);prefix=args.prefix;assert prefix.replace('-','').replace('_','').isalnum()
OUT=ROOT/'artifacts'/('delivery-interactions-'+uuid.uuid4().hex)
OUT.mkdir()

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def sources(folder):
    return [dict(Path=str(p),Hash=sha(p).upper()) for p in sorted(folder.rglob('*')) if p.is_file()]

summary=dict(exe_sha256=sha(EXE),directory=str(OUT),checks_pass=False,rows=[])
(ROOT/'artifacts'/(prefix+'-interaction-running.json')).write_text(json.dumps(summary,indent=2),encoding='utf-8')
for mode,fixture,hashnames,verifier in [
    ('input','input-demo',('p3-input-hashes-before.json','p3-input-hashes-after.json'),'verify_input_smoke.py'),
    ('playback','ui-demo',('p3-source-hashes-before.json','p3-source-hashes-after.json'),'verify_playback_smoke.py')]:
    work=OUT/mode;work.mkdir();evidence=work/'artifacts';evidence.mkdir()
    data=ROOT/'target'/('delivery-'+mode+'-'+uuid.uuid4().hex)
    fixture=ROOT/'test-media'/fixture
    before=sources(fixture)
    (evidence/hashnames[0]).write_text(json.dumps(before),encoding='utf-8')
    began=stamp()
    with (evidence/'stdout.log').open('wb') as stdout,(evidence/'stderr.log').open('wb') as stderr:
        app=subprocess.Popen([str(EXE),'--'+mode+'-smoke','--open',str(fixture),'--data-dir',str(data)],cwd=work,stdout=stdout,stderr=stderr)
        meter=None
        with (evidence/'ui-input-audio-peaks.json').open('wb') as audio,(evidence/'audio-error.log').open('wb') as audioerr:
            if mode=='input':
                meter=subprocess.Popen([str(ROOT/'target/diagnostics/Release/audio-meter.exe'),str(app.pid)],cwd=work,stdout=audio,stderr=audioerr)
            try:
                code=app.wait(timeout=180)
                if meter:meter.wait(timeout=30)
            except subprocess.TimeoutExpired:
                # Keep the process and evidence intact; inspect the PID before further work.
                (evidence/'timeout.json').write_text(json.dumps(dict(pid=app.pid,started_utc=began)),encoding='utf-8')
                raise
    after=sources(fixture)
    (evidence/hashnames[1]).write_text(json.dumps(after),encoding='utf-8')
    run=dict(exit_code=code,started_utc=began,ended_utc=stamp(),exe_sha256=sha(EXE).upper(),data_dir=str(data),pid=app.pid)
    (evidence/('p3-'+mode+'-run.json')).write_text(json.dumps(run),encoding='utf-8')
    assert before==after,'Read-only diagnostic changed source files'
    raw=evidence/('p3-'+mode+'-smoke.json');assert raw.exists()
    with (evidence/'verifier.log').open('wb') as log:
        check=subprocess.run([sys.executable,'-X','utf8',str(ROOT/'tools'/verifier),'--evidence-dir',str(evidence),'--exe',str(EXE)],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
    summary['rows'].append(dict(mode=mode,evidence=str(evidence),pid=app.pid,exit_code=code,verifier_exit_code=check.returncode,source_files=len(before),source_unchanged=before==after))
    (ROOT/'artifacts'/(prefix+'-interaction-recheck.json')).write_text(json.dumps(summary,indent=2),encoding='utf-8')
    assert check.returncode==0,(mode,str(evidence/'verifier.log'))
    print('PASS '+mode,flush=True)
summary['checks_pass']=True
(ROOT/'artifacts'/(prefix+'-interaction-recheck.json')).write_text(json.dumps(summary,indent=2),encoding='utf-8')
print(json.dumps(summary,indent=2))
