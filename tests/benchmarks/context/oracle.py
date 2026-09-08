"""Exact source-fact assertions, independent of AWR's context selection code."""
import json


def assess(oracle, work, bootstrap, compiled):
    """Return each check rather than hiding failures in an aggregate score."""
    expected = oracle['work']
    l0 = bootstrap['rendered_context']
    l1 = compiled['work_context']['rendered_context']
    current = bootstrap['context']['work']
    identity = compiled['work_context']['identity']
    result = {}

    def add(category, name, passed):
        result.setdefault(category, {})[name] = bool(passed)

    key = expected['id']
    add('work_item_id', 'source_key', work['external_key'] == key == current['external_key'] == identity['work_item_key'])
    add('work_item_id', 'runtime_identity', work['id'] == current['id'] == identity['work_item_id'])
    add('work_item_id', 'rendered_identity', all(v in l0 and v in l1 for v in [key, work['id']]))
    status = expected['status']
    add('status', 'current_source_status', all(v['status'] == status == v['raw_status'] for v in [work, current]))
    add('status', 'rendered_status', f'Status: {json.dumps(status)}' in l1 and f'Status: {status}' in l0)
    action = expected['next_action']
    add('next_action', 'verbatim', action and work['next_action'] == action == current['next_action'] and action in l0 and action in l1)
    add('acceptance', 'source_list', work['acceptance'] == expected['acceptance'] and bool(expected['acceptance']))
    for i, value in enumerate(expected['acceptance']):
        add('acceptance', f'verbatim_{i}', bool(value) and value in l1)
    blocker = expected.get('blocker')
    add('blocker', 'source_value', work['blocker'] == blocker == current['blocker'])
    add('blocker', 'presence', f'Blocker present: {str(blocker is not None).lower()}' in l1)
    add('blocker', 'verbatim_or_explicit_absence', blocker in l0 and blocker in l1 if blocker else 'Blocker: none' in l0)

    rules = bootstrap['context']['critical_rules']
    observed = {rule['external_key'].rsplit('#', 1)[-1]: rule['text'] for rule in rules}
    expected_rules = {rule['key']: rule['text'] for rule in oracle['rules']}
    add('applicable_hard_rules', 'exact_set', len(observed) == len(rules) and observed == expected_rules)
    for i, rule in enumerate(oracle['rules']):
        add('applicable_hard_rules', f'verbatim_{i}', rule['text'] in l0 and rule['text'] in l1)

    pending = oracle['unresolved_dependencies']
    add('unresolved_required_dependencies', 'exact_set', sorted(d['id'] for d in pending) ==
        sorted(compiled['completeness']['unresolved_required_dependencies']))
    chunks = compiled['work_context']['selected_chunks']
    for i, dependency in enumerate(pending):
        key = dependency['id']
        add('unresolved_required_dependencies', f'required_identity_{i}', key in l1 and any(
            c['key'] == 'required:' + key and c['required'] and c['section'] == 'dependencies' for c in chunks))
        add('unresolved_required_dependencies', f'status_{i}',
            f'Status: {json.dumps(dependency["status"])} (raw {json.dumps(dependency["status"])})' in l1)
        for field in ['next_action', 'blocker']:
            add('unresolved_required_dependencies', f'{field}_{i}', bool(dependency.get(field)) and dependency[field] in l1)
    total = sum(len(checks) for checks in result.values())
    passed = sum(sum(checks.values()) for checks in result.values())
    return {'categories': result, 'assertions': total, 'passed': passed, 'recall': passed / total}
