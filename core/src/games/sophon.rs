//! HoYoverse Sophon installer: makes a folder match a build, chunk by chunk.
//!
//! Handles a fresh install, an update and the rest of an interrupted one with
//! the same algorithm: only files that changed are touched, chunks already
//! correct on disk are kept, and the only files ever deleted are ones the
//! previous build listed and the new one dropped.

use super::hoyo::Plan;
use super::{RECORD_NAME, clear_record, write_record};
use crate::progress::{Phase, Progress};
use anyhow::{Context, Result, bail};
use md5::{Digest, Md5};
use nix::sys::statvfs::statvfs;
use prost::Message;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Parallel chunk downloads. Chunks are ~1 MB, so more workers hide latency.
const WORKERS: usize = 8;
const MB: u64 = 1024 * 1024;

/// Every file the manifests listed, with its md5 (`<md5>\t<name>` per line).
/// The next update uses it to skip unchanged files and to delete dropped ones.
const LIST_NAME: &str = ".installed_files";

// ---- Manifest format (protobuf). Field numbers come from HoYoverse's format. ----

#[derive(Clone, PartialEq, Message)]
struct Manifest {
    #[prost(message, repeated, tag = "1")]
    files: Vec<ManifestFile>,
}

#[derive(Clone, PartialEq, Message)]
struct ManifestFile {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    chunks: Vec<Chunk>,
    #[prost(bool, tag = "3")]
    is_folder: bool,
    #[prost(uint64, tag = "4")]
    size: u64,
    #[prost(string, tag = "5")]
    md5: String,
}

#[derive(Clone, PartialEq, Message)]
struct Chunk {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    uncompressed_md5: String,
    #[prost(uint64, tag = "3")]
    offset: u64,
    #[prost(uint64, tag = "4")]
    compressed_size: u64,
    #[prost(uint64, tag = "5")]
    uncompressed_size: u64,
}

/// One chunk to fetch and where it goes.
struct Task {
    url: String,
    file: String, // the game file's name from the manifest, for error messages
    path: PathBuf,
    offset: u64,
    size: u64,   // uncompressed
    md5: String, // of the uncompressed data
    /// The file already existed, so this chunk may already be right on disk.
    verify: bool,
}

/// file name -> lowercase md5 (empty if the manifest gave none).
type FileList = HashMap<String, String>;

/// Make `dest` match `plan`, then record `build_id`.
pub fn sync(plan: &Plan, build_id: &str, short: &str, dest: &Path, progress: &Progress) -> Result<()> {
    let had_record = super::is_complete(dest);
    let up_to_date = fs::read_to_string(dest.join(RECORD_NAME)).is_ok_and(|found| found.trim() == build_id);
    if up_to_date && dest.join(LIST_NAME).exists() {
        return Ok(());
    }

    let agent = crate::net::agent();

    // Step 1: read every manifest before touching a single file, so a network
    // failure here leaves the install exactly as it was.
    progress.set_phase(Phase::Preparing);
    let mut manifests = Vec::new();
    for item in &plan.items {
        progress.set_label(format!("Reading manifest: {}", item.name));
        manifests.push((item, fetch_manifest(&agent, &item.manifest_url)?));
        if progress.cancelled() {
            bail!("paused");
        }
    }

    // The manifest is remote data: never let it write outside `dest`.
    let mut new_list = FileList::new();
    for (_, manifest) in &manifests {
        for file in &manifest.files {
            check_name(&file.name)?;
            if !file.is_folder {
                new_list.insert(file.name.clone(), file.md5.to_lowercase());
            }
        }
    }

    // Installed by an older launcher that kept no file list: the game is
    // already complete, so just record the list now.
    if up_to_date {
        write_list(dest, &new_list)?;
        return Ok(());
    }

    let old = read_list(dest);

    // From here on files change. Remove the record so an interruption leaves
    // the game marked unfinished (and a half-updated game is never started).
    fs::create_dir_all(dest)?;
    clear_record(dest);

    // Step 2: create folders and size every file that needs work; list the
    // chunks that might need fetching.
    progress.set_label("Checking files");
    let mut tasks: Vec<Task> = Vec::new();
    let mut needed = 0u64; // extra bytes of disk this will need
    for (item, manifest) in &manifests {
        for file in &manifest.files {
            let path = dest.join(&file.name);
            if file.is_folder {
                fs::create_dir_all(&path)?;
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let md5 = file.md5.to_lowercase();
            let meta = fs::metadata(&path).ok();
            let existing_len = meta.as_ref().map(|m| m.len());
            // Allocated blocks, so holes in a half-finished file don't count as used.
            let allocated = meta.as_ref().map_or(0, |m| m.blocks() * 512);

            // Same file as the last finished sync: leave it alone, don't even read it.
            let unchanged = existing_len == Some(file.size)
                && !md5.is_empty()
                && old.get(&file.name).is_some_and(|previous| *previous == md5);
            if unchanged {
                continue;
            }

            // Never truncate an existing file: it may hold chunks we can keep.
            let f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .with_context(|| format!("opening {}", path.display()))?;
            f.set_len(file.size)?;
            needed += file.size.saturating_sub(allocated);

            for chunk in &file.chunks {
                tasks.push(Task {
                    url: format!("{}/{}", item.chunk_url_prefix, chunk.id),
                    file: file.name.clone(),
                    path: path.clone(),
                    offset: chunk.offset,
                    size: chunk.uncompressed_size,
                    md5: chunk.uncompressed_md5.clone(),
                    verify: existing_len.is_some(),
                });
            }
        }
    }

    // Step 3: make sure what's still missing fits, before downloading anything.
    let gb = |bytes: u64| bytes as f64 / 1e9;
    let free = free_space(dest)?;
    if free < needed {
        bail!(
            "not enough disk space in {}: need {:.1} GB more, only {:.1} GB free",
            dest.display(),
            gb(needed),
            gb(free)
        );
    }

    // Step 4: download on a pool of threads. A chunk that's already on disk
    // and correct is skipped; it still counts as done so the bar is accurate.
    if had_record || !old.is_empty() {
        progress.set_label(format!("Updating {short} to {}", plan.tag));
    } else {
        progress.set_label(format!("Installing {short} {}", plan.tag));
    }
    progress.set_phase(Phase::Downloading);
    progress.set_total(tasks.iter().map(|t| t.size).sum());
    let next = AtomicUsize::new(0);
    let failed: Mutex<Option<anyhow::Error>> = Mutex::new(None);

    thread::scope(|s| {
        let work = || loop {
            if failed.lock().unwrap().is_some() || progress.cancelled() {
                break;
            }
            let Some(task) = tasks.get(next.fetch_add(1, Ordering::Relaxed)) else {
                break; // no tasks left
            };
            if task.verify && chunk_present(task) {
                progress.add(task.size);
                continue;
            }
            // A few quick retries for a flaky connection; a chunk that still
            // fails stops the job, and running it again resumes from here.
            let mut last = None;
            for attempt in 0..3 {
                if progress.cancelled() {
                    break;
                }
                match fetch_chunk(&agent, task, progress) {
                    Ok(()) => {
                        last = None;
                        break;
                    }
                    Err(e) => {
                        last = Some(e);
                        thread::sleep(std::time::Duration::from_millis(500 * (attempt + 1)));
                    }
                }
            }
            match last {
                None if !progress.cancelled() => progress.add(task.size),
                None => break,
                Some(_) if progress.cancelled() => break,
                Some(e) => {
                    failed
                        .lock()
                        .unwrap()
                        .get_or_insert(e.context(format!("chunk failed in {} ({})", task.file, task.url)));
                    break;
                }
            }
        };
        for _ in 0..WORKERS {
            s.spawn(work);
        }
    });

    if let Some(e) = failed.into_inner().unwrap() {
        return Err(e.context("stopped; play or install again to continue where it left off"));
    }
    if progress.cancelled() {
        bail!("paused");
    }

    // Step 5: delete files the old build had and the new one dropped. Only
    // names from our own list are ever touched: never saves, logs or anything
    // else the game wrote itself.
    progress.set_phase(Phase::Installing);
    progress.set_label("Cleaning up");
    let mut removed = 0;
    for name in old.keys() {
        if new_list.contains_key(name) || check_name(name).is_err() {
            continue;
        }
        let path = dest.join(name);
        if fs::remove_file(&path).is_ok() {
            removed += 1;
            remove_empty_parents(&path, dest);
        }
    }
    if removed > 0 {
        progress.note(format!("Removed {removed} files the new version no longer uses."));
    }

    // Last, on purpose: only a finished sync gets a record.
    write_list(dest, &new_list)?;
    write_record(dest, build_id)?;
    Ok(())
}

/// Reject a manifest path that could escape the game folder.
fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('/') || name.split(['/', '\\']).any(|part| part == "..") {
        bail!("unsafe path in manifest: {name}");
    }
    Ok(())
}

/// Read the list the last finished sync wrote (empty if there is none).
fn read_list(dest: &Path) -> FileList {
    let Ok(text) = fs::read_to_string(dest.join(LIST_NAME)) else {
        return FileList::new();
    };
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(md5, name)| (name.to_string(), md5.to_string()))
        .collect()
}

/// Write the list through a temp file so it is never half-written.
fn write_list(dest: &Path, list: &FileList) -> Result<()> {
    let mut entries: Vec<_> = list.iter().collect();
    entries.sort();
    let text: String = entries.iter().map(|(name, md5)| format!("{md5}\t{name}\n")).collect();
    crate::paths::write_atomic(&dest.join(LIST_NAME), text.as_bytes())?;
    Ok(())
}

/// After deleting a file, remove the folders it leaves empty (never `stop` itself).
fn remove_empty_parents(file: &Path, stop: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == stop || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Free bytes on the disk that holds `path`, as a normal user sees it.
fn free_space(path: &Path) -> Result<u64> {
    let s = statvfs(path).context("could not read the free disk space")?;
    Ok(s.blocks_available() as u64 * s.fragment_size() as u64)
}

fn md5_hex(data: &[u8]) -> String {
    format!("{:x}", Md5::digest(data))
}

/// True if the file already holds exactly this chunk (checks the real bytes).
fn chunk_present(task: &Task) -> bool {
    let mut buf = vec![0u8; task.size as usize];
    File::open(&task.path).and_then(|f| f.read_exact_at(&mut buf, task.offset)).is_ok()
        && md5_hex(&buf) == task.md5.to_lowercase()
}

/// Download and decode one manifest (zstd-compressed protobuf).
fn fetch_manifest(agent: &ureq::Agent, url: &str) -> Result<Manifest> {
    let compressed = agent.get(url).call()?.body_mut().with_config().limit(256 * MB).read_to_vec()?;
    let raw = zstd::stream::decode_all(&compressed[..])?;
    Ok(Manifest::decode(raw.as_slice())?)
}

/// Fetch, decompress, verify and write one chunk.
fn fetch_chunk(agent: &ureq::Agent, task: &Task, progress: &Progress) -> Result<()> {
    let compressed = agent.get(task.url.as_str()).call()?.body_mut().with_config().limit(64 * MB).read_to_vec()?;
    progress.add_net(compressed.len() as u64);

    // The exact decompressed size is known, so it doubles as a sanity limit.
    let data = zstd::bulk::decompress(&compressed, task.size as usize)?;
    if data.len() as u64 != task.size {
        bail!("wrong size: got {}, expected {}", data.len(), task.size);
    }
    let md5 = md5_hex(&data);
    if md5 != task.md5.to_lowercase() {
        bail!("checksum mismatch: got {md5}, expected {}", task.md5);
    }

    // Files are pre-sized and chunks touch different byte ranges, so threads don't clash.
    OpenOptions::new()
        .write(true)
        .open(&task.path)
        .with_context(|| format!("opening {}", task.path.display()))?
        .write_all_at(&data, task.offset)?;
    progress.add_disk(data.len() as u64);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_escape_are_rejected() {
        assert!(check_name("StarRail_Data/x.bin").is_ok());
        assert!(check_name("../x").is_err());
        assert!(check_name("/etc/passwd").is_err());
        assert!(check_name("a\\..\\b").is_err());
        assert!(check_name("").is_err());
    }

    #[test]
    fn list_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut list = FileList::new();
        list.insert("a/b.bin".into(), "abc".into());
        list.insert("c.exe".into(), "def".into());
        write_list(tmp.path(), &list).unwrap();
        assert_eq!(read_list(tmp.path()), list);
    }
}
