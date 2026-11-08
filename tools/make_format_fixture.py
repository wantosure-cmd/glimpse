"""Generate actual codec/container fixtures; no renamed extensions or private media.

Everything is generated from quadrant/arrow pixels and a sine source. These are
not iPhone camera recordings. Each output's ffprobe metadata and hash are saved.
"""
import binascii
import hashlib
import json
import struct
import subprocess
import shutil
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = ROOT / 'test-media/format-matrix-v1'
FF = ROOT / 'vendor/runtime/ffmpeg.exe'
FP = ROOT / 'vendor/runtime/ffprobe.exe'


def png(path, w=640, h=480):
    def chunk(kind, payload):
        return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', binascii.crc32(kind + payload) & 0xffffffff)
    pixels = bytearray()
    colors = [(220, 25, 35), (25, 200, 55), (35, 55, 220), (225, 210, 25)]
    for y in range(h):
        pixels.append(0)
        for x in range(w):
            c = colors[(y >= h//2)*2+(x >= w//2)]
            if (120 < x < 390 and 220 < y < 260) or (390 <= x < 520 and abs(y-240) < (520-x)*.6):
                c = (245, 245, 245)
            pixels.extend(c)
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(pixels)) + chunk(b'IEND', b''))


def execute(args):
    p = subprocess.run([str(FF), '-hide_banner', '-loglevel', 'error', '-nostdin', '-n', *map(str, args)],
                       capture_output=True, timeout=180, creationflags=subprocess.CREATE_NO_WINDOW)
    if p.returncode:
        raise RuntimeError(p.stderr.decode('utf8', errors='replace')[-4000:])


def tiff_pages(path, first_png):
    # TIFF little-endian, two uncompressed RGB pages. Page one quadrant pattern,
    # page two magenta: a visible wrong-page choice cannot pass the color audit.
    w, h = 640, 480
    raw = subprocess.check_output([str(FF), '-v', 'error', '-i', str(first_png), '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'])
    assert len(raw) == w*h*3
    data = bytearray(b'II*\x00' + struct.pack('<I', 8))
    count = 10
    ifd_size = 2 + count*12 + 4
    bits1, bits2 = 8 + ifd_size*2, 8 + ifd_size*2 + 6
    pix1, pix2 = bits2+6, bits2+6+len(raw)
    for index, pixel_offset in enumerate([pix1, pix2]):
        data.extend(struct.pack('<H', count))
        entries = [(256,4,1,w),(257,4,1,h),(258,3,3,bits1 if index == 0 else bits2),
                   (259,3,1,1),(262,3,1,2),(273,4,1,pixel_offset),(274,3,1,1),
                   (277,3,1,3),(278,4,1,h),(279,4,1,len(raw))]
        for tag, typ, n, value in entries:
            data.extend(struct.pack('<HHII',tag,typ,n,value))
        data.extend(struct.pack('<I', 8+ifd_size if index == 0 else 0))
    data.extend(struct.pack('<HHHHHH',8,8,8,8,8,8))
    data.extend(raw)
    data.extend(bytes([220,25,220]) * (w*h))
    path.write_bytes(data)


def main():
    BASE.mkdir(exist_ok=False)
    (BASE / '.generated-format-fixture').write_text('generated quadrant/arrow and sine fixtures', encoding='ascii')
    seed = BASE / 'pattern.png'
    png(seed)
    cases = []
    def add(name, kind, extension, expected, args=None):
        folder = BASE / name
        folder.mkdir()
        output = folder / ('sample.' + extension)
        if args is not None:
            execute([*args, output])
        cases.append(dict(name=name, kind=kind, path=str(output.relative_to(ROOT)), expected=expected,
                          provenance='Generated quadrant/arrow pixels and/or testsrc2 with sine; not a real camera recording'))
        return output
    add('png', 'image', 'png', dict(width=640,height=480,orientation=1), ['-i',seed,'-frames:v','1'])
    jpeg = add('jpeg', 'image', 'jpg', dict(width=640,height=480,orientation=1), ['-i',seed,'-frames:v','1','-q:v','2'])
    for orientation in [2,6]:
        output=add(f'jpeg-exif-{orientation}', 'image', 'jpg', dict(width=480 if orientation==6 else 640,height=640 if orientation==6 else 480,orientation=orientation))
        exif=b'Exif\0\0II*\x00'+struct.pack('<I',8)+struct.pack('<H',1)+struct.pack('<HHI',274,3,1)+struct.pack('<H',orientation)+b'\0\0'+struct.pack('<I',0)
        output.write_bytes(jpeg.read_bytes()[:2] + b'\xff\xe1'+struct.pack('>H',len(exif)+2)+exif+jpeg.read_bytes()[2:])
    add('webp', 'image', 'webp', dict(width=640,height=480,orientation=1), ['-i',seed,'-frames:v','1','-c:v','libwebp','-lossless','1'])
    add('bmp', 'image', 'bmp', dict(width=640,height=480,orientation=1), ['-i',seed,'-frames:v','1'])
    tiff_pages(add('tiff-two-pages','image','tiff',dict(width=640,height=480,orientation=1,page=0)),seed)
    base = ['-f','lavfi','-i','testsrc2=size=640x360:rate=15:duration=65',
            '-f','lavfi','-i','sine=frequency=440:sample_rate=48000:duration=65']
    variants = [
        ('h264-aac-flv','flv',['-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac','-f','flv'],'h264','aac',640,360),
        ('flv1-mp3-flv','FLV',['-c:v','flv','-q:v','5','-pix_fmt','yuv420p','-c:a','libmp3lame','-f','flv'],'flv1','mp3',640,360),
        ('h264-mp4','mp4',['-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac'],'h264','aac',640,360),
        ('h264-mov','mov',['-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac'],'h264','aac',640,360),
        ('h264-m4v','m4v',['-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac','-f','mp4'],'h264','aac',640,360),
        ('hevc-mov','mov',['-c:v','libx265','-preset','ultrafast','-x265-params','pools=2:frame-threads=2','-tag:v','hvc1','-pix_fmt','yuv420p','-c:a','aac'],'hevc','aac',640,360),
        ('hevc-10bit-mp4','mp4',['-c:v','libx265','-preset','ultrafast','-x265-params','pools=2:frame-threads=2','-tag:v','hvc1','-pix_fmt','yuv420p10le','-c:a','aac'],'hevc','aac',640,360),
        ('mjpeg-pcm-avi','avi',['-c:v','mjpeg','-q:v','5','-pix_fmt','yuvj420p','-c:a','pcm_s16le'],'mjpeg','pcm_s16le',640,360),
        ('mpeg4-mp3-avi','avi',['-c:v','mpeg4','-q:v','5','-c:a','libmp3lame'],'mpeg4','mp3',640,360),
        ('h263-amr-3gp','3gp',['-vf','scale=176:144','-c:v','h263','-c:a','libopencore_amrnb','-ar','8000','-ac','1','-b:a','12.2k'],'h263','amr_nb',176,144),
        ('h263-amr-3g2','3g2',['-vf','scale=176:144','-c:v','h263','-c:a','libopencore_amrnb','-ar','8000','-ac','1','-b:a','12.2k'],'h263','amr_nb',176,144),
        ('h264-aac-3gp','3gp',['-vf','scale=176:144','-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac','-ar','16000','-ac','1'],'h264','aac',176,144),
        ('h264-aac-3g2','3g2',['-vf','scale=176:144','-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac','-ar','16000','-ac','1'],'h264','aac',176,144),
        ('h264-aac-mkv','mkv',['-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac'],'h264','aac',640,360),
        ('vp9-opus-webm','webm',['-c:v','libvpx-vp9','-deadline','realtime','-cpu-used','8','-c:a','libopus'],'vp9','opus',640,360),
    ]
    for name, ext, options, vc, ac, w, h in variants:
        add(name,'video',ext,dict(video_codec=vc,audio_codec=ac,width=w,height=h,duration_min=64),base+options+['-threads','2','-shortest'])
        print('Generated',name,flush=True)
    source = BASE / 'hevc-mov/sample.mov'
    add('hevc-rotated-mov','video','mov',dict(video_codec='hevc',audio_codec='aac',width=360,height=640,rotation=90,duration_min=64),
        ['-display_rotation:v:0','90','-i',source,'-c','copy'])
    for name, filename, w, h in [('heic','upstream-example.heic',1280,854),
                               ('heic-10bit','RGB_10__128x128.heif',128,128),
                               ('heic-iphone-rotated','arrow.heic',3024,4032)]:
        source=ROOT/'test-media'/filename
        output=add(name,'image',source.suffix[1:],dict(width=w,height=h,orientation=1))
        shutil.copyfile(source,output)
        cases[-1]['provenance']='Existing pinned HEIC source in vendor/heif-test-samples.json; iPhone 8 Plus arrow is camera sample, 10-bit case is synthetic'
    for case in cases:
        f=ROOT/case['path']
        case['bytes']=f.stat().st_size
        case['sha256']=hashlib.sha256(f.read_bytes()).hexdigest()
        if f.suffix.lower() not in ('.heic','.heif'):
            case['ffprobe']=json.loads(subprocess.check_output([str(FP),'-v','error','-show_streams','-show_format','-of','json',str(f)]))
    manifest=dict(scope='Actual generated media, not iPhone recordings or extension-renamed files',cases=cases)
    (BASE/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf8')
    print('Generated and probed',len(cases),'cases')


if __name__=='__main__': main()
