use std::{ cmp, io, path::PathBuf };

use structopt::StructOpt;
use log::*;
use url::Url;
use number_prefix::{ NumberPrefix, Standalone, Prefixed };

use nix_weather::{
    StoreHash, StoreCache,
    Closure,
    CoverageStatistics,
    derivation::*
};

#[derive(StructOpt, Debug)]
struct Opt {
    /// Which derivation to collect coverage statistics for (must reside in store)
    #[structopt(name = "drv", parse(from_os_str))]
    input_derivations: Vec<PathBuf>,

    /// Which HTTP(s) binary caches to query, tried in order of appearance
    #[structopt(name = "cache", short, long, default_value = "https://cache.nixos.org")]
    cache_roots: Vec<Url>,

    /// How many .narinfo files to fetch concurrently
    #[structopt(short, long, default_value = "32")]
    narinfo_concurrency: u32,

    /// How often to try to fetch a .narinfo file
    #[structopt(short = "m", long, default_value = "3")]
    narinfo_max_attempts: u32,

    /// Output statistics in JSON
    #[structopt(long)]
    json: bool,

    #[structopt(short, long, parse(from_occurrences))]
    verbose: i32,
    #[structopt(short, long, parse(from_occurrences))]
    quiet: i32
}

fn format_bytes(amount: u64) -> String {
    match NumberPrefix::binary(amount as f64) {
        Standalone(bytes) =>   format!("{} bytes", bytes),
        Prefixed(prefix, n) => format!("{:.2} {}B", n, prefix)
    }
}

fn print_statistics(stats: CoverageStatistics) {
    println!("Fetched {} .narinfos", stats.total);
    println!("{}/{} ({:.2}%) outputs are available",
             stats.found, stats.total,
             100. * stats.found as f32 / stats.total as f32);

    println!("{} of Nix archives (compressed)", format_bytes(stats.file_size));
    println!("{} of Nix archives (uncompressed)", format_bytes(stats.nar_size));

    let max_length = stats.missing.iter().map(String::len).max().unwrap_or(0);
    if !stats.missing.is_empty() {
        println!("The following derivations are missing and will have to be built locally:");
        for names in stats.missing.chunks(3) {
            for name in names { print!("{: <width$} ", name, width = max_length + 1); }
            println!();
        }
    }
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    // Start at INFO, allow to reduce to warn/error or increase to debug/trace
    let verbosity = cmp::max(0, 2 + opt.verbose - opt.quiet);
    stderrlog::new()
        .module(module_path!())
        .verbosity(verbosity as usize)
        .init().expect("Unable to init logging");

    // Resolve symlinks, useful for ./result outputs
    let input_paths = opt.input_derivations.into_iter()
        .map(|path| path.canonicalize().expect("Unable to canonicalize input path"));

    let inputs = input_paths.map(|path| (StoreHash::from_path(&path), Drv::read_from(path)));

    let outputs: Vec<_> = 
        inputs.flat_map(|(input_hash, input_drv)|
            input_drv.outputs.clone().into_iter()
                .map(move |out| (input_hash, input_drv.clone(), StoreHash::from_path(&out.path))))
        .collect();

    let mut store = StoreCache::default();
    for (input_hash, input_drv, _output_hash) in outputs.iter() {
        store.discover_build_time_closure(*input_hash, &input_drv);
    }

    info!("discovered {} store items...", store.entries().len());

    debug!("using cache_roots: {:?}", &opt.cache_roots);
    let fetched = store.fetch_narinfo(&opt.cache_roots, opt.narinfo_max_attempts, opt.narinfo_concurrency).await;

    info!("fetched {} narinfo...", fetched);

    info!("building runtime closure...");
    let mut runtime_closure = Closure::empty();
    for (_input_hash, _input_drv, output_hash) in &outputs {
        runtime_closure.add_runtime_closure_of(*output_hash, &store);
    }
    info!("runtime closure is at most {} paths large", runtime_closure.entries().len());

    let stats = runtime_closure.coverage_statistics(&store);

    if opt.json {
        serde_json::to_writer(&mut io::stdout().lock(), &stats)
            .expect("Failed to write statistics");
    } else {
        print_statistics(stats);
    }
}
