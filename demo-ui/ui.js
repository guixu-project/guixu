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
  const taskType = $('taskType').value;
  const budget = parseFloat($('budget').value) || 5.0;
  const sourceFilter = $('sourceFilter').value;

  // Probe server
  await engine.init();

  // Step 1: active
  activateStep('step1');
  engine.log('[>]', `Agent task: "${query}"`);
  engine.log('[>]', `Type: ${taskType}, Budget: $${budget.toFixed(2)}`);
  renderLog();
  await delay(400);
  completeStep('step1');

  // Step 2: Search
  activateStep('step2');
  const { datasets, sources } = await engine.search(query, taskType, sourceFilter);
  renderSources(sources);
  await delay(300);
  renderSearchResults(datasets);
  renderLog();
  renderLedger();
  await delay(500);
  completeStep('step2', `${datasets.length} datasets`);

  // Step 3: Evaluate
  activateStep('step3');
  await delay(400);
  const evalResults = await engine.evaluate(query, taskType, []);
  renderEvalResults(evalResults);
  renderTcvBreakdown(evalResults[0]);
  renderLog();
  renderLedger();
  await delay(500);
  completeStep('step3', `Best: TCV ${evalResults[0].tcvScore.toFixed(1)}`);

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
  $('tcvScore').textContent = '—';
  $('tcvScore').className = '';
  $('logEntries').innerHTML = '';
  $('ledgerEntries').innerHTML = '';
  $('costDetails').innerHTML = '';
  $('signalDetails').innerHTML = '';
  $('statTx').textContent = '0';
  $('statCost').textContent = '$0.00';
  $('statAttest').textContent = '0';
}

// --- Renderers ---
function renderSources(sources) {
  const allSources = ['P2P DHT', 'Kaggle', 'HuggingFace', 'BitTorrent', 'IPFS', 'PostgreSQL', 'DuckDB'];
  $('sourceTags').innerHTML = allSources.map(s => {
    const found = sources.some(src => s.toLowerCase().replace(/\s/g,'').includes(src));
    return `<span class="source-tag ${found ? 'found' : ''}">${s}${found ? ' +' : ''}</span>`;
  }).join('');
}

function renderSearchResults(datasets) {
  $('searchResults').innerHTML = datasets.map((d, i) => `
    <div class="result-card" data-idx="${i}">
      <div class="result-title">
        <span class="result-source src-${d.source}">${d.sourceLabel}</span>
        ${d.title}
      </div>
      <div class="result-meta">
        <span>${d.schema.columns.length > 0 ? d.schema.columns.length + ' cols · ' : ''}${d.schema.rows > 0 ? d.schema.rows.toLocaleString() + ' rows · ' : ''}${d.schema.size}</span>
        <span>${d.price === 0 ? 'Free' : '$' + d.price.toFixed(4)}</span>
        <span>${d.source === 'bittorrent' ? (d.description || '') : d.community.reviews + ' reviews'}</span>
      </div>
    </div>
  `).join('');
}

function renderEvalResults(results) {
  $('evalResults').innerHTML = results.map((r, i) => {
    const v = r.verdict;
    const isBest = i === 0;
    return `
    <div class="eval-card ${isBest ? 'best' : ''}" data-idx="${i}" onclick="showTcvDetail(${i})">
      <div class="eval-header">
        <span>${isBest ? '[BEST] ' : ''}${r.title}</span>
        <span class="eval-score ${v.cls}">${r.tcvScore.toFixed(1)}</span>
      </div>
      <div class="eval-verdict">${v.text} · ${r.source === 'p2p' ? 'P2P' : r.sourceLabel}</div>
      ${renderMiniBar('Schema', r.tcv.schema_fit, '#3b82f6')}
      ${renderMiniBar('Temporal', r.tcv.temporal_fit, '#06b6d4')}
      ${renderMiniBar('InfoGain', r.tcv.info_gain, '#22c55e')}
      ${renderMiniBar('Quality', r.tcv.quality, '#eab308')}
      ${renderMiniBar('Community', r.tcv.community, '#a855f7')}
      ${r.tcv.risk > 10 ? renderMiniBar('RISK', r.tcv.risk, '#ef4444') : ''}
    </div>`;
  }).join('');
  // Store for detail view
  window._evalResults = results;
}

function renderMiniBar(label, val, color) {
  return `<div class="eval-bar-row">
    <span class="eval-bar-label">${label}</span>
    <div class="eval-bar-track"><div class="eval-bar-fill" style="width:${val}%;background:${color}"></div></div>
    <span style="width:30px;font-size:10px;color:${color}">${val.toFixed(0)}</span>
  </div>`;
}

// Click handler for eval cards
window.showTcvDetail = function(idx) {
  if (window._evalResults) renderTcvBreakdown(window._evalResults[idx]);
};

function renderTcvBreakdown(dataset) {
  const comps = dataset.tcv;
  let html = '';
  for (const [key, cfg] of Object.entries(TCV_WEIGHTS)) {
    const val = comps[key] || 0;
    const weighted = cfg.weight * val;
    const barW = Math.abs(val);
    html += `<div class="bar-row">
      <span class="bar-label">${cfg.symbol} ${cfg.label}</span>
      <span class="bar-weight">×${cfg.weight >= 0 ? '+' : ''}${cfg.weight.toFixed(2)}</span>
      <div class="bar-track"><div class="bar-fill" style="width:${barW}%;background:${cfg.color}"></div></div>
      <span class="bar-val" style="color:${cfg.color}">${weighted >= 0 ? '+' : ''}${weighted.toFixed(1)}</span>
    </div>`;
  }
  $('tcvBars').innerHTML = html;

  const score = dataset.tcvScore;
  const v = dataset.verdict;
  $('tcvScore').textContent = score.toFixed(1);
  $('tcvScore').className = v.cls;
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
