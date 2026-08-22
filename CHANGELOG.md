# Changelog

Every change *(that is not a tiiiny one)* will be documented in this file.  

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed

- `script!` macro now always explicitly returns a `Result<Script<'c>, Error>`  
  instead of automatically trying to call `?` on it's own functions.  

## [0.1.1] - 2026-08-21 

### Added

- Symlinks to root readme in sub-crates  
- docs.rs metadata to build only for windows target