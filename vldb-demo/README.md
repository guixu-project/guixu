# VLDB Demo Design Pack

This folder contains the standalone design package and the `Vite + React + TypeScript` frontend skeleton for the `VLDB 2026` Guixu demo UI.

The goal is to give the frontend a clear starting point without reusing the legacy `demo-ui/` prototype.

## Files

- `UI_FRAMEWORK.md`
  Design rationale, panel responsibilities, visual direction, and implementation guidance.
- `index.html`
  Vite entry HTML.
- `package.json`
  Frontend dependencies and scripts.
- `src/`
  React source code, mock data, and styles.
- `assets/vldb-demo-mockup.svg`
  Editable mockup source for the conference-oriented UI.
- `assets/vldb-demo-mockup.png`
  Rendered preview image exported from the SVG.

## Core Narrative

The demo UI is organized around three coordinated areas:

1. `Phase 1: Agent Planning & Step-wise Discovery`
   Shows how a natural-language task becomes task description, search keywords, and a code artifact.
2. `Phase 2: Code-Aware Data Valuation & Execution`
   Shows candidate datasets, valuation evidence, recommendation, and training outcome.
3. `Decentralized Asset Ledger & Feedback Loop`
   Shows seller-side assets, transaction history, and feedback as persistent market memory.

## Design Intent

- Keep the interface `paper-demo friendly`: one screenshot should tell the whole story.
- Emphasize `Phase 2` as the primary technical contribution.
- Treat the on-chain area as `part of the ranking loop`, not as decorative blockchain chrome.
- Borrow the readability of workflow tools like Dify, but keep the page closer to an academic systems cockpit than a low-code builder.

## Preview

Install and run:

```bash
cd /home/pyc/workspace/guixu/vldb-demo
npm install
npm run dev
```

Then open the local Vite URL, typically `http://localhost:5173`.

## Suggested Next Step

Use `src/App.tsx` as the first implementation scaffold, and use the panel spec in `UI_FRAMEWORK.md` as the interaction contract. The next concrete coding step should be to connect this shell to typed demo-facing backend responses for:

- a page-level layout
- typed view models per panel
- a small set of demo-facing backend responses for search, detail, evaluate, and acquire flows
