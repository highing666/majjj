# 面向 4 人标准推倒胡的 Rust 强麻将 AI 算法与开源参考调研

## 1. 执行摘要与面向推倒胡的推荐算法管线

针对**家庭局域网 4 人标准推倒胡麻将**（1 人 + 3 AI 或双人 + 2 AI），在**纯 Rust 本地运行、零显存/无神经网络推理依赖、单步响应毫秒至数百毫秒级**的工程约束下，强 AI 的“压迫感”来自三个维度：**极致的进攻速度（牌效与副露节奏）**、**无立直信号下的精准兜牌/止损（牌河读牌与危险度估计）**，以及**面向未来规则扩展的可配置架构**。

调研表明，学术界与开源社区在**高速向听/进张计算**和**非神经网络搜索+统计对手建模**上已有成熟的一手积累，但绝大部分 Rust 麻将生态面向**日本立直麻将（Riichi Mahjong）**。由于推倒胡具备**无振听（No Furiten）、无立直（No Riichi）、无宝牌/里宝牌（No Dora/Ura-dora）、通常允许屁胡（0 番/鸡胡即可胡牌）**等规则特征，直接照搬日麻防守假设（尤其是 `Genbutsu` 现物与 `Suji` 筋）会产生致命误判，必须对搜索目标、危险度先验和副露评估做结构性改造。

### 推荐的纯 Rust 四层混合决策管线（50–200 ms 预算）

```text
[公开局面 + 自家手牌]
   │
   ▼
L1: 状态表示与高速牌效核（< 0.1 ms）
   │  • [u8; 34] 计数 + u64/u128 位掩码 + 剩余可见枚数表
   │  • 向听数（Shanten）& 进张（Ukeire）/ 余牌（Fuyouhai）位掩码计算
   ▼
L2: 单人期望收益评估（1–5 ms）
   │  • 参考 tomohxx/mahjong-win-prob 与 libriichi::algo::sp 的剩余巡目 DP
   │  • 输出每个候选舍牌/鸣牌后的多巡听牌概率 P(tenpai)、胡牌概率 P(win) 与自摸/荣和期望打点 E[Score]
   ▼
L3: 非立直对手威胁与单张放炮危险度模型（< 1 ms）
   │  • 无立直连续听牌后验 P(opp_tenpai_i | 巡目, 副露数, 手切/摸切, 牌河特征)
   │  • 重构危险度先验：降权现物/筋（无振听失效），升权壁 Kabe（No-Chance/One-Chance 组合不变量）与染手/碰碰胡番型警戒
   ▼
L4: 根部限定采样 + 渐进剪枝的浅层搜索 / 攻守权衡（20–150 ms，Anytime 可中断）
      • 参考 SIMCAT (Chen et al., 2022) 与 Mizukami & Tsuruoka (2015) / Kurita & Hoki (2021)
      • 对前 M 个高候选动作，在 K 组按对手建模加权采样的隐藏牌墙（Determinization）中执行浅层 1-ply/2-ply Rollout 或 Expectimax
      • 以 Progressive Pruning（渐进剪枝）淘汰劣势动作，输出最终舍牌或 吃/碰/杠/过 决策
```

**为什么推荐“L2 精确单人 DP + L3 非立直危险度 + L4 渐进剪枝浅层采样”，而非深度 ISMCTS 或大型神经网络？**
1. **深度树搜索在麻将中的天然瓶颈**：正如 Suphx 论文（Li et al., 2020）和 Cowling 等人（2012）指出的，4 人不完全信息麻将具有极高的隐藏状态维度与多人吃碰杠打断分支；直接展开多层 ISMCTS 会在固定时间预算内因 **Strategy Fusion（策略融合）** 与 **Non-locality（非局部性）** 导致采样稀疏且易产生脆弱的“事后诸葛亮”决策。
2. **扁平蒙特卡洛 + 渐进剪枝在无立直麻将中的实战有效性**：台湾 16 张麻将 AI **SIMCAT**（Chen, Tang, & Wu, 2022，同样无立直、无宝牌、允许副露）在 Computer Olympiad 2020 与 TAAI 2019/2020 夺冠，证明了**基于置信区间的渐进剪枝（Progressive Pruning）+ 领域启发式扁平蒙特卡洛（Flat MC）**能够在毫秒级 anytime 约束下稳定战胜传统搜索基线。
3. **日麻搜索与 MDP 抽象验证了浅层非 NN 路线的上限**：Mizukami & Tsuruoka（2015，*Bakuuchi* 前身）仅靠“回归对手模型 + 蒙特卡洛模拟”即在天凤达到 1718 分（后续版本达 R2206）；Kurita & Hoki（2021）通过将对手合并为概率玩家（Chance Player）进行多 MDP 抽象，进一步在无大型深度网络树展开下击败了强基线 AI；而 `critter-mj/akochan` 凭借手调参数、单人 DP 与统计危险度表也在无深度神经网络时期长期保持天凤凤凰桌水准。

---

## 2. 高速向听数（Shanten）与进张效率（Ukeire）算法调研

向听数（Shanten，或 Replacement Number = Shanten + 1）与有效进张（Ukeire / Yūkōhai）是麻将 AI 树搜索与 Rollout 中调用频率最高的基础算子（单回合可能被调用数万至数十万次）。

### 2.1 四类核心算法对比

| 算法路线 | 核心原理 | 代表实现 | 表内存 / 初始化开销 | 单次评估速度与复杂度 | 规则扩展性（推倒胡及变体） |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **剪枝回溯分解法**（DFS + Mentsu/Taatsu 剥离） | 递归枚举各花色面子 $M$、搭子 $T$ 与雀头 $P$，取 $8 - 2M - T - P$ 最小值（需处理总块数 $\le 4$ 上限） | `Apricot-S/shanten-algorithm-collection`（`decomp_ara` / `decomp_kobayashi`）、`agari`、`riichi-tools-rs` 默认模式 | 零静态表（0 B），零初始化耗时 | 较慢（微秒级，随手牌重叠度波动） | **极高**，不依赖 14 张前提，易于修改面子/特殊牌型定义 |
| **无压缩花色拆分查表**（Base-5 / 8 进制静态 LUT） | 将单花色 9 种牌计数编码为 5 进制（$5^9 = 1,953,125$ 状态）或 8 进制（27-bit `u32`），查表输出单花色贡献向量再合并 | `tomohxx/shanten-number`（无 `ENABLE_NYANTEN` 时生成 `index_s.bin` / `index_h.bin`）、`summivox/riichi-decomp-table` | 较大（未压缩 Base-5 若每项 8 B 约 15.6 MB；`index_*.bin` 达数 MB 级；需构建/加载二进制表） | 极快（纳秒至亚微秒级，$O(1)$ 查表 + 小规模状态合并） | **中等**，通常绑定标准 4 面子 1 雀头及最大 14 张限制 |
| **最小完美哈希 / 简洁树压缩查表**（MPHF / LOUDS Trie） | 利用“手牌总枚数 $\le 14$”过滤无效 Base-5 状态，再用 **MPHF** 或 **LOUDS** 紧凑编码数牌状态语言，实现极高缓存命中率 | `Cryolite/nyanten`（C++）、`Apricot-S/xiangting`（Rust）、`tomohxx/shanten-number -DENABLE_NYANTEN` | **极小**（由十余 MB 级压缩至 KB 至低 MB 级；编译期或启动期毫秒级加载） | **极快**：`Cryolite/nyanten` 在 Intel i9-12900 / GCC 下实测约 **40 ns / hand** | **受限**：通常严格限定手牌枚数 $\le 14$ 张（如 `ENABLE_NYANTEN` 的显式约束） |
| **集合递推动态规划**（Set-Recurrence DP） | 按花色/牌种递推维护 $(M, P)$ 状态转移向量，无需外部静态表文件，支持残余枚数与任意牌数约束 | `tomohxx/shanten-dp-rust`（`shanten-dp`）、`tomohxx/mahjong-win-prob` | **极小**（零外部表文件，无需磁盘加载） | 快（亚微秒级，可同时输出必要牌与不要牌位掩码） | **极高**：天然支持任意张数、副露数、枚数上限（如 3 麻/换三张/缺门） |

### 2.2 关键数据结构与底层技巧

1. **34-Tile 紧凑表示与位掩码（Bitmask）**：
   - 几乎所有高性能 Rust 麻将库（`xiangting`、`shanten-dp`、`agari`、`riichi-decomp`、`libriichi`）均采用 **`[u8; 34]`** 表示每种牌的持牌枚数（`0..=4`，依次对应 `1-9m`, `1-9p`, `1-9s`, `1-7z`）。
   - 对于**有效进张集合（Ukeire）**、**不要牌集合（Fuyouhai）**、**现物/振听集合**与**吃碰合法掩码**，采用 **`u64` 位掩码**（低 34 bit 对应 34 种牌）：一次集合求交/求并/算剩余枚数仅需单条 CPU 位运算指令，避免堆内存分配（`Vec`）。
   - 如 `riichi-decomp-table` 将单花色 9 种牌的计数直接打包为 **27-bit 八进制 `u32`**（每种牌占 3 bit），通过 `nanovec` 做紧密位压缩存储。
2. **避免重算 Ukeire 的增量与同时计算技巧**：
   - 朴素求“打出哪张牌进张最多”需要枚举 $\le 14$ 种舍牌 $\times 34$ 种摸牌，调用 400+ 次向听函数；即便单次 40 ns 也会放大至十数微秒，在 MC Rollout 中累积开销惊人。
   - **同时计算（Simultaneous Calculation）**：`tomohxx/necessary-and-unnecessary-tiles` 与 `shanten-dp-rust` 通过一次 DP 扫描**同时返回当前手牌的“必要牌 bitmask”与“不要牌 bitmask”**，直接过滤掉所有导致向听退步的废牌，使进张评估开销降低一个数量级。
   - **`Apricot-S/shanten-algorithm-collection` 的正确性基准**：该仓库使用 `normal`、`half_flush`（混一色）、`full_flush`（清一色）、`thirteen_orphans`（国士无双）四组各 **10,000 手**固定种子数据集做回归与性能测试，重点暴露了经典分解法的常见边界 Bug（如“4 副露无雀头”时，只有 `decomp_kobayashi` 等修正版通过，旧分解器容易误判）。

### 2.3 对推倒胡的直接复用与定制点

- **基础向听与牌效可直接复用**：推倒胡标准胡牌结构同样是 **4 面子 + 1 雀头**（一般形）以及常见的 **七对子（Chiitoitsu）**（注意国内规则中“豪华七对/龙七对”常允许 4 张同牌作两对，`shanten-dp` 原生支持四枚使い七対子，而部分日麻实现默认禁止同牌四张七对，需特别校验！）。
- **推倒胡可裁剪/扩展项**：若本地规则不支持“国士无双/十三幺”，可在 `Mode` 中关闭国士向听分支；若未来扩展四川“血战到底”（必须缺一门 `Que Men`），可直接利用 `shanten-dp` 的 `tile_limits` 将缺门花色上限设为 `0`，无需重写向听内核。

---

## 3. 不完全信息下的搜索算法与非立直防守推理

### 3.1 经典不完全信息搜索算法对比

1. **PIMC（Perfect Information Monte Carlo / 朴素限定采样）与 ISMCTS（Information Set MCTS）**：
   - Cowling, Powley, & Whitehouse (2012) 在 *IEEE TCIAIG* 指出，对隐藏信息采样得到完整确定性牌局（Determinization）后直接运行完美信息搜索，存在两大理论缺陷：
     - **Strategy Fusion（策略融合）**：搜索树误以为 AI 在信息集内部能区分不同真实世界状态并采取不同分支策略；
     - **Non-Locality（非局部性）**：不同采样状态各自建树，统计量不共享且极易低估对手的隐蔽选项。
   - **SO-ISMCTS / MO-ISMCTS** 通过在**信息集（Information Set）**节点上聚合统计量来缓解上述问题。然而在 4 人麻将中，完整一局的隐藏牌墙超过 $10^{48}$ 种排列（Chen et al., 2022），且每巡存在他家 吃/碰/杠/胡 的多分支响应；在纯 CPU、数十至数百毫秒限制下，深层 ISMCTS 的树节点展开深度与访问次数严重不足，实战棋力往往反而不如“高质量启发式 Rollout + 扁平采样”。
2. **扁平蒙特卡洛 + 渐进剪枝（Flat MC + Progressive Pruning，SIMCAT 路线）**：
   - Chen, Tang, & Wu (2022) 针对 16 张台湾麻将（同样**无立直、无振听式声明、无宝牌、允许吃碰**）提出了 **SIMCAT**：
     - 对所有合法候选动作（如手牌中不重复的 $\le 14$ 种舍牌），采样符合公开牌河与对手倾向的确定化世界（Determinization），执行快节奏启发式模拟；
     - 引入基于统计置信区间的 **Progressive Pruning（渐进剪枝）**：在几轮采样后提前淘汰显著落后的候选牌，把算力集中到 Top-2 ~ Top-4 关键选择上；
     - 具备 **Anytime（随时可中断）** 特性，完美契合家庭局域网 50–200 ms 的交互延迟要求。
3. **单人期望搜索（Single-Player DP / Expectimax）与 MDP 抽象**：
   - **单人 DP（`tomohxx/mahjong-win-prob`、`libriichi::algo::sp::SPCalculator`）**：把“从当前手牌到胡牌”建模为受剩余巡目（如 `MAX_TSUMOS_LEFT`）与剩余牌枚数约束的有限步动态规划，精确计算打掉每张牌后未来各巡的**听牌概率、自摸胡牌概率与期望得分**（无需蒙特卡洛噪声，零方差）。
   - **多 MDP 抽象与机会玩家合并（Kurita & Hoki, 2021）**：由于直接展开 4 人博弈树在计算上不可行，作者将另外三家抽象为按历史统计规律行动的“机会玩家（Chance Player）”，将多人问题分解为若干 MDP，使状态价值推断与浅层 Expectimax 树在毫秒至百毫秒内可解，且实验击败了高段位基准 AI *Bakuuchi*。
   - **监督对手模型 + MC（Mizukami & Tsuruoka, 2015）**：将复杂不可见的对手手牌抽象为三个可计算输出——**是否听牌（Logistic Regression）**、**34 种牌各自的点炮概率（34 个 Logistic Regression）**、**若胡牌的期望打点（Linear Regression）**——再注入蒙特卡洛模拟计算动作期望收益。

### 3.2 核心差异剖析：无立直、无振听推倒胡 vs 日本立直麻将

这是决定推倒胡 AI 能否从“像日麻机器人乱打”蜕变为“真正会防守、有压迫感”的核心分水岭。下表总结了所有关键规则与推理机制的迁移结论：

| 机制 / 推理信号 | 日本立直麻将（Riichi） | 4 人标准推倒胡（无立直、无振听、无宝牌、可屁胡） | AI 算法改造结论 |
| :--- | :--- | :--- | :--- |
| **立直宣言（Riichi）** | 公开强制信号：宣告者 **100% 已听牌**，且牌形锁定无法换牌（除少数暗杠） | **不存在**。任何玩家听牌都不会发出显式声明 | **必须用连续听牌概率估计 $P(\text{Tenpai}_i) \in [0, 1]$ 替代二值立直判断**，融合巡目、副露数、手切/摸切序列与牌河特征 |
| **现物（Genbutsu）** | 因存在永久/立直振听，立直家牌河中的牌 **100% 绝对安全（0% 放炮）** | **不再保证 100% 安全！** 若无振听，对手早巡切过 `3m` 仍可能在后续换听或重构搭子后荣和 `3m`（若房规含“同巡过水不胡”，则仅提供**单巡内**的临时保护） | **降级处理**：不能把早巡现物当绝对安全牌硬留；但**近期切出**（尤其是高概率听牌阶段后的摸切或晚巡手切）仍因手牌结构连续性保留极低点炮先验 |
| **筋（Suji：1-4-7, 2-5-8, 3-6-9）** | 依赖**振听规则**封杀两面听（如切 `4m` 则不能以 `23m`/`56m` 荣和 `1m`/`7m`）；在 Chiba (Table 8.5) 的安全分级中，全筋（尤其是中张筋 `4/5/6`）位于中等安全层（`BBB–BB-` 级），显著优于非筋数牌（`CC–D` 级），仅仍怕嵌张/边张/双碰/单骑 | **规则保护完全失效！** 没有振听约束，切过 `4m` 后仍可保留或后摸 `23m`/`56m` 荣和 `1m`/`7m`；甚至 `2344m` 切 `4m` 后，若规则无振听，可同时胡 `1m` 与 `4m` | **从硬约束降级为弱统计先验**：早巡切 `4m` 仅说明*在切牌那一巡*对方大概率没有 `23m`/`56m` 强连形；随巡目推进、手切变多，该先验衰减（Decay）加快。切忌迷信推倒胡里的“晚巡筋牌” |
| **壁（Kabe：No-Chance / One-Chance）** | 场上明见 4 张 `2m`（No-Chance）或 3 张 `2m`（One-Chance）时，物理上阻断对手持有 `23m` 或 `123m` | **100% 组合数学不变量，完全成立！** 每种牌总共 4 张的物理约束与振听/立直无关 | **大幅升权！** 在推倒胡防守危险度排序中，**No-Chance（4 枚全见）与死绝字牌（4 枚全见）上升为最高可靠度的结构性安全牌**，One-Chance（见 3 枚）价值亦显著高于无振听下的普通筋牌 |
| **宝牌（Dora / 里宝牌）** | 宝牌周边的浮牌与高目打点极大改变攻防阈值（立直+宝牌易跳满） | **无宝牌/无里宝牌**。对手手牌打点方差缩小，威胁集中于可观测副露番型（清一色/混一色/碰碰胡）与自摸/门清加成 | 无需计算宝牌周围危险度溢价；**放炮代价估计更可预测**，AI 在中盘面对普通屁胡听牌时的对攻（Push）阈值可适当更果断 |
| **手切 vs 摸切（Tedashi vs Tsumogiri）** | 用于立直前隐蔽听牌判断及立直后确认牌形不变 | **核心替代雷达！** 因无立直，对手若已听牌且不换听，往往连续**摸切（Tsumogiri）**；若副露后突然打出中张**手切（Tedashi）**，极大概率在调整听牌或听牌瞬间 | 若局域网协议可观测手切/摸切，将其作为推倒胡听牌概率模型最高权重特征之一；即使规则隐藏手切/摸切，仍可由牌河顺序（早巡字牌 $\rightarrow$ 边张 $\rightarrow$ 突切中张）进行贝叶斯推断 |

### 3.3 推倒胡单张放炮危险度估算模型（工程可落地公式）

借鉴 Mizukami & Tsuruoka (2015) 的特征分解思想并针对推倒胡重设计（不依赖神经网络，仅用逻辑回归/可校准轻量权重表即可运行在微秒级）：

对于对手 $i \in \{1,2,3\}$ 和候选舍牌 $x \in [0, 33]$，估算点炮期望损失：
$$\text{Risk}_i(x) = P(\text{Tenpai}_i \mid H_i) \cdot P(\text{Wait}_i(x) \mid \text{Tenpai}_i, H_i, B) \cdot \mathbb{E}[\text{Score}_i \mid H_i]$$

1. **听牌概率 $P(\text{Tenpai}_i \mid H_i)$**：
   - 先验随当前巡目 $t$ 单调上升；
   - **副露数修正**：推倒胡允许屁胡，一旦对手 $1$ 副露（手牌 10 张）、$2$ 副露（7 张）或 $3$ 副露（4 张），其离听牌距离极大缩短，$2\text{–}3$ 副露时在 6–8 巡即可给出高听牌概率；
   - **牌河行为特征**：连续摸切长度、中张（`3..7`）打出的巡目位置（若早巡就打出 `4/5/6` 且随后切字牌，往往暗示手牌原本很整、或正在做混一色/清一色/七对）。
2. **候选牌 $x$ 的点炮条件概率 $P(\text{Wait}_i(x))$**（基于剩余可见牌数 $rem(x)$ 与几何形态）：
   - **结构性物理阻断**：
     - 若场上可见 4 张 $x$，则 $P(\text{Wait}_i(x)) = 0$（绝张）；
     - 计算围绕 $x$ 的**两面（Ryanmen）、嵌张（Kanchan）、边张（Penchan）、双碰（Shanpon）、单骑（Tanki）剩余组合权重**，直接乘以邻近牌的可见枚数限制——**自动统一涵盖 No-Chance 与 One-Chance（Kabe）**！例如 $x=1m$ 时，两面 `23m` 组合数正比于 $rem(2m) \times rem(3m)$（若 $rem(2m)=0$ 即 No-Chance，两面权重自动归零；若 $rem(2m)=1$ 即 One-Chance，权重自然骤降）。
   - **弱化的排斥修正（替代刚性现物/筋，以下为工程建议初始先验，需结合自博弈日志做逻辑回归校准）**：
     - 若 $x$ 在对手 $i$ **最近 $k$ 巡的牌河**中出现（尤其是听牌高概率窗口之后），给予较强衰减系数（示例先验 $0.15\text{–}0.35$，而非 $0.0$）；若在早巡（$1\text{–}4$ 巡）切出，系数回调至 $0.5\text{–}0.75$（反映无振听下回头胡的真实风险）。
     - 若形成无振听下的“早期筋（Suji）”，仅给予温和折减（示例先验 $0.7\text{–}0.85$），绝不视为安全牌。
   - **花色染手与碰碰胡修正（推倒胡核心读牌）**：
     - 若对手 $i$ 副露了 2 组同一花色（如 `万子`），且牌河中极早大量切出其他两门数牌，则该花色 $x$ 的权重按染手倍数放大（示例倍率 $2.5\times \text{–} 4.0\times$，并结合回归校准），且 $\mathbb{E}[\text{Score}_i]$ 同步提升至清一色/混一色量级；
     - 若对手连续 `碰（Pon）` 两副以上，显著提升生张字牌与牌河外侧牌的 `Shanpon/Tanki` 权重。

---

## 4. 吃碰杠副露（Meld / 鸣牌）与做牌策略

推倒胡与日麻在副露决策上的经济结构截然不同：

1. **没有“食断/无役”惩罚与立直机会成本**：
   - 在日麻中，副露会永久丧失“立直（1 番+里宝）”、“门前清自摸和（1 番）”、“平和（1 番）”，且在无食断规则下甚至无法胡牌；因此日麻 AI（如 Mortal / Akochan）常为了保门清高打点而频繁忍耐不鸣。
   - 在推倒胡中（尤其允许屁胡时），**吃/碰几乎不损失胡牌资格**，只要向听数前进，就能用“快速抢听 + 抢先自摸/荣和”给人类玩家极强的压迫感。
2. **推倒胡副露的真实代价模型**：
   算法必须在每一步面对可吃/可碰牌时，比较 **`EV(Meld)` vs `EV(Pass)`**，扣除三项真实代价：
   - **守备手牌缩水代价（Hand Shrinking Cost）**：每次副露使手里可自由替换的安全牌减少 3 张（13 $\to$ 10 $\to$ 7 $\to$ 4 $\to$ 1）。当检测到桌上已有其他家高概率听牌（且对手在做清一色/大牌），如果此时副露仍不能进入 0-shanten（听牌）或强 1-shanten，副露会导致后续毫无安全牌可退，应果断 **Pass**。
   - **进张宽度与打点转换代价**：
     - 避免“虚进张”陷阱：例如当前 1-shanten 已有 20 枚宽进张，盲目吃一个边张虽推进到 0-shanten 却锁定为仅剩 2 枚的单吊/嵌张，可通过 L2 的单人 DP `P(win)` 直接量化识别——若 `P(win | Meld) < P(win | Pass)`，自动拒绝劣质副露。
     - **大牌阈值判断**：当手牌满足特定番型原型（如起手/中盘已经有 5–6 对指向**七对子**，或单一花色已达 9–10 张指向**清一色**，或对子数 $\ge 4$ 指向**碰碰胡**），若当前副露会破坏七对子（七对禁止吃碰）或毁掉清一色加番收益，用期望得分 $\mathbb{E}[\text{Score}]$ 对比即可自然抑制短视屁胡鸣牌。
3. **杠（Kong）在无宝牌推倒胡中的决策逻辑**：
   - **明杠（大明杠 / 加杠）**：无杠宝牌收益，主要收益仅为“岭上摸一张 + 潜在杠开/杠分奖励（视具体地方推倒胡计分表而定）”，代价是**彻底暴露手牌信息（减少手中安全牌）**以及**加杠面临被抢杠胡（Qianggang）风险**。因此当场上存在高听牌概率对手时，应严格抑制低收益加杠/大明杠。
   - **暗杠（Ankan）**：不降门清状态且额外获得一次免费摸牌换形机会，但会固化暗刻结构（如拆掉潜在的顺子连形，或暴露多枚已打不出的牌）；可由 L2 DP 评估“暗杠后少 1 组可选顺子变化 vs 多 1 次免费岭上抽牌 + 杠分加成”的净胜率差决定。

---

## 5. 开源参考剖析（Rust crates 与经典博弈引擎）

以下是可直接参考或复用的 Rust crates 及经典开源麻将引擎评估表（均基于一手仓库与 crates.io / docs.rs 核验）：

| 项目 / Crate | 语言 / 版本 | 许可证 | 核心算法与数据结构 | 性能 / 内存特征 | 对本项目的适用性评估与避坑点 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`xiangting`** (`Apricot-S/xiangting`) | Rust (`v6.0.2`, `no_std`) | **MIT** | 受 Cryolite 启发；基于 `[u8; 34]` 与编译生成紧凑查找表计算 `replacement_number`、必要牌与不要牌 | 零外部运行时依赖（`no_std`），内存占用极低，纳秒至亚微秒级查表 | **首选快速向听核（标准 14 张内推倒胡）**。⚠️ 坑点：返回值是 `replacement_number`（`= shanten + 1`），调用时务必 `- 1`；且表结构绑定 $\le 14$ 张手牌上限 |
| **`shanten-dp`** (`tomohxx/shanten-dp-rust`) | Rust (`v0.3.2`) | **MIT** | 纯动态规划（Set-Recurrence DP），`[u8; 34]` 输入，支持同时输出必要/不要牌与 `tile_limits` 约束 | **零预计算表文件**，支持任意张数、副露、4 张同牌七对子；deps: `bitflags`, `thiserror` | **最适合复杂规则扩展的向听核**。若未来扩展血战到底（缺门 `tile_limits`）、花牌或 16 张制，可直接用其替换/兜底 |
| **`Cryolite/nyanten`** | C++ (Header-only) | **MIT** | Base-5 花色状态经“数牌语言”**Minimal Perfect Hash (MPHF)** / LOUDS 压缩 | Core i9-12900 / GCC 下约 **40 ns / hand**；表体积压至 KB–低 MB 级 | 算法理论标杆；Rust 项目优先用原生 Rust 的 `xiangting` 或参考其 MPHF 表构建思路，避免引入 C++ FFI 复杂性 |
| **`Apricot-S/shanten-algorithm-collection`** | Rust Workspace | 开源基准集 | 统一 `ShantenCalculator` 接口，集成 `decomp_ara`、`decomp_kobayashi`、`sac_*` 等适配器 | 含 `normal`/`half_flush`/`full_flush`/`thirteen_orphans` 各 10,000 手基准 | **绝佳的内部单元测试与性能基准基线**；验证自研或封装向听模块在“4 副露无雀头”等边界手牌下零 Bug |
| **`agari`** (`agari-industries/agari`) | Rust (`v0.27.0`) | **MIT** | `[u8; 34]` + `TileCounts`、标准/七对/国士向听及含副露 `calculate_ukeire_with_visible` | 带 `clap`/`serde` 等常规依赖，偏全套日麻工具库 | **⚠️ 注意**：其日麻役种/符数计分耦合较重，推倒胡不可直接用其计分，仅可借鉴 `shanten`/`ukeire` API 设计 |
| **`riichi-decomp` / `riichi-decomp-table`** (`summivox/riichi-rs`) | Rust (`v0.1.0`) | **MIT**（注：同仓主引擎 `riichi` 为 **LGPL-3.0**） | 27-bit 八进制压缩单花色直方图（`C-Table` $3N+2$ 与 `W-Table` $3N+1$），`nanovec` 静态 LUT | `build.rs` `static-lut` 生成，听牌分解极快 | **⚠️ 许可证注意**：子 crate 为 MIT，但主引擎 `riichi` 是 **LGPL-3.0**！其 27-bit 八进制打包技巧值得借鉴 |
| **`Equim-chan/Mortal` (`libriichi`)** | Rust + Python | **AGPL-3.0** | 含 `shanten.rs`、SIMD 优化的单人 DP `sp/calc.rs`（`SPCalculator` 计算 `win_probs`/`tenpai_probs`/`exp_values`） | 极高吞吐与向量化优化 | **⚠️ 强传染性许可证**：**AGPL-3.0**，严禁直接复制代码到闭源/非 AGPL 项目；可阅读其 `algo/sp` 数学思路（参考 `tomohxx/mahjong-win-prob`）独立实现 |
| **`smly/RiichiEnv`** (`riichienv-core`) | Rust (`v0.4.11`) | **Apache-2.0** (+ MIT 表) | 紧凑 `u8`/`u32` 状态与动作空间、MJAI 协议、集成 `nyanten` 思路表 | 专为高速自博弈 Rollout 设计 | 非常值得参考其零分配状态机（State Transition）与 MJAI 协议事件流设计 |
| **`riichi-tools-rs`** (`harphield`) | Rust (`v0.0.70`) | **MIT** | 双模式：无表轻量版 + `"fast_shanten"`（移植自 `Spines.Mahjong`） | 适合 WASM / 轻量化对比测试 | 可作轻量对比，核心算力不如 `xiangting` / `shanten-dp` 专注 |
| **`critter-mj/akochan`** | C++11 | **自定义限制性许可**（非商用/禁止未经许可修改再分发） | 单人期望、Expectimax 风格评估、统计危险度表与手调 `params` | 需 Boost + OpenMP，天凤高段非 NN 标杆 | **⚠️ 法律避坑**：**不可直接商用或二次分发改版代码**；仅限学术研究其“无深度网络也能打到凤凰桌”的参数化特征分解框架 |

---

## 6. 关键工程实现细节与避坑清单

1. **四张同牌七对子（豪华七对 / 龙七对）的向听与胡牌陷阱**：
   - 多数日麻 crate（如默认日麻七对逻辑）将同种牌 4 张只算作 1 个对子（剩余 2 张作废），导致手牌有 `1111m` 时算出的七对子向听数比中国推倒胡规则（允许 4 张拆成两对）多 1 向听。若目标规则允许龙七对，务必选用或配置支持四枚对子的计算路径（如 `shanten-dp` 原生支持选项）。
2. **并发确定化采样时的状态深拷贝（Clone）开销**：
   - 在 Rust 中执行 $K=64$ 次扁平蒙特卡洛或浅层 Rollout 时，切忌把 `Vec<Tile>`、`String` 或复杂事件历史全部放进 `SimulationState`。
   - **最佳实践**：将模拟态精简为一个完全可 `Copy` 的栈上结构体（总大小控制在一组固定数组内，含 `hands: [[u8; 34]; 4]`、`melds_count: [u8; 4]`、`wall_counts: [u8; 34]`、`remaining_draws: u8`、`turn: u8`），结合无堆分配 PRNG 与线程复用缓冲区，以满足本地 50–200 ms 预算内执行数百至数千次浅层 Rollout 的性能目标（具体吞吐需在目标硬件上以 `criterion` 实测）。
3. **局域网对局的“压迫感”与随机性控制**：
   - 家庭局域网 AI 如果在 3 家完全同参数下死板出牌，容易出现“三家同时在相同巡目齐刷刷缩进防守”的机械感，或全员盲目快速屁胡。
   - **策略性格参数化（Personality Weights）**：在 L3/L4 的推倒胡目标函数 $\text{Utility} = w_{\text{speed}} P(\text{win}) + w_{\text{value}} \mathbb{E}[\text{Score}] - w_{\text{def}} \text{Risk}$ 上，为 3 个 AI 分配微调权重（如稳健防守型、快攻抢听型、做大牌型），既保持强胜率又不至于行为同质化；同时在优势绝对明确时不添加无意义随机噪声，确保对局“稳准狠”。
4. **向后兼容的 Trait 规则抽象（为后续玩法留口子）**：
   - 将向听与胡牌检测抽象为 `RuleEngine` trait（配置 `has_furiten: bool`, `allow_chi: bool`, `allow_chicken_hand: bool`, `allow_four_same_chiitoi: bool`, `missing_suit_required: bool`, `yaku_multiplier_table`），使 L1–L4 搜索算法完全与特定地区推倒胡计分表解耦。
5. **许可证合规红线**：
   - 坚决避开 **AGPL-3.0**（`Mortal`/`libriichi`）、**LGPL-3.0**（`summivox/riichi`、`tomohxx/shanten-number`）以及 **限制性私有许可**（`critter-mj/akochan`）的源码直接搬运；核心基础设施优先选用 **MIT / Apache-2.0** 协议的 crates（如 `xiangting`、`shanten-dp`）并独立实现推倒胡规则与搜索管线。

---

## 7. 一手权威来源引用（Primary Sources）

### 学术论文与专著
1. **Cowling, P. I., Powley, E. J., & Whitehouse, D. (2012).** *Information Set Monte Carlo Tree Search.* IEEE Transactions on Computational Intelligence and AI in Games, 4(2), 120–143.
   - DOI: https://doi.org/10.1109/TCIAIG.2012.2200894
   - 机构公开版: https://eprints.whiterose.ac.uk/id/eprint/75048/
2. **Mizukami, N., & Tsuruoka, Y. (2015).** *Building a Computer Mahjong Player Based on Monte Carlo Simulation and Opponent Models.* IEEE Conference on Computational Intelligence and Games (CIG).
   - DOI: https://doi.org/10.1109/CIG.2015.7317929
3. **Kurita, M., & Hoki, K. (2021).** *Method for Constructing Artificial Intelligence Player With Abstractions to Markov Decision Processes in Multiplayer Game of Mahjong.* IEEE Transactions on Games, 13(1), 99–110.
   - DOI: https://doi.org/10.1109/TG.2020.3036471
   - arXiv 预印本: https://arxiv.org/abs/1904.07491
4. **Li, J., Koyamada, S., Ye, Q., Liu, G., Wang, C., Yang, R., Zhao, L., Qin, T., Liu, T.-Y., & Hon, H.-W. (2020).** *Suphx: Mastering Mahjong with Deep Reinforcement Learning.*
   - arXiv: https://arxiv.org/abs/2003.13590
5. **Chen, J.-C., Tang, S.-C., & Wu, I.-C. (2022).** *Monte-Carlo Simulation for Mahjong* (SIMCAT). Journal of Information Science and Engineering, 38(4), 775–790.
   - DOI: https://doi.org/10.6688/JISE.202207_38(4).0005
6. **Official International Mahjong / MCR Playground (2023).** *Official International Mahjong: A New Playground for AI Research.* Algorithms, 16(5), 239.
   - URL: https://www.mdpi.com/1999-4893/16/5/239
   - DOI: https://doi.org/10.3390/a16050239
7. **Gao, S., & Li, S. (2022).** *Bloody Mahjong playing strategy based on the integration of deep learning and XGBoost.* CAAI Transactions on Intelligence Technology.
   - DOI: https://doi.org/10.1049/cit2.12031
8. **Chiba, D.** *Riichi Book 1* (包含表 8.5 危险度分级与 `Genbutsu`/`Suji`/`Kabe`/`Tedashi` 统计防守理论).
   - 官方发布站: https://dainachiba.github.io/RiichiBooks/
   - GitHub 源码: https://github.com/dainachiba/RiichiBooks

### 官方开源仓库与 Crates / 文档
9. **Cryolite — `nyanten`**: https://github.com/Cryolite/nyanten
10. **Apricot-S — `xiangting` (Rust crate v6.0.2, MIT)**: https://github.com/Apricot-S/xiangting | https://crates.io/crates/xiangting | https://docs.rs/xiangting
11. **Apricot-S — `shanten-algorithm-collection`**: https://github.com/Apricot-S/shanten-algorithm-collection
12. **tomohxx — `shanten-dp-rust` (`shanten-dp` v0.3.2, MIT)**: https://github.com/tomohxx/shanten-dp-rust | https://crates.io/crates/shanten-dp | https://docs.rs/shanten-dp
13. **tomohxx — `shanten-number` (LGPL-3.0)** & **Algorithm Book**: https://github.com/tomohxx/shanten-number | https://tomohxx.github.io/mahjong-algorithm-book/
14. **tomohxx — `necessary-and-unnecessary-tiles`** & **`mahjong-win-prob`**: https://github.com/tomohxx/necessary-and-unnecessary-tiles | https://github.com/tomohxx/mahjong-win-prob
15. **agari-industries — `agari` (Rust crate v0.27.0, MIT)**: https://github.com/agari-industries/agari | https://crates.io/crates/agari | https://docs.rs/agari
16. **summivox — `riichi-rs` (`riichi` v0.1.0 LGPL-3.0; `riichi-decomp` / `riichi-decomp-table` MIT)**: https://github.com/summivox/riichi-rs | https://crates.io/crates/riichi-decomp
17. **smly — `RiichiEnv` (`riichienv-core` v0.4.11, Apache-2.0)**: https://github.com/smly/RiichiEnv | https://crates.io/crates/riichienv-core
18. **harphield — `riichi-tools-rs` (`riichi_tools_rs` v0.0.70, MIT)**: https://github.com/harphield/riichi-tools-rs
19. **Equim-chan — `Mortal` & `libriichi` (AGPL-3.0)**: https://github.com/Equim-chan/Mortal
20. **critter-mj — `akochan` (自定义限制性许可)**: https://github.com/critter-mj/akochan
