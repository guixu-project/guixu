// ============================================================
// Guixu Demo — Live Engine
// Connects to real MCP server (data-node start)
// ============================================================

const RPC_URL = '/rpc';

const MODES = {
  'base-x402': {
    chain: 'Base L2', protocol: 'x402', token: 'USDC',
    badge: 'Base L2 / USDC / x402',
    ledgerLabel: 'Base L2 — EAS Attestations',
    gasCost: 0.001, attestCost: 0.0005,
  },
  'op-mpp': {
    chain: 'OP Mainnet', protocol: 'Stripe MPP', token: 'USDC',
    badge: 'OP Mainnet / USDC / MPP',
    ledgerLabel: 'OP Mainnet — EAS Attestations',
    gasCost: 0.0008, attestCost: 0.0003,
  },
  'arb-escrow': {
    chain: 'Arbitrum One', protocol: 'ERC-8183 Escrow', token: 'USDC',
    badge: 'Arbitrum / USDC / Escrow',
    ledgerLabel: 'Arbitrum — EAS Attestations',
    gasCost: 0.0005, attestCost: 0.0002,
  },
};

const TCV_WEIGHTS = {
  schema_fit: { weight: 0.25, label: 'SchemaFit', color: '#3b82f6', symbol: 'α' },
  temporal_fit: { weight: 0.15, label: 'TemporalFit', color: '#06b6d4', symbol: 'β' },
  info_gain: { weight: 0.15, label: 'InfoGain', color: '#22c55e', symbol: 'γ' },
  quality: { weight: 0.10, label: 'Quality', color: '#eab308', symbol: 'δ' },
  community: { weight: 0.15, label: 'Community', color: '#a855f7', symbol: 'ε' },
  risk: { weight: -0.20, label: 'RiskPenalty', color: '#ef4444', symbol: 'ζ' },
};

function computeTCV(c) {
  const raw = TCV_WEIGHTS.schema_fit.weight * c.schema_fit
    + TCV_WEIGHTS.temporal_fit.weight * c.temporal_fit
    + TCV_WEIGHTS.info_gain.weight * c.info_gain
    + TCV_WEIGHTS.quality.weight * c.quality
    + TCV_WEIGHTS.community.weight * c.community
    + TCV_WEIGHTS.risk.weight * c.risk;
  return Math.max(-100, Math.min(100, raw));
}

function tcvVerdict(score) {
  if (score > 60) return { label: 'StrongPositive', cls: 'score-strong-pos', text: 'Strongly Recommended' };
  if (score > 30) return { label: 'Positive', cls: 'score-pos', text: 'Recommended' };
  if (score > 0) return { label: 'Neutral', cls: 'score-neutral', text: 'Marginal' };
  if (score > -30) return { label: 'Negative', cls: 'score-neg', text: 'Not Recommended' };
  return { label: 'StrongNegative', cls: 'score-strong-neg', text: 'Harmful' };
}

function randomHash() {
  return '0x' + Array.from({length: 64}, () => Math.floor(Math.random()*16).toString(16)).join('');
}

function shortHash(h) { return h.slice(0, 10) + '...' + h.slice(-6); }

class GuixuEngine {
  constructor() {
    this.mode = 'base-x402';
    this.ledger = [];
    this.totalCost = 0;
    this.datasets = [];
    this.selectedDataset = null;
    this.logs = [];
  }

  getMode() { return MODES[this.mode]; }

  log(icon, msg) {
    const t = new Date().toLocaleTimeString('en', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
    this.logs.push({ time: t, icon, msg });
  }

  addLedger(type, detail, extra = {}) {
    this.ledger.unshift({
      type, detail, hash: randomHash(),
      chain: this.getMode().chain,
      timestamp: new Date().toLocaleTimeString('en', { hour12: false }),
      ...extra,
    });
  }

  // JSON-RPC call to real MCP server (with timeout)
  async rpc(method, params, timeoutMs = 30000) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const res = await fetch(RPC_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: Date.now(), method, params }),
        signal: controller.signal,
      });
      clearTimeout(timeout);
      const json = await res.json();
      if (json.error) throw new Error(json.error.message);
      return json.result;
    } catch (e) {
      clearTimeout(timeout);
      this._lastError = e.message || String(e);
      return null;
    }
  }

  async callTool(name, args) {
    const BT_TOOLS = ['dataset_bt_download', 'dataset_bt_preview'];
    const timeoutMs = BT_TOOLS.includes(name) ? 120000 : 30000;
    const result = await this.rpc('tools/call', { name, arguments: args }, timeoutMs);
    if (result && result.content && result.content[0]) {
      try { return JSON.parse(result.content[0].text); } catch { return result.content[0].text; }
    }
    return null;
  }

  // Step 2: Search
  async search(query, taskType, sourceFilter) {
    this.log('[S]', `dataset_search("${query}"${sourceFilter ? `, source=${sourceFilter}` : ''})`);

    const filters = {};
    if (sourceFilter) filters.source = sourceFilter;
    this._lastError = null;
    const data = await this.callTool('dataset_search', {
      query,
      task_type: taskType,
      filters,
      limit: 10,
    });

    // Response is { results: [...], errors: [...] } or legacy array
    const results = Array.isArray(data) ? data : (data?.results || []);
    const serverErrors = Array.isArray(data) ? [] : (data?.errors || []);

    // Log intent parsing result
    const intent = data?.intent;
    if (intent) {
      this.log('[I]', `Intent parsed → task_type=${intent.task_type || '—'}, entity=${intent.target_entity || '—'}, keywords=[${(intent.keywords || []).join(', ')}]`);
      if (intent.task_description) this.log('[I]', `Task: ${intent.task_description}`);
      this.addLedger('intent', `query="${query}" → keywords=[${(intent.keywords || []).join(', ')}]`);
    }

    if (serverErrors.length > 0) {
      serverErrors.forEach(e => this.log('[!]', `adapter error: ${e}`));
    }

    if (results.length > 0) {
      this.datasets = results.map(r => ({
        cid: r.cid,
        title: r.title,
        description: r.description,
        source: r.source ? r.source.toLowerCase().replace(/\s/g,'') : 'p2p',
        sourceLabel: r.source || 'P2P',
        dataType: r.data_type || 'tabular',
        schema: {
          columns: Array.from({ length: r.schema?.columns || 0 }, (_, i) => `col_${i}`),
          rows: r.schema?.rows || 0,
          size: formatBytes(r.schema?.size_bytes || 0),
          sizeBytes: r.schema?.size_bytes || 0,
        },
        price: typeof r.price === 'object' ? r.price.amount : (r.price || 0),
        community: {
          reviews: r.community?.total_reviews || 0,
          avg_relevance: parseFloat(r.community?.avg_relevance) || 0,
          positive_rate: parseFloat(r.community?.positive_rate) / 100 || 0,
          negative_rate: parseFloat(r.community?.negative_rate) / 100 || 0,
          task_signals: [],
        },
        tcv: null,
        _raw: r,
      }));
      const sources = [...new Set(this.datasets.map(d => d.source))];
      const rawCount = results.length;

      // Log deduplication
      const uniqueCids = new Set(this.datasets.map(d => d.cid));
      if (uniqueCids.size < rawCount) {
        this.log('[S]', `Dedup: ${rawCount} → ${uniqueCids.size} (removed ${rawCount - uniqueCids.size} duplicates)`);
      }

      // Log modality filtering
      if (intent?.task_type) {
        const types = [...new Set(this.datasets.map(d => d.dataType))];
        this.log('[S]', `Modality filter: task_type=${intent.task_type} → kept types=[${types.join(', ')}]`);
      }

      // Log per-source breakdown
      const perSource = {};
      this.datasets.forEach(d => { perSource[d.sourceLabel] = (perSource[d.sourceLabel] || 0) + 1; });
      const breakdown = Object.entries(perSource).map(([s, n]) => `${s}:${n}`).join(', ');
      this.log('[S]', `${this.datasets.length} results from ${sources.length} sources (${breakdown})`);

      // Log TCV-lite ranking
      if (this.datasets.length > 1 && this.datasets[0]._raw.rank_score) {
        this.log('[R]', `Ranked by TCV-lite: #1 score=${this.datasets[0]._raw.rank_score}, #${this.datasets.length} score=${this.datasets[this.datasets.length-1]._raw.rank_score}`);
      }

      this.addLedger('search', `query="${query}" > ${this.datasets.length} results`);
      return { datasets: this.datasets, sources };
    }

    // 0 results — show error reason
    this.datasets = [];
    const reason = serverErrors.length > 0
      ? serverErrors.join('; ')
      : (this._lastError || 'no matching datasets found');
    this.log('[!]', `0 results (${reason})`);
    this.addLedger('search', `query="${query}" > 0 results`);
    return { datasets: [], sources: [] };
  }

  // Step 3: Evaluate
  async evaluate(taskDesc, taskType, requiredCols) {
    this.log('[E]', `dataset_evaluate() x ${this.datasets.length}`);

    const results = [];
    for (const d of this.datasets) {
      let tcvComps = d.tcv;
      let tcvScore;

      if (!tcvComps && d.source === 'p2p') {
        // Call backend for local P2P datasets (they exist in store)
        const evalData = await this.callTool('dataset_evaluate', {
          cid: d.cid,
          task_description: taskDesc,
          task_type: taskType,
          required_columns: requiredCols,
          budget: 10,
        });
        if (evalData && evalData.tcv) {
          const t = evalData.tcv;
          tcvComps = {
            schema_fit: t.schema_fit,
            temporal_fit: t.temporal_fit,
            info_gain: t.information_gain,
            quality: t.quality_score,
            community: t.community_signal,
            risk: t.risk_penalty,
          };
          tcvScore = t.tcv_score;
          this.log('[E]', `${d.title}: server TCV (P2P store)`);
        }
      }

      // Client-side TCV for external datasets (BT, Kaggle, HF, etc.)
      if (!tcvComps) {
        const sizeScore = Math.min(100, (d.schema.rows || 1) / 100);
        tcvComps = d.tcv || {
          schema_fit: d.schema.columns.length > 0 ? 60 : 20,
          temporal_fit: 50,
          info_gain: 60,
          quality: Math.min(80, 30 + sizeScore * 0.5),
          community: d.community.reviews > 0 ? d.community.positive_rate * 80 : 30,
          risk: d.community.negative_rate * 100 || 0,
        };
        this.log('[E]', `${d.title}: client-side TCV estimate (${d.sourceLabel})`);
      }
      if (tcvScore === undefined) tcvScore = computeTCV(tcvComps);

      const verdict = tcvVerdict(tcvScore);
      results.push({ ...d, tcv: tcvComps, tcvScore, verdict });
    }

    results.sort((a, b) => b.tcvScore - a.tcvScore);
    results.forEach((r, i) => {
      this.log(i === 0 ? '[*]' : '[.]', `${r.title}: TCV=${r.tcvScore.toFixed(1)} (${r.verdict.text})`);
      this.addLedger('evaluate', `${r.title} > TCV ${r.tcvScore.toFixed(1)}`);
    });
    this.selectedDataset = results[0];
    this.log('[+]', `Best pick: ${results[0].title}`);
    return results;
  }

  // Step 4: Purchase
  async purchase(dataset) {
    const m = this.getMode();

    // BT datasets: download instead of purchase
    if (dataset.source === 'bittorrent') {
      return this.btDownload(dataset);
    }

    const data = await this.callTool('dataset_purchase', { cid: dataset.cid, max_price: 10 });
    if (data && data.status === 'purchased') {
      const paid = data.price_paid || 0;
      const gasCost = m.gasCost;
      this.totalCost += paid + gasCost;
      // Log budget check
      if (data.budget_check) this.log('[P]', `Budget check: ${data.budget_check}`);
      // Log protocol selection reasoning
      if (data.protocol_selection_reason) this.log('[P]', `Protocol selection: ${data.protocol_selection_reason}`);
      this.log('[P]', `${data.payment_protocol} $${paid}`);
      // Log delivery resolution
      const deliveryMethod = data.delivery?.method || 'local';
      const deliveryPath = data.delivery?.file_path || data.delivery?.download_path || '';
      this.log('[D]', `Delivery: ${deliveryMethod}${deliveryPath ? ' → ' + deliveryPath : ''}`);
      const txId = data.tx_id || randomHash();
      this.addLedger('purchase', `${dataset.title} — $${(paid + gasCost).toFixed(4)}`, { txId });
      return {
        pay: { protocol: data.payment_protocol, desc: data.protocol_description },
        gasCost, totalPaid: paid + gasCost, txId,
        delivery: data.delivery?.method || 'local',
      };
    }

    const reason = this._lastError || 'purchase failed';
    this.log('[!]', `Purchase error: ${reason}`);
    return { pay: { protocol: 'none', desc: reason }, gasCost: 0, totalPaid: 0, txId: '', delivery: 'failed' };
  }

  // BT Download — for BitTorrent sourced datasets
  async btDownload(dataset) {
    this.log('[B]', `dataset_bt_download("${dataset.cid}")`);

    const data = await this.callTool('dataset_bt_download', { info_hash: dataset.cid });
    if (data && data.status === 'completed') {
      this.log('[B]', `Downloaded to ${data.downloaded_to}`);
      this.addLedger('download', `BT download: ${dataset.title}`, { txId: dataset.cid });
      return {
        pay: { protocol: 'BitTorrent', desc: 'Free P2P download via BT DHT' },
        gasCost: 0, totalPaid: 0, txId: dataset.cid,
        delivery: `BitTorrent DHT · ${dataset.schema.size}`,
      };
    }

    const reason = this._lastError || 'BT download failed';
    this.log('[!]', `BT error: ${reason}`);
    return {
      pay: { protocol: 'BitTorrent', desc: reason },
      gasCost: 0, totalPaid: 0, txId: dataset.cid || '',
      delivery: 'failed',
    };
  }

  // Step 5: Feedback
  async feedback(dataset, positive) {
    const m = this.getMode();
    const attestCost = m.attestCost;
    this.totalCost += attestCost;

    const fb = positive
      ? { relevance: 0.92, quality: 4, success: true, assessment: 'positive', comment: 'Complete data, good prediction results' }
      : { relevance: -0.5, quality: 2, success: false, assessment: 'negative', comment: 'Data does not match task requirements' };

    await this.callTool('dataset_feedback', {
      cid: dataset.cid,
      relevance_score: fb.relevance,
      quality_rating: fb.quality,
      task_success: fb.success,
      value_assessment: fb.assessment,
      task_type: 'time_series_prediction',
      task_description: 'GDP prediction',
      comment: fb.comment,
    });
    this.log('[F]', `Feedback recorded`);

    this.log('[A]', `EAS attestation on ${m.chain} (gas: $${attestCost.toFixed(4)})`);
    this.addLedger('feedback', `${fb.assessment} — "${fb.comment}"`, { attestCost });
    this.addLedger('attestation', `EAS schema: DatasetFeedback v1 on ${m.chain}`);
    return { fb, attestCost };
  }
}

function formatBytes(b) {
  if (b < 1024) return b + ' B';
  if (b < 1048576) return (b / 1024).toFixed(1) + ' KB';
  if (b < 1073741824) return (b / 1048576).toFixed(1) + ' MB';
  return (b / 1073741824).toFixed(1) + ' GB';
}
