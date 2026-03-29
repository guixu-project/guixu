import { useState } from 'react'
import DemoHeader from './components/DemoHeader'
import LedgerPanel from './components/LedgerPanel'
import PlanningPanel from './components/PlanningPanel'
import ValuationPanel from './components/ValuationPanel'
import { candidates, type CandidateId } from './data'

const App = () => {
  const [selectedId, setSelectedId] = useState<CandidateId>('safehat-premium')
  const selected = candidates[selectedId]

  return (
    <div className="page-shell">
      <DemoHeader />

      <main className="dashboard">
        <PlanningPanel onRecommendCandidate={setSelectedId} />
        <ValuationPanel
          selectedId={selectedId}
          onSelectCandidate={setSelectedId}
        />
      </main>

      <LedgerPanel />
    </div>
  )
}

export default App
