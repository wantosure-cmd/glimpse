"""Inspect actual native pixels for checkbox fill bounds and paired navigation arrows."""
from pathlib import Path
import argparse,json
from PIL import Image
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--recovery',type=Path,required=True);p.add_argument('--media-jump',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();assert not a.output.exists()
recovery=json.loads(a.recovery.read_text());jump=json.loads(a.media_jump.read_text());assert recovery['checks_pass'] and jump['checks_pass'] and recovery['exe_sha256']==jump['exe_sha256']
row=next(v for v in recovery['raw']['rows'] if v['name']=='failure-reason-menu');x,y,w,h=row['card_rect'];scale=row['pixels_per_point'];cx,cy=x+18,y+18
image=Image.open(Path(recovery['task'])/'work/artifacts/recovery-failure-reason-menu.png').convert('RGB');colored=[]
for py in range(round((y+6)*scale),round((y+30)*scale)):
 for px in range(round((x+6)*scale),round((x+30)*scale)):
  c=image.getpixel((px,py))
  if max(abs(v-t) for v,t in zip(c,(177,160,231)))<=3:colored.append((px,py))
assert colored,'Selected checkbox must have accent fill'
bounds=[min(v[0] for v in colored),min(v[1] for v in colored),max(v[0] for v in colored)+1,max(v[1] for v in colored)+1]
assert bounds[2]-bounds[0]<=13*scale and bounds[3]-bounds[1]<=13*scale,bounds
assert all(abs(px+.5-cx*scale)<=8*scale and abs(py+.5-cy*scale)<=8*scale for px,py in colored),'Accent escaped the16pt square icon'
native=Path(jump['native']);raw=json.loads((native/'report.json').read_text());rows={v['name']:v for v in raw['rows']}
def mask(name):
 row=rows[name];x,y,w,h=row['button'];scale=row['pixels_per_point'];cx=(x+w/2)*scale;cy=(y+h/2)*scale
 # An even window is centered on the actual integer physical icon center.
 # A27px window at150% rotates around a half-pixel-shifted center instead.
 size=2*round(9*scale);left=round(cx-size/2);top=round(cy-size/2)
 image=Image.open(native/(name+'.png')).convert('RGB');points=set()
 for py in range(size):
  for px in range(size):
   if min(image.getpixel((left+px,top+py)))>160:points.add((px,py))
 return size,points
size,up=mask('up-target-video');down_size,down=mask('down-empty-gap');assert size==down_size and up and down
flipped={(size-1-x,size-1-y) for x,y in up};overlap=len(flipped&down)/len(flipped|down);assert overlap>=.8,overlap
r=dict(checks_pass=True,exe_sha256=recovery['exe_sha256'],checkbox_accent_bounds_px=bounds,checkbox_accent_size_pt=[(bounds[2]-bounds[0])/scale,(bounds[3]-bounds[1])/scale],checkbox_accent_contained=True,arrow_rotation_mask_overlap=overlap,scope='Generated native screenshots: selected accent within square and up/down shaft masks agree under180degree rotation. Native menu click/navigation tests provide interaction evidence; no physical input claim.')
a.output.write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n');print(json.dumps(r,ensure_ascii=False))
