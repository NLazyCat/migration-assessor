# AST 级差异分析升级计划（专家评审版）

## 1. 总体架构

核心范式转变：从**文本行级 diff** 升级到 **AST 结构级 diff**。

对于每个变更文件：
1. 获取旧版本完整内容（本地缓存或 `git show`）
2. 获取新版本完整内容（`git show <new-rev>:<path>`）
3. 分别提取 AST（复用 `SymbolExtractor`）
4. 执行结构对比，输出语义级变更记录

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│ 旧版本文件内容   │────→│ 旧 AST       │     │                 │
│ (git show/base) │     │ SymbolIndex  │────→│                 │
└─────────────────┘     └──────────────┘     │   AST Diff Engine│────→ 结构化变更报告
┌─────────────────┐     ┌──────────────┐     │                 │
│ 新版本文件内容   │────→│ 新 AST       │────→│                 │
│ (git show/head) │     │ SymbolIndex  │     └─────────────────┘
└─────────────────┘     └──────────────┘
```

---

## 2. 变更类型完整分类体系

### 2.1 符号级变更 (Symbol-level)

| 变更类型 | 检测方式 | 迁移影响 |
|---------|---------|---------|
| `symbol_added` | 新 AST 中存在，旧 AST 中不存在 | 需要评估新增符号的迁移成本 |
| `symbol_removed` | 旧 AST 中存在，新 AST 中不存在 | 检查是否有反向引用被断链 |
| `symbol_renamed` | 旧符号消失 + 新符号出现 + 结构相似度 > 阈值 | 低影响（通常是重构） |
| `visibility_changed` | `export`/`pub` 修饰符增减 | 可能影响模块边界 |
| `location_moved` | 同文件内行号大幅偏移 | 通常是重构，低影响 |

**重命名检测算法（AST 级）**：
```
对于旧 AST 中每个消失的符号 S_old：
  对于新 AST 中每个新增的符号 S_new：
    如果 S_old.kind == S_new.kind：
      similarity = 计算结构相似度(S_old, S_new)
      // 结构相似度 = children kind 序列的 LCS 相似度 × 0.6 + 嵌套深度相似度 × 0.2 + 行数比例 × 0.2
      如果 similarity >= 0.75：
        输出 rename: S_old.name → S_new.name
        标记 confidence = similarity
```

> **优势**：文本 diff 做重命名检测时，函数体内递归调用自身会导致文本相似度骤降（旧名全被替换成新名）。AST 级的 children 序列不受名称变化影响，准确度远高于文本相似度。

### 2.2 签名级变更 (Signature-level)

针对 `function` / `method` / `arrow_function` / `fn` 等可调用符号：

| 变更类型 | 检测方式 | 破坏性 |
|---------|---------|-------|
| `parameter_added` | 参数列表长度增加 | ⚠️ Breaking |
| `parameter_removed` | 参数列表长度减少 | ⚠️ Breaking |
| `parameter_type_changed` | 同位置参数类型变化 | ⚠️ Breaking |
| `parameter_default_changed` | 默认值出现/消失/变化 | ⚠️ 可能 Breaking |
| `return_type_changed` | 返回类型变化 | ⚠️ Breaking |
| `generic_signature_changed` | 泛型参数增减或约束变化 | ⚠️ Breaking |
| `async_changed` | `async` 修饰符增减 | ⚠️ Breaking |

> **注**：签名级变更可直接标记 `severity: Breaking`，因为这些变更意味着所有调用方都需要修改。

### 2.3 数值与状态变更 (Value-level)

针对 `const` / `let` / `var` / `static`：

| 变更类型 | 检测方式 | 示例 |
|---------|---------|------|
| `literal_value_changed` | 初始值从字面量 A 变为字面量 B | `const MAX = 100` → `const MAX = 200` |
| `expression_changed` | 初始值从字面量变为表达式 | `const MAX = 100` → `const MAX = compute()` |
| `type_annotation_changed` | 类型标注变化 | `let x: number` → `let x: string` |
| `mutability_changed` | `const` ↔ `let` / `mut` 变化 | `const` → `let` |

### 2.4 逻辑级变更 (Logic-level)

基于函数体 AST 的语句级对比：

| 变更类型 | 检测方式 | 粒度 |
|---------|---------|------|
| `condition_changed` | `IfStmt` / `WhileStmt` / `MatchExpr` 的 condition 子树结构变化 | 语句级 |
| `loop_structure_changed` | `ForStmt` ↔ `WhileStmt` 转换，或循环条件变化 | 语句级 |
| `call_added` | 函数体内 `CallExpr` 节点集合的差分（新 - 旧） | 表达式级 |
| `call_removed` | 函数体内 `CallExpr` 节点集合的差分（旧 - 新） | 表达式级 |
| `return_behavior_changed` | `ReturnStmt` 数量变化，或返回值表达式变化 | 语句级 |
| `error_handling_changed` | `TryStmt` / `CatchClause` / `ThrowStmt` / `?` 操作符出现/消失 | 语句级 |
| `variable_assignment_changed` | 局部变量赋值表达式变化 | 表达式级 |
| `early_exit_added` | 新增 `return` / `break` / `continue` / `?` 提前退出点 | 语句级 |

**调用变化检测的精确做法**：
- 遍历函数体的 AST，收集所有 `CallExpr` 节点
- 对每个 `CallExpr`，提取被调用者的标识符（如 `axios.get`、`this.foo`）
- 旧函数体调用集合 vs 新函数体调用集合 → 集合差分
- 输出 `call_added: ["axios.post", "validateInput"]`、`call_removed: ["oldHelper"]`

### 2.5 结构级变更 (Structure-level)

针对 `class` / `struct` / `interface` / `trait` / `enum`：

| 变更类型 | 检测方式 |
|---------|---------|
| `member_added` | children 中新增成员符号 |
| `member_removed` | children 中消失的成员符号 |
| `member_renamed` | 同 2.1 的重命名检测，限制在同一父符号的 children 内 |
| `inheritance_changed` | `extends` / `implements` / `trait` 列表变化 |
| `visibility_changed` | `public` / `private` / `protected` / `pub` 变化 |

### 2.6 依赖级变更 (Dependency-level)

**文件级 import 变化**：
- 对比新旧 AST 中所有 `ImportDecl` / `UseStmt` / `ExternCrate` 节点
- 检测：新增 import、删除 import、import 路径变化（如 `'axios'` → `'fetch'`）

**项目级 package 变化**：
- 如果 `package.json` / `Cargo.toml` 在 diff 中，解析新旧依赖列表
- 对比版本变化，输出 `upgraded` / `downgraded` / `added` / `removed`
- 查 compatibility matrix，评估对迁移的影响

### 2.7 文档级变更 (Documentation-level)

从 AST 中提取注释节点并对比：

| 变更类型 | 检测方式 | 迁移意义 |
|---------|---------|---------|
| `doc_added` | 新增文档注释块 | 通常是 API 稳定化信号 |
| `doc_removed` | 文档注释被删除 | 可能是 API 废弃前兆 |
| `doc_changed` | 文档内容变化 | 可能包含行为变更说明 |
| `deprecated_added` | 新增 `@deprecated` / `#[deprecated]` | ⚠️ 重要：该符号即将废弃 |
| `todo_added` | 新增 `TODO` / `FIXME` / `HACK` | 代码未完成，迁移时需特别注意 |
| `safety_doc_added` | 新增 `SAFETY` / `unsafe` 说明 | Rust 迁移中需评估 unsafe 边界 |

> **文档变更对迁移的意义**：原项目维护者通常会在文档中说明行为变更。`@deprecated` 的新增意味着该符号可能在后续版本中被移除，迁移时应避免使用或寻找替代方案。

---

## 3. AST Diff Engine 设计

### 3.1 核心对比算法

```rust
/// 对比两个文件的 AST，输出结构化差异
fn diff_symbol_indexes(old: &SymbolIndex, new: &SymbolIndex) -> Vec<SymbolChange> {
    let mut changes = Vec::new();
    
    // Step 1: 建立名称映射
    let old_by_name: HashMap<(String, String), &Symbol> = // (name, kind) → symbol
    let new_by_name: HashMap<(String, String), &Symbol> = 
    
    // Step 2: 检测 added / removed / stable
    let old_keys: HashSet<_> = old_by_name.keys().collect();
    let new_keys: HashSet<_> = new_by_name.keys().collect();
    
    let added = &new_keys - &old_keys;
    let removed = &old_keys - &new_keys;
    let stable = &old_keys & &new_keys;
    
    // Step 3: 对 removed + added 做重命名检测
    let renames = detect_renames(&removed, &added, &old_by_name, &new_by_name);
    
    // Step 4: 对 stable 符号做深度对比
    for key in stable {
        let old_sym = old_by_name[key];
        let new_sym = new_by_name[key];
        if let Some(change) = compare_symbol_deep(old_sym, new_sym) {
            changes.push(change);
        }
    }
    
    // Step 5: 对未匹配上的 added / removed 输出
    // ...
    
    changes
}
```

### 3.2 深度符号对比

```rust
fn compare_symbol_deep(old: &Symbol, new: &Symbol) -> Option<SymbolChange> {
    let mut details = Vec::new();
    
    // 1. 对比 visibility（如果 AST 能提取到）
    // 2. 对比 children（递归）
    // 3. 根据 kind 做特定对比
    match old.kind.as_str() {
        "function" | "fn" | "method" | "arrow_function" => {
            details.extend(compare_function_signature(old, new));
            details.extend(compare_function_body(old, new));
        }
        "const" | "let" | "var" | "static" => {
            details.extend(compare_value(old, new));
        }
        "class" | "struct" | "interface" | "trait" | "enum" => {
            details.extend(compare_structure(old, new));
        }
        _ => {}
    }
    
    if details.is_empty() {
        None
    } else {
        Some(SymbolChange {
            symbol: new.name.clone(),
            kind: new.kind.clone(),
            change_type: "modified",
            details,
            ..Default::default()
        })
    }
}
```

### 3.3 结构相似度计算（用于重命名检测）

```rust
fn structural_similarity(old: &Symbol, new: &Symbol) -> f64 {
    // 1. children kind 序列的 LCS 相似度
    let old_kinds: Vec<_> = old.children.iter().map(|c| c.kind.as_str()).collect();
    let new_kinds: Vec<_> = new.children.iter().map(|c| c.kind.as_str()).collect();
    let lcs_sim = lcs_similarity(&old_kinds, &new_kinds);
    
    // 2. 行数比例
    let old_lines = old.line_range[1] - old.line_range[0];
    let new_lines = new.line_range[1] - new.line_range[0];
    let line_sim = if old_lines.max(new_lines) == 0 {
        1.0
    } else {
        (old_lines.min(new_lines) as f64) / (old_lines.max(new_lines) as f64)
    };
    
    // 3. children 数量比例
    let child_sim = if old.children.is_empty() && new.children.is_empty() {
        1.0
    } else {
        (old.children.len().min(new.children.len()) as f64) 
            / (old.children.len().max(new.children.len()) as f64)
    };
    
    lcs_sim * 0.5 + line_sim * 0.3 + child_sim * 0.2
}
```

---

## 4. 数据模型设计

### 4.1 输出报告结构

```rust
#[derive(Debug, Clone, Serialize)]
struct AstDiffReport {
    pub generated_at: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub summary: DiffSummary,
    pub file_changes: Vec<FileAstChanges>,
    pub dependency_changes: Vec<DependencyChange>,
    pub propagation: PropagationResult,
}

#[derive(Debug, Clone, Serialize)]
struct DiffSummary {
    pub total_files_changed: usize,
    pub symbols_added: usize,
    pub symbols_removed: usize,
    pub symbols_renamed: usize,
    pub symbols_modified: usize,
    pub breaking_changes: usize,
    pub new_dependencies: usize,
    pub removed_dependencies: usize,
}

#[derive(Debug, Clone, Serialize)]
struct FileAstChanges {
    pub file: String,
    pub status: String, // "A" | "M" | "D" | "R"
    pub symbol_changes: Vec<SymbolChange>,
    pub import_changes: Vec<ImportChange>,
    pub doc_changes: Vec<DocChange>,
}

#[derive(Debug, Clone, Serialize)]
struct SymbolChange {
    pub symbol: String,
    pub kind: String,
    pub change_type: String, // "added" | "removed" | "renamed" | "modified"
    pub severity: String,    // "breaking" | "compatible" | "cosmetic" | "unknown"
    pub old_name: Option<String>, // 仅 rename
    pub rename_confidence: Option<f64>, // 仅 rename
    pub details: Vec<ChangeDetail>,
    pub old_line_range: Option<[usize; 2]>,
    pub new_line_range: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize)]
struct ChangeDetail {
    pub aspect: String,      // "signature" | "value" | "body" | "visibility" | "logic" | "structure" | "documentation"
    pub change_type: String, // "added" | "removed" | "changed"
    pub description: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub migration_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ImportChange {
    pub change_type: String, // "added" | "removed" | "path_changed"
    pub package: String,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub is_external: bool,
    pub compatibility: Option<CompatibilityInfo>, // 查 matrix
}

#[derive(Debug, Clone, Serialize)]
struct DocChange {
    pub change_type: String, // "added" | "removed" | "content_changed"
    pub symbol: String,
    pub is_deprecated: bool,
    pub has_todo: bool,
    pub has_safety_note: bool,
    pub old_doc: Option<String>,
    pub new_doc: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DependencyChange {
    pub package: String,
    pub change_type: String, // "added" | "removed" | "upgraded" | "downgraded"
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub compatibility: CompatibilityInfo,
    pub affected_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CompatibilityInfo {
    pub equivalent: Option<String>,
    pub compatibility: String,
    pub effort: String,
    pub guidance: Option<String>,
    pub is_high_risk: bool,
}
```

---

## 5. 关键实现问题

### 5.1 如何获取旧版本文件内容

当前 `analyze` 只保存了 symbol index JSON，没有保存原始文件内容。获取旧版本内容有两个方案：

**方案 A：从本地 git 提取（推荐）**
```bash
git show <from-version>:<file-path>
```
- 优点：零额外存储，100% 准确
- 前提：用户本地有 git 仓库，且 `<from-version>` 在本地历史中

**方案 B：缓存原始文件内容**
- 在 `analyze` 时把源文件内容复制到 `{repo}-migration/sources/` 下
- `diff` 时直接读取缓存
- 优点：不依赖 git 历史
- 缺点：额外存储，大仓库成本高

**推荐方案 A**，因为 `migration.toml` 中已经记录了 `source_version`，且 `diff` 本来就依赖 git 做 diff。

### 5.2 如何获取新版本文件内容

```bash
# 本地 git
git show <to-version>:<file-path>

# 远程（已有 fetch_remote_diff 的临时目录复用）
# 在临时目录中 git show 即可
```

### 5.3 AST 提取性能

每个变更文件需要跑两次 `SymbolExtractor`（旧 + 新）。假设一次 diff 涉及 50 个文件：
- 100 次 AST 提取
- 每次提取一个文件约 1-10ms（视文件大小）
- 总计 < 1s，完全可接受

可以用 `rayon` 并行处理不同文件。

### 5.4 现有 Symbol 结构是否足够

当前 `Symbol` 结构：
```rust
struct Symbol {
    id, name, kind, line_range, children, partial_analysis, partial_reason
}
```

**不足**：缺少以下信息，需要扩展：
- `visibility`：`pub` / `export` / `private`
- `signature`：函数的参数列表、返回类型（目前只在 `ApiContract` 中，不在 `Symbol` 中）
- `value`：常量的初始值表达式
- `docs`：文档注释

**建议**：扩展 `Symbol` 结构，或让 `diff` 同时读取 `symbols.json` 和 `contracts.json`（后者已有签名信息）。

> **需要确认**：是直接扩展 `core/src/symbols/mod.rs` 中的 `Symbol`，还是在 diff 层单独做更细粒度的解析？

---

## 6. 实施步骤

| 阶段 | 任务 | 产出 | 依赖 |
|------|------|------|------|
| Phase 0 | 扩展 AST 输出：给 `Symbol` / `ApiContract` 增加 `visibility`、`docs`、`value_initializer` 字段 | core 改动 | 无 |
| Phase 1 | 实现 `git show` 文件内容获取模块 | `diff.rs` 新增函数 | 无 |
| Phase 2 | 实现 AST Diff Engine：符号映射、重命名检测、深度对比 | 新增 `diff_engine.rs` | Phase 0, 1 |
| Phase 3 | 实现各维度变更检测器：签名、数值、逻辑、结构、文档 | `diff_engine.rs` 扩展 | Phase 2 |
| Phase 4 | 实现依赖变更检测：import 对比 + package.json 对比 | `diff.rs` 扩展 | Phase 1 |
| Phase 5 | 整合报告输出：组装 `AstDiffReport` | `diff.rs` 输出 | Phase 2-4 |
| Phase 6 | 端到端测试 | `e2e_diff.rs` | Phase 5 |

---

## 7. 待确认的技术决策

1. **Symbol 结构扩展 vs 独立解析**
   - 选项 A：扩展 `core/src/symbols/mod.rs` 的 `Symbol`，增加 `visibility`、`docs` 等字段
   - 选项 B：在 `diff` 层单独做更细粒度解析（不改动 core）
   - 建议 A，因为这些信息对 `analyze` 本身也有价值

2. **旧版本内容获取策略**
   - 选项 A：`git show <source_version>:<path>`（依赖 git）
   - 选项 B：`analyze` 时缓存源文件到 migration 目录
   - 建议 A，简单可靠

3. **重命名检测阈值**
   - 结构相似度阈值建议 0.75，是否需要可配置？

4. **文档注释提取**
   - 当前 parser（oxc / syn）是否已提取注释？如果未提取，需要扩展 parser 还是文本级提取？

---

*计划完成，等待专家意见确认后进入实施阶段。*
