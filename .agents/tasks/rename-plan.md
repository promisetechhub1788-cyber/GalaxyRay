# GalaxyRay → GalaxyRay: Project Rename Plan

**Project Path:** `c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay`  
**Target Directory Name:** `c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay`  
**Working Directory:** `c:\Users\LENOVO PC\Desktop\New folder (8)`

---

## Phase 1: Find and Replace Mapping

### Case/Style Variants and Targets

| Variant | Target | Count | Context |
|---------|--------|-------|---------|
| `GalaxyRay` | `GalaxyRay` | 76+ | Project name in docs, badges, GitHub URLs (readme), config, comments |
| `GalaxyRay` | `galaxyray` | 52+ | Cargo package name, binary name, env file names, Docker references, function names |
| `galaxy-ray` | `galaxy-ray` | 15+ | kebab-case in URLs, config files, environment variables |
| `galaxy_ray` | `galaxy_ray` | 8+ | snake_case in environment variable names, function params, test names |
| `GalaxyRay` | `GALAXYRAY` | 3+ | All-caps in env vars, Docker container names, metrics names |
| `galaxy_ray` | `GALAXY_RAY` | 7+ | All-caps snake_case in env variable names |
| `Galaxy Ray` | `Galaxy Ray` | 2 | Human-readable display text (headers, comments) |

**Note:** Environment variables containing these strings should be renamed. This is intentional—existing user `.env` files will need manual updates (expected behavior).

---

## Phase 2: Files Containing Occurrences (Grouped by Variant)

### Files with `GalaxyRay` (primary case variant)

| File | Occurrence Count | Sample Context |
|------|------------------|---|
| README.md | 8 | `# GalaxyRay`, badge URLs, GitHub links |
| Cargo.toml | 2 | `repository`, `homepage` GitHub URLs |
| Cargo.lock | 1 | `name = "GalaxyRay"` inside lock entry |
| CHANGELOG.md | 10+ | Historical release notes (EXCLUSION: preserve release entries verbatim) |
| docker-compose.yml | 1 | `GalaxyRay_data` volume name |
| docker-compose.prod.yml | Similar | Production variant |
| Dockerfile | 5 | Binary path, user creation, ENV vars |
| LICENSE | 1 | Copyright/attribution |
| CODE_OF_CONDUCT.md | 2 | Email `conduct@GalaxyRay.dev`, GitHub repo link |
| CONTRIBUTING.md | 1 | Setup instructions reference |
| SECURITY.md | 1 | Security policy URL |
| MIGRATIONS.md | 1 | Project reference |
| DEPLOYMENT.md | 1 | Project reference |
| WEBHOOK_REFERENCE.md | 1 | Header reference |
| deny.toml | 1 | `# cargo-deny configuration for GalaxyRay` |
| .github/workflows/ci.yml | Multiple | Docker image names, binary references |
| .github/workflows/dashboard.yml | Multiple | Similar |
| .github/workflows/release.yml | Multiple | `ghcr.io/GalaxyRaylabs/GalaxyRay` image |
| .github/workflows/supply-chain.yml | Multiple | Image and binary references |
| .github/ISSUE_TEMPLATE/bug_report.yml | 1 | Version placeholder |
| .github/ISSUE_TEMPLATE/feature_request.yml | 1 | Description text |
| .github/ISSUE_TEMPLATE/config.yml | 3 | GitHub URLs |
| openapi.yaml | 6+ | API server description, title, contact info |
| redocly.yaml | 1 | Spec reference |
| .env.example | 1 | Comment line reference |
| Caddyfile (deploy/) | 1 | Reverse proxy config comment/host |
| package.json | 1 | Project name in JSON metadata |
| package-lock.json | 1 | Lock file entry |
| README.md (in static/) | Possibly | Dashboard docs |
| *.rs files | 40+ | doc-tests, doctests in comments, module-level docs |
| *.js files | 8+ | Dashboard UI references, comments |
| *.html files | 8+ | Dashboard HTML templates |

### Files with `GalaxyRay` (lowercase package name)

| File | Context |
|------|---------|
| Cargo.toml | `name = "GalaxyRay"` (CRITICAL: Cargo package name) |
| Cargo.lock | Package entries |
| .env.example | `DATABASE_URL=sqlite:GalaxyRay.db`, comment references |
| docker-compose.yml | `GalaxyRay_data` volume, `DATABASE_URL=sqlite:///data/GalaxyRay.db` |
| docker-compose.prod.yml | Similar |
| Dockerfile | `USER GalaxyRay`, `/usr/local/bin/GalaxyRay` binary path, `CMD [...GalaxyRay...]` |
| .gitignore | `GalaxyRay.db*`, `deploy/GalaxyRay.env` |
| .dockerignore | Similar patterns |
| deploy/GalaxyRay.env.example | File name and env var prefix references (rename file too) |
| src/main.rs | Binary/crate references |
| src/lib.rs | Crate-level documentation |
| All test files (*.rs in tests/) | `use GalaxyRay::` imports |
| examples/dump_schema.rs | `use GalaxyRay::` |
| Dockerfile | User name, binary references |
| .github/workflows/*.yml | Docker image, binary run commands |

### Files with `galaxy_ray` (snake_case)

| File | Context |
|------|---------|
| .env.example | Comments with variable naming pattern |
| config.rs | Function/struct documentation |
| Cargo.lock | Some lock entries |
| horizon.rs | Code comments, variable names |
| stream_tests.rs | Function names, variable names |
| strkey.rs | Similar |
| playwright.config.js | Configuration property |
| README.md | Code snippet comments |

### Files with `galaxy-ray` or `galaxy_ray` or `GalaxyRay`

Detected in various files; see comprehensive grep results above.

---

## Phase 3: Files Requiring Rename (git mv)

### Files to Rename

| Current Path | New Path | Reason |
|---|---|---|
| `deploy/GalaxyRay.env.example` | `deploy/galaxyray.env.example` | Filename contains old project name |

### Cross-References to Renamed Files (Must Update)

These files reference the old env filename and must be updated:

- `.gitignore` — contains `deploy/GalaxyRay.env` reference
- `.dockerignore` — may contain similar patterns
- `.github/workflows/*.yml` — may reference the env file
- docker-compose files — may reference the env file in env_file directives
- DEPLOYMENT.md — may document the env file path
- README.md — may mention deploy/GalaxyRay.env setup

---

## Phase 4: Cargo Package and Binary Names (CRITICAL)

**Cargo Package Name:**
- **Current:** `GalaxyRay` (in `Cargo.toml` under `[package] name`)
- **New:** `galaxyray`
- **Impact:** Changes internal crate reference from `GalaxyRay::` to `galaxyray::`

**Binary Names:**
- **Current:** `[[bin]] name = "GalaxyRay"` → `[[bin]] name = "galaxyray"`
- **Files to Update:** `Cargo.toml`
- **Downstream:** Docker commands, `.github/workflows/`, shell scripts

**Internal Crate References Requiring Update:**

Files using `use GalaxyRay::` or `GalaxyRay::` paths (ALL MUST CHANGE to `use galaxyray::` or `galaxyray::`):

1. `tests/write_throughput_bench.rs` — `use GalaxyRay::db`
2. `tests/webhook_dispatch_tests.rs` — `use GalaxyRay::{...}`
3. `src/bin/audit_undercredit.rs` — `use GalaxyRay::{audit, config::Config}`
4. `tests/trustline_tests.rs` — `use GalaxyRay::{...}`
5. `tests/stream_tests.rs` — `use GalaxyRay::{...}`
6. `tests/sqlx_type_mapping_tests.rs` — `use GalaxyRay::db`
7. `tests/schema_snapshot_test.rs` — `use GalaxyRay::db`
8. `tests/rate_limit_tests.rs` — `use GalaxyRay::{...}`
9. `tests/openapi_contract.rs` — `use GalaxyRay::{...}`
10. `tests/multi_op_transaction_tests.rs` — `use GalaxyRay::{...}`
11. `tests/multi_op_credit_tests.rs` — `use GalaxyRay::{...}`
12. `tests/horizon_stream_idle_tests.rs` — `use GalaxyRay::{...}`
13. `src/money.rs` — doc-test with `use GalaxyRay::money::parse_stroops`
14. All other `*.rs` test/example files with `GalaxyRay::` references

Count: **~60+ occurrences across 20+ Rust files**

---

## Phase 5: Complete File-by-File Change List

### Directory Renames

- `c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay` → `c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay` (deferred to final step; use `git mv` after file changes complete)

### Individual File Content Changes

#### Cargo.toml
- `[package] name = "GalaxyRay"` → `name = "galaxyray"`
- `[[bin]] name = "GalaxyRay"` → `name = "galaxyray"`
- `repository = "https://github.com/GalaxyRayLabs/GalaxyRay"` → NO CHANGE (GitHub repo rename is manual; excluded per rules)
- `homepage = "https://github.com/GalaxyRayLabs/GalaxyRay"` → NO CHANGE (same reason)

#### README.md
- `# GalaxyRay` → `# GalaxyRay`
- All GitHub badge URLs: keep as-is (GitHub repo rename manual)
- All doc links to `galaxy_ray` environment variables → `galaxy_ray`
- All references to `GalaxyRay` in text → `GalaxyRay`
- All references to `GalaxyRay` package → `galaxyray`
- All references to `galaxy_ray` config naming → `galaxy_ray`

#### Dockerfile
- Line with `RUN useradd -r -u 1001 -U GalaxyRay` → `RUN useradd -r -u 1001 -U galaxyray`
- Line with `COPY --from=builder /app/target/release/GalaxyRay /usr/local/bin/GalaxyRay` → `COPY --from=builder /app/target/release/galaxyray /usr/local/bin/galaxyray`
- Line with `COPY --from=builder /app/target/release/GalaxyRay /usr/local/bin/GalaxyRay` → (same, second occurrence)
- `USER GalaxyRay` → `USER galaxyray`
- `ENV DATABASE_URL=sqlite:///data/GalaxyRay.db` → `ENV DATABASE_URL=sqlite:///data/galaxyray.db`
- `HEALTHCHECK ... CMD ["/usr/local/bin/GalaxyRay", "healthcheck", "health"]` → `CMD ["/usr/local/bin/galaxyray", "healthcheck", "health"]`
- `CMD ["/usr/local/bin/GalaxyRay"]` → `CMD ["/usr/local/bin/galaxyray"]`

#### docker-compose.yml
- `GalaxyRay_data:/data` volume name → `galaxyray_data:/data`
- `DATABASE_URL: sqlite:///data/GalaxyRay.db` → `DATABASE_URL: sqlite:///data/galaxyray.db`
- Reference to `volumes: GalaxyRay_data:` → `volumes: galaxyray_data:`

#### docker-compose.prod.yml
- Similar changes to docker-compose.yml

#### .gitignore
- `GalaxyRay.db*` → `galaxyray.db*`
- `deploy/GalaxyRay.env` → `deploy/galaxyray.env`

#### .dockerignore
- Similar patterns as .gitignore

#### .env.example
- `DATABASE_URL=sqlite:GalaxyRay.db` → `DATABASE_URL=sqlite:galaxyray.db`
- Comment references to `GalaxyRay` → `GalaxyRay`

#### deploy/GalaxyRay.env.example
- Rename file: `deploy/GalaxyRay.env.example` → `deploy/galaxyray.env.example`
- Inside file: all `galaxy_ray_*` or `GalaxyRay_*` env var names → `GALAXY_RAY_*` / `GALAXYRAY_*`
- References to `GalaxyRay` in comments → `galaxyray`
- `DATABASE_URL=sqlite:GalaxyRay.db` → `DATABASE_URL=sqlite:galaxyray.db` (if present)

#### All .rs files (source, tests, examples)
- `use GalaxyRay::` → `use galaxyray::`
- `GalaxyRay::module::path` → `galaxyray::module::path` (all occurrences)
- `GalaxyRay_` prefix in variable names, function names → `galaxyray_` (context-dependent: metric names like `GalaxyRay_db_file_size_bytes` → `galaxyray_db_file_size_bytes`)
- Doc-test examples: update paths

#### All .js / .html files (dashboard)
- Comment references to `GalaxyRay` → `GalaxyRay`
- String literals referencing the project → update accordingly
- Variable/class names containing `GalaxyRay` → `galaxyray` (if user-visible, may need UI review)

#### openapi.yaml
- `title: GalaxyRay` → `title: GalaxyRay`
- `description: "GalaxyRay is..."` → update to `GalaxyRay is...`
- Contact info references → update

#### Cargo.lock
- Automatically regenerated by `cargo build`, but currently shows old name. Will be regenerated.

#### CHANGELOG.md
- **EXCLUSION:** Leave historical entries verbatim. Only update the top-level project title if a summary exists at the very top.

#### CODE_OF_CONDUCT.md
- `conduct@GalaxyRay.dev` email → `conduct@galaxyray.dev` (if this email actually exists and is being updated)
- `GalaxyRay` references → `GalaxyRay`

#### SECURITY.md
- `GalaxyRay` references → `GalaxyRay`

#### .github/workflows/ci.yml
- Image references: `GalaxyRay` → `galaxyray`
- Binary references: `GalaxyRay` → `galaxyray`
- Job naming, comments: update accordingly

#### .github/workflows/dashboard.yml
- Similar to ci.yml

#### .github/workflows/release.yml
- `ghcr.io/GalaxyRaylabs/GalaxyRay` → `ghcr.io/GalaxyRaylabs/galaxyray` (NOTE: org name stays same; only project name changes in image path)
- `GalaxyRay_VERSION` env var → `GALAXYRAY_VERSION`
- Binary name references

#### .github/workflows/supply-chain.yml
- Image references, binary references

#### deny.toml
- Top comment: `# cargo-deny configuration for GalaxyRay` → `# cargo-deny configuration for GalaxyRay`

#### package.json (if it exists)
- Project name metadata: `"GalaxyRay"` → `"galaxyray"`

#### playwright.config.js (if e2e tests exist)
- `galaxy_ray` references in config → `galaxy_ray`

#### Other Documentation Files
- LICENSE, CONTRIBUTING.md, MIGRATIONS.md, DEPLOYMENT.md, WEBHOOK_REFERENCE.md, etc.
- Replace `GalaxyRay` → `GalaxyRay`
- Replace `galaxy_ray` → `galaxy_ray`
- Replace `GalaxyRay` → `galaxyray` (in comments, not in historical examples that should stay verbatim)

---

## Phase 6: Verification Commands

### Pre-Build Verification

1. **Check that all old variants are removed (excluding .git and CHANGELOG historical entries):**

```bash
cd "c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay"

# Case-insensitive search for old names, excluding .git
Get-ChildItem -Recurse -Exclude ".git" -File -ErrorAction SilentlyContinue |
  Select-String -Pattern "GalaxyRay|GalaxyRay|GalaxyRay|galaxy_ray|galaxy_ray|galaxy-ray" -ErrorAction SilentlyContinue |
  Where-Object { $_.FileName -notlike "*CHANGELOG.md*" } |
  Select-Object FileName, LineNumber, Line

# Expected output: NO matches (except in CHANGELOG.md which we intentionally exclude)
```

### Build Verification

2. **Verify Cargo builds successfully:**

```bash
cd "c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay"
cargo build --manifest-path Cargo.toml
```

Expected: Build succeeds with no errors. The binary should be named `galaxyray` (or `galaxyray.exe` on Windows).

3. **Run tests:**

```bash
cd "c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay"
cargo test --manifest-path Cargo.toml
```

Expected: All tests pass. All `use GalaxyRay::` references have been updated to `use galaxyray::`.

4. **Verify the binary name:**

```bash
ls -Recurse target/debug | Where-Object { $_.Name -match "^galaxyray" }
ls -Recurse target/release | Where-Object { $_.Name -match "^galaxyray" }
```

Expected: Binary files named `galaxyray` or `galaxyray.exe` are present.

5. **Check Docker image builds (if applicable):**

```bash
cd "c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay"
docker build -t galaxyray:latest .
```

Expected: Docker build succeeds.

6. **Verify no git-ignored or historical files were accidentally modified:**

```bash
cd "c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay"
git status
```

Expected: Only tracked files show as modified. The `.git/` directory remains unchanged. `.env` or other gitignored files don't appear (they're excluded by .gitignore rules).

---

## Phase 7: Summary of Changes

| Category | Count | Notes |
|----------|-------|-------|
| **Files with content changes** | 60+ | Text replacements across docs, configs, code |
| **Rust crate references updated** | ~60+ | `GalaxyRay::` → `galaxyray::` in all .rs files |
| **File renames (git mv)** | 1 | `deploy/GalaxyRay.env.example` → `deploy/galaxyray.env.example` |
| **Directory renames (git mv)** | 1 | `GalaxyRay` → `GalaxyRay` (parent dir rename) |
| **Environment variable renames** | 7+ | `galaxy_ray_*` / `GalaxyRay_*` → `GALAXY_RAY_*` / `GALAXYRAY_*` |
| **Docker volume/container name changes** | 2+ | `GalaxyRay_data` → `galaxyray_data`, user name, etc. |
| **GitHub URLs** | (not modified) | GitHub repo rename is manual; URLs in docs stay as-is during this step |
| **CHANGELOG.md historical entries** | (preserved) | Old release notes kept verbatim per exclusion rule |

---

## Implementation Order (Sequential)

The implementer should follow this order to keep the codebase buildable at each step:

1. **Update Cargo.toml** — change package name and binary name (allows cargo commands to work)
2. **Update all Rust source files** — replace `GalaxyRay::` with `galaxyray::` in all .rs files
3. **Update Cargo.lock** — will auto-regenerate; commit after running `cargo build`
4. **Update documentation and config files** — README.md, .env.example, deploy files, docker-compose, Dockerfile
5. **Update GitHub workflows and deployment scripts** — .github/workflows/*.yml, deploy/ scripts
6. **Update dashboard and static files** — .js, .html, package.json, playwright.config.js
7. **Rename file** — `git mv deploy/GalaxyRay.env.example deploy/galaxyray.env.example`
8. **Verify build** — `cargo build`, `cargo test`
9. **Verify no old references remain** — grep for lingering GalaxyRay variants (excluding CHANGELOG.md and .git/)
10. **Rename directory** — `git mv /path/GalaxyRay /path/GalaxyRay` (FINAL STEP, after all file changes committed)

---

## Exclusions (Per Requirements)

1. ✅ **CHANGELOG.md** — Historical release entries left verbatim. Only top-level title updated if present.
2. ✅ **GitHub repo URLs** — `GalaxyRayLabs/GalaxyRay` GitHub URLs remain unchanged (GitHub repo rename is a separate manual step outside this workflow).
3. ✅ **.git/ directory** — Untouched by find/replace operations.
4. ✅ **Third-party trademarks** — Not present; no exclusions needed.
5. ✅ **Environment variables** — Intentionally renamed; users will need to update their `.env` files (expected).

---

## Notes for Implementer

- **Windows Paths:** Use PowerShell with proper quoting: `"c:\Users\LENOVO PC\Desktop\New folder (8)\GalaxyRay\path\to\file"`
- **Git Commands:** Use `git mv` for file/directory renames to preserve history.
- **Cargo Package Name:** Once Cargo.toml is updated, `cargo` commands must run in the renamed directory context.
- **Buildability:** After each logical group (e.g., after Rust file updates), run `cargo build` to verify.
- **Testing:** Run `cargo test` frequently to catch import errors early.
- **Documentation:** Update paths in docs if they reference `/GalaxyRay/` → `/GalaxyRay/`.

