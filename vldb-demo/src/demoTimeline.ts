/*
 * Demo timing presets.
 *
 * What you usually need to tweak:
 * 1. planningNodeDurationsMs: how long each workflow card stays "running".
 * 2. traceStageDurationsMs: for input2 only, how long each On-chain Agent Trace stage runs.
 *
 * For input2, `purchase` is derived automatically from:
 *   payment + shards + unlock
 * so Task Execution will not start before Verified Data Delivery completes.
 *
 * Refreshing the page resets all in-memory demo state.
 */

export type PlanningRuntimePhase = 'idle' | 'running' | 'done'
export type PlanningRuntimeNodeId = 'parser' | 'search' | 'code' | 'valuation' | 'purchase' | 'execution'
export type DemoPresetIndex = 0 | 1

export type PlanningRuntimeState = {
  launchId: number
  launched: boolean
  hasGuixuHub: boolean
  presetIndex: DemoPresetIndex
  nodePhases: Record<PlanningRuntimeNodeId, PlanningRuntimePhase>
  nodeProgress: Record<PlanningRuntimeNodeId, number>
}

export const idlePlanningRuntimeState: PlanningRuntimeState = {
  launchId: 0,
  launched: false,
  hasGuixuHub: false,
  presetIndex: 0,
  nodePhases: {
    parser: 'idle',
    search: 'idle',
    code: 'idle',
    valuation: 'idle',
    purchase: 'idle',
    execution: 'idle',
  },
  nodeProgress: {
    parser: 0,
    search: 0,
    code: 0,
    valuation: 0,
    purchase: 0,
    execution: 0,
  },
}

export type DemoTimingPreset = {
  revealIntervalMs: number
  executionStartDelayMs: number
  planningNodeDurationsMs: Partial<Record<PlanningRuntimeNodeId, number>>
  traceStageDurationsMs?: {
    payment: number
    shards: number
    unlock: number
    review: number
  }
}

export const demoTimingPresets: Record<DemoPresetIndex, DemoTimingPreset> = {
  0: {
    revealIntervalMs: 680,
    executionStartDelayMs: 420,
    planningNodeDurationsMs: {
      parser: 3200,
      search: 4300,
      code: 3600,
      valuation: 2800,
      execution: 4200,
    },
  },
  1: {
    revealIntervalMs: 680,
    executionStartDelayMs: 420,
    planningNodeDurationsMs: {
      parser: 3300,
      search: 5200,
      code: 4000,
      valuation: 3100,
      execution: 5200,
    },
    traceStageDurationsMs: {
      payment: 5200,
      shards: 4200,
      unlock: 3600,
      review: 3400,
    },
  },
}
