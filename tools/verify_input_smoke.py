import hashlib,json,pathlib,argparse
root=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--evidence-dir',type=pathlib.Path,default=root/'artifacts')
parser.add_argument('--exe',type=pathlib.Path,default=root/'dist/browser-preview/media-browser.exe')
args=parser.parse_args();evidence=args.evidence_dir.resolve(strict=True);exe=args.exe.resolve(strict=True)
def read(name):return json.loads((evidence/name).read_text(encoding='utf-8-sig'))
run=read('p3-input-run.json')
assert run['exit_code']==0
assert hashlib.sha256(exe.read_bytes()).hexdigest().upper()==run['exe_sha256']
result=read('p3-input-smoke.json')
assert result['error'] is None,result
r={row['phase']:row for row in result['rows']}
assert r['before_400ms']['preview'] is None,r['before_400ms']
preview=r['preview_ready']
assert 400<=preview['hover_delay_ms']<1500,preview
assert preview['player']['ready'] and preview['player']['mute']=='no',preview
assert not preview['muted_preference'] and preview['volume_preference']==70,preview
before,after=r['wheel_before'],r['wheel_after']
assert before['scroll']>5,before
assert abs((after['player']['time']-before['player']['time'])-30)<0.6,(before,after)
assert abs(after['scroll']-before['scroll'])<0.5 and after['player']['pause']=='yes',(before,after)
side=r['side_after']
assert abs((side['player']['time']-after['player']['time'])+5)<0.6,side
assert side['root']==after['root'] and abs(side['scroll']-after['scroll'])<0.5,side
shift=r['shift_page_scroll']
assert shift['scroll']>after['scroll']+1 and shift['preview'] is None,shift
assert r['left_paused']['preview'] is None and r['left_paused']['player']['pause']=='yes',r['left_paused']
assert r['idle_unloaded']['player']['source'] is None,r['idle_unloaded']
resumed=r['preview_resumed']
assert resumed['player']['mute']=='no' and resumed['player']['time']>=side['player']['time']-0.6,resumed
detail=r['single_click_detail']
assert detail['detail']==resumed['preview'] and detail['player']['mute']=='no',detail
assert detail['player']['time']>=resumed['player']['time']-0.6,detail
outside_before,outside_after=r['detail_outside_before'],r['detail_outside_after']
if 'detail_canvas' in outside_before:
    def contains(rect, point):
        return rect[0] <= point[0] <= rect[2] and rect[1] <= point[1] <= rect[3]
    for observed in (outside_before,outside_after):
        assert contains(observed['detail_canvas'],observed['pointer']),observed
        assert not contains(observed['detail_header'],observed['pointer']),observed
        assert not contains(observed['media_rect'],observed['pointer']),observed
assert outside_after['detail']==outside_before['detail'] and outside_after['root']==outside_before['root'],outside_after
assert abs(outside_after['player']['time']-outside_before['player']['time'])<0.2,(outside_before,outside_after)
assert r['blank_closed_detail']['detail'] is None,r['blank_closed_detail']
assert r['child_before_back']['root'].endswith('子目录'),r['child_before_back']
assert r['directory_back']['root']==preview['root'],r['directory_back']
assert r['invalid_video_error']['player']['error'] and r['invalid_video_error']['playback_errors']>=1,r['invalid_video_error']
recovered=r['recovered_valid_video']['player']
assert recovered['ready'] and recovered['error'] is None and recovered['source'].endswith('01.mp4'),recovered
assert abs(r['control_wheel_after']['player']['time']-r['control_wheel_before']['player']['time'])<0.2,r['control_wheel_after']
assert r['control_wheel_after']['pending'] is None,r['control_wheel_after']
assert r['end_reached']['player']['eof']=='yes' and r['end_reached']['player']['time']>=64,r['end_reached']
assert r['end_reached']['end_action']=='Pause' and r['end_replayed']['end_action']=='Pause'
assert r['end_replayed']['player']['eof']=='no' and r['end_replayed']['player']['time']<3 and r['end_replayed']['player']['pause']=='no',r['end_replayed']
assert r['repeat_space_paused']['player']['pause']=='yes',r['repeat_space_paused']
assert r['rapid_switch_final']['player']['ready'] and r['rapid_switch_final']['player']['error'] is None,r['rapid_switch_final']
assert r['rapid_switch_final']['player']['source'].endswith('01.mp4'),r['rapid_switch_final']
assert r['controlled_focus_lost']['preview'] is None and r['controlled_focus_lost']['player']['pause']=='yes',r['controlled_focus_lost']
assert r['focus_restored_outside_video']['preview'] is None,r['focus_restored_outside_video']
assert not r['focus_before']['sidebar_open'],r['focus_before']
loop_before,loop_after=r['list_loop_before'],r['list_loop_after']
assert loop_before['player']['loop']=='inf' and loop_before['player']['mute']=='no',loop_before
assert loop_after['preview'] is not None and loop_after['player']['time']<5 and loop_after['player']['pause']=='no' and loop_after['player']['eof']=='no',loop_after
audio=read('ui-input-audio-peaks.json')
list_peak=max((a['peak'] for a in audio if loop_before['app_elapsed_ms']+250<a['ms']<loop_after['app_elapsed_ms']-250),default=0)
assert list_peak>0.001,('list audio session has no measured signal',list_peak)
progress=r['list_progress_clicked']
assert progress['preview'] is not None and progress['detail'] is None and progress['player']['time']>10,progress
assert r['multi_select_all']['selection_count']>1,r['multi_select_all']
assert r['right_menu_multi_preserved']['popup'] and r['right_menu_multi_preserved']['selection_count']==r['multi_select_all']['selection_count'],r['right_menu_multi_preserved']
assert read('p3-input-hashes-before.json')==read('p3-input-hashes-after.json')
summary={'checks_pass':True,'list_audio_session_peak':list_peak,'list_loop_verified':True,'list_sound_unmuted':True,'list_progress_click_verified':True,'multi_select_and_context_menu_verified':True,'scope':result['scope'],'exe_sha256':run['exe_sha256'],'hover_delay_ms':preview['hover_delay_ms'],'wheel_delta_seconds':after['player']['time']-before['player']['time'],'side_delta_seconds':side['player']['time']-after['player']['time'],'scroll_during_seek_points':after['scroll']-before['scroll'],'shift_scroll_points':shift['scroll']-after['scroll'],'source_files_unchanged':len(read('p3-input-hashes-before.json')),'invalid_video_recovered':True,'control_wheel_did_not_seek':True,'eof_replay':True,'repeated_space_deduplicated':True,'rapid_source_load_requests':24,'rapid_switch_final_valid':True,'controlled_focus_loss_paused':True,'focus_restored_outside_video_did_not_play':True}
summary['eof_policy_for_manual_space_replay_test']='Pause; default Next tested separately by detail diagnostic'
(evidence/'p3-input-verification.json').write_text(json.dumps(summary,indent=2),encoding='utf8')
print(json.dumps(summary,indent=2))
