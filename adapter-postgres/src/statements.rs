use postgres::{Client, Statement};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS states (
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    version BIGINT NOT NULL,
    state TEXT NOT NULL,
    PRIMARY KEY (lifecycle, stream)
);
CREATE TABLE IF NOT EXISTS facts (
    position BIGINT PRIMARY KEY,
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    version BIGINT NOT NULL,
    event TEXT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (lifecycle, stream, version)
);
CREATE TABLE IF NOT EXISTS rejections (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    refusal TEXT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS outbox (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    lifecycle TEXT NOT NULL,
    stream TEXT NOT NULL,
    request TEXT NOT NULL,
    done BOOLEAN NOT NULL DEFAULT false,
    attempts BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS read_models (
    name TEXT NOT NULL,
    key TEXT NOT NULL,
    row TEXT NOT NULL,
    PRIMARY KEY (name, key)
);
CREATE TABLE IF NOT EXISTS read_model_positions (
    name TEXT PRIMARY KEY,
    position BIGINT NOT NULL
);
";

pub struct Statements {
    pub lock: Statement,
    pub state: Statement,
    pub states: Statement,
    pub row: Statement,
    pub rows: Statement,
    pub append: Statement,
    pub save: Statement,
    pub reject: Statement,
    pub request: Statement,
    pub facts: Statement,
    pub position: Statement,
    pub advance: Statement,
    pub upsert: Statement,
    pub delete: Statement,
    pub clear_rows: Statement,
    pub clear_position: Statement,
    pub requests: Statement,
    pub finish: Statement,
    pub failed: Statement,
    pub rejections: Statement,
}

impl Statements {
    pub fn prepare(client: &mut Client) -> Result<Statements, postgres::Error> {
        Ok(Statements {
            lock: client.prepare("SELECT pg_advisory_xact_lock($1)")?,
            state: client.prepare("SELECT version, state FROM states WHERE lifecycle = $1 AND stream = $2")?,
            states: client.prepare("SELECT stream, state FROM states WHERE lifecycle = $1 ORDER BY stream COLLATE \"C\"")?,
            row: client.prepare("SELECT row FROM read_models WHERE name = $1 AND key = $2")?,
            rows: client.prepare("SELECT key, row FROM read_models WHERE name = $1 ORDER BY key COLLATE \"C\"")?,
            append: client.prepare(
                "INSERT INTO facts (position, lifecycle, stream, version, event)
                 SELECT coalesce(max(position), 0) + 1, $1, $2, $3, $4 FROM facts RETURNING position",
            )?,
            save: client.prepare(
                "INSERT INTO states (lifecycle, stream, version, state) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (lifecycle, stream) DO UPDATE SET version = excluded.version, state = excluded.state",
            )?,
            reject: client.prepare("INSERT INTO rejections (lifecycle, stream, refusal) VALUES ($1, $2, $3)")?,
            request: client.prepare("INSERT INTO outbox (lifecycle, stream, request) VALUES ($1, $2, $3)")?,
            facts: client.prepare("SELECT position, stream, version, event FROM facts WHERE lifecycle = $1 AND position > $2 ORDER BY position LIMIT $3")?,
            position: client.prepare("SELECT position FROM read_model_positions WHERE name = $1")?,
            advance: client.prepare(
                "INSERT INTO read_model_positions (name, position) VALUES ($1, $2)
                 ON CONFLICT (name) DO UPDATE SET position = excluded.position",
            )?,
            upsert: client.prepare(
                "INSERT INTO read_models (name, key, row) VALUES ($1, $2, $3)
                 ON CONFLICT (name, key) DO UPDATE SET row = excluded.row",
            )?,
            delete: client.prepare("DELETE FROM read_models WHERE name = $1 AND key = $2")?,
            clear_rows: client.prepare("DELETE FROM read_models WHERE name = $1")?,
            clear_position: client.prepare("DELETE FROM read_model_positions WHERE name = $1")?,
            requests: client.prepare("SELECT id, stream, request FROM outbox WHERE lifecycle = $1 AND NOT done ORDER BY attempts, id LIMIT $2")?,
            finish: client.prepare("UPDATE outbox SET done = true WHERE id = $1 AND lifecycle = $2 AND NOT done RETURNING stream")?,
            failed: client.prepare("UPDATE outbox SET attempts = attempts + 1 WHERE id = $1")?,
            rejections: client.prepare("SELECT refusal FROM rejections WHERE lifecycle = $1 AND stream = $2 ORDER BY id")?,
        })
    }
}
