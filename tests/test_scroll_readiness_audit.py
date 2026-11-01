"""Measurement gates must retain blank, unstable and wrong-identity pauses."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('scroll_audit', Path(__file__).parents[1] / 'tools/audit_scroll_readiness.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ScrollReadinessAuditTests(unittest.TestCase):
    def fixture(self):
        native = dict(error=None, pattern='fling_then_read', requested_seconds=5,
            samples=[dict(elapsed_ms=t, app_elapsed_ms=t + 10000, scroll_y=5400)
                     for t in (2400, 2500, 4800, 5000)])
        native['samples'].insert(0, dict(elapsed_ms=2000, app_elapsed_ms=12000, scroll_y=0))
        def row(t, ids, ready):
            return dict(elapsed_ms=t + 10000, visible_ids=ids, visible_cards=len(ids),
                visible_pending_geometry=0, visible_ready=ready, visible_clear_ready=ready,
                layout=dict(scroll_y=5400), failures=0)
        return native, dict(rows=[row(2350, [8, 9], 2), row(2450, [1, 2], 0),
                                 row(2550, [1, 2], 2), row(4900, [1, 2], 2)])

    def test_does_not_count_ready_previous_contents_and_retains_sampling_delay(self):
        native, load = self.fixture()
        result = module.audit(native, load)
        self.assertEqual(result['pauses'][0]['all_clear_ms'], 250)
        self.assertEqual(result['pauses'][0]['first_at_target_preview_ms'], 50)
        self.assertTrue(result['all_pauses_ready'])
        self.assertIsNone(result['statistics']['all_clear_ms']['p95_ms'])

    def test_blank_and_pending_geometry_are_misses_not_excluded_samples(self):
        native, load = self.fixture()
        for row in load['rows']:
            row['visible_pending_geometry'] = 1
        result = module.audit(native, load)
        self.assertFalse(result['all_pauses_ready'])
        self.assertEqual(result['statistics']['all_clear_ms']['misses'], 1)

    def test_moving_reading_position_cannot_pass(self):
        native, load = self.fixture()
        native['samples'][1]['scroll_y'] += 50
        self.assertFalse(module.audit(native, load)['all_pauses_ready'])

    def test_incomplete_or_failed_native_run_cannot_be_accepted(self):
        native, load = self.fixture()
        native['error'] = 'DPI changed'
        with self.assertRaises(AssertionError):
            module.audit(native, load)
        native['error'] = None
        native['samples'].pop()
        with self.assertRaises(AssertionError):
            module.audit(native, load)

    def test_fully_visible_fixture_is_not_a_fast_scroll_pass(self):
        native, load = self.fixture()
        for sample in native['samples']:
            sample['scroll_y'] = 0
        with self.assertRaises(AssertionError):
            module.audit(native, load)


if __name__ == '__main__':
    unittest.main()
