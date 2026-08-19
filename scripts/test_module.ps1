$LUA_PATH = Resolve-Path "prebuilt"
$env:LUA_LIB = $LUA_PATH

# so we can restore path later
$old_path = $env:Path;

# add DaVinci Resolve's directory to path so Lua::new() within tests can find `lua5.1.dll`
$env:Path += ";C:/Program Files/Blackmagic Design/DaVinci Resolve"

cargo test --package lua_module

$env:Path = $old_path;