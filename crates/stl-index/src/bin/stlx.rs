//! Developer CLI over the index DB (`$STL_BROWSER_HOME` or `~/.stl-browser/index.db`).
//!
//!   stlx add <dir>                  register a folder as a root
//!   stlx index                      scan all roots and resolve changed collections
//!   stlx tree models|files [<path>] [--depth N] [--real]   print a view
//!   stlx extract <entry_id> <out>   extract one indexed file
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use stl_core::vtree::{NodeId, NodeKind, is_file_kind};
use stl_index::{db, paths, resolvestore, scanstore, views};
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
        ["index"] => {
            let t = Instant::now();
            let stats = resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |p| match p {
                scanstore::Progress::Source { index, total, name } => println!("  [{}/{total}] {name}", index + 1),
                scanstore::Progress::Warning(w) => println!("  ! {w}"),
            })?;
            let s = &stats.scan;
            println!(
                "sources {} · listed {} · reused {} · removed {} · errors {} · locked {}",
                s.sources, s.listed, s.reused, s.removed, s.errors, s.locked
            );
            let entries: i64 = conn.query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?;
            println!(
                "resolved {} collections · {} models · {entries} entries ({:.1}s)",
                stats.resolved,
                stats.models,
                t.elapsed().as_secs_f64()
            );
        }
        ["tree", view, rest @ ..] => tree(&conn, view, rest)?,
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
            return Err(USAGE.into());
        }
    }
    Ok(())
}

const USAGE: &str = "usage: stlx add <dir> | index | extract <entry_id> <out> | tree models|files [<path>] [--depth N] [--real]";

/// Print a view (or the subtree at `path`) as an indented list.
fn tree(conn: &rusqlite::Connection, view: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let t = match view {
        "models" => views::models_view(conn)?,
        "files" => views::files_view(conn)?,
        _ => return Err(USAGE.into()),
    };
    let (mut path, mut depth, mut real) = ("", usize::MAX, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--depth" => depth = it.next().ok_or(USAGE)?.parse()?,
            "--real" => real = true,
            p => path = p,
        }
    }
    let start = t.find(path).ok_or_else(|| format!("no such path: {path}"))?;
    let mut stack: Vec<(NodeId, usize)> = t.nodes[start].children.iter().rev().map(|&c| (c, 0)).collect();
    while let Some((id, d)) = stack.pop() {
        let n = &t.nodes[id];
        let suffix = match n.kind {
            NodeKind::Locked => " [locked]",
            k if is_file_kind(k) => "",
            _ => "/",
        };
        let hover = if real { format!("  ← {}", n.real.join(" | ")) } else { String::new() };
        println!("{}{}{suffix}{hover}", "  ".repeat(d), n.name);
        if d + 1 < depth {
            stack.extend(n.children.iter().rev().map(|&c| (c, d + 1)));
        }
    }
    Ok(())
}
