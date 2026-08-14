<div align="center">

# resolved

Execute `Lua` code with *DaVinci Resolve Studio's* **Scripting API** in `Rust`

</div>

> [!IMPORTANT]  
> This crate only works on **Windows**  
> See [`Why Windows Only?`](#why-windows-only) further down for details.

> [!NOTE]  
> This crate only works with *DaVinci Resolve* ***Studio***, aka the paid version.  
> This will not work ever in the *free* version, and there's nothing I can do.

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
Any execution on an `ItemRef`, will change the global variable `self` to the stored value it references.  

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
it changes `self` to that instance so we can use it. Basically, `self` is your active execution instance.

When a `ItemRef` is dropped, it will send a message to the lua context and garbage collect the stored value.

### Script

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

#### Script Macro

To easier construct scripts with arguments from `Rust`, you can use the `script!` macro for this.  
This macro makes it easy to directly reference *Rust Variables* inside your `Lua` code.

The above script examples can be written like:

```rust ignore
// script!
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;

    // Reference Rust variables with: `$`
    let (a, b) = (5, 3);
    let result: i32 = resolve.execute(script! {
        return $a + $b
    }).await?;
    assert_eq!(8, result);

    let media = resolve.store("return self:GetMediaStorage").await?;
    // Reference ItemRef's with: `#`
    let result: Vec<String> = resolve.execute(script! {
        local current_page = self:GetCurrentPage()
        return #media:GetFileList("/")
    }).await?;

    Ok(())
}
```

We can just write any `Lua` code we want\* inside `Rust` and with our variables seamlessly.  
Any value passed in using `$` must implement `Serialize`.  
If you read how `Script` works normally with arguments,  
you'd see why `ItemRef` has a special prefix for it.  
*(It doesn't implement Serialize and needs to call it's own `named_arg_ref` function to pass it in)*

Look at the `script!` docs for more information on this.

## Features

Both of the following features are **enabled by default**:

- `macros`  
    Enables `script!` macro to write *Lua* in *Rust* with references to variables.
- `pool`  
    Enables `PooledResolve` which can contain multiple instances to execute multiple things at the same time

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
you can set the environment variable `FUSCRIPT` to the path to the `fuscript.exe` in your installation.

## Benchmarks

### Resolve client baseline
Time to create a new `Resolve` instance.  
This benchmarks connect to the test dummy binary instead of DaVinci Resolve.  
Also measures the startup of the lua module.

| Metric    | Time        |
|-----------|-------------|
| Mean      | `65.692 ms` |
| Std. Dev. | `19.488 ms` |
| Median    | `57.668 ms` |
| MAD       | `8.2494 ms` |

### Resolve client
Time to create a new `Resolve` instance that connects to DaVinci Resolve.  
This also measures the startup time of the lua module.

| Metric    | Time        |
|-----------|-------------|
| Mean      | `547.41 ms` |
| Std. Dev. | `6.1195 ms` |
| Median    | `550.04 ms` |
| MAD       | `5.1227 ms` |

### Script execution baseline
This is time to execute an empty script.  
This mostly measures the networking, serializing and request handling

| Metric    | Time        |
|-----------|-------------|
| Mean      | `267.04 µs` |
| Std. Dev. | `109.68 µs` |
| Median    | `246.40 µs` |
| MAD       | `102.29 µs` |

## Why Windows Only?

This crate heavily depends on using `.dll` files to custom Rust lua modules to work in DaVinci Resolve's Scripting API enviroment.  
Mostly because DaVinci Resolve's lua file is named `lua5.1.lib` where most others *(including mlua)*, expects a `lua51.lib`.  
This causes a clash which fails our custom lua module not to properly work.  

So we need to recompile a new `.lib` file with the exports from DaVinci Resolve's `.dll` file to make our own working `.lib` file which `mlua-sys` can properly link against.  
See [`lua_module`](./lua_module/readme.md) for more info on it.  

And because of confusing dependency and build script problems, this `lua_module.dll` file is prebuilt and included in the library, but can be built yourself. Again see [`lua_module`](./lua_module/readme.md) & [`build_lib`](./build_lib/building.md) for more on building it.

I personally don't own a desktop Apple device *(DaVinci Resolve on linux doesn't even support this type of Scripting API)* so it's very difficult for me to make this library work on that platform.

This crate has only been tested on `Windows 11 25H2 x64`

## Tests

To easier run and execute tests without having *DaVinci Resolve Studio* open,  
there is a `fudummy` binary which replicates the behavior of the real `fuscript.exe` binary.  

This dummy binary will take in the same arguments and execute the script without the **Scripting API**.  
But this is enough to test networking, packets, registries, references, serializing and more core functionality.  

### Running Dummy Tests

There is a `run_tests_with_dummy.ps1` script which automates some of the process of running dummy tests.  
This expects the artifacts of `./compile_module.ps1` to exist in `/prebuilt`.

The script does the following:  
- Builds `fudummy`
- Adds the default DaVinci Resolve installation path to `$Path`  
    *This is so `fudummy` can find the correctly named lua `.dll` (`lua5.1.dll`)*  
- Sets environment variable `FUSCRIPT` to the built `fudummy` binary  
- Runs all tests labeled `dummy` with the `test-dummy` feature enabled

If your *DaVinci Resolve* installation is not in the default path or you something you can do all of this manually.  
`fudummy` just needs access to a dll named `lua5.1.dll` so it can run and replicate `fuscript`'s behavior.  
Then for the actual tests it needs `FUSCRIPT` to be pointing at the `fudummy` binary for it to be used instead.

This `lua5.1.dll` can also be a `lua51.dll` but renamed to `lua5.1.dll` if you don't have *DaVinci Resolve* installed

If the tests hang, panic or the `run_tests_with_dummy.ps1` script doesn't fully execute,  
to reset your environment back to before:
```bash
$env:FUSCRIPT = $null
# or your previous configured path
```
Then the crate will work again since it temporary replaces that env variable to use the dummy binary. 

### Running Crate Tests

To run crate tests that don't ever need some kind of `Resolve` instance,  
you can run the normal test command but skip any *dummy* tests.

```bash
cargo test -- --skip dummy
```