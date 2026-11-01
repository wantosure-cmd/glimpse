"""Validate controlled-event detail UI observations from a real Release player."""
import argparse, hashlib, json
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('raw',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
r=json.loads(a.raw.read_text(encoding='utf-8'));assert r['error'] is None,r
rows={e['phase']:e for e in r['rows']};ids=r['ids'];assert len(ids)==5
for name in ['image_hidden_arrows','thumbnail_open_video','eof_next_skips_image','eof_replay','eof_pause','last_video_stops','video_context_menu','image_hover_arrows','arrow_open_video','video_blank_returns']:assert name in rows,name
assert rows['image_hidden_arrows']['arrows']==0
assert rows['image_hover_arrows']['arrows']==2
assert 0<rows['image_hidden_arrows']['thumbnails']<25
assert rows['thumbnail_open_video']['detail']==ids[1]
v=rows['thumbnail_open_video'];assert v['seek'][2]/v['canvas'][2]>.60
assert abs((v['seek'][1]+v['seek'][3]/2)-(v['transport'][1]+v['transport'][3]/2))<3
assert v['transport'][0]+v['transport'][2]<=v['canvas'][0]+v['canvas'][2]+1
assert v['strip'][1]>=v['canvas'][1]+v['canvas'][3]
assert rows['eof_next_skips_image']['detail']==ids[3]
replay=rows['eof_replay'];assert replay['detail']==ids[3] and replay['player']['time']<3 and replay['player']['pause']=='no'
for name,expected in [('eof_pause',ids[3]),('last_video_stops',ids[4])]:
    e=rows[name];assert e['detail']==expected and e['player']['eof']=='yes' and e['player']['pause']=='yes',e
assert rows['video_context_menu']['popup']
assert rows['arrow_open_video']['detail']==ids[1]
assert rows['video_blank_returns']['detail'] is None
assert rows['image_context_menu']['popup']
e=rows['wide_slider_paused_seek'];assert 30<e['player']['time']<35 and e['player']['pause']=='yes'
fixture=json.loads((ROOT/'artifacts/detail-redesign-fixture.json').read_text(encoding='utf-8'))
assert all(hashlib.sha256((ROOT/e['path']).read_bytes()).hexdigest()==e['sha256'] for e in fixture)
for name in ['image_hidden_arrows','last_thumbnail_centered']:
    e=rows[name];assert abs(e['current_thumbnail_x']-(e['strip'][0]+e['strip'][2]/2))<3,e
exe=ROOT/'target/release/media-browser.exe';result=dict(checks_pass=True,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),raw=str(a.raw),dedicated_fixture_files=len(fixture),fixture_hashes_unchanged=True,seek_width_ratio=v['seek'][2]/v['canvas'][2],visible_thumbnail_count=rows['image_hidden_arrows']['thumbnails'],scope='Real libmpv EOF and controlled egui raw-event widgets; not physical mouse or cross-machine verification')
a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
