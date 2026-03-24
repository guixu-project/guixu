// ============================================================
// Guixu Demo — Mock Data (mirrors real Rust crate structures)
// ============================================================

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

// Mock datasets — mirrors DatasetMetadata + SearchResult from crates/core
const DATASETS = {
  'gdp-prediction': [
    {
      cid: 'bafk...china_gdp_2020_2025',
      title: 'China Provincial GDP (2020-2025)',
      description: 'Guangdong, Jiangsu, Zhejiang — 6-year GDP, growth rate, population',
      source: 'p2p', sourceLabel: 'P2P',
      schema: { columns: ['province','year','gdp_billion_cny','growth_rate','population_million'], rows: 18, size: '2.1 KB' },
      price: 0, license: 'CC-BY-4.0',
      provider: 'did:key:z6Mk...local',
      // TCV components (pre-computed to match Rust TcvEngine logic)
      tcv: { schema_fit: 92, temporal_fit: 85, info_gain: 100, quality: 78, community: 65, risk: 5 },
      community: { reviews: 12, avg_relevance: 0.88, positive_rate: 0.92, negative_rate: 0.08, task_signals: [{ task_type: 'time_series_prediction', count: 8, avg_relevance: 0.91, success_rate: 0.88 }] },
    },
    {
      cid: 'kaggle:sudalairajkumar/world-gdp-data',
      title: 'World GDP Data (1960-2025)',
      description: 'GDP data for all countries from World Bank',
      source: 'kaggle', sourceLabel: 'Kaggle',
      schema: { columns: ['country','year','gdp'], rows: 15000, size: '2.0 MB' },
      price: 0, license: 'CC-BY-4.0',
      provider: 'did:kaggle:sudalairajkumar',
      tcv: { schema_fit: 68, temporal_fit: 70, info_gain: 75, quality: 72, community: 50, risk: 0 },
      community: { reviews: 0, avg_relevance: 0, positive_rate: 0, negative_rate: 0, task_signals: [] },
    },
    {
      cid: 'hf:worldbank/global-economic-indicators',
      title: 'Global Economic Indicators',
      description: 'Comprehensive economic indicators from World Bank',
      source: 'huggingface', sourceLabel: 'HuggingFace',
      schema: { columns: ['country_code','indicator','year','value'], rows: 500000, size: '50 MB' },
      price: 0, license: 'CC-BY-4.0',
      provider: 'did:hf:worldbank',
      tcv: { schema_fit: 55, temporal_fit: 65, info_gain: 80, quality: 75, community: 50, risk: 0 },
      community: { reviews: 0, avg_relevance: 0, positive_rate: 0, negative_rate: 0, task_signals: [] },
    },
    {
      cid: 'bafk...random_noise_data',
      title: 'Random Noise Data',
      description: 'Irrelevant random numerical data',
      source: 'p2p', sourceLabel: 'P2P',
      schema: { columns: ['id','random_value','noise_level','category'], rows: 10, size: '0.3 KB' },
      price: 0, license: 'MIT',
      provider: 'did:key:z6Mk...noise',
      tcv: { schema_fit: 5, temporal_fit: 30, info_gain: 10, quality: 45, community: 20, risk: 60 },
      community: { reviews: 5, avg_relevance: -0.7, positive_rate: 0.0, negative_rate: 0.8, task_signals: [{ task_type: 'time_series_prediction', count: 3, avg_relevance: -0.8, success_rate: 0.0 }] },
    },
    {
      cid: 'bafk...china_weather_2024',
      title: 'China Weather Data (2024)',
      description: 'Major city temperature, humidity, rainfall',
      source: 'p2p', sourceLabel: 'P2P',
      schema: { columns: ['city','date','temperature_c','humidity_pct','rainfall_mm'], rows: 8, size: '0.5 KB' },
      price: 0.005, license: 'CC-BY-4.0',
      provider: 'did:key:z6Mk...weather',
      tcv: { schema_fit: 25, temporal_fit: 60, info_gain: 55, quality: 62, community: 50, risk: 0 },
      community: { reviews: 2, avg_relevance: 0.3, positive_rate: 0.5, negative_rate: 0.0, task_signals: [] },
    },
  ],
  'classification': [
    {
      cid: 'kaggle:nih-chest-xrays/data',
      title: 'NIH Chest X-rays',
      description: '112,120 X-ray images with disease labels',
      source: 'kaggle', sourceLabel: 'Kaggle',
      schema: { columns: ['image_path','finding_labels','patient_id'], rows: 112120, size: '45 GB' },
      price: 0, license: 'CC0-1.0',
      provider: 'did:kaggle:nih',
      tcv: { schema_fit: 85, temporal_fit: 50, info_gain: 95, quality: 88, community: 72, risk: 2 },
      community: { reviews: 47, avg_relevance: 0.82, positive_rate: 0.89, negative_rate: 0.04, task_signals: [{ task_type: 'classification', count: 35, avg_relevance: 0.85, success_rate: 0.91 }] },
    },
  ],
};

// TCV weights — mirrors crates/valuation/src/tcv.rs constants
const TCV_WEIGHTS = {
  schema_fit: { weight: 0.25, label: 'SchemaFit', color: '#3b82f6', symbol: 'α' },
  temporal_fit: { weight: 0.15, label: 'TemporalFit', color: '#06b6d4', symbol: 'β' },
  info_gain: { weight: 0.15, label: 'InfoGain', color: '#22c55e', symbol: 'γ' },
  quality: { weight: 0.10, label: 'Quality', color: '#eab308', symbol: 'δ' },
  community: { weight: 0.15, label: 'Community', color: '#a855f7', symbol: 'ε' },
  risk: { weight: -0.20, label: 'RiskPenalty', color: '#ef4444', symbol: 'ζ' },
};

function computeTCV(components) {
  const raw = TCV_WEIGHTS.schema_fit.weight * components.schema_fit
    + TCV_WEIGHTS.temporal_fit.weight * components.temporal_fit
    + TCV_WEIGHTS.info_gain.weight * components.info_gain
    + TCV_WEIGHTS.quality.weight * components.quality
    + TCV_WEIGHTS.community.weight * components.community
    + TCV_WEIGHTS.risk.weight * components.risk;
  return Math.max(-100, Math.min(100, raw));
}

function tcvVerdict(score) {
  if (score > 60) return { label: 'StrongPositive', cls: 'score-strong-pos', text: 'Strongly Recommended' };
  if (score > 30) return { label: 'Positive', cls: 'score-pos', text: 'Recommended' };
  if (score > 0) return { label: 'Neutral', cls: 'score-neutral', text: 'Marginal' };
  if (score > -30) return { label: 'Negative', cls: 'score-neg', text: 'Not Recommended' };
  return { label: 'StrongNegative', cls: 'score-strong-neg', text: 'Harmful' };
}

function selectPaymentProtocol(price, mode) {
  if (price === 0) return { protocol: 'none', desc: 'Free dataset — no payment required' };
  const m = MODES[mode];
  if (m.protocol === 'x402') return { protocol: 'x402', desc: `Micropayment via x402 (${m.token} on ${m.chain})` };
  if (m.protocol === 'Stripe MPP') return { protocol: 'Stripe MPP', desc: `Session payment via Stripe MPP (${m.token})` };
  return { protocol: 'ERC-8183 Escrow', desc: `Escrowed payment via ERC-8183 (verify then release)` };
}

function randomHash() {
  return '0x' + Array.from({length: 64}, () => Math.floor(Math.random()*16).toString(16)).join('');
}

function shortHash(h) { return h.slice(0, 10) + '...' + h.slice(-6); }
