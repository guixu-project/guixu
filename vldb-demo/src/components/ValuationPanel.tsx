import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { Candidate, CandidateId } from '../data'
import { candidates } from '../data'
import { demoTimingPresets, idlePlanningRuntimeState, type PlanningRuntimeState } from '../demoTimeline'
import SectionTitle from './SectionTitle'

const candidateOrder = Object.keys(candidates) as CandidateId[]
const scoreSignals = ['Code Compatibility', 'Annotation Quality', 'Task Relevance', 'On-chain Reputation']
const paymentSequenceSteps = [
  { label: 'x402 request sent', direction: 'outbound' },
  { label: 'payment required', direction: 'inbound' },
  { label: 'signed payment', direction: 'outbound' },
  { label: 'transaction confirmed', direction: 'inbound' },
] as const
const reviewAttestationSteps = [0, 1, 2] as const
const deliveryStepCount = 3
const keyReleaseFrames: Array<{
  activeNode: number | null
  collectedNodes: number[]
  unlocked: boolean
}> = [
  { activeNode: 0, collectedNodes: [0], unlocked: false },
  { activeNode: 2, collectedNodes: [0, 2], unlocked: false },
  { activeNode: 4, collectedNodes: [0, 2, 4], unlocked: false },
  { activeNode: null, collectedNodes: [0, 2, 4], unlocked: true },
]

type TraceStage = {
  kind: 'payment' | 'shards' | 'unlock' | 'review'
  label: string
  chips: string[]
}

type DeliveryTrace = {
  isOnChain: boolean
  contract: string
  quorum: string
  stages: TraceStage[]
  inactiveNote?: string
}

type StageVisualStatus = 'done' | 'current' | 'upcoming' | 'idle'

const stageLogText = (candidateName: string, stage: TraceStage, isOnChain: boolean) => {
  if (!isOnChain)
    return `Public source selected for ${candidateName}. No on-chain unlock path.`

  switch (stage.kind) {
    case 'payment':
      return `Agent is buying ${candidateName} on-chain...`
    case 'shards':
      return `Getting distributed key shares for ${candidateName}...`
    case 'unlock':
      return `Decrypting, verifying, and handing ${candidateName} to task execution...`
    case 'review':
      return `Posting review attestation back to market memory...`
  }
}

const stepIndexFor = (elapsedMs: number, totalMs: number, count: number) => {
  if (count <= 1 || totalMs <= 0)
    return Math.max(0, count - 1)

  const clamped = Math.max(0, Math.min(elapsedMs, totalMs))
  return Math.min(count - 1, Math.floor((clamped / totalMs) * count))
}

const radarPoints = (values: Candidate['radar']) => {
  const centerX = 110
  const centerY = 102
  const radius = 68
  const angles = [-90, -18, 54, 126, 198]

  return values
    .map((value, index) => {
      const angle = (angles[index] * Math.PI) / 180
      const r = (value / 100) * radius
      const x = centerX + Math.cos(angle) * r
      const y = centerY + Math.sin(angle) * r
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
}

const shortenValue = (value: string, head = 10, tail = 8) => {
  if (!value || value === '—' || value.length <= head + tail + 3)
    return value

  return `${value.slice(0, head)}...${value.slice(-tail)}`
}

const TraceIcon = ({
  kind,
}: {
  kind: 'agent' | 'wallet' | 'contract' | 'blockchain' | 'key' | 'archive' | 'dataset' | 'review' | 'memory'
}) => {
  switch (kind) {
    case 'agent':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="8" r="3.2" fill="currentColor" />
          <path d="M6.2 18.2c1.3-2.8 3.3-4.2 5.8-4.2s4.5 1.4 5.8 4.2" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )
    case 'wallet':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect x="4" y="7" width="16" height="10" rx="2.5" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="M15 12h3" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
          <circle cx="17.8" cy="12" r="1" fill="currentColor" />
        </svg>
      )
    case 'contract':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M8 4h6l4 4v12H8z" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
          <path d="M14 4v4h4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
          <path d="M10 12h6M10 15h6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )
    case 'blockchain':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <ellipse cx="12" cy="6.5" rx="5.5" ry="2.5" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="M6.5 6.5v7c0 1.4 2.5 2.5 5.5 2.5s5.5-1.1 5.5-2.5v-7" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="M6.5 10c0 1.4 2.5 2.5 5.5 2.5s5.5-1.1 5.5-2.5M6.5 13.5c0 1.4 2.5 2.5 5.5 2.5s5.5-1.1 5.5-2.5" fill="none" stroke="currentColor" strokeWidth="2" />
        </svg>
      )
    case 'key':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="8" cy="12" r="3.2" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="M11.2 12H20m-3 0v-2m-3 2v2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )
    case 'archive':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect x="6" y="5" width="12" height="14" rx="2" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="M9 8h6M12 8v8" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )
    case 'dataset':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 8.5 12 5l8 3.5-8 3.5z" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
          <path d="M4 12.5 12 16l8-3.5M4 16.5 12 20l8-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
        </svg>
      )
    case 'review':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 5h12v9H9l-3 3z" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
          <path d="M9 9h6M9 12h4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )
    case 'memory':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect x="5" y="7" width="14" height="10" rx="2" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="M8 4v3m8-3v3m-8 10v3m8-3v3M2 10h3m14 0h3M2 14h3m14 0h3" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )
  }
}

const buildDeliveryTrace = (candidate: Candidate): DeliveryTrace => {
  if (candidate.platform === 'guixu-hub') {
    return {
      isOnChain: true,
      contract: 'Blockchain',
      quorum: '3/5',
      stages: [
        {
          kind: 'payment',
          label: 'Agentic Payment',
          chips: ['x402-style', 'fileHash', 'escrow'],
        },
        {
          kind: 'shards',
          label: 'Decentralized Key Management',
          chips: ['5 stored', '3 recovered'],
        },
        {
          kind: 'unlock',
          label: 'Verified Data Delivery',
          chips: ['local decrypt', 'task execution'],
        },
        {
          kind: 'review',
          label: 'On-Chain Feedback',
          chips: ['memo posted', 'memory updated'],
        },
      ],
    }
  }

  return {
    isOnChain: false,
    contract: '—',
    quorum: '0/5',
    stages: [
      {
        kind: 'payment',
        label: 'Agentic Payment',
        chips: ['off-chain', 'no escrow'],
      },
      {
        kind: 'shards',
        label: 'Distributed Key Release',
        chips: ['no key nodes', 'no quorum'],
      },
      {
        kind: 'unlock',
        label: 'Direct Data Delivery',
        chips: ['public data', 'no locked asset'],
      },
      {
        kind: 'review',
        label: 'Feedback Record',
        chips: ['no memo sync', 'weaker trust'],
      },
    ],
    inactiveNote: 'Only Guixu-HUB assets expose the full on-chain agent trace.',
  }
}

const platformLabels: Record<string, string> = {
  'guixu-hub': 'GUIXU HUB',
  kaggle: 'KAGGLE',
  huggingface: 'HUGGINGFACE',
  roboflow: 'ROBOFLOW',
  torrent: 'TORRENT',
}

const CandidateButton = ({
  candidate,
  active,
  onClick,
}: {
  candidate: Candidate
  active: boolean
  onClick: () => void
}) => (
  <button className={`candidate-item${active ? ' active' : ''}`} type="button" onClick={onClick}>
    <div className="candidate-badge-row">
      <span className={`candidate-platform platform-${candidate.platform}`}>
        {platformLabels[candidate.platform] ?? candidate.platform.toUpperCase()}
      </span>
      <span className="candidate-type-sep">·</span>
      <span className="candidate-type-text">{candidate.dataType}</span>
      <span className="candidate-score-inline">{candidate.score}</span>
    </div>
    <span className="candidate-title">{candidate.name}</span>
    <span className="candidate-meta-text">
      {candidate.size} · {candidate.cost} · {candidate.reviewCount} reviews
    </span>
  </button>
)

const agentAvatarSrc = new URL('../../assets/Adventure_Time_Profile_Pictures/A_Ice_King.png', import.meta.url).href
const blockchainIconSrc = new URL('../../assets/Blockchain.png', import.meta.url).href

const KeyReleaseVisual = ({
  frame,
  isOnChain,
  stageStatus,
}: {
  frame: { activeNode: number | null, collectedNodes: number[], unlocked: boolean }
  isOnChain: boolean
  stageStatus: StageVisualStatus
}) => {
  const shardNodes = [0, 2, 4]
  const displayFrame = stageStatus === 'done'
    ? keyReleaseFrames[keyReleaseFrames.length - 1]
    : stageStatus === 'current'
      ? frame
      : { activeNode: null, collectedNodes: [], unlocked: false }
  const displayFrameKey = `${displayFrame.activeNode ?? 'idle'}:${displayFrame.collectedNodes.join('-')}:${displayFrame.unlocked ? 1 : 0}`
  const stackRef = useRef<HTMLDivElement | null>(null)
  const nodeIconRefs = useRef<Array<HTMLSpanElement | null>>([])
  const shardRefs = useRef<Array<HTMLSpanElement | null>>([])
  const [svgBox, setSvgBox] = useState({ width: 0, height: 0 })
  const [paths, setPaths] = useState<string[]>([])

  useLayoutEffect(() => {
    const updatePaths = () => {
      const stack = stackRef.current
      if (!stack)
        return

      const stackRect = stack.getBoundingClientRect()
      const nextPaths = shardNodes.map((nodeIdx, shardIdx) => {
        const nodeEl = nodeIconRefs.current[shardIdx]
        const shardEl = shardRefs.current[shardIdx]
        if (!nodeEl || !shardEl)
          return ''

        const nodeRect = nodeEl.getBoundingClientRect()
        const shardRect = shardEl.getBoundingClientRect()

        const startX = nodeRect.left - stackRect.left + nodeRect.width / 2
        const startY = nodeRect.bottom - stackRect.top
        const endX = shardRect.left - stackRect.left + shardRect.width / 2
        const endY = shardRect.top - stackRect.top
        const controlY = startY + Math.max(16, (endY - startY) * 0.48)

        return `M ${startX} ${startY} C ${startX} ${controlY}, ${endX} ${controlY}, ${endX} ${endY}`
      })

      const nextBox = {
        width: Math.max(1, stackRect.width),
        height: Math.max(1, stackRect.height),
      }

      setSvgBox(prev =>
        prev.width === nextBox.width && prev.height === nextBox.height ? prev : nextBox,
      )
      setPaths(prev =>
        prev.length === nextPaths.length && prev.every((path, index) => path === nextPaths[index])
          ? prev
          : nextPaths,
      )
    }

    updatePaths()

    const observer = new ResizeObserver(() => {
      updatePaths()
    })

    if (stackRef.current)
      observer.observe(stackRef.current)

    nodeIconRefs.current.forEach(el => el && observer.observe(el))
    shardRefs.current.forEach(el => el && observer.observe(el))
    window.addEventListener('resize', updatePaths)

    return () => {
      observer.disconnect()
      window.removeEventListener('resize', updatePaths)
    }
  }, [displayFrameKey])

  return (
    <div className={`key-release-flow${isOnChain ? '' : ' muted'}`}>
      <div ref={stackRef} className="key-release-stack">
        <div className="key-network-grid">
          {[0, 1, 2, 3, 4].map(i => {
            const collected = displayFrame.collectedNodes.includes(i)
            const current = displayFrame.activeNode === i
            const shardIndex = shardNodes.indexOf(i)
            const hasShard = shardIndex !== -1

            return (
              <div key={i} className="key-network-node-wrap">
                <div className={`key-network-node${collected ? ' collected' : ''}${current ? ' current' : ''}${hasShard ? ' has-shard' : ''}`}>
                  <span
                    ref={el => {
                      if (hasShard)
                        nodeIconRefs.current[shardIndex] = el
                    }}
                    className="key-network-icon"
                  >
                    <TraceIcon kind="blockchain" />
                  </span>
                  <span className="key-network-label">Node {i + 1}</span>
                </div>
              </div>
            )
          })}
        </div>

        <svg
          className="key-merge-svg"
          width={svgBox.width}
          height={svgBox.height}
          viewBox={`0 0 ${svgBox.width} ${svgBox.height}`}
          aria-hidden="true"
        >
          {(stageStatus === 'current' || stageStatus === 'done') && shardNodes.map((nodeIdx, index) => (
            <path
              key={nodeIdx}
              d={paths[index] ?? ''}
              className={`key-merge-curve${displayFrame.collectedNodes.includes(nodeIdx) ? ' active' : ''}${displayFrame.activeNode === nodeIdx ? ' current' : ''}`}
            />
          ))}
        </svg>

        <div className="key-merge-row">
          <div className="key-merge-pills">
            {shardNodes.map((nodeIdx, i) => (
              <span
                key={nodeIdx}
                ref={el => {
                  shardRefs.current[i] = el
                }}
                className={`key-merge-pill${displayFrame.collectedNodes.includes(nodeIdx) ? ' filled' : ''}`}
              >
                shard{i + 1}
              </span>
            ))}
          </div>
          <span className="key-assembly-arrow">→</span>
          <span className={`key-master${displayFrame.unlocked ? ' unlocked' : ''}`}>
            <TraceIcon kind="key" />
          </span>
        </div>
      </div>
    </div>
  )
}

const DataDeliveryVisual = ({
  isOnChain,
  deliveryStepIndex,
  stageStatus,
}: {
  isOnChain: boolean
  deliveryStepIndex: number
  stageStatus: StageVisualStatus
}) => (
  <div className={`delivery-flow${isOnChain ? '' : ' muted'}`}>
    <div className={`delivery-node ${stageStatus === 'done' ? ' done' : stageStatus === 'current' ? (deliveryStepIndex > 0 ? ' done' : deliveryStepIndex === 0 ? ' current' : ' idle') : ' idle'}`}>
      <span className="delivery-icon archive"><TraceIcon kind="archive" /></span>
      <strong>{isOnChain ? 'Encrypted Data' : 'Open Dataset'}</strong>
    </div>

    <span className={`delivery-arrow ${stageStatus === 'done' || (stageStatus === 'current' && deliveryStepIndex >= 1) ? ' active' : ''}`}>→</span>

    <div className={`delivery-core ${stageStatus === 'done' ? ' done' : stageStatus === 'current' ? (deliveryStepIndex > 1 ? ' done' : deliveryStepIndex === 1 ? ' current' : ' idle') : ' idle'}`}>
      <span className="delivery-core-icon"><TraceIcon kind="key" /></span>
      <strong>{isOnChain ? 'Decrypt' : 'Direct Fetch'}</strong>
    </div>

    <span className={`delivery-arrow ${stageStatus === 'done' || (stageStatus === 'current' && deliveryStepIndex >= 2) ? ' active' : ''}`}>→</span>

    <div className={`delivery-node ready ${stageStatus === 'done' ? ' done' : stageStatus === 'current' ? (deliveryStepIndex > 2 ? ' done' : deliveryStepIndex === 2 ? ' current' : ' idle') : ' idle'}`}>
      <span className="delivery-icon dataset"><TraceIcon kind="dataset" /></span>
      <strong>Verified Data</strong>
    </div>
  </div>
)

const ReviewAttestationVisual = ({
  isOnChain,
  reviewStepIndex,
  executionMetric,
  stageStatus,
}: {
  isOnChain: boolean
  reviewStepIndex: number
  executionMetric: string
  stageStatus: StageVisualStatus
}) => (
  <div className={`review-trace${isOnChain ? '' : ' muted'}`}>
    <div className={`review-input-card ${stageStatus === 'done' ? ' done' : stageStatus === 'current' ? (reviewStepIndex > 0 ? ' done' : reviewStepIndex === 0 ? ' current' : ' idle') : ' idle'}`}>
      <span className="review-input-tag">Task Finished</span>
      <strong>{`acc ${executionMetric}`}</strong>
    </div>

    <span className={`review-trace-arrow ${stageStatus === 'done' || (stageStatus === 'current' && reviewStepIndex >= 1) ? ' active' : ''}`}>→</span>

    <div className={`review-draft-card ${stageStatus === 'done' ? ' done' : stageStatus === 'current' ? (reviewStepIndex > 1 ? ' done' : reviewStepIndex === 1 ? ' current' : ' idle') : ' idle'}`}>
      <span className="review-badge">Review Generation</span>
      <p>“Strong task fit with stable gain after execution.”</p>
    </div>

    <span className={`review-trace-arrow ${stageStatus === 'done' || (stageStatus === 'current' && reviewStepIndex >= 2) ? ' active' : ''}`}>→</span>

    <div className={`review-chain-card ${stageStatus === 'done' ? ' done' : stageStatus === 'current' ? (reviewStepIndex === 2 ? ' current' : reviewStepIndex > 2 ? ' done' : ' idle') : ' idle'}`}>
      <span className="review-badge">{isOnChain ? 'On-Chain Feedback' : 'Feedback Record'}</span>
      <span>{isOnChain ? '0x72bc...881f' : 'local-only'}</span>
    </div>
  </div>
)

const TraceVisual = ({
  stage,
  contract,
  isOnChain,
  shardsRecovered,
  paymentStepIndex,
  shareFrameIndex,
  deliveryStepIndex,
  reviewStepIndex,
  executionMetric,
  isStageActive,
  stageStatus,
}: {
  stage: TraceStage
  contract: string
  isOnChain: boolean
  shardsRecovered: number
  paymentStepIndex: number
  shareFrameIndex: number
  deliveryStepIndex: number
  reviewStepIndex: number
  executionMetric: string
  isStageActive: boolean
  stageStatus: StageVisualStatus
}) => {
  switch (stage.kind) {
    case 'payment': {
      const step = paymentSequenceSteps[paymentStepIndex]

      return (
        <div className={`payment-flow${isOnChain ? '' : ' muted'}${isStageActive ? ' running' : ''}${stageStatus === 'idle' ? ' hidden' : ''}`}>
          <div className="payment-flow-row">
            <span className="payment-node">
              <img src={agentAvatarSrc} alt="Agent" className="payment-avatar" />
              <strong>Agent</strong>
            </span>
            <div className="payment-channel">
              <div className={`payment-channel-bar ${step.direction}`} />
              <span className="payment-channel-arrow">
                {step.direction === 'outbound' ? '▸' : '◂'}
              </span>
              <span className="payment-channel-label">{step.label}</span>
            </div>
            <span className="payment-node">
              <img src={blockchainIconSrc} alt="Blockchain" className="payment-avatar" />
              <strong>{shortenValue(contract, 5, 3)}</strong>
            </span>
          </div>
          <div className="payment-progress">
            {paymentSequenceSteps.map((s, i) => (
              <span
                key={s.label}
                className={`payment-dot${i === paymentStepIndex ? ' active' : ''}${i < paymentStepIndex ? ' done' : ''}`}
              />
            ))}
          </div>
        </div>
      )
    }

    case 'shards': {
      const frame = keyReleaseFrames[shareFrameIndex]
      return <KeyReleaseVisual frame={frame} isOnChain={isOnChain} stageStatus={stageStatus} />
    }

    case 'unlock':
      return <DataDeliveryVisual isOnChain={isOnChain} deliveryStepIndex={deliveryStepIndex} stageStatus={stageStatus} />

    case 'review':
      return <ReviewAttestationVisual isOnChain={isOnChain} reviewStepIndex={reviewStepIndex} executionMetric={executionMetric} stageStatus={stageStatus} />
  }
}

const ValuationPanel = ({
  selectedId,
  onSelectCandidate,
  onTraceReviewCommit,
  planningRuntime = idlePlanningRuntimeState,
  paperMode = false,
}: {
  selectedId: CandidateId
  onSelectCandidate: (candidateId: CandidateId) => void
  onTraceReviewCommit?: (candidateId: CandidateId) => void
  planningRuntime?: PlanningRuntimeState
  paperMode?: boolean
}) => {
  const selected = candidates[selectedId]
  const selectedRadar = radarPoints(selected.radar)
  const signalMetrics = selected.metrics.filter(metric => scoreSignals.includes(metric.label))
  const utilityMetric = selected.metrics.find(metric => metric.label === 'Fast-DataShapley Value')
  const trace = buildDeliveryTrace(selected)
  const reviewExecutionMetric = '95.2%'
  const [traceClockMs, setTraceClockMs] = useState(0)
  const [reviewTraceStartedAt, setReviewTraceStartedAt] = useState<number | null>(null)
  const committedReviewRef = useRef<string | null>(null)
  const traceEnabled = trace.isOnChain && planningRuntime.hasGuixuHub
  const traceStageDurationsMs = demoTimingPresets[planningRuntime.presetIndex].traceStageDurationsMs
  const purchasePhase = planningRuntime.nodePhases.purchase
  const purchaseProgress = planningRuntime.nodeProgress.purchase
  const executionPhase = planningRuntime.nodePhases.execution
  const purchaseStarted = traceEnabled && purchasePhase !== 'idle'
  const executionDone = traceEnabled && executionPhase === 'done'
  const paymentDurationMs = traceStageDurationsMs?.payment ?? 1
  const shardDurationMs = traceStageDurationsMs?.shards ?? 1
  const unlockDurationMs = traceStageDurationsMs?.unlock ?? 1
  const reviewDurationMs = traceStageDurationsMs?.review ?? 1
  const paymentEndMs = paymentDurationMs
  const shardsEndMs = paymentEndMs + shardDurationMs
  const unlockEndMs = shardsEndMs + unlockDurationMs
  const purchaseElapsedMs = purchaseStarted
    ? Math.min(unlockEndMs, Math.max(0, purchaseProgress) * unlockEndMs)
    : 0
  const reviewElapsedMs = reviewTraceStartedAt === null ? 0 : Math.max(0, traceClockMs - reviewTraceStartedAt)
  const paymentCurrent = traceEnabled && purchaseStarted && purchaseElapsedMs < paymentEndMs
  const shardsCurrent = traceEnabled && purchaseStarted && purchaseElapsedMs >= paymentEndMs && purchaseElapsedMs < shardsEndMs
  const unlockCurrent = traceEnabled && purchaseStarted && purchaseElapsedMs >= shardsEndMs && purchaseElapsedMs < unlockEndMs
  const unlockDone = traceEnabled && purchaseStarted && purchaseElapsedMs >= unlockEndMs
  const reviewStarted = traceEnabled && reviewTraceStartedAt !== null
  const reviewCurrent = reviewStarted && reviewElapsedMs < reviewDurationMs
  const reviewDone = reviewStarted && reviewElapsedMs >= reviewDurationMs
  const traceActivated = traceEnabled && purchaseStarted
  const paymentStepIndex = stepIndexFor(purchaseElapsedMs, paymentDurationMs, paymentSequenceSteps.length)
  const shareFrameIndex = stepIndexFor(Math.max(0, purchaseElapsedMs - paymentEndMs), shardDurationMs, keyReleaseFrames.length)
  const deliveryStepIndex = stepIndexFor(Math.max(0, purchaseElapsedMs - shardsEndMs), unlockDurationMs, deliveryStepCount)
  const reviewStepIndex = stepIndexFor(reviewElapsedMs, reviewDurationMs, reviewAttestationSteps.length)

  useEffect(() => {
    if (paperMode) {
      setTraceClockMs(0)
      setReviewTraceStartedAt(0)
      committedReviewRef.current = selectedId
      if (trace.isOnChain)
        onTraceReviewCommit?.(selectedId)
      return
    }

    setTraceClockMs(0)
    setReviewTraceStartedAt(null)
    committedReviewRef.current = null
  }, [paperMode, selectedId, planningRuntime.launchId, trace.isOnChain, trace.stages.length, onTraceReviewCommit])

  useEffect(() => {
    if (paperMode)
      return

    const shouldTickReview = reviewStarted && reviewElapsedMs < reviewDurationMs
    if (!shouldTickReview)
      return

    const timer = window.setInterval(() => {
      setTraceClockMs(performance.now())
    }, 90)

    return () => {
      window.clearInterval(timer)
    }
  }, [paperMode, reviewStarted, reviewElapsedMs, reviewDurationMs])

  useEffect(() => {
    if (paperMode || !traceEnabled || !unlockDone || !executionDone || reviewTraceStartedAt !== null)
      return

    const now = performance.now()
    setReviewTraceStartedAt(now)
    setTraceClockMs(now)
  }, [paperMode, traceEnabled, unlockDone, executionDone, reviewTraceStartedAt])

  useEffect(() => {
    if (!traceEnabled || !reviewDone)
      return

    if (committedReviewRef.current === selectedId)
      return

    committedReviewRef.current = selectedId
    onTraceReviewCommit?.(selectedId)
  }, [traceEnabled, reviewDone, selectedId, onTraceReviewCommit])

  const stageState = (index: number) => {
    if (paperMode && traceEnabled)
      return 'done'
    if (!traceEnabled || !traceActivated)
      return 'idle'

    switch (index) {
      case 0:
        return paymentCurrent ? 'current' : 'done'
      case 1:
        if (purchaseElapsedMs < paymentEndMs)
          return 'idle'
        return shardsCurrent ? 'current' : purchaseElapsedMs >= shardsEndMs ? 'done' : 'upcoming'
      case 2:
        if (purchaseElapsedMs < shardsEndMs)
          return 'idle'
        return unlockCurrent ? 'current' : unlockDone ? 'done' : 'upcoming'
      case 3:
        if (!reviewStarted)
          return 'idle'
        return reviewCurrent ? 'current' : reviewDone ? 'done' : 'upcoming'
      default:
        return 'idle'
    }
  }

  const shardsRecovered = traceEnabled ? 3 : 0
  const currentStageIndex = paperMode && traceEnabled
    ? trace.stages.length - 1
    : paymentCurrent ? 0 : shardsCurrent ? 1 : unlockCurrent || (!unlockDone && traceActivated) ? 2 : reviewStarted ? 3 : 0
  const currentStage = trace.stages[currentStageIndex]
  const statusLog = paperMode && traceEnabled
    ? 'Completed trace ready for paper export.'
    : !traceEnabled
    ? 'Select Guixu Hub in the workflow to enable the on-chain agent trace.'
    : !purchaseStarted
        ? 'Waiting for Agentic Purchase in the workflow...'
        : unlockDone && !executionDone
            ? 'Waiting for Task Execution to finish...'
            : stageLogText(selected.name, currentStage, traceEnabled)

  return (
    <section className="panel valuation-panel">
      <div className="panel-heading">
        <SectionTitle variant="valuation" title="Data Valuation, Selection, and Execution" />
      </div>

      <div className="valuation-workspace">
        <aside className="workspace-card candidate-column">
          <div className="card-header">
            <h3>Candidate Datasets</h3>
          </div>
          <div className="candidate-list">
            {candidateOrder.map(id => (
              <CandidateButton
                key={id}
                candidate={candidates[id]}
                active={id === selectedId}
                onClick={() => onSelectCandidate(id)}
              />
            ))}
          </div>
        </aside>

        <section className="workspace-card valuation-column">
          <div className="valuation-topline">
            <div>
              <p className="label">Valuation core</p>
              <h3>{selected.name}</h3>
            </div>
            <div className="score-block">
              <span className="label">Task Fitness</span>
              <strong>{selected.score}</strong>
            </div>
          </div>

          <div className="valuation-core">
            <div className="radar-panel">
              <p className="mini-title">Multi-signal fit</p>
              <svg viewBox="0 0 220 220" aria-hidden="true">
                <polygon points="110,26 176,74 151,152 69,152 44,74" className="radar-grid" />
                <polygon points="110,46 158,82 140,140 80,140 62,82" className="radar-grid inner" />
                <polygon points={selectedRadar} className="radar-shape" />
              </svg>
              <div className="radar-labels">
                <span>Code Fit</span>
                <span>Quality</span>
                <span>Temporal</span>
                <span>Diversity</span>
                <span>Trust</span>
              </div>
            </div>

            <div className="signal-panel">
              <p className="mini-title">Scoring signals</p>
              <div className="metric-list compact">
                {signalMetrics.map(metric => (
                  <div key={metric.label} className="metric-row">
                    <span>{metric.label}</span>
                    <div className="metric-track">
                      <div
                        className="metric-fill"
                        style={{ width: `${metric.value}%`, background: metric.color }}
                      />
                    </div>
                    <strong>{metric.value}</strong>
                  </div>
                ))}
              </div>

              {utilityMetric && (
                <div className="utility-box">
                  <span className="label">Expected utility</span>
                  <strong>{utilityMetric.label}</strong>
                  <p>{selected.roi}</p>
                </div>
              )}
            </div>
          </div>
        </section>

        <aside className="workspace-card execution-column">
          <div className="card-header">
            <h3>On-chain Agent Trace</h3>
          </div>

          <div className={`agent-trace${traceEnabled ? '' : ' inactive'}`}>
            <div className="agent-trace-head">
              <div className="agent-trace-log">
                <span className={`agent-trace-log-dot${traceActivated ? ' active' : ''}`} />
                <span>{statusLog}</span>
              </div>
            </div>

            <div className="agent-trace-lane">
              {trace.stages.map((stage, index) => {
                const nodeState = stageState(index)

                return (
                  <div key={stage.label} className={`agent-trace-stage ${nodeState}`}>
                    <div className="agent-trace-marker">
                      <span className="agent-trace-dot" />
                      {index < trace.stages.length - 1 && <span className="agent-trace-line" />}
                    </div>

                    <div className="agent-trace-card">
                      <div className="agent-trace-card-head">
                        <div>
                          <strong>{stage.label}</strong>
                        </div>
                      </div>

                      <TraceVisual
                        stage={stage}
                        contract={trace.contract}
                        isOnChain={trace.isOnChain}
                        shardsRecovered={shardsRecovered}
                        paymentStepIndex={paymentStepIndex}
                        shareFrameIndex={shareFrameIndex}
                        deliveryStepIndex={deliveryStepIndex}
                        reviewStepIndex={reviewStepIndex}
                        executionMetric={reviewExecutionMetric}
                        isStageActive={traceActivated && index === currentStageIndex && nodeState === 'current'}
                        stageStatus={nodeState}
                      />

                    </div>
                  </div>
                )
              })}
            </div>

            {trace.inactiveNote && <p className="trace-note">{trace.inactiveNote}</p>}
          </div>
        </aside>
      </div>
    </section>
  )
}

export default ValuationPanel
