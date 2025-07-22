use anyhow::{bail, Context, Result};
use move_compiler_v2::{run_move_compiler_to_stderr, Options as CompilerOptions};
use move_model::model::{
    EnvDisplay, FieldEnv, FunctionEnv, GlobalEnv, ModuleEnv, StructEnv,
};
use move_model::ty::TypeDisplayContext;
use serde::Serialize;
use std::{env, fs, path::PathBuf};

// JSON structures 

#[derive(Serialize)]
struct AstJson {
    modules: Vec<ModuleJson>,
}

#[derive(Serialize)]
struct ModuleJson {
    name: String,
    address: String,
    is_script: bool,
    structs: Vec<StructJson>,
    functions: Vec<FunctionJson>,
}

#[derive(Serialize)]
struct StructJson {
    name: String,
    abilities: Vec<String>,
    type_params: Vec<String>,
    fields: Vec<FieldJson>,
    is_native: bool,
    is_ghost_memory: bool,
}

#[derive(Serialize)]
struct FieldJson {
    name: String,
    ty: String,
    offset: usize,
    variant: Option<String>,
}

#[derive(Serialize)]
struct FunctionJson {
    name: String,
    visibility: String,
    kind: String,
    type_params: Vec<String>,
    parameters: Vec<ParamJson>,
    results: Vec<String>,
    is_native: bool,
    is_intrinsic: bool,
    is_entry: bool,
}

#[derive(Serialize)]
struct ParamJson {
    name: String,
    ty: String,
}

// AST traversal 

fn build_ast(env: &GlobalEnv) -> AstJson {
    AstJson {
        modules: env.get_modules().map(module_to_json).collect(),
    }
}

fn module_to_json(m: ModuleEnv) -> ModuleJson {
    let address = m.env.display(m.get_name().addr()).to_string();

    ModuleJson {
        name: m.get_name()
            .name()
            .display(m.symbol_pool())
            .to_string(),
        address,
        is_script: m.is_script_module(),
        structs: m.get_structs().map(struct_to_json).collect(),
        functions: m.get_functions().map(function_to_json).collect(),
    }
}

fn struct_to_json(s: StructEnv) -> StructJson {
    let ctx = s.get_type_display_ctx();
    StructJson {
        name: s.get_name().display(s.symbol_pool()).to_string(),
        abilities: s
            .get_abilities()
            .into_iter()
            .map(|a| format!("{a:?}"))
            .collect(),
        type_params: s
            .get_type_parameters()
            .iter()
            .map(|tp| tp.0.display(s.symbol_pool()).to_string())
            .collect(),
        fields: s.get_fields().map(|f| field_to_json(&ctx, f)).collect(),
        is_native: s.is_native(),
        is_ghost_memory: s.is_ghost_memory(),
    }
}

fn field_to_json(ctx: &TypeDisplayContext, f: FieldEnv) -> FieldJson {
    FieldJson {
        name: f.get_name().display(ctx.env.symbol_pool()).to_string(),
        ty: f.get_type().display(ctx).to_string(),
        offset: f.get_offset(),
        variant: f
            .get_variant()
            .map(|v| v.display(ctx.env.symbol_pool()).to_string()),
    }
}

fn function_to_json(f: FunctionEnv) -> FunctionJson {
    let ctx = f.get_type_display_ctx();
    FunctionJson {
        name: f.get_name().display(f.symbol_pool()).to_string(),
        visibility: format!("{:?}", f.visibility()),
        kind: format!("{:?}", f.get_kind()),
        type_params: f
            .get_type_parameters()
            .iter()
            .map(|tp| tp.0.display(f.symbol_pool()).to_string())
            .collect(),
        parameters: f
            .get_parameters()
            .into_iter()
            .map(|p| ParamJson {
                name: p.0.display(f.symbol_pool()).to_string(),
                ty: p.1.display(&ctx).to_string(),
            })
            .collect(),
        results: f
            .get_result_type()
            .flatten()
            .into_iter()
            .map(|ty| ty.display(&ctx).to_string())
            .collect(),
        is_native: f.is_native(),
        is_intrinsic: f.is_intrinsic(),
        is_entry: f.is_entry(),
    }
}

// main 

fn main() -> Result<()> {
    let arg = env::args()
        .nth(1)
        .context("Usage: move_ast_exporter <FILE|DIR>")?;
    let path = PathBuf::from(arg);

    let pkg_root = if path.is_dir() {
        path
    } else {
        let tmp = tempfile::tempdir().context("create temp package")?;
        let root = tmp.path();
        fs::write(
            root.join("Move.toml"),
            "[package]\nname=\"scratch\"\nversion=\"0.0.0\"\n",
        )?;
        let src_dir = root.join("sources");
        fs::create_dir_all(&src_dir)?;
        fs::copy(&path, src_dir.join("main.move"))?;
        root.to_path_buf()
    };

    let mut opts = CompilerOptions::default();
    opts.sources = vec![pkg_root.join("sources").to_string_lossy().into()];
    opts.named_address_mapping = vec![
        "BasicCoin=0x1".to_string(),
        "std=0x1".to_string(),
    ];
    opts.dependencies = vec![
        "../aptos-core/third_party/move/move-stdlib/sources".to_string(),
    ];

    let (env, _units) = run_move_compiler_to_stderr(opts)?;
    if env.has_errors() {
        bail!("Compilation failed");
    }

    println!("{}", serde_json::to_string_pretty(&build_ast(&env))?);
    Ok(())
}
