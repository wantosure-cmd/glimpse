"""Read-only HEIC metadata with the packaged libheif; copy only into owned fixtures.

No image editing, source writes, metadata injection or location/identity extraction.
EXIF inspection is limited to Make, Model and Orientation.
"""
import ctypes as C
import hashlib
import json
import shutil
import struct
import argparse
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
RUNTIME=ROOT/'vendor/runtime'
BASE=ROOT/'test-media/format-matrix-v1'
class Error(C.Structure):
    _fields_=[('code',C.c_int),('subcode',C.c_int),('message',C.c_char_p)]


def check(e):
    if e.code:raise RuntimeError((e.code,e.subcode,e.message.decode('utf8',errors='replace')))


def exif_fields(data):
    starts=[data.find(b'II*\0'),data.find(b'MM\0*')]
    starts=[n for n in starts if 0<=n<4096]
    if not starts:return {}
    data=data[min(starts):]
    order='<' if data[:2]==b'II' else '>'
    start=struct.unpack_from(order+'I',data,4)[0]
    if start+2>len(data):return {}
    n=struct.unpack_from(order+'H',data,start)[0]
    if n>512:return {}
    result={}
    for i in range(n):
        offset=start+2+i*12
        if offset+12>len(data):break
        tag,typ,count=struct.unpack_from(order+'HHI',data,offset)
        if tag not in (271,272,274):continue
        if typ==2 and 0<count<4096:
            pos=offset+8 if count<=4 else struct.unpack_from(order+'I',data,offset+8)[0]
            if pos+count<=len(data):result[{271:'Make',272:'Model'}[tag]]=data[pos:pos+count].rstrip(b'\0').decode('utf8',errors='replace')
        elif tag==274 and typ==3 and count==1:
            result['Orientation']=struct.unpack_from(order+'H',data,offset+8)[0]
    return result


def inspect(path):
    data=path.read_bytes()
    assert len(data)<512*1024*1024
    lib=C.CDLL(str(RUNTIME/'heif.dll'),winmode=0x100|0x800)
    def fn(name,args,ret):
        f=getattr(lib,name);f.argtypes=args;f.restype=ret;return f
    alloc=fn('heif_context_alloc',[],C.c_void_p)
    free=fn('heif_context_free',[C.c_void_p],None)
    read=fn('heif_context_read_from_memory_without_copy',[C.c_void_p,C.c_void_p,C.c_size_t,C.c_void_p],Error)
    primary=fn('heif_context_get_primary_image_handle',[C.c_void_p,C.POINTER(C.c_void_p)],Error)
    release=fn('heif_image_handle_release',[C.c_void_p],None)
    limit=fn('heif_context_set_maximum_image_size_limit',[C.c_void_p,C.c_int],None)
    ctx=alloc();assert ctx
    handle=C.c_void_p();buffer=C.create_string_buffer(data)
    try:
        limit(ctx,10000);check(read(ctx,buffer,len(data),None));check(primary(ctx,C.byref(handle)))
        result={}
        for name in ['width','height','ispe_width','ispe_height','luma_bits_per_pixel','chroma_bits_per_pixel']:
            result[name]=fn('heif_image_handle_get_'+name,[C.c_void_p],C.c_int)(handle)
        item=C.c_uint32();check(fn('heif_context_get_primary_image_ID',[C.c_void_p,C.POINTER(C.c_uint32)],Error)(ctx,C.byref(item)))
        transform_ids=fn('heif_item_get_transformation_properties',[C.c_void_p,C.c_uint32,C.POINTER(C.c_uint32),C.c_int],C.c_int)
        n=transform_ids(ctx,item.value,None,0);assert 0<=n<100
        ids=(C.c_uint32*n)();got=transform_ids(ctx,item.value,ids,n)
        transforms=[]
        for prop in list(ids)[:got]:
            kind=fn('heif_item_get_property_type',[C.c_void_p,C.c_uint32,C.c_uint32],C.c_int)(ctx,item.value,prop)
            name=kind.to_bytes(4,'big').decode('ascii',errors='replace')
            t=dict(property=name)
            if name=='irot':t['rotation_ccw']=fn('heif_item_get_property_transform_rotation_ccw',[C.c_void_p,C.c_uint32,C.c_uint32],C.c_int)(ctx,item.value,prop)
            if name=='imir':t['mirror_direction']=fn('heif_item_get_property_transform_mirror',[C.c_void_p,C.c_uint32,C.c_uint32],C.c_int)(ctx,item.value,prop)
            transforms.append(t)
        result['container_transforms']=transforms
        # Independent geometry reference: decode coded pixels with all container
        # transforms disabled, then map sample coordinates using the documented
        # property order. No source pixel buffers are edited or written.
        opts=fn('heif_decoding_options_alloc',[],C.c_void_p)();assert opts
        image=C.c_void_p()
        try:
            C.cast(opts,C.POINTER(C.c_uint8))[1]=1  # header: version, ignore_transformations
            check(fn('heif_decode_image',[C.c_void_p,C.POINTER(C.c_void_p),C.c_int,C.c_int,C.c_void_p],Error)(handle,C.byref(image),1,11,opts))
            w=fn('heif_image_get_width',[C.c_void_p,C.c_int],C.c_int)(image,10)
            h=fn('heif_image_get_height',[C.c_void_p,C.c_int],C.c_int)(image,10)
            stride=C.c_size_t()
            plane=fn('heif_image_get_plane_readonly2',[C.c_void_p,C.c_int,C.POINTER(C.c_size_t)],C.c_void_p)(image,10,C.byref(stride))
            assert plane and 0<w<=10000 and 0<h<=10000 and stride.value>=w*4
            references={}
            for mode,ops in [('none',[]),('rotation_only',[t for t in transforms if t['property']=='irot']),('container_order',transforms)]:
                samples=[]
                for sy in range(1,12):
                    for sx in range(1,12):
                        x,y=sx/12,sy/12
                        for t in reversed(ops):
                            if t['property']=='irot':
                                angle=t['rotation_ccw']
                                if angle==90:x,y=1-y,x
                                elif angle==180:x,y=1-x,1-y
                                elif angle==270:x,y=y,1-x
                            elif t['property']=='imir':
                                if t['mirror_direction']==0:y=1-y
                                elif t['mirror_direction']==1:x=1-x
                            else:raise RuntimeError('Reference does not implement this transform')
                        px=max(0,min(w-1,round(x*(w-1))));py=max(0,min(h-1,round(y*(h-1))))
                        samples.append(list(C.string_at(plane+py*stride.value+px*4,3)))
                references[mode]=samples
            result['untransformed_decode_reference']=dict(width=w,height=h,samples=references)
        finally:
            if image.value:fn('heif_image_release',[C.c_void_p],None)(image)
            fn('heif_decoding_options_free',[C.c_void_p],None)(opts)
        n=fn('heif_image_handle_get_number_of_thumbnails',[C.c_void_p],C.c_int)(handle);assert 0<=n<100
        result['embedded_thumbnail_count']=n
        number=fn('heif_image_handle_get_number_of_metadata_blocks',[C.c_void_p,C.c_char_p],C.c_int)(handle,b'Exif')
        assert 0<=number<=100
        if number:
            ids=(C.c_uint32*number)()
            got=fn('heif_image_handle_get_list_of_metadata_block_IDs',[C.c_void_p,C.c_char_p,C.POINTER(C.c_uint32),C.c_int],C.c_int)(handle,b'Exif',ids,number)
            for mid in list(ids)[:got]:
                n=fn('heif_image_handle_get_metadata_size',[C.c_void_p,C.c_uint32],C.c_size_t)(handle,mid)
                assert n<4*1024*1024
                raw=(C.c_ubyte*n)();check(fn('heif_image_handle_get_metadata',[C.c_void_p,C.c_uint32,C.c_void_p],Error)(handle,mid,raw))
                result['exif']=exif_fields(bytes(raw))
        result.update(source=str(path),bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
        return result
    finally:
        if handle.value:release(handle)
        free(ctx)


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('files',nargs='+',type=Path,help='Explicit user-authorized source files; read-only')
    args=parser.parse_args()
    assert (BASE/'.generated-format-fixture').is_file()
    sources=[p.resolve(strict=True) for p in args.files]
    records=[];cases=[]
    for i,source in enumerate(sources,1):
        result=inspect(source);records.append(result)
        folder=BASE/f'heic-user-{i}';folder.mkdir(exist_ok=True)
        dest=folder/source.name
        if dest.exists():assert hashlib.sha256(dest.read_bytes()).hexdigest()==result['sha256']
        else:shutil.copyfile(source,dest)
        assert hashlib.sha256(dest.read_bytes()).hexdigest()==result['sha256']
        cases.append(dict(name=f'heic-user-{i}',kind='image',path=str(dest.relative_to(ROOT)),
                          expected=dict(width=result['width'],height=result['height'],orientation=1),
                          bytes=result['bytes'],sha256=result['sha256'],provenance='User-provided local file, original read-only; test operates on dedicated copy'))
        assert hashlib.sha256(source.read_bytes()).hexdigest()==result['sha256']
    (ROOT/'artifacts/p5-user-heic-inspection.json').write_text(json.dumps(dict(scope='Primary-image libheif bit depth/dimensions and limited EXIF; no source writes',records=records),ensure_ascii=False,indent=2),encoding='utf8')
    (BASE/'user-heic-manifest.json').write_text(json.dumps(dict(cases=cases),ensure_ascii=False,indent=2),encoding='utf8')
    print(json.dumps([{k:v for k,v in r.items() if k!='untransformed_decode_reference'} for r in records],ensure_ascii=False,indent=2))


if __name__=='__main__':main()
