# resolved/build_lib

## Manually building `lua5.1.lib`

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