#!/usr/bin/env python3
"""Validate all fixture definitions and exercise their intake and author updates, not E4."""
import argparse
import copy
from datetime import datetime
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from prepare import create, output_path, publish_round
from support import (
    BASE,
    ROOT,
    client_input_policy,
    coverage,
    digest,
    load_bundle,
    read_yaml,
    require,
    validate_spec,
)


def render(contract, specs):
    matrix={'contract_id':contract['contract_id'],'version':contract['version'],
            'scenario_count':len(specs),'work_node_count':sum(len(s[2]['work_keys']) for s in specs.values()),
            'dimensions':coverage(specs),'required_gates':contract['required_gates'],
            'status_authority':contract['status_authority'],'execution_status_inferred':False}
    lines=['# 业务场景覆盖矩阵','',
           '本表由版本化 fixture 合同生成，只描述材料与覆盖安排。实际执行记录保留在本地，准备通过不计 E4。','',
           '| 场景 | 业务背景 | 工作节点 | 角色 | 覆盖维度 |',
           '| --- | --- | ---: | --- | --- |']
    for key,(spec,_,result) in specs.items():
        lines.append('| '+ ' | '.join([key,spec['business_title'],str(len(result['work_keys'])),', '.join(result['roles']),', '.join(spec['dimensions'])])+' |')
    lines += ['', '所有场景都要求自然发起、实际执行与产物、两轮业务追问、独立复核、最终交付、可追溯回执、独立 fixture，以及独立提交和远端 SHA。', '',
              '恢复、并行与跨客户端场景还要求真实前序过程；准备工具不会创建会话、claim、checkpoint、事件、通过记录或交付产物。', '',
              '本矩阵不提供性能、Token 消耗或真实客户端执行结果。', '']
    return matrix,'\n'.join(lines)


def fingerprints():
    return {str(p.relative_to(ROOT)):digest(p) for p in sorted(BASE.rglob('*'))
            if p.is_file() and '__pycache__' not in p.parts}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--render',action='store_true',help='Regenerate the derived coverage files; no runtime verification')
    parser.add_argument('--output',type=Path,help='New local directory for all eight intake fixtures and receipts')
    parser.add_argument('--awr',type=Path,default=ROOT/'target/debug/awr')
    args=parser.parse_args()
    contract,authority,specs=load_bundle()
    matrix,markdown=render(contract,specs)
    matrix_path=BASE/'coverage.json'
    markdown_path=BASE/'coverage.md'
    if args.render:
        require(args.output is None,'Render definitions separately from an execution run')
        matrix_path.write_text(json.dumps(matrix,ensure_ascii=False,indent=2)+'\n')
        markdown_path.parent.mkdir(parents=True,exist_ok=True)
        markdown_path.write_text(markdown)
        print('Rendered eight fixture definitions; no runtime or E4 execution.')
        return 0
    require(args.output is not None,'Provide --output for actual intake checks, or --render for definitions')
    require(not subprocess.check_output(['rtk','proxy','git','status','--porcelain'],cwd=ROOT,text=True).strip(),
            'Commit reviewed fixture definitions first so verification binds an exact source commit')
    require(matrix_path.read_text()==json.dumps(matrix,ensure_ascii=False,indent=2)+'\n' and markdown_path.read_text()==markdown,'Coverage index is stale; render it before verification')
    output=output_path(args.output)
    require(not output.exists(),'Use a new verification output directory')
    output.mkdir(parents=True)
    binary=args.awr.resolve(strict=True)
    before=fingerprints()
    input_policy=client_input_policy()
    report={'work_item':'AWR-QA-002','contract_id':contract['contract_id'],'contract_version':contract['version'],
            'contract_sha256':digest(BASE/'contract.json'),'checked_at':datetime.now().astimezone().isoformat(timespec='seconds'),
            'source_commit':subprocess.check_output(['rtk','proxy','git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'source_tree':subprocess.check_output(['rtk','proxy','git','rev-parse','HEAD^{tree}'],cwd=ROOT,text=True).strip(),
            'clean_source':True,
            'fixture_input_sha256':before,'binary_sha256':digest(binary),'matrix':matrix,'passed':False,
            'definition_rejections':[],'fixtures':[],'e4_credit_from_this_run':0,'native_client_invoked':False,
            'model_calls':0,'independent_business_review_performed':False,
            'client_input_preflight':{
                'policy_id':input_policy['policy_id'],'policy_version':input_policy['policy_version'],
                'rules_sha256':input_policy['rules_sha256'],
                'fixture_contract':input_policy['fixture_contract'],
                'authority_contract':input_policy['authority_contract'],
                'rule_sources':input_policy['rule_sources'],
                'implementation':input_policy['implementation'],
                'work_graphs':input_policy['work_graphs'],
                'canonical_inputs_checked':sum(
                    3 + (1 if spec.get('prelude') else 0)
                    for spec,_,_ in specs.values()
                ),
                'supplemental_business_inputs_submitted':0,
                'business_completed':False,'e4_credit':0,
            }}

    def save():
        (output/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')

    # Falsify the definition check with real missing contract elements, rather
    # than declaring a well-formed JSON document sufficient for a business loop.
    key=next(iter(specs))
    original,directory,definition=specs[key]
    canonical=next(c for c in authority['scenarios'] if c['id']==key)
    mutations={
        'missing_followup':lambda s,c:s['followups'].pop(),
        'missing_hard_gate':lambda s,c:s['required_gates'].pop(),
        'reviewer_not_independent':lambda s,c:s['reviewer_distinct_from'].clear(),
        'missing_required_artifact':lambda s,c:s['artifacts'].pop(0),
        'changed_namespace':lambda s,c:s.update(namespace='reused/namespace'),
        'leaked_evaluator_input':lambda s,c:(
            s.update(initial_request=s['initial_request']+' '+definition['work_keys'][0]),
            c.update(initial_request=c['initial_request']+' '+definition['work_keys'][0]),
        ),
    }
    for name,mutate in mutations.items():
        damaged=copy.deepcopy(original)
        damaged_canonical=copy.deepcopy(canonical)
        mutate(damaged,damaged_canonical)
        try:
            validate_spec(damaged,damaged_canonical,directory)
        except ValueError as error:
            report['definition_rejections'].append({'case':name,'rejected':True,'reason':str(error)})
        else:
            raise ValueError('Invalid definition accepted: '+name)
    with tempfile.TemporaryDirectory(prefix='definition-cycle-',dir=output) as scratch:
        damaged=Path(scratch)/'fixture'
        shutil.copytree(directory,damaged)
        graph=read_yaml(damaged/'work-graph.yaml')
        ledger=read_yaml(damaged/'project/work-ledger.yaml')
        first,second=graph['nodes'][1:3]
        first['depends_on']=[second['key']]
        for row in ledger['work_items']:
            if row['id']==first['key']:
                row['depends_on']=first['depends_on']
        import yaml
        (damaged/'work-graph.yaml').write_text(yaml.safe_dump(graph,allow_unicode=True,sort_keys=False))
        (damaged/'project/work-ledger.yaml').write_text(yaml.safe_dump(ledger,allow_unicode=True,sort_keys=False))
        try:
            validate_spec(original,canonical,damaged)
        except ValueError as error:
            require('cycle' in str(error),'Cycle fixture failed for a different reason')
            report['definition_rejections'].append({'case':'dependency_cycle','rejected':True,'reason':str(error)})
        else:
            raise ValueError('Cyclic business graph accepted')
    save()
    projects, runtime_work_ids = set(), set()
    conflict_checked=False
    for index,(key,(spec,_,definition)) in enumerate(specs.items(),1):
        print('Checking isolated inputs for '+key,flush=True)
        run=output/f'case-{index}'
        marker=create(key,run,f'{output.name}-case-{index}')
        project=run/'project'
        row={'scenario_id':key,'namespace':marker['namespace'],'project_root':str(project),
             'source_seed_commit':marker['seed_commit'],'commands':[],'passed':False,'business_completed':False}
        report['fixtures'].append(row)
        save()
        marker_bytes=(run/'run.json').read_bytes()
        for negative,operation in [('run_reuse',lambda:create(key,run,marker['run_id'])),
                                   ('out_of_order_round',lambda:publish_round(run,2))]:
            try:
                operation()
            except ValueError:
                require((run/'run.json').read_bytes()==marker_bytes,'Rejected preparation mutated its marker')
                row[negative+'_rejected']=True
            else:
                raise ValueError('Invalid preparation accepted: '+negative)

        def awr(label,*arguments):
            cmd=['rtk','proxy',str(binary),'--project',str(project),'--json',*map(str,arguments)]
            result=subprocess.run(cmd,capture_output=True,text=True,timeout=60)
            receipt=run/(label+'.json')
            receipt.write_text(json.dumps({'command':cmd,'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr},ensure_ascii=False,indent=2)+'\n')
            row['commands'].append({'label':label,'exit_code':result.returncode,'receipt':str(receipt.relative_to(ROOT)),'sha256':digest(receipt)})
            save()
            require(result.returncode==0,label+' failed; inspect '+str(receipt.relative_to(ROOT)))
            return json.loads(result.stdout)

        awr('01-preview','init','--manifest','project.toml')
        initialized=awr('02-init','init','--manifest','project.toml','--accept')
        require(initialized['index']['ok'],'Fixture has source issues')
        status=awr('03-status','status')
        project_id=initialized['index']['project_id']
        require(project_id not in projects,'Project runtime identity was reused')
        projects.add(project_id)
        work=awr('04-entry','work','show',spec['entry_work'])
        require(work['work']['ready'],'Fixture entry work is blocked before execution')
        context=awr('05-context','context','compile','--work',spec['entry_work'],'--detached','--budget','5000')
        require(context['completeness']['complete'],'Initial fixture context is incomplete')
        context_hashes=[context['work_context']['context_hash']]
        import sqlite3
        with sqlite3.connect(f'file:{project/".awr/state.db"}?mode=ro',uri=True) as db:
            ids={value for value, in db.execute('SELECT id FROM work_items')}
            require(len(ids)==len(definition['work_keys']) and not runtime_work_ids.intersection(ids),'Work runtime identities were reused')
            runtime_work_ids.update(ids)
        for number in (1,2):
            round_spec=spec['followups'][number-1]
            if not conflict_checked:
                candidate=next((c for c in round_spec['publish'] if c['expected_previous_sha256']),None)
                if candidate:
                    target=project/candidate['target']
                    original_bytes=target.read_bytes()
                    changed=original_bytes+b'\nA fixture author has an unmerged edit.\n'
                    target.write_bytes(changed)
                    before_marker=(run/'run.json').read_bytes()
                    try:
                        publish_round(run,number)
                    except ValueError as error:
                        require('conflicts' in str(error),'Conflict rejected for an unrelated reason')
                        require(target.read_bytes()==changed and (run/'run.json').read_bytes()==before_marker,'Conflicting author input was overwritten')
                        conflict_checked=True
                        row['author_edit_conflict_preserved']=True
                    else:
                        raise ValueError('Author publication overwrote a concurrent edit')
                    finally:
                        target.write_bytes(original_bytes)
            receipt=publish_round(run,number)
            require(receipt['actual_user_followup_submitted'] is False and receipt['business_followup_completed'] is False,'Input publication claimed execution')
            reindex=awr(f'06-round-{number}-reindex','source','reindex')
            require(reindex['ok'],'Published business input broke source intake')
            context=awr(f'07-round-{number}-context','context','compile','--work',spec['entry_work'],'--detached','--budget','5000')
            require(context['completeness']['complete'],'Updated fixture context is incomplete')
            context_hashes.append(context['work_context']['context_hash'])
            if any(change['target']=='RULES.md' for change in round_spec['publish']):
                require(context_hashes[-1]!=context_hashes[-2],'A changed hard rule kept the old context hash')
                require('追加业务要求' in context['work_context']['rendered_context'],'New hard rule was omitted')
        doctor=awr('08-doctor','doctor')
        require(doctor['ok'] and not doctor['findings'],'Prepared fixture has unexplained Doctor findings')
        with sqlite3.connect(f'file:{project/".awr/state.db"}?mode=ro',uri=True) as db:
            runtime_counts={table:db.execute(f'SELECT count(*) FROM {table}').fetchone()[0] for table in ['sessions','claims','checkpoints','artifacts','mutation_proposals']}
            require(all(count==0 for count in runtime_counts.values()),'Intake fabricated business runtime work')
            locators=[value for value, in db.execute('SELECT locator FROM sources')]
            require(len(locators)==4 and not any('control/' in value or 'restricted-note' in value for value in locators),
                    'Intake included material outside its authority mapping')
        require(all(not (project/artifact['path']).exists() for artifact in spec['artifacts']),'Preparation fabricated a business artifact')
        row.update(passed=True,project_id=project_id,work_count=len(ids),runtime_work_ids=sorted(ids),
                   source_context_hashes=context_hashes,final_context_tokens=context['work_context']['token_estimate'],
                   runtime_counts=runtime_counts,prepared_rounds=[1,2],doctor_findings=0,
                   generated_material=marker.get('generated_material'),e4_credit=0)
        save()
    unchanged=before==fingerprints()
    report.update(passed=all(row['passed'] for row in report['fixtures']) and len(report['fixtures'])==8 and unchanged and conflict_checked,
                  prepared_fixtures=len(report['fixtures']),distinct_projects=len(projects),distinct_work_identities=len(runtime_work_ids),
                  source_pack_unchanged=unchanged,author_conflict_preserved=conflict_checked,
                  finished_at=datetime.now().astimezone().isoformat(timespec='seconds'))
    save()
    print(json.dumps({k:report[k] for k in ['passed','prepared_fixtures','distinct_projects','distinct_work_identities','source_pack_unchanged','e4_credit_from_this_run']},ensure_ascii=False))
    return 0 if report['passed'] else 1


if __name__=='__main__':
    raise SystemExit(main())
