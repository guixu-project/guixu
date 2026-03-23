# Dataset Search Engines for AI Agents 调研

> 调研时间：2026-03-23
>
> 核心问题：当前是否存在面向 AI Agent 的数据集搜索引擎？Agent 能否自主发现、评估和获取所需数据集？

VLDB 2026 Demo Proposal关于“面向ai agents的novel p2p dataset search protocol (including data sharing, data authentication, data trading, data valuation)”

---

## 一、直接回答

**有，但处于非常早期的阶段。** 目前存在三个层次的方案：

1. **已有数据集平台的 MCP Server 封装**（Kaggle MCP、HuggingFace MCP、Google Data Commons MCP）— Agent 可以搜索和下载公开数据集
2. **Agent 原生的数据集工作流工具**（Vesper）— Agent 可以跨平台搜索、下载、清洗、导出数据集
3. **MCP Server 发现层**（MCPfinder、.well-known/mcp.json、MCP Registry API）— Agent 可以动态发现新的数据源

但**不存在**一个统一的、面向 Agent 的"数据集搜索引擎"，能像 Google Dataset Search 对人类做的那样，让 Agent 跨所有数据源发现和获取数据集。

---

## 二、现有方案全景

### 2.1 已有数据集平台的 MCP 封装

这些是把现有数据集平台包装成 MCP Server，让 Agent 可以通过标准协议搜索和访问。

#### Kaggle MCP Server

- 多个实现版本（Python/Node.js），已上架 MCP 市场
- 能力：搜索数据集、下载数据集、获取元数据（Croissant/JSON-LD 格式）、列出竞赛、生成 EDA notebook
- 可与 Claude、Cursor、CrewAI、AutoGen 等集成
- 限制：仅限 Kaggle 平台数据，需要 Kaggle API Token

#### Hugging Face MCP Server

- 官方提供 MCP Server，支持 VSCode、Cursor、Claude Desktop
- 能力：
  - 搜索模型和数据集
  - Dataset Viewer MCP：验证数据集存在性、获取详细信息、分页检索行、统计信息、文本搜索、SQL 过滤、Parquet 下载
  - 访问 Spaces 和 Papers
- 还提供 `@huggingface/mcp-client` npm 包，可构建自定义 Agent

#### Google Data Commons MCP Server

- Google 官方于 2025 年 9 月发布
- 聚合全球公共数据集（经济、健康、人口、气候等），来源包括政府和国际组织
- Agent 可通过 MCP 原生消费 Data Commons 数据，无需学习底层 API
- 限制：仅限公共统计数据

#### 其他平台 MCP 封装

| 平台 | MCP Server | 数据类型 |
|------|-----------|---------|
| Dune Analytics | Dune Agent MCP | 链上数据、区块链分析 |
| Databricks Unity Catalog | Genie Spaces MCP | 企业结构化数据 |
| DataHub | DataHub MCP Server | 企业数据目录元数据 |
| Select Star | Select Star MCP | 数据目录（数据集、仪表板、字段、指标） |
| Bright Data | Web MCP | 网页抓取数据 |

### 2.2 ⭐ Vesper — Agent 原生的数据集工作流引擎

**目前最接近"Dataset Search Engine for Agents"的产品。**

- 定位：自主数据引擎（Autonomous Data Engine for AI Agents）
- 形态：MCP Server，Agent 像调用其他工具一样调用它
- 在 Hacker News (Show HN) 上发布，引起关注
- 核心能力：
  1. **跨源搜索**：跨多个数据集平台搜索
  2. **自动下载**：找到后直接下载
  3. **质量分析**：自动评估数据质量
  4. **数据清洗**：自动清理问题数据
  5. **导出**：输出为 Agent 可用的格式
- 全流程无需人工干预
- 网站：https://getvesper.dev

### 2.3 MCP Server 发现层

这一层解决的是"Agent 如何发现新的数据源/工具"的问题。

#### MCPfinder

- 自身是一个 MCP Server，连接到 Agent 后，Agent 可以动态发现和启用其他 MCP Server
- 工作流：Agent 需要某个能力 → 问 MCPfinder → 找到对应 MCP Server → 自动安装和连接
- 相当于"MCP Server 的搜索引擎"
- 网站：https://mcpfinder.dev

#### .well-known/mcp.json 标准

- 类似 robots.txt 的发现机制
- 网站在 `/.well-known/mcp.json` 路径暴露 MCP Server 信息
- AI 助手（Claude、ChatGPT、Cursor）可自动检测、检查和连接
- WellKnownMCP.org 正在推动标准化，包括 JSON Schema 验证和 LLMCA（认证机构）

#### MCP Registry API（官方）

- 开源的 MCP Server 目录，作为统一发现点
- 提供 REST API 供开发者搜索相关 MCP Server
- 强制标准化格式

#### 学术研究：Intent-Aware MCP Server Retrieval

- 使用双编码器模型、层次化向量路由和加权语义嵌入
- 根据用户意图动态选择和查询 MCP Server
- 实现实时 API 发现、风险感知执行

### 2.4 传统数据集搜索引擎（面向人类）

这些是面向人类的数据集搜索引擎，Agent 目前只能通过网页抓取或非标准 API 间接使用。

| 搜索引擎 | 覆盖范围 | Agent 可用性 |
|---------|---------|------------|
| **Google Dataset Search** | 3000 万+ 数据集，基于 schema.org/Dataset 标记 | ❌ 无官方 API，Agent 无法直接调用 |
| **Kaggle Datasets** | ML/AI 数据集 | ✅ 有 MCP Server |
| **Hugging Face Hub** | ML 模型和数据集 | ✅ 有 MCP Server |
| **data.gov** | 美国政府公开数据 | ⚠️ 有 API 但无 MCP |
| **OpenDataBay** | AI/LLM 训练数据市场 | ❌ 面向人类买家 |
| **Snowflake Marketplace** | 企业数据 | ⚠️ 有 MCP 但面向企业 |

### 2.5 新兴的 Agent 原生数据平台

#### Inflectiv

- 将非结构化数据转化为结构化、Token 化的智能资产
- Agent 可通过 API/SDK 查询、推理和使用
- 数据贡献者可变现，数据可追溯、可交易
- 与 Walrus（Sui 链上存储）集成，实现去中心化持久存储
- 定位为"$10T+ AI Agent 经济的缺失原语"
- 网站：https://inflectiv.ai

#### Mflo

- Pay-per-query 数据集市场（上一份调研已详述）
- Agent 通过 x402 协议按查询付费获取数据
- 网站：https://mflo.ai

---

## 三、学术前沿

### DatasetResearch Benchmark（2025）

- 来源：GAIR-NLP（上海交大/复旦相关团队）
- 发表于 OpenReview，GitHub 开源
- 核心问题：**AI Agent 能否超越传统搜索，系统性地发现满足特定需求的数据集？**
- 构建了 208 个真实世界需求的 benchmark，覆盖知识密集型和推理密集型任务
- 评估 Agent 从发现到综合数据集的全流程能力
- 提出"demand-driven dataset discovery"概念
- 论文：https://arxiv.org/html/2508.06960
- 代码：https://github.com/GAIR-NLP/DatasetResearch

### MCP Registry 学术分析（2025）

- 分析了 5 种 Agent 注册/发现方案：MCP Registry（中心化）、企业级、分布式等
- 探讨自主 AI Agent 在云、企业和去中心化环境中的可信发现、能力协商和身份保证
- 论文：https://arxiv.org/html/2508.03095v3

---

## 四、Gap 分析

### 理想的"Dataset Search Engine for Agents"应该是什么样？

```
Agent 有数据需求
  → 用自然语言描述需要什么数据
  → 搜索引擎跨所有数据源（公开+私有+付费）统一检索
  → 返回结构化结果：数据集描述、schema、质量评分、价格、许可证
  → Agent 评估后选择
  → 自动获取（免费下载 or 付费购买）
  → 数据就绪，可直接使用
```

### 现状 vs 理想

| 能力 | 现状 | 差距 |
|------|------|------|
| 跨平台统一搜索 | ⚠️ Vesper 做了初步尝试，但覆盖有限 | 需要聚合 Kaggle + HF + Google Dataset Search + 政府数据 + 商业数据 |
| 私有/付费数据集发现 | ❌ 几乎不存在 | 需要去中心化发现协议 |
| 数据集质量评估 | ⚠️ Vesper 有基础质量分析 | 需要标准化的质量评分体系 |
| 自动付费获取 | ⚠️ Mflo/x402 可以做到 | 需要与搜索层打通 |
| Schema 标准化 | ⚠️ schema.org/Dataset 存在但 Agent 不易用 | 需要 Agent 原生的数据集描述标准 |
| 许可证理解 | ❌ Agent 不理解数据许可证含义 | 需要机器可读的许可证协议 |

### 核心缺失

1. **没有统一的跨源数据集搜索 API**
   - Google Dataset Search 索引了 3000 万数据集但没有公开 API
   - 各平台 MCP Server 是孤岛，没有聚合层

2. **私有数据集完全不可发现**
   - 个人/企业的私有数据集没有标准化的暴露和发现机制
   - 上一份调研的结论：没有"本地运行 → 挂到网上 → Agent 发现"的工具

3. **搜索与交易未打通**
   - 搜索层（Vesper/Kaggle MCP）和支付层（x402/Mflo）是分离的
   - Agent 找到数据集后，无法在同一个流程中完成购买

4. **数据集质量和可信度无标准**
   - Agent 无法判断数据集是否真实、完整、未被篡改
   - 没有类似"数据集信用评分"的机制

---

## 五、技术栈拼图

如果要构建一个完整的"Dataset Search Engine for Agents"：

```
发现层:
  ├── 公开数据集索引（爬取 schema.org/Dataset 标记的网页，类似 Google Dataset Search）
  ├── 平台 API 聚合（Kaggle API + HF API + data.gov + ...）
  ├── 去中心化发现（.well-known/mcp.json + P2P 广播）
  └── 私有数据集注册（个人用户通过本地节点注册元信息）

搜索层:
  ├── 语义搜索（向量化数据集描述 + schema 匹配）
  ├── 需求理解（自然语言 → 结构化数据需求）
  └── 质量排序（数据质量评分 + 用户评价 + 使用频次）

交易层:
  ├── 免费数据集：直接下载
  ├── 付费数据集：x402 协议自动支付
  └── 隐私数据集：Compute-to-Data / ZKP 验证

接口层:
  └── MCP Server（Agent 通过标准 MCP 协议调用以上所有能力）
```

这个东西目前不存在。最接近的是 Vesper（搜索+下载+清洗）+ Mflo（付费获取）+ MCPfinder（动态发现），但它们是分离的、覆盖面有限的。

---

## 六、与你们方向的关联

结合前两份调研，一个清晰的产品机会浮现：

> **一个去中心化的、面向 AI Agent 的数据集发现与交易网络**
>
> - 个人用户本地运行节点 → 注册数据集元信息到 P2P 网络
> - Agent 通过 MCP 协议搜索 → 发现公开+私有+付费数据集
> - x402 协议自动完成支付
> - 可选隐私计算层保护数据不被拷走
> - 数据质量通过链上声誉系统验证

这本质上是把 **Google Dataset Search（发现）+ Ocean Protocol（交易）+ MCP（Agent 接口）+ x402（支付）** 融合成一个 Agent-native 的产品。目前没有人在做这件事。

---

*Content was rephrased for compliance with licensing restrictions.*

**参考来源：**
- [1] Vesper - Autonomous Data Engine for AI Agents - https://getvesper.dev/
- [2] DatasetResearch Benchmark - https://arxiv.org/html/2508.06960
- [3] Google Data Commons MCP Server - https://developers.googleblog.com/en/datacommonsmcp/
- [4] Kaggle MCP Server - https://playbooks.com/mcp/arrismo/kaggle-mcp
- [5] Hugging Face MCP Server - https://huggingface.co/docs/hub/en/agents
- [6] MCPfinder - https://mcpfinder.dev/
- [7] .well-known/mcp.json 标准 - https://www.ekamoira.com/blog/mcp-server-discovery-implement-well-known-mcp-json-2026-guide
- [8] WellKnownMCP - https://wellknownmcp.org
- [9] MCP Registry API - https://nordicapis.com/getting-started-with-the-official-mcp-registry-api/
- [10] Google Dataset Search - https://developers.google.com/search/docs/appearance/structured-data/dataset
- [11] Inflectiv - https://inflectiv.ai/
- [12] Select Star MCP - https://www.selectstar.com/product/mcp-for-data
- [13] MCP Registry 学术分析 - https://arxiv.org/html/2508.03095v3
- [14] Mflo - https://docs.mflo.ai/
- [15] HN: Vesper Show HN - https://news.ycombinator.com/item?id=47384735


---

## 七、面向 AI Agent 的 Novel P2P Dataset Search Protocol：竞品与相关协议分析

> 以下内容聚焦于构建一个面向 AI Agent 的 P2P 数据集搜索协议所涉及的四大核心能力：
> Data Sharing、Data Authentication、Data Trading、Data Valuation，
> 并对每个维度的现有竞品和相关协议进行分析。

### 7.1 Data Sharing：P2P 数据共享协议

#### 7.1.1 基础网络层

| 协议/项目 | 类型 | 核心机制 | Agent 适配性 |
|-----------|------|---------|-------------|
| **libp2p** | P2P 网络栈 | 模块化（传输/发现/路由/加密），支持 DHT、mDNS、gossip | ✅ OpenPond 已用于 Agent P2P |
| **IPFS** | 内容寻址存储 | CID 内容哈希 + DHT 路由 + BitSwap 交换 | ✅ 已有 AI+IPFS 集成研究 |
| **BitTorrent/DHT** | 文件分发 | Info Hash + DHT peer 发现 + 分片传输 | ⚠️ 面向文件，非结构化数据 |
| **Filecoin** | 激励存储网络 | Proof-of-Replication + Proof-of-Spacetime + PDP (2025) | ⚠️ 存储层，非搜索层 |

#### 7.1.2 Agent 专用 P2P 协议

| 协议 | 发起方 | 核心设计 | 与数据集搜索的关系 |
|------|--------|---------|------------------|
| **OpenPond** | DuckAI | libp2p + Ethereum，Agent 发现/连接/通信 | 提供 Agent 间 P2P 通信基础，可扩展为数据交换 |
| **DIAP** (Decentralized Interstellar Agent Protocol) | 学术 (arxiv 2511.11619) | libp2p + ZKP 身份验证 + 混合 P2P 栈 | 提供隐私保护的 Agent 身份和通信，可作为数据共享信任层 |
| **DUADP** (Decentralized Universal AI Discovery Protocol) | 开源 | 联邦 DNS + WebFinger + gossip 协议 | 解决"任何 Agent 找到任何 Agent"的发现问题 |
| **ARDP** (Agent Registration and Discovery Protocol) | IETF draft | 传输无关的 Agent 注册与发现 | IETF 标准化进程中，可能成为正式标准 |
| **ATP** (Agent Trust Protocol) | zCloak AI / ICP | 链上身份 + AI-Name 系统 + 信任四支柱 | 提供 Agent 身份和信任基础设施 |

#### 7.1.3 关键洞察

- **libp2p 正在成为 Agent P2P 的事实标准网络栈**（OpenPond、DIAP 均基于 libp2p）
- 2026 年 libp2p 年报明确提到其定位为"去中心化 AI 和自主系统的通信基础"
- 但现有 Agent P2P 协议都聚焦于**Agent 间通信**，没有专门针对**数据集发现和共享**的协议

### 7.2 Data Authentication：数据认证与可验证性

#### 7.2.1 现有方案

| 方案 | 机制 | 适用场景 | 局限 |
|------|------|---------|------|
| **C2PA** (Content Credentials) | 加密签名 + 篡改检测 + 来源追踪 | 媒体文件真实性验证 | 面向图片/视频，未覆盖结构化数据集 |
| **Ocean Compute-to-Data** | 计算移动到数据侧，买家看不到原始数据 | 隐私保护的数据使用 | 不验证数据质量，只保护数据不泄露 |
| **ZKP 数据验证** | 零知识证明数据满足特定属性 | 证明数据集统计特征而不暴露数据 | 计算开销大，实用性有限 |
| **Merkle Tree / 内容哈希** | 数据分片哈希树 | 验证数据完整性和未篡改 | 只验证完整性，不验证质量/真实性 |
| **schema.org/Dataset** | 结构化元数据标记 | Google Dataset Search 索引 | 元数据可伪造，无加密保证 |
| **Croissant (JSON-LD)** | ML 数据集元数据标准 | Kaggle/HuggingFace 数据集描述 | 描述性标准，非验证性标准 |

#### 7.2.2 学术前沿

- **端到端可验证 AI 流水线** (arxiv 2503.22573)：用 ZKP 验证过程完整性 + 密码学承诺验证数据对象，覆盖从数据到模型的全链路
- **ZK-DPPS** (arxiv 2410.15568)：零知识去中心化数据共享中间件，FHE 加密计算 + SMPC 密钥重建
- **Truthful Dataset Valuation** (arxiv 2405.18253)：通过点互信息保证数据提供者如实报告数据

#### 7.2.3 关键缺失

- **没有面向"数据集"的 C2PA 等价物** — C2PA 解决了媒体文件的来源认证，但结构化数据集（CSV/Parquet/数据库）没有类似标准
- **数据质量的可验证性**几乎空白 — 能证明"数据没被篡改"，但无法证明"数据是高质量的"

### 7.3 Data Trading：数据交易协议

#### 7.3.1 支付/交易层

| 协议/项目 | 机制 | Agent 原生 | 数据集适配 |
|-----------|------|-----------|-----------|
| **x402** (Coinbase) | HTTP 402 + USDC 链上即时结算 | ✅ Agent 钱包自动签名支付 | ⚠️ 按请求付费，非按数据集 |
| **ERC-8183** | 可编程托管：Client→Provider→Evaluator | ✅ Agent 间无中介交易 | ✅ 支持任务验证后释放资金 |
| **ERC-8004** | Agent 链上身份 NFT + 声誉系统 | ✅ Agent 身份和信任 | ⚠️ 身份层，非交易层 |
| **Ocean Datatoken** | ERC20 数据代币 + AMM 自动定价 | ❌ 面向人类 | ✅ 专为数据集设计 |
| **Nevermined Pay** | MCP Server paywall + 实时结算 | ✅ per-tool-call 计费 | ⚠️ 工具调用，非数据集 |
| **Mflo** | x402 + EIP-3009 元交易 | ✅ pay-per-query | ✅ 数据集市场 |

#### 7.3.2 关键洞察

- **ERC-8183 是目前最完整的 Agent 交易协议**：Client 创建 Job → 资金锁定在智能合约 → Provider 提交工作 → Evaluator 验证 → 释放支付。2026 年 1 月已上线以太坊主网。由 MetaMask、以太坊基金会、Google、Coinbase 工程师共同编写。
- **x402 + MCP 组合**正在形成 Agent 支付的事实标准，但缺少数据集特有的交易逻辑（预览、采样、许可证协商）
- **数据集交易与 API 调用交易有本质区别**：数据集需要预览/采样→评估→协商→批量获取→验证完整性，现有协议都是为单次请求设计的

### 7.4 Data Valuation：数据估值

#### 7.4.1 学术方法

| 方法 | 核心思想 | 计算复杂度 | 实用性 |
|------|---------|-----------|--------|
| **Data Shapley** | 博弈论公平分配，量化每个数据点对模型的贡献 | O(2^N) 精确 / O(NlogN) 近似 | 小数据集可用，大规模不实际 |
| **Fast-DataShapley** | 训练可复用的 explainer 模型，实时推理 | 一次训练后 O(1) 推理 | ✅ 可实际部署 |
| **Unlearning Shapley** | 通过机器遗忘计算 Shapley 值，无需重训练 | 显著低于传统方法 | ✅ 隐私友好 |
| **Fairshare Pricing** (OpenReview 2025) | 用数据估值方法为 LLM 训练数据定价 | 取决于估值方法 | ✅ 直接面向市场定价 |
| **Data Distribution Valuation** (arxiv 2410.04386) | 对数据分布而非离散数据集估值 | 理论框架 | ⚠️ 学术阶段 |
| **Influence Functions** (arxiv 2405.13954) | 用影响函数量化数据对 GPT 的价值 | 可扩展到 LLM 规模 | ⚠️ 需要模型访问权 |

#### 7.4.2 实际市场定价

现有数据市场的定价方式远比学术方法粗糙：

| 平台 | 定价方式 |
|------|---------|
| OpenDataBay | 卖家自定价（一次性/订阅/按 API 调用） |
| Snowflake Marketplace | 提供者自定价 |
| Ocean Protocol | AMM 自动做市 + datatoken 供需定价 |
| Mflo | 按查询固定价格 |
| x402 生态 | 按请求固定价格（低至 $0.001） |

#### 7.4.3 关键缺失

- **学术估值方法与市场定价完全脱节** — Shapley value 等方法需要知道数据对模型的贡献，但在交易前买家还没用数据
- **缺少 Agent 可执行的自动估值** — Agent 需要在购买前快速评估数据集价值，目前没有标准化的"数据集质量评分 API"
- **动态定价机制缺失** — 数据集价值随时间衰减（新鲜度）、随使用增加（网络效应）、随竞品出现下降，没有协议处理这些

---

## 八、综合竞品矩阵

将所有相关项目按"P2P 数据集搜索协议"的四大能力维度打分：

| 项目 | Data Sharing | Data Auth | Data Trading | Data Valuation | Agent Native | 综合 |
|------|:-----------:|:---------:|:------------:|:--------------:|:------------:|:----:|
| **Ocean Protocol** | ⬤⬤⬤ | ⬤⬤◯ | ⬤⬤⬤ | ⬤◯◯ | ⬤◯◯ | 中 |
| **Vana Network** | ⬤⬤◯ | ⬤◯◯ | ⬤⬤◯ | ⬤◯◯ | ⬤◯◯ | 低 |
| **Masa Network** | ⬤⬤◯ | ⬤⬤◯ | ⬤⬤◯ | ⬤◯◯ | ⬤⬤◯ | 中低 |
| **Bagel Network** | ⬤⬤◯ | ⬤◯◯ | ⬤⬤◯ | ⬤◯◯ | ⬤⬤◯ | 中低 |
| **Mflo** | ⬤◯◯ | ⬤◯◯ | ⬤⬤⬤ | ⬤◯◯ | ⬤⬤⬤ | 中 |
| **Inflectiv** | ⬤⬤◯ | ⬤⬤◯ | ⬤⬤◯ | ⬤◯◯ | ⬤⬤⬤ | 中 |
| **Fetch.ai** | ⬤⬤⬤ | ⬤⬤◯ | ⬤⬤⬤ | ⬤◯◯ | ⬤⬤⬤ | 中高 |
| **Vesper** | ⬤⬤◯ | ⬤◯◯ | ⬤◯◯ | ⬤◯◯ | ⬤⬤⬤ | 中低 |
| **OpenPond** | ⬤⬤⬤ | ⬤⬤◯ | ⬤◯◯ | ⬤◯◯ | ⬤⬤⬤ | 中 |
| **ERC-8183 生态** | ⬤◯◯ | ⬤⬤⬤ | ⬤⬤⬤ | ⬤◯◯ | ⬤⬤⬤ | 中高 |

⬤ = 有能力, ◯ = 无能力

**核心结论：没有任何一个项目在四个维度上都达到强水平。** Fetch.ai 和 ERC-8183 生态最接近，但前者缺少数据估值，后者缺少数据共享层。

---

## 九、Novel P2P Dataset Search Protocol 的设计空间

基于以上分析，一个完整的协议需要四层：

```
┌─────────────────────────────────────────────┐
│  Layer 4: Valuation 估值层                    │
│  - 数据集质量评分（schema完整性/新鲜度/覆盖度）  │
│  - Agent 可调用的自动估值 API                   │
│  - 动态定价引擎（供需/时间衰减/独特性）          │
├─────────────────────────────────────────────┤
│  Layer 3: Trading 交易层                      │
│  - ERC-8183 式托管（预览→评估→购买→验证）       │
│  - x402 微支付（采样预览按次付费）               │
│  - 许可证协商（机器可读的使用条款）              │
├─────────────────────────────────────────────┤
│  Layer 2: Authentication 认证层               │
│  - 数据集 C2PA（来源签名 + 篡改检测）           │
│  - ZKP 属性证明（证明统计特征不暴露数据）        │
│  - Merkle 完整性验证                           │
├─────────────────────────────────────────────┤
│  Layer 1: Sharing & Discovery 共享发现层       │
│  - libp2p P2P 网络                            │
│  - DHT 数据集元信息索引                        │
│  - MCP Server 标准接口                        │
│  - .well-known/mcp.json 自动发现              │
└─────────────────────────────────────────────┘
```

**可复用的现有组件：**
- Layer 1: libp2p + MCP + .well-known/mcp.json
- Layer 2: Merkle Tree + ZKP (Noir circuits from DIAP)
- Layer 3: ERC-8183 + x402
- Layer 4: Fast-DataShapley + 自定义质量评分

**需要从零构建的：**
- 数据集元信息的 DHT 索引协议（类似 BitTorrent DHT 但面向结构化数据集）
- 数据集专用的 C2PA 等价标准
- 预览/采样→评估→批量获取的交易工作流
- Agent 可调用的自动估值 MCP 工具

---

*Content was rephrased for compliance with licensing restrictions.*

**补充参考来源：**
- [16] DIAP 论文 - https://arxiv.org/abs/2511.11619
- [17] OpenPond 协议 - https://protocol.duckai.ai/
- [18] DUADP - https://duadp.org/
- [19] ARDP IETF Draft - https://www.ietf.org/archive/id/draft-pioli-agent-discovery-01.html
- [20] ATP Agent Trust Protocol - https://forum.dfinity.org/t/atp-the-agent-trust-protocol-built-on-the-ic/64288
- [21] ERC-8183 - https://www.ccn.com/education/crypto/erc-8183-programmable-escrow-ai-agents-ethereum-how-it-works/
- [22] ERC-8004 - https://web3.gate.com/blog/100237/ethereum-erc-8004-ai-agent-autonomous-trading-m2m-economy
- [23] Fetch.ai - https://coinpaprika.com/education/what-is-fetch-ai-fet-and-how-its-ai-agent-network-works/
- [24] C2PA 标准 - https://c2pa.org/specifications/specifications/2.3/explainer/Explainer.html
- [25] Fairshare Data Pricing - https://openreview.net/forum?id=QnFNXM5tBp
- [26] Data Shapley - https://ar5iv.labs.arxiv.org/html/1902.10275
- [27] Fast-DataShapley - https://arxiv.org/html/2506.05281v2
- [28] ZK-DPPS 中间件 - https://arxiv.org/html/2410.15568v1
- [29] 端到端可验证 AI 流水线 - https://arxiv.org/html/2503.22573v1
- [30] Filecoin PDP - https://www.ccn.com/top-101-in-crypto/filecoin/
- [31] Vana Whitepaper - https://www.vana.org/posts/vana-whitepaper
- [32] libp2p 2025 年报 - https://docs.libp2p.io/reports/annual-reports/2025/
- [33] Phala Compute-to-Data - https://phala.com/solutions/private-ai-data
