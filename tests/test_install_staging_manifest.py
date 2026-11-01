import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parents[1] / 'tools'))
from package_local import PRODUCT
from verify_local_install import installation_metadata


class InstallStagingManifestTests(unittest.TestCase):
    def test_staging_requires_explicit_mode_and_correct_identity(self):
        with tempfile.TemporaryDirectory() as task:
            root = Path(task)
            exe = root / 'setup.exe'
            staging = dict(status='internal-payload-test-only', product=PRODUCT, missing_license_texts=[])
            (root / 'staging.json').write_text(json.dumps(staging), encoding='utf8')
            with self.assertRaises(RuntimeError):
                installation_metadata(exe)
            self.assertTrue(installation_metadata(exe, True)[1])
            staging['status'] = 'local-test-only'
            (root / 'staging.json').write_text(json.dumps(staging), encoding='utf8')
            with self.assertRaises(AssertionError):
                installation_metadata(exe, True)

    def test_license_gap_cannot_enter_install_smoke(self):
        with tempfile.TemporaryDirectory() as task:
            root = Path(task)
            (root / 'staging.json').write_text(json.dumps(dict(status='internal-payload-test-only',
                product=PRODUCT, missing_license_texts=['missing-crate'])), encoding='utf8')
            with self.assertRaises(AssertionError):
                installation_metadata(root / 'setup.exe', True)

    def test_final_manifest_keeps_existing_path_and_cannot_be_replaced_by_staging(self):
        with tempfile.TemporaryDirectory() as task:
            root = Path(task)
            (root / 'delivery.json').write_text(json.dumps(dict(application_version='0.4.18')), encoding='utf8')
            (root / 'staging.json').write_text('invalid staging data', encoding='utf8')
            metadata, staging = installation_metadata(root / 'setup.exe', True)
            self.assertFalse(staging)
            self.assertEqual(metadata['application_version'], '0.4.18')


if __name__ == '__main__':
    unittest.main()
