from pathlib import Path
import hashlib, json, time, sys

root=Path(__file__).resolve().parents[1]
control=root.parent/'control'
snap=root/'review-inputs/corrections/review-correction'
report=root/'deliverables/qh-independent-re-review.md'
source_sha='cd44a161471cee94fd4f797955327a4fd995cf3a'
session='01M21Z2F3HZ8EHTRYY5N7RMRVZ'
claim='01M21Z2F3JS2KK0S8K77D6MBTT'
c1='独立核对来源、依赖和生效规则，逐条记录修改要求。'
c2='保留实际产物与来源引用，无法确认的内容显式说明。'

def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()

def load(rel):
    return json.loads((root/rel).read_text())

checks=[]

def check(name, passed, details, criteria):
    checks.append({'name':name,'passed':bool(passed),'details':details,'criteria':criteria})

pub=json.loads((control/'review-correction-publication.json').read_text())
missing=[]; mismatch=[]
for rel,want in pub['files_sha256'].items():
    p=snap/rel
    if not p.is_file(): missing.append(rel)
    elif sha(p)!=want: mismatch.append(rel)
check('fixed correction publication', not missing and not mismatch and len(pub['files_sha256'])==136,
      f"Checked {len(pub['files_sha256'])} published paths; missing={len(missing)}, hash_mismatch={len(mismatch)}; publication independent_verdict={pub['independent_verdict']}, e4_credit={pub['e4_credit']}.",[c1,c2])

expected={
 'deliverables/qh-independent-review.md':'cfd81dd8bbc85608a1d6210d9f482dd300b71af8687fb3efc3538b3a8ac19de1',
 'deliverables/qh-review-response.md':'92bc899572eab62027c30ec143b75fd9c9c8971569888493a14e17e31d43398d',
 'deliverables/qinghe-brief.md':'31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd',
 'deliverables/qinghe-dependencies.md':'42f98e5869ff2edc64b11a51a021b390e1586ea8ad7c1fcf09b1dc0c340385f5',
 'deliverables/qinghe-context-reference.md':'fde028a64afc2d1f8543aa28ee7c90375714aa583ea778b9de7d25e9d7f4124a',
 'RULES.md':'05c7d1e94f96e3f6bdaccfd8790895a07e11472fe95542c046eb570456ad218d',
 'review-inputs/reference-lookup-process-record.json':'0aa782e30d7ac1f2f2b9ba65f9c2224a07404d3111ea5353f1ff0f3015722920',
}
bad={p:{'expected':w,'actual':sha(root/p)} for p,w in expected.items() if sha(root/p)!=w}
check('authoritative and retained file hashes',not bad,f'Checked {len(expected)} current or preserved files; mismatches={bad}.',[c1,c2])

before_after={
 'brief_before':sha(root/'review-inputs/round-2/deliverables/qinghe-brief.md'),
 'brief_after':sha(snap/'deliverables/qinghe-brief.md'),
 'dependencies_before':sha(root/'review-inputs/round-2/deliverables/qinghe-dependencies.md'),
 'dependencies_after':sha(snap/'deliverables/qinghe-dependencies.md'),
 'context_before':sha(root/'review-inputs/round-2/deliverables/qinghe-context-reference.md'),
 'context_after':sha(snap/'deliverables/qinghe-context-reference.md'),
 'ledger_before':sha(root/'review-inputs/round-2/sources/work-ledger.yaml'),
 'ledger_after':sha(snap/'sources/work-ledger.yaml'),
}
expected_ba={
 'brief_before':'31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd',
 'brief_after':'31b843562c3918b531d774e1ab19b7b6912db627de6ce2dc4ba37515fa1cfccd',
 'dependencies_before':'b3bbb599f1ef5e3f15ffe1b2e149bb1edc5d218c470eab30186744167071161c',
 'dependencies_after':'42f98e5869ff2edc64b11a51a021b390e1586ea8ad7c1fcf09b1dc0c340385f5',
 'context_before':'8b9508d2965a6d97fd6b76c6afc5f3acad15f52ac326c9819633ad7da19571d3',
 'context_after':'fde028a64afc2d1f8543aa28ee7c90375714aa583ea778b9de7d25e9d7f4124a',
 'ledger_before':'5ca64a6962d2e318dab3c7bc97316eae71a4487248c0d674827b8c2e45ab603a',
 'ledger_after':'4db09cdb71fd2f9b6bf34ded6a9ea9e2fd5bf1567215881efa9c6a0ebe25985c',
}
check('before and after executor hashes',before_after==expected_ba,f'Observed before/after hashes: {before_after}.',[c1,c2])

start=load('.work-receipts/re-review-session-start.json')
claim_ok=(start['session']['id']==session and start['session']['agent_id']=='luna_worker' and start['claim']['id']==claim and start['claim']['status']=='active')
check('independent reviewer identity and claim',claim_ok,
      f"session={start['session']['id']} agent={start['session']['agent_id']} claim={start['claim']['id']} work=QH-REVIEW.",[c1])

inp=load('.work-receipts/re-review-work-qh-input.json')
brief=load('.work-receipts/re-review-work-qh-brief.json')
review=load('.work-receipts/re-review-work-qh-review.json')
deliver=load('.work-receipts/re-review-work-qh-deliver.json')
status_ok=(inp['work']['status']=='completed' and brief['work']['status']=='completed' and review['work']['status']=='in_progress' and deliver['work']['status']=='planned')
check('current AWR work arrangement',status_ok,
      f"QH-INPUT={inp['work']['status']}; QH-BRIEF={brief['work']['status']}; QH-REVIEW={review['work']['status']}; QH-DELIVER={deliver['work']['status']}.",[c1,c2])

def evidence_shape(doc):
    explicit=[e for e in doc['evidence'] if e['level']=='locally_verified' and e['currency']=='current' and not e['missing_bindings']]
    unknown=[e for e in doc['evidence'] if e['level']=='unknown' and e['currency']=='unknown' and set(e['missing_bindings'])=={'sha256','source_sha','command','verified_at'}]
    return len(explicit),len(unknown)
isx,inu=evidence_shape(inp); bsx,bnu=evidence_shape(brief)
check('explicit versus locator-only evidence projection',isx>=1 and inu>=1 and bsx>=2 and bnu>=2,
      f'QH-INPUT explicit={isx}, locator_unknown={inu}; QH-BRIEF explicit={bsx}, locator_unknown={bnu}. Unknown entries remain unpromoted.',[c1,c2])

text=report.read_text()
need=[
 'QH-R01','QH-R02','QH-R03','QH-R04','QH-R05',
 '安排级交接范围内通过返检','目录级门槛仍未满足',
 'locator-only Unknown','初始范围偏离仍是不合规过程',
 '不授予 E4','最终交付责任回到原执行者',
 session,claim,
]
absent=[x for x in need if x not in text]
check('re-review report decisions and boundaries',not absent,f'Required report assertions absent={absent}.',[c1,c2])

check('no final delivery created',not (root/'deliverables/qh-delivery.md').exists(),
      'deliverables/qh-delivery.md does not exist; re-review does not perform final delivery.',[c2])

out={
 'version':1,
 'work_item':'QH-REVIEW',
 'report_kind':'independent_remediation_re_review',
 'reviewer':{'agent_id':'luna_worker','session_id':session,'claim_id':claim},
 'executor':{'native_thread_id':pub['actual_client_task_id']},
 'source_sha':source_sha,
 'command':'rtk proxy python3 .work-receipts/re-review-verify.py',
 'scope':['QH-REVIEW'],
 'verified_at':int(time.time()*1000),
 'report':{'locator':'deliverables/qh-independent-re-review.md','sha256':sha(report)},
 'reviewed_files':{'locator':'.work-receipts/re-review-reviewed-files.json','sha256':sha(root/'.work-receipts/re-review-reviewed-files.json')},
 'checks':checks,
 'all_passed':all(x['passed'] for x in checks),
 'verdict':'remediation_accepted_for_arrangement_level_handoff' if all(x['passed'] for x in checks) else 're_review_failed',
 'limits':[
  'No verification of actual document bodies, titles, links, access status or responsible contacts.',
  'No verification of directory contents or personal-contact removal.',
  'No confirmation of domain, legacy-page stop date, switch time, external readiness, portal launch or final delivery.',
  'Locator-only Unknown evidence remains a visible gap.',
  'The initial reference lookup remains a noncompliant process event with no observed future-business-result contamination.'
 ],
 'missing_snapshot_paths':missing,
 'snapshot_hash_mismatches':mismatch
}
(root/'.work-receipts/re-review-verification.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'all_passed':out['all_passed'],'checks':len(checks),'report_sha256':out['report']['sha256']},ensure_ascii=False))
sys.exit(0 if out['all_passed'] else 1)
