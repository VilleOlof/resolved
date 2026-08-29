# Changelog

Every change *(that is not a tiiiny one)* will be documented in this file.  

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `source()` on `ItemRefList` to expose the lists source reference.  
- `values()` on `ItemRefList` that behaves exactly like it's `keys()` function,  
  and returns all values serialized as `T` at once.

- `new_ref!`, creates a new struct wrapper around `ItemRef` that behaves the exact same.  
- `new_ref_list!`, creates a new struct wrapper around `ItemRefList` that behaves the exact same.  

- `store_list_option` to `Resolve` and `ItemRef` which *maybe* returns a reference to a lua table.  
  Since single `store` has a `store_option`, the list variant should also have one.
- `store_list_option` to `ResolveStore` trait

- `is_locked` function to `Resolve` which tries to briefly get the lock.  
  if it fails, some other task is executing code and is thus locked.

### Fixed

- `fuscript`'s `stdout`, `stderr` and `stdin` are properly handled and null'd now.  
- spawning of `fuscript` now never creates a new window regardless of it's process context.  

### Removed

- `__seal__::Sealed` bounds from `ResolveStore` & `ResolveExecute`, now consumer *can* implement these themself.  
  This was made so `new_ref!` macro can auto implement it in it's macro so the wrappers behaves exactly like ItemRefs.

## [0.2.0] - 2026-08-26

### Added

- `ToLuaRef`, which `item_ref` in `Script::named_arg_ref` & `Script::arg_ref` now uses.  
  `impl ToLuaRef` is now the new type so `ItemRefList` can also be used,  
  in which it will use the source reference as the reference variable

- `Void` that discards and skips serializing the returned value while executing.  
  Used as the return type when running `.execute`.

- `is_resolve_available` option to `ResolveConfig` where you specify a root Scripting API function,  
   This function will be checked *(but not executed)* to see if *DaVinci Resolve* is still open.  
   Added has an option since it adds an extra call to the underlying API.

- `remove_named_arg` to `Script` which removes a named argument with a specified key.  
- `named_args` to `Script` that lists all named arguments currently attached to the `Script`.

### Changed

- `script!` macro now always explicitly returns a `Result<Script<'c>, Error>`  
  instead of automatically trying to call `?` on it's own functions.  

- Internal `script!` generated arguments are now a rolling id that is tracked.  
  Exact same behavior from the caller, but their variable names aren't used in the script.  
  variables are still synced and maintain their reference but are named something like:  
  `__r0`, `__c0`, `__c1` to avoid any collision in names.  
  This also provides the benefit of having a *constant~* length of the names, so less bytes to handle later on.

- All `execute` functions now require `T` to be `'static`,  
  this shouldnt change anything since `T` must also implement `DeserializeOwned`.  
  This is for `Void` and `TypeId::of` to properly work.

- `Script::named_arg` and `Script::named_arg_ref` now overwrites named arguments with the same key.

## [0.1.1] - 2026-08-21 

### Added

- Symlinks to root readme in sub-crates  
- docs.rs metadata to build only for windows target