# resolved architecture

how the crate works, why we have a custom lua module, and how everythings connects

## Terms

Client = the rust `resolved` crate that the consumer interfaces with.  
Module = the lua module which runs when the `fuscript` binary starts  
Resolve/DaVinci = DaVinci Resolve and it's Scripting API

## Goal

we want to use most of the Scripting API DaVinci Resolve provides in Rust.  
to do this we could reverse engineer the `fusionscript.dll` which is what `fuscript.exe` uses internally.  
this would be a massive undertaking that I personally dont have the experience in.  

Instead we can use the already existing `fuscript.exe` which provides a CLI for us to interact with DaVinci with.  
This binary take in either lua or python and executes it with the Scripting API provided.  

For this we will use lua as it's simpler, doesnt suck ass *(sorry python but no)* and we can do some funky stuff with it.  

Outside from execution lua code from Rust into the Scripting API,  
we want to be able to return the values in the script back to Rust so we can use it outside the Scripting API.

Preferebly we also want to be able to store references to to lua-only variables *(userdata objects)*.  
since we want serialize those we would need to find a workaround for this.  


## Lua module

We only start `fuscript.exe` once since we dont want to deal with it's startup time *(and references)*.  
Once the Scripting API has started we run our own lua module which never exits until the client drops or dies.  

This module communicates with the client to agree on some configuration,  
in the process the module binds to a random available port and sends that back to the client.  

The modules work is to maintain references to lua-only values with it's `ItemRefHandler`.  
And to set script arguments so when the script actually loads and runs it has all the context it needs.  

The module has a lifetime bound to the client, when the client drops it sends a shutdown signal to the module.  
Or if the module doesnt recieve a pong back from the client it will self terminate to not be left hanging.

Once the startup has been done, the module starts its `TcpListener` that the client will request to for all user packets.  

### Lua version

DaVinci uses lua 5.1 and has a dll file called `lua5.1.dll` in its installation.  
This is the file which `fuscript` loads and uses to execute lua code with.  

But the crate we use to handle lua (mlua) looks for a `lua51.dll` file, notice the missing `.`  
Due to this we need to recompile a `.lib` file from DaVinci's `lua5.1.dll` file so we can use that when linking `mlua-sys`.  
we set both `LUA_LIB_NAME` and `LUA_LIB` to properly compile our module

[*lua_module/readme*](/lua_module/readme.md) & [*/build_lib/building*](/build_lib/building.md) has more info on this exact problem.

## Client

The client handles all communication to the module which in turns actually executes the lua code.  
Through the client, it also starts the module itself as a child process to the crate. 

When starting the client, it will create a temporary directory to write some files in.  
This directory contains the lua script to pass to `fuscript` later,  
but notably this directory also contains the `lua_module.dll` which is our entire custom rust lua module.  
This file needs to be in a place where the lua script can find it and properly load it

Every client also has a random u64 id (random enough for us),  
which identifies this specific client and it's module connection.  
When we get a `ItemRef` from a `.store` function, that item holds onto its derived client.  
Meaning that when it gets used again, the client will check `ItemRef`'s derived resolve id to see if it matches.  
If they dont, the item reference is from another lua vm context and arent valid to use since they can point to `nil` or another unexpected `value` since the item reference id is a rolling id starting at 0

Once the client has spawned `fuscript` and it has now started our module.  
We can send our provided configuration to the module so it can properly setup and get ready for incoming requests.  
Once the module is ready the module sends a ready packet indicating that we,  
the client and finally return the client back to the consumer of the crate to use.  

When the consumer wants to execute a `Script` object, we serialize it and send a packet to the module.  
this packet contains our entire serialized script and it's optional argument values.  

After module has done its things, the returned value is deserialized and sent to the consumer back.  

## Script

To easily support arguments to lua scripts, `Script` can be passed any argument values that implement `Serialize`.  
These values are sent along side the lua script string and is added as global variables in the lua context  
before loading and running the lua script. 

Nameless arguments are pushed to `arg` as a sequence.  
And named arguments are simply added as global variables.

## ItemRef

If we didnt hold up `fuscript` forever and just exited after we returned our value.  
we wouldnt be able to hold long living references to lua variables.  

under the hood, every object from DaVinci in the Scripting API has a underlying UUID which is its Remote Object Id.  
We *can* access this for some objects, but we cant use it again to lookup values and instances.  

So we can instead with our long living `fuscript` and module,  
store these values in the lua registry and return back a simple u64 id to the client that is a reference to the registry key.  

When we want to use these variables, instances and more.  
We use our u64 id, lookup the registry key which looks up the proper value.  
*(we cant directly return the registrykey as it must live inside the lua vm)*  
with this value we can use it global variables for the consumers script to access again.

all resolve objects are thus also a userdata object, which we for sure cannot serialize in anyway.  

## A normal request

- Create a new `Resolve` client
    - Creates a temporary directory to write the module dll and lua script to
    - Starts the client server for pre request communication
    - Generates a unique id to this `Resolve` instance to hinder misuse of `ItemRef`'s
    - Spawns `fuscript` with the port of it's client server formatted into the lua script  
    - Module recieves the client configuration and sets up the module for the client to further connect
    - Starts the ping/pong handlers, so the module can ensure it's linked client is always alive
- Building a `Script` object with some lua code and optional arguments to pass along with it  
- The client runs `.execute`  
    - The client packs the `Script` object and sends it to a new connection to the module  
        - If the client used a `.execute_with` function this will also attach the `ItemRef` to the script  
            - This also validates that the references match the same internal `Resolve` instance
    - Module recieves the `Script` and it's payload  
        - Optionally resets the global table
        - It sets up globals, self references and loads the script
        - Executes the lua code  
        - Saves the execution time of the script  
        - Sends back the returned `LuaValue`, serialized into a buffer
    - Client recieves the results of the executed lua script  
        - Deserializes it into T & returns it

## Helpers

To easier compile the module, you can use [`compile_module.ps1`](/compile_module.ps1).  
this script runs `build_lib`, which assumes:  
- you have a default installation pathof DaVinci  
- have `dumpbin` and `lib` installed from MSVC build tools  

manual building instructions can be found in [*build_lib/building*](/build_lib/building.md).  
then it will build and move around some files to populate `/prebuilt` with the lua `.lib` and module `.dll` file.

---

To easier test the crate without even having DaVinci installed or running, we provide a `fudummy` binary.  
Which replicates just enough of `fuscript`'s behavior for use to use it as a dummy binary to run our scripts with.  

This binary runs a lua vm just as `fuscript` but without the while Scripting API.  
This is enough for us to test the networking, references, items, globals, configurations, packets, client lifetime and execute.  
[`run_tests_with_dummy.ps1`](/run_tests_with_dummy.ps1) makes running these dummy tests really easy.  
It builds the dummy binary, sets up the paths *(assuming you the default installation path for DaVinci)*  
and runs the tests, then cleaning up after itself.
