#!/usr/bin/env python3
import os, re, shutil, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.abspath(__file__))
CRATES = ["domain", "ports", "app", "adapter-sqlite", "adapter-http", "server", "bench"]
FILES = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "clippy.toml", "rustfmt.toml", "justfile", "guide-sync.bb", "mutants.py"]

HANDLE = "let applied: Applied<C::Reads> = unit.apply(id, |state: C::Reads| ports::decide(command, state))?;"
REACT = "let applied: Applied<X::Reads> = unit.apply(id, |state: X::Reads| ports::react(progress, state))?;"
PUT = "unit.put(ports::project::<P>(row, fact))"

MUTANTS = [
    ("C1 a state is applied in place of an outcome", "app/src/lib.rs", HANDLE,
     "let applied: Applied<C::Reads> = unit.apply(id, |state: C::Reads| state)?;", "E0308"),
    ("C2 a transition is built outside ports", "app/src/lib.rs", REACT,
     "let applied: Applied<X::Reads> = unit.apply(id, |state: X::Reads| ports::Outcome::<X::Reads, X::Request>::Accepted(ports::Transition { state, events: Vec::new(), requests: Vec::new() }))?;", "E0451"),
    ("C3 the private fold is called outside ports", "app/src/lib.rs", REACT,
     "let applied: Applied<X::Reads> = unit.apply(id, |state: X::Reads| ports::Outcome::<X::Reads, X::Request>::Accepted(ports::Transition::fold(state, Vec::new(), Vec::new())))?;", "E0624"),
    ("C4 a change is built outside ports", "app/src/lib.rs", PUT,
     "unit.put(ports::Change::<P> { position: fact.position, write: ports::Write::Nothing })", "E0451"),
    ("C5 an outcome is computed and dropped", "app/src/lib.rs", HANDLE,
     "ports::decide(domain::task::CompleteTask, domain::task::Task::Absent); " + HANDLE, "unused"),
    ("C6 applied facts never reach a policy", "app/src/lib.rs",
     REACT + "\n    settle(unit, applied).map(|_answer: Answer<<X::Reads as Lifecycle>::Refusal>| ())",
     "unit.apply(id, |state: X::Reads| ports::react(progress, state))?;\n    Ok(())", "unused"),
    ("C7 a change is computed and never put", "app/src/lib.rs", PUT,
     "{ ports::project::<P>(row, fact); Ok(()) }", "unused"),
    ("C8 a lifecycle has no policy", "app/src/tasks.rs",
     "impl Policy for Task {\n    fn after<U: Unit>(unit: &mut U, fact: &Recorded<TaskEvent>) -> Result<(), StoreError> {\n        project::<U, TaskTally>(unit, fact)\n    }\n}",
     "pub fn unused_task_event(_fact: &Recorded<TaskEvent>) {}", "E0277"),
    ("C9 a command issues a request", "domain/src/task.rs",
     "Task::Open { .. } => Ok(vec![TaskEvent::Completed]),",
     "Task::Open { .. } => Ok(crate::Step { events: vec![TaskEvent::Completed], requests: vec![()] }),", "E0308"),
    ("T1 the stored state is not the fold of the facts", "ports/src/lib.rs",
     "let state: S = events.iter().fold(state, S::evolve);", "let state: S = events.iter().fold(state, |state: S, _event: &S::Event| state);", "test"),
    ("T2 a refusal is not recorded", "adapter-sqlite/src/lib.rs",
     ".execute(params![S::NAME, id.0, encode(\"rejections\", &refusal)?])", ".execute(params![S::NAME, id.0, encode(\"rejections\", &refusal)?]).and(Ok(0)).and_then(|_: usize| self.transaction.execute(\"DELETE FROM rejections\", []))", "test"),
    ("T3 the state row is not written", "adapter-sqlite/src/lib.rs", "if next != version {", "if false {", "test"),
    ("T4 the log ignores the position it resumes after", "adapter-sqlite/src/lib.rs",
     "WHERE lifecycle = ?1 AND position > ?2 ORDER BY position LIMIT ?3", "WHERE lifecycle = ?1 AND position > ?2 - ?2 ORDER BY position LIMIT ?3", "test"),
    ("T5 the log returns another lifecycle's facts", "adapter-sqlite/src/lib.rs",
     "WHERE lifecycle = ?1 AND position > ?2 ORDER BY position LIMIT ?3", "WHERE ?1 = ?1 AND position > ?2 ORDER BY position LIMIT ?3", "test"),
    ("T6 the log ignores its limit", "adapter-sqlite/src/lib.rs",
     "WHERE lifecycle = ?1 AND position > ?2 ORDER BY position LIMIT ?3", "WHERE lifecycle = ?1 AND position > ?2 ORDER BY position LIMIT ?3 + 1000", "test"),
    ("T7 a read model accepts a change out of order", "adapter-sqlite/src/lib.rs", "if position <= at {", "if false {", "test"),
    ("T8 a read model's position does not advance", "adapter-sqlite/src/lib.rs",
     ".execute(params![P::NAME, stored(position.0)?])", ".execute(params![P::NAME, stored(position.0 - position.0)?])", "test"),
    ("T9 a failed unit is committed", "adapter-sqlite/src/lib.rs",
     "let value: T = work(&mut unit)?;\n        let last: Option<Position> = unit.last;\n        transaction.commit().map_err(unavailable)?;",
     "let outcome: Result<T, StoreError> = work(&mut unit);\n        let last: Option<Position> = unit.last;\n        transaction.commit().map_err(unavailable)?;\n        let value: T = outcome?;", "test"),
    ("T10 a policy skips its projection", "app/src/tasks.rs",
     "        project::<U, TaskTally>(unit, fact)\n", "        Ok(()).map(|ok: ()| { let _unused: (&mut U, &Recorded<TaskEvent>) = (unit, fact); ok })\n", "test"),
    ("T11 a policy skips its process manager", "app/src/payments.rs",
     "PaymentEvent::Captured { .. } => react(unit, &fulfilment_of(&fact.stream), Progress::PaymentCaptured),",
     "PaymentEvent::Captured { .. } => Ok(()).map(|ok: ()| { let _unused: (&mut U, Progress) = (unit, Progress::PaymentCaptured); ok }),", "test"),
    ("T12 applied facts skip the policy", "app/src/lib.rs", "S::after(unit, fact)?;", "let _skipped: &Recorded<S::Event> = fact;", "test"),
    ("T13 a projection folds without its previous row", "app/src/lib.rs",
     "Some(key) => unit.row::<P>(&key)?,", "Some(_key) => None,", "test"),
    ("T14 the relay marks a failed request done", "app/src/lib.rs", "Err(GatewayError { .. }) => outbox.failed(request.id)?,",
     "Err(GatewayError { .. }) => outbox.done(request.id)?,", "test"),
    ("T26 a failed request is claimed first again", "adapter-sqlite/src/lib.rs", "AND done = 0 ORDER BY attempts, id", "AND done = 0 ORDER BY id", "test"),
    ("T15 the relay never marks a request done", "app/src/lib.rs", "outbox.done(request.id)?;", "", "test"),
    ("T16 the outbox hands out a request already done", "adapter-sqlite/src/lib.rs", "AND done = 0 ORDER BY attempts, id", "ORDER BY attempts, id", "test"),
    ("T17 a commit does not report its position", "adapter-sqlite/src/lib.rs", "self.last = Some(position);", "self.last = self.last.and(Some(position));", "test"),
    ("T18 a rebuild keeps the old rows", "app/src/lib.rs", "unit.clear::<P>()?;", "", "test"),
    ("T19 requests are not written to the outbox", "adapter-sqlite/src/lib.rs", "for request in &requests {", "for request in requests.iter().take(0) {", "test"),
    ("T20 a stored fact changes its encoding", "domain/src/task.rs",
     "pub enum TaskEvent {\n    Added { title: Title },\n    Completed,", "pub enum TaskEvent {\n    Added { title: Title },\n    #[serde(rename = \"Finished\")]\n    Completed,", "test"),
    ("T21 a fact takes a position that is not its own", "adapter-sqlite/src/lib.rs",
     "                        position,\n                        stream: id.clone(),",
     "                        position: Position(position.0 + 1),\n                        stream: id.clone(),", "test"),
    ("T22 a blank title is accepted", "domain/src/task.rs", "if text.trim().is_empty() {", "if text.is_empty() && !text.is_empty() {", "test"),
    ("T23 a title over the limit is accepted", "domain/src/task.rs", "if text.chars().count() > Title::LIMIT {", "if text.chars().count() > Title::LIMIT + 1 {", "test"),
    ("T24 a stored title skips the constructor on load", "domain/src/task.rs", "#[serde(try_from = \"String\")]", "#[serde(transparent)]", "test"),
    ("H1 a refusal is answered as accepted", "adapter-http/src/lib.rs",
     "(StatusCode::CONFLICT, refusal_name(refusal)).into_response()",
     "(StatusCode::NO_CONTENT, refusal_name(refusal)).into_response()", "test"),
    ("H2 a task id is not minted per request", "adapter-http/src/lib.rs",
     "StreamId(Uuid::now_v7().to_string())", "StreamId(Uuid::nil().to_string())", "test"),
    ("H3 a title reaches the page unescaped", "adapter-http/src/lib.rs", "escape(title.as_str()),", "title.as_str(),", "test"),
    ("H4 a unit of work runs on the executor", "adapter-http/src/lib.rs",
     "= tokio::task::spawn_blocking(move || work(&journal)).await;", "= Ok(work(&journal));", "test"),
    ("H5 a line break in a title starts a new event line", "adapter-http/src/lib.rs",
     "'\\n' => escaped.push_str(\"&#10;\"),", "'\\n' => escaped.push('\\n'),", "test"),
    ("H6 an open stream outlives a stop", "adapter-http/src/lib.rs",
     "wait_for(|stopped: &bool| *stopped)", "wait_for(|stopped: &bool| *stopped && !*stopped)", "test"),
    ("H7 a store error is answered and not logged", "adapter-http/src/lib.rs",
     "tracing::error!(parent: span, error = ?error, \"{message}\");", "let _unlogged: (&StoreError, &'static str) = (error, message);", "test"),
    ("H8 a refusal is logged as accepted", "adapter-http/src/lib.rs", "span.record(\"outcome\", \"refused\");", "span.record(\"outcome\", \"accepted\");", "test"),
    ("H9 a request body is logged", "adapter-http/src/lib.rs",
     "span.record(\"command\", \"AddTask\");", "span.record(\"command\", String::from_utf8_lossy(&body).as_ref());", "test"),
    ("T25 a panic inside a unit of work locks the store for good", "adapter-sqlite/src/lib.rs",
     "Ok(self.connection.lock().unwrap_or_else(PoisonError::into_inner))",
     "self.connection.lock().map_err(|_: PoisonError<MutexGuard<'_, Connection>>| StoreError::Unavailable { cause: String::new() })", "test"),
    ("G1 a driven adapter depends on the shell", "adapter-sqlite/Cargo.toml", 'ports = { path = "../ports" }',
     'ports = { path = "../ports" }\napp = { path = "../app" }', ("gate", ["hex-lint"], "adapter-sqlite")),
    ("G8 a driving adapter depends on a driven adapter", "adapter-http/Cargo.toml", 'ports = { path = "../ports" }',
     'ports = { path = "../ports" }\nadapter-sqlite = { path = "../adapter-sqlite" }', ("gate", ["hex-lint"], "adapter-http")),
    ("G2 the core reads the clock", "domain/src/lib.rs", "pub type Facts<S> = Vec<<S as Lifecycle>::Event>;",
     "pub fn now() -> std::time::SystemTime {\n    std::time::SystemTime::now()\n}\n\npub type Facts<S> = Vec<<S as Lifecycle>::Event>;", ("gate", ["effect-audit", "--require-domain", "--strict", "."], "clock")),
    ("G3 the core takes a dependency it does not vouch for", "domain/Cargo.toml", "[dependencies]\nserde = { workspace = true }",
     "[dependencies]\nserde = { workspace = true }\nserde_json = { workspace = true }", ("gate", ["effect-audit", "--require-domain", "--strict", "."], "serde_json")),
    ("G4 the core runs async code", "domain/src/lib.rs", "pub type Facts<S> = Vec<<S as Lifecycle>::Event>;",
     "pub async fn later() {}\n\npub type Facts<S> = Vec<<S as Lifecycle>::Event>;", ("gate", ["effect-audit", "--require-domain", "--strict", "."], "async")),
    ("G5 a comment explains code", "app/src/lib.rs", "pub trait Policy: Lifecycle {",
     "// names the readers\npub trait Policy: Lifecycle {", ("gate", ["just", "comments"], "arch-0d2")),
    ("G6 a match on an enum has a wildcard arm", "domain/src/task.rs", "Task::Open { .. } | Task::Done { .. } => Err(TaskRefusal::AlreadyExists),",
     "_ => Err(TaskRefusal::AlreadyExists),", ("gate", ["cargo", "clippy", "-q", "--all-targets", "--", "-D", "warnings"], "wildcard_enum_match_arm")),
    ("G9 a lint is silenced where it fires", "app/src/lib.rs", "pub trait Policy: Lifecycle {",
     "#[allow(dead_code)]\npub trait Policy: Lifecycle {", ("gate", ["just", "allows"], "arch-tsu")),
    ("G7 a rule document shows code the workspace does not hold", "ports/src/lib.rs", "pub struct RequestId(pub u64);",
     "pub struct RequestId(pub u32);", ("gate", ["bb", "guide-sync.bb"], "arch-ajd")),
]

EXAMPLE = {"C5", "C8", "C9", "T10", "T11", "T20", "T22", "T23", "T24", "G6", "H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"}


def run(name, path, old, new, expect):
    scratch = tempfile.mkdtemp(prefix="mutant-")
    try:
        for crate in CRATES:
            shutil.copytree(os.path.join(ROOT, crate), os.path.join(scratch, crate), copy_function=shutil.copy)
        for file in FILES:
            shutil.copy(os.path.join(ROOT, file), scratch)
        target = os.path.join(scratch, path)
        source = open(target).read()
        if source.count(old) != 1:
            return False, "ANCHOR FOUND %d TIMES" % source.count(old)
        open(target, "w").write(source.replace(old, new))
        env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(ROOT, "target", "mutants"))
        if isinstance(expect, tuple):
            shutil.copytree(os.path.join(ROOT, "docs"), os.path.join(scratch, "docs"))
            done = subprocess.run(expect[1], cwd=scratch, env=env, capture_output=True, text=True)
            output = done.stdout + done.stderr
            hit = done.returncode != 0 and expect[2] in output
            return hit, "exit %d, %s %s" % (done.returncode, "names" if expect[2] in output else "DOES NOT NAME", expect[2])
        command = ["cargo", "test", "-q", "--no-fail-fast"] if expect == "test" else ["cargo", "check", "-q", "--all-targets"]
        done = subprocess.run(command, cwd=scratch, env=env, capture_output=True, text=True)
        output = done.stdout + done.stderr
        if done.returncode == 0:
            return False, "SURVIVED"
        if expect == "test":
            if "could not compile" in output:
                return False, "DID NOT COMPILE: " + " ".join(sorted(set(re.findall(r"error\[E\d+\]", output))))
            failed = sorted(set(re.findall(r"^---- (\S+) stdout ----", output, re.M)))
            return bool(failed), "killed by " + ", ".join(failed)
        codes = sorted(set(re.findall(r"error\[(E\d+)\]", output)))
        unused = "unused" in output and "must be used" in output
        hit = unused if expect == "unused" else expect in codes
        return hit, ("unused_must_use" if unused else "") + " ".join(codes)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def main():
    wanted = sys.argv[1:]
    if wanted == ["--list"]:
        for name, path, _old, _new, _expect in MUTANTS:
            print("%-9s %-28s %s" % ("example" if name.split()[0] in EXAMPLE else "machinery", path, name))
        return
    red = 0
    for name, path, old, new, expect in MUTANTS:
        if wanted and name.split()[0] not in wanted:
            continue
        ok, detail = run(name, path, old, new, expect)
        red += 0 if ok else 1
        print("%-4s %s [%s] %s" % ("ok" if ok else "FAIL", name, expect if isinstance(expect, str) else " ".join(expect[1][:2]), detail), flush=True)
    print("%d mutant(s) not killed as expected" % red)
    sys.exit(1 if red else 0)


main()
