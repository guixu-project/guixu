/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

const DemoHeader = ({ paperMode = false }: { paperMode?: boolean }) => (
  <header className="topbar">
    <div className="brand-line slim"><h1>Guixu</h1></div>
    <div className="header-actions">
      {paperMode && (
        <button type="button" className="ghost-button" onClick={() => window.print()}>
          Print PDF
        </button>
      )}
      <p className="brand-meta">{paperMode ? 'VLDB 2026 paper export' : 'VLDB 2026 demo'}</p>
    </div>
  </header>
)

export default DemoHeader
