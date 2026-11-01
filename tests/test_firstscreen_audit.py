import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('firstscreen_audit', Path(__file__).parents[1] / 'tools/audit_firstscreen.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class FirstScreenAuditTests(unittest.TestCase):
    def test_equal_card_count_does_not_accept_different_media(self):
        old = dict(visible_cards=2, visible_ids=[1, 2], visible_pending_geometry=0, visible_ready=2)
        final = dict(old, visible_ids=[3, 4])
        self.assertIs(module.find_ready_sample([old, final], final, 'visible_ready'), final)

    def test_unknown_geometry_or_partial_pixels_do_not_pass(self):
        final = dict(visible_cards=2, visible_ids=[1, 2], visible_pending_geometry=0, visible_ready=2)
        pending = dict(final, visible_pending_geometry=1)
        partial = dict(final, visible_ready=1)
        self.assertIsNone(module.find_ready_sample([pending, partial], final, 'visible_ready'))

    def test_historical_missing_identity_retains_original_gate(self):
        historical = dict(visible_cards=2, visible_pending_geometry=0, visible_ready=2)
        self.assertIs(module.find_ready_sample([historical], historical, 'visible_ready'), historical)


if __name__ == '__main__':
    unittest.main()
