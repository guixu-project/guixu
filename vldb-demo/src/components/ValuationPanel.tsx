import { useEffect, useState } from 'react'
import type { CandidateId } from '../data'
import { candidates } from '../data'
import { idlePlanningRuntimeState, type PlanningRuntimeState } from '../demoTimeline'
import {
  getValuationSearchCandidateByCandidateId,
  getValuationSearchCandidateByKey,
  getValuationSearchCandidates,
  type ValuationSearchCandidate,
} from '../valuationDemo'
import OnChainAgentTrace from './OnChainAgentTrace'
import SectionTitle from './SectionTitle'
import ValuationCore from './ValuationCore'

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
  scored,
  loading,
  onClick,
}: {
  candidate: ValuationSearchCandidate
  active: boolean
  scored: boolean
  loading: boolean
  onClick: () => void
}) => (
  <button className={`candidate-item${active ? ' active' : ''}`} type="button" onClick={onClick}>
    <div className="candidate-badge-row">
      <span className={`candidate-platform platform-${candidate.platform}`}>
        {platformLabels[candidate.platform] ?? candidate.platform.toUpperCase()}
      </span>
      <span className="candidate-type-sep">·</span>
      <span className="candidate-type-text">{candidate.dataType}</span>
      {loading && (
        <span className="candidate-score-loading" aria-label="Sample scoring in progress">
          <span />
          <span />
          <span />
        </span>
      )}
      {scored && <span className="candidate-score-inline">{candidate.sampleScore}</span>}
    </div>
    <span className="candidate-title">{candidate.name}</span>
    <span className="candidate-meta-text">
      {candidate.size} · {candidate.cost} · {candidate.reviewCount} reviews
    </span>
  </button>
)

const ValuationPanel = ({
  selectedId,
  onSelectCandidate,
  onTraceReviewCommit,
  planningRuntime = idlePlanningRuntimeState,
  completedMode = false,
}: {
  selectedId: CandidateId
  onSelectCandidate: (candidateId: CandidateId) => void
  onTraceReviewCommit?: (candidateId: CandidateId) => void
  planningRuntime?: PlanningRuntimeState
  completedMode?: boolean
}) => {
  const selected = candidates[selectedId]
  const samplePhase = completedMode ? 'done' : planningRuntime.nodePhases.valuation
  const valuationProgress = completedMode ? 1 : planningRuntime.nodeProgress.valuation
  const valuationStarted = samplePhase !== 'idle'
  const sampleScored = completedMode || samplePhase === 'done' || (samplePhase === 'running' && valuationProgress >= 0.58)
  const sampleLoading = samplePhase === 'running' && !sampleScored
  const valuationCandidates = getValuationSearchCandidates(sampleScored)
  const [activeSearchKey, setActiveSearchKey] = useState(
    () => getValuationSearchCandidateByCandidateId(selectedId)?.key ?? valuationCandidates[0]?.key ?? '',
  )
  const activeSearchCandidate = getValuationSearchCandidateByKey(activeSearchKey) ?? valuationCandidates[0]

  useEffect(() => {
    const linked = getValuationSearchCandidateByCandidateId(selectedId)
    if (linked)
      setActiveSearchKey(linked.key)
  }, [selectedId])

  return (
    <section className="panel valuation-panel">
      <div className="panel-heading">
        <SectionTitle variant="valuation" title="Task-aware Data Valuation" />
      </div>

      <div className="valuation-workspace">
        <aside className="workspace-card candidate-column">
          <div className="card-header">
            <h3>Candidate Datasets</h3>
            {valuationStarted && <span className="valuation-count-pill">10 filtered</span>}
          </div>
          <div className="candidate-list">
            {!valuationStarted
              ? Array.from({ length: 4 }).map((_, index) => (
                  <div key={index} className="candidate-item candidate-item-placeholder" aria-hidden="true" />
                ))
              : valuationCandidates.map(candidate => (
                  <CandidateButton
                    key={candidate.key}
                    candidate={candidate}
                    active={candidate.key === activeSearchKey}
                    scored={sampleScored}
                    loading={sampleLoading}
                    onClick={() => {
                      setActiveSearchKey(candidate.key)
                      if (candidate.candidateId)
                        onSelectCandidate(candidate.candidateId)
                    }}
                  />
                ))}
          </div>
        </aside>

        <section className="workspace-card valuation-column">
          <ValuationCore
            selected={activeSearchCandidate}
            planningRuntime={planningRuntime}
            completedMode={completedMode}
            visible={valuationStarted}
          />
        </section>

        <aside className="workspace-card execution-column">
          <div className="card-header">
            <h3>On-chain Agent Trace</h3>
          </div>

          <OnChainAgentTrace
            candidate={selected}
            onTraceReviewCommit={onTraceReviewCommit}
            planningRuntime={planningRuntime}
            completedMode={completedMode}
          />
        </aside>
      </div>
    </section>
  )
}

export default ValuationPanel
