default: check

[doc("Everything that must be green before a commit. Seconds, not minutes.")]
check: fmt clippy test roles purity comments allows docs ids views-current

[doc("Serve the task list. TASKS_DATABASE, TASKS_LISTEN and TASKS_DURABILITY (full|normal) override the defaults.")]
run:
    cargo run -p server

[doc("Formatting is checked, never applied, so a red is a diff you can read.")]
fmt:
    @cargo fmt --check || (echo 'fmt: run `cargo fmt` and commit the result' && false)

[doc("Lints in Cargo.toml [workspace.lints]: exhaustive matches, no unwrap, must-use outcomes.")]
clippy:
    cargo clippy -q --all-targets -- -D warnings

test:
    cargo test -q

[doc("arch-jxh, arch-yr1: every dependency edge fits the role matrix and stays in its context.")]
roles:
    hex-lint

[doc("arch-zhm: the core does no I/O, reads no clock, runs no async.")]
purity:
    effect-audit --require-domain --strict .

[doc("arch-0d2: no comments; names and types carry the meaning. Doc comments and fc-allow only.")]
comments:
    @! grep -rnE '^[[:space:]]*//([^/!]|$)' --include='*.rs' domain ports app adapter-* server bench 2>/dev/null | grep -v 'fc-allow:' || (echo 'arch-0d2: remove the comment, or make it a contract in a doc comment' && false)

[doc("arch-tsu: no allow attribute outside a tests/ directory; fix what the lint names.")]
allows:
    @! grep -rnE '#!?\[allow\(' --include='*.rs' domain ports app adapter-* server bench 2>/dev/null | grep -v '/tests/' || (echo 'arch-tsu: remove the allow attribute and fix what the lint names (a type alias, a smaller function)' && false)

[doc("Every Rust block in docs/architecture is code that compiles in this workspace.")]
docs:
    bb guide-sync.bb

[doc("arch-0id: the requirement ids and their projection are current.")]
ids:
    req check --out requirements.toon

[doc("Regenerate docs/views (crate graph, state machines) from the code.")]
views:
    python3 views.py

[doc("The committed views match the code; a stale view is a hand-drawn diagram with a delay.")]
views-current:
    python3 views.py --check

[doc("Continuous integration only: plant every architecture mutant and require each to die as stated.")]
mutants:
    python3 mutants.py

[doc("Continuous integration only: native mutation of the core.")]
mutants-core:
    cargo mutants -p domain --no-shuffle

[doc("arch-ukt: build the benchmark in release, then run the binary alone, so no cargo process is alive while it measures.")]
bench:
    BENCH_COMMIT="$(git rev-parse --short HEAD)$(git diff --quiet HEAD || echo -dirty)" BENCH_RUSTC="$(rustc --version)" cargo build --release -q -p bench
    ./target/release/bench
