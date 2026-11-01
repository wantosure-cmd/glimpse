import hashlib,json,pathlib,platform,subprocess
import argparse
parser=argparse.ArgumentParser()
parser.add_argument('--expected-tests',type=int,default=57)
parser.add_argument('--test-log',default='artifacts-p4-trash-all-tests.log')
parser.add_argument('--backend-evidence',default='p4-trash-evidence-summary.json')
parser.add_argument('--output',default='artifacts/p4-current-verification.json')
args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parent.parent
def read(n):return json.loads((root/'artifacts'/n).read_text(encoding='utf-8-sig'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest().upper()
exe=root/'dist/browser-preview/media-browser.exe';fingerprint=sha(exe)
checks={}
for name in ['p4-trash-ui-verification.json','p3-input-verification.json','p3-playback-verification.json','p3-gif-verification.json','p4-watch-ui-verification.json','p2-ui-verification.json','p4-system-integration-verification.json']:
    data=read(name);assert data['checks_pass'] and data['exe_sha256']==fingerprint,(name,data);checks[name]=data
assert checks['p4-watch-ui-verification.json']['descendant_and_ancestor_counts_verified']
assert checks['p4-trash-ui-verification.json']['ancestor_counts_after_recycle']
assert sha(root/'target/release/media-browser.exe')==fingerprint
files=[*root.glob('src/**/*.rs'),root/'Cargo.toml',root/'Cargo.lock',root/'assets/ui-chinese.ttf']
assert all(p.stat().st_mtime<=exe.stat().st_mtime for p in files)
backend=read(args.backend_evidence);assert backend['checks_pass'] and backend['tests_passed']==args.expected_tests and backend['tests_ignored']==3
log=(root/args.test_log).read_text(encoding='utf-8-sig')
for name in ['cancelled_backpressured_worker_acknowledges_closed_file','blocked_source_waits_for_running_decode_and_refuses_new_jobs','native_async_unload_waits_for_idle_and_releases_dedicated_copies','recycled_result_is_idempotent_and_stale_scan_cannot_resurrect_it','protocol_rejects_versions_lengths_nul_and_roundtrips_utf16','native_same_user_pipe_acl_forward_bounded_queue_and_reopen','silent_client_does_not_hold_server_shutdown','registry_ownership_upgrade_uninstall_preserves_other_install_and_unknown_verbs','command_preserves_quoted_executable_and_drive_root_directory_arguments','descendant_view_reconciles_ancestor_siblings_missing_view_and_scope_reset','metadata_only_publication_does_not_rebuild_unchanged_tree','scoped_snapshot_uses_real_parent_tree_and_rejects_prefix_siblings']:
    assert name+' ... ok' in log,name
player=read('p4-player-release.json');assert player['passed'] and len(player['rows'])==6 and all(r['exclusive_open_after_release'] for r in player['rows'])
summary={'checks_pass':True,'scope':'Current Windows Release preview, dedicated recycle/restore copies and synthetic native input; not complete P4/P5 or physical-input certification','exe_sha256':fingerprint,'os':platform.platform(),'compiler':subprocess.check_output(['rustc','--version'],text=True).strip(),'source_sha256':{str(p.relative_to(root)):sha(p) for p in files},'native_backend':backend,'player_release':player,'native_ui_checks':checks,'limits':['Win11 and clean offline installs untested','Physical keyboard/mouse and broad OS focus matrix remain untested; one native minimized restore case is verified','Disabled/full recycle bins, network drives and full focus matrix remain untested','Explorer physical input and Win11 classic placement untested; installers and final performance remain incomplete']}
(root/args.output).write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'checks_pass':True,'exe_sha256':fingerprint,'full_suite_passed':args.expected_tests,'ignored':3,'same_build_gui_regressions':len(checks),'recycle_batches':checks['p4-trash-ui-verification.json']['batches']},indent=2))
