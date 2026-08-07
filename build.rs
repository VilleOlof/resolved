use std::{
    env::{self, VarError},
    fs::{File, copy},
    io,
    path::PathBuf,
    process::Command,
};

fn main() {
    lua_lib();
    lua_module();
}

fn lua_module() {
    println!("cargo::rerun-if-changed=./lua_module/src");
    println!("cargo::rerun-if-changed=./lua_module/Cargo.toml");

    let status = Command::new("cargo")
        .args(["build", "-p", "lua_module", "--release"])
        .status()
        .unwrap();
    assert!(status.success());

    let mut root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    root.push("target/release/lua_module.dll");

    let mut out = PathBuf::from(env::var("OUT_DIR").unwrap());
    out.push("lua_module.dll");

    copy(root, out).unwrap();
}

fn lua_lib() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let lib_src = out.join(LIB);

    if !lib_src.exists() {
        println!("building '{LIB}' from scratch");

        // we just want to open file to check permission
        let dll_path = davinci_resolve_path();
        match File::open(&dll_path) {
            Ok(_) => (),
            Err(e) => match e.kind() {
                io::ErrorKind::PermissionDenied => panic!(
                    "Cannot read {}, Access was denied, try running cargo build with Administrator permissions or copy the file to a directory where cargo can access it and set '{ENV_DV_LUA}' to point at it",
                    dll_path.display()
                ),
                _ => panic!("{e:?}"),
            },
        }

        let dumpbin_out = Command::new("dumpbin")
            .arg("/exports")
            .arg(dll_path.display().to_string())
            .output()
            .unwrap();
        let dumpbin = String::from_utf8(dumpbin_out.stdout)
            .unwrap()
            .lines()
            .map(|s| s.to_string())
            .collect();

        let exports = parse_dumpbin(dumpbin);

        let def_file = format!(
            r#"LIBRARY lua5.1.dll
EXPORTS
{}"#,
            exports.join("\n")
        );

        let def_path = out.join(DEF);
        std::fs::write(def_path, def_file).unwrap();

        Command::new("lib")
            .args([
                &format!("/def:{DEF}"),
                "/machine:x64",
                &format!("/out:{LIB}"),
            ])
            .current_dir(&out)
            .spawn()
            .unwrap();
    }

    unsafe {
        env::set_var("LUA_LIB", out.to_string_lossy().to_string());
    }
}

const DEFAULT_DAVINCI_RESOLVE: &str =
    "C:/Program Files/Blackmagic Design/DaVinci Resolve/lua5.1.dll";
const ENV_DV_LUA: &str = "DAVINCI_RESOLVE_LUA_DLL";
const LIB: &str = "lua5.1.lib";
const DEF: &str = "lua5.1.def";
fn davinci_resolve_path() -> PathBuf {
    match env::var(ENV_DV_LUA) {
        Ok(s) => PathBuf::from(s),
        Err(VarError::NotPresent) => PathBuf::from(DEFAULT_DAVINCI_RESOLVE),
        Err(VarError::NotUnicode(s)) => panic!("Not Unicode: {s:?}"),
    }
}

// 100% better way to parse this but it works
fn parse_dumpbin(lines: Vec<String>) -> Vec<String> {
    let mut exports = Vec::new();

    let mut found_exports = false;
    let mut i = 0;
    for _ in 0..lines.len() {
        let line = &lines[i];

        if !found_exports {
            if line.contains("RVA") {
                found_exports = true;
                i += 1; // skip empty line
            }

            i += 1;
            continue;
        }

        let export = line.trim().split(' ').last().unwrap();
        exports.push(export.to_string());

        i += 1;

        if line.is_empty() {
            break;
        }
    }

    exports
}
