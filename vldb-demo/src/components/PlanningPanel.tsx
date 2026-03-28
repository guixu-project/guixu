import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import {
  buildPlanningWorkflow,
  planningSourceOptions,
  type CandidateId,
  type PlanningSourceId,
  type WorkflowEdge,
  type WorkflowNode,
} from '../data'
import SectionTitle from './SectionTitle'
import WorkflowCard from './WorkflowCard'

type StageSize = { width: number; height: number }
type DragState = { id: string; offsetX: number; offsetY: number }
type RunConfig = { query: string; sources: PlanningSourceId[] } | null
type AnchorSide = 'left' | 'right' | 'top' | 'bottom'
type NodeFrame = { x: number; y: number; w: number; h: number }
type NodeCardLayout = { width: number; height: number }

const defaultQuery = 'Write an image classifier that checks whether Caesar is in the photo taken by my house monitor'
const defaultSources: PlanningSourceId[] = ['kaggle', 'huggingface', 'guixu-hub']
const stageInset = {
  left: 18,
  right: 6,
  top: 18,
  bottom: 18,
}
const REVEAL_INTERVAL_MS = 680
const EXECUTION_INTERVAL_MS = 1650
const EXECUTION_START_DELAY_MS = 420

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max)

const nodeCardLayout: Record<string, NodeCardLayout> = {
  parser: { width: 170, height: 258 },
  search: { width: 156, height: 220 },
  code: { width: 156, height: 200 },
  valuation: { width: 152, height: 164 },
  execution: { width: 156, height: 124 },
  provenance: { width: 156, height: 230 },
}

const layoutWorkflowNodes = (nodes: WorkflowNode[], stage: StageSize) => {
  const sizeFor = (nodeId: string) => nodeCardLayout[nodeId] ?? { width: 136, height: 136 }
  const hasProvenance = nodes.some(node => node.id === 'provenance')
  const safeWidth = Math.max(stage.width - stageInset.left - stageInset.right, 1)
  const preferredColWidths = [
    sizeFor('parser').width,
    Math.max(sizeFor('search').width, sizeFor('code').width),
    sizeFor('valuation').width,
    Math.max(sizeFor('execution').width, hasProvenance ? sizeFor('provenance').width : 0),
  ]
  const minimumGap = 10
  const totalPreferredWidth = preferredColWidths.reduce((sum, width) => sum + width, 0) + minimumGap * 3
  const widthScale = totalPreferredWidth > safeWidth ? safeWidth / totalPreferredWidth : 1
  const colWidths = preferredColWidths.map(width => Math.max(72, Math.round(width * widthScale)))
  const actualGap = Math.max(8, Math.floor((safeWidth - colWidths.reduce((sum, width) => sum + width, 0)) / 3))
  const upperBandHeight = Math.max(sizeFor('search').height, sizeFor('execution').height)
  const lowerBandHeight = Math.max(sizeFor('code').height, hasProvenance ? sizeFor('provenance').height : sizeFor('code').height)
  const topYBase = stageInset.top + 6
  const bottomYBase = stage.height - stageInset.bottom - 24
  const minRowGap = clamp(Math.round(stage.height * 0.06), 24, 40)

  let topY = topYBase
  let lowerY = bottomYBase - lowerBandHeight

  if (lowerY - (topY + upperBandHeight) < minRowGap) {
    const layoutHeight = upperBandHeight + minRowGap + lowerBandHeight
    topY = clamp(
      Math.round((stage.height - layoutHeight) / 2),
      topYBase,
      Math.max(topYBase, bottomYBase - layoutHeight),
    )
    lowerY = topY + upperBandHeight + minRowGap
  }

  const laneMidY = (topY + upperBandHeight / 2 + lowerY + lowerBandHeight / 2) / 2
  const centerY = (height: number) => Math.round(laneMidY - height / 2 - 10)

  const colX = [
    stageInset.left,
    stageInset.left + colWidths[0] + actualGap,
    stageInset.left + colWidths[0] + actualGap + colWidths[1] + actualGap,
    stageInset.left + colWidths[0] + actualGap + colWidths[1] + actualGap + colWidths[2] + actualGap,
  ]

  return nodes.map((node) => {
    const preferred = sizeFor(node.id)
    const height = preferred.height
    const width = Math.max(72, Math.round(preferred.width * widthScale))
    let x = colX[0]
    let y = topY

    switch (node.id) {
      case 'parser':
        x = colX[0]
        y = centerY(height)
        break
      case 'search':
        x = colX[1]
        y = topY + Math.round((upperBandHeight - height) / 2)
        break
      case 'code':
        x = colX[1]
        y = lowerY + Math.round((lowerBandHeight - height) / 2)
        break
      case 'valuation':
        x = colX[2]
        y = centerY(height)
        break
      case 'execution':
        x = colX[3]
        y = hasProvenance ? topY + Math.round((upperBandHeight - height) / 2) : centerY(height)
        break
      case 'provenance':
        x = colX[3]
        y = lowerY + Math.round((lowerBandHeight - height) / 2)
        break
      default:
        break
    }

    const maxX = Math.max(stage.width - width - stageInset.left - stageInset.right, 1)
    const maxY = Math.max(stage.height - height - stageInset.top - stageInset.bottom, 1)
    const clampedX = clamp(x, stageInset.left, stageInset.left + maxX)
    const clampedY = clamp(y, stageInset.top, stageInset.top + maxY)

    return {
      ...node,
      size: { w: width, h: height },
      position: {
        x: (clampedX - stageInset.left) / maxX,
        y: (clampedY - stageInset.top) / maxY,
      },
    }
  })
}

const toPixels = (node: WorkflowNode, stage: StageSize) => {
  const maxX = Math.max(stage.width - node.size.w - stageInset.left - stageInset.right, 1)
  const maxY = Math.max(stage.height - node.size.h - stageInset.top - stageInset.bottom, 1)

  return {
    x: stageInset.left + node.position.x * maxX,
    y: stageInset.top + node.position.y * maxY,
  }
}

type PositionedNode = WorkflowNode & {
  status: { phase: 'idle' | 'running' | 'done' }
  px: { x: number; y: number }
}

type MeasuredNode = PositionedNode & {
  frame: NodeFrame
}

type PositionedEdge = WorkflowEdge & {
  d: string
  start: { x: number; y: number }
  end: { x: number; y: number }
  target: { x: number; y: number }
  targetSide: AnchorSide
}

type PortShape =
  | { kind: 'vertical'; x: number; y: number; width: number; height: number }
  | { kind: 'horizontal'; x: number; y: number; width: number; height: number }

const defaultFrame = (node: PositionedNode): NodeFrame => ({
  x: node.px.x,
  y: node.px.y,
  w: node.size.w,
  h: node.size.h,
})

const sameFrame = (a: NodeFrame | undefined, b: NodeFrame | undefined) => {
  if (!a || !b)
    return false

  return a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h
}

const workflowOrder = (node: WorkflowNode) => Number(node.badge) || Number.MAX_SAFE_INTEGER

const getEdgeSides = (edge: WorkflowEdge, fromNode: MeasuredNode, toNode: MeasuredNode) => {
  const fromCenterX = fromNode.frame.x + fromNode.frame.w / 2
  const toCenterX = toNode.frame.x + toNode.frame.w / 2
  const sameColumn = Math.abs(fromCenterX - toCenterX) < 16

  if (edge.from === 'execution' && edge.to === 'provenance')
    return { source: 'bottom' as const, target: 'top' as const }

  if (sameColumn && toNode.frame.y > fromNode.frame.y)
    return { source: 'bottom' as const, target: 'top' as const }

  return { source: 'right' as const, target: 'left' as const }
}

const getAnchorPoint = (node: MeasuredNode, side: AnchorSide) => {
  switch (side) {
    case 'left':
      return { x: node.frame.x, y: node.frame.y + node.frame.h / 2 }
    case 'right':
      return { x: node.frame.x + node.frame.w, y: node.frame.y + node.frame.h / 2 }
    case 'top':
      return { x: node.frame.x + node.frame.w / 2, y: node.frame.y }
    case 'bottom':
      return { x: node.frame.x + node.frame.w / 2, y: node.frame.y + node.frame.h }
  }
}

const getTargetApproachPoint = (target: { x: number; y: number }, side: AnchorSide) => {
  const depth = 8

  switch (side) {
    case 'left':
      return { x: target.x - depth / 2, y: target.y }
    case 'right':
      return { x: target.x + depth / 2, y: target.y }
    case 'top':
      return { x: target.x, y: target.y - depth / 2 }
    case 'bottom':
      return { x: target.x, y: target.y + depth / 2 }
  }
}

const getTargetPortShape = (target: { x: number; y: number }, side: AnchorSide): PortShape => {
  switch (side) {
    case 'left':
    case 'right':
      return {
        kind: 'vertical',
        x: target.x - 4,
        y: target.y - 8,
        width: 8,
        height: 16,
      }
    case 'top':
    case 'bottom':
      return {
        kind: 'horizontal',
        x: target.x - 8,
        y: target.y - 4,
        width: 16,
        height: 8,
      }
  }
}

const buildEdgeGeometry = (fromNode: MeasuredNode, toNode: MeasuredNode, edge: WorkflowEdge) => {
  const { source, target } = getEdgeSides(edge, fromNode, toNode)
  const start = getAnchorPoint(fromNode, source)
  const targetPoint = getAnchorPoint(toNode, target)
  const end = getTargetApproachPoint(targetPoint, target)

  if (source === 'bottom' && target === 'top') {
    const control = clamp(Math.abs(end.y - start.y) * 0.45, 24, 56)
    return {
      start,
      end,
      target: targetPoint,
      targetSide: target,
      d: `M ${start.x} ${start.y} C ${start.x} ${start.y + control}, ${end.x} ${end.y - control}, ${end.x} ${end.y}`,
    }
  }

  const control = clamp(Math.abs(end.x - start.x) * 0.42, 28, 70)
  return {
    start,
    end,
    target: targetPoint,
    targetSide: target,
    d: `M ${start.x} ${start.y} C ${start.x + control} ${start.y}, ${end.x - control} ${end.y}, ${end.x} ${end.y}`,
  }
}

const statusForNode = (node: WorkflowNode, runStep: number) => {
  if (runStep < node.lifecycle.showAt)
    return { phase: 'idle' as const }

  if (runStep < node.lifecycle.doneAt)
    return { phase: 'running' as const }

  return { phase: 'done' as const }
}

const PlanningPanel = ({ onRecommendCandidate }: { onRecommendCandidate: (candidateId: CandidateId) => void }) => {
  const stageRef = useRef<HTMLDivElement>(null)
  const dragRef = useRef<DragState | null>(null)
  const cardRefs = useRef<Record<string, HTMLDivElement | null>>({})

  const [query, setQuery] = useState(defaultQuery)
  const [sources, setSources] = useState<PlanningSourceId[]>(defaultSources)
  const [runConfig, setRunConfig] = useState<RunConfig>(null)
  const [nodes, setNodes] = useState<WorkflowNode[]>([])
  const [runStep, setRunStep] = useState(-1)
  const [revealCount, setRevealCount] = useState(0)
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null)
  const [stageSize, setStageSize] = useState<StageSize>({ width: 0, height: 0 })
  const [cardFrames, setCardFrames] = useState<Record<string, NodeFrame>>({})

  const workflow = useMemo(
    () => (runConfig ? buildPlanningWorkflow(runConfig.query, runConfig.sources) : null),
    [runConfig],
  )
  const revealOrderIds = useMemo(
    () => workflow ? [...workflow.nodes].sort((a, b) => workflowOrder(a) - workflowOrder(b)).map(node => node.id) : [],
    [workflow],
  )

  useEffect(() => {
    if (!stageRef.current)
      return

    const measureStage = (element: HTMLDivElement) => {
      setStageSize({
        width: element.clientWidth,
        height: element.clientHeight,
      })
    }

    const observer = new ResizeObserver(([entry]) => {
      measureStage(entry.target as HTMLDivElement)
    })

    observer.observe(stageRef.current)
    measureStage(stageRef.current)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (!workflow) {
      setNodes([])
      setRunStep(-1)
      setRevealCount(0)
      setSelectedNodeId(null)
      setCardFrames({})
      return
    }

    setNodes(stageSize.width > 0 && stageSize.height > 0 ? layoutWorkflowNodes(workflow.nodes, stageSize) : workflow.nodes)
    setRunStep(-1)
    setRevealCount(0)
    setSelectedNodeId(null)

    let revealTimer: number | null = null
    let executionTimer: number | null = null
    let executionStartTimer: number | null = null

    const startExecution = () => {
      setRunStep(0)
      executionTimer = window.setInterval(() => {
        setRunStep(prev => {
          if (prev >= workflow.maxStep) {
            if (executionTimer)
              window.clearInterval(executionTimer)
            return prev
          }
          return prev + 1
        })
      }, EXECUTION_INTERVAL_MS)
    }

    revealTimer = window.setInterval(() => {
      setRevealCount(prev => {
        const next = Math.min(prev + 1, revealOrderIds.length)

        if (next >= revealOrderIds.length && revealTimer) {
          window.clearInterval(revealTimer)
          executionStartTimer = window.setTimeout(startExecution, EXECUTION_START_DELAY_MS)
        }

        return next
      })
    }, REVEAL_INTERVAL_MS)

    return () => {
      if (revealTimer)
        window.clearInterval(revealTimer)
      if (executionTimer)
        window.clearInterval(executionTimer)
      if (executionStartTimer)
        window.clearTimeout(executionStartTimer)
    }
  }, [revealOrderIds, workflow])

  useEffect(() => {
    if (!workflow)
      return

    const runningNode = workflow.nodes.find(node => runStep >= node.lifecycle.showAt && runStep < node.lifecycle.doneAt)
    if (runningNode) {
      setSelectedNodeId(runningNode.id)
      return
    }

    const finishedNode = [...workflow.nodes]
      .reverse()
      .find(node => runStep >= node.lifecycle.doneAt)

    if (finishedNode)
      setSelectedNodeId(finishedNode.id)
  }, [runStep, workflow])

  useEffect(() => {
    if (runStep >= 0 || revealCount <= 0)
      return

    const latestVisibleId = revealOrderIds[revealCount - 1]
    if (latestVisibleId)
      setSelectedNodeId(latestVisibleId)
  }, [revealCount, revealOrderIds, runStep])

  useEffect(() => {
    if (!workflow || stageSize.width <= 0 || stageSize.height <= 0)
      return

    setNodes(prev => {
      if (!prev.length)
        return prev
      return layoutWorkflowNodes(prev, stageSize)
    })
  }, [stageSize, workflow])

  useEffect(() => {
    if (!draggingId)
      return

    const handlePointerMove = (event: PointerEvent) => {
      if (!dragRef.current || !stageRef.current)
        return

      const rect = stageRef.current.getBoundingClientRect()

      setNodes(prev => prev.map(node => {
        if (node.id !== dragRef.current?.id)
          return node

        const maxX = Math.max(rect.width - node.size.w - stageInset.left - stageInset.right, 1)
        const maxY = Math.max(rect.height - node.size.h - stageInset.top - stageInset.bottom, 1)
        const nextX = clamp(event.clientX - rect.left - dragRef.current.offsetX - stageInset.left, 0, maxX)
        const nextY = clamp(event.clientY - rect.top - dragRef.current.offsetY - stageInset.top, 0, maxY)

        return {
          ...node,
          position: {
            x: nextX / maxX,
            y: nextY / maxY,
          },
        }
      }))
    }

    const stopDragging = () => {
      dragRef.current = null
      setDraggingId(null)
    }

    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', stopDragging)

    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', stopDragging)
    }
  }, [draggingId])

  const visibleNodes = useMemo(() => {
    const visibleIds = new Set(revealOrderIds.slice(0, revealCount))

    return nodes
      .filter(node => visibleIds.has(node.id))
      .map(node => {
        const status = statusForNode(node, runStep)
        return {
          ...node,
          status,
          px: toPixels(node, stageSize),
        }
      })
  }, [nodes, revealCount, revealOrderIds, runStep, stageSize]) as PositionedNode[]

  useLayoutEffect(() => {
    if (!stageRef.current) {
      return
    }

    if (!visibleNodes.length) {
      setCardFrames(prev => (Object.keys(prev).length ? {} : prev))
      return
    }

    const stageRect = stageRef.current.getBoundingClientRect()
    const nextFrames: Record<string, NodeFrame> = {}

    visibleNodes.forEach((node) => {
      const element = cardRefs.current[node.id]
      if (!element)
        return

      const rect = element.getBoundingClientRect()
      nextFrames[node.id] = {
        x: rect.left - stageRect.left,
        y: rect.top - stageRect.top,
        w: rect.width,
        h: rect.height,
      }
    })

    setCardFrames((prev) => {
      const prevKeys = Object.keys(prev)
      const nextKeys = Object.keys(nextFrames)

      if (
        prevKeys.length === nextKeys.length
        && nextKeys.every(key => sameFrame(prev[key], nextFrames[key]))
      ) {
        return prev
      }

      return nextFrames
    })
  }, [visibleNodes, stageSize])

  const measuredNodes = useMemo(() => {
    return visibleNodes.map(node => ({
      ...node,
      frame: cardFrames[node.id] ?? defaultFrame(node),
    }))
  }, [cardFrames, visibleNodes])

  const visibleEdges = useMemo(() => {
    if (!workflow)
      return []

    const nodeMap = new Map(measuredNodes.map(node => [node.id, node]))

    return workflow.edges.flatMap((edge) => {
      const fromNode = nodeMap.get(edge.from)
      const toNode = nodeMap.get(edge.to)

      if (!fromNode || !toNode)
        return []

      const geometry = buildEdgeGeometry(fromNode, toNode, edge)

      return [{
        ...edge,
        ...geometry,
      }]
    }) as PositionedEdge[]
  }, [measuredNodes, workflow])

  const canLaunch = query.trim().length > 0 && sources.length > 0

  const toggleSource = (sourceId: PlanningSourceId) => {
    setSources(prev => (
      prev.includes(sourceId)
        ? prev.filter(id => id !== sourceId)
        : [...prev, sourceId]
    ))
  }

  const launchWorkflow = () => {
    if (!canLaunch)
      return

    const nextWorkflow = buildPlanningWorkflow(query.trim(), sources)
    onRecommendCandidate(nextWorkflow.recommendedCandidateId)
    setRunConfig({
      query: query.trim(),
      sources,
    })
  }

  return (
    <section className="panel planning-panel">
      <div className="panel-heading">
        <SectionTitle variant="planning" title="Agent Planning & Step-wise Discovery" />
      </div>

      <div className="planning-launcher">
        <div className="input-shell">
          <input
            id="demo-query"
            type="text"
            value={query}
            onChange={event => setQuery(event.target.value)}
          />
        </div>

        <div className="launcher-footer">
          <div className="source-picker">
            {planningSourceOptions.map(source => (
              <button
                key={source.id}
                type="button"
                className={`source-chip source-chip-${source.id}${sources.includes(source.id) ? ' active' : ''}`}
                onClick={() => toggleSource(source.id)}
              >
                <span className="source-check" aria-hidden="true">{sources.includes(source.id) ? '✓' : ''}</span>
                <span className={`source-icon source-icon-${source.id}`} aria-hidden="true">
                  {source.id === 'kaggle' && (
                    <svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M18.825 23.859c-.022.092-.117.141-.281.141h-3.139c-.187 0-.351-.082-.492-.248l-5.178-6.589-1.448 1.374v5.111c0 .235-.117.352-.351.352H5.505c-.236 0-.354-.117-.354-.352V.353c0-.233.118-.353.354-.353h2.431c.234 0 .351.12.351.353v14.343l6.203-6.272c.165-.165.33-.246.495-.246h3.239c.144 0 .236.06.281.18.046.149.034.238-.036.27l-6.555 6.344 6.836 8.507c.059.083.063.167.012.252l.063-.172z"/></svg>
                  )}
                  {source.id === 'huggingface' && (
                    <svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12.025 1.13c-5.77 0-10.449 4.647-10.449 10.378 0 1.112.178 2.181.503 3.185.064-.222.203-.444.416-.577a.96.96 0 0 1 .524-.15c.293 0 .584.124.84.284.278.173.48.408.71.694.226.282.458.611.684.951v-.014c.017-.324.106-.622.264-.874s.403-.487.762-.543c.3-.047.596.06.787.203s.31.313.4.467c.15.257.212.468.233.542.01.026.653 1.552 1.657 2.54.616.605 1.01 1.223 1.082 1.912.055.537-.096 1.059-.38 1.572.637.121 1.294.187 1.967.187.657 0 1.298-.063 1.921-.178-.287-.517-.44-1.041-.384-1.581.07-.69.465-1.307 1.081-1.913 1.004-.987 1.647-2.513 1.657-2.539.021-.074.083-.285.233-.542.09-.154.208-.323.4-.467a1.08 1.08 0 0 1 .787-.203c.359.056.604.29.762.543s.247.55.265.874v.015c.225-.34.457-.67.683-.952.23-.286.432-.52.71-.694.257-.16.547-.284.84-.285a.97.97 0 0 1 .524.151c.228.143.373.388.43.625l.006.04a10.3 10.3 0 0 0 .534-3.273c0-5.731-4.678-10.378-10.449-10.378M8.327 6.583a1.5 1.5 0 0 1 .713.174 1.487 1.487 0 0 1 .617 2.013c-.183.343-.762-.214-1.102-.094-.38.134-.532.914-.917.71a1.487 1.487 0 0 1 .69-2.803m7.486 0a1.487 1.487 0 0 1 .689 2.803c-.385.204-.536-.576-.916-.71-.34-.12-.92.437-1.103.094a1.487 1.487 0 0 1 .617-2.013 1.5 1.5 0 0 1 .713-.174m-10.68 1.55a.96.96 0 1 1 0 1.921.96.96 0 0 1 0-1.92m13.838 0a.96.96 0 1 1 0 1.92.96.96 0 0 1 0-1.92M8.489 11.458c.588.01 1.965 1.157 3.572 1.164 1.607-.007 2.984-1.155 3.572-1.164.196-.003.305.12.305.454 0 .886-.424 2.328-1.563 3.202-.22-.756-1.396-1.366-1.63-1.32q-.011.001-.02.006l-.044.026-.01.008-.03.024q-.018.017-.035.036l-.032.04a1 1 0 0 0-.058.09l-.014.025q-.049.088-.11.19a1 1 0 0 1-.083.116 1.2 1.2 0 0 1-.173.18q-.035.029-.075.058a1.3 1.3 0 0 1-.251-.243 1 1 0 0 1-.076-.107c-.124-.193-.177-.363-.337-.444-.034-.016-.104-.008-.2.022q-.094.03-.216.087-.06.028-.125.063l-.13.074q-.067.04-.136.086a3 3 0 0 0-.135.096 3 3 0 0 0-.26.219 2 2 0 0 0-.12.121 2 2 0 0 0-.106.128l-.002.002a2 2 0 0 0-.09.132l-.001.001a1.2 1.2 0 0 0-.105.212q-.013.036-.024.073c-1.139-.875-1.563-2.317-1.563-3.203 0-.334.109-.457.305-.454m.836 10.354c.824-1.19.766-2.082-.365-3.194-1.13-1.112-1.789-2.738-1.789-2.738s-.246-.945-.806-.858-.97 1.499.202 2.362c1.173.864-.233 1.45-.685.64-.45-.812-1.683-2.896-2.322-3.295s-1.089-.175-.938.647 2.822 2.813 2.562 3.244-1.176-.506-1.176-.506-2.866-2.567-3.49-1.898.473 1.23 2.037 2.16c1.564.932 1.686 1.178 1.464 1.53s-3.675-2.511-4-1.297c-.323 1.214 3.524 1.567 3.287 2.405-.238.839-2.71-1.587-3.216-.642-.506.946 3.49 2.056 3.522 2.064 1.29.33 4.568 1.028 5.713-.624m5.349 0c-.824-1.19-.766-2.082.365-3.194 1.13-1.112 1.789-2.738 1.789-2.738s.246-.945.806-.858.97 1.499-.202 2.362c-1.173.864.233 1.45.685.64.451-.812 1.683-2.896 2.322-3.295s1.089-.175.938.647-2.822 2.813-2.562 3.244 1.176-.506 1.176-.506 2.866-2.567 3.49-1.898-.473 1.23-2.037 2.16c-1.564.932-1.686 1.178-1.464 1.53s3.675-2.511 4-1.297c.323 1.214-3.524 1.567-3.287 2.405.238.839 2.71-1.587 3.216-.642.506.946-3.49 2.056-3.522 2.064-1.29.33-4.568 1.028-5.713-.624"/></svg>
                  )}
                  {source.id === 'guixu-hub' && (
                    <svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z"/></svg>
                  )}
                </span>
                {source.label}
              </button>
            ))}
          </div>

          <button
            type="button"
            className="launch-button"
            disabled={!canLaunch}
            onClick={launchWorkflow}
          >
            Start
          </button>
        </div>
      </div>

      <div className="workflow-stage orchestration-stage" ref={stageRef}>
        {!workflow && (
          <div className="workflow-empty">
            <strong>Launch the workflow to generate the orchestration graph.</strong>
          </div>
        )}

        {stageSize.width > 0 && stageSize.height > 0 && visibleEdges.length > 0 && (
          <svg
            className="workflow-links"
            viewBox={`0 0 ${stageSize.width} ${stageSize.height}`}
            preserveAspectRatio="none"
            aria-hidden="true"
          >
            <defs>
              <marker
                id="workflow-arrow"
                markerWidth="12"
                markerHeight="12"
                refX="10"
                refY="6"
                orient="auto"
                markerUnits="userSpaceOnUse"
              >
                <path d="M 1 1 L 10 6 L 1 11 Z" />
              </marker>
            </defs>

            {visibleEdges.map(edge => (
              <g key={`${edge.from}-${edge.to}`}>
                <path className="edge-halo" d={edge.d} />
                <path className={`edge-line ${edge.kind ?? 'primary'}`} d={edge.d} markerEnd="url(#workflow-arrow)" />
                <circle className="edge-start" cx={edge.start.x} cy={edge.start.y} r="5.5" />
                {(() => {
                  const port = getTargetPortShape(edge.target, edge.targetSide)

                  if (port.kind === 'vertical') {
                    return (
                      <rect
                        className="edge-target-port"
                        x={port.x}
                        y={port.y}
                        width={port.width}
                        height={port.height}
                        rx="2.5"
                      />
                    )
                  }

                  return (
                    <rect
                      className="edge-target-port"
                      x={port.x}
                      y={port.y}
                      width={port.width}
                      height={port.height}
                      rx="2.5"
                    />
                  )
                })()}
              </g>
            ))}
          </svg>
        )}

        {measuredNodes.map(node => (
          <WorkflowCard
            key={node.id}
            node={node}
            selected={selectedNodeId === node.id}
            dragging={draggingId === node.id}
            cardRef={(element) => {
              cardRefs.current[node.id] = element
            }}
            style={{
              width: node.size.w,
              minHeight: node.size.h,
              transform: `translate(${node.px.x}px, ${node.px.y}px)`,
            }}
            onPointerDown={(event) => {
              if (!stageRef.current)
                return

              event.preventDefault()
              const rect = stageRef.current.getBoundingClientRect()
              const position = cardFrames[node.id] ?? defaultFrame(node)

              dragRef.current = {
                id: node.id,
                offsetX: event.clientX - rect.left - position.x,
                offsetY: event.clientY - rect.top - position.y,
              }
              setDraggingId(node.id)
              setSelectedNodeId(node.id)
            }}
          />
        ))}
      </div>
    </section>
  )
}

export default PlanningPanel
