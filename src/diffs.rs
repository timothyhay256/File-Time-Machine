use std::error::Error;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::fs::File;
use bsdiff::diff;
use walkdir::WalkDir;
use std::fs::metadata;
use std::{io, time::UNIX_EPOCH};
use std::io::ErrorKind;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle, ProgressState};
use std::thread;
use chrono::DateTime;
use std::io::Write;
use std::io::Read;
use std::process;
use log::debug;
use xxhash_rust::xxh3::xxh3_64;

use crate::compression;
use crate::restore;
use crate::DiffEntry;
use crate::MetaFile;
use crate::ModifiedList;

pub fn create_diff( // Never call this on a directory. Do checks outside of the function
    mut old_file: String,
    new_file: String,
    target_path: String,
    time_dir: String,
    ref_patch: String,
    old_raw: Vec<u8>,
    compression_level: u32,
    patch_store: &Arc<Mutex<Vec<DiffEntry>>>,
    create_reverse: bool,
) -> Result<String, Box<dyn Error>> {
    /* This handles everything related to creating a diff, including storing its metadata/location.
    If old_raw is set, then we will use it as the target file. Will create a forward diff and backward diff.
    Backward diff will be {diff_id}-reverse. Every diff is compressed with brotli before being written.
    */
    // println!("create_diff called");
    // println!("New: {new_file}");
    // println!("Old: {old_file}");
    let mut sha256 = Sha256::new();
    let old: Vec<u8>;
    let current_time: String = chrono::offset::Local::now().to_string();

    if !Path::new(&old_file).exists() || !Path::new(&old_file).is_file() {
        // In this case, we assume there is a new file, so old_file is directed to an empty file
        old_file = time_dir.clone() + "/tmp_empty";
    }

    if !old_raw.is_empty() {
        // Handle case where old is stored in memory
        debug!("create_diff: Old stored in memory!");
        old = old_raw;
    } else {
        old = std::fs::read(old_file.clone()).unwrap_or_else(|_| panic!("Could not open {old_file}!"));
    }
    // println!("Old file is {}", old_file);
    let new = std::fs::read(new_file.clone()).unwrap_or_else(|_| panic!("Could not open {new_file}!"));

    sha256.update(current_time.clone() + &target_path); // Generate an ID to identify the patch. This can be derived from the data stored in DiffEntry, which can then be used to identify where the patch file is.
    let patch_id: String = format!("{:X}", sha256.finalize());

    let mut patch_target = File::create(Path::new(&(time_dir.clone() + "/" + &patch_id))).unwrap_or_else(|_| panic!("Could not create patch_target at {}",
        time_dir.clone() + "/" + &patch_id));
    if create_reverse {
        debug!("Creating reverse!");
        let mut patch_target_reverse =
            File::create(Path::new(&(time_dir.clone() + "/" + &patch_id + "-reverse"))).unwrap_or_else(|_| panic!("Could not create patch_target at {}",
                time_dir.clone() + "/" + &patch_id));

        let mut patch_reverse = Vec::new();
        // println!("{:?}", new);
        // println!("{:?}", old);
        diff(&new, &old, &mut patch_reverse)?;
        // println!("Compressing reverse...");
        
        let temp_compressed = compression::compress_data(patch_reverse, compression_level)?;
        // let elapsed = now.elapsed();
        // println!("Compressing reverse: {:.2?}", elapsed);

        patch_target_reverse
            .write_all(&temp_compressed)
            .expect("Unable to write to patch file!");
    } else {
        debug!("Creating false reverse!");
        let mut patch_target_reverse =
            File::create(Path::new(&(time_dir.clone() + "/" + &patch_id + "-reverse"))).unwrap_or_else(|_| panic!("Could not create patch_target at {}",
                time_dir.clone() + "/" + &patch_id));
        write!(patch_target_reverse, ":3").unwrap_or_else(|_| panic!("There was an issue writing to {}!", time_dir.clone() + "/" + &patch_id + "-reverse"));
    }

    let mut patch = Vec::new();

    

    // let now = Instant::now();
    diff(&old, &new, &mut patch)?;
    // let elapsed = now.elapsed();
    // println!("Diff calc: {:.2?}", elapsed);

    // let now = Instant::now();
    // println!("Compressing patch...");
    let temp_compressed = compression::compress_data(patch, compression_level)?;
    // let elapsed = now.elapsed();
    // println!("Compressing orig: {:.2?}", elapsed);

    patch_target
        .write_all(&temp_compressed)
        .expect("Unable to write to patch file!");

    // let now = Instant::now();

    // let mut writer = brotli::Compressor::new(&mut io::stdout(), 4096, 4, 20);
    let patch_store_file = time_dir.clone() + "/patches.json";

    let patch_entry = DiffEntry {
        date_created: current_time,
        target_path,
        ref_patch,
    };

    {
        let mut patch_store = patch_store.lock().unwrap();
        patch_store.push(patch_entry);

        let json =
            serde_json::to_string_pretty(&*patch_store).expect("Unable to serialize metadata!");
        let mut file = File::create(Path::new(&patch_store_file)).unwrap_or_else(|_| panic!("Unable to create metadata file at {patch_store_file}"));
        file.write_all(json.as_bytes()).unwrap_or_else(|_| panic!("Unable to write to metadata file at {patch_store_file}"));
    }
    Ok(patch_id)
}

pub fn get_diffs(
    check_hash: bool,
    metadata_holder: &HashSet<MetaFile>,
    folder_path: &str,
) -> Result<HashSet<ModifiedList>, Box<dyn Error>> {
    
    let mut different_files: HashSet<ModifiedList> = HashSet::new();
    let mut temp_hold: HashSet<ModifiedList> = HashSet::new();
    let mut current_files: HashSet<ModifiedList> = HashSet::new();
    debug!("folder_path is {folder_path}");
    for entry in WalkDir::new(folder_path) {
        let entry = entry?;
        let path = entry.path();
        // debug!("{:?}", path);
        if let Some(path_str) = path.to_str() {
            if !path_str.contains(".time") && !path_str.contains(".git") && path_str != folder_path {
                current_files.insert(ModifiedList {
                    path: path_str.to_string(),
                    exists: true,
                    modified: false, // We don't know yet, but we will change this if needed. false will be the default.
                });
            }
        } else {
            // Handle the case where the path is not valid UTF-8
            eprintln!("Error: Path is not valid UTF-8: {}", path.display());
        }
    }
    for path in metadata_holder.iter() {
        temp_hold.insert(ModifiedList {
            path: path.path.to_string(),
            exists: true,
            modified: false,
        });
    }

    for path in current_files.iter() {
        if !temp_hold.contains(&ModifiedList {
            path: path.path.clone(),
            exists: true,
            modified: false,
        }) {
            debug!("Found new file:{}", path.path.clone()); 
            different_files.insert(ModifiedList {
                path: path.path.clone(),
                exists: true,
                modified: true,
            });
        }
    }
    for meta in metadata_holder.iter() {
        // println!("Got: {}", meta.path);
        match metadata(&meta.path) {
            Ok(metadata) => {
                // File exists, continue
                // let metadata = metadata(&meta.path)?;
                // Get the modification time from the metadata
                let modified_time = metadata.modified()?; // Replace ? with proper error handling if we want to do it here. Otherwise, we handle it outside the function.

                // Convert SystemTime to UNIX epoch
                let duration_since_epoch = modified_time.duration_since(UNIX_EPOCH)?;
                let epoch_seconds = duration_since_epoch.as_secs();
                // Checking date modified and size is prioritized over hash since it is much faster.
                // if Path::new(&meta.path.clone()).is_file() {
                    // Ensure the parent directory is not counted as updated file
                    if epoch_seconds != meta.date_modified {
                        // Check if file is modified using date modified
                        debug!(
                            "File is different: {} (discovered using modify date)",
                            meta.path
                        );
                        different_files.insert(ModifiedList {
                            path: meta.path.clone(),
                            exists: true,
                            modified: true,
                        });
                    } else if metadata.len() != meta.size {
                        // If date modified is the same, check if file size has changed
                        debug!("File is different: {} (discovered using size)", meta.path);
                        different_files.insert(ModifiedList {
                            path: meta.path.clone(),
                            exists: true,
                            modified: true,
                        });
                    } else if check_hash {
                        // check_hash enabled, check hash as last resort
                        if hash(&meta.path)? != meta.hash {
                            debug!("File is different: {} (discovered using hash)", meta.path);
                            different_files.insert(ModifiedList {
                                path: meta.path.clone(),
                                exists: true,
                                modified: true
                            });
                        } else {
                            // println!("Confirmed file is not modified. (Used hash)");
                            different_files.insert(ModifiedList {
                                path: meta.path.clone(),
                                exists: true,
                                modified: false,
                            });
                        }
                    } else {
                        // println!("Confirmed file is not modified. (Used modify date and size)");
                        different_files.insert(ModifiedList {
                            path: meta.path.clone(),
                            exists: true,
                            modified: false,
                        });
                    }
                // } else if meta.path != folder_path {
                //     // println!("insert {}", meta.path);
                //     different_files.insert(ModifiedList {
                //         path: meta.path.clone(),
                //         exists: true,
                //         modified: true,
                //     });
                // }
            }
            Err(error) => match error.kind() {
                ErrorKind::NotFound => {
                    debug!("File no longer exists: {}", meta.path);
                    different_files.insert(ModifiedList {
                        path: meta.path.clone(),
                        exists: false,
                        modified: true,
                    });
                }
                other_error => {
                    panic!(
                        "Problem reading file: {} with error: {}",
                        meta.path, other_error
                    );
                }
            },
        }
    }
    // println!("{:?}", different_files);
    Ok(different_files)
}

pub fn update_metadata(
    metadata_holder: &mut HashSet<MetaFile>,
    modified_list: &HashSet<ModifiedList>,
    hash_enabled: bool,
) -> Result<(), Box<dyn Error>> {
    // Update metadata with modified_list to update data.
    let mut paths_to_update = Vec::new(); // Paths that need updating
    let mut temp_hold: HashSet<ModifiedList> = HashSet::new();
    let mut updated_files = HashSet::new(); // Temp set to hold elements that we will add at the end

    // for meta in metadata_holder.iter() {
    //     let item_to_check = ModifiedList { path: meta.path.clone(), exists: true };

    //     if modified_list.contains(&item_to_check) {
    //         paths_to_update.push(meta.path.clone()); // Collect paths that need updates
    //     }
    // }
    for path in metadata_holder.iter() {
        temp_hold.insert(ModifiedList {
            path: path.path.to_string(),
            exists: true,
            modified: false,
        });
    }

    for path in modified_list.iter() {
        if temp_hold.contains(&ModifiedList {
            path: path.path.clone(),
            exists: true,
            modified: false,
        }) {
            if path.exists {
                paths_to_update.push(path.path.clone());
            }
        } else if !temp_hold.contains(&ModifiedList {
            path: path.path.clone(),
            exists: false,
            modified: false,
        }) {
            paths_to_update.push(path.path.clone());
        }
    }

    // for path in modified_list.iter() {
    //     paths_to_update.push(path.path.clone());
    // }

    println!("Finished generating list. Recalculating metadata...");
    // debug!("{:?}", modified_list);
    // println!("{:?}", modified_list);
    {
        let mut modified_files = false;
        for modified in modified_list {
            if modified.modified {
                modified_files = true;
                break
            }
        }
        if !modified_files {
            println!("No files changed, nothing to do!");
            process::exit(1);
        }
    }

    for path in paths_to_update {
        let _hash_str: String = Default::default();
        if hash_enabled {
            let _hash_str: String = hash(&path).unwrap_or_else(|_| panic!("There was a unhandled issue getting the hash of {path}"));
        } else {
            let _hash_str: String = "".to_string();
        }
        let file_metadata = metadata(&path)?;
        let size = file_metadata.len(); // Get file size

        // Get the modification time from the metadata
        let modified_time = file_metadata.modified()?;

        // Convert SystemTime to UNIX epoch
        let duration_since_epoch = modified_time.duration_since(UNIX_EPOCH)?;
        let epoch_seconds = duration_since_epoch.as_secs();

        let updated_meta_file = MetaFile {
            date_modified: epoch_seconds,
            hash: _hash_str,
            size,
            path: path.clone(),
        };

        // Remove the old element
        metadata_holder.retain(|meta| meta.path != path);

        // Insert the updated element
        updated_files.insert(updated_meta_file); // updated_files gets extended at the end
    }

    metadata_holder.extend(updated_files);

    let paths_to_remove: HashSet<_> = metadata_holder
        .iter()
        .filter_map(|meta| {
            let item_to_check = ModifiedList {
                path: meta.path.clone(),
                exists: false,
                modified: true,
            };
            if modified_list.contains(&item_to_check) {
                Some(meta.path.clone())
            } else {
                None
            }
        })
        .collect();

    metadata_holder.retain(|meta| !paths_to_remove.contains(&meta.path));

    Ok(())
}

pub fn get_properties(
    folder_path: &str,
    mut metadata_holder: HashSet<MetaFile>,
    hash_enabled: bool,
) -> Result<HashSet<MetaFile>, Box<dyn std::error::Error>> {
    let mut file_count = 0;
    let mut file_index = 0;

    for _entry in WalkDir::new(folder_path) {
        file_count += 1;
    }

    let pb = ProgressBar::new(file_count);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>3}/{len:3} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

    for entry in WalkDir::new(folder_path) {
        file_index += 1;
        pb.set_position(file_index); // Update progress bar.

        let entry = entry?;
        let path = entry.path();

        // Convert Path to &str
        if let Some(path_str) = path.to_str() {
            if !path_str.contains(".time") && !path_str.contains(".git") && path_str != folder_path {
                // Use the path as a &str
                let _hash_str: String = Default::default();
                if hash_enabled {
                    let _hash_str: String = hash(path_str).unwrap_or_else(|_| panic!("There was a unhandled issue getting the hash of {path_str}"));
                } else {
                    let _hash_str: String = "".to_string();
                }
                let metadata = metadata(path)?;
                let size = metadata.len(); // Get file size

                // Get the modification time from the metadata
                let modified_time = metadata.modified()?;

                // Convert SystemTime to UNIX epoch
                let duration_since_epoch = modified_time.duration_since(UNIX_EPOCH)?;
                let epoch_seconds = duration_since_epoch.as_secs();
                // println!("{}", size);
                // println!("{}", epoch_seconds);
                // println!("{}", path_str);

                let meta_file = MetaFile {
                    date_modified: epoch_seconds,
                    hash: _hash_str,
                    size,
                    path: path_str.to_string(),
                };
                metadata_holder.insert(meta_file);
            }
            // metadata_holder.push(MetaFile {hash: hash});
        } else {
            // Handle the case where the path is not valid UTF-8
            eprintln!("Error: Path is not valid UTF-8: {}", path.display());
        }
    }
    pb.finish();
    Ok(metadata_holder)
}

pub fn hash(path: &str) -> Result<String, Box<dyn Error>> {
    // println!("hash called");
    let mut file = match File::open(Path::new(path)) {
        Ok(file) => file,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                eprintln!("Error: The file '{}' was not found.", path);
                panic!("quit");
            } else {
                // Handle other kinds of I/O errors
                eprintln!("Error: Unable to open file '{}': {}", path, e);
            }
            return Err(Box::new(e));
        }
    };

    let mut hasher = Sha256::new();

    let mut buffer = [0u8; 1024];
    while let Ok(bytes_read) = file.read(&mut buffer) {
        // Run the loop as long as file.read returns Ok(bytes_read)
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]); // Slice of buffer that starts at 0 and ends at bytes_read
    }

    let result = hasher.finalize();
    let hash_string = hex::encode(result);

    // println!("Hash is {:x}", result);

    Ok(hash_string)
}

pub fn create_diffs_multithread(
    patch_ids: &Arc<Mutex<Vec<String>>>,
    ref_patch_ids: &Arc<Mutex<Vec<String>>>,
    target_paths: &Arc<Mutex<Vec<String>>>,
    modified: &Arc<Mutex<Vec<bool>>>,
    folder_path: &String,
    changed_files_vec: Vec<ModifiedList>, // We need it to be a vec since hashset doesn't support slices
    changed_count: u32,
    thread_count: u32,
    compression_level: u32,
    patch_store: &Arc<Mutex<Vec<DiffEntry>>>, // This will be populated if first run, otherwise it must be pre populated
    mut create_reverse: bool,
    inital_run: bool,
    snapshot_mode: &String,
) {
    /*
    Get the amount that we should give to each thread via split_into. Then calculate slice begin and end
    and pass a cloned slice, the thread can own this. The thread will need to lock and unlock patch_ids and target_paths
    however.
        */
    debug!("create_diffs_multithread called");
    let mut children = Vec::new();
    let split_into = changed_count / thread_count;
    let split_into_rem = changed_count % thread_count;

    let mut path_temp_hold_ref = HashSet::new();
    {
        let patch_store = patch_store.lock().unwrap();
        for path in patch_store.iter() {
            path_temp_hold_ref.insert(ModifiedList {
                path: path.target_path.clone().to_string(),
                exists: true,
                modified: true, // Not needed. This is not really proper usage of ModifiedList.
            });
        }
    }

    let m = MultiProgress::new();

    for i in 0..thread_count {
        // Spawn our childrenfolder_path
        let folder_path_new = folder_path.clone(); // To prevent moving ownership, we need to clone this value.
        let slice_begin: usize = (i * split_into).try_into().unwrap();
        let mut slice_end: usize = ((i * split_into) + split_into).try_into().unwrap();
        // println!("slice_begin: {}", slice_begin);
        // println!("slice_end: {}", slice_end);
        if i == thread_count-1 {
            slice_end += split_into_rem as usize;
        }
        let patch_ids = Arc::clone(patch_ids);
        let target_paths = Arc::clone(target_paths);
        let ref_patch_ids = Arc::clone(ref_patch_ids);
        let patch_store = Arc::clone(patch_store);
        let modified = Arc::clone(modified);



        let slice = changed_files_vec[slice_begin..slice_end].to_vec(); // Create new vector since our reference will die
        // println!("{:?}", slice);
        if inital_run {
            children.push(thread::spawn(move || {
                for path in slice.iter() {
                    if path.modified {
                        if Path::new(&path.path.clone()).is_file() {
                            let patch_id = create_diff(
                                "".to_string(), // This will never exist, so we can always create a temp file instead.
                                path.path.clone(),
                                path.path.clone(),
                                folder_path_new.clone() + "/.time",
                                "First patch".to_string(),
                                Vec::new(),
                                compression_level,
                                &patch_store,
                                create_reverse,
                            )
                            .unwrap_or_else(|_| panic!("Was unable to create a diff between a new empty file and {}",
                                path.path));
                            {
                                let mut patch_ids = patch_ids.lock().unwrap();
                                let mut target_paths = target_paths.lock().unwrap();
                                let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                let mut modified = modified.lock().unwrap();

                                patch_ids.push(patch_id); // Deref is automatic when using a `.`
                                target_paths.push(path.path.clone());
                                ref_patch_ids.push("First patch".to_string());
                                modified.push(true); // We want to push true since technically going from no file to a file is "modified".
                            } // Go out of scope to release our lock
                        } else {
                            {
                                let mut patch_ids = patch_ids.lock().unwrap();
                                let mut target_paths = target_paths.lock().unwrap();
                                let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                let mut modified = modified.lock().unwrap();

                                patch_ids.push("DIR".to_string());
                                target_paths.push(path.path.clone());
                                ref_patch_ids.push("DIR".to_string());
                                modified.push(true); // We want to push true since technically going from no file to a file is "modified".
                            }
                        }
                    } else {
                        let mut patch_ids = patch_ids.lock().unwrap();
                        let mut target_paths = target_paths.lock().unwrap();
                        let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                        let mut modified = modified.lock().unwrap();
                        
                        if Path::new(&path.path).is_file() {
                            let file_contents = std::fs::read(&path.path).unwrap_or_else(|_| panic!("Could not open {} to check if it has been modified! Do I have read permission?",
                            path.path));
                            let hash = xxh3_64(&file_contents);
                            patch_ids.push(hash.to_string());
                        } else {
                            patch_ids.push("UNMODIFIED_DIRECTORY".to_string());
                        }
                        target_paths.push(path.path.clone());
                        ref_patch_ids.push("UNMODIFIED".to_string());
                        modified.push(false); 
                    }
                }
            }));
        } else {
            create_reverse = true;
            debug!("create_reverse is true");
            let path_temp_hold = path_temp_hold_ref.clone();
            let folder_path_clone = folder_path.clone();
            let m = m.clone();
            let snapshot_mode = snapshot_mode.clone(); // Is this creating correct snapshots?
            children.push(thread::spawn(move || {
                let total: u64 = slice.len() as u64;
                let pb = m.add(ProgressBar::new(total));
                pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>3}/{len:3} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));
                for path in slice.iter() {
                    if path.modified {
                        pb.inc(1);
                        // println!("{}", path.path.clone());

                        if path_temp_hold.contains(&ModifiedList {
                            path: path.path.clone().to_string(),
                            exists: path.exists,
                            modified: true,
                        }) {
                            debug!("Snapshot that can be used for reference exists!");
                            // Snapshot exists that we can restore for reference
                            let search_path = path.path.clone().to_string(); // File that we want to snapshot
                            // let mut matching_items: Vec<&DiffEntry>;
                            let patch_unguard;
                            let patch_store = Arc::clone(&patch_store);
                            {
                                let patch_store = Arc::clone(&patch_store);
                                patch_unguard = patch_store.lock().unwrap().clone();
                                
                            }
                            let matching_items: Vec<&DiffEntry> = patch_unguard
                                    .iter()
                                    .filter(|item| item.target_path == search_path)
                                    .collect(); // Collect all items inside patch_store that have target_path equal to search_path
                                // Print all matching items
                                if !matching_items.is_empty() {
                                    if matching_items.len() > 1 {
                                        // println!("Found matching items:");
                                        // println!("{:?}", matching_items);
                                        let mut date_check;
                                        let mut target_path: String;
                                        if let Some(first_item) = matching_items.first() {
                                            let first_date_string = first_item.date_created.clone();
                                            // println!("{first_date_string}");
                                            date_check = DateTime::parse_from_str(
                                                &first_date_string,
                                                "%Y-%m-%d %H:%M:%S%.9f %z",
                                            )
                                            .unwrap();
                                            target_path = first_item.target_path.clone();
                                        } else {
                                            panic!("There was an issue parsing the patch store! Is this a valid date: {:?}", matching_items);
                                        }
                                        // Find correct patch to restore
                                        debug!("{:?}", matching_items);
                                        for item in matching_items {
                                            // Files with snapshots
                                            let date_check_string = item.date_created.clone();
                                            let new_date_check = DateTime::parse_from_str(
                                                &date_check_string,
                                                "%Y-%m-%d %H:%M:%S%.9f %z",
                                            )
                                            .unwrap();
                                            // println!("{}", new_date_check);
                                            // println!("{}", date_check);
                                            if new_date_check > date_check {
                                                // println!("Setting!");
                                                date_check = new_date_check;
                                                target_path = item.target_path.clone();
                                            }
                                        }
                                        if Path::new(&target_path).is_file() {
                                            let patch_id = restore::restore_and_diff(
                                                &date_check.to_string(),
                                                &target_path,
                                                &folder_path_clone.clone(),
                                                compression_level,
                                                &patch_store,
                                                create_reverse,
                                                &snapshot_mode
                                            ).expect("There was an issue restoring a reference patch and creating a new patch, did the .time folder go corrupt?");
                                        
                                        {
                                            let mut patch_ids = patch_ids.lock().unwrap();
                                            let mut target_paths = target_paths.lock().unwrap();
                                            let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                            let mut modified = modified.lock().unwrap();

                                            patch_ids.push(patch_id);
                                            target_paths.push(target_path.clone());
                                            let mut sha256 = Sha256::new();
                                            sha256.update(date_check.to_string() + &target_path);
                                            ref_patch_ids.push(format!("{:X}", sha256.finalize()));
                                            modified.push(path.modified);
                                        }
                                    } else {
                                        {
                                            let mut patch_ids = patch_ids.lock().unwrap();
                                            let mut target_paths = target_paths.lock().unwrap();
                                            let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                            let mut modified = modified.lock().unwrap();

                                            patch_ids.push("DIR".to_string());
                                            target_paths.push(target_path.clone());
                                            ref_patch_ids.push("DIR".to_string());
                                            modified.push(path.modified);
                                        }
                                    }
                                    } else {
                                        // Restore only existing patch
                                        {
                                            // let mut patch_store = patch_store.lock().unwrap();
                                            if let Some(first_item) = matching_items.first() {
                                                if Path::new(&first_item.target_path).is_file() {
                                                let patch_id = restore::restore_and_diff(
                                            &first_item.date_created,
                                            &first_item.target_path,
                                            &folder_path_clone.clone(),
                                            compression_level,
                                            &patch_store,
                                            create_reverse,
                                            &snapshot_mode
                                            
                                        ).expect("There was an issue restoring a reference patch and creating a new patch, did the .time folder go corrupt?");
                                    
                                                {
                                                    let mut patch_ids = patch_ids.lock().unwrap();
                                                    let mut target_paths = target_paths.lock().unwrap();
                                                    let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                                    let mut modified = modified.lock().unwrap();

                                                    patch_ids.push(patch_id);
                                                    target_paths.push(first_item.target_path.clone());
                                                    let mut sha256 = Sha256::new();
                                                    sha256.update(first_item.date_created.clone() + &first_item.target_path);
                                                    ref_patch_ids.push(format!("{:X}", sha256.finalize()));
                                                    modified.push(path.modified);
                                                }
                                            } else {
                                                let mut patch_ids = patch_ids.lock().unwrap();
                                                let mut target_paths = target_paths.lock().unwrap();
                                                let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                                let mut modified = modified.lock().unwrap();

                                                patch_ids.push("DIR".to_string());
                                                target_paths.push(first_item.target_path.clone());
                                                ref_patch_ids.push("DIR".to_string());
                                                modified.push(path.modified);
                                            }
                                        }
                                            } 
                                    }
                                } else {
                                    panic!("Did not find a valid patch in the patch store, even though there should be one!");
                                }
                            
                        } else if path.exists {
                            debug!("No existing patch! I will create a compressed copy of the original file. ");
                            if Path::new(&path.path).is_file() {
                                let patch_id = create_diff(
                                    "".to_string(),
                                    path.path.clone(),
                                    path.path.clone(),
                                    folder_path_clone.clone() + "/.time",
                                    "First patch".to_string(),
                                    Vec::new(),
                                    compression_level,
                                    &patch_store,
                                    create_reverse,
                                )
                                .unwrap_or_else(|_| panic!("Was unable to create a diff from a new empty file and {}",
                                    path.path));
                                {
                                    let mut patch_ids = patch_ids.lock().unwrap();
                                    let mut target_paths = target_paths.lock().unwrap();
                                    let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                    let mut modified = modified.lock().unwrap();

                                    patch_ids.push(patch_id);
                                    target_paths.push(path.path.clone());
                                    ref_patch_ids.push("First patch".to_string());
                                    modified.push(true);
                                }
                            } else {
                                
                                let mut patch_ids = patch_ids.lock().unwrap();
                                let mut target_paths = target_paths.lock().unwrap();
                                let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                let mut modified = modified.lock().unwrap();

                                patch_ids.push("DIR".to_string());
                                target_paths.push(path.path.clone());
                                ref_patch_ids.push("DIR".to_string());
                                modified.push(true);
                                
                            }
                        } else {
                            /*
                            When we detect a removed file, mark it as such without creating a patch. We will create a special case to
                            detect the removed file and thus remove it when restoring and moving forward/create it when restoring and
                            moving backwards.
                                */
                            debug!("Detected removed file!");

                            {
                                let mut patch_ids = patch_ids.lock().unwrap();
                                let mut target_paths = target_paths.lock().unwrap();
                                let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                let mut modified = modified.lock().unwrap();

                                patch_ids.push("REMOVED".to_string());
                                target_paths.push(path.path.clone());
                                ref_patch_ids.push("NONE".to_string());
                                modified.push(true);
                            }
                        }
                    } else {
                        let mut patch_ids = patch_ids.lock().unwrap();
                        let mut target_paths = target_paths.lock().unwrap();
                        let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                        let mut modified = modified.lock().unwrap();

                        // We will take a hash of the date modified and size of the file to use as an way to identify when the file has been changed.

                        if Path::new(&path.path).is_file() {
                            let file_contents = std::fs::read(&path.path).unwrap_or_else(|_| panic!("Could not open {} to check if it has been modified! Do I have read permission?",
                            path.path));
                            let hash = xxh3_64(&file_contents);
                            patch_ids.push(hash.to_string());
                        } else {
                            patch_ids.push("UNMODIFIED_DIRECTORY".to_string());
                        }
                        target_paths.push(path.path.clone());
                        ref_patch_ids.push("UNMODIFIED".to_string());
                        modified.push(false); 
                        // debug!("Skipping {} because it is not modified!", path.path);
                    }
                }    // Code for checking existing snapshot goes here
                pb.finish();
            }))
        }
    }
    for handle in children {
        // Wait for our children to die
        handle.join().expect("There was an issue joining all the threads, did a child die?");
    }
}

