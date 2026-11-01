"""Deterministic vector-first Glimpse mark and multi-size Windows icon; stdlib only."""
from pathlib import Path
import struct,zlib
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'assets/brand';OUT.mkdir(exist_ok=True)
polys=[([(48,112),(174,48),(210,80),(84,144)],(186,164,245)), ([(48,176),(174,112),(210,144),(84,208)],(126,103,186))]
svg='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" rx="58" fill="#1c1b23"/><path d="M48 112 174 48 210 80 84 144Z" fill="#baa4f5"/><path d="M48 176 174 112 210 144 84 208Z" fill="#7e67ba"/></svg>'
(OUT/'glimpse.svg').write_text(svg,encoding='utf-8')
def inside(x,y,poly):
    signs=[(b[0]-a[0])*(y-a[1])-(b[1]-a[1])*(x-a[0]) for a,b in zip(poly,poly[1:]+poly[:1])]
    return all(v>=0 for v in signs) or all(v<=0 for v in signs)
def png(n):
    rows=[];samples=4
    for y in range(n):
        row=bytearray([0])
        for x in range(n):
            accum=[0,0,0,0]
            for sy in range(samples):
                for sx in range(samples):
                    px=(x+(sx+.5)/samples)*256/n;py=(y+(sy+.5)/samples)*256/n
                    dx=max(58-px,px-198,0);dy=max(58-py,py-198,0)
                    c=(28,27,35,255) if dx*dx+dy*dy<=58*58 else (0,0,0,0)
                    for poly,rgb in polys:
                        if inside(px,py,poly):c=(*rgb,255)
                    for k,v in enumerate(c):accum[k]+=v
            row.extend(round(v/(samples*samples)) for v in accum)
        rows.append(bytes(row))
    def chunk(t,b):return struct.pack('>I',len(b))+t+b+struct.pack('>I',zlib.crc32(t+b)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',n,n,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows),9))+chunk(b'IEND',b'')
sizes=[16,24,32,48,64,128,256];images=[png(n) for n in sizes]
(OUT/'glimpse.png').write_bytes(images[-1])
offset=6+16*len(sizes);entries=[]
for n,b in zip(sizes,images):
    entries.append(struct.pack('<BBBBHHII',n%256,n%256,0,0,1,32,len(b),offset));offset+=len(b)
(OUT/'glimpse.ico').write_bytes(struct.pack('<HHH',0,1,len(sizes))+b''.join(entries)+b''.join(images))
print('Created SVG, PNG and seven-size ICO')
