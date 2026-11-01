import hashlib,json,pathlib
root=pathlib.Path(__file__).resolve().parent.parent
data=json.loads((root/'artifacts/p4-system-integration.json').read_text(encoding='utf-8-sig'))
assert data['exe_sha256']==data['copied_exe_sha256']==hashlib.sha256(pathlib.Path(data['exe_path']).read_bytes()).hexdigest().upper()
assert data['primary_exit_code']==data['directory_shell_exit']==data['drive_shell_exit']==0
rows=data['rows'];assert [r['received'] for r in rows]==list(range(7)) and all(r['media']==1 for r in rows)
assert rows[5]['generation']==rows[6]['generation'],'Activation should not navigate again'
visibility=data['visibility'];hidden=next(i for i,r in enumerate(visibility) if r['minimized']);assert any(not r['minimized'] for r in visibility[hidden+1:]) and not rows[1]['minimized']
assert data['drive_removed'] and data['registry_removed'] and data['reopened']['pid']!=data['primary_pid']
assert len(data['reopened']['rows'])>=1 and data['reopened']['rows'][0]['received']==0
for client in data['clients']:
    if client['label']=='reject-file':assert client['exit_code']!=0;continue
    assert client['exit_code']==0
    forwarded=json.loads(client['stdout']);assert forwarded['forwarded'] and forwarded['server_pid']==data['primary_pid'] and forwarded['client_pid']!=data['primary_pid']
assert len(data['registry_readback'])==3
for row in data['registry_readback']:
    command=row['command'];assert command.startswith('"'+data['copied_exe']+'" --instance-scope ')
    assert command.endswith('\\."') and ('%1' in command or '%V' in command)
old=json.loads(next(r['stdout'] for r in data['registration'] if r['label']=='old-install-unregister'))
new=json.loads(next(r['stdout'] for r in data['registration'] if r['label']=='new-install-unregister'))
assert len(old['preserved'])==3 and len(old['removed'])==0 and len(new['removed'])==3
assert {r['Path']:r['Hash'] for r in data['originals_before']}=={r['Path']:r['Hash'] for r in data['originals_after']}
summary={'checks_pass':True,'exe_sha256':data['exe_sha256'],'scope':data['input_scope'],'directory_static_verb':True,'drive_root_static_verb':True,'background_expanded_command':True,'background_limit':data['background_scope'],'same_native_pid_for_six_requests':True,'activation_did_not_navigate':True,'minimized_native_window_restored':True,'file_target_rejected':True,'old_uninstall_preserved_new_keys':True,'three_owned_keys_removed':True,'temporary_drive_removed':True,'reopen_after_exit':True,'media_unchanged':4,'limits':['No physical Explorer context menu input','Windows 11 classic menu placement untested','Real OS foreground/minimized behavior not fully tested']}
(root/'artifacts/p4-system-integration-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8');print(json.dumps(summary,indent=2))
