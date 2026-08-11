<div align="center">

# resolved

Execute `Lua` code with *DaVinci Resolve Studio's* **Scripting API** in `Rust`

</div>

> [!IMPORTANT]  
> This crate only works on **Windows**  
> See [`Why Windows Only?`](#why-windows-only) further down for details.

> [!NOTE]  
> This crate only works with *DaVinci Resolve* ***Studio***, aka the paid version.  
> This will not work ever in the *free* version and theres nothing i can do.

*DaVinci Resolve* exposes a **Scripting API** via `Lua` or `Python`, but this crate only supports `Lua`.  
From `Rust` you can send a piece of `Lua` code to *DaVinci Resolve* and get the resulting value back in `Rust`.

This makes it easy to externally call *DaVinci Resolve* and automate tasks and do stuff with the values returned.  
With the power of how this crate is built and it's custom lua module, most simple API calls take *less* than a millisecond.

## Install

[**`tokio`**](https://crates.io/crates/tokio) is required to use `resolved` due to it being async with it.

```toml
[dependencies]
resolved = "*"
tokio = { version = "*", features = ["rt-multi-thread", "macros"] }
```

## Usage

Below is a very simple example on how you may go about to get the version string from the **Scripting API**.  

```rust ignore
// Simple
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;
    let version: String = resolve.execute("return self:GetVersionString()").await?;
    Ok(())
}
```

---

But there is 4 parts to understand to fully utilize this crate: `resolve`, `execute`, `store` and `script`.  
And we'll go over all of them right here:

### Resolve

`Resolve` is a struct which holds a *single-threaded* connection to *DaVinci Resolve*.  
Any instance of `Resolve` in Rust can cheaply be cloned and it still references that one connection.  

Any and all functions to execute code will end up in the `Resolve` struct which will make it happen.  
It mostly handles the networking to it's linked custom lua module which has control over the lua context.  

`Resolve` instances are by far the slowest part of this crate, so try and re-use instances.

```rust ignore
// Resolve
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;
    Ok(())
}
```

Notice how I said *single-threaded*, one `Resolve` instance can only process one piece of code at a time.  
To more easily execute multiple pieces of code at the same time, you can use `PooledResolve`.  
This will hold `n` amount instances in an internal pool and  
pick any available `Resolve` instance which currently *isn't* executing code.  

If all instances are taken, the next call to `PooledResolve` will wait until one instance is available.  
This order is fair, so the first one waiting will be the first one to get the next available instance.

The following example would be able to run *4* tasks at a time:

```rust ignore
// PooledResolve
use resolve::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
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

The sweet spot for smaller pieces of code is around 2-6, but heavily depends on the lifetime of every code execution.

### Execute

`.execute` is the function you'll call to well, *execute* any piece of lua code with *DaVinci Resolve's* **Scripting API**.  
The input of this function can be any type of `str`, `String`, `Cow<'_, str>`, you name it.  

The root of the **Scripting API** is a function called `Resolve()`  
which returns an instance, which holds all other API functions in it.  

This instance is always available through the `resolve` global variable.  
Or when executing code from a `Resolve` instance, you can also use `self`.  
*(Tho `self` can be changed if you execute from a reference, we''ll touch on that in [store](#store))*

```rust ignore
// Execute
use resolved::prelude::*;

const QUIT: &str = "return resolve:Quit()";

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;
    resolve.execute::<()>(QUIT).await?;
    Ok(())
}
```

### Store

`.store` is a very powerful function that you can use to get references to `lua` variables in `Rust`.  
How does that work? Your client doesn't even have any lua logic in it at all.  

Well, the value returned from the code executed from `.store`, is stored in the lua module.  
And you get an `id` which can be used to retrieve this value later on in another execution.  

This struct that you get is an `ItemRef`, you can call both `.execute` and `.store` on this, just as on `Resolve`.  
Any execution on an `ItemRef`, will change the global varialbe `self` to the stored value it references.  

Normally, `GetProjectManager` returns an instance to well the *project manager*,  
which is a value we can't serialize and send back to `Rust` normally.  
But with `.store` we can return a *reference* to it. *(tho our stored value can be any value really)*

```rust ignore
// Store
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;
    let p_manager = resolve.store("return self:GetProjectManager()").await?;

    let folder: String = p_manager.execute("return self:GetCurrentFolder").await?;
    Ok(())
}
```

See how we use `self` in both, when we run `.execute` on `Resolve`, it is the global *resolve* instance.  
Then when execute with our `ItemRef` which holds a reference to our *project manager*,  
it changes `self` to that insance so we can use it. Basically, `self` is your active execution instance.

When a `ItemRef` is dropped, it will send a message to the lua context and garbage collect the stored value.

### Script

TODO: argument, building scripts

We can execute code and store references,  
but what if we want to include our `Rust` variables as arguments to a piece of lua code?  

You may have noticed that both `.execute` & `.store` take in `impl Into<Script<'_>>` as their argument.  
Strings automatically gets converted to a so called `Script`.  

But we can build this `Script` ourself and now include arguments to it which will be sent along side the lua code.  
This basic example adds 2 arguments which we can access in different ways:

```rust ignore
// Script
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;
    
    let script = Script::new("return arg[1] + a")
        .arg(5)?
        .named_arg("a", 3)?;

    let result: i32 = resolve.execute(script).await?;
    assert_eq!(8, result);

    Ok(())
}
```

The `Script` as a builder pattern for it's arguments.  

Any values added with `.arg` will be pushed to a global lua variable called `arg`.  
So in the example we can access this argument with `arg[1]`, so it will equal `5` when it runs.  

We can also name our values with `.named_arg` which will take in the variable name and the value.  
This makes it easy to reference specific arguments in our lua code.  
Note that your variables can't be named `self`, as that is reserved for `resolve` and or `ItemRef` execution.

If you now tried to pass a `ItemRef` as argument *(a reference to an already existing lua value)*,  
You would get an error as `ItemRef` doesn't implement `Serialize`.  

To use references as arguments, both with `.arg` and `.named_arg`.  
You can use their `_ref` variants: `arg_ref` and `named_arg_ref`.  
These behave the exact same but instead you pass in an `ItemRef` instead of a direct value.

```rust ignore
// Script Ref
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;

    let media_storage = resolve.store("return self:GetMediaStorage").await?;
    
    let script = Script::new(
        r#"
            local current_page = self:GetCurrentPage()
            return media:GetFileList("/")
        "#
        )
        .named_arg_ref("media", &media_storage)?;

    let result: Vec<String> = resolve.execute(script).await?;

    Ok(())
}
```

In this example, we can both call with `self` and `media` which both holds instances to lua-only variables.

## Scripting API Documentation

The current **Scripting API** can be found [`here`](https://gist.github.com/X-Raym/2f2bf453fc481b9cca624d7ca0e19de8).  
Or locally if you got DaVinci Resolve installed:  
`C:\ProgramData\Blackmagic Design\DaVinci Resolve\Support\Developer\Scripting\README.txt`  

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
