use anyhow::{bail, Context, Result};
use move_compiler_v2::{run_move_compiler_to_stderr, Options as CompilerOptions};
use move_model::{
    ast::{Attribute, Exp, Spec},
    model::{FieldEnv, FunctionEnv, GlobalEnv, ModuleEnv, StructEnv},
    ty::TypeDisplayContext,
};
use serde::Serialize;
use std::{env, fs, path::PathBuf};

#[derive(Serialize)]
struct AttrJson {
    name: String,
    value: String,
}

#[derive(Serialize)]
struct ExpJson {
    kind: String,
    value: Option<String>,
    children: Vec<ExpJson>,
}

#[derive(Serialize)]
struct FunJson {
    name: String,
    params: Vec<String>,
    ret: String,
    attrs: Vec<AttrJson>,
    body: Option<ExpJson>,
}

#[derive(Serialize)]
struct StructJson {
    name: String,
    fields: Vec<String>,
    attrs: Vec<AttrJson>,
}

#[derive(Serialize)]
struct ModuleJson {
    name: String,
    structs: Vec<StructJson>,
    functions: Vec<FunJson>,
    attrs: Vec<AttrJson>,
}

#[derive(Serialize)]
struct PackageJson {
    modules: Vec<ModuleJson>,
}

fn attrs_to_json(attrs: &[Attribute], env: &GlobalEnv) -> Vec<AttrJson> {
    attrs
        .iter()
        .map(|a| AttrJson {
            name: env.symbol_pool().string(a.name()).to_string(),
            value: format!("{a:?}"),
        })
        .collect()
}

fn exp_to_json(e: &Exp, env: &GlobalEnv) -> ExpJson {
    use move_model::ast::ExpData::*;
    match &e.as_ref() {
        Value(_, v) => ExpJson {
            kind: "Value".to_string(),
            value: Some(format!("{v:?}")),
            children: vec![],
        },
        LocalVar(_, sym) => ExpJson {
            kind: "LocalVar".to_string(),
            value: Some(sym.display(env.symbol_pool()).to_string()),
            children: vec![],
        },
        Call(_, oper, args) => ExpJson {
            kind: format!("Call::{oper:?}"),
            value: None,
            children: args.iter().map(|e| exp_to_json(e, env)).collect(),
        },
        Block(_, _, _, body) => ExpJson {
            kind: "Block".to_string(),
            value: None,
            children: vec![exp_to_json(body, env)],
        },
        Loop(_, body) => ExpJson {
            kind: "Loop".to_string(),
            value: None,
            children: vec![exp_to_json(body, env)],
        },
        Assign(_, _, rhs) => ExpJson {
            kind: "Assign".to_string(),
            value: None,
            children: vec![exp_to_json(rhs, env)],
        },
        Return(_, vals) => ExpJson {
            kind: "Return".to_string(),
            value: None,
            children: vec![exp_to_json(vals, env)],
        },
        IfElse(_, c, t, e2) => ExpJson {
            kind: "IfElse".to_string(),
            value: None,
            children: vec![exp_to_json(c, env), exp_to_json(t, env), exp_to_json(e2, env)],
        },
        other => ExpJson {
            kind: format!("Unhandled::{other:?}"),
            value: None,
            children: vec![],
        },
    }
}

fn struct_to_json(s: &StructEnv, env: &GlobalEnv) -> StructJson {
    let tctx = s.get_type_display_ctx();
    StructJson {
        name: s.get_name().display(env.symbol_pool()).to_string(),
        fields: s
            .get_fields()
            .map(|f| {
                format!(
                    "{}: {}",
                    f.get_name().display(env.symbol_pool()),
                    f.get_type().display(&tctx)
                )
            })
            .collect(),
        attrs: attrs_to_json(s.get_attributes(), env),
    }
}

fn function_to_json(f: &FunctionEnv, env: &GlobalEnv) -> FunJson {
    let tctx = f.get_type_display_ctx();
    FunJson {
        name: f.get_name().display(env.symbol_pool()).to_string(),
        params: f
            .get_parameters()
            .iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    p.get_name().display(env.symbol_pool()),
                    p.get_type().display(&tctx)
                )
            })
            .collect(),
        ret: f.get_result_type().display(&tctx).to_string(),
        attrs: attrs_to_json(f.get_attributes(), env),
        body: f.get_def().map(|d| exp_to_json(d, env)),
    }
}

fn module_to_json(m: &ModuleEnv) -> ModuleJson {
    let env = m.env;
    ModuleJson {
        name: m.get_name().display_full(env).to_string(),
        structs: m.get_structs().map(|s| struct_to_json(&s, env)).collect(),
        functions: m
            .get_functions()
            .map(|f| function_to_json(&f, env))
            .collect(),
        attrs: attrs_to_json(m.get_attributes(), env),
    }
}

fn main() -> Result<()> {
    let arg = env::args().nth(1).context("Usage: <bin> <FILE|DIR>")?;
    let path = PathBuf::from(arg);

    let pkg_root = if path.is_dir() {
        path
    } else {
        let tmp = tempfile::tempdir().context("create temp package")?;
        let root = tmp.path();
        fs::write(
            root.join("Move.toml"),
            "[package]\nname = \"scratch\"\nversion = \"0.0.0\"\n",
        )?;
        let src_dir = root.join("sources");
        fs::create_dir_all(&src_dir)?;
        fs::copy(&path, src_dir.join("main.move"))?;
        root.to_path_buf()
    };

    let mut opts = CompilerOptions::default();
    opts.sources = vec![pkg_root.join("sources").to_string_lossy().into()];
    opts.named_address_mapping = vec!["BasicCoin=0x1".into(), "std=0x1".into()];
    opts.dependencies = vec!["../aptos-core/third_party/move/move-stdlib/sources".into()];

    let (env, _units) = run_move_compiler_to_stderr(opts)?;
    if env.has_errors() {
        bail!("Compilation failed");
    }

    let pkg_json = PackageJson {
        modules: env.get_modules().map(|m| module_to_json(&m)).collect(),
    };
    println!("{}", serde_json::to_string_pretty(&pkg_json)?);
    Ok(())
}
