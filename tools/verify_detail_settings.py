"""Check old-config upgrade and EOF policy persistence using isolated settings."""
import hashlib,json,subprocess,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
exe=ROOT/'target/release/media-browser.exe';task=ROOT/'target'/('detail-settings-'+uuid.uuid4().hex);task.mkdir()
rows=[]
for input_value in [None,'Next','Replay','Pause']:
    data=task/str(input_value);data.mkdir()
    config={'volume':23.0,'muted':True}
    if input_value is not None:config['video_end_action']=input_value
    (data/'settings.json').write_text(json.dumps(config),encoding='utf-8')
    p=subprocess.run([str(exe),'--instance-scope','detail-test-'+uuid.uuid4().hex,'--data-dir',str(data),'--quit-after','400'],capture_output=True,timeout=30)
    assert p.returncode==0,(p.returncode,p.stderr)
    saved=json.loads((data/'settings.json').read_text(encoding='utf-8'))
    assert saved['video_end_action']==(input_value or 'Next') and saved['volume']==23 and saved['muted'] is True,saved
    rows.append({'input':config,'saved':saved,'exit_code':p.returncode})
result={'checks_pass':True,'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'fixture':str(task),'rows':rows,'scope':'Isolated config load and normal-exit persistence, not settings-menu physical-input verification'}
(ROOT/'artifacts/detail-settings-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print('4 isolated config roundtrips passed')
