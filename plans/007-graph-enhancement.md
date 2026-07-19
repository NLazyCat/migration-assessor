# Plan 007: Enrich graph data with degrees and directory hierarchy

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- core/src/graph.rs core/src/report.rs migration-analyze/src/web/routes.rs migration-analyze/src/web/templates.rs`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: 006
- **Category**: direction
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

The current graph serialization only contains node IDs and edges. The HTML report recomputes link counts in JavaScript and has to guess directory groupings. Precomputing in-degree, out-degree, and directory layers on the Rust side reduces frontend work and makes the graph data useful for other consumers (e.g., the serve API and scoring algorithms).

## Current state

- `core/src/graph.rs:10-13`:
  ```rust
  pub struct DependencyGraph {
      pub nodes: Vec<String>,
      pub edges: Vec<Edge>,
  }
  ```
- `core/src/graph.rs:16-19`:
  ```rust
  pub struct Edge {
      pub from: String,
      pub to: String,
  }
  ```
- `core/src/report.rs:32-33` serializes `dag.nodes` and `dag.edges` directly into the HTML.
- `web/routes.rs` serves `internal-deps/dag.json` (or `graph/nodes.json`/`graph/edges.json` after plan 006).

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| E2E       | `cargo run -p migration-analyze -- analyze <fixture>` | graph files contain enriched node data |

## Scope

**In scope**:
- `core/src/graph.rs`
- `core/src/report.rs`
- `migration-analyze/src/web/routes.rs`
- `migration-analyze/src/web/templates.rs` (if it consumes graph data)

**Out of scope**:
- Changing the visual graph layout algorithm.
- Adding new graph metrics beyond in-degree, out-degree, top-level directory, and cycle membership.

## Steps

### Step 1: Add enriched node type

In `core/src/graph.rs`, replace the plain `Vec<String>` nodes with a richer type while keeping edge endpoints as strings:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub in_degree: usize,
    pub out_degree: usize,
    pub top_dir: String,
    pub dir_path: String,
    pub in_cycle: bool,
}
```

Keep `Edge` unchanged.

Update `DependencyGraph`:
```rust
pub struct DependencyGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 2: Compute node metadata in `GraphBuilder::build`

After collecting `nodes` and `edges`, compute degrees and directory metadata:

```rust
let mut in_degree: HashMap<String, usize> = HashMap::new();
let mut out_degree: HashMap<String, usize> = HashMap::new();
for edge in &edges {
    *out_degree.entry(edge.from.clone()).or_default() += 1;
    *in_degree.entry(edge.to.clone()).or_default() += 1;
}

let mut node_list: Vec<Node> = nodes.into_iter().map(|id| {
    let top_dir = id.split('/').next().unwrap_or("_root").to_string();
    let dir_path = id.rsplitn(2, '/').nth(1).unwrap_or("").to_string();
    Node {
        id,
        in_degree: in_degree.get(&id).copied().unwrap_or(0),
        out_degree: out_degree.get(&id).copied().unwrap_or(0),
        top_dir,
        dir_path,
        in_cycle: false,
    }
}).collect();
node_list.sort_by(|a, b| a.id.cmp(&b.id));
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 3: Mark cycle membership

In `DependencyGraph::detect_cycles`, after identifying cycles and self-loops, mark the affected nodes:

```rust
let mut cycle_nodes: HashSet<String> = HashSet::new();
for cycle in &cycles {
    for node in &cycle.nodes {
        cycle_nodes.insert(node.clone());
    }
}
for node in &self_loops {
    cycle_nodes.insert(node.clone());
}

for node in &mut self.nodes {
    node.in_cycle = cycle_nodes.contains(&node.id);
}
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 4: Update consumers

Update `core/src/report.rs` and `web/routes.rs` to use `node.id` where they previously used the node string directly. For example, in `report.rs` where it builds `nodes_json`, the JS currently receives an array of strings. Change it to receive an array of objects and update the embedded JS to use `n.id`, `n.in_degree`, `n.out_degree`, `n.top_dir`, etc.

In `report.rs`:
```rust
let nodes_json = serde_json::to_string(&dag.nodes).unwrap_or_default();
```

Update the JS section that builds `allNodes` and `topDir` to use the new shape. For example:
```javascript
const allNodes = {nodes_json};
const allEdges = {edges_json};

function topDir(n) { return n.top_dir; }
function linkCount(n) { return n.out_degree; }
```

Update edge filtering to compare `e.from`/`e.to` against `n.id`.

**Verify**: `cargo check -p migration-core` and `cargo check -p migration-analyze` succeed.

### Step 5: Update serve graph endpoint

Ensure `/api/graph` (and the new `/api/graph/nodes`, `/api/graph/edges` if added) returns the enriched node objects. If plan 006 has already split nodes/edges, make sure the nodes file contains the new `Node` structure.

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 6: Full verification

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Then run `analyze` on a fixture and inspect `graph/nodes.json` to confirm each node has `id`, `in_degree`, `out_degree`, `top_dir`, `dir_path`, and `in_cycle`.

## Test plan

- Add unit tests in `core/src/graph.rs`:
  - A simple graph with edges A→B and B→C produces correct in/out degrees.
  - A cycle A→B→A marks both nodes with `in_cycle: true`.
  - A node at `src/utils.ts` has `top_dir: "src"` and `dir_path: "src"`.
  - A node at `src/lib/utils.ts` has `top_dir: "src"` and `dir_path: "src/lib"`.

## Done criteria

- [ ] `Node` struct exists with `id`, `in_degree`, `out_degree`, `top_dir`, `dir_path`, `in_cycle`.
- [ ] `GraphBuilder` computes degrees and directory metadata.
- [ ] `detect_cycles` marks cycle membership on nodes.
- [ ] HTML report and serve endpoints use the enriched node shape.
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `plans/README.md` status row for plan 007 updated to DONE.

## STOP conditions

Stop and report if:
- The HTML report's D3 code is too tightly coupled to string nodes and cannot be updated without a full rewrite.
- Other consumers of `DependencyGraph` (e.g., scoring) break due to the node type change.

## Maintenance notes

- Scoring algorithms can now use precomputed `in_degree` instead of recomputing it from edges.
- Future metrics (e.g., betweenness, page-rank) can extend `Node` without breaking the edge structure.
