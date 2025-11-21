//! FormaLens CLI - Semantic search for FormaLang projects
//!
//! Usage:
//!   formalens index [--root <path>]     Index the codebase
//!   formalens search <query> [--limit]  Search the index
//!   formalens clear                     Clear the index

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use formalens_semantic::{Indexer, SearchEngine};

/// FormaLens - Semantic search toolkit for FormaLang projects
#[derive(Parser)]
#[command(name = "formalens")]
#[command(about = "Semantic search toolkit for FormaLang projects")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index the codebase for semantic search
    Index {
        /// Root directory to index (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        root: PathBuf,

        /// File patterns to index (e.g., "*.md", "*.fv", "*.rs")
        #[arg(short, long, default_values = ["*.md", "*.fv", "*.rs"])]
        patterns: Vec<String>,

        /// Clear existing index before indexing
        #[arg(long)]
        clear: bool,
    },

    /// Search the indexed codebase
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,

        /// Show full content (not truncated)
        #[arg(long)]
        full: bool,
    },

    /// Clear the search index
    Clear,

    /// Show index statistics
    Stats,
}

fn get_db_path() -> String {
    // Store index in .formalens directory
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let cache_dir = home.join(".formalens");
    std::fs::create_dir_all(&cache_dir).ok();
    cache_dir.join("index.lancedb").to_string_lossy().to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = get_db_path();

    match cli.command {
        Commands::Index {
            root,
            patterns,
            clear,
        } => {
            println!("Initializing indexer...");
            let indexer = Indexer::new(&db_path).await?;

            if clear {
                println!("Clearing existing index...");
                indexer.clear().await?;
            }

            println!("Indexing {} with patterns {:?}...", root.display(), patterns);
            let patterns: Vec<&str> = patterns.iter().map(|s| s.as_str()).collect();
            let stats = indexer.index_directory(&root, &patterns).await?;

            println!("{}", stats);
        }

        Commands::Search { query, limit, full } => {
            let engine = SearchEngine::new(&db_path).await?;

            if !engine.has_index().await? {
                eprintln!("No index found. Run 'formalens index' first.");
                std::process::exit(1);
            }

            println!("Searching for: {}\n", query);
            let results = engine.search(&query, limit).await?;

            if results.is_empty() {
                println!("No results found.");
                return Ok(());
            }

            for (i, result) in results.iter().enumerate() {
                println!("{}. {} (score: {:.3})", i + 1, result.path, result.score);
                println!("   Lines: {}-{}", result.line_start, result.line_end);

                if let Some(title) = &result.title {
                    println!("   Title: {}", title);
                }

                println!("   Type: {}", result.chunk_type);

                // Show content preview
                let content = if full {
                    result.content.clone()
                } else {
                    let lines: Vec<&str> = result.content.lines().take(5).collect();
                    let preview = lines.join("\n");
                    if result.content.lines().count() > 5 {
                        format!("{}...", preview)
                    } else {
                        preview
                    }
                };

                println!("   ---");
                for line in content.lines() {
                    println!("   {}", line);
                }
                println!();
            }
        }

        Commands::Clear => {
            let indexer = Indexer::new(&db_path).await?;
            indexer.clear().await?;
            println!("Index cleared.");
        }

        Commands::Stats => {
            let engine = SearchEngine::new(&db_path).await?;

            if !engine.has_index().await? {
                println!("No index found.");
            } else {
                println!("Index exists at: {}", db_path);
                // Could add more stats here in the future
            }
        }
    }

    Ok(())
}
