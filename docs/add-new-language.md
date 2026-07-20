# 新增语言支持指南

## 概述

新增一门语言（以 Python 为例），需要：

- **新建 5 个核心模块**（符号提取、解析器、引用提取、依赖解析、语言适配器）
- **新建 `compatibility_data` 和 `naming_data` 数据目录**
- **修改约 17 个 dispatch 文件**（enum match / extension match / validation）
- **约 300–800 行代码实现**

下面按修改位置分类列出。

---

## 1. 核心枚举与检测 —— `core/src/project.rs`

### 1a. `SourceLanguage` 枚举加新变体

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    TypeScript,
    Rust,
    JavaScript,
    Python,       // <-- 新增
}
```

### 1b. hint 字符串解析 + 自动检测

自动检测需要在 `Project::detect()` 中新增 manifest 文件检查：

```rust
let has_pyproject_toml = root.join("pyproject.toml").exists();
```

match 分支：
```rust
Some("python") | Some("py") => SourceLanguage::Python,
// ...
_ => match (has_package_json, has_cargo_toml, has_pyproject_toml) {
    (true, false, false) => SourceLanguage::TypeScript,
    (false, true, false) => SourceLanguage::Rust,
    (false, false, true) => SourceLanguage::Python,   // <-- 新增
    // ...
}
```

### 1c. `source_language_str()`

```rust
SourceLanguage::Python => "python",
```

---

## 2. 文件发现 —— `core/src/discovery.rs`

### `should_include()` 扩展名过滤

```rust
SourceLanguage::Python => {
    matches!(extension, Some("py"))
}
```

---

## 3. 符号提取 —— `core/src/symbols/`

### 3a. 新建 `core/src/symbols/python.rs`

```rust
pub fn extract(
    module: &str,
    source: &str,
) -> anyhow::Result<(SymbolIndex, ApiContract)> { ... }
```

—— 手动 AST 解析（不用编译器框架，Python 用 `rustpython-parser` 或手写字符串匹配提取 function/class/variable 声明）

### 3b. `core/src/symbols/mod.rs`

- 模块声明：`pub mod python;`
- `SymbolExtractor::extract_all()` match 新增分支：

```rust
SourceLanguage::Python => match python::extract(&module, &source) {
    Ok(r) => Some(r),
    Err(e) => { /* warning */ None }
},
```

- `extract_all_from_dir()` string→enum 新增：

```rust
"python" | "py" => SourceLanguage::Python,
```

---

## 4. 导入/引用解析 —— `core/src/parser/`

### 4a. 新建 `core/src/parser/python.rs`

```rust
pub fn parse_references(source: &str) -> anyhow::Result<ModuleReferences> { ... }
pub fn parse_import_bindings(source: &str) -> anyhow::Result<Vec<ImportBinding>> { ... }
```

—— 需要处理 `import foo`、`from foo import bar` 等语法

### 4b. `core/src/parser/mod.rs`

- 模块声明：`pub mod python;`
- `parse_file_references()` extension match 新增：

```rust
Some("py") => python::parse_references(source),
```

---

## 5. 跨文件引用 —— `core/src/references/`

### 5a. 新建 `core/src/references/python.rs`

```rust
pub fn extract_all(root: &Path, files: &[PathBuf]) -> anyhow::Result<(ForwardIndex, ReverseIndex)> { ... }
pub fn build_import_map(root: &Path, files: &[PathBuf]) -> ... { ... }
```

### 5b. `core/src/references/mod.rs`

- 模块声明：`pub mod python;`
- `parse_import_bindings()` extension match 新增：

```rust
"py" => python::parse_import_bindings(source),
```

---

## 6. 依赖解析 —— `core/src/deps/`

### 6a. 新建 `core/src/deps/python.rs`

```rust
pub fn resolve(root: &Path) -> anyhow::Result<Vec<ResolvedDependency>> { ... }
```

—— 读取 `pyproject.toml` / `requirements.txt` / `Pipfile.lock`

### 6b. `core/src/deps/mod.rs`

- 模块声明：`pub mod python;`
- `resolve_dependencies()` match 新增：

```rust
SourceLanguage::Python => python::resolve(root),
```

### 6c. `core/src/deps/module_map.rs`

`extract_external_specifiers()` match 新增：

```rust
SourceLanguage::Python => extract_python_external(source),
```

新增函数 `extract_python_external()`，处理 `import numpy`、`from django.db import models` 等语法。

---

## 7. 依赖图 —— `core/src/graph.rs`

### `resolve_relative_import()` match 新增：

```rust
SourceLanguage::Python => resolve_python_import(file, import, root),
```

新增函数 `resolve_python_import()`，处理 Python 的 `.` / `..` 相对导入路径解析。

---

## 8. 语言适配器 —— `core/src/language/`

### 8a. 新建 `core/src/language/python.rs`

实现 `Language` trait 和 `DiffAnalyzer` trait：

```rust
pub struct PythonLanguage;

impl Language for PythonLanguage {
    fn name(&self) -> &str { "python" }
    fn file_extensions(&self) -> &[&str] { &["py"] }
    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile<'_>> { ... }
    fn extract_symbols(&self, parsed: &ParsedFile) -> ... { ... }
    fn extract_references(&self, parsed: &ParsedFile) -> ... { ... }
    fn resolve_dependencies(&self, project_root: &Path) -> ... { ... }
    fn diff_analyzer(&self) -> &dyn DiffAnalyzer { &PythonDiffAnalyzer }
    fn detect_project_type(&self, project_root: &Path) -> bool {
        project_root.join("pyproject.toml").exists()
            || project_root.join("requirements.txt").exists()
    }
}
```

### 8b. `core/src/language/mod.rs`

- 模块声明：`pub mod python;`
- `AstNode` enum 可新增变体（Python 用 `Other(serde_json::Value)` 即可）

### 8c. `core/src/language/registry.rs`

注册新语言（注意检测优先级顺序）：

```rust
registry.register(Box::new(PythonLanguage));
// 放在 JS 和 TS 后
```

---

## 9. 数据文件

### 9a. `core/compatibility_data/python_libraries/`

新建目录，放 `.toml` 文件。格式参照 `ts_libraries/`：

```toml
[library.numpy]
type = "scientific"
description = "NumPy"
tags = ["array", "math"]
```

### 9b. `core/naming_data/python_to_rust/` 和 `rust_to_python/`

各新建 3 个文件：

- `conventions.toml`
- `type_map.toml`
- `api_map.toml`

### 9c. `core/build.rs`

- compat data 数组新增：`"python_libraries"`
- naming pair 数组新增：`"python_to_rust"`, `"rust_to_python"`

---

## 10. 兼容性与对齐

### 10a. `core/src/compatibility/types.rs` — `MatrixRegistry::load()`

```rust
"python" => include_str!(concat!(env!("OUT_DIR"), "/python_libraries.toml")),
```

### 10b. `core/src/align/naming.rs` — `NamingRegistry::new()`

```rust
"python_to_rust" => include_str!(concat!(env!("OUT_DIR"), "/python_to_rust.toml")),
"rust_to_python" => include_str!(concat!(env!("OUT_DIR"), "/rust_to_python.toml")),
```

### 10c. `core/src/align/api_map.rs` — `ApiMapRegistry::new()`

```rust
"python_to_rust" => include_str!(concat!(env!("OUT_DIR"), "/python_to_rust.toml")),
"rust_to_python" => include_str!(concat!(env!("OUT_DIR"), "/rust_to_python.toml")),
```

---

## 11. 配置验证 —— `core/src/config.rs`

```rust
const VALID_LANGUAGES: &[&str] = &["typescript", "rust", "javascript", "python"];
```

---

## 12. CLI 命令

### 12a. `migration-analyze/src/commands/analyze.rs`

`run()` 中的引用提取 dispatch 新增：

```rust
project::SourceLanguage::Python => references::python::extract_all(&project.root, &files),
```

`guess_source_language()` 回退检测新增 `pyproject.toml`：

```rust
} else if project_root.join("pyproject.toml").exists() {
    "python".to_string()
```

### 12b. `migration-analyze/src/commands/init.rs`

帮助注释更新：

```rust
# Source language to analyze (typescript | javascript | rust | python)
```

### 12c. `migration-analyze/src/commands/check_updates.rs`

依赖解析 dispatch 新增：

```rust
Some("python") | Some("py") => {
    migration_core::deps::resolve_dependencies(source_dir, SourceLanguage::Python)
        .unwrap_or_default()
}
```

---

## 13. 单元测试

### 每个新建模块都需要单元测试

| 文件 | 测试内容 |
|------|----------|
| `symbols/python.rs` | 提取 function/class/variable/async/export |
| `parser/python.rs` | 解析 `import` / `from ... import` / 无导入 |
| `references/python.rs` | 空文件 / 跨文件引用 |
| `language/python.rs` | `name()` / `file_extensions()` / `detect_project_type()` / `parse()` |
| `discovery.rs` | Python 文件发现 / 排除 node_modules / 空目录 |
| `project.rs` | 检测 `pyproject.toml` / hint `--source-lang python` |
| `graph.rs` | Python 相对导入解析 |
| `deps/python.rs` | 从 `pyproject.toml` 提取依赖 |
| `config.rs` | `VALID_LANGUAGES` 包含 "python" |

---

## 完整修改清单

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| 1 | `core/src/project.rs` | 修改 | 加 `Python` 变体 + hint 处理 + 检测逻辑 + `source_language_str()` |
| 2 | `core/src/discovery.rs` | 修改 | `should_include()` 加 Python 扩展名 |
| 3 | `core/src/symbols/python.rs` | **新建** | 符号提取 |
| 4 | `core/src/symbols/mod.rs` | 修改 | 模块声明 + extract dispatch |
| 5 | `core/src/parser/python.rs` | **新建** | import/reference 解析 |
| 6 | `core/src/parser/mod.rs` | 修改 | 模块声明 + extension match |
| 7 | `core/src/references/python.rs` | **新建** | 跨文件引用提取 |
| 8 | `core/src/references/mod.rs` | 修改 | 模块声明 + extension match |
| 9 | `core/src/deps/python.rs` | **新建** | 依赖解析 |
| 10 | `core/src/deps/mod.rs` | 修改 | 模块声明 + dispatch |
| 11 | `core/src/deps/module_map.rs` | 修改 | extract dispatch + `extract_python_external()` |
| 12 | `core/src/graph.rs` | 修改 | import resolution dispatch + `resolve_python_import()` |
| 13 | `core/src/language/python.rs` | **新建** | `Language` + `DiffAnalyzer` impl |
| 14 | `core/src/language/mod.rs` | 修改 | 模块声明 |
| 15 | `core/src/language/registry.rs` | 修改 | 注册 |
| 16 | `core/src/config.rs` | 修改 | `VALID_LANGUAGES` |
| 17 | `core/src/compatibility/types.rs` | 修改 | `MatrixRegistry::load()` |
| 18 | `core/src/align/naming.rs` | 修改 | `NamingRegistry::new()` |
| 19 | `core/src/align/api_map.rs` | 修改 | `ApiMapRegistry::new()` |
| 20 | `core/build.rs` | 修改 | 数据目录数组 |
| 21 | `migration-analyze/src/commands/analyze.rs` | 修改 | extract dispatch + guess fallback |
| 22 | `migration-analyze/src/commands/init.rs` | 修改 | 注释 |
| 23 | `migration-analyze/src/commands/check_updates.rs` | 修改 | deps dispatch |

共计：**新建 6 个文件** + **修改 17 个文件**。

---

## 设计原则

1. **各语言模块独立**：每个语言有自己独立的 `symbols/<lang>.rs`、`parser/<lang>.rs`、`references/<lang>.rs`、`deps/<lang>.rs`、`language/<lang>.rs`，不互相引用内部实现。共享逻辑抽到上层 `mod.rs`。
2. **兼容性数据复用**：如果新语言与已有语言共享同一个生态（如 Python 的 `requirements.txt` vs `Pipfile`），可以用别名处理；否则新建 `compatibility_data/<lang>_libraries/`。
3. **检测优先级**：在 `registry.rs` 中的注册顺序就是检测优先级。最具体的放前面。
4. **Naming/Api 映射复用**：源语言到下语言的命名转换通过 `NamingRegistry` + `ApiMapRegistry` 在 `align/` 中统一处理。
