from pathlib import Path
import hashlib,json,time,sys
root=Path(__file__).resolve().parents[1]
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p): return json.loads((root/p).read_text())
status=load('.work-receipts/re-review-final-status.json')
ready=load('.work-receipts/re-review-final-ready.json')
review=load('.work-receipts/re-review-final-work-show.json')
deliver=load('.work-receipts/re-review-final-deliver-show.json')
session=load('.work-receipts/re-review-final-session-show.json')
complete=load('.work-receipts/re-review-work-complete.json')
ev2=load('.work-receipts/re-review-evidence-add-v2.json')
apply=load('.work-receipts/re-review-ledger-final-apply.json')
verification=load('.work-receipts/re-review-verification.json')
checks=[]
def add(name,passed,details): checks.append({'name':name,'passed':bool(passed),'details':details})
add('final project shape',status['project_revision']==178 and status['counts']=={'completed':3,'planned':1} and status['suggested_work']['external_key']=='QH-DELIVER',
    f"project_revision={status['project_revision']}, counts={status['counts']}, suggested={status['suggested_work']['external_key']}")
add('review completed and record finalized',review['work']['status']=='completed' and '整改均通过' in review['work']['summary'] and '由原执行者接续 QH-DELIVER' in review['work']['next_action'] and not review['work']['active_claims'],
    f"status={review['work']['status']}; summary={review['work']['summary']}; next_action={review['work']['next_action']}")
add('final delivery remains pending',deliver['work']['status']=='planned' and not deliver['work']['active_claims'] and not (root/'deliverables/qh-delivery.md').exists(),
    f"QH-DELIVER={deliver['work']['status']}; qh-delivery.md exists={(root/'deliverables/qh-delivery.md').exists()}")
claims=session['claims']
add('reviewer session closed',session['session']['status']=='ended' and session['session']['id']=='01M21Z2F3HZ8EHTRYY5N7RMRVZ' and len(claims)==1 and claims[0]['id']=='01M21Z2F3JS2KK0S8K77D6MBTT' and claims[0]['status']=='released',
    f"session={session['session']['status']}; claim={claims[0]['status'] if claims else 'missing'}")
ce=complete['event']['payload']['work_action']['completion']['evidence']
accepted=any(e['external_key']=='QH-INDEPENDENT-RE-REVIEW-20260909-V2' and e['sha256']==sha(root/'.work-receipts/re-review-verification.json') for e in ce)
add('completion bound accepted evidence',complete['ok'] and accepted and ev2['evidence']['external_key']=='QH-INDEPENDENT-RE-REVIEW-20260909-V2',
    f"work-complete project_revision={complete['project_revision']}; accepted_v2={accepted}")
add('report and evidence hashes stable',verification['report']['sha256']==sha(root/'deliverables/qh-independent-re-review.md') and ev2['evidence']['sha256']==sha(root/'.work-receipts/re-review-verification.json'),
    f"report={sha(root/'deliverables/qh-independent-re-review.md')}; verification={sha(root/'.work-receipts/re-review-verification.json')}")
add('final ledger proposal applied',apply['ok'] and apply['event']['payload']['source_revision']==19 and apply['event']['payload']['after_fingerprint'].removeprefix('sha256:')==sha(root/'work-ledger.yaml'),
    f"source_revision={apply['event']['payload']['source_revision']}; ledger={sha(root/'work-ledger.yaml')}")
add('no scenario completion claim',not (root/'deliverables/qh-delivery.md').exists() and status['counts']['planned']==1,
    'One planned work item remains and no final-delivery artifact exists.')
out={
 'version':1,
 'reviewer':{'agent_id':'luna_worker','session_id':'01M21Z2F3HZ8EHTRYY5N7RMRVZ','claim_id':'01M21Z2F3JS2KK0S8K77D6MBTT'},
 'verified_at':int(time.time()*1000),
 'checks':checks,
 'all_passed':all(c['passed'] for c in checks),
 'final_project_revision':status['project_revision'],
 'final_source_revision':apply['event']['payload']['source_revision'],
 'final_ledger_sha256':sha(root/'work-ledger.yaml'),
 'report_sha256':sha(root/'deliverables/qh-independent-re-review.md'),
 'completion_evidence_sha256':sha(root/'.work-receipts/re-review-verification.json'),
 'caveats':[
  'QH-DELIVER remains planned and no qh-delivery.md exists.',
  'QH-DELIVER source next_action still uses its pre-re-review waiting wording; its readiness and status are current, and the finalized QH-REVIEW next_action assigns continuation to the original executor.',
  'The first evidence binding was registered but rejected for completion because its report command differed; V2 is the accepted bound evidence.',
  'A proposal submit attempt with stale project revision 171 was rejected; the successful sequence used current revisions and is retained.'
 ]
}
(root/'.work-receipts/re-review-post-close-verification.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'all_passed':out['all_passed'],'checks':len(checks),'project_revision':out['final_project_revision'],'source_revision':out['final_source_revision']},ensure_ascii=False))
sys.exit(0 if out['all_passed'] else 1)
