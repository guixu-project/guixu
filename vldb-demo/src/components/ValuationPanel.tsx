import type { Candidate, CandidateId } from '../data'
import { candidates } from '../data'
import SectionTitle from './SectionTitle'

const candidateOrder = Object.keys(candidates) as CandidateId[]
const scoreSignals = ['Code Compatibility', 'Annotation Quality', 'Task Relevance', 'On-chain Reputation']

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

const platformLabels: Record<string, string> = {
  'guixu-hub': 'GUIXU HUB',
  'kaggle': 'KAGGLE',
  'huggingface': 'HUGGINGFACE',
  'roboflow': 'ROBOFLOW',
  'torrent': 'TORRENT',
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
    <span className="candidate-meta-text">{candidate.size} · {candidate.cost} · {candidate.reviewCount} reviews</span>
  </button>
)

const ValuationPanel = ({
  selectedId,
  onSelectCandidate,
}: {
  selectedId: CandidateId
  onSelectCandidate: (candidateId: CandidateId) => void
}) => {
  const selected = candidates[selectedId]
  const selectedRadar = radarPoints(selected.radar)
  const signalMetrics = selected.metrics.filter(metric => scoreSignals.includes(metric.label))
  const utilityMetric = selected.metrics.find(metric => metric.label === 'Fast-DataShapley Value')

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
                <polygon points="110,26 176,74 151,152 69,152 44,74" className="radar-grid"></polygon>
                <polygon points="110,46 158,82 140,140 80,140 62,82" className="radar-grid inner"></polygon>
                <polygon points={selectedRadar} className="radar-shape"></polygon>
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
                      <div className="metric-fill" style={{ width: `${metric.value}%`, background: metric.color }}></div>
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
          <div className="recommendation-box">
            <p className="label">Recommendation</p>
            <h3>{selected.name}</h3>
            <div className="recommendation-grid">
              <span>Source</span><strong>{selected.source}</strong>
              <span>Cost</span><strong>{selected.cost}</strong>
              <span>Decision</span><strong>{selected.confidence}</strong>
            </div>
          </div>

          <div className="execution-box">
            <div className="card-header">
              <h3>Execution</h3>
              <span className="status-chip">demo run</span>
            </div>

            <div className="step-list compact">
              {selected.steps.map(step => (
                <div key={step.label} className={`step-item ${step.status}`}>
                  <span className="step-dot"></span>
                  <span>{step.label}</span>
                </div>
              ))}
            </div>

            <div className="execution-log compact">
              {selected.logs.slice(0, 3).map(line => <div key={line} className="log-line">{line}</div>)}
            </div>

            <div className="outcome-grid">
              <div className="outcome-card">
                <span>Baseline</span>
                <strong>{selected.outcome.baseline}</strong>
              </div>
              <div className="outcome-card highlight">
                <span>Selected</span>
                <strong>{selected.outcome.selected}</strong>
              </div>
              <div className="outcome-card gain">
                <span>Gain</span>
                <strong>{selected.outcome.gain}</strong>
              </div>
            </div>
          </div>
        </aside>
      </div>
    </section>
  )
}

export default ValuationPanel
