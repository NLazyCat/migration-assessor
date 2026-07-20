# 差异分析升级技术计划

## 目标
在现有 `diff` 命令基础上，增加四个维度的精细化分析能力：
1. **变量/函数重命名检测**
2. **常量数值变化检测**
3. **函数逻辑变化分析**
4. **新增文件的依赖替换分析**

---

## 当前架构回顾

`diff` 核心流程（`migration-analyze/src/commands/diff.rs`）：

```
fetch_diff → parse_file_diffs → analyze_file_changes → propagate_changes → output report
```

`analyze_file_changes` 现有三阶段：
- **Pass 1**：基于旧版行号映射，标记被修改的已有符号
- **Pass 2**：扫描 `+` 行，检测新增符号（单行列匹配）
- **输出**：`SymbolChangeDetail { symbol, kind, change_type, full_body, context_before, context_after, position }`

`diff` 强依赖 `analyze` 输出的 symbol index（`report/symbols/*.json`）和 reverse index。

---

## 功能一：变量/函数重命名检测（Rename Detection）

### 问题
Git diff 把重命名展示为"删除旧定义 + 新增新定义"，当前代码会拆成两个独立变更（`removed` + `added`），用户无法感知这是同一个实体的重命名。

### 方案
在 `analyze_file_changes` 中增加 **Pass 3：Rename Detection**。

**收集阶段**：
- 遍历 diff_lines，提取被完全删除的符号定义（`-` 行中的 `function fn`, `class`, `const`, `let`, `var`, `struct`, `enum` 等）
- 提取同一 hunk 内新增的符号定义（`+` 行中同类声明）

**匹配规则**（需同时满足）：
1. 同文件、同一 `@@` hunk 内
2. 符号 kind 相同（如都是 `function`）
3. 行号距离 <= 10 行（阈值可调）
4. **函数体相似度 >= 70%**（用行集合 Jaccard 相似度，轻量计算）

**合并输出**：
匹配到的重命名，把两个变更合并为一个 `change_type = "renamed"`。

### 数据结构变更

```rust
struct SymbolChangeDetail {
    // 现有字段保留...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_name: Option<String>,          // 重命名前的名字
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_score: Option<f64>,     // 函数体相似度（如 0.85）
}
```

### 风险
- **误报**：两个无关函数"一删一增"碰巧相似。用 70% 阈值 + 行号距离限制可大幅降低。
- **TS/Rust 语法差异**：提取函数体起始位置需区分 `function foo()` / `fn foo()` / `const foo = () =>`。

---

## 功能二：常量数值变化检测（Value Change Detection）

### 问题
当前对 `const MAX_SIZE = 100` 改为 `const MAX_SIZE = 200` 只标记为 `modified`，不展示具体数值变化。这对迁移者很关键（配置值、阈值、魔法数字）。

### 方案
对 `change_type = "modified"` 且 `kind` 为 `const` / `variable` / `arrow_function` / `constant` 的符号，提取其赋值表达式的旧/新值。

**提取规则**：
- 扫描该符号所在范围内的所有 `-` 行和 `+` 行
- 正则匹配：`^(const|let|var|static)\s+<symbol_name>\s*=\s*(.+?)(;|$)`
- 旧值 = 该符号首次出现在 `-` 行中的等号右侧内容
- 新值 = 该符号首次出现在 `+` 行中的等号右侧内容

**输出**：如果旧值和新值不同，附加 `value_change` 信息。

### 数据结构变更

```rust
#[derive(Debug, Clone, Serialize)]
struct ValueChange {
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub change_type: String, // "literal_changed" | "expression_changed" | "type_changed"
}

struct SymbolChangeDetail {
    // 现有字段保留...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_change: Option<ValueChange>,
}
```

### 风险
- 多行表达式（如对象字面量、数组）提取不完整。可限制只展示前 80 字符，或标记为 `"expression_changed"`。
- Rust 的 `const` 和 `let` 语法差异需分别处理。

---

## 功能三：函数逻辑变化分析（Logic Change Analysis）

### 问题
当前只标记函数为 `"modified"`，不说明内部改了什么。用户需要知道：条件变了？循环变了？调用了新函数？返回值变了？

### 方案
对 `kind = "function" | "method" | "arrow_function"` 且 `change_type = "modified"` 的符号，对比其旧函数体（从 `-` 行重建）和新函数体（从 `+` 行重建），识别具体逻辑变更类别。

**重建函数体**：
- 旧函数体：收集该符号行号范围内的所有 `-` 行和上下文行（` `）
- 新函数体：收集该符号行号范围内的所有 `+` 行和上下文行（` `）

**关键词匹配规则**（轻量级，无需 AST）：

| 类别 | 检测方式 |
|------|---------|
| `condition_changed` | `if` / `while` / `match` 条件表达式所在行出现在 `-` 或 `+` 中 |
| `loop_changed` | `for` / `while` / `.forEach` / `.map` 所在行变化 |
| `call_added` | `+` 行中出现 `<identifier>(` 且该调用未在旧函数体中 |
| `call_removed` | `-` 行中出现 `<identifier>(` 且该调用未在新函数体中 |
| `return_changed` | `return` / `=>`（箭头函数返回值）所在行变化 |
| `exception_changed` | `try` / `catch` / `throw` / `?`（Rust）所在行变化 |

**去重策略**：同一类别在一个函数内只报告一次。

### 数据结构变更

```rust
#[derive(Debug, Clone, Serialize)]
struct LogicChange {
    pub category: String,      // "condition" | "loop" | "call" | "return" | "exception"
    pub description: String,   // 如 "if condition changed" | "3 new function calls"
    pub old_snippet: Option<String>,
    pub new_snippet: Option<String>,
}

struct SymbolChangeDetail {
    // 现有字段保留...
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub logic_changes: Vec<LogicChange>,
}
```

### 风险
- **误判**：字符串常量里包含 `if (` 会被误检。可增加简单的括号平衡检查。
- **粒度**：只是关键词级分析，无法区分"条件从 `a > 1` 改为 `a > 2`"和"条件从 `a > 1` 改为 `b > 1`"。对迁移场景来说，知道"条件变了"已经足够指导人工 review。

---

## 功能四：新增文件的依赖替换分析（Dependency Replacement）

### 问题
新增文件（status == `"A"`）当前只报告符号列表，不做依赖分析。用户不知道这个文件引入了哪些外部库，迁移到 Rust 时该用什么替换。

### 方案
对新增文件做两步分析：

**Step 1：提取文件引入的外部依赖**
- 用 `reconstruct_full_file` 重建新增文件的完整内容
- 正则提取 import/require/use 语句：
  - TS: `import { ... } from 'package-name'` / `require('package-name')` / `import * as x from "package-name"`
  - Rust: `use crate::...` / `use std::...` / `extern crate ...`
- 收集唯一包名列表

**Step 2：查兼容性矩阵给出替换建议**
- 复用 `core/src/compatibility.rs` 的 `CompatibilityMatrix`
- 对每个外部包查 `matrix.lookup(package)`
- 输出等价库、兼容性等级、迁移 effort、指导说明

**Step 3：风险标记**
- 如果新增文件引入了 `compatibility == None` 或 `Unknown` 的依赖，在报告中标记为 `"high_risk_addition"`

### 数据结构变更

```rust
#[derive(Debug, Clone, Serialize)]
struct NewFileDepItem {
    pub package: String,
    pub equivalent: Option<String>,
    pub compatibility: String,
    pub effort: String,
    pub guidance: Option<String>,
    pub is_high_risk: bool,
}

#[derive(Debug, Clone, Serialize)]
struct NewFileDeps {
    pub file: String,
    pub imported_packages: Vec<String>,
    pub recommendations: Vec<NewFileDepItem>,
    pub high_risk_count: usize,
}

struct DiffReport {
    // 现有字段保留...
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub new_file_dependencies: Vec<NewFileDeps>,
}
```

### 兼容性矩阵数据来源
`diff` 当前不加载 compatibility matrix。需要：
- 在 `diff.rs` 的 `run` 函数中，根据 `config.project.source_lang` 和 `target_lang` 构造 `CompatibilityMatrix`
- 或者从 `report/external/compatibility.json` 中读取已计算好的结果（更轻量）

**推荐后者**：`analyze` 已经算好了兼容性矩阵，直接读 JSON 即可，避免 `diff` 重复构造。

### 风险
- 正则提取 import 可能漏掉动态 import (`await import(...)`) 和路径别名 (`@/utils`)。路径别名可标记为 `"local"` 忽略。
- 如果 `analyze` 没跑过，`external/compatibility.json` 不存在，此功能降级为只展示原始 import 列表。

---

## 报告格式总览（升级后）

```json
{
  "generated_at": "2026-07-20T10:00:00Z",
  "from_version": "v1.0.0",
  "to_version": "v1.1.0",
  "files": ["src/utils.ts", "src/config.ts"],
  "file_changes": [
    {
      "file": "src/config.ts",
      "changes": [
        {
          "symbol": "MAX_RETRY",
          "kind": "const",
          "change_type": "modified",
          "value_change": {
            "old_value": "3",
            "new_value": "5",
            "change_type": "literal_changed"
          }
        },
        {
          "symbol": "fetchData",
          "kind": "function",
          "change_type": "renamed",
          "old_name": "fetchDataLegacy",
          "similarity_score": 0.82,
          "logic_changes": [
            { "category": "condition", "description": "if condition changed" },
            { "category": "call", "description": "2 new function calls" }
          ]
        }
      ]
    }
  ],
  "new_file_dependencies": [
    {
      "file": "src/new-api.ts",
      "imported_packages": ["axios", "lodash"],
      "recommendations": [
        {
          "package": "axios",
          "equivalent": "reqwest",
          "compatibility": "partial",
          "effort": "moderate",
          "guidance": "Replace axios calls with reqwest",
          "is_high_risk": false
        }
      ],
      "high_risk_count": 0
    }
  ],
  "propagation": { ... }
}
```

---

## 实施步骤与优先级

| 优先级 | 功能 | 预估改动量 | 依赖 |
|--------|------|-----------|------|
| P0 | 常量数值变化 | 小（在现有 Pass 1/2 后加正则提取） | 无 |
| P0 | 新增文件依赖分析 | 中（需读 compatibility JSON + 正则提取 import） | report/external/compatibility.json |
| P1 | 函数逻辑变化 | 中（需重建函数体并做关键词 diff） | 无 |
| P1 | 重命名检测 | 大（需收集删除/新增对 + 相似度计算 + 合并逻辑） | 无 |

### 建议实施顺序
1. **先扩展数据结构**：给 `SymbolChangeDetail` 和 `DiffReport` 加新字段（向后兼容，`serde` 默认忽略未知字段）
2. **实现数值变化**：最简单，验证流程打通
3. **实现新增文件依赖分析**：可以复用现有 `compatibility.rs` 能力
4. **实现函数逻辑变化**：在已有 `extract_full_body` 基础上扩展
5. **最后实现重命名检测**：逻辑最复杂，需要充分测试

---

## 需要修改的文件清单

1. **`migration-analyze/src/commands/diff.rs`**
   - 扩展 `SymbolChangeDetail`、`DiffReport` 结构
   - 新增 `ValueChange`、`LogicChange`、`NewFileDeps` 等结构
   - 在 `analyze_file_changes` 中插入 Pass 3（rename）和数值/逻辑分析
   - 新增 `extract_imports_from_source`、`analyze_new_file_deps` 函数
   - 修改 `run` 函数末尾组装报告的逻辑

2. **`migration-analyze/src/commands/context.rs`**（可选）
   - 如选择从 report 读 compatibility 而非重新构造 matrix，加 `load_compatibility()` 方法

3. **`core/src/output_paths.rs`**（可选）
   - 如需新增 diff 输出路径常量

4. **测试文件**
   - `migration-analyze/tests/e2e_diff.rs`：补充重命名、数值变化、逻辑变化的端到端断言

---

## 兼容性说明

- 所有新增字段都使用 `#[serde(skip_serializing_if = ...)]` 和 `#[serde(default)]`，确保旧版报告消费者不会解析失败。
- `change_type` 现有枚举（`added`/`removed`/`modified`）保持不变，新增 `"renamed"` 值。
- 如果用户没有重新 `analyze`（缺少 symbol index / compatibility JSON），新增功能 gracefully 降级为现有行为。

---

*计划完成。请评审后确认是否按此方案实施。*
