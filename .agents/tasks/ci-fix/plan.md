# CI Fix Plan for GalaxyRay

## Issues Identified

After exploring the GalaxyRay codebase following the rename from StellarGate to GalaxyRay and addition of three new features (analytics, invoices, subscriptions), the following CI failures have root causes:

### Compilation Errors in src/main.rs
1. **Duplicate `use std::time::Duration;` import** (lines 9 and 21)
   - Line 9: `use std::time::Duration;`
   - Line 21: `use std::time::Duration;` (duplicate)

2. **Undefined `shutdown_grace` variable** (line 145)
   - Line 145 uses `shutdown_grace` in: `let deadline = tokio::time::Instant::now() + shutdown_grace;`
   - This variable is never defined or loaded from config
   - Should be sourced from `state.config.shutdown_grace_secs` or a constant

3. **Unused import** (line 11)
   - `use std::future::Future;` is imported but never used in the file
   - Also `use moka::sync::Cache;` is imported but never used (AppState field is `analytics_cache: moka::sync::Cache<String, String>` which is the correct generic type)

### Binary Name Mismatch (Docker-related)
- **Cargo.toml** specifies binary name: `galaxyray` (lowercase)
- **Dockerfile** builds: `/usr/local/bin/GalaxyRay` (PascalCase)
- **docker-compose.yml** and **docker-compose.prod.yml** both reference `/usr/local/bin/GalaxyRay`
- The actual binary from `cargo build --release` is `/app/target/release/galaxyray`, not `/app/target/release/GalaxyRay`
- This causes the Docker smoke test and runtime to fail (file not found)

### Environment File References
- **deploy/GalaxyRay.env.example** and **.env.example** reference `GALAXYRAY_PUBLIC` environment variable
- The codebase appears configured correctly, but ensure consistency across all references

### Deploy Consistency Script Issues
- **scripts/check-deploy-consistency.sh** contains undefined bash function `note()` which is called but never defined
- The script will fail when checking deploy assets

---

## Implementation Plan

### Phase 1: Fix Rust Compilation Errors (Priority: Critical)

- [ ] 1. Fix duplicate imports in src/main.rs and remove unused imports.
      Remove the duplicate `use std::time::Duration;` on line 21, and remove the unused `use std::future::Future;` and `use moka::sync::Cache;` imports.
      Files: src/main.rs
      Verify: `cargo check` passes without import warnings or duplicate import errors.

- [ ] 2. Define the `shutdown_grace` variable in main() by loading from config.
      Add code to load `SHUTDOWN_GRACE_SECS` from the config and use it to calculate the deadline. The value should default to `DEFAULT_SHUTDOWN_GRACE_SECS` (30 seconds) if not set. Load this early in main(), after config is initialized, before the deadline calculation on line 145.
      Files: src/main.rs
      Verify: `cargo check` passes; search src/main.rs for `shutdown_grace` to confirm it is now defined before line 145.

- [ ] 3. Verify AppState analytics_cache initialization uses correct generic types.
      The analytics_cache field is `moka::sync::Cache<String, String>` in lib.rs. Ensure the CacheBuilder in main.rs (around line 85-87) correctly builds a cache with String keys and String values. The current code appears correct, but verify during Cargo check that the types match.
      Files: src/main.rs, src/lib.rs
      Verify: `cargo check` passes without type mismatch errors on the analytics_cache field.

### Phase 2: Fix Docker Binary Name Mismatch (Priority: Critical)

- [ ] 4. Update Dockerfile COPY command to use the correct lowercase binary name.
      Change line 29 in Dockerfile from `COPY --from=builder /app/target/release/GalaxyRay /usr/local/bin/GalaxyRay` to `COPY --from=builder /app/target/release/galaxyray /usr/local/bin/galaxyray`. This matches the binary name defined in Cargo.toml (`name = "galaxyray"`).
      Files: Dockerfile
      Verify: Confirm the change with `grep "COPY --from=builder" Dockerfile` and ensure it references `galaxyray` (lowercase) on both sides.

- [ ] 5. Update docker-compose.yml healthcheck to reference the correct binary path.
      Change the healthcheck `test` command from `["/usr/local/bin/GalaxyRay", "healthcheck", "health"]` to `["/usr/local/bin/galaxyray", "healthcheck", "health"]`.
      Files: docker-compose.yml
      Verify: `grep -n "GalaxyRay" docker-compose.yml` should return zero results (case-sensitive); all healthcheck paths should reference `galaxyray` (lowercase).

- [ ] 6. Update docker-compose.prod.yml to use consistent binary naming and fix image registry reference.
      Change the healthcheck `test` command from `["/usr/local/bin/GalaxyRay", "healthcheck", "ready"]` to `["/usr/local/bin/galaxyray", "healthcheck", "ready"]`. 
      Also review and fix the image reference line. The current line references `ghcr.io/GalaxyRaylabs/GalaxyRay:...` but based on the GitHub username in the deploy env file and repository URL, this should reference the correct registry path (likely `ghcr.io/promisetechhub1788-cyber/GalaxyRay` or similar). Keep the user's actual registry, but ensure PascalCase is used for the image name if that matches the user's image registry conventions.
      Files: deploy/docker-compose.prod.yml
      Verify: `grep -n "galaxyray\|GalaxyRay" deploy/docker-compose.prod.yml` confirms lowercase binary path references and correct image registry.

- [ ] 7. Update Dockerfile runtime stage to reference lowercase binary name in USER and CMD.
      Confirm line 40 `RUN useradd -r -u 1001 -U GalaxyRay` and line 47 `CMD ["/usr/local/bin/GalaxyRay"]` are consistent with the lowercase binary. The USER can stay as PascalCase (it is a system user name, which may be separate from the binary name), but verify CMD references the lowercase path: `CMD ["/usr/local/bin/galaxyray"]`.
      Files: Dockerfile
      Verify: `grep -n "CMD\|useradd" Dockerfile` confirms CMD uses the lowercase path.

- [ ] 8. Update Dockerfile EXPOSE port and environment to reference lowercase database file names.
      Check line 47 `ENV DATABASE_URL=sqlite:///data/GalaxyRay.db` and line 50 `HEALTHCHECK...CMD ["/usr/local/bin/GalaxyRay", ...]`. The DATABASE_URL database filename should remain as `GalaxyRay.db` (matches the container user and service name), but the HEALTHCHECK CMD binary path must be lowercase.
      Files: Dockerfile
      Verify: `grep "DATABASE_URL\|HEALTHCHECK" Dockerfile` confirms paths are correctly set.

### Phase 3: Fix Deploy Consistency Script (Priority: Medium)

- [ ] 9. Define the `note()` function in scripts/check-deploy-consistency.sh or use a standard logging pattern.
      Add a `note()` function definition at the top of the script that logs informational messages (similar to warnings). This function is called multiple times in the script but is never defined, causing execution to fail. Define it as: `note() { echo "$@" >&2; }` to mirror the error output pattern of other shell functions.
      Files: scripts/check-deploy-consistency.sh
      Verify: Run `bash scripts/check-deploy-consistency.sh` and confirm it completes without "note: command not found" errors. The script should output "deploy assets are consistent with Dockerfile" at the end.

### Phase 4: Verify Full Build (Priority: Critical)

- [ ] 10. Run `cargo build --release --locked` to verify all Rust code compiles.
       After all source fixes are applied, perform a full release build to catch any remaining compilation errors, type mismatches, or linker issues.
       Files: (all Rust source)
       Verify: `cargo build --release --locked` completes without errors. Binary is available at `target/release/galaxyray`.

- [ ] 11. Run `cargo test` to verify unit tests pass.
       Run the full test suite to ensure no tests were broken by the changes and the feature additions (analytics, invoices, subscriptions) integrate correctly.
       Files: (all test files in the project)
       Verify: `cargo test --all` passes all tests. No failing tests related to the fixes.

- [ ] 12. Run `cargo fmt --check` to verify code formatting compliance.
       Ensure all Rust source files meet the project's formatting standard.
       Files: src/main.rs, src/lib.rs, src/api/*, src/db/*, etc.
       Verify: `cargo fmt --check` reports no formatting violations. If any exist, run `cargo fmt` to auto-fix.

- [ ] 13. Run `cargo clippy --all-targets --all-features -- -D warnings` to verify linting compliance.
       Catch any code quality issues flagged by Clippy (both MSRV and stable toolchains).
       Files: (all Rust source)
       Verify: `cargo clippy --all-targets --all-features -- -D warnings` reports no warnings or errors.

### Phase 5: Verify Docker Build (Priority: High)

- [ ] 14. Build the Docker image locally to verify the multi-stage build succeeds and the binary is correctly copied.
       Run `docker build -t galaxyray:test .` to verify the Dockerfile builds cleanly with the corrected binary names and paths.
       Files: Dockerfile
       Verify: Docker build completes successfully. `docker run galaxyray:test /usr/local/bin/galaxyray healthcheck health` returns exit code 0 (or connection refused, which is normal if the service is not running).

### Phase 6: Verify Compose Files (Priority: Medium)

- [ ] 15. Verify docker-compose.yml and docker-compose.prod.yml can be parsed and validated.
       Run `docker compose config` against both files to ensure YAML syntax is valid and all referenced paths and environment variables are syntactically correct.
       Files: docker-compose.yml, deploy/docker-compose.prod.yml
       Verify: `docker compose -f docker-compose.yml config > /dev/null` and `docker compose -f deploy/docker-compose.prod.yml config > /dev/null` both complete without YAML or validation errors.

---

## Summary of Root Causes

| CI Job | Root Cause | Fix Location |
|--------|-----------|--------------|
| CI / clippy (msrv) + (stable) | Duplicate imports, undefined `shutdown_grace` variable, unused imports | src/main.rs |
| CI / test (msrv) + (stable) | Same compilation errors prevent tests from running | src/main.rs |
| CI / rustfmt | Formatting compliance after import cleanup | src/main.rs (auto-fixed) |
| CI / coverage (llvm-cov) | Compilation failure blocks coverage runs | src/main.rs |
| CI / docker smoke test | Binary name mismatch: Dockerfile copies `GalaxyRay` (PascalCase) but Cargo.toml defines `galaxyray` (lowercase) | Dockerfile, docker-compose.yml, docker-compose.prod.yml |
| CI / deploy asset consistency | `note()` function undefined in bash script | scripts/check-deploy-consistency.sh |
| Dashboard tests | Not identified as part of this project structure; may be in a separate monorepo or submodule. Likely related to static files or frontend build. | (External to this analysis) |

---

## Execution Order Notes

1. **Phase 1 must complete first** — the Rust code must compile before Docker builds or tests can run.
2. **Phase 2 must complete before Docker operations** — binary name mismatches prevent container runtime and test execution.
3. **Phase 3 is independent** but should be fixed before deployment consistency checks are required.
4. **Phase 4** verifies all code fixes work together.
5. **Phases 5 & 6** verify the deployment artifacts (Docker and Compose files) are consistent and buildable.

Each phase leaves the codebase in a buildable state, and each fix is independently verifiable.
