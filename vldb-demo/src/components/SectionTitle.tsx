/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

type SectionVariant = 'planning' | 'valuation' | 'ledger'

const icons: Record<SectionVariant, JSX.Element> = {
  planning: (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="3.5" y="4.5" width="6" height="6" rx="2"></rect>
      <rect x="14.5" y="4.5" width="6" height="6" rx="2"></rect>
      <rect x="9" y="14" width="6" height="6" rx="2"></rect>
      <path d="M9.5 7.5h5M12 10.5V14"></path>
    </svg>
  ),
  valuation: (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 19.5h16"></path>
      <rect x="5" y="11" width="3" height="6.5" rx="1"></rect>
      <rect x="10.5" y="8" width="3" height="9.5" rx="1"></rect>
      <rect x="16" y="5" width="3" height="12.5" rx="1"></rect>
    </svg>
  ),
  ledger: (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="4" y="5" width="16" height="4.5" rx="1.5"></rect>
      <rect x="4" y="10.75" width="16" height="4.5" rx="1.5"></rect>
      <rect x="4" y="16.5" width="16" height="2.5" rx="1.25"></rect>
    </svg>
  ),
}

const SectionTitle = ({ variant, title }: { variant: SectionVariant; title: string }) => (
  <div className="section-title">
    <span className={`title-icon ${variant}`}>{icons[variant]}</span>
    <h2>{title}</h2>
  </div>
)

export default SectionTitle
