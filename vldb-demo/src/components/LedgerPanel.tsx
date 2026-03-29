import { useEffect, useMemo, useState } from 'react'
import { transactionRows } from '../data'
import SectionTitle from './SectionTitle'

const avatarImages: string[] = [
  new URL('../../assets/floki-logo.svg', import.meta.url).href,
  new URL('../../assets/dogecoin.svg', import.meta.url).href,
  new URL('../../assets/shiba-inu.svg', import.meta.url).href,
  new URL('../../assets/pepe.svg', import.meta.url).href,
  new URL('../../assets/meme.svg', import.meta.url).href,
]

interface HubDataset {
  id: string
  seller_address: string
  title: string
  description: string
  data_type: string
  status: string
  schema: { columns: unknown[]; row_count: number; size_bytes: number }
  metrics: { download_count: number; review_count: number; trade_count: number }
  price: { amount: number; currency: string; label: string; is_free: boolean }
  tags: string[]
  created_at: string
  updated_at: string
}

interface HubReview {
  id: string
  reviewer_address: string
  content: string
  source: string
  tx_hash: string | null
  created_at: string
}

const HUB_API = '/api/hub/datasets'

function maskAddress(addr: string) {
  if (addr.length <= 18)
    return addr

  return `${addr.slice(0, 10)}******${addr.slice(-8)}`
}

function formatTimeAgo(iso: string) {
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

const LedgerPanel = () => {
  const [datasets, setDatasets] = useState<HubDataset[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [activeDatasetId, setActiveDatasetId] = useState<string | null>(null)
  const [activeHistoryIndex, setActiveHistoryIndex] = useState(0)
  const [revealedSellerId, setRevealedSellerId] = useState<string | null>(null)
  const [reviews, setReviews] = useState<HubReview[]>([])
  const [reviewsLoading, setReviewsLoading] = useState(false)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    fetch(HUB_API)
      .then(res => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        return res.json() as Promise<HubDataset[]>
      })
      .then(data => { if (!cancelled) setDatasets(data) })
      .catch(err => { if (!cancelled) setError(err.message) })
      .finally(() => { if (!cancelled) setLoading(false) })
    return () => { cancelled = true }
  }, [])

  useEffect(() => {
    if (!datasets.length) {
      setActiveDatasetId(null)
      return
    }

    setActiveDatasetId(prev => (
      prev && datasets.some(dataset => dataset.id === prev)
        ? prev
        : datasets[0].id
    ))
  }, [datasets])

  useEffect(() => {
    setActiveHistoryIndex(0)
    setRevealedSellerId(null)
  }, [activeDatasetId])

  // Fetch reviews from real API when active dataset changes
  useEffect(() => {
    if (!activeDatasetId) {
      setReviews([])
      return
    }
    let cancelled = false
    setReviewsLoading(true)
    fetch(`/api/reviews?listingId=${activeDatasetId}`)
      .then(res => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        return res.json() as Promise<HubReview[]>
      })
      .then(data => { if (!cancelled) setReviews(data) })
      .catch(() => { if (!cancelled) setReviews([]) })
      .finally(() => { if (!cancelled) setReviewsLoading(false) })
    return () => { cancelled = true }
  }, [activeDatasetId])

  const toggleSeller = (datasetId: string) => {
    setRevealedSellerId(prev => (prev === datasetId ? null : datasetId))
  }

  const activeDataset = useMemo(
    () => datasets.find(dataset => dataset.id === activeDatasetId) ?? datasets[0] ?? null,
    [activeDatasetId, datasets],
  )

  const datasetTitle = activeDataset?.title ?? 'Dataset'
  const historyRows = useMemo(() => {
    return transactionRows.map((row, index) => {
      if (index === 0)
        return { ...row, detail: `agent purchased ${datasetTitle}` }
      if (index === 1)
        return { ...row, detail: `seller registered ${datasetTitle}` }
      if (index === 2)
        return { ...row, detail: `positive task-fit feedback recorded for ${datasetTitle}` }
      return { ...row, detail: `archived training trace for ${datasetTitle}` }
    })
  }, [datasetTitle])

  return (
    <section className="panel ledger-panel">
      <div className="panel-heading">
        <SectionTitle variant="ledger" title="Decentralized Ledger & Feedback" />
      </div>

      <div className="ledger-grid">
        <section className="ledger-card">
          <div className="card-header">
            <div>
              <h3>Guixu Hub</h3>
            </div>
            <div className="mini-controls"><span></span><span></span><span></span></div>
          </div>
          <div className="table-scroll-area vertical">
            <table className="data-table marketplace-table">
              <colgroup>
                <col className="dataset-col" />
                <col className="trade-col" />
                <col className="price-col" />
                <col className="seller-col" />
              </colgroup>
              <thead>
                <tr>
                  <th>Dataset</th>
                  <th>Trade</th>
                  <th>Price</th>
                  <th>Seller</th>
                </tr>
              </thead>
              <tbody>
                {loading && (
                  <tr><td colSpan={4} style={{ textAlign: 'center', opacity: 0.5 }}>Loading from Guixu Hub...</td></tr>
                )}
                {error && (
                  <tr><td colSpan={4} style={{ textAlign: 'center', color: '#c0392b' }}>Failed: {error}</td></tr>
                )}
                {!loading && !error && datasets.map(ds => (
                  <tr
                    key={ds.id}
                    className={`interactive-row${ds.id === activeDatasetId ? ' highlight-row' : ''}`}
                    onClick={() => setActiveDatasetId(ds.id)}
                  >
                    <td className="dataset-cell"><strong>{ds.title}</strong></td>
                    <td className="trade-cell">
                      <span className="count-pill">{ds.metrics.trade_count.toLocaleString()}</span>
                    </td>
                    <td>
                      <span className={`price-pill${ds.price.is_free ? ' free' : ''}`}>
                        {ds.price.is_free ? 'Free' : ds.price.label}
                      </span>
                    </td>
                    <td className="address-cell">
                      <button
                        type="button"
                        className={`seller-address-button${revealedSellerId === ds.id ? ' revealed' : ''}`}
                        onClick={(event) => {
                          event.stopPropagation()
                          toggleSeller(ds.id)
                        }}
                        title={revealedSellerId === ds.id ? ds.seller_address : 'Click to reveal full seller address'}
                        aria-expanded={revealedSellerId === ds.id}
                      >
                        <span className="seller-address-text">{maskAddress(ds.seller_address)}</span>
                        {revealedSellerId === ds.id && (
                          <span className="seller-address-popover">{ds.seller_address}</span>
                        )}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="ledger-card">
          <div className="card-header">
            <div>
              <h3>Transaction History</h3>
            </div>
            <div className="mini-controls"><span></span><span></span><span></span></div>
          </div>
          <div className="table-scroll-area vertical">
            <table className="data-table history-table">
              <thead>
                <tr>
                  <th>Timestamp</th>
                  <th>Event Type</th>
                  <th>Tx Hash</th>
                  <th>Details</th>
                </tr>
              </thead>
              <tbody>
                {historyRows.map((row, index) => (
                  <tr
                    key={`${row.time}-${row.hash}`}
                    className={`interactive-row${index === activeHistoryIndex ? ' highlight-row' : ''}`}
                    onClick={() => setActiveHistoryIndex(index)}
                  >
                    <td>{row.time}</td>
                    <td>{row.event}</td>
                    <td>{row.hash}</td>
                    <td>{row.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="ledger-card">
          <div className="card-header">
            <h3>Feedback &amp; Reviews</h3>
          </div>
          <div className="review-stack">
            {reviewsLoading && (
              <p style={{ textAlign: 'center', opacity: 0.5, padding: '16px' }}>Loading reviews...</p>
            )}
            {!reviewsLoading && reviews.length === 0 && (
              <p style={{ textAlign: 'center', opacity: 0.5, padding: '16px' }}>No reviews yet for this dataset.</p>
            )}
            {!reviewsLoading && reviews.map((review: HubReview, index: number) => {
              const avatarSrc = index < avatarImages.length ? avatarImages[index] : null
              return (
              <article key={review.id} className={`review-card${index === 0 ? ' highlighted' : ''}`}>
                <div className={`review-avatar${review.source === 'on-chain' ? ' agent' : review.source === 'user' ? ' human' : ''}`}>
                  {avatarSrc ? (
                    <img src={avatarSrc} alt="" className="review-avatar-img" />
                  ) : (
                    <span className="review-avatar-placeholder" />
                  )}
                </div>
                <div>
                  <h4>{maskAddress(review.reviewer_address)}</h4>
                  <p className="review-text">{review.content}</p>
                </div>
                <div className="stars">{review.source === 'on-chain' ? 'On-chain memo' : `User · ${formatTimeAgo(review.created_at)}`}</div>
              </article>
            )
            })}
          </div>
        </section>
      </div>
    </section>
  )
}

export default LedgerPanel
