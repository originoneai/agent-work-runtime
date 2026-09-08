"""Shared definition checks for the business fixture pack; never assigns E4 status."""
import hashlib
import json
from pathlib import Path
import re
import tomllib

import yaml

ROOT = Path(__file__).resolve().parents[3]
BASE = Path(__file__).resolve().parent


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
    require((directory/'prompts/initial.md').read_text().strip() == spec['initial_request'], 'Client prompt differs from its contract')
    texts = [spec['initial_request'], *(r['request'] for r in spec['followups'])]
    if spec.get('prelude'):
        texts.append(spec['prelude']['request'])
        require(len(spec['prelude']['required_live_preconditions']) >= 2, 'Prelude needs actual execution preconditions')
        require((directory/'prompts/prelude.md').read_text().strip() == texts[-1], 'Prelude input mismatch')
    forbidden = re.compile(r'AWR-SC-\d|预期答案|测试编号|只回复\s*OK|Reply OK only|--expected-revision|state\.db', re.IGNORECASE)
    require(not any(forbidden.search(text) for text in texts), 'Evaluator detail leaked into client input')
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
