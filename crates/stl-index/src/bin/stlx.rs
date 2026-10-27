//! Developer CLI over the index DB (`$STL_BROWSER_HOME` or `~/.stl-browser/index.db`).
//!
//!   stlx add <dir>                  register a folder as a root
//!   stlx scan                       scan all roots, printing progress and stats
//!   stlx extract <entry_id> <out>   extract one indexed file
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use stl_index::{db, paths, scanstore};
use stl_scan::extract::{SourceKind, extract_entry};
use stl_scan::nested::ListOptions;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = db::open(&paths::db_path())?;
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["add", dir] => {
            let id = scanstore::add_root(&conn, &PathBuf::from(dir))?;
            println!("root {id}: {dir}");
        }
        ["scan"] => {
            let roots: Vec<(i64, String)> = {
                let mut st = conn.prepare("SELECT id, path FROM roots ORDER BY id")?;
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<_, _>>()?
            };
            for (id, path) in roots {
                println!("== {path}");
                let t = Instant::now();
                let stats = scanstore::scan_root(
                    &mut conn,
                    id,
                    &ListOptions::default(),
                    &mut |p| match p {
                        scanstore::Progress::Source { index, total, name } => {
                            println!("  [{}/{total}] {name}", index + 1)
                        }
                        scanstore::Progress::Warning(w) => println!("  ! {w}"),
                    },
                )?;
                println!(
                    "  sources {} · listed {} · reused {} · removed {} · errors {} ({:.1}s)",
                    stats.sources,
                    stats.listed,
                    stats.reused,
                    stats.removed,
                    stats.errors,
                    t.elapsed().as_secs_f64()
                );
            }
            let entries: i64 = conn.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?;
            println!("entries in index: {entries}");
        }
        ["extract", id, out] => {
            let (kind, path, chain): (String, String, String) = conn.query_row(
                "SELECT s.kind, s.path, e.chain FROM entries e JOIN sources s ON s.id = e.source_id WHERE e.id = ?",
                [id.parse::<i64>()?],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let chain: Vec<String> = serde_json::from_str(&chain)?;
            let kind = if kind == "folder" {
                SourceKind::Folder
            } else {
                SourceKind::Archive
            };
            let mut f = std::fs::File::create(out)?;
            let n = extract_entry(kind, &PathBuf::from(path), &chain, &mut f)?;
            println!("wrote {n} bytes to {out}");
        }
        _ => {
            return Err("usage: stlx add <dir> | stlx scan | stlx extract <entry_id> <out>".into());
        }
    }
    Ok(())
}
