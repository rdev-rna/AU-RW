//! List an archive's entries: index, file id and stored length.
//!
//! World meshes live as their own entries in `DataPC_ACU_Paris_assets.forge`, addressed
//! by *entry* file id rather than by resource file id, so resolving a selector record
//! needs the entry table. `acu-resource` can read an entry but cannot enumerate them.
//!
//! Read-only. Usage:
//!   acu-entries ARCHIVE [--offset N] [--limit N] [--resources]
//!
//! `--resources` also decodes each listed package and prints its resources; it reads
//! every selected entry, so pair it with `--offset`/`--limit`.
use acu_assets::{Limits, Result, forge::Forge};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("usage: acu-entries ARCHIVE [--offset N] [--limit N] [--resources]")?;
    let mut offset = 0usize;
    let mut limit = usize::MAX;
    let mut resources = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--offset" => {
                offset = args
                    .get(index + 1)
                    .ok_or("--offset needs a count")?
                    .parse()?;
                index += 2;
            }
            "--limit" => {
                limit = args
                    .get(index + 1)
                    .ok_or("--limit needs a count")?
                    .parse()?;
                index += 2;
            }
            "--resources" => {
                resources = true;
                index += 1;
            }
            other => return Err(format!("unknown argument {other:?}").into()),
        }
    }

    let limits = Limits::default();
    let mut forge = Forge::open(path, limits)?;
    let total = forge.entries.len();
    for position in 0..total.min(offset.saturating_add(limit)) {
        if position < offset {
            continue;
        }
        let entry = forge.entries[position].clone();
        if !resources {
            println!(
                "{}\t{}\t{}",
                entry.index, entry.file_id, entry.stored_length
            );
            continue;
        }
        match forge.read_package(position) {
            Ok(package) => {
                println!(
                    "{}\t{}\t{}\t{} resources",
                    entry.index,
                    entry.file_id,
                    entry.stored_length,
                    package.resources.len()
                );
                for resource in &package.resources {
                    println!(
                        "\t{}\t{}\t{}",
                        resource.file_id, resource.type_id, resource.name
                    );
                }
            }
            Err(error) => println!(
                "{}\t{}\t{}\tdecode error: {error}",
                entry.index, entry.file_id, entry.stored_length
            ),
        }
    }
    eprintln!(
        "entries: {total} total, listed {offset}..{}",
        total.min(offset.saturating_add(limit))
    );
    Ok(())
}
