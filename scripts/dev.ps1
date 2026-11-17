# Runs the service on this machine with the development overlay (one-click sign-in, no identity provider).
# The Mongo URI comes from the git-ignored .secrets\mongo-uri.txt and stays in this process's environment;
# the session key is a fixed development value, good for cookies on localhost only.
$root = Split-Path -Parent $PSScriptRoot
$env:MONGO__CONNECTION_STRING = (Get-Content (Join-Path $root '.secrets\mongo-uri.txt') -Raw).Trim()
$env:SESSION__KEY = 'development-only-session-key-0123456789abcdef'
$env:RUST_ENVIRONMENT = 'development'
# The service reads appsettings.toml from its working directory.
Set-Location (Join-Path $root 'src\traveler')
cargo run --locked --package traveler
