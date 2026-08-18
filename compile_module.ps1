# first we'll generate the .lib file based off DaVinci Resolve's lua dll
cargo run --package build_lib --release -- "./build_lib/generated" rerun
Copy "./build_lib/generated/lua5.1.lib" "prebuilt/lua5.1.lib"

# and before building the dll we must set the path to the dir of .lib so mlua-sys build.rs can find it
$LUA_PATH = Resolve-Path "prebuilt"
$env:LUA_LIB = $LUA_PATH

# then use that to compile it with mlua in the lua module
cargo build --package lua_module --profile lua
Copy "target/lua/lua_module.dll" "prebuilt/lua_module.dll"

# build one with tracing enabled
cargo build --package lua_module --profile lua --features tracing
Copy "target/lua/lua_module.dll" "prebuilt/lua_module_tracing.dll"