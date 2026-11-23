use std::process::ExitCode;

use gates::{Unread, Verdict};

type Gate = fn(&[String]) -> Result<Verdict, Unread>;

const GATES: &[(&str, Gate)] = &[("comments", gates::comments::comments), ("allows", gates::allows::allows)];

fn main() -> ExitCode {
    let asked: Vec<String> = std::env::args().skip(2).collect();
    let name: String = std::env::args().nth(1).unwrap_or_default();
    let Some(gate) = GATES.iter().find(|known| known.0 == name).map(|known| known.1) else {
        println!("gates: no gate is called `{name}`");
        return ExitCode::from(2);
    };
    match gate(&asked) {
        Ok(verdict) if verdict.found.is_empty() => ExitCode::SUCCESS,
        Ok(verdict) => {
            verdict.found.iter().chain(verdict.fix.iter()).for_each(|line| println!("{line}"));
            ExitCode::from(1)
        }
        Err(Unread(why)) => {
            println!("{name}: {why}");
            ExitCode::from(2)
        }
    }
}
