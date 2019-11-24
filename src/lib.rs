pub mod derivation;
pub mod narinfo;

use std::{
    str,
    convert::TryInto,
    path::Path,
    collections::{
        hash_map::Entry::*,
        HashMap, HashSet
    },
    time::Duration
};

use futures::prelude::*;
use tokio::timer::delay_for;
use reqwest::{ Client, StatusCode };

use url::Url;
use serde_derive::Serialize;
use log::{ error, warn, debug, trace };

use crate::{ derivation::*, narinfo::* };

const NIX_HASH_LENGTH: usize = 32;

// Nix store hashes are the first 160 bits of a sha256 hash, base32 encoded.
// That base32 representation could be decoded into a [u32; 5], but then
// we'd depend on Nix's exact character set and encoding/decoding rules.
//
// sizeof [u8; 32] = 256 bit > 160 bit = sizeof [u32; 5]
//
// This means our representation is quite a bit less compact than it could be,
// but with a typical system closure of 10k paths, that's still less than a megabyte,
// the derivations themselves are much more expensive.
#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq)]
pub struct StoreHash([u8; NIX_HASH_LENGTH]);
impl StoreHash {
    pub fn split(name: &str) -> (Self, &str) {
        let (hash, rest) = name.split_at(NIX_HASH_LENGTH);
        (StoreHash::from_name(hash), &rest[1..])
    }

    pub fn split_path<P: AsRef<Path>>(path: P) -> (Self, String) {
        let name = path.as_ref()
            .file_name().expect("Not a file")
            .to_str().expect("Invalid filename");
        let (hash, name) = StoreHash::split(name);
        (hash, name.to_string())
    }

    /// e.g. rgmc4d3spji36n2l1sicm80yq79dpcc2-hello-2.10
    pub fn from_name(name: &str) -> Self {
        let hash = name.as_bytes()[..NIX_HASH_LENGTH]
                       .try_into().expect("Wrong slice length");
        StoreHash(hash)
    }

    /// e.g. /nix/store/rgmc4d3spji36n2l1sicm80yq79dpcc2-hello-2.10
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        let name = path.as_ref()
            .file_name().expect("Not a file")
            .to_str().expect("Invalid filename");
        StoreHash::from_name(name)
    }

    pub fn to_str(&self) -> &str { str::from_utf8(&self.0).expect("Invalid UTF8") }
}

#[derive(Debug, Clone)]
pub enum StoreItem {
    Drv(Drv),
    NarInfo(Box<NarInfo>),
    Source(String),
    Output(String, StoreHash)
}

impl StoreItem {
    pub fn as_drv(self) -> Option<Drv> {
        if let StoreItem::Drv(drv) = self {
            Some(drv)
        } else { None }
    }

    pub fn as_narinfo(self) -> Option<NarInfo> {
        if let StoreItem::NarInfo(narinfo) = self {
            Some(*narinfo)
        } else { None }
    }
}

#[derive(Default)]
pub struct StoreCache(HashMap<StoreHash, StoreItem>);
impl StoreCache {
    pub fn entries(&self) -> &HashMap<StoreHash, StoreItem> { &self.0 }
    pub fn get(&self, hash: &StoreHash) -> Option<&StoreItem> { self.0.get(hash) }

    // Condition: discover_build_time_closure is only called with matching hash and drv
    // Invariant: forall d in self: forall d' in build-closure(d): d' in self
    pub fn discover_build_time_closure(&mut self, hash: StoreHash, drv: &Drv) {
        if self.0.contains_key(&hash) { return }
        trace!("registering derivation {}", drv.find_name());
        self.0.insert(hash, StoreItem::Drv(drv.clone()));

        for path in &drv.input_srcs {
            let (input_src_hash, input_src_name) = StoreHash::split_path(&path);

            trace!("registering source {}", path);
            self.0.insert(input_src_hash, StoreItem::Source(input_src_name));
        }

        for DrvOutput { key, path, .. } in &drv.outputs {
            let (output_hash, output_name) = StoreHash::split_path(&path);

            trace!("registering output {} of {} to {}", key, output_name, output_hash.to_str());
            self.0.insert(output_hash, StoreItem::Output(output_name, hash));
        }

        for InputDrv { path, .. } in &drv.input_drvs {
            let input_drv_hash = StoreHash::from_path(path);

            // check cache to avoid unnecessary IO/parsing
            let input_drv = self.0.get(&input_drv_hash)
                                  .and_then(|item| item.clone().as_drv())
                                  .unwrap_or_else(|| Drv::read_from(&path));

            self.discover_build_time_closure(input_drv_hash, &input_drv);
        }
    }

    pub async fn fetch_narinfo(&mut self, cache_roots: &[Url], retries: u32, concurrency: u32) -> u64 {
        let output_hashes: Vec<StoreHash> = self.0.iter()
            .filter_map(|(k, v)| {
                if let StoreItem::Output(_, _) = v { Some(*k) }
                else { None }
            })
            .collect();

        debug!("checking {} outputs", output_hashes.len());

        async fn fetch_narinfo(c: &Client, url: Url) -> Result<Option<NarInfo>, reqwest::Error> {
            let response = c.get(url).send().await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None)
            }

            let bytes = response.bytes().await?;
            Ok(NarInfo::from(&bytes[..]))
        }

        async fn fetch_first_narinfo(c: &Client, cache_roots: &[Url], max_attempts: u32, hash: StoreHash)
                -> Result<(StoreHash, Option<NarInfo>), reqwest::Error> {
            'next_cache: for cache_root in cache_roots {
                let url = cache_root.join(&format!("{}.narinfo", hash.to_str()))
                                    .expect("Invalid URL join");
                trace!("fetching {}", url);
                let mut delay = 64;

                for _ in 0..max_attempts {
                    if let Ok(response) = fetch_narinfo(c, url.clone()).await {
                        match response {
                            Some(narinfo) => return Ok((hash, Some(narinfo))),
                            None => continue 'next_cache
                        }
                    }

                    delay_for(Duration::from_millis(delay)).await;
                    delay *= 2;
                }
            }

            return Ok((hash, None))
        }

        let client = Client::new();

        let mut narinfos = stream::iter(output_hashes)
            .map(|hash| fetch_first_narinfo(&client, cache_roots, retries, hash))
            .buffer_unordered(concurrency as usize)
            .filter_map(|res| match res {
                Ok((_, None))    => future::ready(None),
                Ok((h, Some(n))) => future::ready(Some((h, n))),
                Err(e) => { error!("{}", e); future::ready(None) }
            });

        let mut fetched = 0;
        // merge into self without overwriting
        while let Some((hash, narinfo)) = narinfos.next().await {
            fetched += 1;
            match self.0.entry(hash) {
                Vacant(e) =>       { e.insert(StoreItem::NarInfo(Box::new(narinfo))); }
                Occupied(mut e) => match e.get() {
                    // upgrade output to narinfo
                    StoreItem::Output(_, _) => { e.insert(StoreItem::NarInfo(Box::new(narinfo))); }
                    duplicate => warn!("got duplicate at {:?}", duplicate)
                }
            }
        }

        fetched
    }
}

#[derive(Default, Debug, Serialize)]
pub struct CoverageStatistics {
    pub total: u64,
    pub found: u64,
    pub file_size: u64,
    pub nar_size: u64,
    pub missing: Vec<String>
}

pub struct Closure(HashSet<StoreHash>);
impl Closure {
    pub fn empty() -> Self { Closure(HashSet::default()) }
    
    pub fn add_runtime_closure_of(&mut self, hash: StoreHash, store: &StoreCache) {
        if self.0.contains(&hash) { return }
        self.0.insert(hash);

        match store.get(&hash) {
            Some(StoreItem::NarInfo(narinfo)) =>
                narinfo.references.iter()
                    .map(|name| StoreHash::from_name(name))
                    .for_each(|hash| self.add_runtime_closure_of(hash, store)),

            Some(StoreItem::Output(_, deriver_hash)) =>
                self.add_runtime_closure_of(*deriver_hash, store),

            Some(StoreItem::Drv(drv)) =>
                drv.input_drvs.iter()
                    .flat_map(|input| input.resolve(store))
                    .map(StoreHash::from_path)
                    .for_each(|hash| self.add_runtime_closure_of(hash, store)),

            _ => ()
        }
    }

    pub fn coverage_statistics(&self, store: &StoreCache) -> CoverageStatistics {
        let mut stats = CoverageStatistics::default();

        fn process(stats: &mut CoverageStatistics, store: &StoreCache, hash: StoreHash) {
            match store.get(&hash) {
                Some(StoreItem::NarInfo(narinfo)) => {
                    stats.found += 1;
                    stats.file_size += narinfo.file_size;
                    stats.nar_size += narinfo.nar_size;
                }
                Some(StoreItem::Drv(drv)) => {
                    stats.missing.push(drv.find_name());
                }
                // Sources don't have to be built
                Some(StoreItem::Source(_name)) => {}
                Some(StoreItem::Output(_name, deriver_hash)) => {
                    assert!(&hash != deriver_hash, "output can't derive itself: {}", hash.to_str());
                    process(stats, store, *deriver_hash)
                },
                None => {
                    stats.missing.push(hash.to_str().to_owned());
                }
            }
        }

        stats.total = self.0.len() as u64;
        for hash in &self.0 { process(&mut stats, store, *hash) }

        stats.missing.sort();
        stats.missing.dedup();

        stats
    }

    pub fn entries(&self) -> &HashSet<StoreHash> { &self.0 }
}
