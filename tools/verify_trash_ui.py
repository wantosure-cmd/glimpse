import argparse,hashlib,json,pathlib
root=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--evidence-dir',type=pathlib.Path,default=root/'artifacts')
args=parser.parse_args()
evidence=args.evidence_dir.resolve(strict=True)
def read(name):return json.loads((evidence/name).read_text(encoding='utf-8-sig'))
run=read('p4-trash-ui-run.json');data=read('p4-trash-ui.json');assert run['exit_code']==0 and data['error'] is None,(run,data)
assert hashlib.sha256(pathlib.Path(run['exe_path']).read_bytes()).hexdigest().upper()==run['exe_sha256']
r={r['phase']:r for r in data['rows']}
for row in r.values():
    assert row['tree_media']==row['media'],row
    assert pathlib.PureWindowsPath(row['tree_root'].removeprefix('\\\\?\\'))==pathlib.PureWindowsPath(run['fixture']),row

assert r['empty_selection_noop']['batch']==0
assert (r['video_recycled_next_gif']['batch'],r['video_recycled_next_gif']['media'])==(1,5)
assert r['held_delete_did_not_delete_next']['batch']==1
assert (r['gif_recycled_next_image']['batch'],r['gif_recycled_next_image']['media'])==(2,4)
assert (r['list_partial_failure']['batch'],r['list_partial_failure']['media'])==(3,3)
assert r['failure_window_delete_blocked']['batch']==3
assert (r['retry_success']['batch'],r['retry_success']['media'])==(4,2)
assert (r['last_detail_falls_back_previous']['batch'],r['last_detail_falls_back_previous']['media'])==(5,1)
assert (r['empty_sequence_closes_detail']['batch'],r['empty_sequence_closes_detail']['media'])==(6,0) and r['empty_sequence_closes_detail']['detail'] is None
assert len(data['reports'])==6
assert [(x['success_count'],x['failed_count']) for x in data['reports']]==[(1,0),(1,0),(1,1),(1,0),(1,0),(1,0)]
assert all(x['release_confirmed'] and x['submitted'] for x in data['reports'])
assert all(item['pre_delete_flags'] & 0x80 for report in data['reports'] for item in report['items'] if item['success'])
def hashes(rows):return {r['Path']:r['Hash'] for r in rows}
assert hashes(run['originals_before'])==hashes(run['originals_after'])
assert hashes(run['copies_before'])==hashes(run['copies_after'])
assert len(run['restored'])==6 and all(x['bytes_restored'] for x in run['restored'])
summary={'checks_pass':True,'exe_sha256':run['exe_sha256'],'scope':data['scope'],'batches':6,'successes':6,'failures':1,'successful_copies_restored':6,'source_originals_unchanged':3,'all_fixture_copies_restored_unchanged':6,'release_gates_pass':True,'held_delete_deduplicated':True,'empty_selection_noop':True,'detail_ignores_underlying_selection':True,'failure_window_blocks_delete':True,'watcher_counts_converged':True,'ancestor_counts_after_recycle':True,'last_item_falls_back_previous':True,'empty_sequence_closes_detail':True}
(evidence/'p4-trash-ui-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8');print(json.dumps(summary,indent=2))
