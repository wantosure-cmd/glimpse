import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'tools'))
from audit_bottom_readiness import audit


def native():
    return dict(error=None, pattern='jump_to_current_bottom', input_mode='pointer-neutral-unfocused',
                sizing_priority_scope='possible-next-card-column-span', jump_target_scroll_y=500.,
                samples=[dict(app_elapsed_ms=1000.)])


def row(time, identities, ready, pending=0, failures=0):
    return dict(elapsed_ms=time, visible_ids=identities, visible_cards=len(identities),
                visible_pending_geometry=pending, visible_ready=ready, visible_clear_ready=ready,
                failures=failures, media=10, layout=dict(scroll_y=500.))


class BottomReadiness(unittest.TestCase):
    def run_audit(self, rows):
        return audit(native(), dict(pending_geometry_scope='possible-next-card-column-span', rows=rows))

    def test_visible_pixels_do_not_hide_unknown_column_geometry(self):
        result = self.run_audit([row(t, [1], 1, pending=1) for t in (1100., 1200., 1300.)])
        self.assertEqual(result['first_any_preview_ms'], 100.)
        self.assertIsNone(result['final_target_all_clear_ms'])
        self.assertFalse(result['complete'])

    def test_old_visible_identity_cannot_complete_the_final_view(self):
        result = self.run_audit([row(1100., [9], 1), row(1200., [1,2], 1),
                                 row(1300., [1,2], 2), row(1400., [1,2], 2)])
        self.assertEqual(result['final_target_all_clear_ms'], 300.)
        self.assertTrue(result['complete'])

    def test_empty_tail_and_load_errors_cannot_be_reported_as_acceptance(self):
        empty = self.run_audit([row(t, [], 0) for t in (1100.,1200.,1300.)])
        self.assertFalse(empty['complete'])
        failed = self.run_audit([row(t, [1], 1, failures=1) for t in (1100.,1200.,1300.)])
        self.assertEqual(failed['final_target_all_clear_ms'], 100.)
        self.assertFalse(failed['complete'])


if __name__ == '__main__':
    unittest.main()
