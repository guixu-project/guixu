import { useState } from 'react'
import LedgerPanel, { type HistoryRow } from './components/LedgerPanel'
import PlanningPanel from './components/PlanningPanel'
import ValuationPanel from './components/ValuationPanel'
import { candidates, type CandidateId, type MarketReview } from './data'
import { idlePlanningRuntimeState, type PlanningRuntimeState } from './demoTimeline'

const demoUiMode = true
const completedMode = true

const completedPlanningRuntimeState: PlanningRuntimeState = {
  launchId: 1,
  launched: true,
  hasGuixuHub: true,
  presetIndex: 1,
  nodePhases: {
    parser: 'done',
    search: 'done',
    code: 'done',
    valuation: 'done',
    purchase: 'done',
    execution: 'done',
  },
  nodeProgress: {
    parser: 1,
    search: 1,
    code: 1,
    valuation: 1,
    purchase: 1,
    execution: 1,
  },
}

const App = () => {
  const [selectedId, setSelectedId] = useState<CandidateId>('safehat-premium')
  const [activeMarketDatasetId, setActiveMarketDatasetId] = useState<string | null>(null)
  const [sessionHistoryRowsByDatasetId, setSessionHistoryRowsByDatasetId] = useState<Record<string, HistoryRow>>({})
  const [sessionReviewsByDatasetId, setSessionReviewsByDatasetId] = useState<Record<string, MarketReview>>({})
  const [planningRuntime, setPlanningRuntime] = useState<PlanningRuntimeState>(
    completedMode ? completedPlanningRuntimeState : idlePlanningRuntimeState,
  )

  const handleTracePaymentCommit = (candidateId: CandidateId) => {
    if (!activeMarketDatasetId)
      return

    setSessionHistoryRowsByDatasetId((prev) => {
      if (prev[activeMarketDatasetId])
        return prev

      return {
        ...prev,
        [activeMarketDatasetId]: {
          time: 'Mar 29, 19:11',
          timeTitle: '2026 Mar 29 19:11:00',
          eventType: 'PURCHASE',
          eventTone: 'purchased',
          buyer: '0x72bc...881f',
          buyerTitle: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
          value: candidates[candidateId].cost === 'Free' ? '0 USDC' : '1.10 USDC',
          txHash: '0xc19f8d8a7b31e4cf20c5ad9174ef8a33b6d41a8c0f72b9de55a3c18f9b27d8ea',
          txShort: '0xc19f...d8ea',
        },
      }
    })
  }

  const handleTraceReviewCommit = (_candidateId: CandidateId) => {
    if (!activeMarketDatasetId)
      return

    setSessionReviewsByDatasetId((prev) => {
      if (prev[activeMarketDatasetId])
        return prev

      return {
        ...prev,
        [activeMarketDatasetId]: {
          id: `session-review-${activeMarketDatasetId}`,
          reviewer_address: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
          content: 'Strong task fit with stable gain after execution on SafeHat_Premium.',
          source: 'on-chain',
          tx_hash: null,
          created_at: new Date().toISOString(),
        },
      }
    })
  }

  return (
    <div className={`page-shell${demoUiMode ? ' no-topbar' : ''}`}>
      <main className="dashboard">
        <PlanningPanel
          onRecommendCandidate={setSelectedId}
          onRuntimeChange={setPlanningRuntime}
          completedMode={completedMode}
        />
        <ValuationPanel
          selectedId={selectedId}
          onSelectCandidate={setSelectedId}
          onTracePaymentCommit={handleTracePaymentCommit}
          onTraceReviewCommit={handleTraceReviewCommit}
          planningRuntime={planningRuntime}
          completedMode={completedMode}
        />
      </main>

      <LedgerPanel
        selectedCandidateId={selectedId}
        sessionHistoryRowsByDatasetId={sessionHistoryRowsByDatasetId}
        sessionReviewsByDatasetId={sessionReviewsByDatasetId}
        onActiveDatasetChange={setActiveMarketDatasetId}
      />
    </div>
  )
}

export default App
