export type CandidateId =
  | 'safehat-premium'
  | 'kaggle-construction'
  | 'warehouse-ppe'
  | 'bt-safetyframes'
  | 'roboflow-hardhat'

export type Candidate = {
  id: CandidateId
  name: string
  source: 'decentralized' | 'public' | 'torrent'
  cost: string
  meta: string
  score: number
  roi: string
  confidence: string
  accuracy: string
  radar: [number, number, number, number, number]
  bars: number[]
  metrics: Array<{ label: string; value: number; color: string }>
  fitTags: string[]
  reasons: string[]
  logs: string[]
  steps: Array<{ label: string; status: 'done' | 'active' | 'idle' }>
  outcome: {
    baseline: string
    selected: string
    gain: string
  }
  platform: string
  dataType: 'image' | 'tabular'
  size: string
  reviewCount: number
}

export type MarketReview = {
  id: string
  reviewer_address: string
  content: string
  source: string
  tx_hash: string | null
  created_at: string
}

export type WorkflowNode = {
  id: string
  badge: string
  title: string
  subtitle: string
  accent: 'blue' | 'indigo' | 'cyan' | 'purple' | 'green' | 'teal' | 'amber'
  entry?: boolean
  terminal?: boolean
  position: { x: number; y: number }
  size: { w: number; h: number }
  lifecycle: {
    showAt: number
    doneAt: number
  }
  statusText: {
    running: string
    done: string
  }
  content:
    | { kind: 'query'; query: string; sources: string[] }
    | { kind: 'intent'; taskDescription: string; budget: string; keywords: string[] }
    | {
        kind: 'code'
        files: Array<{ path: string; file: string; addedLines: number }>
        filesChanged: number
        addedLines: number
        removedLines?: number
      }
    | { kind: 'search'; totalResults: number; candidateCount: number }
    | { kind: 'valuation'; selected: string; action: string; detail: string }
    | { kind: 'execution'; stage: string; accuracy: string; loss: string }
    | { kind: 'ledger'; items: string[] }
}

export type WorkflowEdge = {
  from: WorkflowNode['id']
  to: WorkflowNode['id']
  label: string
  kind?: 'primary' | 'branch' | 'feedback'
}

export type PlanningSourceId = 'kaggle' | 'huggingface' | 'guixu-hub'

export const planningSourceOptions = [
  { id: 'kaggle', label: 'Kaggle' },
  { id: 'huggingface', label: 'HuggingFace' },
  { id: 'guixu-hub', label: 'Guixu Hub' },
] as const satisfies ReadonlyArray<{ id: PlanningSourceId; label: string }>

const defaultSources: PlanningSourceId[] = ['kaggle', 'huggingface', 'guixu-hub']

const preferredCandidate = (sources: PlanningSourceId[]) => {
  if (sources.includes('guixu-hub'))
    return { id: 'safehat-premium' as CandidateId, name: 'SafeHat_Premium' }
  if (sources.includes('huggingface'))
    return { id: 'warehouse-ppe' as CandidateId, name: 'HF_HelmetScenes' }
  return { id: 'kaggle-construction' as CandidateId, name: 'Kaggle_Construction' }
}

export const getExecutionSummary = (candidateId: CandidateId) => {
  switch (candidateId) {
    case 'safehat-premium':
      return { accuracy: '96.4%', loss: '0.02' }
    case 'warehouse-ppe':
      return { accuracy: '93.1%', loss: '0.05' }
    case 'bt-safetyframes':
      return { accuracy: '90.4%', loss: '0.07' }
    case 'roboflow-hardhat':
      return { accuracy: '88.9%', loss: '0.09' }
    case 'kaggle-construction':
      return { accuracy: '89.7%', loss: '0.08' }
  }
}

export const buildPlanningWorkflow = (query: string, sources: PlanningSourceId[], presetIndex = 0) => {
  const activeSources = sources.length ? sources : defaultSources
  const candidate = preferredCandidate(activeSources)
  const preset = presetIndex === 0
    ? {
        taskDescription: 'Build an image classifier to detect the presence of the user\'s cat in photos captured by a house monitor.',
        keywords: ['cat', 'image'],
        budget: '$0',
        totalResults: 47,
        candidateCount: 10,
        valuationSelected: 'HF_NightVision_Cats',
        valuationAction: '3-dataset bundle',
        valuationDetail: 'budget: Free',
        codeFiles: [
          { path: 'cat-classification/train_cat.py', file: 'train_cat.py', addedLines: 93 },
          { path: 'cat-classification/prepare.py', file: 'prepare.py', addedLines: 304 },
        ],
      }
    : {
        taskDescription: 'Train an image classifier to determine whether workers in construction site images are wearing safety helmets correctly.',
        keywords: ['safety helmet', 'worker'],
        budget: '$2.00',
        totalResults: 22,
        candidateCount: 10,
        valuationSelected: 'SafeHat_Premium',
        valuationAction: '2-dataset bundle',
        valuationDetail: 'budget: $1.10',
        codeFiles: [
          { path: 'safetyhelmet-classification/train_helmet.py', file: 'train_helmet.py', addedLines: 118 },
          { path: 'safetyhelmet-classification/prepare.py', file: 'prepare.py', addedLines: 348 },
          { path: 'safetyhelmet-classification/export_queue.py', file: 'export_queue.py', addedLines: 361 },
        ],
      }
  const hasGuixuHub = activeSources.includes('guixu-hub')
  const { totalResults, candidateCount } = preset
  const executionSummary = getExecutionSummary(candidate.id)

  const nodes: WorkflowNode[] = [
    {
      id: 'parser',
      badge: '1',
      title: 'Semantic Query Parser',
      subtitle: 'structured task profile',
      accent: 'indigo',
      position: { x: 0.02, y: 0.06 },
      size: { w: 188, h: 144 },
      lifecycle: { showAt: 0, doneAt: 2 },
      statusText: { running: 'parsing', done: 'parsed' },
      content: {
        kind: 'intent',
        taskDescription: preset.taskDescription,
        budget: preset.budget,
        keywords: preset.keywords,
      },
    },
    {
      id: 'search',
      badge: '2',
      title: 'Data Search',
      subtitle: 'coarse shortlist',
      accent: 'green',
      position: { x: 0.36, y: 0.06 },
      size: { w: 188, h: 158 },
      lifecycle: { showAt: 2, doneAt: 6 },
      statusText: { running: 'searching', done: `${candidateCount} candidates` },
      content: {
        kind: 'search',
        totalResults,
        candidateCount,
      },
    },
    {
      id: 'valuation',
      badge: '4',
      title: 'Data Valuation',
      subtitle: 'selection core',
      accent: 'cyan',
      position: { x: 0.7, y: 0.06 },
      size: { w: 188, h: 154 },
      lifecycle: { showAt: 6, doneAt: 9 },
      statusText: { running: 'valuating', done: 'selected' },
      content: {
        kind: 'valuation',
        selected: preset.valuationSelected,
        action: preset.valuationAction,
        detail: preset.valuationDetail,
      },
    },
    {
      id: 'code',
      badge: '3',
      title: 'Code Generator',
      subtitle: 'training script',
      accent: 'purple',
      position: { x: 0.02, y: 0.62 },
      size: { w: 188, h: 128 },
      lifecycle: { showAt: 3, doneAt: 6 },
      statusText: { running: 'writing', done: 'ready' },
      content: {
        kind: 'code',
        files: preset.codeFiles,
        filesChanged: preset.codeFiles.length,
        addedLines: preset.codeFiles.reduce((sum, f) => sum + f.addedLines, 0),
      },
    },
    {
      id: 'execution',
      badge: hasGuixuHub ? '6' : '5',
      title: 'Task Execution',
      subtitle: 'training run',
      accent: 'teal',
      terminal: true,
      position: { x: 0.36, y: 0.62 },
      size: { w: 188, h: 112 },
      lifecycle: hasGuixuHub ? { showAt: 12, doneAt: 15 } : { showAt: 9, doneAt: 12 },
      statusText: { running: 'training', done: 'completed' },
      content: {
        kind: 'execution',
        stage: 'epoch 20/20',
        accuracy: executionSummary.accuracy,
        loss: executionSummary.loss,
      },
    },
  ]

  if (hasGuixuHub) {
    nodes.push({
      id: 'purchase',
      badge: '5',
      title: 'Agentic Purchase',
      subtitle: 'x402 + unlock',
      accent: 'amber',
      position: { x: 0.7, y: 0.62 },
      size: { w: 188, h: 144 },
      lifecycle: { showAt: 9, doneAt: 12 },
      statusText: { running: 'purchasing', done: 'unlocked' },
      content: {
        kind: 'ledger',
        items: ['x402 payment settled', '3/5 key shares released'],
      },
    })
  }

  const edges: WorkflowEdge[] = [
    { from: 'parser', to: 'search', label: 'keywords', kind: 'branch' },
    { from: 'parser', to: 'code', label: 'task description', kind: 'branch' },
    { from: 'code', to: 'valuation', label: 'training code' },
    { from: 'search', to: 'valuation', label: 'candidate datasets' },
  ]

  if (hasGuixuHub) {
    edges.push(
      { from: 'valuation', to: 'purchase', label: 'selected asset' },
      { from: 'purchase', to: 'execution', label: 'verified data' },
    )
  } else {
    edges.push({ from: 'valuation', to: 'execution', label: 'selected asset' })
  }

  return {
    nodes,
    edges,
    recommendedCandidateId: candidate.id,
    maxStep: hasGuixuHub ? 15 : 12,
  }
}

export const candidates: Record<CandidateId, Candidate> = {
  'safehat-premium': {
    id: 'safehat-premium',
    name: 'SafeHat_Premium',
    source: 'decentralized',
    cost: '$1.10',
    meta: 'decentralized · $1.10 · strong labels',
    score: 92,
    roi: 'ROI: +12% expected mAP lift',
    confidence: 'recommended',
    accuracy: '81.2% mAP',
    radar: [84, 90, 76, 70, 88],
    bars: [6, 9, 11, 14, 15, 18, 19, 24],
    metrics: [
      { label: 'Code Compatibility', value: 89, color: '#168078' },
      { label: 'Annotation Quality', value: 90, color: '#2b8ab6' },
      { label: 'Task Relevance', value: 84, color: '#36bffa' },
      { label: 'Diversity Gain', value: 70, color: '#5f9b73' },
      { label: 'Fast-DataShapley Value', value: 78, color: '#d6a23c' },
      { label: 'On-chain Reputation', value: 86, color: '#296dff' },
    ],
    fitTags: ['bbox-ready', 'construction scenes', 'high label quality'],
    reasons: [
      'Matches the generated detection code and expected annotation format.',
      'Has stronger label quality than public free alternatives.',
      'On-chain trade and feedback history reduces selection risk.',
    ],
    logs: [
      'Generate training code ... done',
      'Bind shortlisted dataset schema ... done',
      'Run valuation model ... SafeHat_Premium selected',
      'Start training job ... epoch 20/20  mAP=0.81',
    ],
    steps: [
      { label: 'Valuation input binding', status: 'done' },
      { label: 'Dataset scoring', status: 'done' },
      { label: 'Training execution', status: 'active' },
      { label: 'Feedback recording', status: 'idle' },
    ],
    outcome: {
      baseline: '72.4% mAP',
      selected: '81.2% mAP',
      gain: '+8.8 pts',
    },
    platform: 'guixu-hub',
    dataType: 'image',
    size: '2.3 GB',
    reviewCount: 32,
  },
  'kaggle-construction': {
    id: 'kaggle-construction',
    name: 'Kaggle_Construction',
    source: 'public',
    cost: 'Free',
    meta: 'public · free · broad coverage',
    score: 76,
    roi: 'ROI: +4% expected mAP lift',
    confidence: 'usable',
    accuracy: '74.8% mAP',
    radar: [64, 68, 74, 56, 71],
    bars: [4, 6, 7, 8, 10, 11, 11, 13],
    metrics: [
      { label: 'Code Compatibility', value: 66, color: '#168078' },
      { label: 'Annotation Quality', value: 68, color: '#2b8ab6' },
      { label: 'Task Relevance', value: 74, color: '#36bffa' },
      { label: 'Diversity Gain', value: 56, color: '#5f9b73' },
      { label: 'Fast-DataShapley Value', value: 60, color: '#d6a23c' },
      { label: 'On-chain Reputation', value: 38, color: '#296dff' },
    ],
    fitTags: ['public data', 'lower trust signal', 'acceptable schema'],
    reasons: [
      'Low cost makes it a reasonable baseline candidate.',
      'Coverage is broad, but annotation quality is weaker.',
      'No decentralized trade memory to improve confidence.',
    ],
    logs: [
      'Generate training code ... done',
      'Bind shortlisted dataset schema ... done',
      'Run valuation model ... ranked below decentralized asset',
      'Training result estimate ... mAP 0.75',
    ],
    steps: [
      { label: 'Valuation input binding', status: 'done' },
      { label: 'Dataset scoring', status: 'active' },
      { label: 'Training execution', status: 'idle' },
      { label: 'Feedback recording', status: 'idle' },
    ],
    outcome: {
      baseline: '72.4% mAP',
      selected: '74.8% mAP',
      gain: '+2.4 pts',
    },
    platform: 'kaggle',
    dataType: 'image',
    size: '5.1 GB',
    reviewCount: 14,
  },
  'warehouse-ppe': {
    id: 'warehouse-ppe',
    name: 'HF_HelmetScenes',
    source: 'public',
    cost: 'Free',
    meta: 'huggingface · free · curated labels',
    score: 83,
    roi: 'ROI: +8% expected mAP lift',
    confidence: 'promising',
    accuracy: '78.9% mAP',
    radar: [72, 79, 66, 61, 81],
    bars: [5, 7, 9, 11, 12, 14, 15, 18],
    metrics: [
      { label: 'Code Compatibility', value: 74, color: '#168078' },
      { label: 'Annotation Quality', value: 79, color: '#2b8ab6' },
      { label: 'Task Relevance', value: 66, color: '#36bffa' },
      { label: 'Diversity Gain', value: 61, color: '#5f9b73' },
      { label: 'Fast-DataShapley Value', value: 69, color: '#d6a23c' },
      { label: 'On-chain Reputation', value: 74, color: '#296dff' },
    ],
    fitTags: ['huggingface source', 'free access', 'moderate quality'],
    reasons: [
      'Provides a curated public-source alternative from HuggingFace.',
      'Quality is acceptable, but label consistency is below SafeHat_Premium.',
      'Useful fallback when decentralized assets are not selected.',
    ],
    logs: [
      'Generate training code ... done',
      'Bind shortlisted dataset schema ... done',
      'Run valuation model ... second-best candidate',
      'Training result estimate ... mAP 0.79',
    ],
    steps: [
      { label: 'Valuation input binding', status: 'done' },
      { label: 'Dataset scoring', status: 'done' },
      { label: 'Training execution', status: 'idle' },
      { label: 'Feedback recording', status: 'idle' },
    ],
    outcome: {
      baseline: '72.4% mAP',
      selected: '78.9% mAP',
      gain: '+6.5 pts',
    },
    platform: 'huggingface',
    dataType: 'image',
    size: '3.8 GB',
    reviewCount: 8,
  },
  'bt-safetyframes': {
    id: 'bt-safetyframes',
    name: 'BT_SafetyFrames',
    source: 'torrent',
    cost: 'Free',
    meta: 'torrent · free · unclear provenance',
    score: 61,
    roi: 'ROI: uncertain',
    confidence: 'risky',
    accuracy: '71.0% mAP',
    radar: [48, 51, 63, 72, 55],
    bars: [3, 4, 5, 7, 8, 8, 9, 10],
    metrics: [
      { label: 'Code Compatibility', value: 48, color: '#168078' },
      { label: 'Annotation Quality', value: 51, color: '#2b8ab6' },
      { label: 'Task Relevance', value: 63, color: '#36bffa' },
      { label: 'Diversity Gain', value: 72, color: '#5f9b73' },
      { label: 'Fast-DataShapley Value', value: 46, color: '#d6a23c' },
      { label: 'On-chain Reputation', value: 18, color: '#296dff' },
    ],
    fitTags: ['free source', 'high uncertainty', 'poor trust'],
    reasons: [
      'May increase diversity, but provenance is weak.',
      'No credible feedback history to support valuation.',
      'High execution risk makes it unsuitable for the final pick.',
    ],
    logs: [
      'Generate training code ... done',
      'Torrent metadata found ...',
      'Provenance warning: attestation missing',
      'Training result estimate ... unstable outcome',
    ],
    steps: [
      { label: 'Valuation input binding', status: 'done' },
      { label: 'Dataset scoring', status: 'active' },
      { label: 'Training execution', status: 'idle' },
      { label: 'Feedback recording', status: 'idle' },
    ],
    outcome: {
      baseline: '72.4% mAP',
      selected: '71.0% mAP',
      gain: '-1.4 pts',
    },
    platform: 'torrent',
    dataType: 'image',
    size: '1.2 GB',
    reviewCount: 2,
  },
  'roboflow-hardhat': {
    id: 'roboflow-hardhat',
    name: 'Roboflow_HardHat',
    source: 'public',
    cost: 'Free',
    meta: 'roboflow · free · moderate coverage',
    score: 71,
    roi: 'ROI: +3% expected mAP lift',
    confidence: 'moderate',
    accuracy: '73.5% mAP',
    radar: [58, 62, 68, 54, 45],
    bars: [3, 5, 6, 7, 8, 9, 10, 12],
    metrics: [
      { label: 'Code Compatibility', value: 58, color: '#168078' },
      { label: 'Annotation Quality', value: 62, color: '#2b8ab6' },
      { label: 'Task Relevance', value: 68, color: '#36bffa' },
      { label: 'Diversity Gain', value: 54, color: '#5f9b73' },
      { label: 'Fast-DataShapley Value', value: 49, color: '#d6a23c' },
      { label: 'On-chain Reputation', value: 28, color: '#296dff' },
    ],
    fitTags: ['roboflow source', 'free access', 'basic labels'],
    reasons: [
      'Provides a basic public-source dataset from Roboflow.',
      'Label quality is acceptable for initial prototyping.',
      'Limited community feedback and no on-chain attestation.',
    ],
    logs: [
      'Generate training code ... done',
      'Bind shortlisted dataset schema ... done',
      'Run valuation model ... ranked below top candidates',
      'Training result estimate ... mAP 0.735',
    ],
    steps: [
      { label: 'Valuation input binding', status: 'done' },
      { label: 'Dataset scoring', status: 'active' },
      { label: 'Training execution', status: 'idle' },
      { label: 'Feedback recording', status: 'idle' },
    ],
    outcome: {
      baseline: '72.4% mAP',
      selected: '73.5% mAP',
      gain: '+1.1 pts',
    },
    platform: 'roboflow',
    dataType: 'image',
    size: '0.8 GB',
    reviewCount: 6,
  },
}

export const marketRows = [
  { seller: 'did:example:123', cid: 'bafybe...', dataset: 'SafeHat_Premium', price: '$1.10', rating: '4.8 (32)' },
  { seller: 'did:example:723', cid: 'bafyab...', dataset: 'Warehouse_PPE_Set', price: '$7', rating: '4.3 (18)' },
  { seller: 'did:example:552', cid: 'bafybq...', dataset: 'Construction_Images_v2', price: '$5', rating: '4.1 (20)' },
  { seller: 'did:example:884', cid: 'bafycc...', dataset: 'HelmetAndVest', price: '$6', rating: '4.5 (11)' },
] as const

export const transactionRows = [
  { time: '10:30', event: 'Dataset Purchase', hash: '0x91de...13c0', detail: 'agent purchased SafeHat_Premium' },
  { time: '10:26', event: 'Dataset Publish', hash: '0x81af...be42', detail: 'seller registered SafeHat_Premium' },
  { time: '10:43', event: 'Feedback Attest', hash: '0xa4cf...991e', detail: 'positive task-fit feedback recorded' },
  { time: '09:58', event: 'Dataset Purchase', hash: '0x72bc...881f', detail: 'older PPE detector training run' },
] as const

export const reviews = [
  {
    key: 'recent',
    title: 'SafeHat_Premium',
    subtitle: 'Recent dataset',
    text: 'High annotation quality, strong construction-scene coverage, and stable improvement for helmet detection.',
    accent: 'default',
    stars: '★★★★★',
  },
  {
    key: 'agent',
    title: 'Agent review',
    subtitle: 'Task-fit memory',
    text: 'This asset improved training stability under a tight budget and became the best candidate in valuation.',
    accent: 'agent',
    stars: '★★★★☆',
  },
  {
    key: 'human',
    title: 'Human review',
    subtitle: 'Seller reputation',
    text: 'Metadata and on-chain purchase history look consistent. Label quality is stronger than public free alternatives.',
    accent: 'human',
    stars: '★★★★★',
  },
] as const
