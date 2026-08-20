# resolved &emsp; [<img alt="github" src="https://img.shields.io/badge/github-villeolof/resolved-5b969b?style=for-the-badge&labelColor=555555&logo=github" height="24">](https://github.com/VilleOlof/resolved) [<img alt="crates.io" src="https://img.shields.io/crates/v/resolved?style=for-the-badge&logo=rust&color=5b7a9b" height="24">](https://crates.io/crates/resolved) [<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-resolved-5b659b?style=for-the-badge&labelColor=555555&logo=docs.rs" height="24">](https://docs.rs/resolved)

Execute `Lua` code with *DaVinci Resolve Studio's* **Scripting API** in `Rust`

---

> [!IMPORTANT]  
> This crate only works on **Windows**  
> See [`Why Windows Only?`](#why-windows-only) further down for details.

> [!NOTE]  
> This crate only works with *DaVinci Resolve* ***Studio***, *(the paid version)*.  
> This will not work in the *free* version.  

*DaVinci Resolve* exposes a **Scripting API** via `Lua` for us to use so we can interact with it.  
From `Rust` you can send a piece of `Lua` code to *DaVinci Resolve* and get the resulting value back in `Rust`.

This makes it easy to externally call *DaVinci Resolve* and automate tasks and do stuff with the values returned.  
With the power of how this crate is built and it's custom lua module,  
some *simple* calls take *less* than `20µs` *(that's 0.00002 !! and that's for the entire execution!)*.

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
Any instance of `Resolve` in Rust can be cheaply cloned and it still references that one connection.  

Any and all functions to execute code will end up in the `Resolve` struct which will make it happen.  
It mostly handles the communication to it's linked custom lua module which has control over the lua context.  

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
                "resolve:GetCurrentPage()"
            ).await.unwrap();
        });
    }

    Ok(())
}
```

The sweet spot for smaller pieces of code is around 2-6, but heavily depends on the lifetime of every code execution.

`Resolve` instances can be configured with `ResolveConfig`, this can also be passed into `PooledResolve`.  
Start an instance with a configuration with `new_with_config`.  
Settings that may be configured are:  
- Default `Script` timeout  
- If globals should be reset between executions  
- Globals that persist in all scripts  
- Module tracing logging to file

See `ResolveConfig` for all fields and what exactly they do.  
It has two default configs: `default()` and `keep_globals()`

### Execute

`.execute` is the function you'll call to well, *execute* any piece of lua code with *DaVinci Resolve's* **Scripting API**.  
The input of this function can be any type of `str`, `String`, `Cow<'_, str>`, `Script`, you name it.  
*(we'll touch on what a `Script` is later)*

The root of the **Scripting API** is a function called `Resolve()`  
which returns an instance, which holds all other API functions in it.  

This instance is always available through the `resolve` global variable.  
Or when executing code from a `Resolve` instance, you can also use `self`.  
*(Tho `self` can be changed if you execute from a reference, we''ll touch on that in [store](#store))*

```rust ignore
// Execute
use resolved::prelude::*;

const QUIT: &str = "resolve:Quit()";

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
    let p_manager = resolve.store("self:GetProjectManager()").await?;

    let folder: String = p_manager.execute("self:GetCurrentFolder").await?;
    Ok(())
}
```

See how we use `self` in both, when we run `.execute` on `Resolve`, it is the global *resolve* instance.  
Then when we execute with our `ItemRef` which holds a reference to our *project manager*,  
it changes `self` to that instance so we can use it. Basically, `self` is your active execution instance.

If the stored reference is a *serializeable* value, you can call `.value<T>()` on an `ItemRef` to get it's actual value.

When a `ItemRef` is dropped, it will send a message to the lua context and garbage collect the stored value.

#### Store List

Sometimes the `ItemRef` that you return is a `Table` *(map or array)*, and maybe you'd want to iterate over it... in *rust*  
Using `.store_list` you can get a list of references that points to all values in the returned `Table`.  

Not only can you easily iterate over all references with `.list()` ,  
but it's also really performant with large lists.

Normally each `ItemRef` sends a `DropItem` packet when it goes out of scope, blocking the client for just a tiny bit.  
Now if you had a few hundred items and all of them tries to send a packet at once? That's gonna block for a while.  
`ItemRefList`, the returned list from `.store_list`, drops all of it's references at once in a single packet.

To iterate over all clips in a timeline track and get their name, you could do something like:

```rust ignore
// Store List
use resolved::prelude::*;

#[tokio::main]
async fn main() -> ResolveResult<()> {
    let resolve = Resolve::new().await?;
    let timeline = resolve.store(r#"
        local pm = self:GetProjectManager()
        local p = pm:GetCurrentProject()
        return p:GetCurrentTimeline()
    "#).await?;

    let clips = timeline.store_list(r#"self:GetItemListInTrack("video", 1)"#).await?;

    for clip in &clips.list() {
        let name: String = clip.execute("self:GetName()").await?;
        println!("clip: {name:?}");
    }

    Ok(())
}
```

**Note that the returned must be a `Table` for this to work, otherwise it will error**

And obivously this would be faster to do directly in lua but then you would have no references,  
no access to each element in Rust to do more execution one or even save just the ones you want.

---

Both `ItemRef` and `ItemRefList` can be cheaply cloned and the internal module references  
won't ever get dropped until you've dropped your last reference in your code.  
So an `ItemRef` is always a valid reference, assuming you've done no *unsafe* code.

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
    
    let script = Script::new("arg[1] + a")
        .arg(5)?
        .named_arg("a", 3)?;

    let result: i32 = resolve.execute(script).await?;
    assert_eq!(8, result);

    Ok(())
}
```

The `Script` has a builder pattern for it's arguments.  

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

    let media_storage = resolve.store("self:GetMediaStorage()").await?;
    
    let script = Script::new(
            r#"return media:GetFileList("/")"#
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

    let media = resolve.store(script! { self:GetMediaStorage() }).await?;
    // Reference ItemRef's with: `@`
    let result: Vec<String> = resolve.execute(script! {
        return @media:GetFileList("/")
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

The following features are **enabled by default**:

- `macros`  
    Enables `script!` macro to write *Lua* in *Rust* with references to variables.
- `pool`  
    Enables `PooledResolve` which can contain multiple instances to execute multiple things at the same time  

Optional features:

- `tracing`  
    Enables [`tracing`](https://github.com/tokio-rs/tracing) logging,  
    this only logs `trace` events during client setup and packet handling.

    Note that the lua module can also have tracing enabled to a `module.log` file in it's [instance dir](#data-path).  
    This option is enabled through `ResolveConfig` during runtime and *not* at compile time.  
    *(tho if you dont enable it, you won't get any performance hit. It's 100% excluded)*

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

## Data Path

This library will write temporary files and module log files to:  
`C:\Users\$USER\AppData\Local\resolved`  
Where every instance created will have their own sub-directory named after their internal `id`.

## Benchmarks

### Resolve client baseline
Time to create a new `Resolve` instance.  
This benchmarks connect to the test dummy binary instead of DaVinci Resolve.  
Also measures the startup of the lua module.

| Metric    | Time        |
|-----------|-------------|
| Mean      | `45.824 ms` |
| Std. Dev. | `6.6860 ms` |
| Median    | `44.184 ms` |
| MAD       | `1.9834 ms` |

### Resolve client
Time to create a new `Resolve` instance that connects to DaVinci Resolve.  
This also measures the startup time of the lua module.

| Metric    | Time        |
|-----------|-------------|
| Mean      | `147.79 ms` |
| Std. Dev. | `195.00 ms` |
| Median    | `50.828 ms` |
| MAD       | `4.9415 ms` |

### Script execution baseline
This is time to execute an empty script.  
This mostly measures the communication, serializing, execution and request handling

| Metric    | Time        |
|-----------|-------------|
| Mean      | `19.372 µs` |
| Std. Dev. | `1.9273 µs` |
| Median    | `18.919 µs` |
| MAD       | `1.0058 µs` |

## Why Windows Only?

This crate heavily depends on using `.dll` files to custom Rust lua modules to work in DaVinci Resolve's Scripting API enviroment.  
Mostly because DaVinci Resolve's lua file is named `lua5.1.lib` where most others *(including mlua)*, expects a `lua51.lib`.  
This causes a clash which fails our custom lua module not to properly work.  

So we need to recompile a new `.lib` file with the exports from DaVinci Resolve's `.dll` file to make our own working `.lib` file which `mlua-sys` can properly link against.  
See [`lua_module`](./lua_module/readme.md) for more info on it.  

And because of confusing dependency and build script problems, this `lua_module.dll` file is prebuilt and included in the library, but can be built yourself. Again see [`lua_module`](./lua_module/readme.md) & [`build_lib`](./build_lib/building.md) for more on building it.

I personally don't own a desktop Apple device *(DaVinci Resolve on linux doesn't even support this type of Scripting API)* so it's very difficult for me to make this library work on that platform.

The passing of data between the `.dll` and the library also uses *shared memory* and *named pipes*.  
These implementations are also exclusive to Windows, so don't expect this for any other platform.

This crate has only been tested on `Windows 11 25H2 x64`

## Tests

To easier run and execute tests without having *DaVinci Resolve Studio* open,  
there is a `fudummy` binary which replicates the behavior of the real `fuscript.exe` binary.  

This dummy binary will take in the same arguments and execute the script without the **Scripting API**.  
But this is enough to test communication, packets, registries, references, serializing and more core functionality.  

### Running Dummy Tests

There is a `scripts/run_tests_with_dummy.ps1` script which automates some of the process of running dummy tests.  
This expects the artifacts of `./scripts/compile_module.ps1` to exist in `/prebuilt`.

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

If the tests hang, panic or the `scripts/run_tests_with_dummy.ps1` script doesn't fully execute,  
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
cargo test -- --skip dummy --skip resolve
```

### Resolve tests

There's a few tests that requires a *DaVinci Resolve* version that supports the following API's used in each of their tests.

```bash
# calls resolve:GetVersionString
cargo test resolve::version --features tracing -- --nocapture
# or this for a fully* optimized test
cargo test resolve::version --features tracing --profile bench -- --nocapture
```

### Lua Module tests

Theres lot a ton of tests for the module only since it requires a lot of things that the client does.  
But there's still a few ones to test the ones that we can test.  

These can simply be ran with: 
```bash
# in project root: 
./scripts/test_module.ps1
```

To manually run it you can look at what that script does,  
mostly it's setting up env variables for the module to compile properly.

## Unsafe

For those who care a lot about safety:  

This crate has a bit of unsafe code, but only for shared memory access between the lua module and the client crate.  
If you'd like to analyze the unsafe code, all of it remains in these files:  
- [`resolve_shared/mem.rs`](./resolved_shared/src/mem.rs)  
- [`resolved/put.rs`](./src/put.rs)  
- [`lua_module/reader.rs`](./lua_module/src/reader.rs)

The crate gives some more safety so that both of the processes won't ever access it at the same time.  
This is with help of events through named pipes and the first byte in the shared memory-  
being who currently owns access to it.  
Is this ever mismatched, the current process won't attempt to access any of it and fail.