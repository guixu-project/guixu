/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { useEffect, useMemo, useRef, useState } from 'react'
import { candidates, paperExportDatasetId, type CandidateId, type MarketReview } from '../data'
import SectionTitle from './SectionTitle'

const candidateIds = Object.keys(candidates) as CandidateId[]

const avatarImages: string[] = [
  new URL('../../assets/Adventure_Time_Profile_Pictures/A_Gunter.png', import.meta.url).href,
  new URL('../../assets/Adventure_Time_Profile_Pictures/A_Ice_King.png', import.meta.url).href,
  new URL('../../assets/Adventure_Time_Profile_Pictures/A_Princess_Bubblegum.png', import.meta.url).href,
  new URL('../../assets/Adventure_Time_Profile_Pictures/A_Pepperment_Butler.png', import.meta.url).href,
]

interface HubDataset {
  id: string
  seller_address: string
  contract_address: string
  payment_token: string
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

const HUB_API = '/api/hub/datasets'
const CONTRACT_HISTORY_API = '/api/contracts/history'
const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000'
const HISTORY_CACHE_TTL_MS = 10 * 60 * 1000
const HISTORY_CACHE_STORAGE_KEY = 'vldb-demo:contract-history-cache'
const TOKEN_META: Record<string, { symbol: string; decimals: number }> = {
  eth: { symbol: 'ETH', decimals: 18 },
  usdc: { symbol: 'USDC', decimals: 6 },
  usdt: { symbol: 'USDT', decimals: 6 },
  '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913': { symbol: 'USDC', decimals: 6 },
  '0xfde4c96c8593536e31f229ea8f37b2ada2699bb2': { symbol: 'USDT', decimals: 6 },
}

interface ContractHistoryEvent {
  event?: string
  tx_hash?: string
  block_number?: number
  timestamp?: string | null
  buyer?: string
  payment_token?: string
  amount_wei?: string
  error?: string
}

interface ContractHistoryResponse {
  listing_id: string
  title: string
  seller_address: string
  contract_address: string
  price_wei: string
  payment_token: string
  contract_balance_wei?: string
  events: ContractHistoryEvent[]
}

interface HistoryRow {
  time: string
  timeTitle: string
  eventType: string
  eventTone: 'purchased' | 'released'
  buyer: string
  buyerTitle: string
  value: string
  txHash: string
  txShort: string
}

interface HistoryCacheEntry {
  rows: HistoryRow[]
  fetchedAt: number
}

const PAPER_USDC_ADDRESS = '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913'

const paperDatasets: HubDataset[] = [
  {
    id: paperExportDatasetId,
    seller_address: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
    contract_address: '0x91de13c091de13c091de13c091de13c091de13c0',
    payment_token: PAPER_USDC_ADDRESS,
    title: 'SafeHat_Premium',
    description: 'Task-fit construction helmet detection dataset with strong labels and on-chain trade memory.',
    data_type: 'image',
    status: 'active',
    schema: { columns: [], row_count: 50000, size_bytes: 2469606195 },
    metrics: { download_count: 128, review_count: 32, trade_count: 32 },
    price: { amount: 10, currency: 'USDC', label: '$10', is_free: false },
    tags: ['helmet', 'construction', 'bbox'],
    created_at: '2026-03-24T10:26:00Z',
    updated_at: '2026-03-29T10:43:00Z',
  },
  {
    id: 'paper-warehouse-ppe',
    seller_address: '0x3d4e2a6283d0a5f99d9fa7d65f85740c39a52a44',
    contract_address: '',
    payment_token: PAPER_USDC_ADDRESS,
    title: 'Warehouse_PPE_Set',
    description: 'Private seller PPE detection set with moderate task fit.',
    data_type: 'image',
    status: 'active',
    schema: { columns: [], row_count: 32000, size_bytes: 1691143372 },
    metrics: { download_count: 74, review_count: 18, trade_count: 18 },
    price: { amount: 7, currency: 'USDC', label: '$7', is_free: false },
    tags: ['ppe', 'warehouse'],
    created_at: '2026-03-23T09:58:00Z',
    updated_at: '2026-03-28T09:58:00Z',
  },
  {
    id: 'paper-construction-images-v2',
    seller_address: '0x9aa52f2f6b3f3a4dbb7f7c61e9a95a40f6ef5552',
    contract_address: '',
    payment_token: PAPER_USDC_ADDRESS,
    title: 'Construction_Images_v2',
    description: 'Public construction image set with broader but noisier labels.',
    data_type: 'image',
    status: 'active',
    schema: { columns: [], row_count: 61000, size_bytes: 5476083301 },
    metrics: { download_count: 196, review_count: 20, trade_count: 20 },
    price: { amount: 5, currency: 'USDC', label: '$5', is_free: false },
    tags: ['construction', 'public'],
    created_at: '2026-03-21T08:20:00Z',
    updated_at: '2026-03-27T08:20:00Z',
  },
]

const paperHistoryRows: HistoryRow[] = [
  {
    time: 'Mar 29, 10:43',
    timeTitle: '2026 Mar 29 10:43:00',
    eventType: 'RELEASE',
    eventTone: 'released',
    buyer: '0x72bc...881f',
    buyerTitle: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
    value: '10 USDC',
    txHash: '0xa4cf991ea4cf991ea4cf991ea4cf991ea4cf991ea4cf991ea4cf991ea4cf991e',
    txShort: '0xa4cf...991e',
  },
  {
    time: 'Mar 29, 10:34',
    timeTitle: '2026 Mar 29 10:34:00',
    eventType: 'PURCHASE',
    eventTone: 'purchased',
    buyer: '0x72bc...881f',
    buyerTitle: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
    value: '10 USDC',
    txHash: '0x91de13c091de13c091de13c091de13c091de13c091de13c091de13c091de13c0',
    txShort: '0x91de...13c0',
  },
  {
    time: 'Mar 29, 10:26',
    timeTitle: '2026 Mar 29 10:26:00',
    eventType: 'RELEASE',
    eventTone: 'released',
    buyer: '0xd80e...296e',
    buyerTitle: '0xd80e5bfc2c32ebc209bd91bc234d63aeeda0296e',
    value: '0 USDC',
    txHash: '0x81afbe4281afbe4281afbe4281afbe4281afbe4281afbe4281afbe4281afbe42',
    txShort: '0x81af...be42',
  },
]

const paperReviews: MarketReview[] = [
  {
    id: 'paper-review-1',
    reviewer_address: '0x81afbe4281afbe4281afbe4281afbe4281afbe42',
    content: 'High annotation quality and strong construction-scene coverage for helmet detection.',
    source: 'user',
    tx_hash: null,
    created_at: '2026-03-29T10:44:00Z',
  },
  {
    id: 'paper-review-2',
    reviewer_address: '0x72bc4e34c7f08e2f4bb1a413d9c8a3bfa2fd881f',
    content: 'Stable improvement under a tight budget; became the best candidate in valuation.',
    source: 'on-chain',
    tx_hash: null,
    created_at: '2026-03-29T10:45:00Z',
  },
]

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

function shortenHash(hash: string) {
  if (!hash)
    return '—'

  return `${hash.slice(0, 10)}...${hash.slice(-8)}`
}

function shortenAddress(addr: string) {
  if (!addr)
    return '—'

  return `${addr.slice(0, 8)}...${addr.slice(-8)}`
}

function loadHistoryCache() {
  if (typeof window === 'undefined')
    return new Map<string, HistoryCacheEntry>()

  try {
    const raw = window.sessionStorage.getItem(HISTORY_CACHE_STORAGE_KEY)
    if (!raw)
      return new Map<string, HistoryCacheEntry>()

    const parsed = JSON.parse(raw) as Array<[string, HistoryCacheEntry]>
    const now = Date.now()

    return new Map(
      parsed.filter((entry): entry is [string, HistoryCacheEntry] => {
        const [, value] = entry
        return Array.isArray(value?.rows) && typeof value?.fetchedAt === 'number' && now - value.fetchedAt < HISTORY_CACHE_TTL_MS
      }),
    )
  } catch {
    return new Map<string, HistoryCacheEntry>()
  }
}

function persistHistoryCache(cache: Map<string, HistoryCacheEntry>) {
  if (typeof window === 'undefined')
    return

  try {
    window.sessionStorage.setItem(HISTORY_CACHE_STORAGE_KEY, JSON.stringify(Array.from(cache.entries())))
  } catch {}
}

function formatHistoryTime(iso: string | null | undefined) {
  if (!iso)
    return { text: '—', title: '' }

  const date = new Date(iso)
  return {
    text: date.toLocaleString('en-US', {
      month: 'short',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    }),
    title: date.toLocaleString('en-US', {
      year: 'numeric',
      month: 'short',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    }),
  }
}

function formatTokenAmount(raw: string | undefined, token: string | undefined, fallbackToken: string | undefined) {
  if (!raw || !/^\d+$/.test(raw))
    return '—'

  const normalizedToken = (token || fallbackToken || 'ETH').toLowerCase()
  const meta = TOKEN_META[normalizedToken] ?? TOKEN_META[normalizedToken === ZERO_ADDRESS ? 'eth' : ''] ?? TOKEN_META.eth
  const padded = raw.padStart(meta.decimals + 1, '0')
  const whole = padded.slice(0, -meta.decimals).replace(/^0+(?=\d)/, '')
  const fraction = padded.slice(-meta.decimals).replace(/0+$/, '').slice(0, meta.decimals === 18 ? 6 : 4)
  const amount = fraction ? `${whole}.${fraction}` : whole

  return `${amount} ${meta.symbol}`
}

function hasOnChainContract(address: string | null | undefined) {
  return Boolean(address && /^0x[a-fA-F0-9]{40}$/.test(address) && address.toLowerCase() !== ZERO_ADDRESS)
}

function mapEventType(event: string | undefined): Pick<HistoryRow, 'eventType' | 'eventTone'> {
  if (event === 'Released')
    return { eventType: 'RELEASE', eventTone: 'released' }

  return { eventType: 'PURCHASE', eventTone: 'purchased' }
}

function normalizeHistoryRows(payload: ContractHistoryResponse | null): HistoryRow[] {
  const events = Array.isArray(payload?.events) ? payload.events : []

  return [...events]
    .filter(event => ['Purchased', 'Released'].includes(event.event ?? '') && event.tx_hash)
    .sort((left, right) => {
      const leftTime = left.timestamp ? new Date(left.timestamp).getTime() : 0
      const rightTime = right.timestamp ? new Date(right.timestamp).getTime() : 0
      return rightTime - leftTime
    })
    .map((event) => {
      const time = formatHistoryTime(event.timestamp)
      const mapped = mapEventType(event.event)

      return {
        time: time.text,
        timeTitle: time.title,
        eventType: mapped.eventType,
        eventTone: mapped.eventTone,
        buyer: shortenAddress(event.buyer ?? ''),
        buyerTitle: event.buyer ?? '',
        value: formatTokenAmount(event.amount_wei, event.payment_token, payload?.payment_token),
        txHash: event.tx_hash ?? '',
        txShort: shortenHash(event.tx_hash ?? ''),
      }
    })
}

const normalizeLookup = (value: string) => value.toLowerCase().replace(/[^a-z0-9]/g, '')

const datasetMatchesCandidate = (dataset: HubDataset, candidateId: CandidateId) => {
  const candidateKeys = [candidateId, candidates[candidateId].name].map(normalizeLookup)
  const datasetKeys = [dataset.id, dataset.title].map(normalizeLookup)

  return candidateKeys.some(candidateKey => datasetKeys.some(datasetKey =>
    datasetKey === candidateKey || datasetKey.includes(candidateKey) || candidateKey.includes(datasetKey),
  ))
}

const LedgerPanel = ({
  selectedCandidateId,
  sessionReviewsByDatasetId,
  onActiveDatasetChange,
  paperMode = false,
}: {
  selectedCandidateId: CandidateId
  sessionReviewsByDatasetId: Record<string, MarketReview>
  onActiveDatasetChange?: (datasetId: string | null) => void
  paperMode?: boolean
}) => {
  const [datasets, setDatasets] = useState<HubDataset[]>(paperMode ? paperDatasets : [])
  const [loading, setLoading] = useState(!paperMode)
  const [error, setError] = useState<string | null>(null)
  const [activeDatasetId, setActiveDatasetId] = useState<string | null>(paperMode ? paperExportDatasetId : null)
  const [activeHistoryIndex, setActiveHistoryIndex] = useState(0)
  const [revealedSellerId, setRevealedSellerId] = useState<string | null>(null)
  const [reviews, setReviews] = useState<MarketReview[]>(paperMode ? paperReviews : [])
  const [reviewsLoading, setReviewsLoading] = useState(false)
  const [historyRows, setHistoryRows] = useState<HistoryRow[]>(paperMode ? paperHistoryRows : [])
  const [historyLoading, setHistoryLoading] = useState(false)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const historyCacheRef = useRef<Map<string, HistoryCacheEntry>>(loadHistoryCache())
  const historyRequestRef = useRef(new Map<string, Promise<HistoryRow[]>>())

  useEffect(() => {
    if (paperMode) {
      setDatasets(paperDatasets)
      setLoading(false)
      setError(null)
      return
    }

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
  }, [paperMode])

  useEffect(() => {
    if (!datasets.length) {
      setActiveDatasetId(null)
      return
    }

    setActiveDatasetId(prev => {
      if (prev && datasets.some(dataset => dataset.id === prev)) return prev;
      const cool = datasets.find(d => d.title?.toLowerCase().includes('cool avatar'));
      return cool ? cool.id : datasets[0].id;
    })
  }, [datasets])

  useEffect(() => {
    if (!datasets.length)
      return

    const matchedDataset = datasets.find(dataset => datasetMatchesCandidate(dataset, selectedCandidateId))
    if (!matchedDataset)
      return

    setActiveDatasetId(prev => (prev === matchedDataset.id ? prev : matchedDataset.id))
  }, [datasets, selectedCandidateId])

  useEffect(() => {
    setActiveHistoryIndex(0)
    setRevealedSellerId(null)
  }, [activeDatasetId])

  useEffect(() => {
    onActiveDatasetChange?.(activeDatasetId)
  }, [activeDatasetId, onActiveDatasetChange])

  const activeDataset = useMemo(
    () => datasets.find(dataset => dataset.id === activeDatasetId) ?? datasets[0] ?? null,
    [activeDatasetId, datasets],
  )

  const updateHistoryCache = (listingId: string, rows: HistoryRow[]) => {
    historyCacheRef.current.set(listingId, { rows, fetchedAt: Date.now() })
    persistHistoryCache(historyCacheRef.current)
  }

  const getHistoryRows = async (listingId: string) => {
    const cached = historyCacheRef.current.get(listingId)
    const now = Date.now()
    if (cached && now - cached.fetchedAt < HISTORY_CACHE_TTL_MS)
      return cached.rows

    const pending = historyRequestRef.current.get(listingId)
    if (pending)
      return pending

    const request = fetch(`${CONTRACT_HISTORY_API}?listingId=${encodeURIComponent(listingId)}`)
      .then(res => {
        if (!res.ok)
          throw new Error(`HTTP ${res.status}`)

        return res.json() as Promise<ContractHistoryResponse[]>
      })
      .then((data) => {
        const payload = Array.isArray(data) ? data[0] ?? null : null
        const events = Array.isArray(payload?.events) ? payload.events : []
        const eventError = events.find(event => event.error)?.error
        if (eventError)
          throw new Error(eventError)

        const rows = normalizeHistoryRows(payload)
        updateHistoryCache(listingId, rows)
        return rows
      })
      .finally(() => {
        historyRequestRef.current.delete(listingId)
      })

    historyRequestRef.current.set(listingId, request)
    return request
  }

  useEffect(() => {
    if (paperMode) {
      setHistoryRows(paperHistoryRows)
      setHistoryError(null)
      setHistoryLoading(false)
      return
    }

    if (!activeDatasetId || !hasOnChainContract(activeDataset?.contract_address)) {
      setHistoryRows([])
      setHistoryError(null)
      setHistoryLoading(false)
      return
    }

    let cancelled = false
    const cached = historyCacheRef.current.get(activeDatasetId)
    if (cached) {
      setHistoryRows(cached.rows)
      setHistoryError(null)
      if (Date.now() - cached.fetchedAt < HISTORY_CACHE_TTL_MS) {
        setHistoryLoading(false)
        return () => {
          cancelled = true
        }
      }
    }

    setHistoryLoading(!cached)
    setHistoryError(null)

    getHistoryRows(activeDatasetId)
      .then((rows) => {
        if (!cancelled)
          setHistoryRows(rows)
      })
      .catch((err) => {
        if (!cancelled) {
          if (!cached)
            setHistoryRows([])
          if (!cached)
            setHistoryError(err.message)
        }
      })
      .finally(() => {
        if (!cancelled)
          setHistoryLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [activeDataset?.contract_address, activeDatasetId, paperMode])

  useEffect(() => {
    if (paperMode)
      return

    const contractDatasets = datasets.filter(dataset => hasOnChainContract(dataset.contract_address))
    if (!contractDatasets.length)
      return

    let cancelled = false

    const warmCache = async () => {
      const queue = [...contractDatasets]
      const workers = Array.from({ length: Math.min(3, queue.length) }, () => (async () => {
        while (!cancelled) {
          const dataset = queue.shift()
          if (!dataset)
            return
          if (historyRequestRef.current.has(dataset.id))
            continue
          try {
            await getHistoryRows(dataset.id)
          } catch {}
        }
      })())

      await Promise.all(workers)
    }

    void warmCache()

    return () => {
      cancelled = true
    }
  }, [datasets, paperMode])

  // Fetch reviews from real API when active dataset changes
  useEffect(() => {
    if (paperMode) {
      setReviews(paperReviews)
      setReviewsLoading(false)
      return
    }

    if (!activeDatasetId) {
      setReviews([])
      setReviewsLoading(false)
      return
    }
    let cancelled = false
    setReviews([])
    setReviewsLoading(true)
    fetch(`/api/reviews?listingId=${activeDatasetId}`)
      .then(res => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        return res.json() as Promise<MarketReview[]>
      })
      .then(data => { if (!cancelled) setReviews(data) })
      .catch(() => { if (!cancelled) setReviews([]) })
      .finally(() => { if (!cancelled) setReviewsLoading(false) })
    return () => { cancelled = true }
  }, [activeDatasetId, paperMode])

  const sessionReview = activeDatasetId ? sessionReviewsByDatasetId[activeDatasetId] ?? null : null

  const displayReviews = useMemo(() => {
    if (!sessionReview)
      return reviews

    const baseReviews = reviews.filter(review => review.id !== sessionReview.id)
    if (baseReviews.length === 0)
      return [sessionReview]

    return [baseReviews[0], sessionReview, ...baseReviews.slice(1)]
  }, [sessionReview, reviews])

  const toggleSeller = (datasetId: string) => {
    setRevealedSellerId(prev => (prev === datasetId ? null : datasetId))
  }

  return (
    <section className="panel ledger-panel">
      <div className="panel-heading">
        <SectionTitle variant="ledger" title="Data Market" />
      </div>

      <div className="ledger-grid">
        <section className="ledger-card">
          <div className="card-header">
            <h3>Guixu Hub</h3>
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
            <h3>Transaction History</h3>
          </div>
          <div className="table-scroll-area vertical">
            <table className="data-table history-table">
              <colgroup>
                <col className="time-col" />
                <col className="event-col" />
                <col className="buyer-col" />
                <col className="value-col" />
                <col className="tx-col" />
              </colgroup>
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Event</th>
                  <th>Buyer</th>
                  <th>Value</th>
                  <th>Transaction Hash</th>
                </tr>
              </thead>
              <tbody>
                {historyLoading && (
                  <tr><td colSpan={5} style={{ textAlign: 'center', opacity: 0.5 }}>Loading on-chain events...</td></tr>
                )}
                {!historyLoading && historyError && (
                  <tr><td colSpan={5} style={{ textAlign: 'center', color: '#c0392b' }}>Failed: {historyError}</td></tr>
                )}
                {!historyLoading && !historyError && activeDataset && !hasOnChainContract(activeDataset.contract_address) && (
                  <tr><td colSpan={5} style={{ textAlign: 'center', opacity: 0.5 }}>This dataset does not have an on-chain contract.</td></tr>
                )}
                {!historyLoading && !historyError && hasOnChainContract(activeDataset?.contract_address) && historyRows.length === 0 && (
                  <tr><td colSpan={5} style={{ textAlign: 'center', opacity: 0.5 }}>No contract events recorded yet.</td></tr>
                )}
                {historyRows.map((row, index) => (
                  <tr
                    key={`${row.txHash}-${row.eventType}`}
                    className={`interactive-row${index === activeHistoryIndex ? ' highlight-row' : ''}`}
                    onClick={() => setActiveHistoryIndex(index)}
                  >
                    <td title={row.timeTitle}>{row.time}</td>
                    <td>
                      <div className="history-event-cell">
                        <span className={`event-pill ${row.eventTone}`}>{row.eventType}</span>
                      </div>
                    </td>
                    <td className="history-buyer-cell" title={row.buyerTitle}>{row.buyer}</td>
                    <td className="history-value-cell">{row.value}</td>
                    <td>
                      <a
                        className="history-tx-link"
                        href={`https://basescan.org/tx/${row.txHash}`}
                        target="_blank"
                        rel="noreferrer"
                        onClick={event => event.stopPropagation()}
                        title={row.txHash}
                      >
                        {row.txShort}
                      </a>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="ledger-card">
          <div className="card-header">
            <h3>On-chain Reviews</h3>
          </div>
          <div className="review-stack">
            {reviewsLoading && displayReviews.length === 0 && (
              <p style={{ textAlign: 'center', opacity: 0.5, padding: '16px' }}>Loading reviews...</p>
            )}
            {!reviewsLoading && displayReviews.length === 0 && (
              <p style={{ textAlign: 'center', opacity: 0.5, padding: '16px' }}>No reviews yet for this dataset.</p>
            )}
            {displayReviews.map((review: MarketReview, index: number) => {
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
