"""Download upstream test fixtures only, never users' private media."""
import hashlib, json, pathlib, urllib.request
root = pathlib.Path(__file__).resolve().parent.parent
media = root / 'test-media'
media.mkdir(exist_ok=True)
base = 'https://raw.githubusercontent.com/bigcat88/pillow_heif/master/tests/images/'
records=[]
for relative in ['heif/RGB_10__128x128.heif', 'heif/zPug_3.heic', 'heif_other/arrow.heic', 'heif_other/spatial_photo.heic']:
    path=media/relative.rsplit('/',1)[-1]
    url=base+relative
    if not path.exists():
        print('Downloading', relative, flush=True)
        with urllib.request.urlopen(url,timeout=90) as src:
            path.write_bytes(src.read())
    records.append(dict(filename=path.name,source=url,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),redistribution='test-only; review source fixture rights before public distribution'))
(root/'vendor/heif-test-samples.json').write_text(json.dumps(records,indent=2),encoding='utf8')
