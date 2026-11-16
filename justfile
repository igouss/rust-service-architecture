default: check

[doc("Everything that must be green before a commit. Seconds, not minutes.")]
check: fmt clippy test roles raw-store purity events comments allows docs ids views-current

[doc("Serve the task list. TASKS_DATABASE, TASKS_LISTEN and TASKS_DURABILITY (full|normal) override the defaults.")]
run:
    cargo run -p server

[doc("Formatting is checked, never applied, so a red is a diff you can read.")]
fmt:
    @cargo fmt --check || (echo 'fmt: run `cargo fmt` and commit the result' && false)
    @rustfmt --check --edition 2021 ports/tests/conformance/*.rs || (echo 'fmt: run `rustfmt --edition 2021 ports/tests/conformance/*.rs` and commit the result' && false)

[doc("Lints in Cargo.toml [workspace.lints]: exhaustive matches, no unwrap, must-use outcomes.")]
clippy:
    cargo clippy -q --all-targets -- -D warnings

test:
    cargo test -q

[doc("arch-jxh, arch-yr1: every dependency edge fits the role matrix and stays in its context.")]
roles:
    hex-lint

[doc("A driven adapter stores text: it depends on no domain crate and no serializer outside its tests.")]
raw-store:
    python3 raw-store.py

[doc("The Postgres adapter against a server: lints, the conformance suite, concurrent writers. Not in `check`: it needs STORE_POSTGRES_URL.")]
postgres:
    @test -n "${STORE_POSTGRES_URL:-}" || (echo 'postgres: set STORE_POSTGRES_URL to a database these tests may write to, or run `just postgres-local`' && false)
    cargo clippy -q -p adapter-postgres --all-targets --features server -- -D warnings
    cargo test -q -p adapter-postgres --features server

[doc("Start a throwaway Postgres under target/, run `just postgres` against it, stop it.")]
postgres-local:
    #!/usr/bin/env bash
    set -euo pipefail
    data=target/postgres
    test -d "$data" || initdb -D "$data" -A trust -U store >/dev/null
    pg_ctl -D "$data" -o "-c listen_addresses=127.0.0.1 -c port=54330 -c unix_socket_directories=''" -l "$data/server.log" -w start >/dev/null
    trap 'pg_ctl -D "$data" -m fast stop >/dev/null' EXIT
    createdb -h 127.0.0.1 -p 54330 -U store store 2>/dev/null || true
    STORE_POSTGRES_URL="host=127.0.0.1 port=54330 user=store dbname=store" just postgres

[doc("arch-zhm: the core does no I/O, reads no clock, runs no async.")]
purity:
    effect-audit --require-domain --strict .

[doc("An event names what happened: no lifecycle's event is called Updated, Changed or their kin.")]
events:
    python3 events.py

[doc("arch-0d2: no comments; names and types carry the meaning. Doc comments, and fc-allow in the core, only.")]
comments:
    @python3 lint.py comments

[doc("arch-tsu: no allow or expect attribute, but the one that opens a file under tests/; fix what the lint names.")]
allows:
    @python3 lint.py allows

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
