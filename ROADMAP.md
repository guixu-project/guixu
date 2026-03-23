# DIP001 Implementation Roadmap

> 基于架构设计文档，分 4 个 Milestone 渐进式实现。每个 Milestone 产出可独立演示的功能。

---

## Milestone 1: P2P 骨架 + 本地发布/发现（4 周）

> 目标：两个节点能互相发现对方发布的免费数据集并下载。VLDB Demo 最小可演示版本。

### 功能

| # | 任务 | Crate | 状态 |
|---|------|-------|------|
| 1.1 | `data-node init` — 生成 Ed25519 身份 + DID + 配置文件 | `core`, `node` | 🔲 |
| 1.2 | libp2p 网络启动 — Kademlia DHT + GossipSub + mDNS + Noise | `p2p/network` | 🔲 |
| 1.3 | Bootstrap 机制 — 硬编码 peers + DNS 发现 + peer 缓存 | `p2p/network` | 🔲 |
| 1.4 | WatchDir 文件监控 — 检测 data_dir 中新增文件 | `p2p/watchdir` | 🔲 |
| 1.5 | 自动发布流程 — CSV→Parquet + Merkle Tree + 元数据生成 + DID 签名 | `p2p/torrent`, `core` | 🔲 |
| 1.6 | DHT PUT/GET — 元数据写入和查询 | `p2p/dht` | 🔲 |
| 1.7 | GossipSub 广播 — 新数据集实时通知 | `p2p/gossip` | 🔲 |
| 1.8 | BitTorrent v2 seeding + downloading (Open 模式) | `p2p/torrent` | 🔲 |
| 1.9 | RocksDB 本地元数据存储 | `p2p/storage` | 🔲 |
| 1.10 | MCP Server stdio — initialize + tools/list + dataset_publish + dataset_search (DHT only) | `mcp-server` | 🔲 |
| 1.11 | VC 签发 — DatasetCredential 生成 | `auth/credential` | 🔲 |
| 1.12 | 基础验证 — 签名校验 + Merkle 完整性 | `auth/verifier` | 🔲 |

### 演示场景
```
Node A: data-node start → 放入 CSV → 自动发布到 DHT
Node B: data-node mcp --mode light → Agent 调用 dataset_search → 找到 Node A 的数据集 → 下载
```

### 交付物
- `data-node init / start / mcp` 三个命令可用
- 两节点 mDNS 局域网互发现
- 免费数据集端到端：发布 → 搜索 → 下载 → 验证

---

## Milestone 2: 搜索引擎 + 质量评估（3 周）

> 目标：Agent 能用自然语言搜索数据集，获得质量评分和 Task Fitness 推荐。

### 功能

| # | 任务 | Crate | 状态 |
|---|------|-------|------|
| 2.1 | Qdrant embedded 本地向量索引 | `search/vector_index` | 🔲 |
| 2.2 | Embedding 模型集成 (all-MiniLM-L6-v2 ONNX) | `search/vector_index` | 🔲 |
| 2.3 | GossipSub → 本地 Qdrant 实时同步 | `search/vector_index` | 🔲 |
| 2.4 | Intent Parser — 关键词提取 + 规则引擎 | `search/intent` | 🔲 |
| 2.5 | 多源合并搜索 — DHT + Vector + 去重 + 排序 | `search/engine` | 🔲 |
| 2.6 | QualityScorer — 6 维度质量评分 | `valuation/scorer` | 🔲 |
| 2.7 | FreeDataEvaluator — Task Fitness Score | `valuation/free_evaluator` | 🔲 |
| 2.8 | PaidDataEvaluator — ROI 评估 | `valuation/paid_evaluator` | 🔲 |
| 2.9 | dataset_evaluate MCP tool 完整接线 | `mcp-server` | 🔲 |
| 2.10 | dataset_preview — BT range request 下载前 N 行 | `p2p/torrent` | 🔲 |
| 2.11 | `data-node set-price / describe` CLI 命令 | `node` | 🔲 |

### 演示场景
```
Agent: dataset_search("中国 GDP 时间序列") → 返回排序结果 + 质量评分
Agent: dataset_evaluate(cid, task="预测 2026 GDP") → Task Fitness 报告
Agent: dataset_preview(cid, rows=10) → 预览前 10 行
```

---

## Milestone 3: 付费交易 + 外部平台（4 周）

> 目标：Agent 能自动付费购买数据集，支持 x402 和 Stripe MPP 两种支付协议。

### 功能

| # | 任务 | Crate | 状态 |
|---|------|-------|------|
| 3.1 | Seller-Only Seeding 模式 — 付费数据集 BT 限制 | `p2p/torrent` | 🔲 |
| 3.2 | x402 Client — HTTP 402 + USDC on Base 支付 | `trading/x402` | 🔲 |
| 3.3 | Stripe MPP Client — Session 创建 + 流式支付 | `trading/mpp` | 🔲 |
| 3.4 | PaymentRouter — 三协议自动选择 | `trading/router` | 🔲 |
| 3.5 | ERC-4337 Smart Account 集成 (Agent 钱包) | `trading` | 🔲 |
| 3.6 | dataset_purchase MCP tool 完整接线 | `mcp-server` | 🔲 |
| 3.7 | EAS 交易凭证上链 | `auth` | 🔲 |
| 3.8 | Kaggle API adapter | `search/adapters` | 🔲 |
| 3.9 | HuggingFace API adapter | `search/adapters` | 🔲 |
| 3.10 | Dynamic Pricing Engine | `valuation/pricing` | 🔲 |
| 3.11 | 声誉系统 — EAS attestation 聚合 | `valuation` | 🔲 |
| 3.12 | Intent Parser 升级 — 本地 SLM (Phi-3-mini ONNX) | `search/intent` | 🔲 |

### 演示场景
```
Agent: dataset_search("real-time weather") → 找到付费数据集 $0.50
Agent: dataset_preview(cid) → x402 微支付 $0.001 → 预览 10 行
Agent: dataset_purchase(cid) → MPP session → 下载 → Merkle 验证 → 自动付款
```

---

## Milestone 4: 版权保护 + Agent Memory + 高级功能（3 周）

> 目标：完整的版权保护体系，Agent Memory/Skills 交易，ZKP 属性证明。

### 功能

| # | 任务 | Crate | 状态 |
|---|------|-------|------|
| 4.1 | HashMark 水印嵌入 — 数值 LSB + 哨兵行 + 行顺序指纹 | `auth/watermark` | 🔲 |
| 4.2 | 水印提取 + 泄露追溯 | `auth/watermark` | 🔲 |
| 4.3 | ERC-8183 Escrow Client — 锁定→交付→验证→释放 | `trading/escrow` | 🔲 |
| 4.4 | ODRL 许可证引擎 — 机器可读许可证生成/解析 | `trading` | 🔲 |
| 4.5 | MemoryEvaluator — Agent Memory/Skill 任务适配评估 | `valuation/memory_evaluator` | 🔲 |
| 4.6 | memory_evaluate MCP tool 接线 | `mcp-server` | 🔲 |
| 4.7 | ZKP 属性证明 (Noir 电路) — 行数/空值率/统计特征 | `auth` | 🔲 |
| 4.8 | Provenance Chain — 派生数据集 DAG 溯源 | `auth/credential` | 🔲 |
| 4.9 | SPT (Shared Payment Token) 法币支付集成 | `trading/mpp` | 🔲 |
| 4.10 | 跨链桥接 (Across Protocol) | `trading` | 🔲 |

### 演示场景
```
Seller: 发布付费数据集 → 买方购买 → 水印版数据交付
Agent: memory_evaluate(skill_cid, task="数据清洗") → 适配度报告
Buyer: 尝试泄露数据 → 水印提取 → 追溯到买方 DID
```

---

## 代码结构总览

```
data-protocols/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── core/                     # 共享类型、身份、配置、错误
│   │   └── src/
│   │       ├── types.rs          # DatasetCid, Did, SearchResult, Price, ...
│   │       ├── identity.rs       # Ed25519 keypair, DID, sign/verify
│   │       ├── metadata.rs       # DatasetMetadata, Provenance
│   │       ├── config.rs         # NodeConfig, NodeMode
│   │       └── error.rs          # DataProtocolError
│   │
│   ├── p2p/                      # P2P 网络 + 数据分发
│   │   └── src/
│   │       ├── network.rs        # libp2p swarm, DataProtocolBehaviour
│   │       ├── dht.rs            # Kademlia DHT 索引操作
│   │       ├── gossip.rs         # GossipSub 广播
│   │       ├── torrent.rs        # BitTorrent v2 引擎 (Open/Paid 双模式)
│   │       ├── storage.rs        # RocksDB 本地存储
│   │       └── watchdir.rs       # 文件目录监控
│   │
│   ├── search/                   # 搜索引擎
│   │   └── src/
│   │       ├── engine.rs         # 统一搜索入口, 多源合并排序
│   │       ├── vector_index.rs   # Qdrant embedded 向量索引
│   │       ├── intent.rs         # 自然语言意图解析
│   │       └── adapters.rs       # Kaggle/HuggingFace 外部适配器
│   │
│   ├── auth/                     # 认证 + 版权
│   │   └── src/
│   │       ├── verifier.rs       # 签名验证, Merkle 校验, 信任等级
│   │       ├── credential.rs     # W3C Verifiable Credential 签发
│   │       └── watermark.rs      # HashMark 水印嵌入/提取
│   │
│   ├── trading/                  # 交易引擎
│   │   └── src/
│   │       ├── router.rs         # 三协议支付路由
│   │       ├── x402.rs           # x402 微支付客户端
│   │       ├── mpp.rs            # Stripe MPP 会话支付客户端
│   │       └── escrow.rs         # ERC-8183 托管交易客户端
│   │
│   ├── valuation/                # 估值引擎
│   │   └── src/
│   │       ├── scorer.rs         # 通用质量评分 (6 维度)
│   │       ├── free_evaluator.rs # 免费数据 Task Fitness 评估
│   │       ├── paid_evaluator.rs # 付费数据 ROI 评估
│   │       ├── memory_evaluator.rs # Agent Memory/Skill 适配评估
│   │       └── pricing.rs        # 动态定价引擎
│   │
│   ├── mcp-server/               # MCP 协议层
│   │   └── src/
│   │       ├── server.rs         # stdio JSON-RPC 服务器
│   │       ├── tools.rs          # 7 个 MCP tool 定义
│   │       └── protocol.rs       # JSON-RPC 请求/响应类型
│   │
│   └── node/                     # CLI 入口
│       └── src/
│           └── main.rs           # data-node init/start/mcp/set-price/describe
```

---

## 里程碑时间线

```
Week 1-4:   Milestone 1 — P2P 骨架 + 本地发布/发现
            ✅ VLDB Demo 最小可演示版本
            
Week 5-7:   Milestone 2 — 搜索引擎 + 质量评估
            ✅ Agent 语义搜索 + 质量评分

Week 8-11:  Milestone 3 — 付费交易 + 外部平台
            ✅ 完整交易流程 + 多支付协议

Week 12-14: Milestone 4 — 版权保护 + Agent Memory
            ✅ 水印 + Memory 评估 + ZKP
```
