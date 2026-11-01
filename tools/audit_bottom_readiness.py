"""Readiness after one native bottom jump, with no repeated scroll correction."""
import argparse
import hashlib
import json
from pathlib import Path


def audit(native, load):
    assert native['error'] is None and native['pattern'] == 'jump_to_current_bottom'
    assert native['input_mode'] == 'pointer-neutral-unfocused'
    assert load['pending_geometry_scope'] == 'possible-next-card-column-span'
    assert native['samples'] and load['rows']
    began = native['samples'][0]['app_elapsed_ms']
    rows = [r for r in load['rows'] if r['elapsed_ms'] >= began]
    assert len(rows) >= 3
    last = rows[-1]
    identities = last['visible_ids']
    stable = bool(identities) and all(r['visible_ids'] == identities for r in rows[-3:])
    geometry_ready = stable and all(r['visible_pending_geometry'] == 0 for r in rows[-3:])

    def event(field, final_target):
        if final_target and not geometry_ready:
            return None
        for row in rows:
            if final_target and (row['visible_ids'] != identities or row['visible_pending_geometry']):
                continue
            required = row['visible_cards'] if final_target else 1
            if required > 0 and row[field] >= required:
                return max(0., row['elapsed_ms'] - began)
        return None

    clear = event('visible_clear_ready', True)
    failures = max(r['failures'] for r in rows)
    return dict(sizing_priority_scope=native['sizing_priority_scope'],
                view_preparation_mode=native.get('view_preparation_mode'),
                pending_geometry_scope=load['pending_geometry_scope'],
                media=last['media'], media_count_stable=len({r['media'] for r in rows}) == 1,
                jump_target_scroll_y=native['jump_target_scroll_y'],
                final_scroll_y=last['layout']['scroll_y'],
                final_visible_ids=identities, final_visible_cards=last['visible_cards'],
                final_visible_pending_geometry=last['visible_pending_geometry'],
                final_target_stable=stable, final_target_geometry_ready=geometry_ready,
                view_reuse_observed=any(r['layout'].get('view_reused',False) for r in rows),
                max_layout_build_ms=max((r['layout'].get('build_ms',0.) for r in rows), default=0.),
                first_any_preview_ms=event('visible_ready', False),
                final_target_all_preview_ms=event('visible_ready', True),
                final_target_all_clear_ms=clear,
                max_failures=failures, complete=clear is not None and failures == 0,
                scope='One native bottom jump after10s first-screen warmup; normal browser anchors thereafter. '
                      'Final visible IDs must stabilize and potential-column geometry must be ready. '
                      'One visit, no P95; no physical presentation or full-source hash claim.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native', type=Path, required=True)
    parser.add_argument('--load', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = audit(json.loads(args.native.read_text(encoding='utf-8-sig')),
                   json.loads(args.load.read_text(encoding='utf-8-sig')))
    result.update(native=str(args.native), load=str(args.load),
                  native_sha256=hashlib.sha256(args.native.read_bytes()).hexdigest(),
                  load_sha256=hashlib.sha256(args.load.read_bytes()).hexdigest())
    with args.output.open('x', encoding='utf8') as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
    print(json.dumps(result, ensure_ascii=False))
