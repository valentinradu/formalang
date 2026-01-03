//! FormaLang Compiler CLI
//!
//! Usage:
//!   fvc compile <file.fv> [-o output.fvc]
//!   fvc check <file.fv>
//!   fvc watch <file.fv>

use formalang::codegen::{generate_wgsl, transpile_wgsl, FvcWriter, ShaderTarget};
use formalang::{compile_to_ir, report_errors};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    match args[1].as_str() {
        "compile" => {
            if args.len() < 3 {
                eprintln!("Error: Missing input file");
                print_usage();
                return ExitCode::from(1);
            }
            compile_command(&args[2], parse_output_path(&args))
        }
        "check" => {
            if args.len() < 3 {
                eprintln!("Error: Missing input file");
                print_usage();
                return ExitCode::from(1);
            }
            check_command(&args[2])
        }
        "watch" => {
            if args.len() < 3 {
                eprintln!("Error: Missing input file");
                print_usage();
                return ExitCode::from(1);
            }
            watch_command(&args[2])
        }
        "help" | "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        "version" | "--version" | "-v" => {
            println!("fvc {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        cmd => {
            eprintln!("Error: Unknown command '{}'", cmd);
            print_usage();
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    println!("FormaLang Compiler v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage:");
    println!("  fvc compile <file.fv> [-o output.fvc]  Compile to .fvc binary");
    println!("  fvc check <file.fv>                    Type check without compiling");
    println!("  fvc watch <file.fv>                    Watch and recompile on changes");
    println!("  fvc help                               Show this help");
    println!("  fvc version                            Show version");
}

fn parse_output_path(args: &[String]) -> Option<PathBuf> {
    for i in 0..args.len() - 1 {
        if args[i] == "-o" || args[i] == "--output" {
            return Some(PathBuf::from(&args[i + 1]));
        }
    }
    None
}

fn compile_command(input_path: &str, output_path: Option<PathBuf>) -> ExitCode {
    let start = Instant::now();

    println!("Compiling {}...", input_path);

    // Read source file
    let source = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", input_path, e);
            return ExitCode::from(1);
        }
    };

    // Determine output path
    let output = output_path.unwrap_or_else(|| {
        let mut p = PathBuf::from(input_path);
        p.set_extension("fvc");
        p
    });

    // Compile to IR
    let ir = match compile_to_ir(&source) {
        Ok(ir) => ir,
        Err(errors) => {
            eprintln!("{}", report_errors(&errors, &source, input_path));
            return ExitCode::from(1);
        }
    };

    // Generate WGSL
    let wgsl = generate_wgsl(&ir);

    // Validate WGSL (optional - for debugging)
    if let Err(e) = formalang::codegen::validate_wgsl(&wgsl) {
        eprintln!("Warning: Generated WGSL validation failed: {}", e);
    }

    // Transpile to SPIR-V
    let spirv = match transpile_wgsl(&wgsl, ShaderTarget::SpirV) {
        Ok(result) => result.code.as_binary().map(|b| b.to_vec()),
        Err(e) => {
            eprintln!("Warning: SPIR-V transpilation failed: {}", e);
            None
        }
    };

    // Build FVC file
    let mut fvc_writer = FvcWriter::new();

    // Add struct definitions
    for s in &ir.structs {
        let fields: Vec<(String, u32)> = s
            .fields
            .iter()
            .map(|f| (f.name.clone(), 0)) // Type tag 0 for now
            .collect();
        fvc_writer.add_struct(&s.name, &fields);
    }

    // Add WGSL shader
    fvc_writer.set_wgsl_shader(wgsl);

    // Add SPIR-V if available
    if let Some(spirv_bytes) = spirv {
        fvc_writer.set_spirv_shader(spirv_bytes);
    }

    // Write output
    let fvc_bytes = match fvc_writer.to_bytes() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error generating FVC: {}", e);
            return ExitCode::from(1);
        }
    };

    if let Err(e) = fs::write(&output, &fvc_bytes) {
        eprintln!("Error writing {}: {}", output.display(), e);
        return ExitCode::from(1);
    }

    let duration = start.elapsed();
    println!(
        "Compiled {} -> {} ({} bytes) in {:.2}ms",
        input_path,
        output.display(),
        fvc_bytes.len(),
        duration.as_secs_f64() * 1000.0
    );

    ExitCode::SUCCESS
}

fn check_command(input_path: &str) -> ExitCode {
    let start = Instant::now();

    println!("Checking {}...", input_path);

    // Read source file
    let source = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", input_path, e);
            return ExitCode::from(1);
        }
    };

    // Compile (type check)
    match compile_to_ir(&source) {
        Ok(ir) => {
            let duration = start.elapsed();
            println!(
                "OK: {} structs, {} traits, {} enums ({:.2}ms)",
                ir.structs.len(),
                ir.traits.len(),
                ir.enums.len(),
                duration.as_secs_f64() * 1000.0
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprintln!("{}", report_errors(&errors, &source, input_path));
            ExitCode::from(1)
        }
    }
}

fn watch_command(input_path: &str) -> ExitCode {
    use std::thread;
    use std::time::Duration;

    println!("Watching {}... (Ctrl+C to stop)", input_path);

    let path = PathBuf::from(input_path);
    let mut last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

    loop {
        thread::sleep(Duration::from_millis(500));

        let current_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();

        if current_modified != last_modified {
            last_modified = current_modified;

            println!("\n--- File changed, recompiling... ---\n");

            let output_path = {
                let mut p = path.clone();
                p.set_extension("fvc");
                Some(p)
            };

            let result = compile_command(input_path, output_path);
            if result != ExitCode::SUCCESS {
                println!("Compilation failed. Watching for more changes...");
            }
        }
    }
}
