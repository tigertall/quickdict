mod engine;
#[cfg(feature = "gui")]
mod config;
#[cfg(feature = "gui")]
mod application;
#[cfg(feature = "gui")]
mod window;
#[cfg(feature = "gui")]
mod preferences;
#[cfg(feature = "gui")]
mod capture;
#[cfg(feature = "gui")]
mod views;

use std::path::PathBuf;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    // GUI 模式 (默认)
    if !cfg!(feature = "gui") || args.len() > 1 {
        // CLI 模式（无 GUI 或 有参数）
        run_cli(&args);
    } else {
        #[cfg(feature = "gui")]
        run_gui();
        #[cfg(not(feature = "gui"))]
        {
            eprintln!("GUI support not compiled. Use: cargo run -- <word> --dict <path>");
            eprintln!("Commands: lookup, info, scan");
        }
    }
}

#[cfg(feature = "gui")]
fn run_gui() {
    let app = application::DictionaryApplication::new();
    app.run();
}

fn run_cli(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        eprintln!("Commands:");
        eprintln!("  gui                   Launch GUI (default)");
        eprintln!("  lookup <word> --dict <path>    Look up a word");
        eprintln!("  info <ifo_path>               Show dictionary info");
        eprintln!("  scan <directory>              Scan and list dictionaries");
        return;
    }

    match args[1].as_str() {
        "gui" => {
            #[cfg(feature = "gui")]
            run_gui();
            #[cfg(not(feature = "gui"))]
            eprintln!("GUI not compiled.");
        }
        "lookup" => cmd_lookup(args),
        "info" => cmd_info(args),
        "scan" => cmd_scan(args),
        _ => eprintln!("Unknown command: {}", args[1]),
    }
}

fn cmd_lookup(args: &[String]) {
    let word = args.get(2).expect("Missing word argument");
    let dict_path = parse_dict_arg(args);

    match dict_path {
        Some(path) => {
            match engine::dict_reader::DictionaryReader::open(&path) {
                Ok(dict) => {
                    println!("Dictionary: {}", dict.info.bookname);
                    println!("Looking up: {}", word);
                    println!("---");

                    match dict.lookup_exact(word) {
                        Some(article) => {
                            println!("{}", article.raw_text);
                        }
                        None => {
                            println!("Word not found. Trying prefix scan...");
                            let results = dict.lookup_prefix(word, 10);
                            if results.is_empty() {
                                println!("No results found.");
                            } else {
                                println!("Suggestions:");
                                for r in &results {
                                    println!("  - {} (from {})", r.word, r.dict_name);
                                }
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error loading dictionary: {}", e),
            }
        }
        None => eprintln!("Error: No dictionary path specified. Use --dict <path>"),
    }
}

fn cmd_info(args: &[String]) {
    let ifo_path = args.get(2).expect("Missing ifo_path argument");
    let path = PathBuf::from(ifo_path);

    match engine::dict_reader::DictionaryReader::open(&path) {
        Ok(dict) => {
            println!("Dictionary: {}", dict.info.bookname);
            println!("Version: {}", dict.info.version);
            println!("Word count: {}", dict.word_count());
            if let Some(ref author) = dict.info.author {
                println!("Author: {}", author);
            }
            if let Some(ref desc) = dict.info.description {
                println!("Description: {}", desc);
            }
            if let Some(ref sametype) = dict.info.sametypesequence {
                let fmt = if sametype.contains('h') || sametype.contains('H') {
                    "HTML"
                } else {
                    "Plain text"
                };
                println!("Format: {}", fmt);
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn cmd_scan(args: &[String]) {
    let dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    let manager = engine::dict_manager::DictManager::new();
    match manager.scan_directories(&[dir]) {
        Ok(found) => {
            println!("Found {} dictionaries:", found.len());
            for (kind, name, path) in &found {
                println!("  - {}:{} ({})", kind, name, path);
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn parse_dict_arg(args: &[String]) -> Option<PathBuf> {
    for i in 0..args.len() {
        if args[i] == "--dict" {
            return args.get(i + 1).map(PathBuf::from);
        }
    }
    None
}
