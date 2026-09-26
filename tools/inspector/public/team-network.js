/**
 * The AWR website collaboration network, adapted to authorized Inspector data.
 * Keeps the website's project/card/lane markup and measured-node SVG layout.
 * No synthetic people, delivery pipeline, execution state or completion ratio.
 */
(function (root) {
  'use strict';

  function model(works, streams, unassigned) {
    const lanes = (streams || []).map(s => ({ id: s.id, name: s.title || s.external_key || s.id,
      tag: s.external_key || '', works: [] }));
    const byLane = new Map(lanes.map(l => [l.id, l]));
    for (const work of works) {
      const id = work.workstream_id || '';
      if (!byLane.has(id)) {
        const lane = { id, name: id || unassigned, tag: '', works: [] };
        lanes.push(lane); byLane.set(id, lane);
      }
      byLane.get(id).works.push(work);
    }
    const byKey = new Map(works.map(w => [w.key, w]));
    const edges = [], seen = new Set();
    for (const work of works) for (const dep of work.depends_on || []) {
      // Visibility is explicit. Never infer edges from card order or hidden refs.
      if (dep.visible !== true || !byKey.has(dep.key)) continue;
      const key = JSON.stringify([dep.key, work.key]);
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push([dep.key, work.key, byKey.get(dep.key).workstream_id !== work.workstream_id ? 'cross' : 'local']);
    }
    // Retain source order among peers, but place visible prerequisites first.
    for (const lane of lanes) {
      const remaining = new Map(lane.works.map(w => [w.key, w]));
      const ordered = [];
      while (remaining.size) {
        const ready = [...remaining.values()].filter(w => !(w.depends_on || [])
          .some(d => d.visible === true && remaining.has(d.key)));
        if (!ready.length) { ordered.push(...remaining.values()); break; }
        for (const work of ready) { ordered.push(work); remaining.delete(work.key); }
      }
      lane.works = ordered;
    }
    return { lanes, edges };
  }

  function visualStatus(work) {
    if (work.attention) return 'blocked';
    return ({ completed: 'done', accepted: 'done', in_progress: 'developing',
      running: 'developing', claimed: 'claimed', review: 'review', in_review: 'review',
      blocked: 'blocked', waiting: 'waiting' })[work.status] || 'unknown';
  }

  function render(host, net, options) {
    const { el, text, card, project } = options;
    const frame = el('div', { class: 'scene-frame', tabindex: '0', 'aria-label': text('graph_pan') });
    const scene = el('div', { class: 'scene' });
    const grid = el('div', { class: 'flow-grid' });
    grid.style.minWidth = Math.max(440, net.lanes.length * 260) + 'px';
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'flow-wires'); svg.setAttribute('aria-hidden', 'true');
    grid.appendChild(svg);
    const summary = el('div', { class: 'project-card' });
    summary.appendChild(el('span', { class: 'project-badge', 'aria-hidden': 'true' }, 'A'));
    const info = el('div', { class: 'project-info' });
    const name = el('div', { class: 'project-name-row' });
    name.appendChild(el('strong', null, project.title || project.key));
    info.appendChild(name);
    info.appendChild(el('small', { class: 'project-url' }, project.key));
    info.appendChild(el('small', { class: 'project-goal' }, text('visible_scope')));
    summary.appendChild(info);
    const stats = el('div', { class: 'project-stats' });
    for (const [value, label] of [[options.count, 'tasks'], [net.lanes.length, 'streams'], [net.edges.length, 'edges']]) {
      const stat = el('span'); stat.appendChild(el('b', null, value));
      stat.appendChild(el('small', null, text(label))); stats.appendChild(stat);
    }
    summary.appendChild(stats); grid.appendChild(summary);
    net.lanes.forEach((lane, index) => {
      const head = el('div', { class: 'lane-head', dataset: { lane: index } });
      const dot = el('i', { 'aria-hidden': 'true' });
      dot.style.background = ['#3b82f6', '#8b5cf6', '#22a06b', '#f59e0b'][index % 4];
      head.appendChild(dot);
      const caption = el('span'); caption.appendChild(el('strong', null, lane.name));
      caption.appendChild(el('small', null, text('lane_count', { count: lane.works.length })));
      head.appendChild(caption); grid.appendChild(head);
      for (const work of lane.works) grid.appendChild(card(work, lane));
    });
    scene.appendChild(grid); frame.appendChild(scene); host.appendChild(frame);
    const legend = el('div', { class: 'flow-legend' });
    for (const [className, label] of [['lg-line lg-rail', 'membership'], ['lg-line', 'dependency'], ['lg-line lg-line-cross', 'cross']]) {
      const item = el('span', { class: 'lg-item' });
      item.appendChild(el('i', { class: className, 'aria-hidden': 'true' }));
      item.appendChild(document.createTextNode(text(label))); legend.appendChild(item);
    }
    host.appendChild(legend);
  }

  // Website layoutGraph: the three fixed lanes become any number of real streams.
  // The synthetic delivery pipeline is intentionally absent from the live graph.
  function layout(host, net) {
    const grid = host && host.querySelector('.flow-grid');
    if (!grid || !grid.clientWidth || !net.lanes.length) return;
    const width = grid.clientWidth;
    const nodes = [...grid.querySelectorAll('.task-node')];
    const heads = [...grid.querySelectorAll('.lane-head')];
    const project = grid.querySelector('.project-card');
    const svg = grid.querySelector('.flow-wires');
    const padX = Math.min(26, width * .02);
    const laneX = i => padX + (i + .5) * (width - 2 * padX) / net.lanes.length;
    project.style.left = width / 2 + 'px'; project.style.top = '18px';
    const headY = 18 + project.offsetHeight + 30;
    heads.forEach((h, i) => { h.style.left = laneX(i) + 'px'; h.style.top = headY + 'px'; });
    const topY = headY + Math.max(...heads.map(h => h.offsetHeight)) + 28;
    const slotH = Math.max(126, ...nodes.map(n => n.offsetHeight + 42));
    const rows = Math.max(1, ...net.lanes.map(l => l.works.length));
    const height = topY + rows * slotH + 12;
    grid.style.height = height + 'px';
    const pos = new Map();
    const elements = new Map(nodes.map(n => [n.dataset.key, n]));
    net.lanes.forEach((lane, i) => lane.works.forEach((w, row) => {
      const node = elements.get(w.key), x = laneX(i), y = topY + row * slotH;
      node.style.left = x + 'px'; node.style.top = y + 'px';
      pos.set(w.key, { x, y, h: node.offsetHeight, w: node.offsetWidth });
    }));
    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    const parts = ['<defs><marker id="team-flow-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M1 1L9 5L1 9" fill="none" stroke="#6e98d9"/></marker><marker id="team-cross-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M1 1L9 5L1 9" fill="none" stroke="#e14b54"/></marker></defs>'];
    const py = 18 + project.offsetHeight;
    heads.forEach((h, i) => {
      const x = laneX(i), y = headY - 5, mid = (py + y) / 2;
      parts.push(`<line class="lane-rail" x1="${x}" y1="${headY + h.offsetHeight}" x2="${x}" y2="${height - 18}"/>`);
      parts.push(`<path class="beam-base proj-link" d="M${width / 2},${py} C${width / 2},${mid} ${x},${mid} ${x},${y}"/>`);
    });
    for (const [from, to, kind] of net.edges) {
      const a = pos.get(from), b = pos.get(to), cross = kind === 'cross';
      const x1 = a.x, y1 = a.y + a.h, x2 = b.x, y2 = b.y - 6;
      let d;
      if (!cross && y2 > y1 && y2 - y1 < slotH) d = `M${x1},${y1} L${x2},${y2}`;
      else if (!cross) {
        // Non-adjacent or backward dependencies route around intervening cards.
        const side = a.x + a.w / 2 + 15;
        d = `M${a.x + a.w / 2},${a.y + a.h / 2} C${side},${a.y + a.h / 2} ${side},${b.y + b.h / 2} ${b.x + b.w / 2},${b.y + b.h / 2}`;
      } else {
        const mid = (y1 + y2) / 2;
        d = `M${x1},${y1} C${x1},${mid + 14} ${x2},${mid - 14} ${x2},${y2}`;
      }
      parts.push(`<path d="${d}" marker-end="url(#team-${cross ? 'cross' : 'flow'}-arrow)" class="beam-base${cross ? ' beam-cross' : ''}"/>`);
    }
    // Only numeric measured geometry enters SVG markup; project text uses textContent.
    svg.innerHTML = parts.join('');
  }

  const api = { model, visualStatus, render, layout };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  root.AWR_TEAM_NETWORK = api;
})(typeof window !== 'undefined' ? window : globalThis);
