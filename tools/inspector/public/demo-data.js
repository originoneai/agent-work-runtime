/**
 * 演示数据
 *
 * 桥接连不上、或者项目还没初始化时，用这份数据。
 *
 * 它的结构照着仓库源码里真实的 JSON 形状写：
 *   status  → crates/awr-runtime/src/status_action.rs（四个顶层队列数组 + omissions）
 *   条目    → crates/awr-runtime/src/status_summary.rs 的 brief()
 *   work    → crates/awr-cli/src/query.rs（work 在 `work` 下，acceptance 是字符串数组）
 *   context → crates/awr-context/src/{compile,budget}.rs（work_context.selected_chunks）
 *
 * 这样 app.js 里那套字段映射在演示模式下也在跑——演示模式同时是映射的测试夹具。
 * 工作项和缺口都是编的；token 数取自仓库公开 benchmark。
 */

(function () {
  'use strict';

  const details = (key) => ({ cli: ['work', 'show', key], mcp: 'awr_work_get' });

  const item = (o) => Object.assign({
    key: o.key,
    title: o.title,
    status: o.status,
    raw_status: o.raw_status || o.status,
    owner: o.owner || null,
    next_action: o.next_action || null,
    blocker: o.blocker || null,
    revision: 128,
    source_revision: o.source_revision || 41,
    details: details(o.key),
  }, o.extra || {});

  const CURRENT = [
    item({
      key: 'EXAMPLE-001', title: '确定性的上下文编译', status: 'in_progress', owner: 'lin',
      next_action: '把渲染器第三遍过完，然后催一下 OL-14 的 review',
      extra: {
        ownership_required: true,
        codes: [],
        claims: [{ id: 'clm_01', session_id: 'sess_8f21', agent_id: 'coding-agent', expires_at: iso(-45) }],
      },
    }),
  ];

  const READY = [
    item({
      key: 'EXAMPLE-005', title: 'Windows 路径规范化', status: 'planned',
      next_action: '先在 windows-latest 上把 flaky 复现出来',
      extra: { codes: [] },
    }),
    item({
      key: 'EXAMPLE-006', title: '检查点压缩策略', status: 'planned',
      next_action: '设计已定，等 EXAMPLE-002',
      extra: { codes: [] },
    }),
  ];

  const WAITING = [
    item({
      key: 'EXAMPLE-004', title: '对已有仓库做 intake inspect', status: 'in_review', owner: 'mei',
      next_action: 'Collect and record the actual user reply before resuming.',
      extra: { wait_ids: [9021], wait_total: 1, execution_total: 0, executions: [], codes: [] },
    }),
    item({
      key: 'EXAMPLE-002', title: '源漂移时重建 SQLite 投影', status: 'planned',
      next_action: 'Continue the unfinished prerequisite; recheck its source state.',
      extra: { wait_total: 0, codes: ['dependency_not_completed'] },
    }),
  ];

  const BLOCKED = [
    item({
      key: 'EXAMPLE-003', title: 'context compile 的 MCP 工具面', status: 'blocked',
      blocker: 'OL-09：从 symlink 路径启动时，handshake 返回的 project root 是陈旧的',
      next_action: '先查清楚 symlink 路径下 handshake 拿到的 root 为什么是旧的',
      extra: { codes: ['source_blocked'], structural_codes: [] },
    }),
    item({
      key: 'EXAMPLE-008', title: '砍掉 YAML front-matter 回退路径', status: 'blocked',
      blocker: 'OL-19：决策本身还没做，没有决策记录之前没有可实现的东西',
      next_action: '先把决策记录写出来',
      extra: { codes: ['missing_decision_record'], structural_codes: ['goal_not_linked'] },
    }),
  ];

  // work show 的完整响应（键是 work 的 external key）
  const DETAIL = {
    'EXAMPLE-001': {
      acceptance: [
        'Packet fits the declared budget without truncating open loops',
        'Rendered facts round-trip against the source of record',
        'Compile p95 stays under 150 ms on a single host',
      ],
      required_dependencies: [],
      missing_dependencies: [],
      milestone: 'goal#demo',
    },
    'EXAMPLE-002': {
      acceptance: [
        'Drift is detected from mtime plus content hash',
        'A rebuild touches only rows derived from changed files',
      ],
      required_dependencies: [{ external_key: 'EXAMPLE-001', status: 'in_progress', revision: 128 }],
      missing_dependencies: [],
      milestone: 'goal#demo',
    },
    'EXAMPLE-003': {
      acceptance: [
        'Tool schemas match the CLI --json output',
        'Handshake resolves the project root absolutely',
      ],
      required_dependencies: [{ external_key: 'EXAMPLE-001', status: 'in_progress', revision: 128 }],
      missing_dependencies: [],
      milestone: 'goal#demo',
    },
    'EXAMPLE-004': {
      acceptance: [
        'Dry run writes nothing to .local',
        'Reports matched, skipped and ambiguous files separately',
        'Exit code distinguishes empty match from failure',
      ],
      required_dependencies: [],
      missing_dependencies: [],
      milestone: 'goal#demo',
    },
    'EXAMPLE-005': {
      acceptance: [
        'UNC and drive-letter paths normalise to the same key',
        'intake inspect passes on windows-latest CI',
      ],
      required_dependencies: [],
      missing_dependencies: [],
      milestone: 'goal#demo',
    },
    'EXAMPLE-006': {
      acceptance: [
        'Compaction preserves every open loop and its age',
        'A compacted history replays to the same projection',
      ],
      required_dependencies: [{ external_key: 'EXAMPLE-002', status: 'planned', revision: 128 }],
      missing_dependencies: [],
      milestone: 'goal#demo',
    },
    'EXAMPLE-008': {
      acceptance: [
        'A decision record exists and is indexed',
        'Migration note covers projects relying on the fallback',
      ],
      required_dependencies: [{ external_key: 'EXAMPLE-004', status: 'in_review', revision: 128 }],
      missing_dependencies: ['EXAMPLE-011'],
      milestone: 'goal#demo',
    },
  };

  // 证据与决策：只有少数工作项有，正好演示「有」和「没有」两种样子。
  const DETAIL_EXTRA = {
    'EXAMPLE-001': {
      evidence: [
        { external_key: 'bench-p95-0917', evidence_type: 'benchmark', level: 'locally_verified' },
      ],
      decisions: [{ external_key: 'dec-0003', title: '渲染器按段落切块，不按文件切' }],
    },
    'EXAMPLE-004': {
      evidence: [
        { external_key: 'dry-run-0915', evidence_type: 'command', level: 'locally_verified' },
        { external_key: 'inspect-report-02', evidence_type: 'report', level: 'locally_verified' },
      ],
      decisions: [],
    },
  };

  const ALL = CURRENT.concat(READY, WAITING, BLOCKED);

  window.AWR_DEMO = {
    health: { mode: 'demo', project: '.local/demo', awrVersion: null, bridgeVersion: '1.0.0' },

    status: {
      view: 'action',
      schema_version: 1,
      project: 'demo',
      project_id: 1,
      project_revision: 128,
      total: 7,
      project_work_total: 8,
      current_total: 1,
      ready_count: 4,
      waiting_count: 2,
      blocked_count: 2,
      current: CURRENT,
      ready: READY,
      waiting: WAITING,
      blocked: BLOCKED,
      omissions: { current: 0, ready: 2, waiting: 0, blocked: 0 },
      freshness_basis: 'source_refresh',
      guidance: {
        when: 'Current work can be continued',
        next_action: 'Prepare the selected work; continue only with its owned session or explicitly resume it',
      },
      pending_operations: {
        basis: 'project-wide registered runtime findings',
        total: 2,
        items: [
          { code: 'execution_outcome_unknown', kind: 'execution', id: 'exe_4411' },
          { code: 'mutation_proposal_pending', kind: 'proposal', id: 'prp_9002' },
        ],
      },
      organization: {
        state: 'partially_structured',
        business_execution_ready: false,
        gap_total: 3,
        project_gap_total: 3,
        gaps: [
          { code: 'goal_not_linked', target: 'EXAMPLE-008', detail: '这个工作项没有关联到任何目标' },
          { code: 'acceptance_missing', target: 'EXAMPLE-011', detail: '源文件里没有写验收标准' },
          { code: 'plan_not_found', target: 'project', detail: '项目没有可索引的计划文件' },
        ],
      },
      // 概览那张对比图用的数字（公开 benchmark，不是本项目实测）
      context_sample: {
        full_corpus_tokens: 18955,
        json_dump_tokens: 12748,
        compiled_tokens: 4998,
        note: '公开 benchmark 值（39 个活跃任务，o200k_base 计数），不是本项目实测。',
      },
    },

    /** work show 的响应 */
    workShow(key) {
      const brief = ALL.find((w) => w.key === key) || ALL[0];
      const extra = DETAIL[brief.key] || { acceptance: [], required_dependencies: [], missing_dependencies: [] };
      return Object.assign({
        ok: true,
        project_revision: 128,
        freshness_basis: 'source_refresh',
        read_only: false,
        work: Object.assign({}, brief, {
          ready: brief.status === 'planned',
          diagnostics: brief.codes || [],
          active_claims: brief.claims || [],
        }),
        evidence: DETAIL_EXTRA[brief.key] ? DETAIL_EXTRA[brief.key].evidence : [],
        decisions: DETAIL_EXTRA[brief.key] ? DETAIL_EXTRA[brief.key].decisions : [],
        dependency_cycles: [],
      }, extra);
    },

    sources: {
      project_revision: 128,
      summary: { markdown: 7, yaml: 2, manifest: 1 },
      files: [
        { path: 'docs/architecture.md', kind: 'markdown', items: 12, state: 'indexed', indexed_at: iso(6) },
        { path: 'docs/work/EXAMPLE-001.md', kind: 'markdown', items: 9, state: 'drift', indexed_at: iso(4320) },
        { path: 'docs/work/EXAMPLE-002.md', kind: 'markdown', items: 6, state: 'indexed', indexed_at: iso(6) },
        { path: 'docs/work/EXAMPLE-004.md', kind: 'markdown', items: 7, state: 'indexed', indexed_at: iso(6) },
        { path: 'docs/work/EXAMPLE-008.md', kind: 'markdown', items: 3, state: 'new', indexed_at: null },
        { path: 'docs/bench/README.md', kind: 'markdown', items: 5, state: 'indexed', indexed_at: iso(4320) },
        {
          path: 'docs/scratch-config.md', kind: 'markdown', items: 0, state: 'rejected', indexed_at: null,
          rejection: {
            rule: 'secret-boundaries/assigned-credential',
            location: { line: 42, column: 3 },
            repair: '把赋了值的凭据挪到环境变量，或改成不带值的类型声明。',
          },
        },
        { path: 'agents/handoff.yaml', kind: 'yaml', items: 4, state: 'indexed', indexed_at: iso(1440) },
        { path: 'agents/roles.yaml', kind: 'yaml', items: 2, state: 'indexed', indexed_at: iso(1440) },
        { path: 'project.toml', kind: 'manifest', items: null, state: 'indexed', indexed_at: iso(6) },
      ],
    },

    /** context compile 的响应；预算小于 5,000 时会省略非必需的块。 */
    compile(key, budget) {
      const brief = ALL.find((w) => w.key === key) || ALL[0];
      const extra = DETAIL[brief.key] || { acceptance: [], required_dependencies: [] };
      const tight = budget < 5000;

      const chunks = [
        { key: 'rules/project', section: 'rules', required: true },
        { key: 'rules/adapters', section: 'rules', required: true },
        { key: 'goal/demo', section: 'goal', required: true },
        { key: 'work/' + brief.key, section: 'work', required: true },
        { key: 'acceptance/' + brief.key, section: 'acceptance', required: true },
        { key: 'dependencies/' + brief.key, section: 'dependencies', required: true },
        { key: 'loops/open', section: 'loops', required: false },
        { key: 'source/architecture#1', section: 'source', required: false },
        { key: 'source/architecture#2', section: 'source', required: false },
        { key: 'source/architecture#3', section: 'source', required: false },
      ];
      const selected = tight ? chunks.filter((c) => c.required || c.section === 'loops') : chunks;
      const omitted = chunks
        .filter((c) => selected.indexOf(c) < 0)
        .map((c) => ({ key: c.key, section: c.section, reason: 'insufficient_budget_for_whole_chunk' }));

      const lines = [
        '# work: ' + brief.key,
        '# goal: ' + (extra.milestone || 'goal#demo'),
        '# revision: 128',
        '',
        '## 必须遵守的规则',
        '- Markdown / YAML 是唯一的记录来源；SQLite 只是投影。',
        '- 任何写操作都要带上你读到的 revision。',
        '',
        '## 验收标准（逐字取自源文件）',
      ].concat(
        extra.acceptance.map((a) => '- ' + a),
        [
          '',
          '## 依赖',
          extra.required_dependencies.length
            ? extra.required_dependencies.map((d) => `- ${d.external_key}（${d.status}）`).join('\n')
            : '- 无',
          '',
          '## 阻塞',
          brief.blocker ? '- ' + brief.blocker : '- 无',
          '',
          '## 下一步',
          '- ' + (brief.next_action || '（源文件里没写）'),
          '',
          omitted.length
            ? `（因预算省略了 ${omitted.length} 块非必需内容，详见完整性面板）`
            : '（全部选中的块均已装入）',
        ]
      );

      return {
        ok: omitted.length === 0,
        level: 'L1',
        project_revision: 128,
        completeness: {
          complete: omitted.length === 0,
          status: omitted.length === 0 ? 'CONTEXT COMPLETE' : 'CONTEXT INCOMPLETE',
          project_revision: 128,
          rules_complete: true,
          goal_context_complete: true,
          work_state_complete: true,
          acceptance_complete: true,
          dependencies_complete: (extra.required_dependencies || []).length === 0,
          source_fresh: !tight,
          issues: [],
          evidence_gaps: (DETAIL_EXTRA[brief.key] && DETAIL_EXTRA[brief.key].evidence.length)
            ? []
            : [{ code: 'no_evidence', reason: 'no evidence is associated with this work', reference: brief.key }],
          unresolved_required_dependencies: (extra.required_dependencies || [])
            .filter((d) => d.status !== 'done')
            .map((d) => ({ external_key: d.external_key })),
        },
        omitted_refs: [],
        work_context: {
          rendered_context: lines.join('\n'),
          context_hash: '9f3c1ad2e7b04c58',
          token_estimate: tight ? 3311 : 4998,
          required_tokens: 3311,
          token_budget: budget,
          tokenizer: 'o200k_base',
          selected_chunks: selected,
          omitted_chunks: omitted,
        },
      };
    },
  };

  function iso(minutesAgo) {
    return new Date(Date.now() - minutesAgo * 60000).toISOString();
  }
})();
