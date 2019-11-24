# Usage

```
nix-weather 0.1.0

USAGE:
    nix-weather [FLAGS] [OPTIONS] [--] [drv]...

FLAGS:
    -h, --help                  Prints help information
        --json                  Output statistics in JSON
    -p, --percentage-as-exit    Output coverage percentage as exit code
    -q, --quiet                 
    -V, --version               Prints version information
    -v, --verbose               

OPTIONS:
    -c, --cache <cache>...
            Which HTTP(s) binary caches to query, tried in order of appearance [default: https://cache.nixos.org]

    -n, --narinfo-concurrency <narinfo-concurrency>      How many .narinfo files to fetch concurrently [default: 32]
    -m, --narinfo-max-attempts <narinfo-max-attempts>    How often to try to fetch a .narinfo file [default: 3]

ARGS:
    <drv>...    Which derivation to collect coverage statistics for (must reside in store)
```
