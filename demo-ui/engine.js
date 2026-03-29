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
  return Math.max(0, Math.min(100, (raw + 100) / 2));
}

function tcvVerdict(score) {
  if (score > 80) return { label: 'StrongPositive', cls: 'score-strong-pos', text: 'Strongly Recommended' };
  if (score > 65) return { label: 'Positive', cls: 'score-pos', text: 'Recommended' };
  if (score > 50) return { label: 'Neutral', cls: 'score-neutral', text: 'Marginal' };
  if (score > 35) return { label: 'Negative', cls: 'score-neg', text: 'Not Recommended' };
  return { label: 'StrongNegative', cls: 'score-strong-neg', text: 'Harmful' };
}

function randomHash() {
  return '0x' + Array.from({length: 64}, () => Math.floor(Math.random()*16).toString(16)).join('');
}

function shortHash(h) { return h.slice(0, 10) + '...' + h.slice(-6); }

function prettySourceName(source) {
  const map = {
    p2p: 'P2P',
    bittorrent: 'BitTorrent',
    huggingface: 'HuggingFace',
    kaggle: 'Kaggle',
    ipfs: 'IPFS',
    postgresql: 'PostgreSQL',
    duckdb: 'DuckDB',
  };
  return map[source] || source || 'Unknown';
}

function tokenize(text) {
  const stopwords = new Set([
    'a', 'an', 'and', 'as', 'at', 'be', 'build', 'by', 'data', 'dataset', 'for', 'from',
    'i', 'in', 'into', 'is', 'need', 'of', 'on', 'or', 'predict', 'the', 'to', 'with',
  ]);
  return String(text || '')
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(token => token.length > 1 && !stopwords.has(token));
}

function lexicalSimilarity(left, right) {
  const leftTokens = tokenize(left);
  const rightTokens = new Set(tokenize(right));
  if (leftTokens.length === 0 || rightTokens.size === 0) return 0;
  const matched = leftTokens.filter(token => rightTokens.has(token)).length;
  return (matched / leftTokens.length) * 100;
}

function normalizeLog(value, saturation) {
  if (!value || value <= 0) return 0;
  if (!saturation || saturation <= 1) return 100;
  return Math.min(100, (Math.log(value + 1) / Math.log(saturation + 1)) * 100);
}

function parseSeeders(description) {
  const match = String(description || '').match(/(\d+)\s+seeders?/i);
  return match ? parseInt(match[1], 10) : 0;
}

function inferRequiredColumns(taskDesc, taskType) {
  const query = String(taskDesc || '').toLowerCase();
  if (taskType === 'classification') return ['label'];
  if (taskType === 'time_series_prediction') {
    if (query.includes('gdp')) return ['province', 'year', 'gdp'];
    return ['timestamp', 'value'];
  }
  if (taskType === 'regression') return ['feature', 'target'];
  if (taskType === 'video_classification') return ['video', 'label'];
  if (taskType === 'nlp') return ['text', 'label'];
  return [];
}

function estimateExternalTCV(dataset, taskDesc, taskType, requiredCols) {
  const cols = requiredCols && requiredCols.length > 0
    ? requiredCols
    : inferRequiredColumns(taskDesc, taskType);
  const text = `${dataset.title} ${dataset.description || ''}`;
  const similarity = lexicalSimilarity(taskDesc, text);
  const rankPrior = dataset.rankScore ?? 50;
  const seederScore = normalizeLog(dataset.seeders, 500);
  const sizeScore = normalizeLog(dataset.schema.sizeBytes, 1024 * 1024 * 1024);
  const rowScore = normalizeLog(dataset.schema.rows, 100000);

  const labelHint = /(label|class|category|target|classification)/i.test(text);
  const timeHint = /(time series|forecast|prediction|gdp|economic|economy|year|month|date|quarter)/i.test(text);
  const entityHint = tokenize(taskDesc).some(token => text.toLowerCase().includes(token));

  let schemaFit = Math.max(15, 0.6 * rankPrior + 0.4 * similarity);
  let temporalFit = 50;

  if (taskType === 'classification' || taskType === 'video_classification' || taskType === 'nlp') {
    if (labelHint) schemaFit += 15;
    temporalFit = 50;
  } else if (taskType === 'time_series_prediction') {
    if (timeHint) {
      schemaFit += 10;
      temporalFit = 75;
    } else {
      temporalFit = 35;
    }
  } else if (taskType === 'regression') {
    schemaFit += dataset.schema.columns.length > 0 ? 10 : 0;
  }

  if (cols.length > 0 && dataset.schema.columns.length > 0) {
    schemaFit += 10;
  }
  if (entityHint) {
    schemaFit += 5;
  }

  const infoGain = Math.min(100, 0.5 * rankPrior + 0.3 * similarity + 0.2 * sizeScore);
  const quality = Math.min(100, 0.45 * sizeScore + 0.20 * rowScore + 0.35 * seederScore);
  const community = dataset.community.reviews > 0
    ? Math.min(100, dataset.community.positive_rate * 80 + dataset.community.reviews * 2)
    : Math.max(20, seederScore * 0.8);
  const risk = dataset.community.negative_rate > 0 ? dataset.community.negative_rate * 100 : 0;

  return {
    schema_fit: Math.min(100, schemaFit),
    temporal_fit: Math.min(100, temporalFit),
    info_gain: Math.min(100, infoGain),
    quality: Math.min(100, quality),
    community: Math.min(100, community),
    risk: Math.min(100, risk),
  };
}

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
    const params = { query, filters, limit: 10 };
    if (taskType) params.task_type = taskType;
    const data = await this.callTool('dataset_search', params);

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
        sourceLabel: prettySourceName(r.source ? r.source.toLowerCase().replace(/\s/g,'') : 'p2p'),
        dataType: r.data_type || 'tabular',
        schema: {
          columns: Array.from({ length: r.schema?.columns || 0 }, (_, i) => `col_${i}`),
          rows: r.schema?.rows || 0,
          sizeBytes: r.schema?.size_bytes || 0,
          size: formatBytes(r.schema?.size_bytes || 0),
        },
        price: typeof r.price === 'object' ? r.price.amount : (r.price || 0),
        rankScore: parseFloat(r.rank_score) || 0,
        seeders: parseSeeders(r.description),
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
      if (!sourceFilter && sources.length === 1 && sources[0] === 'bittorrent') {
        this.log(
          '[i]',
          'Only BitTorrent returned results. Kaggle/HuggingFace need credentials, P2P needs locally indexed metadata, and the remaining adapters currently return empty results.',
        );
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
    const effectiveRequiredCols = requiredCols && requiredCols.length > 0
      ? requiredCols
      : inferRequiredColumns(taskDesc, taskType);
    for (const d of this.datasets) {
      let tcvComps = d.tcv;
      let tcvScore;

      if (!tcvComps && d.source === 'p2p') {
        // Call backend for local P2P datasets (they exist in store)
        const evalParams = {
          cid: d.cid,
          task_description: taskDesc,
          task_type: taskType,
          required_columns: effectiveRequiredCols,
          budget: 10,
        };
        if (taskType) evalParams.task_type = taskType;
        const evalData = await this.callTool('dataset_evaluate', evalParams);
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
        tcvComps = d.tcv || estimateExternalTCV(d, taskDesc, taskType, effectiveRequiredCols);
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
