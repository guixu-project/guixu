/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useEffect, useState } from 'react'
import DemoHeader from './components/DemoHeader'
import LedgerPanel from './components/LedgerPanel'
import PlanningPanel from './components/PlanningPanel'
import ValuationPanel from './components/ValuationPanel'
import { candidates, paperExportDatasetId, type CandidateId, type MarketReview } from './data'
import { idlePlanningRuntimeState, type PlanningRuntimeState } from './demoTimeline'

const searchParams = new URLSearchParams(window.location.search)
const paperMode = searchParams.get('paper') === '1'
  || searchParams.get('mode') === 'paper'
  || searchParams.get('export') === 'paper'

const paperPlanningRuntimeState: PlanningRuntimeState = {
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

const initialPaperReviews = (): Record<string, MarketReview> => {
  if (!paperMode)
    return {}

  return {
    [paperExportDatasetId]: {
      id: `session-review-${paperExportDatasetId}`,
      reviewer_address: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
      content: 'Strong task fit with stable gain after execution on SafeHat_Premium.',
      source: 'on-chain',
      tx_hash: null,
      created_at: new Date().toISOString(),
    },
  }
}

const App = () => {
  const [selectedId, setSelectedId] = useState<CandidateId>('safehat-premium')
  const [activeMarketDatasetId, setActiveMarketDatasetId] = useState<string | null>(paperMode ? paperExportDatasetId : null)
  const [sessionReviewsByDatasetId, setSessionReviewsByDatasetId] = useState<Record<string, MarketReview>>(initialPaperReviews)
  const [planningRuntime, setPlanningRuntime] = useState<PlanningRuntimeState>(paperMode ? paperPlanningRuntimeState : idlePlanningRuntimeState)

  useEffect(() => {
    document.body.classList.toggle('paper-mode', paperMode)
    return () => {
      document.body.classList.remove('paper-mode')
    }
  }, [])

  const handleTraceReviewCommit = (candidateId: CandidateId) => {
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
          content: `Strong task fit with stable gain after execution on ${candidates[candidateId].name}.`,
          source: 'on-chain',
          tx_hash: null,
          created_at: new Date().toISOString(),
        },
      }
    })
  }

  return (
    <div className={`page-shell${paperMode ? ' paper-mode' : ''}`}>
      <DemoHeader paperMode={paperMode} />

      <main className="dashboard">
        <PlanningPanel
          onRecommendCandidate={setSelectedId}
          onRuntimeChange={setPlanningRuntime}
          paperMode={paperMode}
        />
        <ValuationPanel
          selectedId={selectedId}
          onSelectCandidate={setSelectedId}
          onTraceReviewCommit={handleTraceReviewCommit}
          planningRuntime={planningRuntime}
          paperMode={paperMode}
        />
      </main>

      <LedgerPanel
        selectedCandidateId={selectedId}
        sessionReviewsByDatasetId={sessionReviewsByDatasetId}
        onActiveDatasetChange={setActiveMarketDatasetId}
        paperMode={paperMode}
      />
    </div>
  )
}

export default App
