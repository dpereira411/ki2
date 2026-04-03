use std::path::Path;

use ki2::loader::load_schematic_tree;
use ki2::parser::parse_schematic_file;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("usage: ki2 validate <path> [--tree]");
        std::process::exit(2);
    };

    match command.as_str() {
        "validate" => {
            let mut tree = false;
            let mut path = None;
            for arg in args {
                if arg == "--tree" {
                    tree = true;
                } else if path.is_none() {
                    path = Some(arg);
                } else {
                    eprintln!("unexpected argument: {arg}");
                    std::process::exit(2);
                }
            }

            let Some(path) = path else {
                eprintln!("usage: ki2 validate <path> [--tree]");
                std::process::exit(2);
            };

            let result = if tree {
                load_schematic_tree(Path::new(&path)).map(|loaded| loaded.schematics.len())
            } else {
                parse_schematic_file(Path::new(&path)).map(|_| 1usize)
            };

            match result {
                Ok(count) => println!("validated {count} schematic(s)"),
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("unknown command: {command}");
            std::process::exit(2);
        }
    }
}
