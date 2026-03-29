import { useEffect, useState, type CSSProperties, type PointerEventHandler, type Ref } from 'react'
import type { WorkflowNode } from '../data'

const easeOut = (t: number) => 1 - (1 - t) ** 2.4

const progressValue = (target: number, ratio: number) => {
  if (ratio <= 0)
    return 0

  return Math.min(target, Math.max(1, Math.round(target * easeOut(ratio))))
}

const STATIC_CODE_PREVIEWS: Record<string, string> = {
  'train_safehat.py': `from pathlib import Path
from ultralytics import YOLO

DATA_ROOT = Path("datasets/safehat")
DATA_YAML = DATA_ROOT / "safehat.yaml"


def build_model() -> YOLO:
    model = YOLO("yolov8n.pt")
    return model


def train() -> None:
    model = build_model()
    model.train(
        data=str(DATA_YAML),
        imgsz=640,
        epochs=30,
        batch=16,
        project="runs/safehat",
        name="helmet-detector",
    )


if __name__ == "__main__":
    print("launch preview for train_safehat.py")
    train()
`,
}

const trainingPreviewFor = (file: string) =>
  STATIC_CODE_PREVIEWS[file] ?? '# preview unavailable\n'

const parseEpochProgress = (stage: string) => {
  const match = stage.match(/(\d+)\s*\/\s*(\d+)/)
  if (!match)
    return { targetEpoch: 14, totalEpochs: 30 }

  return {
    targetEpoch: Number.parseInt(match[1], 10) || 14,
    totalEpochs: Number.parseInt(match[2], 10) || 30,
  }
}

const SearchIcon = () => (
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <circle cx="7" cy="7" r="4.25"></circle>
    <path d="M10.2 10.2L13.3 13.3"></path>
  </svg>
)

const FilterIcon = () => (
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <path d="M2.3 3h11.4L9.7 7.6v3.2l-3.4 2V7.6L2.3 3z"></path>
  </svg>
)

const ParserIcon = () => (
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <path d="M3 4.2h10"></path>
    <path d="M3 8h7.2"></path>
    <path d="M3 11.8h5.2"></path>
    <circle cx="11.8" cy="8" r="1.8"></circle>
  </svg>
)

const AgentTrail = () => (
  <span className="agent-run-trail" aria-hidden="true">
    <span></span>
    <span></span>
    <span></span>
  </span>
)

const ParserRunState = () => (
  <div className="agent-run-card parser">
    <div className="agent-run-head">
      <div className="agent-run-icon" aria-hidden="true"><ParserIcon /></div>
      <div className="agent-run-title">
        <span>Parsing</span>
        <AgentTrail />
      </div>
    </div>
    <div className="parser-run-body">
      <div className="parser-run-row">
        <span className="parser-run-label">Task Description</span>
        <div className="parser-run-bar-stack">
          <span className="parser-run-bar parser-run-bar-long"></span>
          <span className="parser-run-bar parser-run-bar-mid"></span>
        </div>
      </div>
      <div className="parser-run-row">
        <span className="parser-run-label">Task Type</span>
        <span className="parser-run-bar parser-run-bar-short"></span>
      </div>
      <div className="parser-run-row">
        <span className="parser-run-label">Keywords</span>
        <div className="parser-run-tags">
          <span></span>
          <span></span>
          <span></span>
        </div>
      </div>
    </div>
  </div>
)

const InlineRunState = ({
  label,
  tone,
}: {
  label: string
  tone: 'coding' | 'assessing'
}) => (
  <div className={`agent-run-card ${tone}`}>
    <div className="inline-run-shell">
      <div className="inline-run-status">
        <span className={`inline-run-star ${tone}`} aria-hidden="true">*</span>
        <span className="inline-run-text">{label}</span>
        <span className="inline-run-dots" aria-hidden="true">
          <span>.</span>
          <span>.</span>
          <span>.</span>
        </span>
      </div>
    </div>
  </div>
)

const CodeRunState = () => <InlineRunState label="Coding" tone="coding" />

const ValuationRunState = () => <InlineRunState label="Assessing" tone="assessing" />

const ExecutionRunState = ({
  stage,
}: {
  stage: string
}) => {
  const { targetEpoch, totalEpochs } = parseEpochProgress(stage)
  const [epoch, setEpoch] = useState(1)

  useEffect(() => {
    const duration = 4400
    const start = performance.now()

    const tick = () => {
      const elapsed = performance.now() - start
      const ratio = Math.min(elapsed / duration, 1)
      const nextEpoch = Math.max(1, Math.round(1 + (targetEpoch - 1) * easeOut(ratio)))
      setEpoch(nextEpoch)
    }

    tick()
    const timer = window.setInterval(tick, 110)
    return () => window.clearInterval(timer)
  }, [targetEpoch])

  const progress = Math.min(100, Math.round((epoch / totalEpochs) * 100))

  return (
    <div className="training-run-shell">
      <div className="training-run-line">
        <span className="training-run-label">
          epoch
          {' '}
          {String(epoch).padStart(2, '0')}
          /
          {totalEpochs}
        </span>
        <span className="training-run-track" aria-hidden="true">
          <span className="training-run-fill" style={{ width: `${progress}%` }}></span>
        </span>
        <span className="training-run-percent">{progress}%</span>
      </div>
    </div>
  )
}

const SearchStep = ({
  title,
  unit,
  value,
  icon,
  state,
}: {
  title: string
  unit: string
  value: number | null
  icon: JSX.Element
  state: 'idle' | 'pending' | 'active' | 'complete'
}) => (
  <div className={`search-step state-${state}`}>
    <div className="search-step-head">
      <div className="search-step-icon" aria-hidden="true">{icon}</div>
      <div className="search-step-title">
        <span>{title}</span>
        {state === 'active' && (
          <span className="search-trail" aria-hidden="true">
            <span></span>
            <span></span>
            <span></span>
          </span>
        )}
      </div>
    </div>
    <div className="search-step-value">
      <strong>{value === null ? '—' : value}</strong>
      <span>{unit}</span>
    </div>
  </div>
)

const SearchProcess = ({
  phase,
  totalResults,
  candidateCount,
}: {
  phase: 'idle' | 'running' | 'done'
  totalResults: number
  candidateCount: number
}) => {
  const [progress, setProgress] = useState({ results: 0, candidates: 0, stage: 'idle' as 'idle' | 'searching' | 'filtering' | 'done' })

  useEffect(() => {
    if (phase === 'idle') {
      setProgress({ results: 0, candidates: 0, stage: 'idle' })
      return
    }

    if (phase === 'done') {
      setProgress({ results: totalResults, candidates: candidateCount, stage: 'done' })
      return
    }

    const searchDuration = 1580
    const filterDelay = 1480
    const filterDuration = 1620
    const start = performance.now()

    const tick = () => {
      const elapsed = performance.now() - start
      const searchRatio = Math.min(elapsed / searchDuration, 1)
      const shouldFilter = elapsed >= filterDelay
      const filterRatio = !shouldFilter ? 0 : Math.min((elapsed - filterDelay) / filterDuration, 1)
      const removed = Math.round((totalResults - candidateCount) * easeOut(filterRatio))

      setProgress({
        results: progressValue(totalResults, searchRatio),
        candidates: shouldFilter ? Math.max(candidateCount, totalResults - removed) : totalResults,
        stage: shouldFilter ? 'filtering' : 'searching',
      })
    }

    tick()
    const timer = window.setInterval(tick, 90)
    return () => window.clearInterval(timer)
  }, [candidateCount, phase, totalResults])

  const searchingState
    = phase === 'idle'
      ? 'idle'
      : progress.stage === 'searching'
        ? 'active'
        : 'complete'

  const filteringState
    = phase === 'idle'
      ? 'idle'
      : progress.stage === 'searching'
        ? 'pending'
        : phase === 'done'
          ? 'complete'
          : 'active'

  const showSearching = phase !== 'idle'
  const showFiltering = progress.stage === 'filtering' || phase === 'done'

  return (
    <div className={`search-block phase-${phase}`}>
      {showSearching && (
        <SearchStep
          title="Searching"
          unit="results"
          value={progress.results}
          icon={<SearchIcon />}
          state={searchingState}
        />
      )}

      {showFiltering && (
        <>
          <div className={`search-connector ${phase === 'done' || progress.stage === 'filtering' ? 'active' : ''}`} aria-hidden="true">
            <span className="search-connector-line"></span>
            <span className="search-connector-dot"></span>
          </div>

          <SearchStep
            title="Filtering"
            unit="candidates"
            value={progress.candidates}
            icon={<FilterIcon />}
            state={filteringState}
          />
        </>
      )}
    </div>
  )
}

const WorkflowCard = ({
  node,
  selected = false,
  dragging = false,
  style,
  cardRef,
  onPointerDown,
}: {
  node: WorkflowNode & { status: { phase: 'idle' | 'running' | 'done' } }
  selected?: boolean
  dragging?: boolean
  style?: CSSProperties
  cardRef?: Ref<HTMLDivElement>
  onPointerDown?: PointerEventHandler<HTMLDivElement>
}) => {
  const [codePreviewOpen, setCodePreviewOpen] = useState(false)
  let body: JSX.Element

  useEffect(() => {
    if (!codePreviewOpen)
      return

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape')
        setCodePreviewOpen(false)
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [codePreviewOpen])

  if (node.content.kind === 'search' && node.status.phase === 'idle') {
    body = <div className="operator-placeholder idle"></div>
  } else if (node.content.kind === 'search') {
    body = (
      <SearchProcess
        phase={node.status.phase}
        totalResults={node.content.totalResults}
        candidateCount={node.content.candidateCount}
      />
    )
  } else if (node.content.kind === 'intent' && node.status.phase === 'running') {
    body = <ParserRunState />
  } else if (node.content.kind === 'code' && node.status.phase === 'running') {
    body = <CodeRunState />
  } else if (node.content.kind === 'valuation' && node.status.phase === 'running') {
    body = <ValuationRunState />
  } else if (node.content.kind === 'execution' && node.status.phase === 'running') {
    body = <ExecutionRunState stage={node.content.stage} />
  } else if (node.status.phase !== 'done') {
    body = (
      <div className={`operator-placeholder ${node.status.phase}`}>
        {node.status.phase === 'running' && (
          <div className="running-dots" aria-hidden="true">
            <span></span>
            <span></span>
            <span></span>
          </div>
        )}
      </div>
    )
  } else {
    switch (node.content.kind) {
      case 'query':
        body = (
          <div className="query-block">
            <p>{node.content.query}</p>
            <div className="source-badge-row">
              {node.content.sources.map(source => <span key={source} className="mini-pill">{source}</span>)}
            </div>
          </div>
        )
        break
      case 'intent':
        body = (
          <div className="intent-block">
            <div className="intent-section">
              <span className="mini-title">Task Description</span>
              <div className="task-description-box">
                <p>{node.content.taskDescription}</p>
              </div>
            </div>
            <div className="intent-section">
              <span className="mini-title">Task Type</span>
              <div className="profile-pill-row">
                <span className="profile-pill">{node.content.taskType}</span>
              </div>
            </div>
            <div className="intent-section">
              <span className="mini-title">Keywords</span>
              <div className="keyword-cloud compact">
                {node.content.keywords.map(item => <span key={item}>{item}</span>)}
              </div>
            </div>
          </div>
        )
        break
      case 'code':
        body = (
          <div className="code-stack">
            <div className="code-summary-line">
              <span className="code-summary-text">
                {node.content.filesChanged}
                {' '}
                {node.content.filesChanged === 1 ? 'file changed' : 'files changed'}
              </span>
              <span className="code-diff-plus">
                +
                {node.content.addedLines}
              </span>
              {node.content.removedLines
                ? (
                    <span className="code-diff-minus">
                      -
                      {node.content.removedLines}
                    </span>
                  )
                : null}
            </div>
            <div
              className="code-file-line interactive"
              role="button"
              tabIndex={0}
              title="Click to preview generated code"
              onPointerDown={event => event.stopPropagation()}
              onClick={(event) => {
                event.stopPropagation()
                setCodePreviewOpen(true)
              }}
              onDoubleClick={event => event.stopPropagation()}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault()
                  event.stopPropagation()
                  setCodePreviewOpen(true)
                }
              }}
            >
              <code>{node.content.file}</code>
            </div>
          </div>
        )
        break
      case 'valuation':
        body = (
          <div className="valuation-node">
            <div className="valuation-pill">selected asset</div>
            <strong>{node.content.selected}</strong>
            <p>{node.content.detail}</p>
            <span className="valuation-action">{node.content.action}</span>
          </div>
        )
        break
      case 'execution':
        body = (
          <div className="execution-node">
            <div className="inline-metric">
              <span>stage</span>
              <strong>{node.content.stage}</strong>
            </div>
            <div className="inline-metric">
              <span>result</span>
              <strong>{node.content.result}</strong>
            </div>
          </div>
        )
        break
      case 'ledger':
        body = (
          <div className="ledger-node">
            {node.content.items.map(item => (
              <div key={item} className="inline-metric">
                <span>record</span>
                <strong>{item}</strong>
              </div>
            ))}
          </div>
        )
        break
    }
  }

  const previewCode = node.content.kind === 'code' ? trainingPreviewFor(node.content.file) : ''

  return (
    <>
      <div
        ref={cardRef}
        className={`operator-card ${node.content.kind} phase-${node.status.phase}${selected ? ' selected' : ''}${dragging ? ' dragging' : ''}`}
        data-accent={node.accent}
        style={style}
        onPointerDown={onPointerDown}
      >
        <span className="drag-handle" aria-hidden="true">⋮⋮</span>

        <div className="operator-head">
          <div className={`operator-icon ${node.accent}`}>{node.badge}</div>
          <h3>{node.title}</h3>
        </div>

        {body}
      </div>

      {codePreviewOpen && node.content.kind === 'code' && (
        <div className="code-preview-overlay" onClick={() => setCodePreviewOpen(false)}>
          <div
            className="code-preview-modal"
            onClick={event => event.stopPropagation()}
            onPointerDown={event => event.stopPropagation()}
          >
            <div className="code-preview-header">
              <div className="code-preview-header-copy">
                <h4>{node.content.file}</h4>
              </div>
              <button
                type="button"
                className="code-preview-close"
                onClick={() => setCodePreviewOpen(false)}
              >
                Close
              </button>
            </div>

            <div className="code-preview-body">
              <div className="code-preview-gutter" aria-hidden="true">
                {previewCode.split('\n').map((_, index) => (
                  <span key={index}>{index + 1}</span>
                ))}
              </div>
              <pre className="code-preview-content">
                <code>{previewCode}</code>
              </pre>
            </div>
          </div>
        </div>
      )}
    </>
  )
}

export default WorkflowCard
