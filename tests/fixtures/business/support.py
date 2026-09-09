"""Shared definition checks for the business fixture pack; never assigns E4 status."""
import hashlib
import json
from pathlib import Path
import re
import tomllib

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = Path(__file__).resolve().parent

CLIENT_INPUT_POLICY_ID = 'awr-real-client-input-preflight'
CLIENT_INPUT_POLICY_VERSION = '1.0.0'
CLIENT_INPUT_KINDS = (
    'canonical',
    'prelude',
    'technical-supplement',
    'rework',
    'final-delivery',
)

# These rules describe syntax classes, not scenario-specific answers. Exact
# project/work/gate identifiers are added from the current contract and graphs.
CLIENT_INPUT_PATTERNS = (
    ('internal_scenario_id', r'(?<![0-9A-Za-z_-])AWR-SC-[0-9]+(?![0-9A-Za-z_-])'),
    ('native_uuid', r'(?<![0-9A-Fa-f])[0-9A-Fa-f]{8}-(?:[0-9A-Fa-f]{4}-){3}[0-9A-Fa-f]{12}(?![0-9A-Fa-f])'),
    ('native_ulid', r'(?<![0-9A-Za-z])[0-7][0-9A-HJKMNP-TV-Z]{25}(?![0-9A-Za-z])'),
    ('native_item_id', r'(?<![0-9A-Za-z_])(?:item_[0-9]+|(?:call|tool)_[0-9A-Za-z_-]{6,})(?![0-9A-Za-z_-])'),
    ('cli_state_parameter', r'--(?:expected-revision|session|agent|provider|model|claim|ttl-ms|context-hash|checkpoint|work|project|json)(?:\b|=)'),
    ('awr_cli_state_command', r'(?<![0-9A-Za-z_])(?:rtk\s+(?:proxy\s+)?)?(?:[./0-9A-Za-z_-]+/)?awr\s+(?:(?:--project\s+\S+|--json)\s+)*(?:session\s+(?:start|resume|checkpoint|end)|work\s+(?:claim|progress|block|unblock|cancel|reopen|complete|release|handoff))\b'),
    ('quoted_work_state_command', r'`\s*(?:awr\s+)?work\s+(?:claim|progress|block|unblock|cancel|reopen|complete|release|handoff)\b[^`]*`'),
    ('direct_session_state_command', r'(?<![0-9A-Za-z_])/?session\s+(?:start|resume|checkpoint|end)\b'),
    ('runtime_state_path', r'(?<![0-9A-Za-z_])(?:\.awr/)?state\.db(?![0-9A-Za-z_])'),
    ('expected_answer', r'(?:预期|期望|正确)(?:答案|答复|回复)|expected\s+(?:answer|response)'),
    ('reply_ok_only', r'(?:只|仅)(?:需|要)?回复\s*[`\'\"]?OK\b|(?:reply|respond\s+with)\s+[`\'\"]?OK[`\'\"]?\s+only\b'),
    ('test_marker', r'测试(?:编号|用例(?:编号)?)|test\s*(?:case\s*)?id\b|\bE4\b'),
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pairs(rows):
    result = {}
    for key, value in rows:
        require(key not in result, 'Duplicate object key: ' + str(key))
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(), object_pairs_hook=pairs)


class UniqueLoader(yaml.SafeLoader):
    pass


def unique_yaml(loader, node, deep=False):
    return pairs((loader.construct_object(key, deep=deep), loader.construct_object(value, deep=deep))
                 for key, value in node.value)


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_yaml)


def read_yaml(path):
    return yaml.load(path.read_text(), Loader=UniqueLoader)


def client_input_policy():
    """Bind client-input checks to the current fixture contract and work graphs."""
    fixture_contract_path = BASE / 'contract.json'
    fixture_contract = read_json(fixture_contract_path)
    authority_path = ROOT / fixture_contract['authority']['contract']
    authority = read_json(authority_path)
    require(fixture_contract['authority']['version'] == authority['version'],
            'Client-input authority version mismatch')
    require(fixture_contract['authority']['sha256'] == digest(authority_path),
            'Client-input authority hash mismatch')

    identifiers = {
        'scenario_ids': set(),
        'namespaces': set(),
        'contract_work_keys': set(authority['scope']['required_work_item_ids']) |
                              set(authority['scope']['preparation_work_item_ids']),
        'graph_work_keys': set(),
        'gate_ids': set(authority['completion']['scenario_required_gates']),
    }
    graph_bindings = []
    for scenario in authority['scenarios']:
        identifiers['scenario_ids'].add(scenario['id'])
        identifiers['namespaces'].add(scenario['namespace'])
        graph_path = ROOT / scenario['work_graph']
        graph = read_yaml(graph_path)
        identifiers['graph_work_keys'].update(node['key'] for node in graph['nodes'])
        graph_bindings.append({
            'path': str(graph_path.relative_to(ROOT)),
            'sha256': digest(graph_path),
        })

    serializable_identifiers = {
        key: sorted(values) for key, values in identifiers.items()
    }
    rule_sources = [
        {'path': 'docs/RULES.md', 'sha256': digest(ROOT/'docs/RULES.md')},
        {'path': 'docs/acceptance/README.md',
         'sha256': digest(ROOT/'docs/acceptance/README.md')},
    ]
    implementation = {
        'path': str(Path(__file__).resolve().relative_to(ROOT)),
        'sha256': digest(Path(__file__).resolve()),
    }
    rules = {
        'policy_id': CLIENT_INPUT_POLICY_ID,
        'policy_version': CLIENT_INPUT_POLICY_VERSION,
        'patterns': list(CLIENT_INPUT_PATTERNS),
        'identifiers': serializable_identifiers,
        'matching': 'case-insensitive exact identifiers plus syntax-class patterns',
        'rule_sources': rule_sources,
        'implementation': implementation,
    }
    rules_sha256 = hashlib.sha256(json.dumps(
        rules, ensure_ascii=False, sort_keys=True, separators=(',', ':'),
    ).encode()).hexdigest()
    return {
        **rules,
        'rules_sha256': rules_sha256,
        'fixture_contract': {
            'path': str(fixture_contract_path.relative_to(ROOT)),
            'version': fixture_contract['version'],
            'sha256': digest(fixture_contract_path),
        },
        'authority_contract': {
            'path': str(authority_path.relative_to(ROOT)),
            'version': authority['version'],
            'sha256': digest(authority_path),
        },
        'work_graphs': graph_bindings,
    }


def client_input_violations(text, policy=None):
    """Return deterministic leak findings without submitting or mutating input."""
    policy = policy or client_input_policy()
    findings = []
    if not isinstance(text, str) or not text.strip():
        return [{
            'code': 'empty_client_input',
            'detail': 'Actual client input must contain natural business language.',
        }]

    for category, values in policy['identifiers'].items():
        for identifier in values:
            match = re.search(
                r'(?<![0-9A-Za-z_/-])' + re.escape(identifier) +
                r'(?![0-9A-Za-z_/-])',
                text,
                re.IGNORECASE,
            )
            if match:
                findings.append({
                    'code': 'known_internal_identifier',
                    'category': category,
                    'match': match.group(0),
                    'detail': 'Known evaluator-side identifier must not be sent to the client.',
                })

    for code, expression in policy['patterns']:
        for match in re.finditer(expression, text, re.IGNORECASE | re.MULTILINE):
            findings.append({
                'code': code,
                'match': match.group(0),
                'detail': {
                    'internal_scenario_id': 'Internal scenario identifier must stay evaluator-side.',
                    'native_uuid': 'Native UUID-shaped object identifier must stay evaluator-side.',
                    'native_ulid': 'Native ULID-shaped object identifier must stay evaluator-side.',
                    'native_item_id': 'Native transcript/tool item identifier must stay evaluator-side.',
                    'cli_state_parameter': 'CLI runtime/state parameter must not instruct the actual client.',
                    'awr_cli_state_command': 'AWR CLI state-machine command must not be embedded in business input.',
                    'quoted_work_state_command': 'Quoted work state-machine command must not be embedded in business input.',
                    'direct_session_state_command': 'Direct session state-machine command must not be embedded in business input.',
                    'runtime_state_path': 'Runtime state database details must stay evaluator-side.',
                    'expected_answer': 'Expected-answer language makes the input an evaluator probe.',
                    'reply_ok_only': 'A forced OK-only response is an evaluator probe.',
                    'test_marker': 'Test/evaluation marker must stay evaluator-side.',
                }[code],
            })

    unique = []
    seen = set()
    for finding in findings:
        identity = (finding['code'], finding.get('category'), finding.get('match'))
        if identity not in seen:
            seen.add(identity)
            unique.append(finding)
    return unique


def validate_client_input(text, policy=None):
    """Reject known evaluator leakage; independent review remains required."""
    findings = client_input_violations(text, policy)
    require(not findings, 'Evaluator detail leaked into client input: ' +
            ', '.join(sorted({finding['code'] for finding in findings})))
    return True


def local_path(root, name, exists=True):
    relative = Path(name)
    require(not relative.is_absolute() and relative.parts and '..' not in relative.parts,
            'Expected a contained relative path: ' + name)
    path = root / relative
    cursor = root
    for part in relative.parts:
        cursor = cursor / part
        require(not cursor.is_symlink(), 'Aliased fixture path: ' + name)
    path.resolve().relative_to(root.resolve())
    if exists:
        require(path.is_file(), 'Missing fixture file: ' + name)
    return path


def validate_spec(spec, canonical, directory):
    require(spec['scenario_id'] == canonical['id'] and spec['namespace'] == canonical['namespace'], 'Scenario identity mismatch')
    require(spec['fixture_version'] == '1.0.0', 'Unknown fixture definition version')
    require(spec['required_gates'] == canonical['required_gates'], 'Missing or changed business gate')
    require(spec['dimensions'] == canonical['dimensions'], 'Coverage differs from the V1 contract')
    require(spec['initial_request'] == canonical['initial_request'], 'Initial input changed')
    require([r['request'] for r in spec['followups']] == canonical['followups'], 'Two canonical business followups are required')
    require([r['number'] for r in spec['followups']] == [1, 2], 'Followup ordering changed')
    texts = [spec['initial_request'], *(r['request'] for r in spec['followups'])]
    if spec.get('prelude'):
        texts.append(spec['prelude']['request'])
        require(len(spec['prelude']['required_live_preconditions']) >= 2, 'Prelude needs actual execution preconditions')
    policy = client_input_policy()
    for text in texts:
        validate_client_input(text, policy)
    require((directory/'prompts/initial.md').read_text().strip() == spec['initial_request'], 'Client prompt differs from its contract')
    if spec.get('prelude'):
        require((directory/'prompts/prelude.md').read_text().strip() == texts[-1], 'Prelude input mismatch')
    project = directory / spec['project_directory']
    require(project.resolve() == (directory/'project').resolve() and not project.is_symlink(), 'Unexpected project source root')
    manifest = tomllib.loads((project/'project.toml').read_text())
    require(manifest['project']['external_key'] == canonical['namespace'], 'Project namespace mismatch')
    source_mapping = {(s['domain'], s['role'], s['path']) for s in manifest['sources']}
    require(source_mapping == {('goal','primary','GOALS.md'),('plan','primary','PLAN.md'),('rules','primary','RULES.md'),('ledger','primary','work-ledger.yaml')}, 'Unexpected authority mappings')
    for source in manifest['sources']:
        local_path(project, source['path'])
    rules = (project/'RULES.md').read_text()
    require(rules.count('severity=hard scope=project value=*') >= 2, 'Source authority and business hard rules are required')
    ledger = read_yaml(project/'work-ledger.yaml')
    require(ledger['milestones'] and len(ledger['work_items']) >= 4, 'Incomplete business work source')
    rows = {row['id']: row for row in ledger['work_items']}
    require(len(rows) == len(ledger['work_items']), 'Duplicate work source identity')
    require(spec['entry_work'] in rows and not rows[spec['entry_work']]['depends_on'], 'Entry work is not independent')
    require(all(row['status'] in ('planned','ready') and row.get('owner') is None and not row.get('evidence') for row in rows.values()), 'Seeded execution, ownership or completion evidence')
    require(all(row.get('acceptance') and row.get('next_action') for row in rows.values()), 'Work lacks acceptance or next action')
    graph = read_yaml(local_path(directory, spec['work_graph']))
    require(graph['namespace'] == spec['namespace'], 'Work graph namespace mismatch')
    nodes = {node['key']: node for node in graph['nodes']}
    require(len(nodes) == len(graph['nodes']) and set(nodes) == set(rows), 'Graph and source work differ')
    participants = spec['participants']
    require('executor' in participants and 'reviewer' in participants, 'Missing executor or reviewer role')
    producers = set(participants) - {'reviewer'}
    require(set(spec['reviewer_distinct_from']) == producers and set(graph['reviewer_distinct_from']) == producers,
            'Independent reviewer must differ from every producer')
    visiting, visited = set(), set()

    def walk(key):
        require(key not in visiting, 'Work graph cycle')
        if key in visited:
            return
        visiting.add(key)
        node = nodes[key]
        require(node['role'] in participants, 'Unknown work role')
        require(node['depends_on'] == rows[key]['depends_on'], 'Graph dependency differs from source')
        for dependency in node['depends_on']:
            require(dependency in nodes, 'Cross-fixture or missing dependency')
            walk(dependency)
        visiting.remove(key)
        visited.add(key)
    for key in nodes:
        walk(key)
    require(any(n['role'] == 'reviewer' and n['depends_on'] for n in nodes.values()), 'No independent review work')
    require(len(spec['review_rubric']) >= 3, 'Review needs substantive criteria')
    artifacts = spec['artifacts']
    require(set(canonical['expected_artifacts']) <= {a['title'] for a in artifacts}, 'Missing required business artifact')
    require(len({a['path'] for a in artifacts}) == len(artifacts), 'Reused artifact path')
    require(any(a['producer'] == 'reviewer' for a in artifacts), 'Missing independent review artifact')
    for artifact in artifacts:
        require(artifact['producer'] in participants and artifact['path'].startswith('deliverables/'), 'Invalid artifact ownership or path')
        require(not local_path(project, artifact['path'], exists=False).exists(), 'Output must be created by actual execution')
    require(spec['artifact_directory'] == canonical['artifact_directory'], 'Delivery evidence path changed')
    for number, round_ in enumerate(spec['followups'], 1):
        require((directory/f'prompts/followup-{number}.md').read_text().strip() == round_['request'], 'Followup prompt mismatch')
        require(round_['publish'] and round_['required_observation'], 'Followup needs business material and a meaningful observation')
        for change in round_['publish']:
            source = local_path(directory, change['source'])
            require(change['source'].startswith(f'updates/round-{number}/') and source.stat().st_size > 0, 'Unversioned followup material')
            require(change['target'] in ('RULES.md','PLAN.md','GOALS.md') or change['target'].startswith('materials/'), 'Forbidden author-update target')
            target = local_path(project, change['target'], exists=False)
            expected = digest(target) if target.exists() else None
            require(change['expected_previous_sha256'] == expected, 'Author-update precondition differs from the seed')
    if 'multi_agent' in spec['dimensions']:
        require('executor_peer' in participants, 'Parallel scenario needs separate execution roles')
        require(any(n['role'] == 'executor_peer' for n in nodes.values()), 'Peer has no independent work')
    if 'mcp' in spec['dimensions']:
        require(spec['native_mcp_required'] and spec['minimum_real_clients'] >= 2 and 'previous_executor' in participants, 'Cross-client scenario lacks actual client/MCP requirements')
    if {'session_change','crash','branch_merge'} & set(spec['dimensions']):
        require(spec.get('prelude'), 'Recovery/handoff needs a real prelude, not seeded runtime state')
    generated = spec.get('generated_material')
    if 'size_limit' in spec['dimensions']:
        require(generated and generated['minimum_bytes'] > 2*1024*1024 and generated['publish_round'] == 1, 'Missing large-material boundary fixture')
        require(generated['restricted_file_outside_project'], 'Restricted material must stay outside the client project')
    require(not (project/'.awr').exists() and not (project/'deliverables').exists(), 'Runtime or artifact results seeded in the source pack')
    return {'work_keys':sorted(rows), 'roles':sorted(participants), 'artifact_paths':[spec['artifact_directory']+'/'+a['path'] for a in artifacts]}


def load_bundle():
    contract = read_json(BASE/'contract.json')
    authority = read_json(ROOT/contract['authority']['contract'])
    require(contract['authority']['version'] == authority['version'] and contract['authority']['sha256'] == digest(ROOT/contract['authority']['contract']), 'V1 authority changed; update this fixture contract explicitly')
    canonical = {s['id']:s for s in authority['scenarios']}
    require(len(contract['scenarios']) == len(canonical) == contract['target_fixtures'] == 8, 'Fixture scope changed')
    require(all(s['required_gates'] == contract['required_gates'] for s in canonical.values()), 'Catalog gate scope differs from V1')
    gate_contract = read_json(BASE/'gate-contract.json')
    require(gate_contract['version'] == '1.0.0' and gate_contract['status_authority'] == contract['status_authority'], 'Evidence contract version/authority changed')
    require(set(gate_contract['gates']) == set(contract['required_gates']), 'Evidence gate contract differs')
    require(all(row['required_evidence'] and row['not_sufficient'] for row in gate_contract['gates'].values()), 'Empty evidence gate requirement')
    specs, keys, namespaces, artifacts = {}, set(), set(), set()
    for row in contract['scenarios']:
        key = row['id']
        require(key in canonical and key not in specs, 'Unknown or duplicate scenario')
        path = local_path(ROOT, row['specification'])
        require(path.parent == ROOT/canonical[key]['fixture'], 'Fixture directory differs from V1')
        spec = read_json(path)
        result = validate_spec(spec, canonical[key], path.parent)
        require(row['namespace'] == spec['namespace'] and row['dimensions'] == spec['dimensions'], 'Catalog entry mismatch')
        require(spec['namespace'] not in namespaces and not keys.intersection(result['work_keys']), 'Cross-scenario identity reuse')
        require(not artifacts.intersection(result['artifact_paths']), 'Cross-scenario output reuse')
        namespaces.add(spec['namespace'])
        keys.update(result['work_keys'])
        artifacts.update(result['artifact_paths'])
        specs[key] = (spec, path.parent, result)
    return contract, authority, specs


def coverage(specs):
    dimensions = {}
    for key,(spec,_,_) in specs.items():
        for dimension in spec['dimensions']:
            dimensions.setdefault(dimension, []).append(key)
    return {dimension:sorted(keys) for dimension, keys in sorted(dimensions.items())}
