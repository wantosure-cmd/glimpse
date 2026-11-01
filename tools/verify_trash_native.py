"""Audit the real Windows test evidence, its binary/source freshness, and owned copies."""
import hashlib
import json
import pathlib
import platform
import re
import argparse

parser=argparse.ArgumentParser()
parser.add_argument('--expected-lib-tests',type=int,default=47)
parser.add_argument('--test-log',default='artifacts-p4-trash-all-tests.log')
parser.add_argument('--output',default='artifacts/p4-trash-evidence-summary.json')
args=parser.parse_args()

root = pathlib.Path(__file__).resolve().parent.parent
report_path = root / 'artifacts/p4-trash-native.json'
data = json.loads(report_path.read_text(encoding='utf-8-sig'))
assert data['passed'] and data['restored'] and data['original_unchanged'], data
assert data['system_recycle_entry_removed_after_restore'], data
assert data['pre_delete_flags'] & 0x80, data
assert data['callback_non_recycle_rejected'] and data['callback_stale_fingerprint_rejected'], data
assert data['stale_fingerprint_rejected'], data
partial = data['worker_partial_failure']
assert (partial['succeeded'], partial['failed']) == (1, 2), partial
assert partial['failed_copy_unchanged'] and partial['locked_copy_unchanged'], partial
assert partial['successful_copy_restored'] and partial['duplicate_batch_rejected'], partial
assert set(partial['failure_messages']) == {'2', '3'}, partial
assert '占用' in partial['failure_messages']['3'], partial

fixture = pathlib.Path(data['fixture'].removeprefix('\\\\?\\')).resolve()
assert fixture.is_relative_to(root / 'target/test-fixtures'), fixture
assert re.fullmatch(r'recycle-\d+-\d+', fixture.name), fixture
original = (fixture / 'original.png').read_bytes()
assert original.startswith(b'\x89PNG\r\n\x1a\n')
for name in ('专属回收副本.png', '保留的过期副本.png', '占用副本.png'):
    assert (fixture / name).read_bytes() == original, name
assert not pathlib.Path(data['recycle_path']).exists(), 'Restored payload still in bin'

log_path = root / args.test_log
log = log_path.read_text(encoding='utf-8-sig')
assert 'test trash::tests::native_recycle_and_restore_dedicated_copy ... ok' in log
for count in (args.expected_lib_tests, 8, 1):
    assert f'test result: ok. {count} passed; 0 failed;' in log
match = re.search(r'Running unittests src\\lib.rs \(([^)]+)\)', log)
assert match, 'Missing actual test binary path'
binary = root / match[1]
assert binary.is_file()
sources = ['Cargo.toml', 'Cargo.lock', 'src/lib.rs', 'src/trash.rs']
assert all((root / name).stat().st_mtime <= binary.stat().st_mtime for name in sources), 'Sources newer than tested binary'
assert binary.stat().st_mtime <= report_path.stat().st_mtime <= log_path.stat().st_mtime

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()

summary = {
    'checks_pass': True,
    'scope': data['scope'],
    'os': platform.platform(),
    'test_binary': str(binary),
    'test_binary_sha256': sha(binary),
    'source_sha256': {name: sha(root / name) for name in sources},
    'evidence_sha256': sha(report_path),
    'full_test_log_sha256': sha(log_path),
    'tests_passed': sum(int(p) for p, i in re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;', log)),
    'tests_ignored': sum(int(i) for p, i in re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;', log)),
    'original_and_three_copies_sha256': sha(fixture / 'original.png'),
    'partial_result': {'succeeded': 1, 'failed': 2},
    'physical_metadata_retained': data['physical_metadata_retained_after_restore'],
    'product_delete_ui_verified_by_this_backend_test': False,
    'release_performance_verified_by_this_backend_test': False,
    'goal_complete': False,
}
(root / args.output).write_text(
    json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps(summary, ensure_ascii=False, indent=2))
