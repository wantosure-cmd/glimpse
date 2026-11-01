"""Verify the dedicated duplicate fixture still matches its known source bytes."""
import hashlib
import json
import pathlib

root = pathlib.Path(__file__).resolve().parent.parent
fixture = root / "target/perf-media-5000"
assert (fixture / ".media-browser-fixture").is_file()
hashes = {}

def sha(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()

records = []
for group in range(1, 101):
    for number in range(1, 51):
        if number <= 45:
            source, extension = root / "test-media/ui-demo/封面.jpg", ".jpg"
        elif number <= 48:
            source, extension = root / "artifacts/heic-10bit.png", ".png"
        elif number == 49:
            source, extension = root / "test-media/upstream-example.heic", ".heic"
        elif group % 10 == 0:
            source, extension = root / "test-media/test-h264-aac.mp4", ".mp4"
        else:
            source, extension = root / "test-media/ui-demo/封面.jpg", ".jpg"
        expected = hashes.setdefault(str(source), None)
        if expected is None:
            expected = hashes[str(source)] = sha(source)
        path = fixture / f"图集 {group}" / f"{number}{extension}"
        actual = sha(path)
        assert actual == expected, f"Fixture differs from known generator source: {path}"
        records.append({"path": str(path.relative_to(fixture)), "sha256": actual})
assert sum(p.is_file() for p in fixture.rglob("*")) == 5001  # 5000 media plus marker
result = {"scope": "Dedicated generated duplicates match the generator source bytes after browsing; not a before/after manifest of private user media", "checked": 5000, "all_match": True, "source_sha256": hashes, "files": records}
(root / "artifacts/p2-perf-fixture-verification.json").write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf8")
print(json.dumps({"checked": 5000, "all_match": True}))
