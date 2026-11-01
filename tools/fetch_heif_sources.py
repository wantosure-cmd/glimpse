import hashlib, json, pathlib, urllib.request, zipfile

root = pathlib.Path(__file__).resolve().parent.parent
downloads = root / 'vendor/downloads'
downloads.mkdir(parents=True, exist_ok=True)
records = []
for repo, tag in [('libde265', 'v1.1.3'), ('libheif', 'v1.23.6')]:
    url = f'https://codeload.github.com/strukturag/{repo}/zip/refs/tags/{tag}'
    path = downloads / f'{repo}-{tag}.zip'
    if not path.exists():
        print('Downloading', url, flush=True)
        with urllib.request.urlopen(url, timeout=90) as src:
            path.write_bytes(src.read())
    with zipfile.ZipFile(path) as z:
        z.extractall(root / 'vendor/extracted/sources')
    records.append(dict(component=repo, version=tag, url=url, sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
(root / 'vendor/heif-sources.json').write_text(json.dumps(records, indent=2), encoding='utf8')
