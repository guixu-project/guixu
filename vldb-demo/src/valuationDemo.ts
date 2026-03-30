import type { CandidateId } from './data'

export type ValuationSearchCandidate = {
  key: string
  candidateId?: CandidateId
  name: string
  platform: 'guixu-hub' | 'huggingface' | 'kaggle' | 'roboflow' | 'torrent'
  dataType: 'image'
  size: string
  cost: string
  reviewCount: number
  coarseScore: number
  sampleScore: number
  finalScore: number
  route: 'seed' | 'propagated' | 'rejudged'
  searchRank: number
  sampledRecords: number
  seedRecords: number
  highAnchors: number
  lowAnchors: number
  highBoundScore: number
  lowBoundScore: number
  propagatedRecords: number
  rescoredRecords: number
}

export type BundleCard = {
  name: string
  role: string
  utility: string
}

export type KnapsackDisplay = {
  constraints: Array<{ label: string, value: string }>
  rounds: Array<{
    label: string
    state: 'infeasible' | 'feasible'
    placeholder?: string
    rows: Array<{
      name: string
      score: number
      price: string
      size: string
      chosen?: boolean
    }>
  }>
  selected: {
    datasets: Array<{ name: string, score: number }>
    totalPrice: string
    totalSize: string
    totalScore: number
  }
}

export const valuationSearchCandidates: ValuationSearchCandidate[] = [
  {
    key: 'sitecam-helmet-longtail',
    name: 'SiteCam_Helmet_Longtail',
    platform: 'guixu-hub',
    dataType: 'image',
    size: '1.8 GB',
    cost: '$6',
    reviewCount: 18,
    coarseScore: 71,
    sampleScore: 66,
    finalScore: 68,
    route: 'rejudged',
    searchRank: 1,
    sampledRecords: 500,
    seedRecords: 14,
    highAnchors: 7,
    lowAnchors: 7,
    highBoundScore: 88,
    lowBoundScore: 39,
    propagatedRecords: 474,
    rescoredRecords: 12,
  },
  {
    key: 'kaggle-construction',
    candidateId: 'kaggle-construction',
    name: 'Kaggle_Construction',
    platform: 'kaggle',
    dataType: 'image',
    size: '5.1 GB',
    cost: 'Free',
    reviewCount: 14,
    coarseScore: 78,
    sampleScore: 71,
    finalScore: 75,
    route: 'propagated',
    searchRank: 2,
    sampledRecords: 500,
    seedRecords: 12,
    highAnchors: 6,
    lowAnchors: 6,
    highBoundScore: 85,
    lowBoundScore: 44,
    propagatedRecords: 480,
    rescoredRecords: 8,
  },
  {
    key: 'factory-ppe-microset',
    name: 'Factory_PPE_MicroSet',
    platform: 'guixu-hub',
    dataType: 'image',
    size: '1.1 GB',
    cost: '$4',
    reviewCount: 11,
    coarseScore: 69,
    sampleScore: 67,
    finalScore: 67,
    route: 'propagated',
    searchRank: 3,
    sampledRecords: 500,
    seedRecords: 10,
    highAnchors: 5,
    lowAnchors: 5,
    highBoundScore: 79,
    lowBoundScore: 41,
    propagatedRecords: 483,
    rescoredRecords: 7,
  },
  {
    key: 'warehouse-ppe',
    candidateId: 'warehouse-ppe',
    name: 'HF_HelmetScenes',
    platform: 'huggingface',
    dataType: 'image',
    size: '3.8 GB',
    cost: 'Free',
    reviewCount: 8,
    coarseScore: 81,
    sampleScore: 86,
    finalScore: 84,
    route: 'seed',
    searchRank: 4,
    sampledRecords: 500,
    seedRecords: 16,
    highAnchors: 8,
    lowAnchors: 8,
    highBoundScore: 92,
    lowBoundScore: 51,
    propagatedRecords: 476,
    rescoredRecords: 8,
  },
  {
    key: 'safetyvest-mixedshots',
    name: 'SafetyVest_MixedShots',
    platform: 'huggingface',
    dataType: 'image',
    size: '2.6 GB',
    cost: '$3',
    reviewCount: 9,
    coarseScore: 62,
    sampleScore: 57,
    finalScore: 59,
    route: 'rejudged',
    searchRank: 7,
    sampledRecords: 500,
    seedRecords: 12,
    highAnchors: 6,
    lowAnchors: 6,
    highBoundScore: 71,
    lowBoundScore: 26,
    propagatedRecords: 470,
    rescoredRecords: 18,
  },
  {
    key: 'roboflow-hardhat',
    candidateId: 'roboflow-hardhat',
    name: 'RF_Hardhat_Scenes',
    platform: 'roboflow',
    dataType: 'image',
    size: '2.9 GB',
    cost: '$5',
    reviewCount: 6,
    coarseScore: 74,
    sampleScore: 73,
    finalScore: 73,
    route: 'propagated',
    searchRank: 6,
    sampledRecords: 500,
    seedRecords: 12,
    highAnchors: 6,
    lowAnchors: 6,
    highBoundScore: 82,
    lowBoundScore: 37,
    propagatedRecords: 481,
    rescoredRecords: 7,
  },
  {
    key: 'safehat-premium',
    candidateId: 'safehat-premium',
    name: 'SafeHat_Premium',
    platform: 'guixu-hub',
    dataType: 'image',
    size: '2.3 GB',
    cost: '$10',
    reviewCount: 32,
    coarseScore: 87,
    sampleScore: 94,
    finalScore: 92,
    route: 'rejudged',
    searchRank: 5,
    sampledRecords: 500,
    seedRecords: 14,
    highAnchors: 7,
    lowAnchors: 7,
    highBoundScore: 95,
    lowBoundScore: 47,
    propagatedRecords: 477,
    rescoredRecords: 9,
  },
  {
    key: 'construction-stillframes',
    name: 'Construction_StillFrames',
    platform: 'kaggle',
    dataType: 'image',
    size: '4.0 GB',
    cost: '$2',
    reviewCount: 7,
    coarseScore: 66,
    sampleScore: 62,
    finalScore: 64,
    route: 'propagated',
    searchRank: 8,
    sampledRecords: 500,
    seedRecords: 10,
    highAnchors: 5,
    lowAnchors: 5,
    highBoundScore: 75,
    lowBoundScore: 35,
    propagatedRecords: 484,
    rescoredRecords: 6,
  },
  {
    key: 'bt-safetyframes',
    candidateId: 'bt-safetyframes',
    name: 'BT_SafetyFrames',
    platform: 'torrent',
    dataType: 'image',
    size: '4.4 GB',
    cost: 'Free',
    reviewCount: 3,
    coarseScore: 63,
    sampleScore: 58,
    finalScore: 61,
    route: 'seed',
    searchRank: 9,
    sampledRecords: 500,
    seedRecords: 12,
    highAnchors: 6,
    lowAnchors: 6,
    highBoundScore: 68,
    lowBoundScore: 22,
    propagatedRecords: 468,
    rescoredRecords: 20,
  },
  {
    key: 'helmet-compliance-archive',
    name: 'Helmet_Compliance_Archive',
    platform: 'torrent',
    dataType: 'image',
    size: '6.1 GB',
    cost: 'Free',
    reviewCount: 2,
    coarseScore: 58,
    sampleScore: 54,
    finalScore: 56,
    route: 'seed',
    searchRank: 10,
    sampledRecords: 500,
    seedRecords: 10,
    highAnchors: 5,
    lowAnchors: 5,
    highBoundScore: 63,
    lowBoundScore: 18,
    propagatedRecords: 465,
    rescoredRecords: 25,
  },
]

export const selectedBundles: Record<CandidateId, BundleCard[]> = {
  'safehat-premium': [
    { name: 'SafeHat_Premium', role: 'highest final utility', utility: '+0.94' },
    { name: 'HF_HelmetScenes', role: 'cheap scene diversity', utility: '+0.61' },
    { name: 'RF_Hardhat_Scenes', role: 'fills edge cases', utility: '+0.38' },
  ],
  'warehouse-ppe': [
    { name: 'HF_HelmetScenes', role: 'best public fit', utility: '+0.86' },
    { name: 'SafeHat_Premium', role: 'adds trust signal', utility: '+0.72' },
    { name: 'RF_Hardhat_Scenes', role: 'fills edge cases', utility: '+0.34' },
  ],
  'kaggle-construction': [
    { name: 'Kaggle_Construction', role: 'broad coverage', utility: '+0.71' },
    { name: 'HF_HelmetScenes', role: 'better labels', utility: '+0.58' },
    { name: 'RF_Hardhat_Scenes', role: 'more hardhat frames', utility: '+0.29' },
  ],
  'bt-safetyframes': [
    { name: 'BT_SafetyFrames', role: 'low-cost tail data', utility: '+0.52' },
    { name: 'HF_HelmetScenes', role: 'better label prior', utility: '+0.57' },
    { name: 'SafeHat_Premium', role: 'trust-weighted boost', utility: '+0.74' },
  ],
  'roboflow-hardhat': [
    { name: 'RF_Hardhat_Scenes', role: 'tail hardhat scenes', utility: '+0.63' },
    { name: 'SafeHat_Premium', role: 'best overall fit', utility: '+0.76' },
    { name: 'HF_HelmetScenes', role: 'label stability', utility: '+0.45' },
  ],
}

export const knapsackDisplay: KnapsackDisplay = {
  constraints: [
    { label: 'Budget', value: '≤ $2.00' },
    { label: 'Size', value: '6 - 8 GB' },
    { label: 'Goal', value: 'max utility' },
  ],
  rounds: [
    {
      label: 'Round 1',
      state: 'infeasible',
      rows: [
        { name: 'SafeHat_Premium', score: 94, price: '$2.30', size: '2.3' },
        { name: 'HF_HelmetScenes', score: 86, price: 'Free', size: '3.8' },
        { name: 'SiteCam_Helmet_Longtail', score: 66, price: '$1.10', size: '1.8' },
      ],
    },
    {
      label: 'Round 2',
      state: 'feasible',
      rows: [
        { name: 'HF_HelmetScenes', score: 86, price: 'Free', size: '3.8', chosen: true },
        { name: 'RF_Hardhat_Scenes', score: 73, price: '$0.60', size: '2.9', chosen: true },
        { name: 'Construction_StillFrames', score: 62, price: '$0.40', size: '4.0' },
        { name: 'Factory_PPE_MicroSet', score: 67, price: '$0.50', size: '1.1' },
      ],
    },
    {
      label: 'Round 3',
      state: 'infeasible',
      placeholder: 'reserved next batch',
      rows: [],
    },
  ],
  selected: {
    datasets: [
      { name: 'HF_HelmetScenes', score: 86 },
      { name: 'RF_Hardhat_Scenes', score: 73 },
    ],
    totalPrice: '$0.60',
    totalSize: '6.7 GB',
    totalScore: 159,
  },
}

export const getValuationSearchCandidates = (scored: boolean) =>
  [...valuationSearchCandidates].sort((left, right) => {
    if (!scored)
      return left.searchRank - right.searchRank

    return right.sampleScore - left.sampleScore
  })

export const getValuationSearchCandidateByKey = (key: string) =>
  valuationSearchCandidates.find(candidate => candidate.key === key)

export const getValuationSearchCandidateByCandidateId = (candidateId: CandidateId) =>
  valuationSearchCandidates.find(candidate => candidate.candidateId === candidateId)
