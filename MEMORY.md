# MEMORY

This is a **fork** of herdr, not a checkout of upstream.

- Branch model: `master` mirrors upstream untouched; `eduwass` is the integration
  branch (upstream tag + local patches) and is what the daily binary is built
  from; one standalone branch per feature off upstream for isolation.
- **The drift ledger lives outside this repo**, in the dotfiles superproject:
  `../herdr-fork.md` (i.e. `~/Sites/dotfiles/forks/herdr-fork.md`). It records
  every local patch, why it exists, its upstream Discussion, and the condition
  that should delete it. **Read it before adding, removing, or rebasing any
  patch here.** `../herdr-check.sh` enforces it.
- Upstream does **not** accept unsolicited pull requests, and agents must never
  open an issue or PR against it. See `CONTRIBUTING.md`. New behavior should go
  through config, an existing plugin, or a plugin of our own before anyone
  considers patching this tree.
- `CLAUDE.md` and `AGENTS.md` in this repo are **upstream's** and rebase from
  upstream — do not edit them to record fork-local facts. This file is the
  fork-local surface.
