"""Check actual libmpv subtitle observations and compact transport geometry."""
import argparse, hashlib, json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('raw',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
r=json.loads(a.raw.read_text(encoding='utf-8'));assert r['error'] is None,r
rows={e['phase']:e for e in r['rows']}
auto=rows['subtitle_auto'];assert 'AUTO SRT' in auto['player']['sub_text']
assert 'MANUAL SRT' in rows['subtitle_manual']['player']['sub_text']
gold=rows['subtitle_gold'];assert gold['player']['sub_size']==48 and gold['player']['sub_pos']==65
assert gold['player']['sub_color']=='#FFFFDE80'
assert gold['subtitle_style']['color']==[255,222,128]
assert rows['subtitle_box']['subtitle_style']['background']
assert rows['subtitle_box']['player']['sub_border_style']=='background-box'
assert rows['subtitle_box']['player']['sub_back_color']=='#99000000'
for name,pause in [('video_click_paused','yes'),('video_click_resumed','no')]:
    assert rows[name]['player']['pause']==pause
bad=rows['subtitle_failed_keeps_video'];assert '失败' in bad['subtitle_notice']
assert bad['player']['ready'] and bad['player']['error'] is None
last=rows['subtitle_no_leak_next_video'];assert not last['player']['sub_text']
seek=auto['seek'];bar=auto['transport'];canvas=auto['canvas']
assert abs((seek[1]+seek[3]/2)-(bar[1]+bar[3]/2))<3
assert bar[0]+bar[2]<=canvas[0]+canvas[2]+1
assert seek[2]/canvas[2]>.60
for e in json.loads(Path('artifacts/subtitle-fixture.json').read_text(encoding='utf-8')):
    assert hashlib.sha256(Path(e['path']).read_bytes()).hexdigest()==e['sha256']
result=dict(checks_pass=True,exe_sha256=hashlib.sha256(Path('target/release/media-browser.exe').read_bytes()).hexdigest(),auto_srt=True,manual_srt=True,subtitle_style_changed=True,left_click_pause_resume=True,bad_subtitle_preserves_video=True,no_subtitle_leak=True,single_row_transport=True,seek_canvas_width_ratio=seek[2]/canvas[2],scope='Actual libmpv subtitle text/properties and controlled egui clicks; native picker physical interaction and Win11 not verified')
a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
