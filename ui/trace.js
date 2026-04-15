/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

// Guixu Agent Trace Viewer

const API = window.location.origin;

class TraceViewer {
  constructor(root) {
    this.root = root;
    this.state = {
      traces: [],
      selectedTraceId: null,
      source: 'guixu',
      limit: 50,
      spans: [],
      scores: [],
      activeTab: 'waterfall',
      selectedSpanId: null,
      collapsed: new Set(),
      loading: { list: false, detail: false },
      error: { list: null, detail: null },
    };
    this.els = {
      list: root.querySelector('#traceList'),
      count: root.querySelector('#traceCount'),
      detail: root.querySelector('#traceDetail'),
      sourceFilter: root.querySelector('#sourceFilter'),
      limitInput: root.querySelector('#limitInput'),
      spanDetail: null,
    };
    this.initTheme();
    this.initEventDelegation();
    this.initRouter();
    this.loadTraces();
  }

  // -- Theme ----------------------------------------------------------
  initTheme() {
    const saved = localStorage.getItem('guixu-theme') || 'auto';
    this.applyTheme(saved);
  }

  applyTheme(theme) {
    const html = document.documentElement;
    if (theme === 'auto') {
      html.removeAttribute('data-theme');
    } else {
      html.dataset.theme = theme;
    }
    localStorage.setItem('guixu-theme', theme);
    const btn = this.root.querySelector('#themeToggle');
    if (btn) btn.textContent = theme === 'dark' ? '☀' : theme === 'light' ? '☾' : '◑';
  }

  cycleTheme() {
    const cur = localStorage.getItem('guixu-theme') || 'auto';
    const next = cur === 'auto' ? 'dark' : cur === 'dark' ? 'light' : 'auto';
    this.applyTheme(next);
  }

  // -- Event delegation -----------------------------------------------
  initEventDelegation() {
    this.root.addEventListener('click', (e) => {
      const t = e.target;

      // Theme toggle
      if (t.closest('#themeToggle')) return this.cycleTheme();

      // Panel collapse
      if (t.closest('[data-action="toggle-panel"]')) return this.togglePanel();

      // Refresh
      if (t.closest('#btnRefresh')) return this.loadTraces();

      // Trace item
      const traceItem = t.closest('[data-trace-id]');
      if (traceItem) return this.selectTrace(traceItem.dataset.traceId);

      // Tab
      const tab = t.closest('[data-tab]');
      if (tab && tab.classList.contains('trace-tab')) return this.switchTab(tab.dataset.tab);

      // Waterfall collapse toggle
      const toggle = t.closest('[data-toggle-span]');
      if (toggle) { e.stopPropagation(); return this.toggleCollapse(toggle.dataset.toggleSpan); }

      // Waterfall row
      const wfRow = t.closest('[data-span-id]');
      if (wfRow) return this.selectSpan(wfRow.dataset.spanId);

      // Retry
      const retry = t.closest('[data-retry]');
      if (retry) {
        const fn = retry.dataset.retry;
        if (fn === 'loadTraces') this.loadTraces();
        if (fn === 'loadDetail') this.selectTrace(this.state.selectedTraceId);
        if (fn === 'loadMemory') this.loadMemoryTimeline();
      }

      // Memory timeline query
      if (t.closest('#btnMemQuery')) return this.loadMemoryTimeline();
    });

    // Source filter change
    this.els.sourceFilter?.addEventListener('change', () => {
      this.state.source = this.els.sourceFilter.value;
      this.loadTraces();
    });

    // Limit change
    this.els.limitInput?.addEventListener('change', () => {
      this.state.limit = parseInt(this.els.limitInput.value) || 50;
    });

    // Waterfall zoom (ctrl+wheel)
    this.root.addEventListener('wheel', (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      const container = e.target.closest('.wf-bar-container');
      if (!container) return;
      e.preventDefault();
      const cur = parseFloat(container.dataset.scale || '1');
      const next = Math.max(1, Math.min(10, cur + (e.deltaY < 0 ? 0.3 : -0.3)));
      container.dataset.scale = next;
      container.style.transform = `scaleX(${next})`;
      container.style.transformOrigin = `${e.offsetX}px 0`;
    }, { passive: false });
  }

  // -- Hash routing ---------------------------------------------------
  initRouter() {
    window.addEventListener('hashchange', () => this.handleRoute());
    // Defer initial route to after trace list loads
    this._pendingRoute = true;
  }

  handleRoute() {
    const hash = location.hash.slice(1);
    const m = hash.match(/^trace\/(.+)/);
    if (m) {
      const id = decodeURIComponent(m[1]);
      if (id !== this.state.selectedTraceId) this.selectTrace(id);
    }
  }

  // -- Panel collapse -------------------------------------------------
  togglePanel() {
    this.root.querySelector('.trace-list-panel')?.classList.toggle('collapsed');
  }

  // -- Load traces ----------------------------------------------------
  async loadTraces() {
    this.state.loading.list = true;
    this.state.error.list = null;
    this.renderTraceList();

    try {
      const res = await fetch(`${API}/api/traces?source=${this.state.source}&limit=${this.state.limit}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      this.state.traces = await res.json();
      this.state.loading.list = false;
      this.renderTraceList();

      // Handle pending route after first load
      if (this._pendingRoute) {
        this._pendingRoute = false;
        this.handleRoute();
      }
    } catch (e) {
      this.state.loading.list = false;
      this.state.error.list = e.message;
      this.renderTraceList();
    }
  }

  // -- Select trace ---------------------------------------------------
  async selectTrace(traceId) {
    this.state.selectedTraceId = traceId;
    this.state.selectedSpanId = null;
    this.state.activeTab = 'waterfall';
    history.replaceState(null, '', `#trace/${encodeURIComponent(traceId)}`);
    this.highlightTraceItem();

    this.state.loading.detail = true;
    this.state.error.detail = null;
    this.renderDetailLoading();

    const trace = this.state.traces.find(t => t.trace_id === traceId);
    const source = trace?.source || this.state.source;

    try {
      const [spansRes, scoresRes] = await Promise.all([
        fetch(`${API}/api/traces/${encodeURIComponent(traceId)}/spans?source=${source}`),
        fetch(`${API}/api/traces/${encodeURIComponent(traceId)}/scores`),
      ]);
      if (!spansRes.ok) throw new Error(`Spans: HTTP ${spansRes.status}`);
      if (!scoresRes.ok) throw new Error(`Scores: HTTP ${scoresRes.status}`);
      this.state.spans = await spansRes.json();
      this.state.scores = await scoresRes.json();
      this.state.loading.detail = false;
      this.state.collapsed.clear();
      this.renderDetail(trace);
    } catch (e) {
      this.state.loading.detail = false;
      this.state.error.detail = e.message;
      this.els.detail.innerHTML = renderError(e.message, 'loadDetail');
    }
  }

  // -- Tab switching (no DOM rebuild) ---------------------------------
  switchTab(tabName) {
    this.state.activeTab = tabName;
    this.els.detail.querySelectorAll('.trace-tab').forEach(t =>
      t.classList.toggle('active', t.dataset.tab === tabName));
    this.els.detail.querySelectorAll('.tab-content').forEach(c =>
      c.classList.toggle('active', c.id === 'tab-' + tabName));
  }

  // -- Span selection (no DOM rebuild) --------------------------------
  selectSpan(spanId) {
    this.state.selectedSpanId = spanId;
    this.els.detail.querySelectorAll('.wf-row').forEach(r =>
      r.classList.toggle('active', r.dataset.spanId === spanId));
    const span = this.state.spans.find(s => s.span_id === spanId);
    if (!span) return;
    const el = this.els.detail.querySelector('#spanDetail');
    if (el) el.innerHTML = renderSpanDetail(span);
  }

  // -- Waterfall collapse (minimal re-render) -------------------------
  toggleCollapse(spanId) {
    if (this.state.collapsed.has(spanId)) {
      this.state.collapsed.delete(spanId);
    } else {
      this.state.collapsed.add(spanId);
    }
    // Re-render only the waterfall tab content
    const wfEl = this.els.detail.querySelector('#tab-waterfall');
    if (wfEl) wfEl.innerHTML = this.renderWaterfall(this.state.spans);
  }

  // -- Memory timeline ------------------------------------------------
  async loadMemoryTimeline() {
    const input = this.els.detail.querySelector('#memKeyInput');
    const key = input?.value?.trim();
    if (!key) return;
    const el = this.els.detail.querySelector('#memTimeline');
    if (!el) return;
    el.innerHTML = renderSkeleton(3);

    try {
      const res = await fetch(`${API}/api/memory/timeline?memory_key=${encodeURIComponent(key)}&limit=50`);
      const data = await res.json();
      if (data.error) { el.innerHTML = renderError(data.error, 'loadMemory'); return; }
      if (!data.length) { el.innerHTML = renderEmpty('No mutations found for this key'); return; }
      el.innerHTML = `<div class="mem-count">${data.length} mutations for <code>${esc(key)}</code></div>` + renderMemoryEvents(data);
    } catch (e) {
      el.innerHTML = renderError(e.message, 'loadMemory');
    }
  }

  // -- Highlight active trace (no full re-render) ---------------------
  highlightTraceItem() {
    this.els.list.querySelectorAll('.trace-item').forEach(el =>
      el.classList.toggle('active', el.dataset.traceId === this.state.selectedTraceId));
  }

  // ==================================================================
  // RENDERERS
  // ==================================================================

  renderTraceList() {
    const { traces, loading, error, selectedTraceId } = this.state;
    this.els.count.textContent = loading.list ? '…' : traces.length;

    if (loading.list) {
      this.els.list.innerHTML = renderSkeleton(8);
      return;
    }
    if (error.list) {
      this.els.list.innerHTML = renderError(error.list, 'loadTraces');
      return;
    }
    if (!traces.length) {
      this.els.list.innerHTML = renderEmpty('No traces found');
      return;
    }

    this.els.list.innerHTML = traces.map(t => {
      const src = t.source || 'guixu';
      const time = fmtTime(t.last_span_time || t.first_span_time);
      const dur = fmtDuration(t.total_duration_ms);
      const tokens = (t.total_input_tokens || 0) + (t.total_output_tokens || 0);
      const active = t.trace_id === selectedTraceId ? ' active' : '';
      return `<div class="trace-item${active}" data-trace-id="${esc(t.trace_id)}" role="button" tabindex="0" aria-label="Trace ${esc(t.trace_name || t.trace_id.slice(0,12))}">
        <div class="trace-item-name">${esc(t.trace_name || t.trace_id.slice(0, 20))}</div>
        <div class="trace-item-meta">
          <span class="tag tag-${src}">${src}</span>
          <span>${t.span_count} spans</span>
          <span>${dur}</span>
          ${tokens ? `<span>${tokens} tok</span>` : ''}
        </div>
        <div class="trace-item-id">${esc(t.trace_id)}</div>
        <div class="trace-item-meta"><span>${time}</span>${t.session_id ? `<span>session: ${esc(t.session_id.slice(0,12))}</span>` : ''}</div>
      </div>`;
    }).join('');
  }

  renderDetailLoading() {
    this.els.detail.innerHTML = `<div class="trace-summary">${Array(6).fill('<div class="summary-card"><div class="skeleton-bar" style="width:60%;height:18px;margin-top:16px"></div></div>').join('')}</div>` + renderSkeleton(10);
  }

  renderDetail(trace) {
    const { spans, scores, activeTab } = this.state;
    const totalTokens = spans.reduce((s, sp) => s + (sp.input_tokens || 0) + (sp.output_tokens || 0), 0);
    const errors = spans.filter(s => s.error).length;
    const memMutations = spans.filter(s => s.span_type === 'memory_mutation');

    const tabActive = (name) => activeTab === name ? ' active' : '';

    this.els.detail.innerHTML = `
      <div class="trace-summary">
        ${summaryCard('Spans', spans.length)}
        ${summaryCard('Duration', fmtDuration(trace?.total_duration_ms))}
        ${summaryCard('Tokens', totalTokens)}
        ${summaryCard('Errors', errors, errors ? 'var(--red)' : 'var(--green)')}
        ${summaryCard('Scores', scores.length)}
        ${summaryCard('Mem Mutations', memMutations.length)}
      </div>
      <div class="trace-tabs" role="tablist">
        <div class="trace-tab${tabActive('waterfall')}" data-tab="waterfall" role="tab" aria-selected="${activeTab === 'waterfall'}">Waterfall</div>
        <div class="trace-tab${tabActive('scores')}" data-tab="scores" role="tab" aria-selected="${activeTab === 'scores'}">Scores (${scores.length})</div>
        <div class="trace-tab${tabActive('memory')}" data-tab="memory" role="tab" aria-selected="${activeTab === 'memory'}">Memory</div>
      </div>
      <div id="tab-waterfall" class="tab-content${tabActive('waterfall')}" role="tabpanel">${this.renderWaterfall(spans)}</div>
      <div id="tab-scores" class="tab-content${tabActive('scores')}" role="tabpanel">${renderScores(scores)}</div>
      <div id="tab-memory" class="tab-content${tabActive('memory')}" role="tabpanel">${this.renderMemoryTab(memMutations)}</div>
      <div id="spanDetail"></div>`;
  }

  // -- Waterfall with ticks, collapse, tooltip ------------------------
  renderWaterfall(spans) {
    if (!spans.length) return renderEmpty('No spans');

    // Build tree
    const byId = new Map(spans.map(s => [s.span_id, s]));
    const children = new Map();
    const roots = [];
    for (const s of spans) {
      if (s.parent_span_id && byId.has(s.parent_span_id)) {
        const list = children.get(s.parent_span_id) || [];
        list.push(s);
        children.set(s.parent_span_id, list);
      } else {
        roots.push(s);
      }
    }

    // Flatten with depth, respecting collapsed
    const flat = [];
    const walkTree = (node, depth) => {
      flat.push({ span: node, depth });
      if (this.state.collapsed.has(node.span_id)) return;
      const kids = children.get(node.span_id) || [];
      kids.sort((a, b) => new Date(a.start_time) - new Date(b.start_time));
      for (const kid of kids) walkTree(kid, depth + 1);
    };
    roots.sort((a, b) => new Date(a.start_time) - new Date(b.start_time));
    for (const r of roots) walkTree(r, 0);

    // Time range
    const minTime = Math.min(...spans.map(s => new Date(s.start_time).getTime()));
    const maxTime = Math.max(...spans.map(s => new Date(s.end_time).getTime()));
    const totalMs = Math.max(maxTime - minTime, 1);

    // Time axis ticks
    const TICKS = 5;
    const ticksHtml = Array.from({ length: TICKS + 1 }, (_, i) => {
      const pct = (i / TICKS * 100).toFixed(1);
      const ms = totalMs * i / TICKS;
      return `<span class="wf-tick" style="left:${pct}%">${fmtDuration(ms)}</span>`;
    }).join('');

    // Rows
    const rowsHtml = flat.map(({ span, depth }) => {
      const startPct = ((new Date(span.start_time).getTime() - minTime) / totalMs * 100).toFixed(2);
      const widthPct = Math.max(0.5, (span.duration_ms / totalMs * 100)).toFixed(2);
      const indent = depth * 16;
      const hasKids = children.has(span.span_id);
      const isCollapsed = this.state.collapsed.has(span.span_id);
      const hiddenCount = isCollapsed ? this.countDescendants(span.span_id, children) : 0;
      const toggleBtn = hasKids
        ? `<span class="wf-toggle" data-toggle-span="${esc(span.span_id)}" role="button" aria-label="${isCollapsed ? 'Expand' : 'Collapse'}" tabindex="0">${isCollapsed ? '▶' : '▼'}</span>`
        : '<span class="wf-toggle-spacer"></span>';
      const errMark = span.error ? ' <span class="wf-err" title="Error">⚠</span>' : '';
      const tooltip = `${esc(span.span_name)} · ${fmtDuration(span.duration_ms)}${span.model ? ' · ' + esc(span.model) : ''}${span.input_tokens ? ' · ' + span.input_tokens + '+' + (span.output_tokens||0) + ' tok' : ''}${span.error ? ' · ERROR' : ''}`;
      const active = span.span_id === this.state.selectedSpanId ? ' active' : '';
      const collapsedBadge = isCollapsed && hiddenCount ? ` <span class="wf-collapsed-badge">+${hiddenCount}</span>` : '';

      return `<div class="wf-row${active}" data-span-id="${esc(span.span_id)}" role="button" tabindex="0" aria-label="${esc(span.span_name)}">
        <div class="wf-name" title="${esc(span.span_name)}">
          <span class="wf-indent" style="width:${indent}px"></span>
          ${toggleBtn}
          <span class="wf-type-dot dot-${span.span_type}"></span>
          <span class="wf-label">${esc(span.span_name)}${errMark}${collapsedBadge}</span>
        </div>
        <div class="wf-bar-area">
          <div class="wf-bar type-${span.span_type}" style="left:${startPct}%;width:${widthPct}%" data-tooltip="${tooltip}"></div>
        </div>
        <span class="wf-duration">${fmtDuration(span.duration_ms)}</span>
      </div>`;
    }).join('');

    return `<div class="waterfall">
      <div class="waterfall-header">
        <div class="wf-name-col">Span</div>
        <div class="wf-bar-col">
          <div class="wf-axis">${ticksHtml}</div>
        </div>
        <span class="wf-duration-col">Time</span>
      </div>
      <div class="wf-bar-container" data-scale="1">${rowsHtml}</div>
      <div class="wf-legend">${['agent','generation','tool_use','guardrail','memory_mutation','system'].map(t =>
        `<span class="wf-legend-item"><span class="wf-type-dot dot-${t}"></span>${t.replace('_',' ')}</span>`
      ).join('')}</div>
      <div class="wf-zoom-hint">Ctrl+Scroll to zoom waterfall</div>
    </div>`;
  }

  countDescendants(spanId, children) {
    let count = 0;
    const kids = children.get(spanId) || [];
    for (const kid of kids) {
      count += 1 + this.countDescendants(kid.span_id, children);
    }
    return count;
  }

  // -- Memory tab -----------------------------------------------------
  renderMemoryTab(memMutations) {
    return `<div class="mem-key-input">
      <input type="text" id="memKeyInput" placeholder="Memory key (e.g. mem:global:openclaw)" aria-label="Memory key">
      <button class="btn-primary" id="btnMemQuery">Query Timeline</button>
    </div>
    <div id="memTimeline">${memMutations.length
      ? `<div class="mem-count">${memMutations.length} mutations in this trace:</div>` + renderMemoryEvents(memMutations)
      : renderEmpty('No memory mutations in this trace. Query a memory key above.')
    }</div>`;
  }
}

// ====================================================================
// Pure render functions (no state dependency)
// ====================================================================

function summaryCard(label, value, color) {
  const style = color ? ` style="color:${color}"` : '';
  return `<div class="summary-card"><div class="label">${label}</div><div class="value"${style}>${value}</div></div>`;
}

function renderSpanDetail(span) {
  const attrs = span.attributes && typeof span.attributes === 'object'
    ? JSON.stringify(span.attributes, null, 2) : '{}';
  return `<div class="span-detail">
    <h4>${esc(span.span_name)} <span class="span-type-label">${span.span_type}</span></h4>
    <div class="span-detail-grid">
      ${detailRow('Span ID', span.span_id, true)}
      ${detailRow('Trace ID', span.trace_id, true)}
      ${span.parent_span_id ? detailRow('Parent', span.parent_span_id, true) : ''}
      ${detailRow('Start', fmtTime(span.start_time))}
      ${detailRow('Duration', fmtDuration(span.duration_ms))}
      ${span.model ? detailRow('Model', span.model) : ''}
      ${span.input_tokens ? detailRow('Input Tokens', span.input_tokens) : ''}
      ${span.output_tokens ? detailRow('Output Tokens', span.output_tokens) : ''}
      ${span.error ? `<div class="span-detail-row"><span class="label">Error</span><span class="val err">${esc(span.error)}</span></div>` : ''}
    </div>
    <div class="span-attrs-label">ATTRIBUTES</div>
    <div class="span-attrs">${esc(attrs)}</div>
  </div>`;
}

function detailRow(label, value, mono) {
  const cls = mono ? ' class="mono"' : '';
  return `<div class="span-detail-row"><span class="label">${label}</span><span class="val"${cls}>${esc(String(value))}</span></div>`;
}

function renderScores(scores) {
  if (!scores.length) return renderEmpty('No scores');
  return scores.map(s => {
    const valColor = s.value != null
      ? (s.value >= 0.7 ? 'var(--green)' : s.value >= 0.4 ? 'var(--yellow)' : 'var(--red)')
      : 'var(--text)';
    return `<div class="score-card">
      <div class="score-name">${esc(s.name)}</div>
      ${s.value != null ? `<div class="score-val" style="color:${valColor}">${s.value.toFixed(3)}</div>` : ''}
      ${s.label ? `<div class="score-meta">Label: ${esc(s.label)}</div>` : ''}
      ${s.comment ? `<div class="score-meta">${esc(s.comment)}</div>` : ''}
      <div class="score-meta">Source: ${esc(s.source)} · ${fmtTime(s.created_at)}${s.span_id ? ` · span: ${esc(s.span_id.slice(0,12))}` : ''}</div>
    </div>`;
  }).join('');
}

function renderMemoryEvents(spans) {
  return spans.map(s => {
    const kind = s.attributes?.mutation_kind || s.span_type;
    const diff = s.attributes?.diff;
    const summary = diff?.summary || s.span_name;
    const memKey = s.attributes?.memory_key || '';
    return `<div class="mem-event">
      <div class="mem-event-dot"></div>
      <div class="mem-event-body">
        <div><span class="mem-event-kind">${esc(kind)}</span> <span class="mem-event-time">${fmtTime(s.start_time)}</span></div>
        <div class="mem-event-summary">${esc(summary)}</div>
        ${memKey ? `<div class="mem-event-key">${esc(memKey)}</div>` : ''}
        <div class="mem-event-ids">trace: ${esc(s.trace_id.slice(0,16))} · span: ${esc(s.span_id.slice(0,12))}</div>
      </div>
    </div>`;
  }).join('');
}

function renderSkeleton(rows) {
  return `<div class="skeleton-list" aria-label="Loading">${
    Array(rows).fill('<div class="skeleton-row"><div class="skeleton-bar"></div><div class="skeleton-bar short"></div></div>').join('')
  }</div>`;
}

function renderError(msg, retryAction) {
  return `<div class="error-state" role="alert">
    <div class="error-icon">⚠</div>
    <div class="error-msg">${esc(msg)}</div>
    <button class="btn-primary btn-retry" data-retry="${retryAction}">Retry</button>
  </div>`;
}

function renderEmpty(msg) {
  return `<div class="empty-state">${esc(msg)}</div>`;
}

// ====================================================================
// Helpers
// ====================================================================

function fmtDuration(ms) {
  if (ms == null) return '—';
  if (ms < 1) return '<1ms';
  if (ms < 1000) return Math.round(ms) + 'ms';
  if (ms < 60000) return (ms / 1000).toFixed(1) + 's';
  return (ms / 60000).toFixed(1) + 'm';
}

function fmtTime(iso) {
  if (!iso) return '—';
  const d = new Date(iso);
  if (isNaN(d)) return String(iso).slice(0, 19);
  return d.toLocaleString('en', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
}

function esc(s) {
  if (s == null) return '';
  const d = document.createElement('div');
  d.textContent = String(s);
  return d.innerHTML;
}

// ====================================================================
// Boot
// ====================================================================
document.addEventListener('DOMContentLoaded', () => {
  new TraceViewer(document.getElementById('app'));
});
