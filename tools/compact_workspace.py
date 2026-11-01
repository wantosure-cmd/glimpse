"""Compact generated workspace data, retaining recoverable evidence and a rollback.

No source, assets, vendor files, test-media, git data or installed application is
deleted. Run only after tests/builds finish. --execute requires an explicit,
verified current delivery. Historical evidence uses content-addressed ZIP blobs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SKIPPED_LINKS = set()

def sha(p):
    with p.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def safe_tree(p):
    p = p.absolute()
    assert p.is_relative_to(ROOT) and not p.is_symlink() and not p.is_junction(), p
    for base, dirs, files in os.walk(p, followlinks=False):
        linked = [name for name in dirs if (Path(base)/name).is_symlink() or (Path(base)/name).is_junction()]
        SKIPPED_LINKS.update(str(Path(base)/name) for name in linked)
        dirs[:] = [name for name in dirs if name not in linked]
        for name in files:
            child = Path(base) / name
            if child.is_symlink() or child.is_junction():
                SKIPPED_LINKS.add(str(child))
                continue
            if child.is_file():
                yield child

def read(p):
    return json.loads(p.read_text(encoding='utf-8-sig'))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--current', type=Path, required=True)
    parser.add_argument('--execute', action='store_true')
    parser.add_argument('--rollback', type=Path, help='Explicit prior portable delivery; otherwise choose the newest retained older version')
    parser.add_argument('--evidence-mode', choices=('reports', 'full'), default='reports',
                        help='Default keeps reports/source and hashes; full also retains old image/media/cache payloads')
    args = parser.parse_args()
    current = args.current.resolve(strict=True)
    assert current.parent == ROOT / 'dist'
    m = read(current / 'delivery.json')
    assert m['exe_sha256'] == sha(current / 'media-browser-portable/media-browser.exe')
    for line in (current / 'SHA256SUMS.txt').read_text(encoding='utf8').splitlines():
        digest, name = line.split('  ', 1)
        assert sha(current / name) == digest
    current_version = tuple(map(int, m['application_version'].split('.')))
    older = []
    for record in (ROOT / 'dist').glob('*/delivery.json'):
        version = tuple(map(int, read(record)['application_version'].split('.')))
        if version < current_version and (record.parent / 'media-browser-portable.zip').is_file():
            older.append((version, record.parent))
    rollback = args.rollback.resolve(strict=True) if args.rollback else max(older, default=((), None))[1]
    if rollback is not None:
        assert rollback.parent == ROOT / 'dist' and rollback != current
        assert (rollback / 'media-browser-portable.zip').is_file()
        assert tuple(map(int, read(rollback / 'delivery.json')['application_version'].split('.'))) < current_version
    keep_target = {'release', 'setup-static', 'diagnostics', 'perf-media-5000'}
    cache_roots = [ROOT/'target/source-check-cache']
    for parent in ('target/release', 'target/setup-static/release'):
        cache_roots += [ROOT/parent/name for name in ('deps','build','.fingerprint','incremental','examples')]
    archive_roots = [p for p in (ROOT/'target').iterdir() if p.is_dir() and p.name not in keep_target and p.name != 'source-check-cache']
    archive_roots += [p for p in (ROOT/'artifacts').iterdir() if p.is_dir()
                     and not p.name.startswith(('compact-', 'retained-records-', 'handoff-', 'migration-'))]
    current_prefix='v'+m['application_version'].replace('.','')
    archive_files = [p for p in (ROOT/'artifacts').iterdir() if p.is_file() and p.name != 'heic-10bit.png'
                     and not p.name.startswith((current_prefix,'p2-scroll-'+current_prefix,'storage-','cleanup-','compact-'))]
    archive_files += [p for p in ROOT.iterdir() if p.is_file() and p.name.startswith('artifacts-')]
    obsolete_payloads = []
    for delivery in (ROOT/'dist').iterdir():
        if not delivery.is_dir() or delivery == current:
            continue
        portable = delivery/'media-browser-portable'
        if portable.is_dir():
            obsolete_payloads.append(portable)
        if delivery != rollback and (delivery/'delivery.json').is_file():
            obsolete_payloads += [delivery/name for name in ('media-browser-portable.zip','media-browser-setup-local-test.exe') if (delivery/name).is_file()]
    files = sorted(set(archive_files + [f for p in archive_roots for f in safe_tree(p)]))
    removable = [p for p in cache_roots+obsolete_payloads if p.exists()]
    estimated = sum(p.stat().st_size for p in files)
    estimated += sum(sum(f.stat().st_size for f in safe_tree(p)) if p.is_dir() else p.stat().st_size for p in removable)
    print(json.dumps(dict(planned_files=len(files), candidate_gib=round(estimated/2**30,3), current=str(current),
                          rollback=str(rollback) if rollback else None, evidence_mode=args.evidence_mode),ensure_ascii=False),flush=True)
    if not args.execute:
        return
    # Never remove data beneath a running workspace application or compiler.
    probe=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',
        '$OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); Get-Process cargo,rustc,media-browser,media-browser-setup,ffmpeg,ffprobe -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,Path | ConvertTo-Json -Compress'],
        capture_output=True,text=True,encoding='utf8',creationflags=subprocess.CREATE_NO_WINDOW)
    active=json.loads(probe.stdout) if probe.stdout.strip() else []
    if isinstance(active,dict):active=[active]
    root_prefix=str(ROOT).casefold()+os.sep
    blocked=[x for x in active if x['ProcessName'] in ('cargo','rustc')
             or not x.get('Path') or x['Path'].casefold().startswith(root_prefix)]
    assert not blocked, 'Workspace process still running; cleanup refused: '+repr(blocked)
    # Fingerprints are a preservation invariant, rather than an assumption.
    protected = [p for folder in ('src','assets','docs','tools','vendor','test-media') for p in safe_tree(ROOT/folder)]
    protected += [p for p in ROOT.iterdir() if p.is_file() and not p.name.startswith('artifacts-')]
    before = {str(p.relative_to(ROOT)):sha(p) for p in protected}
    packages = {p.name:sha(p) for p in current.iterdir() if p.is_file()}
    out = ROOT/'artifacts'/('compact-'+time.strftime('%Y%m%dT%H%M%S'))
    out.mkdir()
    archive = out/'historical-evidence.zip'
    entries = []
    blobs = set()
    known_binaries={r['sha256'] for r in read(current/'media-browser-portable/package-files.json')
                    if Path(r['path']).suffix.lower() in {'.exe','.dll'}}
    for record in (ROOT/'dist').glob('*/delivery.json'):
        old=read(record)
        known_binaries.update(old.get(key) for key in ('exe_sha256','setup_stub_sha256'))
        known_binaries.update(r['sha256'] for r in old.get('files',[]) if r['file'].endswith('.exe'))
    with zipfile.ZipFile(archive,'x',zipfile.ZIP_DEFLATED,compresslevel=6,allowZip64=True) as z:
        for i,p in enumerate(files):
            assert not p.is_symlink() and not p.is_junction(), p
            assert p.is_relative_to(ROOT/'target') or p.is_relative_to(ROOT/'artifacts') or (p.parent==ROOT and p.name.startswith('artifacts-'))
            stat = p.stat()
            digest = sha(p)
            # Compiler objects and repeated native DLL copies are reproducible;
            # preserve their hashes, not another copy of their binary payload.
            derived = ((p.is_relative_to(ROOT/'target') and p.suffix.lower() in {'.rlib','.rmeta','.obj','.o','.pdb','.lib'})
                       or (p.suffix.lower() in {'.dll','.exe'} and digest in known_binaries))
            report_payload = (p.suffix.lower() in {'.json','.log','.md','.txt','.rs','.py','.ps1','.cs','.toml','.lock','.yaml','.yml','.c','.h','.cpp','.manifest','.xml'}
                              or p.name == 'media-browser-source.zip') and stat.st_size <= 16 * 1024 * 1024
            if args.evidence_mode == 'reports' and not report_payload:
                derived = True
            row = dict(path=p.relative_to(ROOT).as_posix(),bytes=stat.st_size,mtime_ns=stat.st_mtime_ns,sha256=digest,rebuildable_binary=derived)
            if not derived:
                member = 'blobs/'+digest
                row['blob'] = member
                if digest not in blobs:
                    z.write(p,member)
                    blobs.add(digest)
            entries.append(row)
            if i and i%5000==0:
                print('archived metadata',i,flush=True)
        z.writestr('manifest.json',json.dumps(dict(root=str(ROOT),entries=entries,evidence_mode=args.evidence_mode,
                       scope='Only entries with a blob retain content; omitted binary/media/cache payloads have hashes only'),ensure_ascii=False))
    # Check all archived content before removing a single original evidence file.
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None
        for digest in blobs:
            with z.open('blobs/'+digest) as stream:
                assert hashlib.file_digest(stream,'sha256').hexdigest()==digest
    removed_bytes = 0
    for row in entries:
        p=ROOT/row['path']
        stat=p.stat()
        assert stat.st_size==row['bytes'] and stat.st_mtime_ns==row['mtime_ns'],p
        p.unlink()
        removed_bytes += row['bytes']
    for folder in archive_roots:
        directories=[]
        for base,dirs,names in os.walk(folder,followlinks=False):
            dirs[:]=[name for name in dirs if not (Path(base)/name).is_symlink() and not (Path(base)/name).is_junction()]
            directories.append(Path(base))
        for directory in sorted(directories,key=lambda p:len(p.parts),reverse=True):
            try:directory.rmdir()
            except OSError:pass
    dropped=[]
    for p in removable:
        p=p.absolute()
        assert p.is_relative_to(ROOT/'target') or (p.is_relative_to(ROOT/'dist') and not p.is_relative_to(current))
        links_before=set(SKIPPED_LINKS)
        items=list(safe_tree(p)) if p.is_dir() else [p]
        if any(Path(link).is_relative_to(p) for link in SKIPPED_LINKS):
            dropped.append(dict(path=str(p.relative_to(ROOT)),preserved=True,reason='Link-containing cache preserved'))
            continue
        amount=sum(f.stat().st_size for f in items)
        dropped.append(dict(path=str(p.relative_to(ROOT)),bytes=amount,reason='Rebuildable Cargo cache or superseded binary package; current release and rollback archives retained'))
        if p.is_dir():shutil.rmtree(p)
        else:p.unlink()
        removed_bytes += amount
    assert all(sha(ROOT/name)==digest for name,digest in before.items())
    assert all(sha(current/name)==digest for name,digest in packages.items())
    result=dict(current=str(current),rollback=str(rollback),source_assets_vendor_test_media_unchanged=True,
                skipped_links=sorted(SKIPPED_LINKS),archived_files=len(entries),unique_evidence_blobs=len(blobs),archive_bytes=archive.stat().st_size,archive_sha256=sha(archive),
                removed_bytes=removed_bytes,net_reclaimed_bytes=removed_bytes-archive.stat().st_size,dropped=dropped)
    (out/'cleanup-report.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf8')
    print(json.dumps(result,ensure_ascii=False),flush=True)

if __name__=='__main__':main()
