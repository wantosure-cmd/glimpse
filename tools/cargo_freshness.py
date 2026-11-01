"""Invalidate only this application's Cargo fingerprints, preserving dependencies/samples."""
from pathlib import Path
import uuid

def invalidate_application(target: Path, profile: str) -> list[str]:
    target=target.resolve(strict=True)
    if profile not in ('debug','release'):raise ValueError('Expected debug/release profile')
    parent=(target/profile/'.fingerprint').resolve()
    if not parent.is_relative_to(target):raise ValueError('Fingerprint path escaped target')
    if not parent.is_dir():return []
    moved=[]
    for directory in parent.iterdir():
        if directory.is_symlink() or not directory.is_dir() or not directory.name.startswith('media-browser-'):continue
        for name in ('lib-media_browser','bin-media-browser'):
            file=directory/name
            if not file.is_file() or file.is_symlink():continue
            resolved=file.resolve(strict=True)
            if not resolved.is_relative_to(parent):raise ValueError('Unexpected application fingerprint')
            backup=resolved.with_name(resolved.name+'.before-source-'+uuid.uuid4().hex)
            resolved.rename(backup) # One metadata file only; never recursively remove target contents.
            moved.append(str(backup.relative_to(target)))
    return moved
