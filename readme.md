# vinci

## Usage

```rust ignore
let resolve = Resolve::new().await?;
let version = resolve.execute::<String>("return self:GetVersionString").await?;
```

## Building

There's a very special file we need before we even start to touch this crate at all.  
DaVinci Resolve's lua file is differently named than what `mlua` expects.  
`lua51.lib` vs `lua5.1.lib`.  
So we need to make a new lib file which uses `5.1` instead of `51`.  

The included `build.rs` will attempt to build this file automatically.  
Assuming that the binaries `dumpbin` and `lib` is available.  
These can be found from [`Additional MSVC Build Tools`](https://learn.microsoft.com/en-us/cpp/build/reference/c-cpp-build-tools).

Once a `lua5.1.lib` file exists and can be found,  
it can be built like anything else

```
cargo build --release
```

If you'd rather build this file yourself, you can do the following instructions:

### Manually building `lua5.1.lib`


Extract all exports from the `lua5.1.dll` file from *DaVinci Resolve*.
```ps1
dumpbin /exports "C:\Program Files\Blackmagic Design\DaVinci Resolve\lua5.1.dll" >> dump.txt
```
In this file you have to filter out all actual export names.  

The rest of the contents can be thrown away, we only care about the names.
```diff
-    ordinal hint RVA      name
-
-          1    0 0003C9C0 __swprintf_l
-          2    1 0003CA20 __vswprintf_l
-          3    2 0003CA90 _fprintf_l
-          4    3 0003CAE0 _fprintf_p
-          5    4 0003CB30 _fprintf_p_l
-        324  143 000418A0 wscanf
-        325  144 00041900 wscanf_s
-        
-  Summary
-
-        3000 .data

+__swprintf_l
+__vswprintf_l
+_fprintf_l
+_fprintf_p
+_fprintf_p_l
+wscanf
+wscanf_s
```
*small output for example*

Then we want to add all of these exports to a `lua5.1.def` file like this:
```def
LIBRARY lua5.1.dll
EXPORTS
__swprintf_l
__vswprintf_l
_fprintf_l
_fprintf_p
_fprintf_p_l
wscanf
wscanf_s
```
*again, the real .dll has a few hundred exports, this is only a selective few for example*

Once we have our `.def` file we want to compile it to an actual `.lib` file:
```bash
lib /def:lua5.1.def /machine:x64 /out:lua5.1.lib
```

Then you'll have to set `LUA_LIB` to the directory which contains our new `lua5.1.lib` file.  
```ps1
$env:LUA_LIB = "path/to/parent"
```
or you could create `.cargo/config.toml` with the following contents
```toml
[env]
LUA_LIB = "path/to/parent"
```