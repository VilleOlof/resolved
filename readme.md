<div align="center">

# resolved

Execute `Lua` code with *DaVinci Resolve's* **Scripting API** in `Rust`

</div>

> [!IMPORTANT]  
> This crate only works on **Windows**  
> See [`Why Windows Only?`](#why-windows-only) further down for details.

*DaVinci Resolve* exposes a **Scripting API** via `Lua` or `Python`, but this crate only supports `Lua`.  
From `Rust` you can send a piece of `Lua` code to *DaVinci Resolve* and get the resulting value back in `Rust`.

This makes it easy to externally call *DaVinci Resolve* and automate tasks and do stuff with the values returned.  
With the power of how this crate is built and it's custom lua module, most simple API calls take only a few milliseconds.  

## Install

[**`tokio`**](https://crates.io/crates/tokio) is required to use `resolved` due to it being async with it.

```toml
[dependencies]
resolved = "*"
tokio = { version = "*", features = ["rt-multi-thread", "macros"] }
```

## Usage

By far the slowest part is creating `Resolve` instances, so try and re-use the same one.  
The code when executing can return a value which will be sent back to your code after it's finished executing.  
This value must impl `DeserializeOwned` from [`serde`](https://crates.io/crates/serde).

```rust ignore
use resolved::{Resolve, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let resolve = Resolve::new().await?;
    let version = resolve.execute::<String>(
        r#"return self:GetVersionString"#
    ).await?;
    Ok(())
}
```

### Global Variables

You might have seen the `self` variable in the example above, and that's a special global variable.  
In every script executed, you have access to `self`/`resolve` *(both are the same)*.  
The Scripting API all diverges from the root `Resolve()` instance, like getting the version or project manager.  
This instance of resolve never changes while the program is running so it's been fetched once and stored in globals.  

```lua
local proj_manager = self:GetProjectManager()
local current_proj = proj_manager:GetCurrentProject()
return current_proj:GetName()
```

This example returns the current project's name, using `self` to get the project manager.  
The current **Scripting API** can be found [`here`](https://gist.github.com/X-Raym/2f2bf453fc481b9cca624d7ca0e19de8).  
Or locally if you got DaVinci Resolve installed:  
`C:\ProgramData\Blackmagic Design\DaVinci Resolve\Support\Developer\Scripting\README.txt`  

### References

<TODO: ItemRef>

## Pooled Instances

If you need to execute multiple scripts at the same time you can use a `PooledResolve`.  
The specified amount of instances will be spun up at the pools creation and instances will be re-used when available.  
If all instances are currently in use then the next call will wait until theres one available instances.  

The sweet spot for smaller scripts are around 2-6 instances, but depends heavily on how long running your scripts are.  

```rust ignore
use resolve::{PooledResolve, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let pool = PooledResolve::new(4);

    for _ in 0..512 {
        let _pool = pool.clone();
        tokio::spawn(async move {
            let _ = _pool.execute::<String>(
                "return resolve:GetCurrentPage()"
            ).await.unwrap();
        });
    }

    Ok(())
}
```

## DaVinci Resolve Path

The library needs to know where *DaVinci Resolve* is installed to find the proper executable to run the scripts.  
By default, this path is:  
`C:/Program Files/Blackmagic Design/DaVinci Resolve/fuscript.exe`  
Which the library will attempt to use.  

But if your installation is on any different path for any reason,  
you can set the enviromental variable `FUSCRIPT` to the path to the `fuscript.exe` in your installation.

## Why Windows Only?

This crate heavily depends on using `.dll` files to custom Rust lua modules to work in DaVinci Resolve's Scripting API enviroment.  
Mostly because DaVinci Resolve's lua file is named `lua5.1.lib` where most others *(including mlua)*, expects a `lua51.lib`.  
This causes a clash which fails our custom lua module not to properly work.  

So we need to recompile a new `.lib` file with the exports from DaVinci Resolve's `.dll` file to make our own working `.lib` file which `mlua-sys` can properly link against.  
See [`lua_module`](./lua_module/readme.md) for more info on it.  

And because of confusing dependency and build script problems, this `lua_module.dll` file is prebuilt and included in the library, but can be built yourself. Again see [`lua_module`](./lua_module/readme.md) & [`build_lib`](./build_lib/building.md) for more on building it.

I personally don't own a desktop Apple device *(DaVinci Resolve on linux doesn't even support this type of Scripting API)* so it's very difficult for me to make this library work on that platform.

This crate has only been tested on `Windows 11 25H2 x64`
