# AST 级差异分析重构计划（从零设计版）

## 0. 现状诊断

分析了现有代码后，发现以下结构性缺陷：

| 问题 | 位置 | 影响 |
|------|------|------|
| `Symbol` 缺少 `visibility` | `core/src/symbols/mod.rs` | 无法检测 export/pub 变化 |
| `Symbol` 缺少 `value` | `core/src/symbols/mod.rs` | 无法检测常量值变化 |
| TS 符号提取器不提取注释 | `core/src/symbols/typescript.rs` | 无法检测文档变化 |
| Rust 只提取 public 符号 | `core/src/symbols/rust.rs` | 私有函数变化无法追踪 |
| diff 依赖文本行对比 | `migration-analyze/src/commands/diff.rs` | 重命名检测不准，误报多 |
| 无函数体逻辑分析 | 缺失 | 无法检测条件/循环/调用变化 |

---

## 1. 新架构设计

### 1.1 整体架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Core Library (migration-core)                        │
│                                                                         │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────────┐  │
│  │  Parser     │    │  Symbols    │    │         Diff Engine         │  │
│  │  (oxc/syn)  │───▶│  Extractor  │───▶│                             │  │
│  │             │    │  (enhanced) │    │  - SymbolMapping            │  │
│  └─────────────┘    └─────────────┘    │  - RenameDetection          │  │
│                                        │  - SignatureDiff            │  │
│                                        │  - ValueDiff                │  │
│                                        │  - LogicDiff                │  │
│                                        │  - DependencyDiff           │  │
│                                        │  - DocDiff                  │  │
│                                        └─────────────────────────────┘  │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                         Data Models                              │  │
│  │  Symbol (enhanced)  •  SymbolIndex  •  SymbolDiffResult          │  │
│  │  FileDiffResult     •  DiffReport   •  CompatibilityMatrix       │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    CLI (migration-analyze)                              │
│                                                                         │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────────────┐   │
│  │  analyze     │    │     diff     │    │      report              │   │
│  │  (全量分析)  │    │  (增量对比)   │    │  (格式化输出)             │   │
│  └──────────────┘    └──────────────┘    └──────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 模块划分

| 模块 | 职责 | 文件位置 |
|------|------|---------|
| `core/src/symbols/` | 增强的符号提取（visibility、value、docs） | 改造现有 |
| `core/src/diff/` | AST 级差异对比引擎（新增） | 新建 |
| `core/src/diff/mapping.rs` | 符号映射与重命名检测 | 新建 |
| `core/src/diff/signature.rs` | 签名级差异 | 新建 |
| `core/src/diff/logic.rs` | 函数体逻辑差异 | 新建 |
| `core/src/diff/dependency.rs` | 依赖变化检测 | 新建 |
| `core/src/diff/doc.rs` | 文档注释差异 | 新建 |
| `core/src/diff/mod.rs` | Diff Engine 入口 | 新建 |
| `migration-analyze/src/commands/diff.rs` | CLI 层改造 | 改造现有 |

---

## 2. 增强的 Symbol 结构

```rust
// core/src/symbols/mod.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub line_range: [usize; 2],
    pub children: Vec<Symbol>,
    pub partial_analysis: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_reason: Option<String>,
    
    // 新增字段（向后兼容）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub async: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<Param>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Default,
    Crate,
    Super,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}
```

**改造要点**：
- TS 提取器（`typescript.rs`）：用 `oxc_ast` 的 `ExportNamedDeclaration` 判断 visibility，提取 JSDoc 注释，提取变量初始值
- Rust 提取器（`rust.rs`）：保留 `is_public` 但也记录 visibility，提取 `#[deprecated]` 等属性

---

## 3. Diff Engine 核心设计

### 3.1 入口

```rust
// core/src/diff/mod.rs

pub mod mapping;
pub mod signature;
pub mod logic;
pub mod dependency;
pub mod doc;

use crate::symbols::{Symbol, SymbolIndex};

pub struct DiffEngine;

impl DiffEngine {
    pub fn diff_files(
        old_index: &SymbolIndex,
        new_index: &SymbolIndex,
    ) -> FileDiffResult {
        let mut result = FileDiffResult::default();
        
        let mapping = mapping::build_symbol_mapping(old_index, new_index);
        
        for (old_id, new_id) in &mapping.renamed {
            result.add_rename(old_id, new_id, mapping.confidence(old_id).unwrap_or(0.0));
        }
        
        for sym in &mapping.added {
            result.add_symbol_change(SymbolChange {
                symbol: sym.name.clone(),
                change_type: "added".to_string(),
                ..Default::default()
            });
        }
        
        for sym in &mapping.removed {
            result.add_symbol_change(SymbolChange {
                symbol: sym.name.clone(),
                change_type: "removed".to_string(),
                ..Default::default()
            });
        }
        
        for (old_sym, new_sym) in &mapping.stable {
            if let Some(changes) = Self::diff_symbol(old_sym, new_sym) {
                result.add_symbol_changes(changes);
            }
        }
        
        result
    }
    
    fn diff_symbol(old: &Symbol, new: &Symbol) -> Option<Vec<SymbolChange>> {
        let mut changes = Vec::new();
        
        // 签名差异
        if let Some(sig_diff) = signature::diff(old, new) {
            changes.extend(sig_diff);
        }
        
        // 数值差异
        if let Some(val_diff) = logic::diff_value(old, new) {
            changes.push(val_diff);
        }
        
        // 逻辑差异（函数体）
        if let Some(logic_diffs) = logic::diff_body(old, new) {
            changes.extend(logic_diffs);
        }
        
        // 文档差异
        if let Some(doc_diff) = doc::diff(old, new) {
            changes.push(doc_diff);
        }
        
        if changes.is_empty() {
            None
        } else {
            Some(changes)
        }
    }
}
```

### 3.2 符号映射与重命名检测

```rust
// core/src/diff/mapping.rs

pub struct SymbolMapping {
    pub renamed: HashMap<String, String>,  // old_id -> new_id
    pub added: Vec<&'a Symbol>,
    pub removed: Vec<&'a Symbol>,
    pub stable: Vec<(&'a Symbol, &'a Symbol)>,
    pub confidence: HashMap<String, f64>,
}

pub fn build_symbol_mapping(old: &SymbolIndex, new: &SymbolIndex) -> SymbolMapping {
    let old_by_name: HashMap<&str, &Symbol> = old.symbols.iter().map(|s| (s.name.as_str(), s)).collect();
    let new_by_name: HashMap<&str, &Symbol> = new.symbols.iter().map(|s| (s.name.as_str(), s)).collect();
    
    let mut renamed = HashMap::new();
    let mut confidence = HashMap::new();
    
    let removed_names: Vec<&str> = old.symbols.iter().filter(|s| !new_by_name.contains_key(s.name.as_str())).map(|s| s.name.as_str()).collect();
    let added_names: Vec<&str> = new.symbols.iter().filter(|s| !old_by_name.contains_key(s.name.as_str())).map(|s| s.name.as_str()).collect();
    
    for &old_name in &removed_names {
        let old_sym = old_by_name[old_name];
        for &new_name in &added_names {
            let new_sym = new_by_name[new_name];
            if old_sym.kind != new_sym.kind {
                continue;
            }
            
            let sim = structural_similarity(old_sym, new_sym);
            if sim >= 0.75 {
                renamed.insert(old_sym.id.clone(), new_sym.id.clone());
                confidence.insert(old_sym.id.clone(), sim);
            }
        }
    }
    
    SymbolMapping {
        renamed,
        added: new.symbols.iter().filter(|s| !old_by_name.contains_key(s.name.as_str()) && !renamed.values().any(|id| id == &s.id)).collect(),
        removed: old.symbols.iter().filter(|s| !new_by_name.contains_key(s.name.as_str()) && !renamed.contains_key(&s.id)).collect(),
        stable: old.symbols.iter().filter(|s| new_by_name.contains_key(s.name.as_str())).map(|s| (s, new_by_name[s.name.as_str()])).collect(),
        confidence,
    }
}

fn structural_similarity(old: &Symbol, new: &Symbol) -> f64 {
    let old_kinds: Vec<_> = old.children.iter().map(|c| c.kind.as_str()).collect();
    let new_kinds: Vec<_> = new.children.iter().map(|c| c.kind.as_str()).collect();
    
    let lcs_len = lcs(&old_kinds, &new_kinds);
    let lcs_sim = if old_kinds.len() + new_kinds.len() == 0 { 1.0 } else {
        2.0 * lcs_len as f64 / (old_kinds.len() + new_kinds.len()) as f64
    };
    
    let old_lines = old.line_range[1] - old.line_range[0];
    let new_lines = new.line_range[1] - new.line_range[0];
    let line_sim = if old_lines.max(new_lines) == 0 { 1.0 } else {
        (old_lines.min(new_lines) as f64) / (old_lines.max(new_lines) as f64)
    };
    
    let child_sim = if old.children.is_empty() && new.children.is_empty() { 1.0 } else {
        (old.children.len().min(new.children.len()) as f64) / (old.children.len().max(new.children.len()) as f64)
    };
    
    lcs_sim * 0.5 + line_sim * 0.3 + child_sim * 0.2
}

fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            if a[i-1] == b[j-1] {
                dp[i][j] = dp[i-1][j-1] + 1;
            } else {
                dp[i][j] = dp[i-1][j].max(dp[i][j-1]);
            }
        }
    }
    dp[a.len()][b.len()]
}
```

### 3.3 签名差异检测

```rust
// core/src/diff/signature.rs

pub fn diff(old: &Symbol, new: &Symbol) -> Option<Vec<SymbolChange>> {
    let mut changes = Vec::new();
    
    if let (Some(old_params), Some(new_params)) = (&old.params, &new.params) {
        let old_names: HashSet<&str> = old_params.iter().map(|p| p.name.as_str()).collect();
        let new_names: HashSet<&str> = new_params.iter().map(|p| p.name.as_str()).collect();
        
        for name in &old_names - &new_names {
            changes.push(SymbolChange::param_removed(name));
        }
        for name in &new_names - &old_names {
            changes.push(SymbolChange::param_added(name));
        }
        
        for (old_p, new_p) in old_params.iter().zip(new_params.iter()) {
            if old_p.ty != new_p.ty {
                changes.push(SymbolChange::param_type_changed(&old_p.name, &old_p.ty, &new_p.ty));
            }
            if old_p.optional != new_p.optional {
                changes.push(SymbolChange::param_optional_changed(&old_p.name, old_p.optional));
            }
        }
    }
    
    if old.return_type != new.return_type {
        changes.push(SymbolChange::return_type_changed(
            old.return_type.as_deref(),
            new.return_type.as_deref()
        ));
    }
    
    if old.async != new.async {
        changes.push(SymbolChange::async_changed(new.async.unwrap_or(false)));
    }
    
    if changes.is_empty() { None } else { Some(changes) }
}
```

### 3.4 函数体逻辑差异检测

这是最复杂的部分。需要直接操作 AST 节点：

```rust
// core/src/diff/logic.rs

pub fn diff_body(old: &Symbol, new: &Symbol) -> Option<Vec<SymbolChange>> {
    // 需要访问原始 AST，不能只依赖 Symbol 结构
    // 所以这里的设计需要把 AST 也传进来
    // 或者在 Symbol 中存储更多结构信息
    
    // 方案：在 diff 时，直接解析新旧文件的完整 AST，然后对比
    // 这意味着 diff_files 需要接收 source_code 而不仅仅是 SymbolIndex
    
    // 简化方案：在 Symbol 中存储函数体的"特征向量"——包含所有 CallExpr 的名称集合
    // 这样可以做调用增减检测
    
    let mut changes = Vec::new();
    
    // 如果 Symbol 中有调用特征，做集合差分
    // 否则，这个功能需要在更底层实现
    
    Some(changes)
}

pub fn diff_value(old: &Symbol, new: &Symbol) -> Option<SymbolChange> {
    if old.value != new.value && old.kind == "const" {
        Some(SymbolChange::value_changed(
            old.value.as_deref(),
            new.value.as_deref()
        ))
    } else {
        None
    }
}
```

### 3.5 依赖变化检测

```rust
// core/src/diff/dependency.rs

pub fn diff_imports(old: &ModuleReferences, new: &ModuleReferences) -> Vec<ImportChange> {
    let mut changes = Vec::new();
    
    let old_external: HashSet<&str> = old.external_imports.iter().map(|s| s.as_str()).collect();
    let new_external: HashSet<&str> = new.external_imports.iter().map(|s| s.as_str()).collect();
    
    for pkg in &new_external - &old_external {
        changes.push(ImportChange {
            change_type: "added".to_string(),
            package: pkg.to_string(),
            ..Default::default()
        });
    }
    
    for pkg in &old_external - &new_external {
        changes.push(ImportChange {
            change_type: "removed".to_string(),
            package: pkg.to_string(),
            ..Default::default()
        });
    }
    
    changes
}

pub fn diff_packages(
    old_deps: &[ResolvedDependency],
    new_deps: &[ResolvedDependency],
    matrix: &CompatibilityMatrix
) -> Vec<DependencyChange> {
    // 复用现有 compatibility.rs 的 detect_dep_changes
    // 但需要整合到 diff engine 中
}
```

### 3.6 文档变化检测

```rust
// core/src/diff/doc.rs

pub fn diff(old: &Symbol, new: &Symbol) -> Option<SymbolChange> {
    let old_deprecated = old.attributes.iter().any(|a| a == "#[deprecated]" || a == "@deprecated");
    let new_deprecated = new.attributes.iter().any(|a| a == "#[deprecated]" || a == "@deprecated");
    
    if old_deprecated != new_deprecated {
        if new_deprecated {
            return Some(SymbolChange::deprecated_added());
        }
    }
    
    if old.doc_comment != new.doc_comment {
        return Some(SymbolChange::doc_changed(
            old.doc_comment.as_deref(),
            new.doc_comment.as_deref()
        ));
    }
    
    None
}
```

---

## 4. 输出报告结构

```rust
// core/src/diff/mod.rs

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub generated_at: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub summary: DiffSummary,
    pub file_changes: Vec<FileDiffResult>,
    pub dependency_changes: Vec<DependencyChange>,
    pub propagation: PropagationResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSummary {
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
pub struct FileDiffResult {
    pub file: String,
    pub status: String,
    pub symbol_changes: Vec<SymbolChange>,
    pub import_changes: Vec<ImportChange>,
    pub doc_changes: Vec<DocChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolChange {
    pub symbol: String,
    pub kind: String,
    pub change_type: String,
    pub severity: String,
    pub old_name: Option<String>,
    pub rename_confidence: Option<f64>,
    pub details: Vec<ChangeDetail>,
    pub old_line_range: Option<[usize; 2]>,
    pub new_line_range: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeDetail {
    pub aspect: String,
    pub change_type: String,
    pub description: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub migration_note: Option<String>,
}
```

---

## 5. CLI 层改造

`diff.rs` 的改造路径：

```rust
// 旧流程（基于文本 diff）
fetch_diff() → parse_file_diffs() → analyze_file_changes() → propagate_changes()

// 新流程（基于 AST diff）
get_changed_files() → git_show_old_and_new() → extract_ast() → DiffEngine::diff_files() → propagate_changes()
```

**关键新增函数**：

```rust
// 从 git 获取指定版本的文件内容
fn get_file_at_version(repo_path: &Path, version: &str, file_path: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{}:{}", version, file_path)])
        .current_dir(repo_path)
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// 对变更文件做 AST diff
fn diff_single_file(
    old_source: &str,
    new_source: &str,
    file_path: &str,
    source_lang: SourceLanguage
) -> anyhow::Result<FileDiffResult> {
    let (old_index, _) = SymbolExtractor::extract(file_path, old_source, source_lang);
    let (new_index, _) = SymbolExtractor::extract(file_path, new_source, source_lang);
    
    Ok(DiffEngine::diff_files(&old_index, &new_index))
}
```

---

## 6. 实施路线图

### Phase 0: 基础设施扩展（1-2 天）
- 扩展 `Symbol` 结构（visibility、value、docs、attributes）
- 改造 TS 符号提取器：提取 JSDoc、visibility、变量初始值
- 改造 Rust 符号提取器：记录所有符号（含私有），提取属性

### Phase 1: Git 文件获取（0.5 天）
- 实现 `get_file_at_version()` 工具函数
- 复用现有 `fetch_remote_diff` 的临时目录逻辑

### Phase 2: SymbolMapping 与重命名检测（1 天）
- 实现 `core/src/diff/mapping.rs`
- LCS 相似度计算
- 重命名匹配算法

### Phase 3: 签名与数值差异（1 天）
- 实现 `core/src/diff/signature.rs`
- 实现 `core/src/diff/logic.rs`（value 部分）

### Phase 4: 函数体逻辑差异（2 天）
- 实现 `core/src/diff/logic.rs`（body 部分）
- 需要直接操作 oxc/syn AST 节点
- 调用变化检测（CallExpr 集合差分）

### Phase 5: 依赖与文档差异（0.5 天）
- 实现 `core/src/diff/dependency.rs`
- 实现 `core/src/diff/doc.rs`

### Phase 6: CLI 整合与测试（2 天）
- 改造 `migration-analyze/src/commands/diff.rs`
- 端到端测试

---

## 7. 关键技术决策

### 7.1 AST 存储策略

当前 `analyze` 只保存 `SymbolIndex` JSON，不保存原始 AST。有两个方案：

**方案 A：每次 diff 重新解析**（推荐）
- `diff` 时通过 `git show` 获取完整文件内容
- 直接调用 `SymbolExtractor::extract()` 提取 AST
- 优点：简单，无需修改 `analyze`
- 缺点：每次 diff 需要重新解析

**方案 B：analyze 时缓存 AST**
- `analyze` 时把 AST 序列化保存到 `report/ast/`
- `diff` 时直接读取缓存的 AST
- 优点：性能更好
- 缺点：需要修改 `analyze`，存储成本增加

**推荐方案 A**，因为：
1. 变更文件数量通常较少（几十个以内）
2. `SymbolExtractor` 已经是并行的
3. 不需要修改 `analyze` 的输出格式

### 7.2 函数体逻辑分析深度

**方案 A：基于 AST 节点类型统计**（推荐）
- 遍历函数体 AST，统计 `IfStmt`、`ForStmt`、`CallExpr` 等节点的数量和内容
- 做集合差分，输出"条件变化"、"循环变化"、"调用增减"
- 优点：实现相对简单，误报率低

**方案 B：基于控制流图（CFG）对比**
- 构建函数的 CFG，对比两个 CFG 的差异
- 优点：可以检测更复杂的控制流变化
- 缺点：实现非常复杂，oxc/syn 没有现成的 CFG 构建器

**推荐方案 A**，足够满足迁移场景需求。

### 7.3 私有符号是否追踪

当前 Rust 提取器只提取 public 符号。新系统应该：

**方案 A：提取所有符号**（推荐）
- 私有函数、私有变量也会发生变化，影响内部调用链
- 重命名检测需要对比所有符号，才能准确识别私有函数重命名
- 但在报告中标记 visibility，让用户知道哪些是对外 API

---

*计划完成。请确认后进入实施。*
