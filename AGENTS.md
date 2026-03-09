# Agent Instructions

This project uses **spec-driven development** with **worktrees** and **stacked commits**.

## Development Workflow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        SPEC-DRIVEN DEVELOPMENT                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────────┐   │
│   │  spec:   │───▶│  spec:   │───▶│  spec:   │───▶│   spec:      │   │
│   │ product  │    │  design  │    │  plan    │    │  implement   │   │
│   └──────────┘    └──────────┘    └──────────┘    └──────────────┘   │
│        │              │              │                 │               │
│        ▼              ▼              ▼                 ▼               │
│   requirements     spec.md       plan.json        implement         │
│      .json                                           tasks           │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                        WORKTREE LAYER                         │   │
│   │                                                                 │   │
│   │   worktrees/              worktrees/             worktrees/   │   │
│   │   ├── grpc/               ├── rocksdb/           ├── <feature>/│   │
│   │   │   └── feat/grpc       │   └── feat/rocksdb   │   └── feat/  │   │
│   │   └── (main branch)       └── (main branch)      └── (main)    │   │
│   │                                                                 │   │
│   │   [Each feature in isolated worktree]                        │   │
│   │   [Parallel development possible]                             │   │
│   │   [Clean PRs per feature]                                      │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                      STACKED COMMITS                          │   │
│   │                                                                 │   │
│   │   main ──┬── feat/grpc ──┬── fix-errors ──┬── add-server     │   │
│   │           │               │                │                  │   │
│   │           │               └────────────────┼── add-client    │   │
│   │           │                                 │                  │   │
│   │           └─────────────────────────────────┴── add-integration│   │
│   │                                                                 │   │
│   │   gt stack        # View stack                                 │   │
│   │   gt branch create "feat: add X"  # New commit                │   │
│   │   gt branch restack              # Rebase onto main           │   │
│   │   gt stack submit                # Create stacked PRs         │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                       BEADS TRACKING                          │   │
│   │                                                                 │   │
│   │   bd ready              # Find unblocked work                 │   │
│   │   bd show <id>          # View issue details                  │   │
│   │   bd update <id> --claim  # Start work                        │   │
│   │   bd close <id> -r "done" # Complete work                    │   │
│   │   bd sync               # Sync with git                      │   │
│   │                                                                 │   │
│   │   [Persists across sessions]                                  │   │
│   │   [Tracks dependencies between features]                      │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Quick Reference

```bash
# Spec workflow
ls docs/specs/<feature>/              # Check for spec
cat docs/specs/<feature>/spec.md       # Read spec
cat docs/specs/<feature>/plan.json      # Read tasks

# Worktrees (use code:git-worktrees skill for creation)
cd worktrees/<feature>/                # Work in isolation
git worktree list                      # List all worktrees

# Stacked commits (use code:stacked-commit skill)
gt stack                               # View stack
gt branch create "feat: add feature"   # New commit
gt branch continue                     # Amend current
gt branch restack                      # Rebase onto main
gt stack submit                        # Create PRs

# Beads
bd ready                               # Find work
bd show <id>                           # View details
bd update <id> --claim                # Start work
bd close <id> -r "done"                # Complete
bd sync                                # Sync with git
```

## 1. Spec-Driven Development

**Before writing ANY code:**
1. Check for existing spec: `ls docs/specs/<feature>/`
2. If spec exists: Review spec.md + plan.json
3. If no spec: Use spec:workflow skill
4. **NEVER skip planning** - even "simple" changes need specs

**Artifacts:**
- `requirements.json` - Structured scope
- `spec.md` - Technical design
- `plan.json` - Task breakdown (→ beads issues)

## 2. Worktree Strategy

**Each feature gets its own worktree in `worktrees/`:**
```
worktrees/
├── grpc/      (feat/grpc branch)
├── rocksdb/   (feat/rocksdb branch)
└── <feature>/ (feat/<feature> branch)
```

**Why worktrees:**
- Isolated development (no context switching)
- Parallel feature development
- Clean PRs per feature
- Easy rollback if needed

**Working in worktrees:**
```bash
cd worktrees/<feature>
# All work happens here
# Tests, commits, everything
```

## 3. Stacked Commits with Graphite

**For complex features with multiple commits:**
```bash
# Install Graphite (if needed)
brew install graphite-dev

# Create stacked commits
gt branch create "feat(api): add user endpoint"
git add src/users/
gt branch continue

gt branch create "feat(api): add validation"
git add src/validation/
gt branch continue

# View stack
gt stack

# Submit as stacked PRs
gt branch restack  # Rebase onto main
gt stack submit     # Create PRs
```

## 4. Beads Issue Tracking

**Track work across sessions:**
```bash
bd ready              # Find unblocked work
bd show <id>          # View issue details
bd update <id> --claim  # Claim and start
bd close <id> -r "done" # Complete work
bd sync               # Sync with git
```

## Integrated Workflow Example

```
1. spec:product → spec:design → spec:plan     (planning phase)
2. bd create epic for feature                  (tracking)
3. git worktree add worktrees/<feature> -b feat/<feature>  (isolation)
4. gt branch create "fix: compilation"         (stacked commits)
5. Implement, test, gt branch continue
6. gt branch create "feat: add X"
7. Implement, test, gt branch continue
8. gt branch restack && gt stack submit
9. Create PR, review, merge
10. bd close <epic-id>
```

## Project Structure

```
seshat/
├── worktrees/              # Isolated feature development
│   ├── grpc/              # feat/grpc branch - gRPC network layer
│   ├── rocksdb/           # feat/rocksdb branch - RocksDB storage
│   └── <feature>/         # feat/<feature> branch
├── docs/
│   └── specs/             # Spec-driven development
│       ├── grpc/          # spec.md + plan.json
│       ├── rocksdb/
│       ├── kvservice/
│       └── openraft/     # Phase 1 complete
├── .beads/                # Issue tracking (git-backed)
└── crates/                # Rust workspace
    ├── kv/               # RaftNode + StubNetwork
    ├── storage/          # OpenRaft storage
    ├── resp/             # RESP protocol
    ├── seshat/          # Main binary
    └── raft/            # (in grpc worktree)
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create beads for follow-up
2. **Run quality gates** (if code changed):
   ```bash
   cargo test --workspace
   cargo clippy --workspace
   cargo fmt --check
   ```
3. **Update issue status**:
   ```bash
   bd close <done-id>
   bd update <wip-id> --notes "progress..."
   bd sync
   ```
4. **PUSH TO REMOTE** - This is MANDATORY (Graphite handles it):
   ```bash
   # In worktree - Graphite handles rebase + push
   gt branch sync     # pull --rebase + restack all branches
   gt stack submit   # push all branches + create PRs
   git status         # verify "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
- ALWAYS work in worktrees for feature development
- ALWAYS use spec workflow before implementation

## Current Branches & Worktrees

| Branch | Worktree | Purpose |
|--------|----------|---------|
| main | - | Production branch |
| feat/grpc | worktrees/grpc/ | gRPC network layer (in progress) |
| feat/rocksdb | worktrees/rocksdb/ | RocksDB storage (not started) |
| feat/openraft | - | Merged to main |
| feat/raft | worktrees/raft/ | DEPRECATED - delete later |

## Next Session Quick Start

```bash
# Check ready work
bd ready

# Navigate to worktree
cd worktrees/grpc

# Check status
git status
cargo test --workspace

# Continue implementation
```
