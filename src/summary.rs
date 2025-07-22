use anyhow::{bail, Context, Result};
use move_compiler_v2::{run_move_compiler_to_stderr, Options as CompilerOptions};
use move_model::model::{EnvDisplay, GlobalEnv};
use serde::Serialize;
use std::{env, fs, path::PathBuf};

#[derive(Serialize)]
struct PackageSummary {
    modules: Vec<String>,
    stats: Stats,
}

#[derive(Serialize)]
struct Stats {
    structs: usize,
    functions: usize,
}

fn summarize(env: &GlobalEnv) -> PackageSummary {
    let modules = env
        .get_modules()
        .map(|m| m.get_name().display(env).to_string())   
        .collect();

    let (s_count, f_count) = env
        .get_modules()
        .fold((0, 0), |(s, f), m| (s + m.get_struct_count(), f + m.get_function_count()));

    PackageSummary {
        modules,
        stats: Stats {
            structs: s_count,
            functions: f_count,
        },
    }
}

fn main() -> Result<()> {
    // Parse CLI arg
    let arg = env::args().nth(1).context("Usage: <bin> <FILE|DIR>")?;
    let path = PathBuf::from(arg);

    let pkg_root = if path.is_dir() {
        path
    } else {
        let tmp = tempfile::tempdir().context("create temp package")?;
        let root = tmp.path();
        fs::write(root.join("Move.toml"), "[package]\nname=\"scratch\"\nversion=\"0.0.0\"\n")?;
        let src_dir = root.join("sources");                               // ← literal folder
        fs::create_dir_all(&src_dir)?;
        fs::copy(&path, src_dir.join("main.move"))?;
        root.to_path_buf()
    };

    // Build compiler options
    let mut opts = CompilerOptions::default();
    let src_path = pkg_root.join("sources");
    opts.sources = vec![src_path.to_string_lossy().into()];
    opts.named_address_mapping = vec![
        "BasicCoin=0x1".to_string(),
        "std=0x1".to_string(),
    ];
    opts.dependencies = vec![
        "../aptos-core/third_party/move/move-stdlib/sources".to_string(),
    ];

    // Compile
    let (env, _units) = run_move_compiler_to_stderr(opts)?;

    if env.has_errors() {
        bail!("Compilation failed");
    }

    // Emit summary JSON
    let summary = summarize(&env);
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
