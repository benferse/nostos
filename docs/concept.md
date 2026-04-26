# nostos — Welcome home, hero

## Problem statement

I am a software developer, and I work on many different platforms — Windows,
macOS, Linux. I use a lot of the same tools across the different platforms,
and it's difficult to keep my development environment in sync. For example:

- I might be working on macOS and install a new copilot cli plugin, but when I
  switch to a dev box Cloud PC running Windows, I have to remember to install
  the new plugin there too
- I might tweak some settings or configuration on one machine. This now needs
  to be replicated across machines
- Some configuration or tools may only be useful on some platforms
- There might be machine-specific modifications that need to be made. For
  example, two different macOS laptops might need to have local overrides for
  some settings
- Sometimes I need to spin up a temporary VM or a different OS install, and
  would like all of my tools and config to be made available quickly and
  consistently

There are some solutions for managing dotfiles, automating software installs.
I would like a solution that is optimized for my specific needs.

## Prior art

Several tools exist in this space. Understanding their strengths and
limitations will help guide the design of nostos.

| Tool | Approach | Strengths | Gaps |
|------|----------|-----------|------|
| **chezmoi** | Git-backed dotfile manager with templates | Mature, good templating, encryption support | Dotfiles only — no package management, no plugin sync |
| **GNU Stow** | Symlink farm manager | Dead simple, composable | Manual, no templating, no cross-platform story |
| **yadm** | Git wrapper for `$HOME` | Familiar git workflow, alt files per OS | Limited to dotfiles, weak Windows support |
| **Nix / home-manager** | Declarative, reproducible builds | Extremely powerful, reproducible | Steep learning curve, poor Windows/macOS support |
| **Ansible** | Imperative playbooks with declarative modules | Very flexible, cross-platform | Heavy, designed for fleets not personal machines |
| **Dotbot** | YAML-driven dotfile bootstrapper | Simple config, plugin ecosystem | Narrow scope, no ongoing convergence |

None of these tools cleanly address the full loop of: dotfiles + packages +
editor plugins + shell environment + platform conditionals + machine-specific
overrides, in a single, opinionated tool designed for a single developer
working across many machines.

## Scope

nostos manages the following categories of environment state:

### Configuration files (dotfiles)

Shell config (`.bashrc`, `.zshrc`, PowerShell profile), editor settings
(VS Code `settings.json`, Vim config), git config, SSH config, tool-specific
config files (`.cargo/config.toml`, `.npmrc`, etc.).

Dotfiles are stored in the repository **without their leading dot** (e.g.,
`dotfiles/bashrc` rather than `dotfiles/.bashrc`). nostos prepends the `.`
when copying to the target directory. This keeps files visible in normal
directory listings and file browsers. See the [repository layout](#repository-layout)
example for details.

For files that don't start with a dot (e.g., `~/bin/myscript`), a separate
`[files]` section provides verbatim copying with no path transformation.

On Unix systems, this category also covers **shell environment** — aliases,
functions, environment variables, `PATH` modifications, and shell
completions. These all live in shell configuration files and are managed as
dotfiles.

**Windows caveat:** On Unix, environment variables live in shell config files.
On Windows, system-wide environment variables (visible to all processes, GUI
apps, and non-PowerShell terminals) live in the **Windows Registry**, not in
files. The PowerShell profile covers most interactive shell needs — aliases,
functions, PowerShell-scoped env vars — but cannot set system-wide variables.

| What | Unix | Windows |
|------|------|---------|
| Shell aliases/functions | `.bashrc` / `.zshrc` (dotfile) | PowerShell `$PROFILE` (dotfile) |
| Shell-scoped env vars | `.bashrc` / `.zshenv` (dotfile) | PowerShell `$PROFILE` (dotfile) |
| System-wide env vars | `.pam_environment` or shell config (dotfile) | **Windows Registry** (not a file) |
| PATH additions | Shell config (dotfile) | **Windows Registry** (not a file) |

For the initial implementation, nostos manages the PowerShell profile as a
dotfile, which covers most interactive development needs on Windows. If
system-wide Registry-based environment variables prove necessary (e.g., for
GUI apps or non-PowerShell terminals), this could be added as a dedicated
Windows environment module in the reconciler. This decision is deferred
until there is a concrete use case.

### Package and tool installation

System packages (via `brew`, `winget`, `apt`, `pacman`, etc.), language
toolchains and version managers (`rustup`, `nvm`, `pyenv`), and standalone
CLI tools (`ripgrep`, `fd`, `jq`, etc.).

### What nostos does NOT manage

- **Editor extensions** — VS Code has built-in Settings Sync and per-repo
  `.vscode/extensions.json`. Neovim plugins are declared in config files
  (managed as dotfiles) and auto-install via plugin managers. JetBrains
  has its own sync. Editors already solve this problem — nostos manages
  their config files via the dotfiles module and lets them handle extensions.

- **Secrets and credentials** — integration with platform keychains or
  secret managers is out of scope for the initial implementation. nostos
  should work without it. This could be revisited later if needed.

## Design decisions

### The core modeling problem: tools vs. package managers

Before choosing a configuration format, we need to decide what the primary
abstraction is. Consider ripgrep — a single tool that can be installed in
many different ways depending on the platform:

| Platform | Available installers |
|----------|---------------------|
| macOS | `brew install ripgrep`, `cargo install ripgrep`, `port install ripgrep`, GitHub release binary |
| Windows | `winget install BurntSushi.ripgrep`, `scoop install ripgrep`, `choco install ripgrep`, `cargo install ripgrep`, GitHub release binary |
| Ubuntu/Debian | `apt install ripgrep`, `cargo install ripgrep`, `snap install ripgrep`, GitHub release binary |
| Fedora | `dnf install ripgrep`, `cargo install ripgrep`, GitHub release binary |
| Arch | `pacman -S ripgrep`, `cargo install ripgrep`, GitHub release binary |

And the package name isn't always consistent. `fd` is called `fd` in brew
and pacman, but `fd-find` in apt. `bat` is `bat` most places but `batcat`
in older Debian/Ubuntu.

This creates a fundamental design question: **what does the user declare?**

#### Approach A: Package-manager-centric

The user lists packages grouped by the package manager that installs them:

```toml
[packages.macos]
brew = ["ripgrep", "fd", "jq", "bat"]

[packages.linux.ubuntu]
apt = ["ripgrep", "fd-find", "jq", "bat"]

[packages.windows]
winget = ["BurntSushi.ripgrep"]
scoop = ["fd", "jq", "bat"]

[packages.common]
cargo = ["cargo-edit", "cargo-watch"]
```

**Pros:**
- Simple mental model — you're literally writing install commands
- No abstraction to learn — maps directly to what nostos will execute
- Easy to add a new package manager

**Cons:**
- The same logical tool (ripgrep) appears multiple times under different names
- Adding a new machine means auditing every platform section
- No way to express "I want ripgrep everywhere — figure out how"
- The user must know the package name for every manager on every platform

#### Approach B: Tool-centric with install strategies

The primary abstraction is the **tool**. Each tool declares how it can be
installed, and nostos picks the best available strategy on the current
machine:

```toml
[[tool]]
name = "ripgrep"
description = "Fast recursive grep"
install.brew = "ripgrep"
install.apt = "ripgrep"
install.dnf = "ripgrep"
install.pacman = "ripgrep"
install.winget = "BurntSushi.ripgrep"
install.scoop = "ripgrep"
install.cargo = "ripgrep"

[[tool]]
name = "fd"
description = "Fast find alternative"
install.brew = "fd"
install.apt = "fd-find"          # different name on Debian
install.pacman = "fd"
install.winget = "sharkdp.fd"
install.cargo = "fd-find"

[[tool]]
name = "cargo-edit"
description = "Cargo subcommands for dependency management"
install.cargo = "cargo-edit"     # only available via cargo
```

At apply time, nostos uses a **resolver** that:
1. Detects which package managers are available on the current machine
2. Picks the best one for each tool (based on a preference order)
3. Falls back through alternatives if the preferred manager isn't available

The preference order can be configured globally or per-machine:

```toml
[preferences.macos]
installer-priority = ["brew", "cargo"]

[preferences.linux.ubuntu]
installer-priority = ["apt", "cargo"]

[preferences.windows]
installer-priority = ["winget", "scoop", "cargo"]
```

**Pros:**
- "I want ripgrep" is expressed once — nostos handles the platform details
- Adding a new machine requires zero config changes if the tool mappings
  already cover that platform
- Clean separation between *what* you want and *how* it gets installed
- Makes `nostos plan` output very clear: "Install ripgrep via brew"

**Cons:**
- More upfront work per tool — you're writing a mini package manifest
- The tool mapping database is the user's responsibility (nostos doesn't
  ship a built-in registry of how to install popular tools)
- Resolver logic adds complexity — "why did it pick scoop instead of winget?"

#### Approach C: Hybrid — tool-centric with inline shortcuts

Tools are the primary abstraction, but common cases are concise. If a tool
has the same name across all package managers, you don't need to spell out
every installer:

```toml
# Simple case: same name everywhere, nostos uses whatever's available
[[tool]]
name = "ripgrep"

# Override only where the name differs
[[tool]]
name = "fd"
install.apt = "fd-find"
install.cargo = "fd-find"

# Cargo-only tool — no platform mapping needed
[[tool]]
name = "cargo-edit"
install.cargo = "cargo-edit"

# Platform-specific tool — only install on Linux
[[tool]]
name = "build-essential"
install.apt = "build-essential"
platforms = ["linux"]

# Complex case: full control
[[tool]]
name = "neovim"
install.brew = "neovim"
install.apt = "neovim"
install.winget = "Neovim.Neovim"
install.pacman = "neovim"
install.choco = "neovim"
```

**Pros:**
- Best ergonomics — the simple case (`ripgrep`) is one line
- Overrides only where needed — no boilerplate for well-named packages
- Still fully explicit when names diverge
- `platforms` field naturally limits where a tool is relevant

**Cons:**
- "Same name everywhere" is an implicit assumption — if it's wrong on some
  manager, the error happens at install time, not at config parse time
- Mixed explicitness levels in the same file can be confusing

#### Recommendation

**Approach C (hybrid tool-centric)** gives the best balance. Most popular
CLI tools use the same package name across managers, so the common case is
a single line. Where names diverge, explicit mappings are clean and obvious.
The `platforms` field handles the "only on Linux" case without needing
platform-specific sections.

### Configuration file format

With the tool-centric model decided, the question becomes which file format
best expresses it. This is now a narrower question — the data model is
mostly flat lists of tools with optional key-value overrides.

#### Option A: TOML

```toml
[preferences.macos]
installer-priority = ["brew", "cargo"]

[preferences.linux.ubuntu]
installer-priority = ["apt", "cargo"]

[preferences.linux.fedora]
installer-priority = ["dnf", "cargo"]

[preferences.windows]
installer-priority = ["winget", "scoop", "cargo"]

[[tool]]
name = "ripgrep"

[[tool]]
name = "fd"
install.apt = "fd-find"
install.cargo = "fd-find"

[[tool]]
name = "build-essential"
install.apt = "build-essential"
install.dnf = "gcc"
platforms = ["linux"]

[dotfiles]
source = "dotfiles/"
target = "~"

[dotfiles.platforms.macos]
"config/alacritty/alacritty.toml" = "macos/config/alacritty/alacritty.toml"
```

**Pros:**
- Native to the Rust ecosystem — `serde` + `toml` crate, zero friction
- Familiar to anyone who writes `Cargo.toml`
- `[[tool]]` array-of-tables maps naturally to the tool-centric model
- Strong typing, good error messages on parse failure
- No footguns (no implicit type coercion, no billion-laughs attacks)

**Cons:**
- `[[tool]]` repeated many times gets visually noisy for large tool lists
- Deeply nested structures (dotfile overrides) become verbose
- No native support for multi-line templates or complex conditionals

#### Option B: YAML

```yaml
preferences:
  macos:
    installer-priority: [brew, cargo]
  linux:
    ubuntu:
      installer-priority: [apt, cargo]
    fedora:
      installer-priority: [dnf, cargo]
  windows:
    installer-priority: [winget, scoop, cargo]

tools:
  - name: ripgrep
  - name: fd
    install:
      apt: fd-find
      cargo: fd-find
  - name: build-essential
    install:
      apt: build-essential
      dnf: gcc
    platforms: [linux]

dotfiles:
  source: dotfiles/
  target: "~"
  platforms:
    macos:
      config/alacritty/alacritty.toml: macos/config/alacritty/alacritty.toml
```

**Pros:**
- Very readable for lists of tools — more compact than TOML's `[[tool]]`
- Widely known
- Good support for multi-line strings and complex nesting

**Cons:**
- Notorious parsing gotchas (`no` → `false`, `3.10` → `3.1`)
- Whitespace-sensitive — easy to introduce invisible bugs
- Feels out of place in a Rust project

#### Option C: A custom DSL

```
prefer macos: brew, cargo
prefer linux: apt, cargo
prefer windows: winget, scoop, cargo

tool "ripgrep"
tool "fd" { apt = "fd-find", cargo = "fd-find" }
tool "build-essential" { apt = "build-essential", only linux }
tool "cargo-edit" { cargo = "cargo-edit" }

dotfiles "dotfiles/" -> "~" {
    when macos {
        "config/alacritty/alacritty.toml" <- "macos/config/alacritty/alacritty.toml"
    }
}
```

**Pros:**
- Most compact representation — the simple case is truly one line
- Can be tailored exactly to the domain
- First-class `when`, `only`, `prefer` keywords read naturally
- Opportunity for excellent, domain-specific error messages

**Cons:**
- Significant implementation effort (lexer, parser, error reporting)
- Users must learn a new format
- No existing editor support or syntax highlighting
- Risk of scope creep in the language design
- Harder for other tools to consume or generate programmatically

#### Recommendation

**Start with TOML.** It is native to Rust, has excellent library support, and
avoids the sharp edges of YAML. The `[[tool]]` syntax is slightly verbose
but perfectly functional. If the tool list grows large enough that the
verbosity becomes painful, a custom DSL could be explored — but the data
model should be proven first before investing in a custom syntax.

### Sync and storage mechanism

nostos needs a way to store configuration centrally and make it available
across machines. There are several viable approaches.

#### Option A: Git repository (the chezmoi model)

The nostos configuration lives in a git repository. Users push changes from
one machine and pull on another. nostos provides commands to commit, push,
and pull, or users can use git directly.

**Pros:**
- Full version history, branching, diffing — all for free
- Works offline — changes are local until pushed
- GitHub/GitLab/etc. provide free, reliable hosting
- Encryption for secrets can be layered on (age, GPG)
- Users already know git

**Cons:**
- Manual sync — user must remember to push/pull (or set up hooks)
- Merge conflicts are possible if two machines diverge
- Git is not installed everywhere by default (though it's close)
- Binary files (fonts, icons) bloat the repo over time

#### Option B: Cloud storage (OneDrive, iCloud, Dropbox)

Configuration files live in a cloud-synced folder. nostos reads from and
writes to this folder, and the cloud provider handles replication.

**Pros:**
- Automatic, transparent sync — no manual push/pull
- Works well with binary files
- Already available on most personal machines

**Cons:**
- No version history (or limited — OneDrive has some)
- Sync conflicts are opaque and hard to resolve
- Not available in all environments (CI, containers, ephemeral VMs)
- Ties the tool to a specific vendor ecosystem
- Harder to encrypt selectively

#### Option C: Hybrid — git primary, with optional cloud overlay

Git is the primary source of truth. An optional cloud-synced folder can
provide machine-specific overrides or secrets that shouldn't be in git.

**Pros:**
- Best of both worlds — versioned core config, convenient local overrides
- Cloud layer is optional, not required
- Secrets can live in the cloud layer (synced via OS keychain integration)

**Cons:**
- More complex mental model — "where does this setting live?"
- Two sync mechanisms to reason about
- Cloud layer behavior varies by provider

#### Recommendation

**Start with git as the sole storage mechanism.** It's universal, versioned,
and well-understood. nostos should make the git workflow frictionless
(automatic commit messages, easy push/pull commands). A cloud overlay can be
explored later if real need emerges.

### Execution model

How nostos applies configuration to a machine.

#### Option A: Purely declarative (convergent)

The user declares the desired state. nostos computes the diff between
current and desired state and applies the minimum set of changes to converge.
Running nostos twice in a row is a no-op (idempotent).

**Pros:**
- Predictable — you can always see what will change before it happens
- Idempotent — safe to run repeatedly
- Supports dry-run / plan mode naturally
- Easy to reason about the expected state of a machine

**Cons:**
- Harder to implement — requires modeling the current state of the system
- Some operations are inherently imperative (running a script, cloning a repo)
- Can be surprising if nostos removes something the user installed manually

#### Option B: Imperative (scripted)

The user writes scripts or commands. nostos runs them in order. Think
Makefile or shell scripts with some structure.

**Pros:**
- Simple to implement — just run commands
- Familiar to most developers
- Maximum flexibility — anything you can script, you can do

**Cons:**
- Not idempotent by default — running twice may break things
- No dry-run without significant effort
- Hard to know the expected state of a machine
- Error handling is the user's problem

#### Option C: Declarative with imperative escape hatches (hybrid)

Core operations (dotfile placement, package installation, plugin sync) are
declarative and convergent. For anything that doesn't fit the declarative
model, users can define `hooks` — imperative scripts that run at defined
points (before/after sync, on first setup, etc.).

**Pros:**
- Declarative for 90% of cases — safe, predictable, idempotent
- Escape hatches for the remaining 10% — no artificial limitations
- Hooks can be platform-conditional
- Encourages good practice while allowing pragmatism

**Cons:**
- Hooks can undermine the declarative guarantees if overused
- Slightly more complex config surface

#### Recommendation

**Hybrid: declarative core with imperative hooks.** The declarative model
is strictly better for the common cases (dotfiles, packages), and hooks
provide a clean escape valve for everything else.

### Execution order

When `nostos apply` runs, it processes items in a defined order:

```
1. Pre-apply hooks    (in declaration order from nostos.toml)
2. Dotfiles           (order does not matter — independent file copies)
3. Tools              (in declaration order from nostos.toml)
4. Post-apply hooks   (in declaration order from nostos.toml)
```

**Hooks** run in the order they appear in `nostos.toml`. This gives the
user explicit control over sequencing — for example, ensuring Homebrew
is installed before rustup, if rustup is also installed via a hook.

**Tools** are installed in declaration order. This matters when one tool
is a prerequisite for another — for example, `rustup` should be listed
before `cargo-edit` so that `cargo` is available when nostos tries to
run `cargo install cargo-edit`.

**Dotfiles** are applied as a batch with no guaranteed order. Since each
file copy is independent (one file's placement doesn't affect another),
ordering is irrelevant.

Within each phase, **platform and machine filtering** is applied first —
items that don't match the current platform or machine are skipped before
any ordering takes effect.

### Hooks

Hooks are imperative scripts that run at defined points during
`nostos apply`. They handle anything that doesn't fit the declarative
model — installing tools from GitHub Releases, running setup scripts,
configuring system settings, etc.

#### Hook lifecycle

Hooks run at specific points during `nostos apply`:

```
1. pre-apply hooks     ← before anything else
2. dotfiles applied    ← declarative
3. tools installed     ← declarative
4. post-apply hooks    ← after everything else
```

#### Hook definition

Hooks are defined in `nostos.toml` and point to scripts in the repo:

```toml
[[hook]]
name = "install-rustup"
run = "hooks/install-rustup.sh"
when = "pre-apply"
platforms = ["linux", "macos"]

[[hook]]
name = "install-rustup-windows"
run = "hooks/install-rustup.ps1"
when = "pre-apply"
platforms = ["windows"]

[[hook]]
name = "install-lazygit"
run = "hooks/install-lazygit.sh"
when = "post-apply"
platforms = ["linux"]

[[hook]]
name = "setup-ssh-agent"
run = "hooks/setup-ssh-agent.sh"
when = "post-apply"
platforms = ["linux", "macos"]
```

The hook scripts themselves live in the repo and are simple shell or
PowerShell scripts:

```shell
#!/bin/sh
# hooks/install-rustup.sh
if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
```

```shell
#!/bin/sh
# hooks/install-lazygit.sh
if ! command -v lazygit >/dev/null 2>&1; then
    LAZYGIT_VERSION=$(curl -s "https://api.github.com/repos/jesseduffield/lazygit/releases/latest" \
        | grep -Po '"tag_name": "v\K[^"]*')
    curl -Lo lazygit.tar.gz \
        "https://github.com/jesseduffield/lazygit/releases/latest/download/lazygit_${LAZYGIT_VERSION}_Linux_x86_64.tar.gz"
    tar xf lazygit.tar.gz lazygit
    install lazygit /usr/local/bin
    rm lazygit lazygit.tar.gz
fi
```

#### Hook properties

- **Idempotency is the script's responsibility** — nostos runs hooks
  every time `nostos apply` is called. Scripts should check whether their
  work is already done (e.g., `if ! command -v rustup` above).

- **Platform-conditional** — the `platforms` field controls which platforms
  a hook runs on, just like tools.

- **Machine-conditional** — hooks can also specify `machines = ["work-macbook"]`
  to run only on specific machines.

- **Failure handling** — if a hook script exits with a non-zero status,
  nostos reports the failure and continues with the remaining hooks (it
  does not abort the entire apply).

- **`nostos plan` shows hooks** — dry-run output lists which hooks would
  run, so the user can see the full picture before applying.

#### When to use hooks vs. tools

| Situation | Use |
|-----------|-----|
| Tool available in a package manager | `[[tool]]` — declarative |
| Tool available via `cargo install`, `go install`, `pip install` | `[[tool]]` with appropriate install strategy |
| Tool only available as a GitHub Release binary | Hook script |
| Tool requires a curl-pipe-sh installer (rustup, nvm) | Hook script |
| One-time system setup (SSH agent, font installation) | Hook script |
| Anything requiring interactive input or complex logic | Hook script |

## Platform and machine targeting

A core requirement is that configuration can vary by platform and by
individual machine. nostos should support a layering model:

```
base config          ← applies everywhere
  └─ platform layer  ← applies to all machines of a given OS
       └─ machine layer  ← applies to a specific machine only
```

Later layers override earlier ones. This is conceptually similar to how
CSS cascading works, or how Ansible uses host/group vars.

### Platform detection

nostos should detect the current platform automatically using a combination
of:

- Operating system (linux, macos, windows)
- Architecture (x86_64, aarch64)
- Distro/variant (ubuntu, fedora, arch — on Linux)
- Available package managers (brew, apt, dnf, pacman, winget, etc.)

### Linux distro handling

"Linux" is not a single platform for package management purposes.
Different distros use different package managers and different package
names for the same logical software. For example, the OpenSSL development
library is:

| Distro | Package manager | Package name |
|--------|----------------|--------------|
| Ubuntu/Debian | apt | `libssl-dev` |
| Fedora/RHEL | dnf | `openssl-devel` |
| Arch | pacman | `openssl` |

nostos handles this through two mechanisms:

**1. Per-manager install mappings (already in the tool model):**

The tool config supports different package names per manager. The resolver
picks the right one based on which manager is available on the current
system:

```toml
[[tool]]
name = "openssl-dev"
install.apt = "libssl-dev"
install.dnf = "openssl-devel"
install.pacman = "openssl"
```

This implicitly targets the right distro — if `apt` is available, the
system is Debian-based; if `dnf` is available, it's Fedora/RHEL. No
explicit distro detection is needed for tool installation.

**2. Per-distro installer preferences:**

The `[preferences]` section supports distro-level granularity on Linux:

```toml
[preferences.macos]
installer-priority = ["brew", "cargo"]

[preferences.windows]
installer-priority = ["winget", "scoop", "cargo"]

# Linux preferences can be specified per-distro
[preferences.linux.ubuntu]
installer-priority = ["apt", "cargo"]

[preferences.linux.fedora]
installer-priority = ["dnf", "cargo"]

[preferences.linux.arch]
installer-priority = ["pacman", "cargo"]

# Fallback for unrecognized Linux distros
[preferences.linux]
installer-priority = ["cargo"]
```

nostos detects the Linux distribution (via `/etc/os-release` or
equivalent) and uses the most specific matching preference. If no
distro-specific preference is defined, it falls back to the generic
`[preferences.linux]`.

**Why not distro-level `platforms` filtering?**

The `platforms` field on tools (`platforms = ["linux"]`) intentionally
stays at the OS level, not the distro level. Distro-specific behavior
is handled entirely through install mappings — a tool with only
`install.apt` naturally won't install on a Fedora system that only has
`dnf`. If a tool genuinely shouldn't be installed on any Linux system
(e.g., it's macOS-only), `platforms = ["macos"]` handles that. There's
no need for `platforms = ["linux.ubuntu"]` because the installer mapping
already provides that specificity.

### Machine identity

Each machine gets a stable identity — either auto-detected (hostname) or
explicitly assigned by the user on first run. This identity keys into the
machine-specific override layer.

## Proposed UX

```
# First-time setup: clone config repo and apply
nostos init https://github.com/user/dotfiles.git

# Apply current configuration (converge to desired state)
nostos apply

# Show what would change without applying
nostos plan

# Add a new dotfile to be managed
nostos track ~/.config/starship.toml

# Sync: commit local changes, pull remote changes, apply
nostos sync

# Show current machine identity and platform info
nostos status
```

## Usage examples

### Setting up a new machine from scratch

You just got a new MacBook. You want your full development environment:

```shell
# Install nostos (pre-built binary from GitHub Releases)
curl -fsSL https://github.com/benferse/nostos/releases/latest/download/nostos-macos-arm64 \
  -o /usr/local/bin/nostos
chmod +x /usr/local/bin/nostos

# Clone your config repo and apply everything
# (no external git needed — nostos uses embedded libgit2)
nostos init https://github.com/user/dotfiles.git

# nostos detects: macOS, aarch64
# It runs pre-apply hooks, then applies dotfiles and tools:
#   ▶ Running pre-apply hook: install-homebrew
#     Homebrew not found, installing...
#     ✓ Homebrew installed
#   ✓ Copied bashrc → ~/.bashrc
#   ✓ Copied config/starship.toml → ~/.config/starship.toml
#   ✓ Copied config/alacritty/alacritty.toml → ~/.config/alacritty/alacritty.toml
#       (using macos override)
#   ✓ Installed ripgrep via brew
#   ✓ Installed fd via brew
#   ✓ Installed cargo-edit via cargo
#   ✓ Skipped build-essential (linux only)
```

### Repository layout

nostos uses a convention where dotfiles are stored without their leading
dot. When applying, nostos prepends a `.` to each top-level entry in the
dotfiles directory. This keeps files visible in normal directory listings
and file browsers.

```
my-dotfiles/                       # the nostos config repo
├── nostos.toml                    # nostos configuration
├── hooks/                         # imperative hook scripts
│   ├── install-homebrew.sh        # installs Homebrew on macOS
│   └── install-rustup.sh          # installs rustup on Unix
├── dotfiles/                      # auto-prepend convention
│   ├── bashrc                     # → ~/.bashrc
│   ├── gitconfig                  # → ~/.gitconfig
│   ├── config/                    # → ~/.config/
│   │   ├── starship.toml          # → ~/.config/starship.toml
│   │   └── alacritty/
│   │       └── alacritty.toml     # → ~/.config/alacritty/alacritty.toml
│   └── ssh/                       # → ~/.ssh/
│       └── config                 # → ~/.ssh/config
├── files/                         # verbatim copy (no dot-prepend)
│   └── bin/
│       └── git-cleanup            # → ~/bin/git-cleanup
├── macos/                         # platform-specific overrides
│   └── config/
│       └── alacritty/
│           └── alacritty.toml     # → ~/.config/alacritty/alacritty.toml (on macOS)
├── linux/
│   └── config/
│       └── alacritty/
│           └── alacritty.toml     # → ~/.config/alacritty/alacritty.toml (on Linux)
└── machines/
    └── work-macbook/
        └── gitconfig.local        # → ~/.gitconfig.local (on work-macbook only)
```

The corresponding `nostos.toml`:

```toml
[dotfiles]
source = "dotfiles/"
target = "~"

[dotfiles.platforms.macos]
"config/alacritty/alacritty.toml" = "macos/config/alacritty/alacritty.toml"

[dotfiles.platforms.linux]
"config/alacritty/alacritty.toml" = "linux/config/alacritty/alacritty.toml"

[files]
source = "files/"
target = "~"

[dotfiles.machines.work-macbook]
"gitconfig.local" = "machines/work-macbook/gitconfig.local"

# Pre-apply hooks run before dotfiles and tools — use for bootstrapping
# package managers and other prerequisites
[[hook]]
name = "install-homebrew"
run = "hooks/install-homebrew.sh"
when = "pre-apply"
platforms = ["macos"]

[[hook]]
name = "install-rustup"
run = "hooks/install-rustup.sh"
when = "pre-apply"
platforms = ["linux", "macos"]

[preferences.macos]
installer-priority = ["brew", "cargo"]

[preferences.linux.ubuntu]
installer-priority = ["apt", "cargo"]

[preferences.linux.fedora]
installer-priority = ["dnf", "cargo"]

[preferences.linux.arch]
installer-priority = ["pacman", "cargo"]

[preferences.windows]
installer-priority = ["winget", "scoop", "cargo"]

[[tool]]
name = "ripgrep"

[[tool]]
name = "fd"
install.apt = "fd-find"
install.cargo = "fd-find"

[[tool]]
name = "jq"

[[tool]]
name = "bat"

[[tool]]
name = "cargo-edit"
install.cargo = "cargo-edit"

[[tool]]
name = "cargo-watch"
install.cargo = "cargo-watch"

[[tool]]
name = "build-essential"
install.apt = "build-essential"
install.dnf = "gcc"
install.pacman = "base-devel"
platforms = ["linux"]
```

### Adding a new tool to your setup

You discover a new CLI tool and want it on all your machines:

```shell
# Edit nostos.toml to add:
#   [[tool]]
#   name = "zoxide"

# Preview what nostos would do
nostos plan
#   Tools:
#     zoxide — not installed, would install via brew

# Apply the change on this machine
nostos apply
#   ✓ Installed zoxide via brew

# Commit and push so other machines pick it up
nostos sync
#   Committed: "Add zoxide"
#   Pushed to origin/main
```

On your next machine, `nostos sync` (or `nostos apply` after a git pull)
installs zoxide using whatever package manager is available there.

### Handling a tool with different package names

`fd` is called `fd` on most package managers but `fd-find` on apt:

```toml
[[tool]]
name = "fd"
install.apt = "fd-find"
install.cargo = "fd-find"

# No need to specify brew, winget, pacman, etc. — nostos uses "fd"
# (the tool name) as the default package name for any manager not
# explicitly listed.
```

### Tracking a config change you made locally

You tweaked your starship prompt on your laptop and want to capture it:

```shell
# You edited ~/.config/starship.toml directly
# nostos detects the local modification:
nostos status
#   Modified: config/starship.toml (local changes not in repo)

# Copy the changed file back into the repo (strips the dot)
nostos track ~/.config/starship.toml
#   Updated dotfiles/config/starship.toml

# Commit and push
nostos sync
#   Committed: "Update starship.toml"
#   Pushed to origin/main
```

### Dry run — seeing what nostos would do

Before applying, you can preview all changes:

```shell
nostos plan
#   Dotfiles:
#     bashrc                           — up to date
#     config/starship.toml             — clean update (repo changed)
#     config/alacritty/alacritty.toml  — conflict (both sides changed)
#                                        will back up existing file
#   Files:
#     bin/git-cleanup                  — up to date
#   Tools:
#     ripgrep                          — installed (brew)
#     fd                               — not installed, would install via apt
#     zoxide                           — not installed, would install via apt
#     build-essential                   — installed (apt)
```

### Machine-specific overrides

Your work laptop needs a corporate proxy configured in git:

```toml
# In nostos.toml — only applied on the machine named "work-macbook":
[dotfiles.machines.work-macbook]
"gitconfig.local" = "machines/work-macbook/gitconfig.local"
```

Your main `.gitconfig` includes it conditionally:

```ini
# In dotfiles/gitconfig:
[include]
    path = ~/.gitconfig.local
```

On machines without a `gitconfig.local` override, the include is silently
ignored by git.

## Architecture sketch

```
┌─────────────────────────────────────────────┐
│                  CLI (clap)                 │
├─────────────────────────────────────────────┤
│              Config Parser (TOML)           │
│         ┌───────────────────────┐           │
│         │   Layering / Merge    │           │
│         │  base → platform →    │           │
│         │      machine          │           │
│         └───────────────────────┘           │
├─────────────────────────────────────────────┤
│              Reconciler                     │
│  ┌──────────────┐   ┌──────────────────┐    │
│  │   Dotfiles   │   │  Tool Resolver   │    │
│  │    Module    │   │                  │    │
│  └──────────────┘   └───────┬──────────┘    │
│                             │               │
│              ┌──────────────┴────────────┐  │
│              │    Installer Registry     │  │
│              │  (brew, apt, winget,      │  │
│              │   cargo, scoop, ...)      │  │
│              └───────────────────────────┘  │
├─────────────────────────────────────────────┤
│         Platform Abstraction Layer          │
│   (OS detection, package manager discovery, │
│    installer preference resolution)         │
├─────────────────────────────────────────────┤
│              Git Backend                    │
│           (libgit2 via git2 crate)          │
└─────────────────────────────────────────────┘
```

Key components:

- **Dotfiles Module** — manages configuration files via the copy-based
  placement strategy. Handles conflict detection using content hashes,
  platform/machine-specific file overrides, and reverse sync via
  `nostos track`.

- **Tool Resolver** — takes a logical tool definition (name + install
  strategies) and resolves it to a concrete install action for the current
  machine, based on which package managers are available and the user's
  preference order.

- **Installer Registry** — a set of adapters, one per package manager.
  Each adapter knows how to: check if a package is already installed,
  install a package, and (optionally) remove a package. Adding support for
  a new package manager means implementing one adapter.

- **Platform Abstraction Layer** — detects OS, architecture, distro,
  available package managers, and machine identity. This is the foundation
  that the resolver and installer registry build on.

## File placement and conflict resolution

### How are config files applied?

There are two fundamental approaches to placing managed files on disk.

#### Option A: Symlinks

The target file (e.g., `~/.bashrc`) is a symlink pointing to the managed
file inside the nostos repository clone.

```
~/.bashrc → ~/nostos-repo/dotfiles/.bashrc
```

**Pros:**
- Edits to `~/.bashrc` are immediately reflected in the repo — no
  "forgot to sync" problem
- No conflict detection needed — there's only one copy of the file
- Simple mental model for file changes flowing back

**Cons:**
- The nostos repo must remain on disk and accessible at all times
- Windows requires Developer Mode or elevated privileges for symlinks,
  and some Windows apps don't follow symlinks correctly
- Some applications detect and refuse to operate on symlinked config
  files (security sandboxes, some editors)
- `nostos track` would need to move the original file into the repo
  and replace it with a symlink — destructive if interrupted
- Platform-specific overrides require changing which file the symlink
  points to, which means recreating symlinks on `nostos apply`

#### Option B: Copies

nostos copies managed files from the repository to their target locations.
The repo and the target are independent after copying.

```
~/nostos-repo/dotfiles/.bashrc  →(copy)→  ~/.bashrc
```

**Pros:**
- Works identically on all platforms — no symlink support needed
- Files remain functional even if the nostos repo is deleted or moved
- Applications never see a symlink — maximum compatibility
- Platform-specific overrides are just "copy a different source file"
- Robust against interruption — a failed copy doesn't destroy the original

**Cons:**
- Changes made directly to `~/.bashrc` don't flow back to the repo
- Conflict detection is needed to avoid overwriting user changes
- Two copies of every file (repo + target) — minor disk overhead

#### Recommendation

**Use copies.** The cross-platform requirement (especially Windows) makes
symlinks unreliable. Copies are universally portable, more robust against
failure, and compatible with all applications. The downside — needing
conflict detection — is solvable, and the solution is useful in its own
right (it enables `nostos plan` to show exactly what will change).

### Conflict detection

Since nostos uses copies, it needs to detect when a target file has been
modified outside of nostos (i.e., the user edited `~/.bashrc` directly).
This requires tracking what nostos last applied.

**State tracking approach:**

nostos maintains a local state file (not synced to git) that records, for
each managed file, the hash of the content that was last applied:

```toml
# ~/.config/nostos/state.toml (local, not synced)
[applied]
".bashrc" = { hash = "sha256:abc123...", timestamp = "2026-04-26T20:00:00Z" }
".config/starship.toml" = { hash = "sha256:def456...", timestamp = "2026-04-25T10:30:00Z" }
```

On `nostos apply`, for each managed file, there are four possible states:

| Target file on disk | Repo source file | Situation | Action |
|---------------------|------------------|-----------|--------|
| Matches last-applied hash | Same as last apply | **No change** | Skip — already up to date |
| Matches last-applied hash | Different from last apply | **Clean update** | Copy new version from repo, update state |
| Differs from last-applied hash | Same as last apply | **Local modification** | User changed the file — warn and skip (or offer to overwrite) |
| Differs from last-applied hash | Different from last apply | **Conflict** | Both sides changed — back up the target, apply repo version, warn user |
| Does not exist | Exists in repo | **New file** | Copy from repo, record in state |

The `nostos plan` command performs this analysis without applying changes,
showing the user exactly what would happen.

**Backing up conflicts:**

When a conflict is detected, nostos saves the user's version before
overwriting:

```
~/.bashrc          ← replaced with repo version
~/.bashrc.nostos-backup-20260426  ← user's version preserved
```

This ensures no user work is ever lost, even in the worst case.

**Reverse sync (tracking changes back):**

When a user edits a managed file directly and wants to capture that change
in the repo, they run:

```shell
nostos track ~/.bashrc   # copies the current file back into the repo
```

This updates the repo source and the applied-state hash, bringing
everything back in sync.

## Resolved decisions

- **Bootstrap** — nostos is a single static binary distributed via GitHub
  Releases. It embeds git support via libgit2 (`git2` crate), so no
  external `git` command is needed. Users with Rust installed can also
  `cargo install nostos`. The full bootstrap flow for a fresh machine is:

  1. Download the nostos binary (`curl` or web browser)
  2. `nostos init <url>` clones the config repo (via embedded libgit2)
  3. Pre-apply hooks run (install Homebrew, etc.)
  4. Dotfiles and tools are applied

  The only prerequisite on a fresh machine is the ability to download
  the nostos binary. Everything else — git, package managers, tools —
  flows from there.

- **Plugin system** — nostos will not support third-party plugins or
  extensions. New package manager support or features are added directly
  to the nostos codebase. This keeps the tool simple and avoids the
  complexity of an extension API.

- **Rollback** — no dedicated rollback mechanism. Rolling back config is
  handled by git (`git revert` + `nostos apply`), which naturally triggers
  a clean update via the conflict detection system. Conflict backups
  provide an additional safety net. Package uninstallation is out of
  scope — removing a tool from config simply means nostos stops managing
  it, but does not uninstall it.

- **Templating** — nostos will not support variable substitution or
  templating in dotfiles. Files are managed as-is. All platform and
  machine variation is handled through the layering system (different
  source files per platform/machine, not templates with conditionals).

- **Profiles / tags** — nostos will not support named profiles or
  tag-based filtering. The base → platform → machine layering model is
  sufficient for all variation needs.
