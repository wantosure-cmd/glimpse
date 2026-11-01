import hashlib,json,pathlib,argparse
root=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--evidence-dir',type=pathlib.Path,default=root/'artifacts')
parser.add_argument('--exe',type=pathlib.Path,default=root/'dist/browser-preview/media-browser.exe')
args=parser.parse_args();evidence=args.evidence_dir.resolve(strict=True);exe=args.exe.resolve(strict=True)
def read(name):return json.loads((evidence/name).read_text(encoding='utf-8-sig'))
run=read('p3-playback-run.json')
assert run['exit_code']==0
assert hashlib.sha256(exe.read_bytes()).hexdigest().upper()==run['exe_sha256']
result=read('p3-playback-smoke.json')
assert result['error'] is None,result
rows={r['phase']:r for r in result['rows']}
for row in rows.values():
    assert row['snapshot_media']==20 and row['prepared_media']==20,row
    assert row['shown']=={'images':17,'videos':3},row
initial=rows['initial_detail']['player']
assert initial['ready'] and initial['mute']=='no' and abs(initial['volume']-70)<0.1,initial
pending=rows['five_pending_seeks']['pending']
assert abs(pending-(initial['time']+25))<0.3,(initial,pending)
paused=rows['paused_seek_result']['player']
assert paused['ready'] and paused['pause']=='yes',paused
assert abs(paused['time']-pending)<0.6,(paused,pending)
reopened=rows['reopen_preferences_and_position']['player']
assert reopened['ready'] and reopened['mute']=='yes' and abs(reopened['volume']-23)<0.1,reopened
assert reopened['time']>=paused['time']-0.6,reopened
other=rows['different_source']['player']
assert other['ready'] and other['source'].lower().endswith('.avi'),other
reloaded=rows['same_source_after_stop_and_reload']['player']
assert reloaded['ready'] and reloaded['source'].lower().endswith('.mp4'),reloaded
assert reloaded['mute']=='yes' and abs(reloaded['volume']-23)<0.1,reloaded
assert read('p3-source-hashes-before.json')==read('p3-source-hashes-after.json')
config=json.loads((root/run['data_dir']/'settings.json').read_text(encoding='utf8'))
assert config['muted'] and config['volume']==23,config
summary={'checks_pass':True,'scope':result['scope'],'exe_sha256':run['exe_sha256'],'initial_time':initial['time'],'five_seek_target':pending,'paused_position':paused['time'],'source_files_unchanged':20,'volume_preference_persisted':23,'muted_preference_persisted':True}
(evidence/'p3-playback-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8')
print(json.dumps(summary,indent=2))
