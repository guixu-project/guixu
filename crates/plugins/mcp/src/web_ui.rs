// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

/// Embedded Preact+HTM SPA for the Guixu node management UI.
/// Served at GET / when running `guixu start` or `guixu mcp --mode http`.
/// Zero external dependencies — everything is inlined.
pub const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Guixu — P2P Data Node</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
:root{--bg:#0a0e17;--surface:#111827;--border:#1e293b;--text:#e2e8f0;--dim:#64748b;--accent:#3b82f6;--green:#22c55e;--red:#ef4444;--yellow:#eab308}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:var(--bg);color:var(--text);min-height:100vh;display:flex}
nav{width:200px;background:var(--surface);border-right:1px solid var(--border);padding:16px 0;position:fixed;height:100vh;overflow-y:auto}
nav .logo{padding:0 16px 16px;font-size:18px;font-weight:700;border-bottom:1px solid var(--border);margin-bottom:8px}
nav a{display:block;padding:10px 16px;color:var(--dim);text-decoration:none;font-size:14px;transition:all .15s}
nav a:hover,nav a.active{color:var(--text);background:rgba(59,130,246,.1)}
nav a.active{border-right:2px solid var(--accent)}
main{margin-left:200px;flex:1;padding:24px;max-width:1100px}
h2{font-size:20px;margin-bottom:16px}
.card{background:var(--surface);border:1px solid var(--border);border-radius:8px;padding:16px;margin-bottom:12px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px;margin-bottom:24px}
.stat{text-align:center;padding:20px}
.stat .val{font-size:28px;font-weight:700;color:var(--accent)}
.stat .lbl{font-size:12px;color:var(--dim);margin-top:4px}
.badge{display:inline-block;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600}
.badge.open{background:rgba(34,197,94,.15);color:var(--green)}
.badge.paid{background:rgba(234,179,8,.15);color:var(--yellow)}
.badge.seeding{background:rgba(59,130,246,.15);color:var(--accent)}
table{width:100%;border-collapse:collapse;font-size:13px}
th{text-align:left;padding:8px;color:var(--dim);border-bottom:1px solid var(--border);font-weight:500}
td{padding:8px;border-bottom:1px solid var(--border)}
tr:hover td{background:rgba(59,130,246,.03)}
.mono{font-family:monospace;font-size:11px;color:var(--dim)}
input,select{background:var(--bg);border:1px solid var(--border);color:var(--text);padding:8px 12px;border-radius:6px;font-size:13px}
button{background:var(--accent);color:#fff;border:none;padding:8px 16px;border-radius:6px;cursor:pointer;font-size:13px}
button:hover{opacity:.9}
button.danger{background:var(--red)}
.dropzone{border:2px dashed var(--border);border-radius:12px;padding:40px;text-align:center;cursor:pointer;transition:all .2s;margin-bottom:16px}
.dropzone:hover{border-color:var(--accent);background:rgba(59,130,246,.05)}
.dropzone input{display:none}
.empty{text-align:center;padding:40px;color:var(--dim)}
.search-bar{display:flex;gap:8px;margin-bottom:16px}
.search-bar input{flex:1}
.hidden{display:none}
</style>
</head>
<body>
<nav>
  <div class="logo">⚡ Guixu</div>
  <a href="#/" class="active" data-page="dashboard">📊 Dashboard</a>
  <a href="#/datasets" data-page="datasets">📁 Datasets</a>
  <a href="#/publish" data-page="publish">📤 Publish</a>
  <a href="#/market" data-page="market">🌐 Market</a>
  <a href="#/wallet" data-page="wallet">💰 Wallet</a>
  <a href="#/settings" data-page="settings">⚙️ Settings</a>
</nav>
<main id="app"></main>
<script>
const API=window.location.origin;
let state={page:'dashboard',node:{},datasets:[],seeds:[],market:[]};

// Router
function route(){
  const h=location.hash.slice(1)||'/';
  const page=h==='/'?'dashboard':h.slice(1).split('/')[0];
  state.page=page;
  document.querySelectorAll('nav a').forEach(a=>{
    a.classList.toggle('active',a.dataset.page===page);
  });
  render();
}
window.addEventListener('hashchange',route);

// Fetch helpers
async function api(path){
  try{const r=await fetch(API+path);return await r.json();}catch{return null;}
}

async function loadNode(){state.node=await api('/api/node/status')||{};}
async function loadDatasets(){state.datasets=await api('/api/datasets')||[];}

// Render
function render(){
  const el=document.getElementById('app');
  switch(state.page){
    case 'dashboard':el.innerHTML=renderDashboard();break;
    case 'datasets':el.innerHTML=renderDatasets();break;
    case 'publish':el.innerHTML=renderPublish();setupPublish();break;
    case 'market':el.innerHTML=renderMarket();break;
    case 'wallet':el.innerHTML=renderWallet();break;
    case 'settings':el.innerHTML=renderSettings();break;
    default:el.innerHTML='<div class="empty">Page not found</div>';
  }
}

function esc(s){const d=document.createElement('div');d.textContent=s||'';return d.innerHTML;}
function fmtBytes(b){if(!b)return'0 B';if(b<1024)return b+' B';if(b<1048576)return(b/1024).toFixed(1)+' KB';if(b<1073741824)return(b/1048576).toFixed(1)+' MB';return(b/1073741824).toFixed(1)+' GB';}

function renderDashboard(){
  const n=state.node;const ds=state.datasets;
  const totalSize=ds.reduce((s,d)=>s+(d.schema?.size_bytes||0),0);
  const seedCount=ds.filter(d=>d.info_hash).length;
  return`<h2>Dashboard</h2>
  <div class="grid">
    <div class="card stat"><div class="val">${esc(n.status||'offline')}</div><div class="lbl">Node Status</div></div>
    <div class="card stat"><div class="val">${ds.length}</div><div class="lbl">Published Datasets</div></div>
    <div class="card stat"><div class="val">${seedCount}</div><div class="lbl">Seeding</div></div>
    <div class="card stat"><div class="val">${fmtBytes(totalSize)}</div><div class="lbl">Total Size</div></div>
  </div>
  <div class="card">
    <h3 style="margin-bottom:8px">Node Info</h3>
    <table>
      <tr><td style="color:var(--dim)">DID</td><td class="mono">${esc(n.did||'-')}</td></tr>
      <tr><td style="color:var(--dim)">Peer ID</td><td class="mono">${esc(n.peer_id||'-')}</td></tr>
      <tr><td style="color:var(--dim)">Version</td><td>${esc(n.version||'-')}</td></tr>
    </table>
  </div>`;
}

function renderDatasets(){
  const ds=state.datasets;
  if(!ds.length)return'<h2>My Datasets</h2><div class="empty">No datasets published yet.</div>';
  const rows=ds.map(d=>{
    const price=d.price?.amount>0?`$${d.price.amount}`:'free';
    const badge=d.price?.amount>0?'paid':'open';
    const status=d.info_hash?'<span class="badge seeding">seeding</span>':'local';
    return`<tr>
      <td class="mono">${esc((d.cid||'').slice(0,12))}…</td>
      <td>${esc(d.title)}</td>
      <td>${(d.schema?.row_count||0).toLocaleString()}</td>
      <td>${fmtBytes(d.schema?.size_bytes)}</td>
      <td><span class="badge ${badge}">${price}</span></td>
      <td>${status}</td>
    </tr>`;
  }).join('');
  return`<h2>My Datasets (${ds.length})</h2>
  <div class="card"><table>
    <tr><th>CID</th><th>Title</th><th>Rows</th><th>Size</th><th>Price</th><th>Status</th></tr>
    ${rows}
  </table></div>`;
}

function renderPublish(){
  return`<h2>Publish Dataset</h2>
  <div class="dropzone" id="dropzone">
    <h3>📂 Drop files here or click to browse</h3>
    <p style="color:var(--dim);margin-top:8px">CSV, Parquet, JSON, TSV</p>
    <input type="file" id="fileInput" accept=".csv,.parquet,.json,.tsv">
  </div>
  <div class="card" style="display:flex;gap:16px;flex-wrap:wrap;align-items:end">
    <div><label style="font-size:12px;color:var(--dim)">Privacy</label><br>
      <select id="privacyLevel"><option value="off">Off</option><option value="standard" selected>Standard</option><option value="strict">Strict</option></select></div>
    <div><label style="font-size:12px;color:var(--dim)">Access</label><br>
      <select id="accessMode"><option value="open">Open</option><option value="paid">Paid</option></select></div>
    <div><label style="font-size:12px;color:var(--dim)">Price (USDC)</label><br>
      <input type="number" id="price" value="0" min="0" step="0.01" style="width:80px"></div>
  </div>
  <div id="uploadMsg" style="margin-top:12px;font-size:13px;color:var(--dim)"></div>`;
}

function setupPublish(){
  const dz=document.getElementById('dropzone');
  const fi=document.getElementById('fileInput');
  if(!dz)return;
  dz.onclick=()=>fi.click();
  dz.ondragover=e=>{e.preventDefault();dz.style.borderColor='var(--accent)';};
  dz.ondragleave=()=>{dz.style.borderColor='var(--border)';};
  dz.ondrop=e=>{e.preventDefault();dz.style.borderColor='var(--border)';uploadFiles(e.dataTransfer.files);};
  fi.onchange=()=>uploadFiles(fi.files);
}

async function uploadFiles(files){
  const msg=document.getElementById('uploadMsg');
  for(const f of files){
    msg.textContent=`Uploading ${f.name}...`;
    const form=new FormData();
    form.append('file',f);
    form.append('privacy_level',document.getElementById('privacyLevel')?.value||'standard');
    form.append('access',document.getElementById('accessMode')?.value||'open');
    form.append('price',document.getElementById('price')?.value||'0');
    try{
      const r=await fetch(API+'/api/publish',{method:'POST',body:form});
      const d=await r.json();
      msg.textContent=d.error?'❌ '+d.error:`✅ Published ${f.name} — CID: ${(d.cid||'').slice(0,16)}…`;
      await loadDatasets();
    }catch(e){msg.textContent='❌ '+e.message;}
  }
}

function renderMarket(){
  return`<h2>Data Market</h2>
  <div class="search-bar"><input type="text" id="marketQuery" placeholder="Search datasets across the P2P network...">
    <button onclick="searchMarket()">Search</button></div>
  <div id="marketResults" class="empty">Enter a query to search the decentralized data market.</div>`;
}

async function searchMarket(){
  const q=document.getElementById('marketQuery')?.value;
  if(!q)return;
  const el=document.getElementById('marketResults');
  el.innerHTML='Searching...';
  const data=await api('/api/market/search?q='+encodeURIComponent(q));
  const results=data?.results||[];
  if(!results.length){el.innerHTML='<div class="empty">No results found.</div>';return;}
  el.innerHTML='<div class="card"><table><tr><th>CID</th><th>Title</th><th>Source</th><th>Price</th></tr>'+
    results.map(r=>`<tr><td class="mono">${esc((r.cid||'').slice(0,12))}…</td><td>${esc(r.title)}</td><td>${esc(r.source)}</td><td>${r.price?.amount>0?'$'+r.price.amount:'free'}</td></tr>`).join('')+
    '</table></div>';
}
window.searchMarket=searchMarket;

function renderWallet(){
  return`<h2>Wallet</h2>
  <div class="grid">
    <div class="card stat"><div class="val">$0.00</div><div class="lbl">USDC Balance</div></div>
    <div class="card stat"><div class="val">0</div><div class="lbl">Transactions</div></div>
  </div>
  <div class="card"><p style="color:var(--dim)">Transaction history will appear here once you start trading datasets.</p></div>`;
}

function renderSettings(){
  return`<h2>Settings</h2>
  <div class="card">
    <h3 style="margin-bottom:12px">Provider Configuration</h3>
    <table>
      <tr><td style="color:var(--dim)">Auto-publish</td><td>Enabled</td></tr>
      <tr><td style="color:var(--dim)">Default access</td><td>Open</td></tr>
      <tr><td style="color:var(--dim)">Watermark</td><td>Disabled</td></tr>
      <tr><td style="color:var(--dim)">Max seeds</td><td>50</td></tr>
    </table>
  </div>
  <div class="card">
    <h3 style="margin-bottom:12px">Network</h3>
    <table>
      <tr><td style="color:var(--dim)">Relay enabled</td><td>Yes</td></tr>
      <tr><td style="color:var(--dim)">AutoNAT</td><td>Enabled</td></tr>
    </table>
  </div>`;
}

// Init
(async()=>{
  await Promise.all([loadNode(),loadDatasets()]);
  route();
  setInterval(async()=>{await loadNode();if(state.page==='dashboard')render();},10000);
})();
</script>
</body>
</html>"##;
