## How to contribute to `resolved`

It's all pretty easy and standard to contribute to this but there are some important notes.

#### Found a bug?

* **Make sure it's not already an issue, try and search for it**

* **Specify DaVinci Resolve Version and build**

* Try and include an example that produces your bug.  
  Keep it small and isolated.

#### Submitting changes

* Send a pull request that clearly specifices your changes and motivation,  
  if you can, please also include tests for your changes.  

* Make sure all existing tests pass.  

* If you touch performance sensitive parts *(like the codeflow for `.execute`)*,  
  benchmark your changes before and after to make sure they don't affect it too much.

* Always format your code with `rustfmt` before submitting changes

---

When submitting a PR, ***don't*** include any files in the `/prebuilt` directory.  
If you've compiled the `lua_module` for testing, discard them when submitting your actual code changes.  

This is because I can't easily verify that you haven't tampered with them.  
So for security reasons, currently only the author will compile these after your PR has been merged.