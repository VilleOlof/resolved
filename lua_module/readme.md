# resolved/lua_module

The lua module which is inserted into along side every connection to DaVinci Resolve to hold up a connection to the client

## Building

DaVinci Resolve's lua file is differently named than what `mlua` expects.  
`lua51.lib` vs `lua5.1.lib`.  
So we need to make a new lib file which uses `5.1` instead of `51`.  

`$ROOT/build_lib` is a binary which *(assuming you have `dumpbin` & `lib` installed)*  
can automatically build this `.lib` file for you.  
These can be found from [`Additional MSVC Build Tools`](https://learn.microsoft.com/en-us/cpp/build/reference/c-cpp-build-tools).

Once a `lua5.1.lib` file exists and can be found,  
it can be built like anything else

```sh
cargo build --release
# or the following ps1 script from workspace root to also copy into /prebuilt
./compile_module.ps1
```

If you'd rather build this file yourself, see [`building.md`](../build_lib/building.md)
