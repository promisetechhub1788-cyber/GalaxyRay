# Project Rename: StellarGate → GalaxyRay

## Summary

This commit renames the project from StellarGate to GalaxyRay across configuration, deployment, build, and source files. The scope is substantial: 67 files modified, 1 file renamed via `git mv`, with changes spanning Cargo package metadata, environment variables, Docker references, documentation, and test configuration.

**Watch for:** The environment variable renaming introduced critical malformations: `galaxy_rayWAY_PUBLIC` and `galaxy_rayWAY_SECRET` instead of properly formed names. Additionally, GitHub workflow files still reference `STELLAR_GATEWAY_PUBLIC` which was not renamed. These inconsistencies will break runtime configuration.

**Verdict**: CHANGES_REQUESTED

---

## High-level View

The text-replacement approach produced a critical bug in environment variable naming: `STELLAR_GATEWAY_PUBLIC` was transformed into `galaxy_rayWAY_PUBLIC` instead of a well-formed name like `GALAXYRAY_PUBLIC`. This malformed variable appears across `.env.example`, source code, documentation, and tests, and won't be read at runtime. GitHub Actions workflows still reference the old `STELLAR_GATEWAY_PUBLIC` name, introducing a second inconsistency.

File renames, Cargo metadata, and Rust crate references were handled correctly. Allowed exclusions (CHANGELOG history, GitHub URLs, copyright) were preserved as intended.

<details>
<summary>Issues (3)</summary>

1. **Malformed environment variable names** — `galaxy_rayWAY_PUBLIC` and `galaxy_rayWAY_SECRET` are syntactically invalid for environment variables and will not be read correctly at runtime. These appear in 18+ locations across config files, source code, documentation, and tests. Should be `GALAXYRAY_PUBLIC` and `GALAXYRAY_SECRET`, or `GALAXY_RAY_PUBLIC` and `GALAXY_RAY_SECRET` if following snake_case convention.

2. **Workflows still use old variable name** — `.github/workflows/ci.yml` and `.github/workflows/release.yml` both reference `STELLAR_GATEWAY_PUBLIC=UNCONFIGURED` in docker run commands. This should be updated to match the renamed environment variable (once the malformed name issue is fixed).

3. **Missing Cargo.lock update** — `Cargo.lock` still references `name = "stellargate"` in the package metadata. This will be regenerated on the next `cargo build`, but if the lock file is committed, it should reflect the new package name.

</details>

<details>
<summary>Details</summary>

### Environment Variable Naming Corruption

The replacement of `STELLAR_GATEWAY_PUBLIC` with `galaxyray` produced the malformed variable name `galaxy_rayWAY_PUBLIC` (similarly `galaxy_rayWAY_SECRET`). These appear in 18+ places: `.env.example` (line 20), `deploy/galaxyray.env.example` (line 31), `src/config.rs` (lines 229, 389, 1111, 1143), `README.md` (lines 152, 268), `CONTRIBUTING.md` (line 43), `e2e/playwright.config.js` (line 157), and test code.

The application reads this variable at startup via `std::env::var("galaxy_rayWAY_PUBLIC")`. If the variable is not set, the gateway boots in "UNCONFIGURED" state. Setting a valid value requires knowing the exact malformed name, breaking operator deployments.

**Confirmed** — 18+ occurrences will cause runtime misconfiguration.

### Workflows Reference Unchanged Variable Name

The GitHub Actions workflows in `.github/workflows/ci.yml` (line 149) and `.github/workflows/release.yml` (line 142) pass `-e STELLAR_GATEWAY_PUBLIC=UNCONFIGURED` to docker run. This is the old name and doesn't match the malformed `galaxy_rayWAY_PUBLIC`.

**Confirmed** — two workflow files will pass the wrong variable.

### Cargo.lock Not Updated

`Cargo.lock` line 2502 still shows `name = "stellargate"`. This will be regenerated on `cargo build`, but represents inconsistent state if committed.

**Confirmed** — old name in lock file.

### What Worked

File rename was correct: `deploy/stellargate.env.example` → `deploy/galaxyray.env.example`. Cargo package metadata is correct (`name = "galaxyray"`). All Rust crate references updated; no `use stellargate::` remaining. GitHub URLs and CHANGELOG left unchanged as intended.

</details>

<details>
<summary>File Map</summary>

67 files modified. Key changes:
- `.env.example`, `deploy/galaxyray.env.example` — env var corruption present
- `.github/workflows/ci.yml`, `release.yml` — docker references updated, but env vars not updated
- `Cargo.toml` — package name and binary updated; URLs unchanged (intentional)
- `Cargo.lock` — old name remains (will be regenerated)
- `src/config.rs`, `src/horizon.rs`, `src/main.rs` — crate references and binary command names updated
- `docker-compose.yml`, `deploy/docker-compose.prod.yml` — service and volume names updated
- `README.md`, `CONTRIBUTING.md` — documentation with env var corruption propagated
- `e2e/playwright.config.js` — test config with env var corruption
- `tests/*.rs` — test fixtures updated
- CHANGELOG.md, GitHub URLs — intentionally unchanged

</details>
