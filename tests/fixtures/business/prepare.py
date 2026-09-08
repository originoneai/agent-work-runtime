#!/usr/bin/env python3
"""Prepare or publish author inputs in a new isolated business fixture; never runs a model."""
import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from support import ROOT, BASE, digest, load_bundle, local_path, read_json, require


def timestamp():
    return datetime.now().astimezone().isoformat(timespec='seconds')


def output_path(value):
    path = Path(value).absolute()
    require(not path.is_symlink(), 'Aliased run directory')
    resolved = path.resolve()
    resolved.relative_to(ROOT/'.local')
    require(resolved != ROOT/'.local', 'Choose a new child run directory')
    require(path == resolved, 'Run directory must use its canonical path')
    return resolved


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n')


def command(project, *args):
    result = subprocess.run(['rtk','proxy','git','-C',str(project),*args],capture_output=True,text=True,timeout=60)
    require(result.returncode == 0, result.stderr or result.stdout)
    return result.stdout.strip()


def create(scenario, output, run_id):
    contract,_,specs = load_bundle()
    require(scenario in specs, 'Unknown scenario')
    require(re.fullmatch(r'[a-z0-9][a-z0-9-]{2,63}', run_id) is not None, 'Run ID must be a new lower-case identifier')
    output = output_path(output)
    require(not output.exists(), 'Never overwrite or reuse a previous run')
    spec,directory,_ = specs[scenario]
    output.mkdir(parents=True)
    project = output/'project'
    shutil.copytree(directory/'project', project)
    manifest = project/'project.toml'
    old = 'external_key = '+json.dumps(spec['namespace'])
    text = manifest.read_text()
    require(text.count(old) == 1, 'Unexpected namespace mapping')
    namespace = spec['namespace']+'/'+run_id
    manifest.write_text(text.replace(old,'external_key = '+json.dumps(namespace)))
    inputs = {str(p.relative_to(directory)):digest(p) for p in sorted(directory.rglob('*')) if p.is_file()}
    marker = {'format':'awr-business-run-1','scenario_id':scenario,'fixture_contract_version':contract['version'],
              'fixture_contract_sha256':digest(BASE/'contract.json'),'fixture_files_sha256':inputs,
              'created_at':timestamp(),'run_id':run_id,'namespace':namespace,'project_root':str(project),
              'phase':'preparing','published_rounds':[],'model_calls':0,'e4_completed':0,
              'participants_bound':False,'live_preconditions_satisfied':False}
    save(output/'run.json',marker)
    generated = spec.get('generated_material')
    if generated:
        control = output/'control'
        control.mkdir()
        log = control/'operations.jsonl'
        with log.open('x', encoding='utf-8') as stream:
            index = 0
            while stream.tell() <= generated['minimum_bytes']:
                batch = index % 47
                day = 1 if batch < 12 else 2 if batch < 22 else 3 if batch < 36 else 4
                row = {'event':run_id+'-'+str(index),'day':day,'batch':f'public-batch-{batch+1:02}',
                       'stage':['queued','parsed','recorded'][index % 3],
                       'result':'retry' if index==17 else 'delayed' if index in (45,46) else 'ok',
                       'elapsed_ms':20+(index % 31), 'note':'public synthetic operations record'}
                stream.write(json.dumps(row,separators=(',',':'))+'\n')
                index += 1
        restricted = control/'restricted-note.md'
        restricted.write_text('private_prompt: synthetic-restricted-material-do-not-share\n')
        marker['generated_material'] = {'source':'control/operations.jsonl','target':generated['target'],
            'sha256':digest(log),'size_bytes':log.stat().st_size,'records':index,
            'restricted_source':'control/restricted-note.md','restricted_sha256':digest(restricted)}
    command(project,'init','--initial-branch=main')
    command(project,'add','--',*[str(p.relative_to(project)) for p in sorted(project.rglob('*')) if p.is_file() and '.git' not in p.parts])
    command(project,'-c','user.name=AWR Fixture','-c','user.email=fixtures@example.invalid',
            'commit','-m','chore: seed independent synthetic business inputs')
    marker.update(phase='ready',seed_commit=command(project,'rev-parse','HEAD'),
                  project_source_sha256={str(p.relative_to(project)):digest(p) for p in sorted(project.rglob('*')) if p.is_file() and '.git' not in p.parts})
    save(output/'run.json',marker)
    return marker


def publish_round(output, number):
    output = output_path(output)
    marker = read_json(output/'run.json')
    require(marker['format']=='awr-business-run-1' and marker['phase']=='ready', 'Inspect incomplete preparation/publication before continuing')
    require(marker['published_rounds']==list(range(1,number)) and number in (1,2), 'Publish each round once and in order')
    contract,_,specs=load_bundle()
    require(marker['fixture_contract_version']==contract['version'] and marker['fixture_contract_sha256']==digest(BASE/'contract.json'), 'Fixture contract changed after preparation')
    spec,directory,_=specs[marker['scenario_id']]
    require(all(digest(directory/name)==value for name,value in marker['fixture_files_sha256'].items()), 'Fixture definition changed after preparation')
    project=output/'project'
    require(str(project.resolve())==marker['project_root'] and not project.is_symlink(), 'Project root changed')
    changes=[]
    for change in spec['followups'][number-1]['publish']:
        changes.append((local_path(directory,change['source']),change['target'],change['expected_previous_sha256']))
    if spec.get('generated_material') and spec['generated_material']['publish_round']==number:
        generated=marker['generated_material']
        source=local_path(output,generated['source'])
        require(digest(source)==generated['sha256'],'Generated public material changed')
        changes.append((source,generated['target'],None))
    prepared=[]
    for source,name,expected in changes:
        target=local_path(project,name,exists=False)
        actual=digest(target) if target.exists() else None
        require(actual==expected,'Author update conflicts with current project content: '+name)
        prepared.append((source,target,{'path':name,'before_sha256':actual,'after_sha256':digest(source)}))
    marker['phase']='publishing'
    marker['pending_round']=number
    save(output/'run.json',marker)
    for source,target,_ in prepared:
        target.parent.mkdir(parents=True,exist_ok=True)
        # Only paused, marked temporary fixtures are eligible. On interruption,
        # retain the publishing marker; never blindly replay a partial round.
        with tempfile.NamedTemporaryFile(dir=target.parent,prefix='.author-input-',delete=False) as stream:
            temporary=Path(stream.name)
            stream.write(source.read_bytes())
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary,target)
    receipt={'kind':'fixture_author_input_publication','scenario_id':spec['scenario_id'],'round':number,
             'published_at':timestamp(),'changes':[row for _,_,row in prepared],
             'actual_user_followup_submitted':False,'business_followup_completed':False,'e4_completed':0}
    save(output/f'round-{number}-inputs.json',receipt)
    marker['phase']='ready'
    marker.pop('pending_round')
    marker['published_rounds'].append(number)
    save(output/'run.json',marker)
    return receipt


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='action',required=True)
    create_parser=sub.add_parser('create')
    create_parser.add_argument('--scenario',required=True)
    create_parser.add_argument('--output',type=Path,required=True)
    create_parser.add_argument('--run-id',required=True)
    publish=sub.add_parser('publish-round')
    publish.add_argument('--output',type=Path,required=True)
    publish.add_argument('--round',type=int,choices=[1,2],required=True)
    args=parser.parse_args()
    value=create(args.scenario,args.output,args.run_id) if args.action=='create' else publish_round(args.output,args.round)
    print(json.dumps(value,ensure_ascii=False,indent=2))


if __name__=='__main__':
    main()
