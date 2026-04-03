// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

/// Embedded Web UI for dataset publishing.
/// Served at GET / when running `guixu start` or `guixu mcp --mode http`.
pub const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Guixu — Data Publishing</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
:root{--bg:#0a0e17;--surface:#111827;--border:#1e293b;--text:#e2e8f0;--dim:#64748b;--accent:#3b82f6;--green:#22c55e;--red:#ef4444;--yellow:#eab308}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:var(--bg);color:var(--text);min-height:100vh}
header{background:var(--surface);border-bottom:1px solid var(--border);padding:16px 24px;display:flex;align-items:center;justify-content:space-between}
header h1{font-size:18px;font-weight:600}
header h1 span{color:var(--dim);font-weight:400;font-size:14px;margin-left:8px}
.status{display:flex;align-items:center;gap:8px;font-size:13px;color:var(--dim)}
.status .dot{width:8px;height:8px;border-radius:50%;background:var(--green)}
main{max-width:960px;margin:0 auto;padding:24px}

/* Drop zone */
.dropzone{border:2px dashed var(--border);border-radius:12px;padding:48px;text-align:center;cursor:pointer;transition:all .2s;margin-bottom:24px}
.dropzone:hover,.dropzone.dragover{border-color:var(--accent);background:rgba(59,130,246,.05)}
.dropzone h2{font-size:20px;margin-bottom:8px}
.dropzone p{color:var(--dim);font-size:14px}
.dropzone input{display:none}

/* Privacy config */
.config{background:var(--surface);border:1px solid var(--border);border-radius:8px;padding:16px;margin-bottom:24px;display:flex;gap:24px;align-items:center;flex-wrap:wrap}
.config label{font-size:13px;color:var(--dim)}
.config select,.config input[type=number]{background:var(--bg);border:1px solid var(--border);color:var(--text);padding:6px 10px;border-radius:6px;font-size:13px}

/* Upload progress */
.upload-status{margin-bottom:24px;display:none}
.upload-status.active{display:block}
.upload-bar{height:4px;background:var(--border);border-radius:2px;overflow:hidden;margin-top:8px}
.upload-bar .fill{height:100%;background:var(--accent);transition:width .3s;width:0}
.upload-msg{font-size:13px;color:var(--dim);margin-top:6px}

/* Dataset list */
.datasets h2{font-size:16px;margin-bottom:12px;display:flex;align-items:center;gap:8px}
.datasets h2 .count{background:var(--accent);color:#fff;font-size:11px;padding:2px 8px;border-radius:10px}
.ds-card{background:var(--surface);border:1px solid var(--border);border-radius:8px;padding:16px;margin-bottom:8px;transition:border-color .2s}
.ds-card:hover{border-color:var(--accent)}
.ds-title{font-weight:600;font-size:15px;margin-bottom:4px}
.ds-meta{font-size:12px;color:var(--dim);display:flex;gap:16px;flex-wrap:wrap;margin-bottom:8px}
.ds-meta span{display:flex;align-items:center;gap:4px}
.ds-cid{font-family:monospace;font-size:11px;color:var(--dim);word-break:break-all}
.ds-cols{display:flex;gap:6px;flex-wrap:wrap;margin-top:8px}
.ds-cols .col{background:var(--bg);border:1px solid var(--border);padding:2px 8px;border-radius:4px;font-size:11px;font-family:monospace}
.ds-cols .col.hashed{color:var(--yellow);border-color:rgba(234,179,8,.3)}
.badge{display:inline-block;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600}
.badge.open{background:rgba(34,197,94,.15);color:var(--green)}
.badge.paid{background:rgba(234,179,8,.15);color:var(--yellow)}
.empty{text-align:center;padding:48px;color:var(--dim)}
</style>
</head>
<body>
<header>
  <h1>Guixu <span>Data Publishing</span></h1>
  <div class="status"><div class="dot" id="statusDot"></div><span id="statusText">Connecting...</span></div>
</header>
<main>
  <!-- Drop zone -->
  <div class="dropzone" id="dropzone">
    <h2>📂 Drop datasets here to publish</h2>
    <p>CSV, Parquet, JSON, TSV — or click to browse</p>
    <input type="file" id="fileInput" accept=".csv,.parquet,.json,.tsv" multiple>
  </div>

  <!-- Privacy config -->
  <div class="config">
    <div>
      <label>Privacy Level</label><br>
      <select id="privacyLevel">
        <option value="off">Off — raw metadata</option>
        <option value="standard" selected>Standard — DP noise + hash sensitive cols</option>
        <option value="strict">Strict — suppress min/max, hash all cols</option>
      </select>
    </div>
    <div>
      <label>DP Epsilon (ε)</label><br>
      <input type="number" id="epsilon" value="1.0" min="0.01" max="10" step="0.1" style="width:80px">
    </div>
    <div>
      <label>Access</label><br>
      <select id="accessMode">
        <option value="open">Open (free)</option>
        <option value="paid">Paid</option>
      </select>
    </div>
    <div id="priceGroup" style="display:none">
      <label>Price (USDC)</label><br>
      <input type="number" id="price" value="0" min="0" step="0.01" style="width:80px">
    </div>
  </div>

  <!-- Upload status -->
  <div class="upload-status" id="uploadStatus">
    <div class="upload-bar"><div class="fill" id="uploadFill"></div></div>
    <div class="upload-msg" id="uploadMsg"></div>
  </div>

  <!-- Published datasets -->
  <div class="datasets">
    <h2>Published Datasets <span class="count" id="dsCount">0</span></h2>
    <div id="dsList"></div>
  </div>
</main>

<script>
const API = window.location.origin;

// --- Drop zone ---
const dz = document.getElementById('dropzone');
const fi = document.getElementById('fileInput');

dz.addEventListener('click', () => fi.click());
dz.addEventListener('dragover', e => { e.preventDefault(); dz.classList.add('dragover'); });
dz.addEventListener('dragleave', () => dz.classList.remove('dragover'));
dz.addEventListener('drop', e => {
  e.preventDefault();
  dz.classList.remove('dragover');
  handleFiles(e.dataTransfer.files);
});
fi.addEventListener('change', () => handleFiles(fi.files));

// Show/hide price
document.getElementById('accessMode').addEventListener('change', e => {
  document.getElementById('priceGroup').style.display = e.target.value === 'paid' ? '' : 'none';
});

async function handleFiles(files) {
  for (const file of files) {
    await uploadFile(file);
  }
  loadDatasets();
}

async function uploadFile(file) {
  const status = document.getElementById('uploadStatus');
  const fill = document.getElementById('uploadFill');
  const msg = document.getElementById('uploadMsg');
  status.classList.add('active');
  fill.style.width = '30%';
  msg.textContent = `Uploading ${file.name}...`;

  const form = new FormData();
  form.append('file', file);
  form.append('privacy_level', document.getElementById('privacyLevel').value);
  form.append('epsilon', document.getElementById('epsilon').value);
  form.append('access', document.getElementById('accessMode').value);
  form.append('price', document.getElementById('price').value);

  try {
    fill.style.width = '60%';
    const res = await fetch(API + '/api/publish', { method: 'POST', body: form });
    const data = await res.json();
    fill.style.width = '100%';
    if (data.error) {
      msg.textContent = '❌ ' + data.error;
    } else {
      msg.textContent = `✅ Published ${file.name} — CID: ${data.cid.slice(0, 16)}...`;
    }
  } catch (e) {
    fill.style.width = '100%';
    msg.textContent = '❌ Upload failed: ' + e.message;
  }
  setTimeout(() => { status.classList.remove('active'); fill.style.width = '0'; }, 4000);
}

// --- Load datasets ---
async function loadDatasets() {
  try {
    const res = await fetch(API + '/api/datasets');
    const datasets = await res.json();
    renderDatasets(datasets);
    document.getElementById('statusDot').style.background = 'var(--green)';
    document.getElementById('statusText').textContent = `Node online · ${datasets.length} datasets`;
  } catch {
    document.getElementById('statusDot').style.background = 'var(--red)';
    document.getElementById('statusText').textContent = 'Node offline';
    document.getElementById('dsList').innerHTML = '<div class="empty">Cannot connect to node. Run <code>guixu start</code> first.</div>';
  }
}

function renderDatasets(datasets) {
  const el = document.getElementById('dsList');
  document.getElementById('dsCount').textContent = datasets.length;
  if (!datasets.length) {
    el.innerHTML = '<div class="empty">No datasets published yet. Drop a file above to get started.</div>';
    return;
  }
  el.innerHTML = datasets.map(d => {
    const cols = (d.schema?.columns || []).map(c => {
      const cls = c.name.startsWith('h_') ? 'col hashed' : 'col';
      return `<span class="${cls}">${c.name}</span>`;
    }).join('');
    const badge = d.price?.amount > 0
      ? `<span class="badge paid">$${d.price.amount} USDC</span>`
      : '<span class="badge open">Free</span>';
    const rows = d.schema?.row_count?.toLocaleString() || '?';
    const size = formatBytes(d.schema?.size_bytes || 0);
    const time = new Date(d.updated_at).toLocaleDateString();
    return `<div class="ds-card">
      <div class="ds-title">${esc(d.title)} ${badge}</div>
      <div class="ds-meta">
        <span>📊 ${rows} rows</span>
        <span>💾 ${size}</span>
        <span>📅 ${time}</span>
        <span>🔑 ${esc(d.provider?.slice(0,24))}...</span>
      </div>
      <div class="ds-cid">CID: ${esc(d.cid)}</div>
      ${cols ? '<div class="ds-cols">' + cols + '</div>' : ''}
    </div>`;
  }).join('');
}

function formatBytes(b) {
  if (b < 1024) return b + ' B';
  if (b < 1048576) return (b/1024).toFixed(1) + ' KB';
  return (b/1048576).toFixed(1) + ' MB';
}
function esc(s) { const d = document.createElement('div'); d.textContent = s || ''; return d.innerHTML; }

// Initial load
loadDatasets();
setInterval(loadDatasets, 10000);
</script>
</body>
</html>"##;
