use std::{
    env::{self, VarError},
    fs::File,
    io,
    path::{Path, PathBuf},
    process::Command,
};

use which::which;

fn main() {
    lua_lib();
}

const NOT_INSTALLED_ERR: &str = "currently isn't installed or not in a place where the build script can reach it (like $PATH). Install dumpbin or build the 'lua5.1.lib' manually to get vinci to compile. See https://github.com/VilleOlof/vinci#building";
const DEFAULT_DAVINCI_RESOLVE: &str =
    "C:/Program Files/Blackmagic Design/DaVinci Resolve/lua5.1.dll";
const ENV_DV_LUA: &str = "DAVINCI_RESOLVE_LUA_DLL";
const DEF: &str = "lua5.1.def";
const LIB: &str = "lua5.1.lib";
const DLL: &str = "lua5.1.dll";

fn lua_lib() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let lib_src = out.join(LIB);

    if !lib_src.exists() {
        check_installations(&["dumpbin", "lib"]);

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

        let dumpbin = call_dumpbin(&dll_path);
        let exports = parse_dumpbin(dumpbin);

        let def_file = format!(
            r#"LIBRARY {DLL}
EXPORTS
{}"#,
            exports.join("\n")
        );

        let def_path = out.join(DEF);
        std::fs::write(def_path, def_file).unwrap();

        call_lib(&out);
    }

    // panic!("{out:?}");
    unsafe {
        env::set_var("LUA_LIB", out.to_string_lossy().to_string());
    }
    println!("cargo:rustc-env=LUA_LIB={}", out.display());
}

fn davinci_resolve_path() -> PathBuf {
    match env::var(ENV_DV_LUA) {
        Ok(s) => PathBuf::from(s),
        Err(VarError::NotPresent) => PathBuf::from(DEFAULT_DAVINCI_RESOLVE),
        Err(VarError::NotUnicode(s)) => panic!("Not Unicode: {s:?}"),
    }
}

fn call_dumpbin(dll_path: &Path) -> Vec<String> {
    let dumpbin_out = Command::new("dumpbin")
        .arg("/exports")
        .arg(dll_path.display().to_string())
        .output()
        .unwrap();
    String::from_utf8(dumpbin_out.stdout)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect()
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

fn call_lib(out: &Path) {
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

fn check_installations(binaries: &[&str]) {
    for bin in binaries {
        match which(bin) {
            Ok(_) => (),
            Err(which::Error::CannotFindBinaryPath) => panic!("{bin} {NOT_INSTALLED_ERR}"),
            Err(e) => panic!("{e:?}"),
        }
    }
}
