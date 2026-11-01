import argparse,hashlib,json,pathlib
root=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--evidence-dir',type=pathlib.Path,default=root/'artifacts');args=parser.parse_args();evidence=args.evidence_dir.resolve(strict=True)
def read(name): return json.loads((evidence/name).read_text(encoding='utf-8-sig'))
run=read('p4-watch-ui-run.json')
assert run['exit_code']==0,run
assert hashlib.sha256(pathlib.Path(run['exe_path']).read_bytes()).hexdigest().upper()==run['exe_sha256']
result=read('p4-watch-ui.json')
assert result['error'] is None,result
r={row['phase']:row for row in result['rows']}
assert len(r)==6,r
for row in r.values():
    assert row['generation']==row['prepared_generation'],row
    assert row['watch_warning'] is None,row
assert r['before_add']['media']==40 and r['before_add']['groups']==1
assert r['after_add']['media']==41 and r['after_add']['groups']==2 and r['after_add']['selected']
assert abs(r['after_add']['anchor_error'])<=1,r['after_add']
assert r['detail_before_replace']['detail'] is not None
assert r['detail_after_replace']['detail']==r['detail_before_replace']['detail'] and r['detail_after_replace']['selected']
assert r['after_external_remove']['media']==40 and r['after_external_remove']['detail'] is None and not r['after_external_remove']['selected']
assert r['after_manual_refresh']['generation']>r['after_external_remove']['generation'] and r['after_manual_refresh']['media']==40
tree={row['phase']:row for row in result['tree_rows']}
assert len(tree)==5,tree
assert (tree['tree_expanded']['root_images'],tree['tree_expanded']['root_videos'])==(81,1)
assert tree['child_before_changes']['media']==41 and tree['child_before_changes']['current_images']==41
for key,root_images,sibling_images in [('child_after_additions',83,2),('child_after_sibling_removal',82,1),('child_after_manual_refresh',82,1)]:
    row=tree[key];assert row['media']==row['current_images']==42 and row['current_videos']==0,row
    assert row['root_images']==root_images and row['root_videos']==1 and row['sibling_images']==sibling_images and row['sibling_videos']==1,row
    assert row['generation']==row['tree_generation'] and row['selected'] and abs(row['anchor_error'])<=1,row
    assert row['root'].endswith('新增目录') and not row['tree_root'].endswith('新增目录'),row
assert tree['child_after_manual_refresh']['generation']>tree['child_after_sibling_removal']['generation']
assert read('p4-watch-ui-originals-before.json')==read('p4-watch-ui-originals-after.json')
summary={'checks_pass':True,'descendant_and_ancestor_counts_verified':True,'sibling_changes_watched':True,'descendant_anchor_error_points':tree['child_after_additions']['anchor_error'],'exe_sha256':run['exe_sha256'],'scope':result['scope'],'counts':[r[k]['media'] for k in ('before_add','after_add','after_external_remove','after_manual_refresh')],'add_preserved_selection':True,'anchor_error_points':r['after_add']['anchor_error'],'same_path_replacement_preserved_detail':True,'external_copy_removal_closed_detail_and_cleared_selection':True,'manual_refresh_reconciled':True,'originals_unchanged':True}
(evidence/'p4-watch-ui-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8')
print(json.dumps(summary,indent=2))
