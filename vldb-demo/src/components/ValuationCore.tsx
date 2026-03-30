import { useEffect, useState } from 'react'
import type { PlanningRuntimeState } from '../demoTimeline'
import type { ValuationSearchCandidate } from '../valuationDemo'
import { knapsackDisplay } from '../valuationDemo'
const scatterIconSrc = new URL('../../assets/Scatter.svg', import.meta.url).href

const SampleGlyph = ({ kind }: { kind: 'dataset' | 'oracle' | 'records' | 'mean' }) => {
  switch (kind) {
    case 'dataset':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 8.5 12 5l8 3.5-8 3.5z" />
          <path d="M4 12.5 12 16l8-3.5M4 16.5 12 20l8-3.5" />
        </svg>
      )
    case 'oracle':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="3.5" />
          <path d="M12 4v2.4M12 17.6V20M4 12h2.4M17.6 12H20M6.3 6.3l1.7 1.7M16 16l1.7 1.7M17.7 6.3 16 8M8 16l-1.7 1.7" />
        </svg>
      )
    case 'mean':
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 18h16" />
          <path d="M7 15.5 10.5 11l3 3 3.5-6" />
          <circle cx="7" cy="15.5" r="1.2" />
          <circle cx="10.5" cy="11" r="1.2" />
          <circle cx="13.5" cy="14" r="1.2" />
          <circle cx="17" cy="8" r="1.2" />
        </svg>
      )
  }
}

const ValuationCore = ({
  selected,
  planningRuntime,
  completedMode = false,
  visible = true,
}: {
  selected: ValuationSearchCandidate
  planningRuntime: PlanningRuntimeState
  completedMode?: boolean
  visible?: boolean
}) => {
  const samplePhase = completedMode ? 'done' : planningRuntime.nodePhases.valuation
  const valuationProgress = completedMode ? 1 : planningRuntime.nodeProgress.valuation
  const sampleStarted = samplePhase !== 'idle'
  const sampleDone = completedMode || samplePhase === 'done' || (samplePhase === 'running' && valuationProgress >= 0.58)
  const sampleProgress = sampleDone ? 1 : Math.max(0, Math.min(1, valuationProgress / 0.58))
  const sampleStage = !sampleStarted ? -1 : sampleDone ? 4 : sampleProgress < 0.22 ? 1 : sampleProgress < 0.48 ? 2 : sampleProgress < 0.78 ? 3 : 4
  const knapsackActive = sampleDone
  const knapsackProgress = samplePhase === 'done'
    ? 1
    : !knapsackActive
        ? 0
        : Math.max(0, Math.min(1, (valuationProgress - 0.58) / 0.42))
  const knapsackStage = !knapsackActive ? -1 : knapsackProgress < 0.35 ? 0 : knapsackProgress < 0.74 ? 1 : 2
  const [activeRoundIndex, setActiveRoundIndex] = useState(0)

  useEffect(() => {
    setActiveRoundIndex(knapsackStage >= 1 ? 1 : 0)
  }, [knapsackStage])

  const currentRound = knapsackDisplay.rounds[activeRoundIndex]

  if (!visible) {
    return (
      <div className="valuation-core-shell">
        <div className="valuation-sample-panel">
          <div className="valuation-panel-head">
            <div>
              <h4 className="valuation-panel-title">Sample Scoring</h4>
            </div>
          </div>
          <div className="valuation-empty-board" aria-hidden="true" />
        </div>

        <div className="valuation-bundle-panel">
          <div className="valuation-panel-head">
            <div>
              <h4 className="valuation-panel-title">Knapsack Optimization</h4>
            </div>
          </div>
          <div className="valuation-empty-board" aria-hidden="true" />
        </div>
      </div>
    )
  }

  return (
    <div className="valuation-core-shell">
      <div className="valuation-sample-panel">
        <div className="valuation-panel-head">
          <div>
            <h4 className="valuation-panel-title">Sample Scoring</h4>
          </div>
        </div>

        <div className="sample-inline-flow">
          <div className={`sample-inline-node${sampleStarted ? ' done' : ''}`}>
            <div className="sample-inline-icon dataset">
              <SampleGlyph kind="dataset" />
            </div>
            <strong>Sampled Records</strong>
          </div>

          <div className="sample-inline-arrow" aria-hidden="true">→</div>

          <div className={`sample-inline-node${sampleStage === 1 ? ' current' : sampleStage > 1 ? ' done' : ''}`}>
            <div className="sample-inline-icon oracle">
              <SampleGlyph kind="oracle" />
            </div>
            <strong>Seed Scoring</strong>
          </div>

          <div className="sample-inline-arrow" aria-hidden="true">→</div>

          <div className={`sample-inline-node anchors${sampleStage === 2 ? ' current' : sampleStage > 2 ? ' done' : ''}`}>
            <div className="sample-inline-icon anchors-icon">
              <span className="anchor-boxplot-glyph large" aria-hidden="true">
                <span className="anchor-whisker" />
                <span className="anchor-box" />
                <span className="anchor-median" />
              </span>
            </div>
            <strong>Score Anchors</strong>
          </div>

          <div className="sample-inline-arrow" aria-hidden="true">→</div>

          <div className={`sample-inline-node decision${sampleStage === 3 ? ' current' : sampleStage > 3 ? ' done' : ''}`}>
            <div className="sample-inline-icon records">
              <img src={scatterIconSrc} alt="" className="sample-inline-icon-image" />
            </div>
            <strong>Propagation</strong>
          </div>

          <div className="sample-inline-arrow" aria-hidden="true">→</div>

          <div className={`sample-inline-node mean${sampleStage === 4 ? ' current' : sampleDone ? ' done' : ''}`}>
            <div className="sample-inline-icon result">
              <SampleGlyph kind="mean" />
            </div>
            <strong>Result</strong>
          </div>
        </div>
      </div>

      <div className="valuation-bundle-panel">
        <div className="valuation-panel-head">
          <div>
            <h4 className="valuation-panel-title">Knapsack Optimization</h4>
          </div>
        </div>

        <div className="knapsack-board">
          <div className="knapsack-constraint-stack">
            {knapsackDisplay.constraints.map(constraint => (
              <span key={constraint.label} className="bundle-constraint">
                <strong>{constraint.label}</strong>
                <span>{constraint.value}</span>
              </span>
            ))}
          </div>

          <div className="knapsack-tabs" role="tablist" aria-label="Knapsack rounds">
            {knapsackDisplay.rounds.map((round, index) => {
              const unlocked = index === 0 || (index === 1 && knapsackStage >= 1) || (index === 2 && knapsackStage >= 2)
              return (
                <button
                  key={round.label}
                  className={`knapsack-tab${activeRoundIndex === index ? ' active' : ''}`}
                  type="button"
                  role="tab"
                  aria-selected={activeRoundIndex === index}
                  disabled={!unlocked}
                  onClick={() => unlocked && setActiveRoundIndex(index)}
                >
                  {round.label}
                </button>
              )
            })}
          </div>

          <div className={`knapsack-table-shell${activeRoundIndex === 1 ? ' feasible' : ''}`}>
            <div className="knapsack-mini-table">
              <div className="knapsack-mini-head">
                <span>Dataset</span>
                <span>Score</span>
                <span>Price</span>
                <span>Size</span>
              </div>
              {currentRound.rows.length > 0
                ? currentRound.rows.map(row => (
                    <div key={row.name} className={`knapsack-mini-row${row.chosen ? ' chosen' : ''}`}>
                      <span>{row.name}</span>
                      <span>{row.score}</span>
                      <span>{row.price}</span>
                      <span>{row.size}</span>
                    </div>
                  ))
                : (
                    <div className="knapsack-empty-row">
                      {currentRound.placeholder ?? 'no candidates'}
                    </div>
                  )}
            </div>
          </div>

          <div className={`knapsack-portfolio${knapsackStage >= 2 ? ' ready' : ''}`}>
            <div className="knapsack-portfolio-head">
              <strong>Selected Portfolio</strong>
              <span>{knapsackDisplay.selected.totalPrice} · {knapsackDisplay.selected.totalSize}</span>
            </div>
            <div className="knapsack-portfolio-list">
              {knapsackDisplay.selected.datasets.map(dataset => (
                <span key={dataset.name} className="knapsack-portfolio-chip">
                  {dataset.name}
                  <strong>{dataset.score}</strong>
                </span>
              ))}
            </div>
            <div className="knapsack-portfolio-total">
              <span>Total utility</span>
              <strong>{knapsackDisplay.selected.totalScore}</strong>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export default ValuationCore
