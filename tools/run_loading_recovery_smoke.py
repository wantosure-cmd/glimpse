"""Owned generated PNG, actual Win32 read lock and temporarily unavailable path."""
from pathlib import Path
import argparse,ctypes,hashlib,json,struct,subprocess,time,uuid,zlib
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,required=True);p.add_argument('--prefix',required=True);a=p.parse_args()
assert a.prefix.replace('-','').isalnum()
output=ROOT/'artifacts'/(a.prefix+'.json');assert not output.exists()
task=ROOT/'target'/('recovery-owned-'+uuid.uuid4().hex);media=task/'media';media.mkdir(parents=True);held=task/'held-media';native=task/'native';native.mkdir();work=task/'work';work.mkdir();(task/'.owned-fixture').write_text('Generated PNG only; no private source writes or device changes')
def chunk(kind,data):
 return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
source=media/'locked.png'
source.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',320,240,8,2,0,0,0))+
 chunk(b'IDAT',zlib.compress((b'\0'+bytes((45,105,175))*320)*240))+chunk(b'IEND',b''))
digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=digest(source);fingerprint=(source.stat().st_size,source.stat().st_mtime_ns)
exe=a.exe.resolve(strict=True);identity=digest(exe)
kernel=ctypes.WinDLL('kernel32',use_last_error=True);kernel.CreateFileW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p];kernel.CreateFileW.restype=ctypes.c_void_p;kernel.CloseHandle.argtypes=[ctypes.c_void_p]
handle=kernel.CreateFileW(str(source),0x80000000,6,None,3,0,None);assert handle not in (None,ctypes.c_void_p(-1).value),ctypes.get_last_error()
def checked_move(old,new):
 # Check resolved absolute source and destination remain in this new owned
 # directory before every Windows recursive directory move.
 assert old.resolve().is_relative_to(task.resolve()) and new.resolve().is_relative_to(task.resolve())
 assert (task/'.owned-fixture').is_file() and old.is_dir() and not new.exists()
 old.rename(new)
r=dict(checks_pass=False,exe_sha256=identity,task=str(task),source_sha256=before,events=[],scope='Generated sample only. Same-fingerprint sharing-lock recovery, actual native issue/retry clicks and unavailable-path/restore. Not physical input or physical removable/network drive certification.')
def save():output.write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n')
try:
 with (work/'stdout.log').open('xb') as stdout,(work/'stderr.log').open('xb') as stderr:
  app=subprocess.Popen([str(exe),'--loading-smoke',str(native),'--open',str(media),'--data-dir',str(task/'data'),'--instance-scope',task.name],cwd=work,stdout=stdout,stderr=stderr);r['active_pid']=app.pid;save();began=time.monotonic()
  while app.poll() is None:
   if time.monotonic()-began>75:raise TimeoutError(f'Owned diagnostic still live PID {app.pid}; inspect before restart')
   if (native/'release-request').exists() and (work/'artifacts/recovery-failure-reason-menu.png').is_file() and 'released' not in r['events']:
    assert handle and kernel.CloseHandle(handle);handle=None
    assert digest(source)==before and (source.stat().st_size,source.stat().st_mtime_ns)==fingerprint
    r['events'].append('released');(native/'released').write_text('same fingerprint, read-sharing restored');save()
   if (native/'disconnect-request').exists() and 'disconnected' not in r['events']:
    checked_move(media,held);r['events'].append('disconnected');(native/'disconnected').write_text('generated path temporarily unavailable');save()
   if (native/'restore-request').exists() and 'restored' not in r['events']:
    checked_move(held,media);r['events'].append('restored');(native/'restored').write_text('same generated path available again');save()
   time.sleep(.05)
  r['exit_code']=app.returncode;r['active_pid']=None;save()
 raw=json.loads((native/'report.json').read_text());r['raw']=raw;r['native_report']=str(native/'report.json');save();assert app.returncode==0 and raw['checks_pass'],raw
 assert r['events']==['released','disconnected','restored']
 row=next(v for v in raw['rows'] if v['name']=='failure-reason-menu');x,y,w,h=row['card_rect'];scale=row['pixels_per_point']
 picture=work/'artifacts/recovery-failure-reason-menu.png'
 width,height=struct.unpack('>II',picture.read_bytes()[16:24])
 ffmpeg=next((f for f in (ROOT/'vendor/runtime/ffmpeg.exe',exe.parent/'runtime/ffmpeg.exe') if f.is_file()),None)
 assert ffmpeg is not None,'Restore the pinned runtime before checking native pixels'
 pixels=subprocess.check_output([str(ffmpeg),'-v','error','-i',str(picture),'-f','rawvideo','-pix_fmt','rgb24','-'],creationflags=subprocess.CREATE_NO_WINDOW)
 assert len(pixels)==width*height*3
 # Inspect the card center, excluding selection border and favorite icon.
 colors=[pixels[(py*width+px)*3:][:3] for py in range(max(0,int((y+h*.5-14)*scale)),min(height,int((y+h*.5+14)*scale))) for px in range(max(0,int((x+w*.5-65)*scale)),min(width,int((x+w*.5+65)*scale)))]
 red=sum(c[0]>180 and c[0]-c[1]>50 and c[0]-c[2]>30 for c in colors)
 assert red>8,'Failed metadata must display the danger-colored failure label, not keep saying loading'
 r['failed_card_error_label_pixels']=red
 assert digest(source)==before and (source.stat().st_size,source.stat().st_mtime_ns)==fingerprint and digest(exe)==identity
 r.update(checks_pass=True,source_unchanged=True,fingerprint_unchanged=True);save();print(json.dumps(r,ensure_ascii=False))
finally:
 if handle:kernel.CloseHandle(handle)
