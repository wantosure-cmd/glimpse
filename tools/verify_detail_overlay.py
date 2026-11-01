"""Validate top overlay, split timestamps and actual bounded preloading observations."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('raw',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
r=json.loads(a.raw.read_text(encoding='utf-8'));assert r['error'] is None,r
rows={e['phase']:e for e in r['rows']}
for name in ('image_hidden_arrows','thumbnail_open_video','wide_slider_paused_seek'):
    e=rows[name];canvas=e['canvas'];header=e['header']
    assert canvas[1]<=1,canvas
    assert header[0]>=canvas[0] and header[1]>=canvas[1]
    assert header[1]+header[3]<canvas[1]+canvas[3]
v=rows['thumbnail_open_video'];left=v['current_time'];right=v['total_time'];seek=v['seek'];bar=v['transport']
assert left[0]+left[2]<=seek[0]+1
assert seek[0]+seek[2]<=right[0]+1
assert abs((left[1]+left[3]/2)-(right[1]+right[3]/2))<2
assert abs((seek[1]+seek[3]/2)-(bar[1]+bar[3]/2))<3
assert bar[0]+bar[2]<=v['canvas'][0]+v['canvas'][2]+1
prefetch=rows['image_hidden_arrows']['video_prefetch']
assert prefetch['path'] and prefetch['path'].endswith('02.mp4')
assert 0<prefetch['bytes']<=4*1024*1024+512*1024 and prefetch['error'] is None
image=rows['wide_slider_paused_seek'];assert image['next_image']==r['ids'][2] and image['next_image_ready']
assert rows['header_context_menu']['popup']
assert rows['overlay_back_returns']['detail'] is None
assert rows['overlay_back_returns']['video_prefetch']['path'] is None
result=dict(checks_pass=True,exe_sha256=hashlib.sha256(Path('target/release/media-browser.exe').read_bytes()).hexdigest(),canvas_top=v['canvas'][1],floating_header=True,split_timestamps=True,header_context_menu=True,back_overlay_returns=True,next_image_texture_ready=True,next_video_cache_read_bytes=prefetch['bytes'],scope='Actual native GPU application with controlled egui events and worker read observations. Decoration command is implemented; OS titlebar/physical mouse/Win11 not independently verified. Cache warming is not pre-decoding a second video or a measured latency improvement.')
a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(result,ensure_ascii=False,indent=2))
