/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

// ============================================================
// Guixu Demo — UI Controller
// ============================================================

const engine = new GuixuEngine();

// DOM refs
const $ = id => document.getElementById(id);

// --- Mode Switcher ---
document.querySelectorAll('.mode-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.mode-btn').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');
    engine.mode = btn.dataset.mode;
    const m = engine.getMode();
    $('chainBadge').textContent = m.badge;
    $('ledgerChainInfo').querySelector('span:last-child').textContent = m.ledgerLabel;
    renderLog();
  });
});

// --- Start Button ---
$('btnStart').addEventListener('click', runPipeline);

async function runPipeline() {
  $('btnStart').disabled = true;
  resetUI();

  const query = $('taskInput').value;
  const budget = parseFloat($('budget').value) || 5.0;
  const sourceFilter = $('sourceFilter').value;

  // Step 1: active
  activateStep('step1');
  engine.log('[>]', `Agent task: "${query}"`);
  engine.log('[>]', `Budget: $${budget.toFixed(2)}`);
  renderLog();
  await delay(400);
  completeStep('step1');

  // Step 2: Search
  activateStep('step2');
  const { datasets, sources } = await engine.search(query, null, sourceFilter);
  renderSources(sources);
  await delay(300);
  renderSearchResults(datasets);
  renderLog();
  renderLedger();
  await delay(500);
  completeStep('step2', `${datasets.length} datasets`);

  if (datasets.length === 0) {
    engine.log('[!]', 'No datasets found. Try a different query or source.');
    renderLog();
    $('btnStart').disabled = false;
    return;
  }

  // Step 3: Evaluate
  activateStep('step3');
  await delay(400);
  const evalResults = await engine.evaluate(query, null, []);
  renderEvalResults(evalResults);
  renderTcvBreakdown(evalResults[0]);
  renderLog();
  renderLedger();
  await delay(500);
  const bestParts = scoreComposition(evalResults[0]);
  completeStep(
    'step3',
    bestParts.hasSampleScore
      ? `Best: Meta ${formatScore(bestParts.metadataScore)} · Sample ${formatScore(bestParts.sampleScore)}`
      : `Best: Meta ${formatScore(bestParts.metadataScore)}`,
  );

  // Step 4: Purchase
  activateStep('step4');
  await delay(400);
  const best = evalResults[0];
  const purchaseInfo = await engine.purchase(best);
  renderPurchase(best, purchaseInfo);
  renderCost();
  renderLog();
  renderLedger();
  await delay(500);
  completeStep('step4', `$${purchaseInfo.totalPaid.toFixed(4)}`);

  // Step 5: Feedback
  activateStep('step5');
  await delay(400);
  const fbInfo = await engine.feedback(best, true);
  renderFeedback(best, fbInfo);
  renderSignal(best);
  renderCost();
  renderLog();
  renderLedger();
  completeStep('step5', 'EAS OK');

  $('btnStart').disabled = false;
}

// --- BT Download & Preview ---
async function downloadDataset(idx) {
  const d = engine.datasets[idx];
  if (!d) return;
  const card = document.querySelector(`.result-card[data-idx="${idx}"]`);
  const btn = card?.querySelector('.btn-download');
  if (btn) { btn.disabled = true; btn.textContent = '⏳ Starting...'; }

  // Add progress bar to card
  let progressEl = card?.querySelector('.download-progress');
  if (!progressEl && card) {
    progressEl = document.createElement('div');
    progressEl.className = 'download-progress';
    progressEl.innerHTML = `<div class="progress-bar"><div class="progress-fill"></div></div><span class="progress-text">connecting...</span>`;
    card.appendChild(progressEl);
  }

  engine.log('[B]', `Downloading: ${d.title}`);
  renderLog();

  // Start download (non-blocking)
  const result = await engine.callTool('dataset_bt_download', { info_hash: d.cid });
  if (!result || result.status === 'failed') {
    if (btn) btn.textContent = '❌ Failed';
    if (progressEl) progressEl.querySelector('.progress-text').textContent = engine._lastError || 'failed';
    renderLog();
    return;
  }

  let consecutiveStatsErrors = 0;
  let lastProgressAt = Date.now();
  let lastProgressPct = 0;
  const finishWithError = (message) => {
    clearInterval(poll);
    if (btn) {
      btn.disabled = false;
      btn.textContent = '❌ Error';
    }
    const text = progressEl?.querySelector('.progress-text');
    if (text) text.textContent = message;
    engine.log('[!]', `BT download failed: ${message}`);
    renderLog();
  };

  // Poll stats until finished
  const poll = setInterval(async () => {
    const stats = await engine.callTool('dataset_bt_stats', { info_hash: d.cid });
    if (!stats) {
      consecutiveStatsErrors += 1;
      if (consecutiveStatsErrors >= 3) {
        finishWithError(engine._lastError || 'download status unavailable');
      }
      return;
    }
    consecutiveStatsErrors = 0;

    const pct = parseFloat(stats.progress_pct) || 0;
    const speed = stats.download_speed || '0 B/s';
    const fill = progressEl?.querySelector('.progress-fill');
    const text = progressEl?.querySelector('.progress-text');
    if (fill) fill.style.width = pct + '%';
    if (text) {
      const state = stats.state ? ` · ${stats.state}` : '';
      text.textContent = `${pct.toFixed(1)}% · ${speed}${stats.eta ? ' · ETA ' + stats.eta : ''}${state}`;
    }

    if (pct > lastProgressPct || speed !== '0 B/s') {
      lastProgressPct = pct;
      lastProgressAt = Date.now();
    } else if (Date.now() - lastProgressAt > 45000) {
      finishWithError('no peers or metadata found');
      return;
    }

    if (stats.finished) {
      clearInterval(poll);
      if (btn) btn.textContent = '✅ Done';
      if (text) text.textContent = `100% · ${formatBytes(stats.total_bytes)}`;
      engine.log('[B]', `Download complete: ${d.title} (${formatBytes(stats.total_bytes)})`);
      renderLog();
    }
    if (stats.error) {
      finishWithError(stats.error);
    }
  }, 1500);
}

async function previewDataset(idx) {
  const d = engine.datasets[idx];
  if (!d) return;
  const btn = document.querySelector(`.result-card[data-idx="${idx}"] .btn-preview`);
  if (btn) { btn.disabled = true; btn.textContent = '⏳ Loading...'; }
  engine.log('[P]', `Preview: ${d.title} (first pieces)`);
  renderLog();
  const data = await engine.callTool('dataset_bt_preview', { info_hash: d.cid, max_bytes: 65536 });
  if (data && data.preview) {
    showPreviewModal(d.title, d.dataType, data.preview);
  } else {
    engine.log('[!]', `Preview unavailable: ${engine._lastError || 'no data'}`);
  }
  if (btn) { btn.disabled = false; btn.textContent = '👁 Preview'; }
  renderLog();
}

function showPreviewModal(title, dataType, previewText) {
  let existing = document.getElementById('previewModal');
  if (existing) existing.remove();
  const modal = document.createElement('div');
  modal.id = 'previewModal';
  modal.className = 'preview-modal';
  modal.innerHTML = `
    <div class="preview-content">
      <div class="preview-header">
        <span>📋 ${title} <small>(${dataType})</small></span>
        <button onclick="this.closest('.preview-modal').remove()">✕</button>
      </div>
      <pre class="preview-body">${previewText.replace(/</g,'&lt;').replace(/>/g,'&gt;')}</pre>
    </div>`;
  document.body.appendChild(modal);
}

// --- Step state management ---
function activateStep(id) {
  const el = $(id);
  el.classList.remove('disabled', 'done');
  el.classList.add('active');
  $(id + 'Status').textContent = 'Running...';
}
function completeStep(id, status) {
  const el = $(id);
  el.classList.remove('active');
  el.classList.add('done');
  $(id + 'Status').textContent = status || 'done';
}
function resetUI() {
  engine.logs = [];
  engine.ledger = [];
  engine.totalCost = 0;
  engine.selectedDataset = null;
  engine.selectedDatasets = [];
  engine.selectionSummary = null;
  ['step2','step3','step4','step5'].forEach(id => {
    $(id).classList.add('disabled');
    $(id).classList.remove('active','done');
    $(id + 'Status').textContent = '';
  });
  $('step1Status').textContent = '';
  $('searchResults').innerHTML = '';
  $('sourceTags').innerHTML = '';
  $('evalResults').innerHTML = '';
  $('purchaseResult').innerHTML = '';
  $('feedbackResult').innerHTML = '';
  $('tcvBars').innerHTML = '';
  $('scoreSummary').innerHTML = '';
  $('scoreFormula').textContent = 'Metadata score comes from coarse metadata evaluation. Small-sample score comes from the downloaded sample utility check.';
  $('scoreMeta').textContent = '';
  $('logEntries').innerHTML = '';
  $('ledgerEntries').innerHTML = '';
  $('costDetails').innerHTML = '';
  $('signalDetails').innerHTML = '';
  $('statTx').textContent = '0';
  $('statCost').textContent = '$0.00';
  $('statAttest').textContent = '0';
}

function scoreValue(dataset) {
  const score = Number.isFinite(dataset?.finalScore) ? dataset.finalScore : dataset?.tcvScore;
  return Number.isFinite(score) ? score : 0;
}

function firstFinite(...values) {
  for (const value of values) {
    if (Number.isFinite(value)) return value;
  }
  return null;
}

function formatScore(value) {
  return Number.isFinite(value) ? value.toFixed(1) : '—';
}

function scoreToneClass(score) {
  return Number.isFinite(score) ? tcvVerdict(score).cls : 'score-unavailable';
}

function scoreComposition(dataset) {
  const proxyUtility = dataset?.finalBreakdown?.proxyUtility;
  const metadataScore = firstFinite(
    dataset?.coarseScore,
    dataset?.finalBreakdown?.coarseScore,
    dataset?.rawFinalScore,
    scoreValue(dataset),
  );
  const sampleScore = firstFinite(proxyUtility?.utilityScore);
  const sampledRows = firstFinite(proxyUtility?.sampledRows, dataset?.finalBreakdown?.proxyUtility?.sampledRows);
  const sampledBytes = firstFinite(proxyUtility?.sampledBytes, dataset?.finalBreakdown?.proxyUtility?.sampledBytes);
  const hasSampleScore = Number.isFinite(sampleScore) && Boolean(
    dataset?.sampleScored
      || dataset?.finalBreakdown?.hasSampleScore
      || proxyUtility,
  );
  return {
    metadataScore: metadataScore ?? 0,
    sampleScore,
    hasSampleScore,
    sampledRows: sampledRows ?? 0,
    sampledBytes: sampledBytes ?? 0,
    applyMode: proxyUtility?.applyMode || '',
    failureReason: dataset?.sampleFailureReason || '',
  };
}

function renderScoreBox(label, score) {
  const hasScore = Number.isFinite(score);
  return `<span class="eval-score-box">
    <span class="eval-score-kicker">${label}</span>
    <span class="eval-score ${scoreToneClass(score)}">${hasScore ? score.toFixed(1) : '—'}</span>
  </span>`;
}

function renderSummaryCard(label, score, meta) {
  const hasScore = Number.isFinite(score);
  return `<div class="score-summary-card ${hasScore ? '' : 'is-empty'}">
    <span class="score-summary-label">${label}</span>
    <span class="score-summary-value ${scoreToneClass(score)}">${hasScore ? score.toFixed(1) : '—'}</span>
    <span class="score-summary-meta">${meta}</span>
  </div>`;
}

function renderBreakdownBar(label, score, color, tag) {
  const hasScore = Number.isFinite(score);
  const width = hasScore ? Math.max(0, Math.min(100, score)) : 0;
  return `<div class="bar-row">
    <span class="bar-label">${label}</span>
    <span class="bar-weight">${tag}</span>
    <div class="bar-track">
      <div class="bar-fill ${hasScore ? '' : 'is-empty'}" style="width:${width}%;background:${hasScore ? color : 'transparent'}"></div>
    </div>
    <span class="bar-val ${hasScore ? '' : 'is-empty'}" style="${hasScore ? `color:${color}` : ''}">${hasScore ? score.toFixed(1) : '—'}</span>
  </div>`;
}

// --- Renderers ---
function renderSources(sources) {
  const allSources = ['P2P DHT', 'Kaggle', 'HuggingFace', 'BitTorrent', 'IPFS', 'PostgreSQL', 'DuckDB', 'Local File', 'Google Dataset Search', 'DataCite Commons'];
  $('sourceTags').innerHTML = allSources.map(s => {
    const key = s.toLowerCase().replace(/\s/g,'');
    const found = sources.some(src => key.includes(src) || src.includes(key));
    return `<span class="source-tag ${found ? 'found' : ''}">${s}${found ? ' +' : ''}</span>`;
  }).join('');
}

function renderSearchResults(datasets) {
  $('searchResults').innerHTML = datasets.map((d, i) => {
    const typeIcons = { tabular: '📊', video: '🎬', image: '🖼️', audio: '🎵', text: '📄' };
    const typeIcon = typeIcons[d.dataType] || '📊';
    const isBt = d.source === 'bittorrent';
    return `
    <div class="result-card" data-idx="${i}">
      <div class="result-title">
        <span class="result-source src-${d.source}">${d.sourceLabel}</span>
        <span class="result-type" title="${d.dataType}">${typeIcon} ${d.dataType}</span>
        ${d.title}
      </div>
      <div class="result-meta">
        <span>${d.schema.columns.length > 0 ? d.schema.columns.length + ' cols · ' : ''}${d.schema.rows > 0 ? d.schema.rows.toLocaleString() + ' rows · ' : ''}${d.schema.size}</span>
        <span>${d.price === 0 ? 'Free' : '$' + d.price.toFixed(4)}</span>
        <span>${isBt ? (d.description || '') : d.community.reviews + ' reviews'}</span>
      </div>
      <div class="result-actions">
        ${isBt ? `<button class="btn-sm btn-preview" onclick="event.stopPropagation();previewDataset(${i})">👁 Preview</button>` : ''}
        ${isBt ? `<button class="btn-sm btn-download" onclick="event.stopPropagation();downloadDataset(${i})">⬇ Download</button>` : ''}
      </div>
    </div>`;
  }).join('');
}

function renderEvalResults(results) {
  $('evalResults').innerHTML = results.map((r, i) => {
    const v = r.verdict;
    const isBest = i === 0;
    const isSelected = Boolean(r.selectedInCollection);
    const parts = scoreComposition(r);
    const detail = [
      v.text,
      `Meta ${formatScore(parts.metadataScore)}`,
      parts.hasSampleScore
        ? `Sample ${formatScore(parts.sampleScore)}`
        : (parts.failureReason ? `No sample score: ${parts.failureReason}` : 'sample not scored'),
      isSelected ? 'selected bundle member' : '',
      r.source === 'p2p' ? 'P2P' : r.sourceLabel,
    ].filter(Boolean).join(' · ');
    const preview = [
      renderMiniBar('Metadata', parts.metadataScore, '#3b82f6'),
      renderMiniBar('Sample', parts.hasSampleScore ? parts.sampleScore : null, '#22c55e'),
    ].join('');
    return `
    <div class="eval-card ${isBest ? 'best' : ''}" data-idx="${i}" onclick="showFinalValueDetail(${i})">
      <div class="eval-header">
        <span>${isBest ? '[BEST] ' : (isSelected ? '[SELECTED] ' : '')}${r.title}</span>
        <span class="eval-score-stack">
          ${renderScoreBox('Meta', parts.metadataScore)}
          ${renderScoreBox('Sample', parts.hasSampleScore ? parts.sampleScore : null)}
        </span>
      </div>
      <div class="eval-verdict">${detail}</div>
      ${preview}
    </div>`;
  }).join('');
  // Store for detail view
  window._evalResults = results;
}

function renderMiniBar(label, val, color) {
  const hasScore = Number.isFinite(val);
  const width = hasScore ? Math.max(0, Math.min(100, val)) : 0;
  return `<div class="eval-bar-row">
    <span class="eval-bar-label">${label}</span>
    <div class="eval-bar-track"><div class="eval-bar-fill ${hasScore ? '' : 'is-empty'}" style="width:${width}%;background:${hasScore ? color : 'transparent'}"></div></div>
    <span style="width:30px;font-size:10px;color:${hasScore ? color : 'var(--text-dim)'}">${hasScore ? val.toFixed(0) : '—'}</span>
  </div>`;
}

// Click handler for eval cards
window.showFinalValueDetail = function(idx) {
  if (window._evalResults) renderTcvBreakdown(window._evalResults[idx]);
};

function renderTcvBreakdown(dataset) {
  const parts = scoreComposition(dataset);
  $('scoreFormula').textContent = parts.hasSampleScore
    ? 'Metadata score comes from coarse metadata evaluation. Small-sample score comes from the downloaded sample utility check.'
    : `Only the metadata-side score is available for this candidate. ${parts.failureReason ? `Sample stage did not produce a score: ${parts.failureReason}.` : 'No small sample was downloaded and scored.'}`;
  $('tcvBars').innerHTML = [
    renderBreakdownBar('Metadata Score', parts.metadataScore, '#3b82f6', 'coarse'),
    renderBreakdownBar('Small-Sample Score', parts.hasSampleScore ? parts.sampleScore : null, '#22c55e', 'sample'),
  ].join('');
  $('scoreSummary').innerHTML = [
    renderSummaryCard('Metadata Score', parts.metadataScore, dataset.metadataResolved ? 'resolved metadata' : 'search/result metadata'),
    renderSummaryCard(
      'Small-Sample Score',
      parts.hasSampleScore ? parts.sampleScore : null,
      parts.hasSampleScore
        ? `${parts.sampledRows ? parts.sampledRows + ' rows' : 'downloaded sample'}${parts.sampledBytes ? ' · ' + formatBytes(parts.sampledBytes) : ''}`
        : (parts.failureReason || 'not sampled'),
    ),
  ].join('');
  const meta = [];
  if (parts.applyMode) meta.push(String(parts.applyMode).replace(/_/g, ' '));
  if (parts.hasSampleScore) meta.push('sample scored');
  if (!parts.hasSampleScore) meta.push('sample not scored');
  if (parts.sampledRows) meta.push(`${parts.sampledRows} sampled rows`);
  if (dataset.evaluationMode) meta.push(String(dataset.evaluationMode).replace(/_/g, ' '));
  if (!parts.hasSampleScore && parts.failureReason) meta.push(parts.failureReason);
  $('scoreMeta').textContent = meta.join(' · ');
}

function renderPurchase(dataset, info) {
  const isBt = dataset.source === 'bittorrent';
  $('purchaseResult').innerHTML = `
    <div class="purchase-card" ${isBt ? 'style="border-color:var(--orange)"' : ''}>
      <div class="purchase-row"><span class="label">Dataset</span><span>${dataset.title}</span></div>
      <div class="purchase-row"><span class="label">Price</span><span>${isBt ? 'Free (BT)' : '$' + dataset.price.toFixed(4)}</span></div>
      ${isBt ? '' : `<div class="purchase-row"><span class="label">Gas</span><span>$${info.gasCost.toFixed(4)}</span></div>`}
      <div class="purchase-row"><span class="label">Protocol</span><span class="purchase-protocol">${info.pay.protocol}</span></div>
      <div class="purchase-row"><span class="label">Delivery</span><span>${info.delivery}${!isBt ? ' · ' + dataset.schema.size : ''}</span></div>
      <div class="purchase-row"><span class="label">${isBt ? 'InfoHash' : 'TX'}</span><span style="font-family:monospace;font-size:10px">${shortHash(info.txId)}</span></div>
    </div>`;
}

function renderFeedback(dataset, info) {
  const fb = info.fb;
  $('feedbackResult').innerHTML = `
    <div class="feedback-card">
      <div class="purchase-row"><span class="label">Rating</span><span>${fb.assessment === 'positive' ? '+ Positive' : '- Negative'}</span></div>
      <div class="purchase-row"><span class="label">Relevance</span><span>${fb.relevance}</span></div>
      <div class="purchase-row"><span class="label">Quality</span><span>${fb.quality}/5</span></div>
      <div class="purchase-row"><span class="label">Task Success</span><span>${fb.success ? 'Yes' : 'No'}</span></div>
      <div class="purchase-row"><span class="label">Comment</span><span>${fb.comment}</span></div>
      <div class="purchase-row"><span class="label">EAS Gas</span><span>$${info.attestCost.toFixed(4)}</span></div>
    </div>`;
}

function renderLog() {
  $('logEntries').innerHTML = engine.logs.map(l =>
    `<div class="log-entry"><span class="log-time">${l.time}</span><span class="log-icon">${l.icon}</span><span class="log-msg">${l.msg}</span></div>`
  ).join('');
  $('logEntries').scrollTop = $('logEntries').scrollHeight;
}

function renderLedger() {
  $('ledgerEntries').innerHTML = engine.ledger.map(l =>
    `<div class="ledger-entry type-${l.type}">
      <div><span class="ledger-type">${l.type}</span> <span style="float:right;color:var(--text-dim);font-size:10px">${l.timestamp}</span></div>
      <div class="ledger-detail">${l.detail}</div>
      <div class="ledger-hash">${shortHash(l.hash)}</div>
    </div>`
  ).join('');

  const txCount = engine.ledger.filter(l => l.type === 'purchase' || l.type === 'download').length;
  const attestCount = engine.ledger.filter(l => l.type === 'attestation' || l.type === 'feedback').length;
  $('statTx').textContent = txCount;
  $('statCost').textContent = '$' + engine.totalCost.toFixed(4);
  $('statAttest').textContent = attestCount;
}

function renderCost() {
  const m = engine.getMode();
  const items = [];
  engine.ledger.forEach(l => {
    if (l.type === 'purchase') items.push({ label: 'Data Purchase', val: l.detail.match(/\$[\d.]+/)?.[0] || '$0' });
    if (l.attestCost) items.push({ label: 'EAS Attestation Gas', val: '$' + l.attestCost.toFixed(4) });
  });
  if (items.length === 0) items.push({ label: 'No cost yet', val: '$0.00' });

  $('costDetails').innerHTML = items.map(i =>
    `<div class="cost-row"><span class="label">${i.label}</span><span>${i.val}</span></div>`
  ).join('') + `<div class="cost-row cost-total"><span>Total (${m.chain})</span><span>$${engine.totalCost.toFixed(4)}</span></div>`;
}

function renderSignal(dataset) {
  const c = dataset.community;
  if (c.reviews === 0) {
    $('signalDetails').innerHTML = '<div style="font-size:12px;color:var(--text-dim)">No community feedback yet</div>';
    return;
  }
  $('signalDetails').innerHTML = `
    <div class="signal-row"><span>Total Reviews</span><span>${c.reviews}</span></div>
    <div class="signal-row"><span>Avg Relevance</span><span>${c.avg_relevance.toFixed(2)}</span></div>
    <div class="signal-row">
      <span>Positive Rate</span>
      <span><span class="signal-bar"><span class="signal-bar-fill" style="width:${c.positive_rate*100}%;background:var(--green)"></span></span> ${(c.positive_rate*100).toFixed(0)}%</span>
    </div>
    <div class="signal-row">
      <span>Negative Rate</span>
      <span><span class="signal-bar"><span class="signal-bar-fill" style="width:${c.negative_rate*100}%;background:var(--red)"></span></span> ${(c.negative_rate*100).toFixed(0)}%</span>
    </div>
    ${c.task_signals.map(ts => `
      <div class="signal-row" style="margin-top:4px;padding-top:4px;border-top:1px solid var(--border)">
        <span>${ts.task_type}</span>
        <span>${ts.count} uses · ${(ts.success_rate*100).toFixed(0)}% success</span>
      </div>
    `).join('')}
  `;
}

function delay(ms) { return new Promise(r => setTimeout(r, ms)); }
