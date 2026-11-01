"""Audit original hashes and native FBO orientation against untransformed pixels."""
import hashlib
import json
import statistics
import argparse
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--metadata',type=Path,default=ROOT/'artifacts/p5-user-heic-inspection.json')
parser.add_argument('--ui',type=Path,default=ROOT/'artifacts/p5-user-heic-ui.json')
parser.add_argument('--output',type=Path,default=ROOT/'artifacts/p5-user-heic-verification.json')
parser.add_argument('--overlay-evidence',type=Path,help='Same-build detail diagnostic: exclude actual foreground header, not image pixels hidden by it')
parser.add_argument('--overlay-verification',type=Path,help='Same-build passing detail verification which names the raw diagnostic')
args=parser.parse_args()
metadata=json.loads(args.metadata.read_text(encoding='utf8'))
ui=json.loads(args.ui.read_text(encoding='utf8'))
assert ui['checks_pass'] and ui['finished'] and len(ui['rows'])==len(metadata['records'])==2
assert hashlib.sha256((ROOT/'dist/browser-preview/media-browser.exe').read_bytes()).hexdigest()==ui['exe_sha256']
header=None
if args.overlay_evidence:
    assert args.overlay_verification
    verified=json.loads(args.overlay_verification.read_text(encoding='utf-8'))
    assert verified['checks_pass'] and verified['exe_sha256']==ui['exe_sha256']
    assert (ROOT/verified['raw']).resolve()==args.overlay_evidence.resolve()
    overlay=json.loads(args.overlay_evidence.read_text(encoding='utf-8'));assert overlay['error'] is None
    geometry=next(e for e in overlay['rows'] if e['phase']=='image_hidden_arrows')
    header=geometry['header'];assert header and header[2]<=600 and header[3]<=60
rows=[]
for source,native in zip(metadata['records'],ui['rows']):
    path=Path(source['source'])
    assert hashlib.sha256(path.read_bytes()).hexdigest()==source['sha256']
    assert source['sha256']==native['source_sha256']
    reference=source['untransformed_decode_reference']
    assert (reference['width'],reference['height'])==(source['ispe_width'],source['ispe_height'])
    errors={}
    sample_counts={}
    for phase in ['list','detail']:
        actual=native['screenshots'][phase]['samples']
        indices=list(range(len(actual)))
        assert len(actual)==121
        if phase=='detail' and header:
            shot=native['screenshots'][phase];x0,y0,x1,y1=shot['region']
            # UI and media coordinates must come from matching viewport geometry.
            assert abs(shot['width']/shot['scale']-geometry['canvas'][2])<1
            assert abs((y1-y0)-geometry['canvas'][3])<1
            hx,hy,hw,hh=header
            indices=[i for i in indices if not (hx<=x0+(x1-x0)*((i%11+1)/12)<=hx+hw and hy<=y0+(y1-y0)*((i//11+1)/12)<=hy+hh)]
            assert len(indices)>=110,'Unexpectedly large foreground occlusion'
        sample_counts[phase]=dict(compared=len(indices),foreground_occluded=len(actual)-len(indices))
        errors[phase]={mode:statistics.mean(abs(a-b) for i in indices for a,b in zip(actual[i],expected[i]))
                       for mode,expected in reference['samples'].items()}
        assert errors[phase]['container_order']<12
        assert errors[phase]['container_order']<errors[phase]['none']*.4
        if any(t['property']=='imir' for t in source['container_transforms']):
            assert errors[phase]['container_order']<errors[phase]['rotation_only']*.4
    rows.append(dict(file=path.name,sha256=source['sha256'],source_unchanged=True,
                     primary_bits=source['luma_bits_per_pixel'],exif=source.get('exif'),
                     coded_size=[source['ispe_width'],source['ispe_height']],display_size=[source['width'],source['height']],
                     container_transforms=source['container_transforms'],mean_rgb_errors=errors,sample_counts=sample_counts,evidence=native['evidence']))
result=dict(checks_pass=True,exe_sha256=ui['exe_sha256'],rows=rows,
            scope='User-provided primary HEICs; read-only originals; list/detail FBO geometry compared with raw decode using ignore_transformations and explicit container property coordinate mapping. RGB tolerance allows resampling/ICC; not professional color calibration or a 10-bit/HDR claim.')
if header:
    result['foreground_header_exclusion']=dict(rect=header,source=str(args.overlay_evidence),sha256=hashlib.sha256(args.overlay_evidence.read_bytes()).hexdigest(),rule='Exclude actual header-covered grid points equally for every candidate transform; original RGB <12 and wrong-transform ratio <0.4 thresholds unchanged; all list grid points retained')
args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf8')
print(json.dumps(result,ensure_ascii=False,indent=2))
