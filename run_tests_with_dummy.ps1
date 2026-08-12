# build fudummy test binary that replaces fuscript.exe
cargo build --release --package fudummy

# add DaVinci Resolve's directory to path so fudummy can find `lua5.1.dll`
$env:Path += ";C:/Program Files/Blackmagic Design/DaVinci Resolve"

# set FUSCRIPT to the full path to the binary so we replace fuscript.exe
$OLD_FUSCRIPT = $env:FUSCRIPT;
$env:FUSCRIPT = Resolve-Path "./target/release/fudummy.exe"

# run tests, now without touching DaVinci Resolve
cargo test dummy

# Reset FUSCRIPT
$env:FUSCRIPT = $OLD_FUSCRIPT