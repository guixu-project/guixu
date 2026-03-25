// ============================================================
// Guixu Demo — Simulation Engine
// Connects to real MCP server (data-node mcp --mode http)
// Falls back to mock data if server is unavailable
// ============================================================

const RPC_URL = '/rpc';

class GuixuEngine {
  constructor() {
    this.mode = 'base-x402';
    this.ledger = [];
    this.totalCost = 0;
    this.datasets = [];
    this.selectedDataset = null;
    this.logs = [];
    this.live = false; // true if connected to real server
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
  async rpc(method, params) {
    try {
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), 5000);
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
      return null; // fallback to mock
    }
  }

  async callTool(name, args) {
    const result = await this.rpc('tools/call', { name, arguments: args });
    if (result && result.content && result.content[0]) {
      try { return JSON.parse(result.content[0].text); } catch { return result.content[0].text; }
    }
    return null;
  }

  // Probe server on first run
  async init() {
    const res = await this.rpc('initialize', {});
    this.live = !!res;
    this.log('[i]', this.live ? 'Connected to MCP server (live data)' : 'Offline mode (mock data)');
  }

  // Step 2: Search
  async search(query, taskType, sourceFilter) {
    this.log('[S]', `dataset_search("${query}"${sourceFilter ? `, source=${sourceFilter}` : ''})`);

    if (this.live) {
      const filters = {};
      if (sourceFilter) filters.source = sourceFilter;
      const data = await this.callTool('dataset_search', { query, filters, limit: 10 });
      if (data && Array.isArray(data) && data.length > 0) {
        this.datasets = data.map(r => ({
          cid: r.cid,
          title: r.title,
          description: r.description,
          source: r.source ? r.source.toLowerCase().replace(/\s/g,'') : 'p2p',
          sourceLabel: r.source || 'P2P',
          schema: {
            columns: Array.from({ length: r.schema?.columns || 0 }, (_, i) => `col_${i}`),
            rows: r.schema?.rows || 0,
            size: formatBytes(r.schema?.size_bytes || 0),
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
        this.log('[S]', `Live: ${this.datasets.length} results from ${sources.length} sources`);
        this.addLedger('search', `query="${query}" > ${this.datasets.length} results`);
        return { datasets: this.datasets, sources };
      }
    }

    // Fallback to mock
    let key = 'gdp-prediction';
    if (taskType === 'classification') key = 'classification';
    this.datasets = DATASETS[key] || DATASETS['gdp-prediction'];
    if (sourceFilter) {
      this.datasets = this.datasets.filter(d => d.source === sourceFilter);
    }
    const sources = [...new Set(this.datasets.map(d => d.source))];
    this.log('[S]', `Mock: ${this.datasets.length} results from ${sources.length} sources`);
    this.addLedger('search', `query="${query}" > ${this.datasets.length} results`);
    return { datasets: this.datasets, sources };
  }

  // Step 3: Evaluate
  async evaluate(taskDesc, taskType, requiredCols) {
    this.log('[E]', `dataset_evaluate() x ${this.datasets.length}`);

    const results = [];
    for (const d of this.datasets) {
      let tcvComps = d.tcv;
      let tcvScore;

      if (this.live && !tcvComps && d.source === 'p2p') {
        // Only call backend for local P2P datasets (they exist in store)
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

    if (this.live) {
      const data = await this.callTool('dataset_purchase', { cid: dataset.cid, max_price: 10 });
      if (data && data.status === 'purchased') {
        const paid = data.price_paid || 0;
        const gasCost = m.gasCost;
        this.totalCost += paid + gasCost;
        this.log('[P]', `Live: ${data.payment_protocol} $${paid}`);
        this.log('[D]', `Delivery: ${data.delivery?.method || 'local'}`);
        const txId = data.tx_id || randomHash();
        this.addLedger('purchase', `${dataset.title} — $${(paid + gasCost).toFixed(4)}`, { txId });
        return {
          pay: { protocol: data.payment_protocol, desc: data.protocol_description },
          gasCost, totalPaid: paid + gasCost, txId,
          delivery: data.delivery?.method || 'local',
        };
      }
    }

    // Fallback
    const pay = selectPaymentProtocol(dataset.price, this.mode);
    const gasCost = dataset.price > 0 ? m.gasCost : 0;
    const totalPaid = dataset.price + gasCost;
    this.totalCost += totalPaid;
    this.log('[P]', `${pay.protocol}: $${dataset.price.toFixed(4)} + gas $${gasCost.toFixed(4)}`);
    this.log('[D]', `Delivery: ${dataset.schema.size} via ${dataset.source === 'p2p' ? 'BitTorrent v2' : 'HTTPS'}`);
    const txId = randomHash();
    this.addLedger('purchase', `${dataset.title} — $${totalPaid.toFixed(4)}`, { txId });
    return { pay, gasCost, totalPaid, txId, delivery: dataset.source === 'p2p' ? 'BitTorrent v2' : 'HTTPS CDN' };
  }

  // BT Download — for BitTorrent sourced datasets
  async btDownload(dataset) {
    this.log('[B]', `dataset_bt_download("${dataset.cid}")`);

    if (this.live) {
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
    }

    // Mock fallback
    this.log('[B]', `BT download: ${dataset.schema.size} via DHT swarm`);
    this.addLedger('download', `BT download: ${dataset.title}`, { txId: dataset.cid });
    return {
      pay: { protocol: 'BitTorrent', desc: 'Free P2P download via BT DHT' },
      gasCost: 0, totalPaid: 0, txId: dataset.cid || randomHash(),
      delivery: `BitTorrent DHT · ${dataset.schema.size}`,
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

    if (this.live) {
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
      this.log('[F]', `Live feedback recorded`);
    } else {
      this.log('[F]', `dataset_feedback(${fb.assessment}): relevance=${fb.relevance}`);
    }

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
