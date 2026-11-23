default: check

[doc("Everything that must be green before a commit. Seconds, not minutes.")]
check: fmt clippy names test roles raw-store purity events comments allows wildcards traits stored refined pins docs ids views-current

[doc("Serve the tasks and the payments. TASKS_DATABASE, TASKS_LISTEN, TASKS_DURABILITY (full|normal) and TASKS_RELAY_ATTEMPTS override the defaults.")]
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

[doc("A driven adapter stores text: it depends on no domain crate and no serializer outside its tests.")]
raw-store:
    cargo run -q -p gates -- raw-store

[doc("The Postgres adapter against a server: lints, concurrent writers and the runtime. Not in `check`: it needs STORE_POSTGRES_URL.")]
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
    cargo run -q -p gates -- events

[doc("arch-1bmf: no two lifecycles and no two projections share a NAME; they would share rows. Runs before the tests.")]
names:
    @cargo run -q -p gates -- names

[doc("arch-0d2: no comments; names and types carry the meaning. Doc comments, and fc-allow in the core, only.")]
comments:
    @cargo run -q -p gates -- comments

[doc("arch-tsu: no allow or expect attribute, but the one that opens a file under tests/; fix what the lint names.")]
allows:
    @cargo run -q -p gates -- allows

[doc("arch-j0k: no element of a tuple pattern in domain or app catches every remaining case.")]
wildcards:
    @cargo run -q -p gates -- wildcards

[doc("arch-u8nl: a trait is declared only where the primitives live; a feature is an instance, never a new kind.")]
traits:
    @cargo run -q -p gates -- traits

[doc("arch-btw1: every variant of every stored type has a pinned sample in the crate's tests/stored_encoding.rs.")]
stored:
    @cargo run -q -p gates -- stored

[doc("arch-v9j1: the TryFrom a refined type loads its stored values through does not call the type's new, which holds the rule for new input.")]
refined:
    @cargo run -q -p gates -- refined

[doc("arch-mov6: every tool a workflow builds with `cargo install --git` is pinned with `--rev` to a 40-hex commit.")]
pins:
    @cargo run -q -p gates -- pins

[doc("Every Rust block in docs/architecture is code that compiles in this workspace.")]
docs:
    cargo run -q -p gates -- docs

[doc("arch-0id: the requirement ids and their projection are current.")]
ids:
    req check --out requirements.toon

[doc("Regenerate docs/views (crate graph, state machines) from the code.")]
views:
    cargo run -q -p gates -- views

[doc("The committed views match the code; a stale view is a hand-drawn diagram with a delay.")]
views-current:
    cargo run -q -p gates -- views --check

[doc("Continuous integration only: native mutation of the core.")]
mutants-core:
    cargo mutants -p domain --no-shuffle
