# DIP001: P2P Dataset Search Protocol for AI Agents — 架构设计文档

> 版本：v0.1 | 日期：2026-03-23
>
> 基于 DIP001 调研文档，本文档将协议落实到具体技术架构，定义五大核心组件层的技术选型、交互方式、技术挑战与解决方案。

---

## 一、系统总览

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Agent Interface Layer                         │
│                   MCP Server (JSON-RPC over stdio/SSE)               │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌────────────┐  │
│  │  Data Search  │ │Data Valuation│ │ Data Trading │ │    Data    │  │
│  │    Engine     │ │   Engine     │ │   Engine     │ │   Auth     │  │
│  │              │ │              │ │              │ │   Engine   │  │
│  │ - 语义检索    │ │ - 质量评分    │ │ - 托管交易    │ │ - 来源签名  │  │
│  │ - DHT 索引   │ │ - 动态定价    │ │ - 微支付     │ │ - 完整性   │  │
│  │ - 元数据聚合  │ │ - Shapley    │ │ - 许可证协商  │ │ - ZKP 证明 │  │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘ └─────┬──────┘  │
│         │                │                │               │          │
├─────────┴────────────────┴────────────────┴───────────────┴──────────┤
│                     P2P Data Sharing Layer                            │
│          libp2p + BitTorrent Protocol + Content-Addressed Storage     │
└──────────────────────────────────────────────────────────────────────┘
```

**设计原则：**
- Agent-Native：所有接口通过 MCP 协议暴露，Agent 零适配成本
- 去中心化优先：无中心服务器，节点对等
- 可验证：数据从发布到消费全链路可验证
- 可组合：每层独立可用，组合使用时形成完整工作流

---

## 二、核心组件层详细设计

---

### 2.1 P2P Data Sharing Layer（P2P 数据共享层）

> 负责数据集的分布式存储、分发和节点间通信。

#### 技术架构

```
┌─────────────────────────────────────────────────┐
│              Sharing Layer API                    │
│  publish() / fetch() / seed() / getPeers()       │
├─────────────────────────────────────────────────┤
│                                                   │
│  ┌─────────────────┐  ┌───────────────────────┐  │
│  │  Metadata Plane  │  │    Data Plane         │  │
│  │                  │  │                       │  │
│  │  libp2p Stack:   │  │  BitTorrent Engine:   │  │
│  │  - Kademlia DHT  │  │  - Piece-based chunk  │  │
│  │  - GossipSub     │  │  - Rarest-first algo  │  │
│  │  - mDNS (LAN)    │  │  - Tit-for-tat       │  │
│  │  - Noise crypto  │  │  - Magnet URI scheme  │  │
│  │                  │  │                       │  │
│  │  存储: 元数据     │  │  存储: 数据集分片      │  │
│  │  (JSON-LD/CID)   │  │  (Parquet chunks)     │  │
│  └─────────────────┘  └───────────────────────┘  │
│                                                   │
│  ┌─────────────────────────────────────────────┐  │
│  │           Content-Addressed Storage          │  │
│  │  CID = hash(dataset_chunk)                   │  │
│  │  本地存储: RocksDB / SQLite                   │  │
│  └─────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

#### 技术选型

| 组件 | 技术 | 理由 |
|------|------|------|
| 节点通信 | **libp2p** (Rust: rust-libp2p) | Agent P2P 事实标准；OpenPond/DIAP 已验证；模块化可插拔 |
| 数据分发 | **BitTorrent Protocol v2** (BEP 52) | Merkle tree 分片验证；成熟的 swarm 激励机制；支持大文件高效分发 |
| 元数据路由 | **Kademlia DHT** (libp2p-kad) | O(log N) 查找复杂度；自愈网络拓扑 |
| 新数据集广播 | **GossipSub v1.1** (libp2p) | 低延迟 pub/sub；抗 Sybil 的 mesh 网络 |
| 局域网发现 | **mDNS** (libp2p-mdns) | 零配置本地节点发现 |
| 传输加密 | **Noise Protocol** (libp2p-noise) | 前向保密；无需 CA |
| 内容寻址 | **CID v1** (Multihash) | 与 IPFS 生态兼容；自验证内容 |
| 本地存储 | **RocksDB** | 高性能 KV 存储；LSM-tree 适合写密集场景 |

#### 双平面设计说明

采用 **Metadata Plane + Data Plane 分离** 架构：

- **Metadata Plane (libp2p)**：轻量级，传播数据集元信息（名称、schema、CID、大小、提供者 DID），所有节点参与 DHT 索引
- **Data Plane (BitTorrent)**：重量级，实际数据集分片传输，仅需要数据的节点参与 swarm

这样设计的好处是：搜索时只查 DHT（毫秒级），下载时才启动 BitTorrent swarm（高吞吐）。

#### ⭐ 免费数据 vs 付费数据：双模式分发架构

Sharing 层必须区分两种根本不同的数据分发场景：

```
┌─────────────────────────────────────────────────────────────────┐
│                    Data Distribution Modes                       │
│                                                                  │
│  ┌──────────────────────────┐  ┌──────────────────────────────┐ │
│  │  Mode A: Open Swarm       │  │  Mode B: Seller-Only Seeding │ │
│  │  (免费/开放数据集)         │  │  (付费/版权数据集)            │ │
│  │                           │  │                              │ │
│  │  标准 BitTorrent v2:      │  │  受限分发:                    │ │
│  │  - 任意节点可 seed        │  │  - 仅卖方节点 seed           │ │
│  │  - 下载者自动成为 seeder  │  │  - 买方下载后 ❌ 不缓存分片  │ │
│  │  - Tit-for-tat 激励      │  │  - 传输层加密 (TLS 1.3)     │ │
│  │  - 网络效应: 越多人下载   │  │  - 数据集嵌入买方水印        │ │
│  │    速度越快               │  │  - 许可证绑定买方 DID        │ │
│  │                           │  │                              │ │
│  │  元数据标记:               │  │  元数据标记:                  │ │
│  │  "access": "open"         │  │  "access": "paid"            │ │
│  │  "license": "CC-BY-4.0"  │  │  "license": "commercial"     │ │
│  └──────────────────────────┘  └──────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

**Mode B (付费数据) 的关键设计：**

1. **Seller-Only Seeding**：付费数据集的 BitTorrent swarm 中，只有卖方节点作为 seeder。买方节点下载完成后，协议层强制不缓存分片（不写入本地 piece store），防止买方成为非授权 seeder 向第三方分发。技术实现：在 BitTorrent 引擎中增加 `no_cache` 标志位，下载的分片在组装为完整文件后立即从 piece store 中清除。

2. **传输层端到端加密**：卖方→买方的数据传输使用 TLS 1.3 加密通道（在 libp2p Noise 之上再加一层），确保中间 relay 节点无法窥探数据内容。

3. **数据水印 (Dataset Watermarking)**：

```
水印嵌入流程 (卖方节点，每次交易独立执行):

  原始数据集
      │
      ▼
  ┌─────────────────────────────────────────┐
  │  Watermark Embedder                      │
  │                                          │
  │  技术: HashMark (Cryptographic Hashing)  │
  │                                          │
  │  1. 生成交易唯一水印密钥:                  │
  │     wm_key = HMAC(seller_sk,             │
  │              buyer_did || tx_id || cid)   │
  │                                          │
  │  2. 选择水印嵌入位:                        │
  │     - 数值列: LSB (最低有效位) 微扰        │
  │       (误差 < 0.01%, 不影响统计特征)       │
  │     - 分类列: 同义词替换 / 行顺序置换      │
  │     - 行级: 插入少量合成哨兵行             │
  │       (统计分布一致但可密码学识别)          │
  │                                          │
  │  3. 水印提取 (版权追溯时):                  │
  │     泄露数据 → 提取嵌入位 → 反推 wm_key   │
  │     → 匹配交易记录 → 定位泄露买家 DID      │
  └─────────────────────────────────────────┘
      │
      ▼
  水印版数据集 → 加密传输给买方
```

| 水印技术 | 方法 | 适用数据类型 | 鲁棒性 |
|---------|------|------------|--------|
| **HashMark** | 基于密码学哈希的 binning 水印 | 表格/结构化数据 | 抗行删除、列删除、噪声添加 |
| **LSB 微扰** | 数值最低有效位嵌入 | 数值型列 | 抗统计攻击，误差可控 |
| **合成哨兵行** | 插入统计一致的假行 | 任意表格数据 | 抗采样攻击，可密码学验证 |
| **行顺序指纹** | 行排列编码买方身份 | 任意数据集 | 抗内容修改，弱于行删除 |

#### 数据集发布流程

```
Publisher Node:
  1. 数据集 → Parquet 格式标准化
  2. Parquet → 固定大小分片 (默认 256KB pieces)
  3. 每个分片 → SHA-256 hash → Merkle Tree
  4. 生成 BitTorrent v2 Info Hash (Merkle Root)
  5. 构造元数据 JSON-LD:
     {
       "@type": "Dataset",
       "cid": "bafybeig...",           // 内容寻址 ID
       "infoHash": "v2:abc123...",      // BT v2 info hash
       "schema": { ... },              // 列定义
       "rowCount": 50000,
       "sizeBytes": 12000000,
       "provider": "did:key:z6Mk...",  // 发布者 DID
       "signature": "0x...",           // 对元数据的签名
       "license": "CC-BY-4.0",
       "createdAt": "2026-03-23T00:00:00Z"
     }
  6. 元数据 → DHT PUT (key = CID)
  7. 元数据 → GossipSub 广播到 "datasets" topic
  8. 开始 seed 数据分片
```

#### 技术挑战与解决方案

| 挑战 | 描述 | 解决方案 |
|------|------|---------|
| **冷启动无 Seeder** | 新发布的数据集只有发布者一个 seed，下载慢且单点故障 | 引入 **Super-Seed 激励节点**：协议内置 token 激励，早期 seeder 获得更多奖励（类似 Filecoin 的存储激励）；同时支持 **WebSeed (BEP 19)**，发布者可提供 HTTP fallback URL |
| **大数据集传输效率** | GB 级数据集在 P2P 网络中传输慢 | BitTorrent v2 的 **Merkle Tree 分片** + **Rarest-First 算法** 天然优化；额外实现 **Range Request**：Agent 可只下载数据集的前 N 行用于预览，无需下载全量 |
| **DHT 元数据污染** | 恶意节点向 DHT 注入虚假元数据 | 所有元数据必须携带发布者 DID 签名；DHT 查询结果需验证签名有效性；引入 **声誉加权**：高声誉节点的元数据优先展示 |
| **NAT 穿透** | 家庭网络节点无法被直接连接 | libp2p 内置 **AutoNAT + Circuit Relay v2**：自动检测 NAT 类型，必要时通过 relay 节点中转；BitTorrent 侧使用 **uTP + Hole Punching** |
| **数据集版本管理** | 数据集更新后 CID 变化，旧引用失效 | 引入 **IPNS 式可变指针**：`did:key:z6Mk.../datasets/my-dataset` → 始终指向最新版本 CID；历史版本通过 CID 链保留 |
| **付费数据防泄露** | 买方可能绕过 no_cache 限制，手动保存分片后二次分发 | **多层防御**：(1) 水印追溯 — 每份交易数据集嵌入唯一水印，泄露后可追溯到具体买方 DID；(2) 经济惩罚 — 买方需 stake 保证金，被证实泄露后 slash；(3) 合成哨兵行 — 在数据中插入密码学可验证的假行，任何人发现泄露数据中的哨兵行即可发起链上举报 |
| **水印鲁棒性** | 买方可能通过添加噪声、删除行列、重新排序等方式去除水印 | **组合水印策略**：同时使用 LSB 微扰 + 哨兵行 + 行顺序指纹三种正交水印，攻击者需同时破解三种才能完全去除；HashMark 的密码学 binning 方案已被证明可抗 30% 行删除和 5% 噪声添加 |
| **Seller-Only Seeding 性能瓶颈** | 付费数据集只有卖方节点 seed，高并发购买时带宽不足 | **授权 Seeder 网络**：卖方可授权可信节点（如付费 CDN 节点）作为 delegated seeder，授权通过签名委托实现；delegated seeder 持有加密分片，只有持有买方授权 token 的请求才能解密 |


---

### 2.2 Data Search Engine（数据搜索引擎层）

> 负责跨源数据集发现、语义理解和智能排序。

#### 技术架构

```
┌──────────────────────────────────────────────────────┐
│                  Search API (MCP Tools)                │
│  search(query, filters) → ranked results              │
│  discover(capability) → matching datasets             │
│  preview(cid, rows) → sample data                     │
├──────────────────────────────────────────────────────┤
│                                                        │
│  ┌────────────────┐  ┌─────────────────────────────┐  │
│  │  Query Engine   │  │     Index Engine            │  │
│  │                 │  │                             │  │
│  │  NL → 结构化:   │  │  本地向量索引:               │  │
│  │  - Intent Parse │  │  - Qdrant (embedded mode)   │  │
│  │  - Schema Match │  │  - 数据集描述 embedding      │  │
│  │  - Filter Gen   │  │  - Schema 结构 embedding    │  │
│  │                 │  │                             │  │
│  │  LLM:           │  │  DHT 分布式索引:             │  │
│  │  - 本地 SLM     │  │  - Kademlia key-value      │  │
│  │  - 或远程 API   │  │  - Prefix scan for tags    │  │
│  └────────────────┘  └─────────────────────────────┘  │
│                                                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │              Aggregation Layer                     │  │
│  │  - DHT 本地索引 (P2P 网络内数据集)                  │  │
│  │  - Kaggle API adapter                             │  │
│  │  - HuggingFace API adapter                        │  │
│  │  - schema.org/Dataset 爬虫 (可选)                  │  │
│  └──────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

#### 技术选型

| 组件 | 技术 | 理由 |
|------|------|------|
| 向量索引 | **Qdrant (embedded mode)** | Rust 实现，可嵌入进程内运行；支持 HNSW 索引；无需独立部署 |
| Embedding 模型 | **all-MiniLM-L6-v2** (ONNX Runtime) | 384 维，推理快（<10ms/query）；本地运行无需网络 |
| 意图解析 | **本地 SLM (Phi-3-mini / Qwen2-0.5B)** | 将自然语言查询转为结构化 filter；本地推理保护隐私 |
| 分布式索引 | **Kademlia DHT** (复用 Sharing 层) | 元数据已在 DHT 中，搜索层直接查询 |
| 外部源适配 | **Adapter Pattern** | 每个外部平台一个 adapter（Kaggle/HF/data.gov），统一输出格式 |
| 元数据标准 | **Croissant (JSON-LD)** 扩展 | 兼容 Kaggle/HF 已有标准；扩展 Agent 专用字段 |

#### 搜索流程

```
Agent: search("中国各省 GDP 时间序列数据，2020-2025")
  │
  ├─ 1. Intent Parse (本地 SLM):
  │     → { topic: "GDP", geo: "China/provinces",
  │         temporal: "2020-2025", format: "time_series" }
  │
  ├─ 2. Parallel Search:
  │     ├─ DHT lookup: prefix scan "gdp", "china", "province"
  │     ├─ Vector search: embed(query) → top-K from Qdrant
  │     ├─ Kaggle adapter: kaggle.datasets.search("china gdp")
  │     └─ HF adapter: hf.datasets.search("china gdp")
  │
  ├─ 3. Merge & Deduplicate:
  │     → 按 CID/URL 去重，合并来源信息
  │
  ├─ 4. Rank (多因子排序):
  │     → score = 0.4 * relevance + 0.2 * quality
  │              + 0.2 * freshness + 0.1 * popularity
  │              + 0.1 * provider_reputation
  │
  └─ 5. Return structured results:
        [
          { cid, title, schema, rows, size, quality_score,
            price, license, provider_did, sources: ["p2p","kaggle"] }
        ]
```

#### 技术挑战与解决方案

| 挑战 | 描述 | 解决方案 |
|------|------|---------|
| **DHT 语义搜索能力弱** | Kademlia DHT 只支持精确 key 查找，不支持模糊/语义搜索 | **双层索引**：DHT 存储精确 tag→CID 映射（倒排索引思路）；本地 Qdrant 存储 embedding 向量用于语义搜索。新元数据通过 GossipSub 同步到各节点的本地 Qdrant |
| **跨源结果异构** | Kaggle/HF/P2P 返回的元数据格式不同 | 定义 **统一 DatasetRecord schema**（基于 Croissant 扩展），每个 adapter 负责转换为统一格式 |
| **本地索引同步延迟** | 新发布的数据集需要时间传播到所有节点的本地索引 | GossipSub 实时广播新数据集元数据；节点收到后立即更新本地 Qdrant 索引；DHT 作为最终一致性保证（GossipSub 丢失时可从 DHT 补全） |
| **搜索结果质量** | 无法保证排序结果真正匹配 Agent 需求 | 引入 **反馈循环**：Agent 下载并使用数据集后，上报 relevance feedback（有用/无用）；feedback 通过链上声誉系统影响后续排序权重 |
| **隐私泄露风险** | 搜索查询暴露 Agent 的数据需求意图 | 支持 **Private Information Retrieval (PIR)** 模式：Agent 可选择对 DHT 查询进行混淆（k-anonymity），或通过 Tor/mixnet 路由查询请求 |


---

### 2.3 Data Authentication Engine（数据认证引擎层）

> 负责数据集的来源验证、完整性校验和属性证明。

#### 技术架构

```
┌──────────────────────────────────────────────────────┐
│              Authentication API (MCP Tools)            │
│  verify(cid) → { integrity, provenance, attributes }  │
│  sign(dataset) → signed_metadata                      │
│  proveAttribute(cid, predicate) → zk_proof            │
├──────────────────────────────────────────────────────┤
│                                                        │
│  ┌────────────────┐ ┌──────────────┐ ┌─────────────┐  │
│  │   Provenance   │ │  Integrity   │ │  Attribute  │  │
│  │   Verifier     │ │  Verifier    │ │  Prover     │  │
│  │                │ │              │ │             │  │
│  │  DID 签名验证:  │ │  Merkle 验证: │ │  ZKP 电路:  │  │
│  │  - did:key     │ │  - BT v2     │ │  - Noir     │  │
│  │  - did:web     │ │    Merkle    │ │  - Circom   │  │
│  │  - did:ethr    │ │    Tree      │ │             │  │
│  │                │ │  - 分片级     │ │  可证明:     │  │
│  │  链上锚定:      │ │    校验      │ │  - 行数范围  │  │
│  │  - EAS         │ │              │ │  - 列类型   │  │
│  │    (Ethereum   │ │  CID 自验证:  │ │  - 统计特征 │  │
│  │    Attestation │ │  - Multihash │ │  - 非空率   │  │
│  │    Service)    │ │    重算验证   │ │             │  │
│  └────────────────┘ └──────────────┘ └─────────────┘  │
│                                                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │           Dataset Credential (VC)                  │  │
│  │  W3C Verifiable Credential 格式封装:                │  │
│  │  - issuer: 发布者 DID                              │  │
│  │  - subject: 数据集 CID                             │  │
│  │  - claims: schema, stats, provenance               │  │
│  │  - proof: Ed25519Signature2020                     │  │
│  └──────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

#### 技术选型

| 组件 | 技术 | 理由 |
|------|------|------|
| 身份标识 | **DID (did:key + did:ethr)** | did:key 零成本自生成；did:ethr 可链上锚定获得更高信任 |
| 签名算法 | **Ed25519** | 高性能（76K ops/sec）；libp2p 原生支持 |
| 完整性验证 | **BitTorrent v2 Merkle Tree** | 复用 Sharing 层的分片 Merkle Tree；支持单分片级验证 |
| 来源锚定 | **EAS (Ethereum Attestation Service)** | 链上不可篡改的发布记录；Gas 成本低（L2 部署）；已有成熟生态 |
| 零知识证明 | **Noir (Aztec)** | Rust-like DSL，开发体验好；PLONK 后端，证明生成快；DIAP 已验证可用于 Agent 场景 |
| 凭证格式 | **W3C Verifiable Credentials (VC)** | 标准化的可验证声明格式；与 DID 生态无缝集成 |

#### 认证流程

```
发布者签名流程:
  1. 计算数据集 Merkle Root (BitTorrent v2)
  2. 生成 Dataset Credential (VC):
     {
       "@context": ["https://www.w3.org/2018/credentials/v1"],
       "type": ["VerifiableCredential", "DatasetCredential"],
       "issuer": "did:key:z6MkPublisher...",
       "credentialSubject": {
         "id": "cid:bafybeig...",
         "merkleRoot": "0xabc...",
         "schema": { "columns": [...], "rowCount": 50000 },
         "stats": { "nullRate": 0.02, "uniqueRate": 0.95 },
         "provenance": "original",  // original | derived | aggregated
         "createdAt": "2026-03-23T00:00:00Z"
       },
       "proof": { "type": "Ed25519Signature2020", ... }
     }
  3. (可选) 锚定到 EAS: attestation(schema, data) → onchain tx
  4. VC 附加到元数据，一起发布到 DHT

验证者校验流程:
  1. 从 DHT 获取元数据 + VC
  2. 验证 VC 签名 (Ed25519)
  3. 下载数据分片 → 重算 Merkle Root → 与 VC 中声明比对
  4. (可选) 查询 EAS 链上记录确认发布时间
  5. (可选) 请求 ZKP 证明特定属性 (如 "行数 > 10000")
```

#### 技术挑战与解决方案

| 挑战 | 描述 | 解决方案 |
|------|------|---------|
| **数据质量不可证明** | Merkle Tree 只能证明"数据没被篡改"，无法证明"数据是高质量的" | **分层认证**：Level 1 = 完整性（Merkle，自动）；Level 2 = 自声明统计（VC 中的 stats，发布者签名）；Level 3 = 第三方审计（可信审计节点验证后签发额外 VC）；Level 4 = ZKP 属性证明（数学保证） |
| **ZKP 证明生成慢** | 对大数据集生成 ZKP 证明计算量巨大 | **采样证明**：不对全量数据生成证明，而是对随机采样子集生成 ZKP；采样种子由验证者提供（防止发布者挑选有利样本）；Noir 电路预编译为 WASM，客户端可快速验证 |
| **DID 信任冷启动** | 新 DID 没有历史记录，无法判断可信度 | **渐进式信任**：新 DID 发布的数据集默认低信任分；通过 EAS 链上锚定提升信任；其他节点的正面 attestation 累积声誉；引入 **Web of Trust**：已知可信 DID 为新 DID 背书 |
| **派生数据集溯源** | 数据集经过清洗/合并后，如何追溯原始来源 | **Provenance Chain**：VC 中包含 `derivedFrom: [cid1, cid2]` 字段；形成 DAG 结构的溯源图；任何人可沿 DAG 追溯到原始数据集 |
| **隐私数据集的认证悖论** | 要验证数据质量就需要看到数据，但数据是隐私的 | **ZKP + Compute-to-Data 组合**：发布者在本地对数据运行 ZKP 电路，生成属性证明（如"包含 >10000 行，无空值率 <5%"）；买家验证 ZKP 证明即可，无需看到原始数据 |


---

### 2.4 Data Trading Engine（数据交易引擎层）

> 负责数据集的定价展示、托管交易、自动支付和许可证协商。

#### 技术架构

```
┌───────────────────────────────────────────────────────────────────┐
│                    Trading API (MCP Tools)                         │
│  preview(cid, n) → sample rows                                    │
│  negotiate(cid, terms) → offer                                    │
│  purchase(cid, offer) → receipt + access_token                     │
│  refund(receipt, reason) → dispute                                 │
├───────────────────────────────────────────────────────────────────┤
│                                                                    │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │              Multi-Protocol Payment Router                   │  │
│  │                                                              │  │
│  │  根据场景自动选择最优支付协议:                                  │  │
│  │                                                              │  │
│  │  ┌─────────────┐ ┌─────────────┐ ┌───────────────────────┐  │  │
│  │  │   x402       │ │  Stripe MPP │ │    ERC-8183           │  │  │
│  │  │  (Coinbase)  │ │  (Tempo)    │ │    Escrow             │  │  │
│  │  │              │ │             │ │                       │  │  │
│  │  │ 场景:        │ │ 场景:       │ │ 场景:                 │  │  │
│  │  │ - 单次微支付 │ │ - 高频会话  │ │ - 大额托管交易         │  │  │
│  │  │ - 预览采样   │ │ - 流式支付  │ │ - 需要验证后释放       │  │  │
│  │  │ - 去中心化   │ │ - 法币+加密 │ │ - 争议仲裁             │  │  │
│  │  │              │ │ - 企业合规  │ │                       │  │  │
│  │  │ 结算:        │ │             │ │ 结算:                 │  │  │
│  │  │ USDC on-chain│ │ 结算:       │ │ USDC on L2            │  │  │
│  │  │ (~200ms)     │ │ SPT + USDC │ │ (验证后释放)           │  │  │
│  │  │              │ │ on Tempo    │ │                       │  │  │
│  │  └─────────────┘ │ (<1s)       │ └───────────────────────┘  │  │
│  │                   └─────────────┘                            │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  ┌──────────────────┐  ┌───────────────────────────────────────┐  │
│  │ License Engine    │  │     Receipt Engine                    │  │
│  │                   │  │                                       │  │
│  │ ODRL 机器可读     │  │  链上交易凭证:                          │  │
│  │ 许可证:           │  │  - EAS Attestation                    │  │
│  │ - 使用范围        │  │  - 买卖双方 DID                       │  │
│  │ - 时间限制        │  │  - 数据集 CID                         │  │
│  │ - 派生权限        │  │  - 价格 + 时间戳 + 支付协议类型        │  │
│  │ - 商用/非商用     │  │  - 许可证哈希                          │  │
│  └──────────────────┘  └───────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
```

#### 技术选型

| 组件 | 技术 | 理由 |
|------|------|------|
| 单次微支付 | **x402 (Coinbase)** | HTTP 402 原生集成；无需账户体系；去中心化场景首选；USDC on Base/Polygon/Solana |
| 高频会话支付 | **Stripe MPP (Machine Payments Protocol)** | Session 机制避免逐笔上链；支持 USDC + 法币 (Visa/Mastercard via SPT)；Tempo 链 >10K TPS、亚秒确认；Stripe 合规体系内置 |
| 大额托管交易 | **ERC-8183** | 可编程托管：Client→Provider→Evaluator 三方模型；已上线以太坊主网；支持验证后释放和争议仲裁 |
| 结算网络 | **Base L2 + Tempo Chain** | x402 → Base L2；MPP → Tempo；ERC-8183 → Base L2 或以太坊主网 |
| 许可证标准 | **ODRL (Open Digital Rights Language)** | W3C 标准；机器可读；支持复杂权限表达 |
| 交易凭证 | **EAS (Ethereum Attestation Service)** | 复用认证层基础设施；链上不可篡改 |
| Agent 钱包 | **ERC-4337 Smart Account + Stripe SPT** | ERC-4337 用于链上支付（x402/ERC-8183）；SPT (Shared Payment Token) 用于 MPP 场景，支持法币信用卡代付 |

#### ⭐ 三协议路由策略

```
Payment Router 决策逻辑:

  交易请求进入
      │
      ├─ 金额 < $0.01 且单次请求?
      │   └─ YES → x402 (最低开销，单次 HTTP 往返)
      │
      ├─ 同一卖家的批量/高频请求? (如: 搜索→预览→采样→购买)
      │   └─ YES → Stripe MPP Session
      │          (一次授权，会话内流式结算，无需逐笔签名)
      │
      ├─ 金额 > $1 且需要验证后释放?
      │   └─ YES → ERC-8183 Escrow
      │          (资金锁定→数据交付→Merkle验证→释放)
      │
      ├─ 买方偏好法币支付?
      │   └─ YES → Stripe MPP + SPT
      │          (Agent 使用用户绑定的 Visa/Mastercard)
      │
      └─ 默认 → x402 (最通用，无需注册)
```

#### Stripe MPP 集成细节

```
MPP Session 工作流 (数据集批量交易场景):

Agent (Buyer)                     Stripe MPP / Tempo              Seller
    │                                    │                           │
    │  1. 创建支付会话                     │                           │
    │  POST /mpp/session                 │                           │
    │  { budget: $5, seller_did,         │                           │
    │    payment_method: "spt" | "usdc"} │                           │
    │───────────────────────────────────>│                           │
    │  session_id + auth_token           │                           │
    │<───────────────────────────────────│                           │
    │                                    │                           │
    │  2. 会话内多次微交易 (无需逐笔签名)  │                           │
    │  preview(cid1): $0.001             │  流式结算                  │
    │──────────────────────────────────>│─────────────────────────>│
    │  preview(cid2): $0.001             │                           │
    │──────────────────────────────────>│─────────────────────────>│
    │  sample(cid1, 100rows): $0.01      │                           │
    │──────────────────────────────────>│─────────────────────────>│
    │  purchase(cid1): $0.50             │                           │
    │──────────────────────────────────>│─────────────────────────>│
    │                                    │                           │
    │  3. 会话结束，最终结算               │                           │
    │  Total: $0.522                     │  Stripe 统一结算到卖方     │
    │  (Stripe 处理税务/合规/退款)         │  Stripe 余额              │
    │                                    │                           │

SPT (Shared Payment Token) 机制:
  - Agent 持有用户授权的 SPT (类似一次性虚拟卡)
  - SPT 绑定: 单笔限额 + 总限额 + 有效期 + 商户白名单
  - 支持法币 (Visa/MC/BNPL) 和加密 (USDC on Tempo) 混合支付
  - 用户无需暴露真实卡号，Agent 无需管理私钥
```

#### 交易流程

```
完整购买流程:

Agent (Buyer)                    Protocol                     Provider (Seller)
    │                               │                              │
    │  1. search("GDP data")        │                              │
    │──────────────────────────────>│                              │
    │  results: [{cid, price:$0.5}] │                              │
    │<──────────────────────────────│                              │
    │                               │                              │
    │  2. preview(cid, rows=10)     │                              │
    │──────────────────────────────>│  x402: HTTP 402 → $0.001    │
    │  sample data (10 rows)        │─────────────────────────────>│
    │<──────────────────────────────│  deliver sample              │
    │                               │<─────────────────────────────│
    │                               │                              │
    │  3. purchase(cid)             │                              │
    │──────────────────────────────>│                              │
    │                               │  ERC-8183 Escrow:            │
    │  3a. lock $0.5 USDC           │  createJob(buyer, seller,    │
    │     (Smart Account auto-sign) │    $0.5, cid, timeout=1h)   │
    │──────────────────────────────>│──────────────────────────────>│
    │                               │                              │
    │                               │  4. Seller starts seeding    │
    │                               │<─────────────────────────────│
    │  5. BitTorrent download       │                              │
    │<─────────────────────────────────────────────────────────────│
    │                               │                              │
    │  6. Verify:                   │                              │
    │     Merkle Root match? ✓      │                              │
    │     Row count match? ✓        │                              │
    │     Schema match? ✓           │                              │
    │                               │                              │
    │  7. confirmJob(receipt)       │  Release $0.5 to seller      │
    │──────────────────────────────>│──────────────────────────────>│
    │                               │                              │
    │  8. EAS: attestation          │  交易记录上链                  │
    │     (buyer, seller, cid, $)   │                              │
```

#### 技术挑战与解决方案

| 挑战 | 描述 | 解决方案 |
|------|------|---------|
| **预览与全量数据不一致** | 卖家可能提供高质量预览但全量数据质量差 | **承诺-验证机制**：卖家发布时承诺 Merkle Root + 统计摘要（VC 签名）；买家下载后验证 Merkle Root 一致性；统计摘要不符可发起 dispute，Evaluator 仲裁后自动退款 |
| **Agent 钱包安全** | Agent 自动签名支付存在被恶意利用风险 | **ERC-4337 Session Key**：为每个交易会话生成临时密钥，设置单笔限额（如 $10）和总限额（如 $100/天）；超限需人类确认。**MPP 场景**：SPT 自带限额和有效期，天然安全 |
| **多协议一致性** | x402/MPP/ERC-8183 三种协议的交易凭证格式不同 | **统一 Receipt 层**：无论底层用哪种支付协议，Trading Engine 都生成标准化的 EAS Attestation 作为交易凭证，包含 `paymentProtocol: "x402" | "mpp" | "erc8183"` 字段 |
| **MPP Session 超支风险** | Agent 在 MPP Session 中可能因 bug 或恶意行为超出预算 | **预算守卫**：Payment Router 在 Session 创建时设置硬性 budget cap；每笔交易前检查剩余预算；接近阈值时暂停 Session 并请求人类确认 |
| **法币与加密混合结算** | MPP 支持法币 (SPT) 和加密 (USDC) 混合支付，卖方需统一结算 | **Stripe 统一结算**：MPP 交易通过 Stripe PaymentIntents API 处理，无论买方用 SPT (法币) 还是 USDC (Tempo)，卖方都在 Stripe Dashboard 中看到统一的结算记录，以默认货币入账 |
| **跨链支付碎片化** | 不同卖家可能在不同链上 | x402 → Base L2；MPP → Tempo Chain；ERC-8183 → Base L2。**跨链桥接**：Across Protocol 自动转换；长期目标是 MPP 扩展到更多链 |
| **许可证执行力** | ODRL 许可证是声明性的，无法技术强制执行 | **分层执行**：技术层 — 付费数据 Seller-Only Seeding + 水印追溯（Sharing 层保障）；经济层 — 违规行为通过链上 dispute 扣除 stake；社会层 — 违规记录写入链上声誉，影响未来交易 |
| **定价不透明** | 买家不知道价格是否合理 | 搜索结果中展示 **市场参考价**（同类数据集的历史成交均价）；集成 Valuation 层的自动估值作为参考 |


---

### 2.5 Data Valuation Engine（数据估值引擎层）

> 负责数据集的质量评估、价值量化和动态定价。面向免费数据、付费数据、Agent Memory/Skills 三种场景提供差异化估值策略。

#### 技术架构

```
┌──────────────────────────────────────────────────────────────────┐
│               Valuation API (MCP Tools)                           │
│  evaluate(cid) → quality_report                                   │
│  estimate_value(cid, use_case) → price_range                      │
│  get_market_price(cid) → { avg, min, max, trend }                 │
│  assess_fitness(cid, task_context) → task_fitness_score            │
│  evaluate_memory(memory_cid, task) → relevance_report             │
├──────────────────────────────────────────────────────────────────┤
│                                                                    │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │           Three-Scenario Valuation Router                     │ │
│  │                                                               │ │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────────────┐  │ │
│  │  │ Free Data     │ │ Paid Data    │ │ Agent Memory/Skills  │  │ │
│  │  │ Evaluator     │ │ Evaluator    │ │ Evaluator            │  │ │
│  │  │               │ │              │ │                      │  │ │
│  │  │ 目标:         │ │ 目标:        │ │ 目标:                │  │ │
│  │  │ 帮 Agent 从   │ │ 帮 Agent 判  │ │ 评估 Agent 记忆/技能 │  │ │
│  │  │ 海量免费数据  │ │ 断是否值得   │ │ 是否适合解决当前任务 │  │ │
│  │  │ 中选出最优    │ │ 花钱购买     │ │                      │  │ │
│  │  │               │ │              │ │ 输入:                │  │ │
│  │  │ 核心指标:     │ │ 核心指标:    │ │ - memory/skill CID   │  │ │
│  │  │ - Task Fit    │ │ - ROI 估算   │ │ - 当前任务描述        │  │ │
│  │  │ - 信息增益    │ │ - 替代品分析 │ │ - Agent 能力 profile │  │ │
│  │  │ - 数据新鲜度  │ │ - 质量保证   │ │                      │  │ │
│  │  │ - 去重价值    │ │ - 独特性溢价 │ │ 输出:                │  │ │
│  │  │               │ │              │ │ - 任务匹配度         │  │ │
│  │  └──────────────┘ └──────────────┘ │ - 能力覆盖度         │  │ │
│  │                                     │ - 时效性             │  │ │
│  │                                     └──────────────────────┘  │ │
│  └──────────────────────────────────────────────────────────────┘ │
│                                                                    │
│  ┌──────────────────────┐  ┌───────────────────────────┐          │
│  │  Quality Scorer       │  │   Value Estimator         │          │
│  │  (通用质量评分)        │  │   (经济价值估算)           │          │
│  │                       │  │                           │          │
│  │  静态指标 (本地计算):   │  │  Fast-DataShapley:        │          │
│  │  - Schema 完整性       │  │  - 预训练 explainer 模型   │          │
│  │  - 空值率 / 重复率     │  │  - 输入: 数据集特征        │          │
│  │  - 类型一致性          │  │  - 输出: 边际贡献估计      │          │
│  │  - 时间新鲜度          │  │                           │          │
│  │                       │  │  对比估值:                  │          │
│  │  动态指标 (网络聚合):   │  │  - 同类数据集历史成交价    │          │
│  │  - 下载次数            │  │  - 供需比 (DHT 中同类数量) │          │
│  │  - 正面反馈率          │  │  - 独特性评分              │          │
│  │  - 引用/派生次数       │  │                           │          │
│  └──────────────────────┘  └───────────────────────────┘          │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │              Dynamic Pricing Engine (仅付费数据)               │ │
│  │                                                               │ │
│  │  price(t) = base_value                                        │ │
│  │           × freshness_decay(age)      // 时间衰减              │ │
│  │           × demand_multiplier(downloads) // 需求溢价           │ │
│  │           × scarcity_factor(alternatives) // 稀缺性            │ │
│  │           × reputation_weight(provider)  // 信誉加权           │ │
│  │                                                               │ │
│  │  实现: 链上 Oracle 定期更新参数                                 │ │
│  └──────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

#### 技术选型

| 组件 | 技术 | 理由 |
|------|------|------|
| 质量评分 | **Great Expectations (GX) Core** | 开源数据质量框架；支持 Expectation Suite 定义质量规则；可嵌入运行 |
| 边际价值估计 | **Fast-DataShapley** | 预训练 explainer 模型，推理时 O(1)；无需访问下游模型 |
| 任务适配评估 | **本地 SLM + Schema Matching** | 将任务描述与数据集 schema/描述做语义匹配；计算信息增益 |
| Agent Memory 评估 | **Embedding 相似度 + 时效性衰减** | Memory embedding 与任务 embedding 的余弦相似度；结合记忆创建时间的衰减因子 |
| 动态定价 | **自研定价引擎 + Chainlink Oracle** | 定价公式链上透明可审计；Oracle 提供外部市场数据 |
| 声誉系统 | **EAS Attestation 聚合** | 复用认证层 EAS；聚合历史交易评价计算声誉分 |
| 统计计算 | **Polars (Rust)** | 高性能 DataFrame 库；支持 lazy evaluation；内存效率高 |

#### ⭐ 场景一：免费数据估值 — 帮 Agent 选出最优数据

免费数据的核心问题不是"值不值得买"，而是"海量免费数据中哪个最能帮 Agent 完成当前任务"。

```
Free Data Fitness Score 计算:

  Agent 当前任务: "分析中国各省 GDP 增长趋势并预测 2026"
  候选免费数据集: [Dataset_A, Dataset_B, Dataset_C, ...]

  对每个候选数据集计算 Task Fitness Score:

  ┌─────────────────────────────────────────────────────┐
  │  Dimension           │ Weight │ 计算方法              │
  ├──────────────────────┼────────┼───────────────────────┤
  │  Schema Relevance    │  30%   │ 任务所需列 vs 数据集列 │
  │                      │        │ 的语义匹配度           │
  │  Temporal Coverage   │  20%   │ 数据时间范围覆盖任务   │
  │                      │        │ 所需时间段的比例       │
  │  Information Gain    │  20%   │ 相对于 Agent 已有数据  │
  │                      │        │ 的新增信息量 (KL散度)  │
  │  Data Quality        │  15%   │ 通用质量评分           │
  │  Freshness           │  10%   │ 最后更新时间           │
  │  Dedup Value         │   5%   │ 与 Agent 已有数据的    │
  │                      │        │ 去重后剩余比例         │
  └──────────────────────┴────────┴───────────────────────┘

  关键: Information Gain (信息增益)
    = 该数据集能为 Agent 带来多少"新知识"
    = KL_divergence(P_with_new_data || P_without_new_data)
    → 如果 Agent 已有类似数据，信息增益低，排名下降
    → 如果数据集包含 Agent 完全没有的维度，信息增益高
```

#### ⭐ 场景二：付费数据估值 — ROI 导向

```
Paid Data ROI Assessment:

  purchase_decision = (estimated_value - price) > threshold

  estimated_value 计算:
    1. 替代品分析:
       → 搜索 DHT 中同类免费数据集
       → 如果存在质量相近的免费替代品 → estimated_value 大幅下降
       → 如果无替代品 (独特数据) → scarcity premium

    2. 任务价值估算:
       → Agent 当前任务的预期收益 (如果可量化)
       → 该数据集对任务成功率的边际提升 (Fast-DataShapley)
       → estimated_value = task_reward × marginal_improvement

    3. 质量溢价:
       → 付费数据 vs 最佳免费替代品的质量差
       → quality_premium = (paid_quality - free_best_quality) × weight

  最终建议:
    "该数据集售价 $0.50，预估可提升任务成功率 15%。
     存在 1 个免费替代品 (质量 72 vs 92)。
     建议: 购买 (ROI = 2.3x)"
```

#### ⭐ 场景三：Agent Memory / Skills 估值 — 任务适配评估

Agent Memory（历史经验记忆）和 Skills（可复用的工具/工作流）也是一种可交易的数据资产。评估它们是否适合解决当前任务是全新挑战。

```
Agent Memory/Skills 作为数据资产:

  Memory 类型:
    - Episodic Memory: Agent 过去执行任务的经验记录
    - Semantic Memory: Agent 积累的领域知识图谱
    - Procedural Memory: Agent 学会的操作流程/工作流

  Skill 类型:
    - Tool Chains: 预定义的工具调用序列
    - Prompt Templates: 针对特定任务优化的 prompt
    - Fine-tuned Adapters: 针对特定领域微调的 LoRA 权重

  估值维度:

  ┌──────────────────────────────────────────────────────────┐
  │  Dimension              │ Weight │ 计算方法               │
  ├─────────────────────────┼────────┼────────────────────────┤
  │  Task Relevance         │  35%   │ Memory/Skill 描述与    │
  │                         │        │ 当前任务的语义相似度    │
  │  Historical Success     │  25%   │ 该 Memory/Skill 在     │
  │                         │        │ 类似任务上的成功率      │
  │  Capability Coverage    │  20%   │ 覆盖当前任务所需能力    │
  │                         │        │ 的比例 (能力图谱匹配)  │
  │  Temporal Relevance     │  10%   │ 记忆/技能的时效性      │
  │                         │        │ (API 变更/知识过时)    │
  │  Transferability        │  10%   │ 跨任务/跨领域的        │
  │                         │        │ 可迁移性评分           │
  └─────────────────────────┴────────┴────────────────────────┘

  评估流程:
    1. 解析当前任务 → 提取所需能力集合 {C1, C2, C3, ...}
    2. 解析 Memory/Skill 元数据 → 提取提供能力集合 {S1, S2, ...}
    3. Capability Coverage = |{C} ∩ {S}| / |{C}|
    4. 语义相似度 = cosine(embed(task), embed(memory_description))
    5. 历史成功率 = 从链上 EAS 记录中聚合该 Memory/Skill 的使用反馈
    6. 时效性 = decay(now - last_verified_at, half_life=30days)

  输出示例:
    {
      "memory_cid": "bafybeig...",
      "task_fitness": 0.82,
      "capability_coverage": 0.75,  // 覆盖 3/4 所需能力
      "historical_success_rate": 0.88,
      "temporal_relevance": 0.65,   // 创建于 45 天前，部分过时
      "recommendation": "适合使用，但需注意 API v2→v3 的变更",
      "missing_capabilities": ["real-time-data-access"]
    }
```

#### 质量评分维度

```
Quality Score (0-100) 计算:

┌─────────────────────────────────────────────┐
│  Dimension          │ Weight │ Metrics       │
├─────────────────────┼────────┼───────────────┤
│  Completeness       │  25%   │ 非空率, 列覆盖 │
│  Consistency        │  20%   │ 类型一致, 范围 │
│  Freshness          │  20%   │ 更新时间, 时效 │
│  Schema Quality     │  15%   │ 文档, 类型定义 │
│  Provenance         │  10%   │ 来源可追溯性   │
│  Community Signal   │  10%   │ 下载, 评价     │
└─────────────────────┴────────┴───────────────┘

示例:
  completeness = (1 - null_rate) × 100 = 98
  consistency  = type_match_rate × 100 = 95
  freshness    = max(0, 100 - days_since_update × 0.5) = 85
  schema       = (has_description + has_types + has_examples) / 3 × 100 = 80
  provenance   = has_vc × 50 + has_eas × 30 + has_zkp × 20 = 80
  community    = min(100, downloads / 10 + positive_rate × 50) = 60

  quality_score = 98×0.25 + 95×0.20 + 85×0.20 + 80×0.15 + 80×0.10 + 60×0.10
               = 24.5 + 19.0 + 17.0 + 12.0 + 8.0 + 6.0 = 86.5
```

#### 技术挑战与解决方案

| 挑战 | 描述 | 解决方案 |
|------|------|---------|
| **免费数据的信息增益计算** | 计算 KL 散度需要知道 Agent 已有数据的分布，但 Agent 可能不愿暴露 | **本地计算**：信息增益在 Agent 本地节点计算，只需将候选数据集的统计摘要（从 VC 中获取）与本地数据对比；无需上传 Agent 数据到网络 |
| **免费数据海量候选排序效率** | 免费数据集数量远多于付费数据集，逐一评估 Task Fitness 太慢 | **两阶段过滤**：Stage 1 — 基于元数据的粗筛（embedding 相似度 top-50，毫秒级）；Stage 2 — 对 top-50 计算完整 Task Fitness Score（含采样验证） |
| **购买前无法评估全量付费数据** | Agent 需要在付费前判断数据集价值，但看不到全量数据 | **三级评估**：Level 1 — 元数据评估（免费，基于 VC 中的 schema/stats）；Level 2 — 采样评估（x402/MPP 微支付 $0.001，获取随机 N 行）；Level 3 — ZKP 属性验证（验证卖家的统计声明是否真实） |
| **Agent Memory 时效性判断** | Agent 记忆可能引用已过时的 API、已变更的数据格式 | **版本感知评估**：Memory 元数据中记录依赖的外部 API 版本和数据 schema 版本；评估时检查这些依赖是否仍然有效（通过 DHT 查询最新版本）；过时依赖自动降低 temporal_relevance 分数 |
| **Skill 可迁移性评估** | 一个 Skill 在领域 A 表现好，不代表在领域 B 也好 | **能力图谱匹配**：定义标准化的 Agent 能力分类体系（类似 O*NET 职业能力分类）；每个 Skill 标注其覆盖的能力节点；任务也映射到能力节点；通过图谱重叠度计算可迁移性 |
| **Memory/Skill 质量的冷启动** | 新发布的 Memory/Skill 没有历史使用记录 | **发布者自测报告 + 挑战机制**：发布者提交 benchmark 测试结果（签名的 VC）；其他 Agent 可付费挑战（在自己的任务上测试），结果写入链上；初始信任基于发布者声誉 |
| **Shapley 值计算不可行** | 经典 Data Shapley 对大数据集计算量 O(2^N) | 采用 **Fast-DataShapley**：预训练一个 explainer 模型（一次性成本），之后对任意数据集推理 O(1)；对于协议级部署，训练一个通用 explainer 覆盖常见数据类型 |
| **定价操纵** | 卖家可能通过虚假下载/评价操纵价格 | **Sybil 抵抗**：下载/评价需要消耗 gas（即使极少）提高攻击成本；评价权重与评价者的链上交易历史挂钩；异常检测：短时间大量同源下载自动标记 |
| **跨领域估值不可比** | 医疗数据集和天气数据集的"价值"无法用同一标准衡量 | **领域分类 + 相对估值**：按 topic 分类（经济/医疗/地理/...），每个类别内部相对排序；跨类别不直接比较，而是展示"该类别内 Top X%" |


---

## 三、用户视角：从安装到发现的端到端流程

本章从用户视角描述两个核心场景的完整体验：数据提供者如何"一键"发布数据集，以及 Agent 如何搜索到这些数据集。

### 3.1 数据提供者：安装 Node → 数据自动注册

```
┌─────────────────────────────────────────────────────────────────┐
│  Step 1: 安装并启动 Node                                         │
│                                                                  │
│  $ curl -sSf https://install.dataprotocol.dev | sh              │
│  $ data-node init                                                │
│                                                                  │
│  init 自动完成:                                                   │
│  ├─ 生成节点身份: Ed25519 密钥对 → did:key:z6Mk...              │
│  ├─ 创建配置文件: ~/.data-node/config.toml                       │
│  │   ├─ data_dir = "~/shared-datasets"   # 数据集目录            │
│  │   ├─ access_default = "open"          # 默认免费开放          │
│  │   └─ price_default = 0                # 默认免费              │
│  └─ 生成 MCP Server 配置 (供本地 Agent 连接)                      │
│                                                                  │
│  $ data-node start                                               │
│                                                                  │
│  start 自动完成:                                                  │
│  ├─ 连接 Bootstrap 节点 (硬编码列表 + DNS 发现)                   │
│  ├─ 加入 Kademlia DHT 网络                                       │
│  ├─ 订阅 GossipSub "datasets" topic                             │
│  ├─ 启动 mDNS 局域网发现                                         │
│  └─ 启动文件监控 (watchdir on data_dir)                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 2: 放入数据集 → 自动注册                                    │
│                                                                  │
│  用户只需将文件放入 ~/shared-datasets/ 目录:                       │
│                                                                  │
│  $ cp china_gdp_2020_2025.csv ~/shared-datasets/                │
│                                                                  │
│  Node 文件监控检测到新文件，自动执行:                               │
│  ├─ 1. 格式检测 → 转换为 Parquet (如果是 CSV/JSON/Excel)         │
│  ├─ 2. Schema 推断 → 列名、类型、统计摘要                        │
│  ├─ 3. 分片 → 256KB pieces → Merkle Tree → Info Hash            │
│  ├─ 4. 生成元数据 JSON-LD (自动填充 schema/stats/CID)            │
│  ├─ 5. DID 签名 → 生成 Verifiable Credential                    │
│  ├─ 6. DHT PUT → 元数据写入分布式哈希表                           │
│  ├─ 7. GossipSub 广播 → 通知所有在线节点                         │
│  └─ 8. 开始 seed → 等待其他节点下载                               │
│                                                                  │
│  终端输出:                                                        │
│  [INFO] 检测到新文件: china_gdp_2020_2025.csv                    │
│  [INFO] 转换为 Parquet: 50,000 行, 8 列, 12MB                   │
│  [INFO] CID: bafybeig6...  InfoHash: v2:abc123...               │
│  [INFO] ✅ 已注册到 DHT，全网可发现                                │
│  [INFO] ✅ 已广播到 47 个在线节点                                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 3 (可选): 自定义配置                                        │
│                                                                  │
│  用户可通过 CLI 或编辑 config.toml 调整:                           │
│                                                                  │
│  # 设置某个数据集为付费                                            │
│  $ data-node set-price bafybeig6... --price 0.50 --currency USDC│
│                                                                  │
│  # 添加描述和标签 (提升搜索排名)                                    │
│  $ data-node describe bafybeig6... \                             │
│      --title "中国各省 GDP 2020-2025" \                           │
│      --tags "gdp,china,economics,time-series"                    │
│                                                                  │
│  # 设置许可证                                                     │
│  $ data-node set-license bafybeig6... --license CC-BY-4.0       │
│                                                                  │
│  修改后自动重新签名并更新 DHT 记录                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 数据消费者 (Agent)：安装 MCP → 搜索到数据

```
┌─────────────────────────────────────────────────────────────────┐
│  Step 1: Agent 安装 MCP Server                                   │
│                                                                  │
│  在 Agent 的 MCP 配置中添加:                                      │
│  {                                                               │
│    "mcpServers": {                                               │
│      "dataset-protocol": {                                       │
│        "command": "data-node",                                   │
│        "args": ["mcp", "--mode", "light"]                        │
│      }                                                           │
│    }                                                             │
│  }                                                               │
│                                                                  │
│  --mode light 表示 Light Node:                                   │
│  ├─ 加入 DHT 网络 (可搜索)                                       │
│  ├─ 可下载数据 (BitTorrent client)                               │
│  ├─ ❌ 不 seed 数据 (不占用户带宽)                                │
│  └─ ❌ 不存储完整索引 (按需查询 DHT)                              │
│                                                                  │
│  MCP Server 启动时自动:                                           │
│  ├─ 连接 Bootstrap 节点 → 加入 DHT                               │
│  ├─ 订阅 GossipSub → 接收新数据集通知                             │
│  └─ 初始化本地 Qdrant → 增量构建向量索引                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Step 2: Agent 搜索数据集                                        │
│                                                                  │
│  Agent: "帮我找中国各省 GDP 数据"                                  │
│                                                                  │
│  → MCP tool call: dataset_search({                               │
│      query: "中国各省 GDP 数据"                                    │
│    })                                                            │
│                                                                  │
│  MCP Server (Light Node) 内部执行:                                │
│  ├─ 意图解析 → {topic:"GDP", geo:"China/provinces"}              │
│  ├─ DHT lookup → 找到 3 个 P2P 网络内数据集                      │
│  ├─ 本地 Qdrant 向量搜索 → 语义匹配 top-K                        │
│  ├─ Kaggle/HF adapter → 找到 2 个外部平台数据集                   │
│  ├─ 合并去重 → 5 个候选                                          │
│  └─ 排序 → 返回结果                                              │
│                                                                  │
│  返回给 Agent:                                                    │
│  [                                                               │
│    { title: "中国各省 GDP 2020-2025",                             │
│      cid: "bafybeig6...",                                        │
│      source: "p2p",        ← 刚才那个用户发布的！                  │
│      quality: 86, price: "free", rows: 50000 },                  │
│    { title: "China Economic Indicators",                         │
│      source: "kaggle", quality: 78, price: "free" },             │
│    ...                                                           │
│  ]                                                               │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 网络 Bootstrap 机制

新节点加入网络的发现流程：

```
Node 启动
    │
    ├─ 1. 硬编码 Bootstrap 列表 (协议内置)
    │     → 连接 3-5 个长期稳定运行的 bootstrap 节点
    │     → 获取初始 DHT 路由表
    │
    ├─ 2. DNS Bootstrap (动态更新)
    │     → 查询 _dnsaddr.bootstrap.dataprotocol.dev
    │     → 返回当前活跃的 bootstrap 节点 multiaddr 列表
    │     → 优势: 节点列表可动态更新，无需升级软件
    │
    ├─ 3. mDNS 局域网发现 (零配置)
    │     → 自动发现同一局域网内的其他节点
    │     → 适合企业内网 / 家庭网络场景
    │
    └─ 4. 已知 Peer 缓存 (重启加速)
          → 上次运行时连接过的 peer 列表持久化到磁盘
          → 重启时优先连接这些已知 peer
          → 避免每次都依赖 bootstrap 节点

连接成功后:
    ├─ Kademlia DHT: 通过迭代查找填充路由表 (约 30s 收敛)
    ├─ GossipSub: 加入 "datasets" topic mesh (约 5s)
    └─ 节点就绪，可发布/搜索数据集
```

### 3.4 数据注册到全网可发现的时间线

```
T+0s    用户将文件放入 ~/shared-datasets/
T+1s    文件监控检测到变更，开始处理
T+3s    Parquet 转换 + Merkle Tree 生成完成
T+3s    DHT PUT 发出
T+4s    GossipSub 广播发出
        ┌──────────────────────────────────────────────┐
T+5s    │ GossipSub 路径 (快):                          │
        │ 所有在线且订阅 "datasets" topic 的节点收到通知 │
        │ → 立即更新本地 Qdrant 向量索引                 │
        │ → 该数据集可被这些节点上的 Agent 搜索到        │
        └──────────────────────────────────────────────┘
        ┌──────────────────────────────────────────────┐
T+30s   │ DHT 路径 (慢但可靠):                          │
~T+120s │ DHT PUT 传播到 K 个最近节点 (Kademlia K=20)  │
        │ → 任何节点通过 DHT GET 都能查到该元数据        │
        │ → 作为最终一致性保证                           │
        └──────────────────────────────────────────────┘

结论:
  - 最快 ~5 秒: 在线节点通过 GossipSub 实时收到
  - 最慢 ~2 分钟: 通过 DHT 全网可查
  - 离线节点: 上线后通过 DHT 查询补全，或通过 GossipSub 历史消息回放
```

### 3.5 Full Node vs Light Node 对比

| 能力 | Full Node (数据提供者) | Light Node (Agent/消费者) |
|------|----------------------|--------------------------|
| 安装方式 | `data-node start` | `data-node mcp --mode light` |
| 加入 DHT | ✅ | ✅ |
| GossipSub 订阅 | ✅ | ✅ |
| 发布数据集 | ✅ 自动扫描目录 | ❌ |
| Seed 数据 | ✅ | ❌ |
| 搜索数据集 | ✅ | ✅ |
| 下载数据集 | ✅ | ✅ |
| 本地向量索引 | ✅ 完整索引 | ✅ 增量索引 |
| MCP Server | ✅ 完整 6 tools | ✅ 搜索+下载+评估 |
| 磁盘占用 | ~GB 级 (数据+索引) | ~MB 级 (仅索引) |
| 带宽消耗 | 中-高 (seed 数据) | 低 (仅查询+下载) |

---

## 四、组件间交互：端到端工作流（技术视角）

```
Agent: "我需要中国各省 2020-2025 年 GDP 数据，预算 $5"

═══════════════════════════════════════════════════════════════
Step 1: SEARCH
═══════════════════════════════════════════════════════════════
  Search Engine:
    → Intent parse: {topic:"GDP", geo:"China", temporal:"2020-2025"}
    → DHT lookup + Vector search + Kaggle/HF adapters
    → 返回 12 个候选数据集

═══════════════════════════════════════════════════════════════
Step 2: AUTHENTICATE + VALUATE (并行)
═══════════════════════════════════════════════════════════════
  Auth Engine (per dataset):
    → 验证 VC 签名 ✓/✗
    → 检查 EAS 链上锚定 ✓/✗
    → 信任等级: L1/L2/L3

  Valuation Engine (per dataset):
    → 质量评分: 86.5/100
    → 市场参考价: $0.30 - $0.80
    → 独特性: 高 (仅 2 个同类数据集)

═══════════════════════════════════════════════════════════════
Step 3: RANK & SELECT
═══════════════════════════════════════════════════════════════
  综合排序:
    → Dataset A: quality=92, trust=L3, price=$0.50 ★ Best
    → Dataset B: quality=85, trust=L2, price=$0.30
    → Dataset C: quality=78, trust=L1, price=Free

  Agent 选择 Dataset A

═══════════════════════════════════════════════════════════════
Step 4: PREVIEW (Trading Engine)
═══════════════════════════════════════════════════════════════
  → x402 微支付 $0.001
  → 获取 10 行样本数据
  → Agent 确认 schema 和数据质量符合预期

═══════════════════════════════════════════════════════════════
Step 5: PURCHASE (Trading Engine)
═══════════════════════════════════════════════════════════════
  → ERC-8183 Escrow: 锁定 $0.50 USDC
  → BitTorrent 下载全量数据 (Sharing Layer)
  → Merkle Root 验证通过 (Auth Engine)
  → 自动释放资金给卖家
  → EAS 记录交易凭证

═══════════════════════════════════════════════════════════════
Step 6: FEEDBACK
═══════════════════════════════════════════════════════════════
  → Agent 使用数据后上报: relevance=0.95, useful=true
  → 更新卖家声誉 + 数据集质量分
```

---

## 五、MCP 接口定义

所有组件通过统一的 MCP Server 暴露给 Agent：

```json
{
  "name": "dataset-protocol",
  "version": "0.1.0",
  "tools": [
    {
      "name": "dataset_search",
      "description": "搜索数据集，支持自然语言和结构化过滤",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "自然语言搜索词" },
          "filters": {
            "type": "object",
            "properties": {
              "topic": { "type": "string" },
              "min_rows": { "type": "integer" },
              "max_price": { "type": "number" },
              "license": { "type": "string" },
              "min_quality": { "type": "number" }
            }
          },
          "limit": { "type": "integer", "default": 10 }
        },
        "required": ["query"]
      }
    },
    {
      "name": "dataset_preview",
      "description": "预览数据集的样本行（微支付）",
      "inputSchema": {
        "type": "object",
        "properties": {
          "cid": { "type": "string" },
          "rows": { "type": "integer", "default": 10 }
        },
        "required": ["cid"]
      }
    },
    {
      "name": "dataset_purchase",
      "description": "购买并下载数据集（托管交易）",
      "inputSchema": {
        "type": "object",
        "properties": {
          "cid": { "type": "string" },
          "max_price": { "type": "number" }
        },
        "required": ["cid"]
      }
    },
    {
      "name": "dataset_verify",
      "description": "验证数据集的完整性和来源",
      "inputSchema": {
        "type": "object",
        "properties": {
          "cid": { "type": "string" },
          "check_chain": { "type": "boolean", "default": false }
        },
        "required": ["cid"]
      }
    },
    {
      "name": "dataset_evaluate",
      "description": "评估数据集质量和估值",
      "inputSchema": {
        "type": "object",
        "properties": {
          "cid": { "type": "string" },
          "use_case": { "type": "string" },
          "agent_context": {
            "type": "object",
            "description": "Agent 当前任务上下文，用于计算 Task Fitness",
            "properties": {
              "task_description": { "type": "string" },
              "existing_data_cids": { "type": "array", "items": { "type": "string" } },
              "budget": { "type": "number" }
            }
          }
        },
        "required": ["cid"]
      }
    },
    {
      "name": "memory_evaluate",
      "description": "评估 Agent Memory/Skill 是否适合当前任务",
      "inputSchema": {
        "type": "object",
        "properties": {
          "memory_cid": { "type": "string" },
          "task_description": { "type": "string" },
          "agent_capabilities": {
            "type": "array",
            "items": { "type": "string" },
            "description": "Agent 当前已有的能力列表"
          }
        },
        "required": ["memory_cid", "task_description"]
      }
    },
    {
      "name": "dataset_publish",
      "description": "发布数据集到 P2P 网络",
      "inputSchema": {
        "type": "object",
        "properties": {
          "file_path": { "type": "string" },
          "metadata": {
            "type": "object",
            "properties": {
              "title": { "type": "string" },
              "description": { "type": "string" },
              "license": { "type": "string" },
              "price": { "type": "number" },
              "tags": { "type": "array", "items": { "type": "string" } }
            }
          }
        },
        "required": ["file_path", "metadata"]
      }
    }
  ]
}
```

---

## 六、技术栈总览

| 层 | 核心技术 | 语言 | 关键依赖 |
|----|---------|------|---------|
| P2P Sharing | libp2p + BitTorrent v2 (双模式: Open Swarm / Seller-Only) + HashMark 水印 | Rust | rust-libp2p, cratetorrent |
| Data Search | Qdrant embedded + SLM | Rust + Python | qdrant-client, onnxruntime |
| Data Auth | DID + VC + Noir ZKP | Rust | did-key, noir-lang, eas-sdk |
| Data Trading | x402 + Stripe MPP + ERC-8183 (三协议路由) | Rust + Solidity | ethers-rs, x402-rs, stripe-sdk |
| Data Valuation | Polars + Fast-DataShapley + Task Fitness + Memory Evaluator | Rust + Python | polars, great-expectations |
| Agent Interface | MCP Server | Rust | mcp-rs-template |

**主语言选择 Rust 的理由：**
- libp2p 和 BitTorrent 的最佳实现均为 Rust
- 内存安全，适合长期运行的 P2P 节点
- WASM 编译目标，支持浏览器内运行轻节点
- Noir ZKP 工具链原生 Rust

---

## 七、部署架构

```
Full Node (数据提供者):
  ┌─────────────────────────────┐
  │  MCP Server (stdio/SSE)     │ ← Agent 连接入口
  │  ┌───────────────────────┐  │
  │  │  Protocol Core (Rust)  │  │
  │  │  - Search Engine       │  │
  │  │  - Auth Engine         │  │
  │  │  - Trading Engine      │  │
  │  │  - Valuation Engine    │  │
  │  ├───────────────────────┤  │
  │  │  P2P Layer             │  │
  │  │  - libp2p daemon       │  │
  │  │  - BitTorrent engine   │  │
  │  ├───────────────────────┤  │
  │  │  Storage               │  │
  │  │  - RocksDB (metadata)  │  │
  │  │  - Qdrant (vectors)    │  │
  │  │  - Filesystem (data)   │  │
  │  └───────────────────────┘  │
  └─────────────────────────────┘

Light Node (数据消费者):
  ┌─────────────────────────────┐
  │  MCP Server (stdio)         │ ← Agent 连接入口
  │  ┌───────────────────────┐  │
  │  │  Protocol Core (Rust)  │  │
  │  │  - Search (DHT only)   │  │
  │  │  - Auth (verify only)  │  │
  │  │  - Trading (buy only)  │  │
  │  ├───────────────────────┤  │
  │  │  P2P Layer (light)     │  │
  │  │  - libp2p (DHT client) │  │
  │  │  - BT (download only)  │  │
  │  └───────────────────────┘  │
  └─────────────────────────────┘
```

---

## 八、与 VLDB 2026 Demo 的对应关系

| Demo 展示点 | 对应架构组件 | 演示场景 |
|------------|------------|---------|
| P2P Dataset Sharing | Sharing Layer (libp2p + BT v2) | 两个节点间发布和下载数据集 |
| Data Authentication | Auth Engine (DID + VC + Merkle) | 验证数据集完整性和来源签名 |
| Data Trading | Trading Engine (x402 + ERC-8183) | Agent 自动预览→购买→验证→支付 |
| Data Valuation | Valuation Engine (Quality Score + Pricing) | 展示质量评分和动态定价 |
| Novel Search Protocol | Search Engine (DHT + Vector) | Agent 自然语言搜索跨源数据集 |

---

*文档版本: v0.1 | 最后更新: 2026-03-23*
