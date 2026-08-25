# Git Dashboard TUI Feature Backlog

This backlog tracks planned, upcoming, and considered features for `git-dashboard-tui`.

---

## 🎯 In Progress (Active Development)

- [ ] **A-1: Bulk Pull & Fetch (`Shift+P` / `Shift+F`)**
  - Pull or fetch all registered (or filtered) repositories concurrently with background progress and summary.
- [ ] **A-2: Repository Grouping / Tagging (Groups)**
  - Categorize repositories by group (e.g. `Work`, `Personal`, `OSS`, `Client-A`).
  - Filter by group (`[` / `]` or group tab) and assign groups in Settings.
- [ ] **B-1: Time Span Filter (Activity & Contributors)**
  - Filter commit statistics and contributors by period (`All`, `1 Week`, `1 Month`, `3 Months`).
  - Toggle via `w` / `W` key in Contributors tab.
- [ ] **B-3: Global Members View (Cross-Repository Member Analytics)**
  - View member activity across all registered repositories (`M` key from Home / Settings).
  - Drill down into which products/repositories each member has contributed to, commit counts, and latest activity dates. Helps leads & managers allocate tasks effectively.

---

## 📋 Future Backlog (Under Consideration)

### Category A: Multi-Repository Monitoring
- **A-3: Repository Health Indicators & Stale Branch Alerts**
  - Highlight repositories with uncommitted changes older than X days.
  - Detect stale local/remote branches and unmerged branches.

### Category B: Contributor & Team Analytics
- **B-2: Lines Changed (Additions / Deletions) Impact Metrics**
  - Aggregate total lines added/deleted per contributor in addition to commit count.
  - Identify codebase growth and major refactoring contributors.

### Category C: Development Workflow Integration
- **C-1: Open in External Editor / Terminal (`o` / `t`)**
  - Open the selected repository or changed file directly in `$EDITOR` (VS Code, Cursor, Neovim, IntelliJ) or open a new terminal in the repository directory.
- **C-2: Git Worktree Inspection & Quick Jump**
  - List all active git worktrees for the current repository and allow jumping directly between worktrees.
- **C-3: Quick Jump to Interactive Client (Lazygit / GitUI)**
  - Press `G` to launch `lazygit` or `gitui` on the selected repository, returning seamlessly to the dashboard upon exit.

### Category D: Reporting & Export
- **D-1: Markdown Report Generation for Daily/Weekly Standups (`y`)**
  - Copy formatted Markdown summary of commits, contributors, or repository statuses for weekly reports, 1-on-1s, or sprint retrospectives.
