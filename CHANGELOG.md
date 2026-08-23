# Changelog

Every change *(that is not a tiiiny one)* will be documented in this file.  

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `ToLuaRef`, which `item_ref` in `Script::named_arg_ref` & `Script::arg_ref` now uses.  
  `impl ToLuaRef` is now the new type so `ItemRefList` can also be used,  
  in which it will use the source reference as the reference variable

### Changed

- `script!` macro now always explicitly returns a `Result<Script<'c>, Error>`  
  instead of automatically trying to call `?` on it's own functions.  

- Internal `script!` generated arguments are now a rolling id that is tracked.  
  Exact same behavior from the caller, but their variable names aren't used in the script.  
  variables are still synced and maintain their reference but are named something like:  
  `__r0`, `__c0`, `__c1` to avoid any collision in names.  
  This also provides the benefit of having a *constant~* length of the names, so less bytes to handle later on.

## [0.1.1] - 2026-08-21 

### Added

- Symlinks to root readme in sub-crates  
- docs.rs metadata to build only for windows target