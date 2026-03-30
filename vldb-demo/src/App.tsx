import { useState } from 'react'
import LedgerPanel from './components/LedgerPanel'
import PlanningPanel from './components/PlanningPanel'
import ValuationPanel from './components/ValuationPanel'
import type { CandidateId, MarketReview } from './data'
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
  const [sessionReviewsByDatasetId, setSessionReviewsByDatasetId] = useState<Record<string, MarketReview>>({})
  const [planningRuntime, setPlanningRuntime] = useState<PlanningRuntimeState>(
    completedMode ? completedPlanningRuntimeState : idlePlanningRuntimeState,
  )

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
          content: 'Strong task fit with stable gain after execution. Reliable candidate for future agent runs.',
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
          onTraceReviewCommit={handleTraceReviewCommit}
          planningRuntime={planningRuntime}
          completedMode={completedMode}
        />
      </main>

      <LedgerPanel
        selectedCandidateId={selectedId}
        sessionReviewsByDatasetId={sessionReviewsByDatasetId}
        onActiveDatasetChange={setActiveMarketDatasetId}
        preferredDatasetTitle={demoUiMode ? 'Design Paper' : undefined}
        disableCandidateAutoMatch={demoUiMode}
      />
    </div>
  )
}

export default App
