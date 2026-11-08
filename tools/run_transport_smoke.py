"""Generated PNG/real FLV shortcut and settings-restart native verification."""
from pathlib import Path
import argparse
import json
import shutil
import struct
import subprocess
import time
import tomllib
import uuid
import zlib
from run_daily_smoke import application_version, digest, chunk
from run_video_switch_smoke import stage_application

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    args = parser.parse_args()
    exe = args.exe.resolve(strict=True)
    assert application_version(exe) == tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf8'))['package']['version']
    task = ROOT/'target'/('transport-owned-'+uuid.uuid4().hex)
    media = task/'media'
    media.mkdir(parents=True)
    (task/'.transport-owned').write_bytes(b'generated transport fixtures only')
    staged, runtime, _, pins = stage_application(exe, task)
    png = (b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',320,240,8,2,0,0,0))+
           chunk(b'IDAT',zlib.compress((b'\0'+bytes((45,105,175))*320)*240))+chunk(b'IEND',b''))
    for name in ['01.png','03.png']:
        (media/name).write_bytes(png)
    fixture = json.loads((ROOT/'artifacts/flv56-fixture.json').read_text(encoding='utf8'))
    manifest = json.loads(Path(fixture['manifest']).read_text(encoding='utf8'))
    for case, name in zip(manifest['cases'], ['02.flv','04.FLV'], strict=True):
        source = ROOT/case['path']
        assert digest(source) == case['sha256']
        shutil.copyfile(source,media/name)
    before = {p.name:digest(p) for p in media.iterdir()}
    result = dict(checks_pass=False,exe_sha256=digest(staged),task=str(task),staged_exe=str(staged),runs=[])
    output = ROOT/'artifacts'/(task.name+'-verification.json')

    def save():
        output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf8')

    save()
    for restart in [False,True]:
        name = 'restart' if restart else 'keys'
        raw = task/(name+'.json')
        command = [str(staged),'--transport-smoke',str(raw),'--data-dir',str(task/'data'),'--instance-scope',task.name]
        command += ['--transport-restart'] if restart else ['--open',str(media)]
        with (task/(name+'-stdout.log')).open('xb') as stdout,(task/(name+'-stderr.log')).open('xb') as stderr:
            proc = subprocess.Popen(command,cwd=task,stdout=stdout,stderr=stderr)
            row = dict(name=name,pid=proc.pid,command=command,active=True)
            result['runs'].append(row)
            save()
            started = time.monotonic()
            while proc.poll() is None:
                if time.monotonic()-started > 120:
                    raise TimeoutError(f'Owned shortcut diagnostic still live PID {proc.pid}; inspect before retrying')
                time.sleep(.1)
            row.update(active=False,exit_code=proc.returncode,report=json.loads(raw.read_text(encoding='utf8')))
            save()
        assert row['exit_code']==0 and row['report']['checks_pass'],row['report']
        assert before == {p.name:digest(p) for p in media.iterdir()}
    rows=result['runs'][0]['report']['rows']
    assert sum(r['name'].startswith('key-') for r in rows)==26
    home_rows=[r for r in rows if r['name']=='key-Home']
    assert len(home_rows)==2 and all(r['player']['pause']=='no' and r['player']['time']<1.5 for r in home_rows)
    for expected in ['photo-ignores-video-controls','photo-left-previous','photo-right-next-flv1',
                     'unfocused-controls-blocked','command-editor-controls-blocked','save-selected-speed']:
        assert any(r['name']==expected for r in rows),expected
    assert any(r['name']=='restart-applies-selected-speed-to-mpv' for r in result['runs'][1]['report']['rows'])
    assert digest(exe)==digest(staged)==result['exe_sha256']
    assert all(digest(runtime/p['file'])==p['sha256'].lower() for p in pins)
    result.update(checks_pass=True,source_files_unchanged=True,normal_and_fullscreen_key_cases=26,settings_restart_pass=True)
    save()
    print(json.dumps(dict(checks_pass=True,evidence=str(output),exe_sha256=result['exe_sha256'],normal_and_fullscreen_key_cases=26,settings_restart_pass=True),ensure_ascii=False))


if __name__=='__main__':
    main()
