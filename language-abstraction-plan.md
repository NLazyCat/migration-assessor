# 语言抽象与模块解耦方案

## 0. 当前架构问题

分析现有代码后，发现以下语言耦合问题：

| 问题 | 位置 | 影响 |
|------|------|------|
| 每个语言重复实现 parser/symbols/deps/references | `core/src/parser/`、`core/src/symbols/` 等 | 新增语言需要改 4 个模块 |
| `diff.rs` 硬编码 TS/Rust 的 diff 逻辑 | `migration-analyze/src/commands/diff.rs` | 新增语言需要改 diff 引擎 |
| `analyze.rs` 按语言分支调用 | `migration-analyze/src/commands/analyze.rs` | 新增语言需要改 CLI |
| `Project::detect` 返回语言枚举 | `core/src/project.rs` | 枚举类型需要频繁修改 |
| 没有统一的语言能力注册机制 | 缺失 | 语言实现分散在各处 |

---

## 1. 目标架构

```
┌──────────────────────────────────────────────────────────────────────────┐
│                         Language Agnostic Layer                         │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                        Trait Definitions                           │  │
│  │  Language      •  Parser       •  SymbolExtractor    •  DepResolver│  │
│  │  ReferenceExtractor • DiffAnalyzer                                 │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                   │                                      │
│                                   ▼                                      │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                        Language Registry                           │  │
│  │  - Factory: get_language(lang: &str) -> &dyn Language             │  │
│  │  - Auto-detection: detect_language(path: &Path) -> String        │  │
│  │  - Registry: HashMap<String, Box<dyn Language>>                   │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                   │                                      │
│            ┌──────────────────────┼──────────────────────┐               │
│            ▼                      ▼                      ▼               │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐          │
│  │   TypeScript │      │     Rust     │      │    Python    │          │
│  │   Language   │      │   Language   │      │   Language   │          │
│  └──────────────┘      └──────────────┘      └──────────────┘          │
│            │                      │                      │               │
│            ▼                      ▼                      ▼               │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐          │
│  │  oxc parser  │      │   syn parser │      │    py-syn    │          │
│  │  oxc symbols │      │ syn symbols  │      │ py symbols   │          │
│  └──────────────┘      └──────────────┘      └──────────────┘          │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 核心 Trait 定义

### 2.1 Language 总接口

```rust
// core/src/language/mod.rs

use crate::project::SourceLanguage;
use crate::symbols::{ApiContract, SymbolIndex};
use crate::deps::ResolvedDependency;
use crate::references::{ForwardIndex, ReverseIndex};
use crate::diff::{FileDiffResult, DiffReport};

pub trait Language: Send + Sync + 'static {
    fn name(&self) -> &str;
    
    fn file_extensions(&self) -> &[&str];
    
    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile>;
    
    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(SymbolIndex, ApiContract)>;
    
    fn extract_references(&self, parsed: &ParsedFile) -> anyhow::Result<(ForwardIndex, ReverseIndex)>;
    
    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<ResolvedDependency>>;
    
    fn diff_analyzer(&self) -> &dyn DiffAnalyzer;
    
    fn detect_project_type(&self, project_root: &Path) -> bool;
}
```

### 2.2 ParsedFile（语言无关的 AST 封装）

```rust
// core/src/language/mod.rs

pub struct ParsedFile {
    pub source: String,
    pub file_path: String,
    pub language: String,
    pub ast: AstNode,
    pub diagnostics: Vec<Diagnostic>,
}

pub enum AstNode {
    TypeScript(oxc_ast::ast::Program<'static>),
    Rust(syn::File),
    Python(python_ast::Module),
    Other(serde_json::Value),
}

pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub severity: DiagnosticSeverity,
}

pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}
```

### 2.3 DiffAnalyzer（语言特定的差异分析）

```rust
// core/src/language/mod.rs

pub trait DiffAnalyzer: Send + Sync + 'static {
    fn diff_files(
        &self,
        old_parsed: &ParsedFile,
        new_parsed: &ParsedFile,
    ) -> anyhow::Result<FileDiffResult>;
    
    fn diff_symbols(
        &self,
        old_sym: &Symbol,
        new_sym: &Symbol,
        old_ast: &AstNode,
        new_ast: &AstNode,
    ) -> anyhow::Result<Vec<SymbolChange>>;
    
    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<ModuleReference>;
    
    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)>;
}
```

---

## 3. Language Registry（语言注册表）

```rust
// core/src/language/registry.rs

use std::collections::HashMap;
use std::sync::OnceLock;

pub struct LanguageRegistry {
    languages: HashMap<String, Box<dyn Language>>,
}

impl LanguageRegistry {
    pub fn get() -> &'static Self {
        static INSTANCE: OnceLock<LanguageRegistry> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let mut registry = LanguageRegistry {
                languages: HashMap::new(),
            };
            registry.register(Box::new(TypeScriptLanguage));
            registry.register(Box::new(RustLanguage));
            registry
        })
    }
    
    fn register(&mut self, lang: Box<dyn Language>) {
        self.languages.insert(lang.name().to_string(), lang);
    }
    
    pub fn get_language(&self, name: &str) -> Option<&dyn Language> {
        self.languages.get(name).map(|l| l.as_ref())
    }
    
    pub fn detect_language(&self, project_root: &Path) -> Option<String> {
        for (name, lang) in &self.languages {
            if lang.detect_project_type(project_root) {
                return Some(name.clone());
            }
        }
        None
    }
    
    pub fn list_languages(&self) -> Vec<String> {
        self.languages.keys().cloned().collect()
    }
}
```

---

## 4. 语言实现示例

### 4.1 TypeScript Language

```rust
// core/src/language/typescript.rs

pub struct TypeScriptLanguage;

impl Language for TypeScriptLanguage {
    fn name(&self) -> &str { "typescript" }
    
    fn file_extensions(&self) -> &[&str] { &["ts", "tsx", "js", "jsx"] }
    
    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile> {
        let allocator = oxc_allocator::Allocator::default();
        let ret = Parser::new(&allocator, source, source_type).parse();
        
        let diagnostics: Vec<Diagnostic> = ret.diagnostics.iter()
            .map(|d| Diagnostic {
                message: d.message.to_string(),
                line: /* compute from span */,
                column: /* compute from span */,
                severity: if d.is_error() { DiagnosticSeverity::Error } else { DiagnosticSeverity::Warning },
            })
            .collect();
        
        Ok(ParsedFile {
            source: source.to_string(),
            file_path: file_path.to_string(),
            language: "typescript".to_string(),
            ast: AstNode::TypeScript(ret.program),
            diagnostics,
        })
    }
    
    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(SymbolIndex, ApiContract)> {
        if let AstNode::TypeScript(program) = &parsed.ast {
            typescript::extract(&parsed.file_path, &parsed.source, Some(Path::new(&parsed.file_path)))
        } else {
            anyhow::bail!("Expected TypeScript AST")
        }
    }
    
    fn extract_references(&self, parsed: &ParsedFile) -> anyhow::Result<(ForwardIndex, ReverseIndex)> {
        // ...
    }
    
    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<ResolvedDependency>> {
        typescript::resolve(project_root)
    }
    
    fn diff_analyzer(&self) -> &dyn DiffAnalyzer {
        &TypeScriptDiffAnalyzer
    }
    
    fn detect_project_type(&self, project_root: &Path) -> bool {
        project_root.join("tsconfig.json").exists() 
            || project_root.join("package.json").exists()
    }
}

pub struct TypeScriptDiffAnalyzer;

impl DiffAnalyzer for TypeScriptDiffAnalyzer {
    fn diff_files(&self, old: &ParsedFile, new: &ParsedFile) -> anyhow::Result<FileDiffResult> {
        // 直接操作 oxc AST 节点
        if let (AstNode::TypeScript(old_prog), AstNode::TypeScript(new_prog)) = (&old.ast, &new.ast) {
            // 对比两个 Program 的差异
        }
        // ...
    }
    
    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
        if let AstNode::TypeScript(program) = &parsed.ast {
            // 遍历 AST，收集所有 CallExpr
            let mut calls = Vec::new();
            // ...
            calls
        } else {
            vec![]
        }
    }
    
    // ...
}
```

### 4.2 Rust Language

```rust
// core/src/language/rust.rs

pub struct RustLanguage;

impl Language for RustLanguage {
    fn name(&self) -> &str { "rust" }
    
    fn file_extensions(&self) -> &[&str] { &["rs"] }
    
    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile> {
        let file: syn::File = syn::parse_file(source)?;
        
        Ok(ParsedFile {
            source: source.to_string(),
            file_path: file_path.to_string(),
            language: "rust".to_string(),
            ast: AstNode::Rust(file),
            diagnostics: vec![],
        })
    }
    
    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(SymbolIndex, ApiContract)> {
        if let AstNode::Rust(file) = &parsed.ast {
            rust::extract(&parsed.file_path, &parsed.source)
        } else {
            anyhow::bail!("Expected Rust AST")
        }
    }
    
    // ... 其他方法
}
```

---

## 5. 新增语言的步骤

假设要新增 Python 支持：

```bash
# 1. 在 Cargo.toml 中添加 Python 解析器依赖
py-syn = "..."

# 2. 创建语言实现文件
core/src/language/python.rs

# 3. 在 registry.rs 中注册
registry.register(Box::new(PythonLanguage));

# 4. 在 mod.rs 中导出
pub mod python;
```

仅此而已！不需要修改 `analyze.rs`、`diff.rs`、`symbols/mod.rs` 等任何现有文件。

---

## 6. Diff Engine 语言无关化

```rust
// core/src/diff/engine.rs

use crate::language::{Language, ParsedFile};

pub struct DiffEngine;

impl DiffEngine {
    pub fn diff_files(
        old_source: &str,
        new_source: &str,
        file_path: &str,
        language: &dyn Language,
    ) -> anyhow::Result<FileDiffResult> {
        let old_parsed = language.parse(old_source, file_path)?;
        let new_parsed = language.parse(new_source, file_path)?;
        
        language.diff_analyzer().diff_files(&old_parsed, &new_parsed)
    }
    
    pub fn diff_project(
        project_root: &Path,
        from_version: &str,
        to_version: &str,
    ) -> anyhow::Result<DiffReport> {
        let lang_name = LanguageRegistry::get().detect_language(project_root)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect project language"))?;
        
        let language = LanguageRegistry::get().get_language(&lang_name)
            .ok_or_else(|| anyhow::anyhow!("Language {} not supported", lang_name))?;
        
        // 获取变更文件列表
        let changed_files = get_changed_files(project_root, from_version, to_version)?;
        
        let mut file_changes = Vec::new();
        for file in &changed_files {
            let old_source = get_file_at_version(project_root, from_version, file)?;
            let new_source = get_file_at_version(project_root, to_version, file)?;
            
            let diff = Self::diff_files(&old_source, &new_source, file, language)?;
            file_changes.push(diff);
        }
        
        // 依赖变化检测
        let old_deps = language.resolve_dependencies(project_root)?;
        // ... 获取新版本依赖
        
        Ok(DiffReport {
            file_changes,
            // ...
        })
    }
}
```

---

## 7. 目录结构重构

```
core/src/
├── language/
│   ├── mod.rs              # Trait 定义、ParsedFile、Diagnostic
│   ├── registry.rs         # LanguageRegistry、工厂模式
│   ├── typescript.rs       # TypeScriptLanguage + TypeScriptDiffAnalyzer
│   ├── rust.rs             # RustLanguage + RustDiffAnalyzer
│   └── python.rs           # (未来扩展) PythonLanguage
├── symbols/
│   ├── mod.rs              # Symbol、SymbolIndex 定义（语言无关）
│   ├── typescript.rs       # TS 符号提取（内部实现）
│   └── rust.rs             # Rust 符号提取（内部实现）
├── deps/
│   ├── mod.rs              # ResolvedDependency 定义（语言无关）
│   ├── typescript.rs       # TS 依赖解析（内部实现）
│   └── rust.rs             # Rust 依赖解析（内部实现）
├── references/
│   ├── mod.rs              # ForwardIndex、ReverseIndex（语言无关）
│   ├── typescript.rs       # TS 引用提取（内部实现）
│   └── rust.rs             # Rust 引用提取（内部实现）
├── parser/
│   ├── mod.rs              # ModuleReferences（语言无关）
│   ├── typescript.rs       # TS import 解析（内部实现）
│   └── rust.rs             # Rust use 解析（内部实现）
├── diff/
│   ├── mod.rs              # DiffReport、FileDiffResult（语言无关）
│   ├── engine.rs           # DiffEngine（语言无关核心）
│   ├── mapping.rs          # SymbolMapping（语言无关）
│   └── signature.rs        # SignatureDiff（语言无关）
└── project.rs              # SourceLanguage（保留，但内部使用 LanguageRegistry）
```

---

## 8. 与现有代码的兼容性

### 8.1 渐进式迁移

不需要一次性重构所有代码。可以分阶段进行：

**Phase 1**：定义 `Language` trait 和 `LanguageRegistry`，实现 TS/Rust 的 stub 版本
- 新代码使用 registry，旧代码继续使用直接调用

**Phase 2**：实现完整的 `ParsedFile` 和 `AstNode` 封装
- 让现有 parser/symbols/references 模块适配新接口

**Phase 3**：重构 `diff` 命令，使用 `DiffEngine`
- CLI 层不再需要语言分支

**Phase 4**：重构 `analyze` 命令，使用 `LanguageRegistry`
- CLI 层不再需要语言分支

### 8.2 向后兼容保证

- `SourceLanguage` 枚举保留，但内部实现改为调用 `LanguageRegistry`
- 现有的 `SymbolExtractor::extract_all`、`resolve_dependencies` 等函数保留，内部委托给 `Language` trait
- 报告格式保持不变，新增字段向后兼容

---

## 9. 优势总结

| 维度 | 重构前 | 重构后 |
|------|--------|--------|
| 新增语言 | 修改 4+ 个模块 | 新增 1 个文件 + 注册 |
| 语言切换 | 编译期枚举 | 运行时动态选择 |
| 代码复用 | 无 | Trait 默认实现、共享 DiffEngine |
| 测试 | 每个语言独立测试 | 语言无关测试框架 |
| 维护成本 | 高（分散） | 低（集中 registry） |

---

*计划完成。请确认后进入实施。*
