use bsdiff::patch; // TODO: In fastest mode, we can restore directly the target since the reference is always just the original file. So restore_until needs to implement this.
use chrono::DateTime; // TODO: Snapshots should include a list of every single file at it's current state. This way we can actually ensure we get to the correct state.
use chrono::FixedOffset;
use log::debug;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs::{create_dir_all, exists, remove_dir_all, remove_file, File};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use crate::compression;
use crate::diffs;
use crate::DiffEntry;
use crate::SnapshotEntries;

pub fn restore_and_diff(
    _date_created: &String,
    target_path: &String,
    folder_path: &String,
    compression_level: u32,
    patch_store: &Arc<Mutex<Vec<DiffEntry>>>,
    create_reverse: bool,
    snapshot_mode: &String,
) -> Result<String, Box<dyn Error>> {
    debug!("Creating a patch using reference patch!");
    let mut target_date = "".to_string();
    let mut valid_target_path = "".to_string();
    if snapshot_mode == "fastest" {
        debug!("Trying to find initial patch to use as for fastest mode");
        {
            let patch_store = patch_store.lock().unwrap();

            for patch in patch_store.iter() {
                if patch.ref_patch == "First patch" && patch.target_path == *target_path {
                    debug!("Found good patch");
                    target_date = patch.date_created.clone();
                    valid_target_path = patch.target_path.clone();
                }
            }
            if target_date.is_empty() || valid_target_path.is_empty() {
                panic!("Could not find a valid initial patch for {}!", target_path);
            }
        }
    } else {
        panic!("Invalid snapshot mode {}!", snapshot_mode);
    }
    let mut sha256 = Sha256::new();
    sha256.update(target_date + &valid_target_path); // Generate an ID to identify the patch. This can be derived from the data stored in DiffEntry, which can then be used to identify where the patch file is.
    let patch_id: String = format!("{:X}", sha256.finalize()); // We now have the ID of the patch, so we can restore it.
    let patch_file;
    let target_file;

    let mut patch_file_compressed = std::fs::read(
        folder_path.clone() + "/.time/" + &patch_id + "-reverse",
    )
    .unwrap_or_else(|_| {
        panic!(
            "Could not open patch file! Try removing {} from the patch store.",
            &target_path
        )
    });
    if patch_file_compressed == [58, 51] {
        debug!("Detected fake patch!");
        // Not a valid patch, so we need to recover original file to use as reference.
        patch_file_compressed = std::fs::read(folder_path.clone() + "/.time/" + &patch_id)
            .unwrap_or_else(|_| {
                panic!(
                    "Could not open patch file! Try removing {} from the patch store.",
                    &target_path
                )
            });
        patch_file = compression::decompress_data(patch_file_compressed).unwrap_or_else(|_| {
            panic!(
                "Could not decompress patch file {}! Is it corrupt?",
                target_path
            )
        });
        target_file = Vec::new();
    } else {
        patch_file = compression::decompress_data(patch_file_compressed).unwrap_or_else(|_| {
            panic!(
                "Could not decompress patch file {}! Is it corrupt?",
                target_path
            )
        });
        target_file = std::fs::read(target_path).unwrap_or_else(|_| {
            panic!(
                "Could not open {} to restore reference patch! Metadata needs updating!",
                &target_path
            )
        });
    }
    let mut ref_file = Vec::new();

    patch(&target_file, &mut patch_file.as_slice(), &mut ref_file).unwrap_or_else(|_| {
        panic!(
            "There was an error restoring a reference patch to memory! Target file was {}",
            &target_path
        )
    });

    let patch_id = diffs::create_diff(
        "".to_string(),
        target_path.clone(),
        target_path.clone(),
        folder_path.clone() + "/.time",
        patch_id,
        ref_file,
        compression_level,
        patch_store,
        create_reverse,
    )
    .expect("There was an issue while creating a diff!");
    Ok(patch_id)
}

pub fn restore_snapshot(
    entry: &SnapshotEntries,
    time_dir: String,
    past: bool,
    snapshot_mode: &String,
) {
    let mut patch_path = "".to_string();
    let mut first_cycle = true;
    println!("Restoring snapshot {}!", entry.date_created);
    let mut dirs_to_remove = Vec::new(); // Remove dirs at the end since we need to cleanup the insides first
                                         // println!("{}", entry.patch_ids.len());
                                         // println!("{}", entry.ref_patch_ids.len());
    for (index_counter, id) in entry.patch_ids.clone().iter().enumerate() {
        // println!("{:?}", &entry.target_path);
        // TODO: Remove file if it is supposed to be removed
        // TODO: Check if is first patch, if so, don't attempt to restore
        debug!("Restoring patch {}", id);
        debug!("Restoring past version: {}", past);
        let mut skip_file = false;
        if id == "REMOVED" {
            skip_file = true;
            debug!("Detected removed file!");
            if past {
                // Going to past where file used to exist, so we need to restore upwards to recreate it.
                // Open patch store so we can restore

                let patch_store_file = time_dir.clone() + "/patches.json";

                // let path_temp_hold: HashSet<ModifiedList> = HashSet::new();
                let mut patch_store_path = File::open(Path::new(&patch_store_file))
                    .unwrap_or_else(|_| panic!("Could not open {patch_store_file}!"));

                let mut patch_store_contents = String::new();

                patch_store_path
                    .read_to_string(&mut patch_store_contents)
                    .expect("Patch store contains non UTF-8 characters which are unsupported!");
                let patch_store: Vec<DiffEntry> = serde_json::from_str(&patch_store_contents)
                    .expect("Patch store is corrupt. Sorgy :(");

                for patch_entry in patch_store.iter() {
                    let mut sha256 = Sha256::new();
                    // As long as patch store is properly ordered, we can go through and restore all matching paths.
                    if patch_entry.target_path == entry.target_path[index_counter] {
                        if &patch_entry.ref_patch == "First patch" {
                            let mut new_file: Vec<u8> = Vec::new();
                            if first_cycle {
                                check_and_create(&patch_entry.target_path);
                                first_cycle = false;
                            } //else {
                              // panic!("Detected patches.json is out of order! Cannot safely continue.");
                              // }
                            check_and_create(&patch_entry.target_path);
                            let target_file = std::fs::read(&patch_entry.target_path).unwrap();
                            sha256.update(
                                patch_entry.date_created.clone() + &patch_entry.target_path,
                            );
                            let patch_id = format!("{:X}", sha256.finalize());
                            let patch_path = time_dir.clone() + "/" + &patch_id;
                            let patch_file_compressed =
                                std::fs::read(&patch_path).unwrap_or_else(|_| panic!("Could not open {} to restore snapshot! Do I have read permission?",
                                patch_path));
                            let patch_file = compression::decompress_data(patch_file_compressed)
                                .unwrap_or_else(|_| {
                                    panic!(
                                        "Could not decompress data in file {}! Is it corrupt?",
                                        patch_path
                                    )
                                });
                            patch(&target_file, &mut patch_file.as_slice(), &mut new_file)
                                .unwrap_or_else(|_| {
                                    panic!("Unable to restore patch {}! Is it corrupt?", patch_id)
                                });
                            std::fs::write(&patch_entry.target_path, &new_file).unwrap_or_else(
                                |_| {
                                    panic!(
                                        "Unable to open file for writing: {}",
                                        &patch_entry.target_path
                                    )
                                },
                            );
                        } else if &patch_entry.ref_patch != "NONE" {
                            let mut new_file: Vec<u8> = Vec::new();
                            let target_file =
                                std::fs::read(&patch_entry.target_path).unwrap_or_else(|_| panic!("Could not open {} to restore snapshot. Metadata needs updating!",
                                &patch_entry.target_path));
                            sha256.update(
                                patch_entry.date_created.clone() + &patch_entry.target_path,
                            );
                            let patch_id = format!("{:X}", sha256.finalize());
                            let patch_path = time_dir.clone() + "/" + &patch_id;
                            let patch_file_compressed =
                                std::fs::read(&patch_path).unwrap_or_else(|_| panic!("Could not open {} to restore snapshot! Do I have read permission?",
                                patch_path));
                            let patch_file = compression::decompress_data(patch_file_compressed)
                                .unwrap_or_else(|_| {
                                    panic!(
                                        "Could not decompress data in file {}! Is it corrupt?",
                                        patch_path
                                    )
                                });
                            patch(&target_file, &mut patch_file.as_slice(), &mut new_file)
                                .unwrap_or_else(|_| {
                                    panic!("Unable to restore patch {}! Is it corrupt?", patch_id)
                                });
                            std::fs::write(&patch_entry.target_path, &new_file).unwrap_or_else(
                                |_| {
                                    panic!(
                                        "Unable to open file for writing: {}",
                                        &patch_entry.target_path
                                    )
                                },
                            );
                        } else {
                            debug!("Skipping file since ref_id is NONE");
                        }
                    }
                }
            } else {
                // In future, so we simply remove the file.
                let target_file = &entry.target_path[index_counter];
                let path = Path::new(target_file);
                if path.is_dir() {
                    debug!("Adding directory to queue to be removed: {}", target_file);
                    dirs_to_remove.push(target_file);
                } else {
                    let true_path = Path::new(target_file);
                    if true_path.exists() {
                        debug!("Removing file {}", target_file);
                        remove_file(Path::new(&target_file))
                            .unwrap_or_else(|_| panic!("Could not remove file {}!", &target_file));
                    }
                }
            }
        } else if id == "DIR" || id == "UNMODIFIED_DIRECTORY" {
            skip_file = true;
            debug!(
                "Creating dir if not exists: {}",
                &entry.target_path[index_counter]
            );

            let dir = Path::new(&entry.target_path[index_counter]);

            if !dir.exists() {
                create_dir_all(dir)
                    .unwrap_or_else(|_| panic!("Could not create directory {:?}!", dir));
            }
        } else if id.len() < 64 {
            // Assume this is a unmodified file hash. As such, check if the file is modified, and if it is, restore the original file.
            let file_contents = std::fs::read(&entry.target_path[index_counter]).unwrap_or_else(|_| panic!("Could not open {} to check if it has been modified! Do I have read permission?",
            entry.target_path[index_counter]));
            let hash = xxh3_64(&file_contents);

            if &hash.to_string() == id {
                debug!(
                    "{} is unmodified, leaving it alone!",
                    entry.target_path[index_counter]
                );
                skip_file = true;
            } else {
                debug!(
                    "{} is modified, restoring original",
                    entry.target_path[index_counter]
                );
            }
        }
        if !skip_file {
            debug!("No special conditions met, restoring file.");
            // Not a removed file
            if !past && entry.modified[index_counter] {
                // Target is in future.
                // In fastest mode, the reference is ALWAYS the first patch (which is just a compressed copy of the file.)
                // So we load this and then apply our patch to it. Thus we are fast, but also hog disk usage.
                if snapshot_mode == "fastest" {
                    debug!("Going towards future in fastest mode");
                    let mut patch_store_file =
                        File::open(Path::new(&(time_dir.clone() + "/patches.json")))
                            .unwrap_or_else(|_| {
                                panic!(
                                    "Unable to open patch store at {}!",
                                    time_dir.clone() + "/patches.json"
                                )
                            });

                    let mut patch_store_contents = String::new();
                    patch_store_file
                        .read_to_string(&mut patch_store_contents)
                        .expect("Unable to open patch store file!");
                    // patch_path = time_dir.clone() + "/" + &id + "-reverse";
                    let patch_store: Vec<DiffEntry> = serde_json::from_str(&patch_store_contents)
                        .expect("Patch store is corrupt. Sorgy :(");
                    // let mut iter = patch_store.iter().peekable(); // Wrong mode dipshit, you can use this in the future for other modes.
                    // let mut target_id: String = "".to_string();
                    // while let Some(patch) = iter.next() {
                    //     let mut sha256 = Sha256::new();
                    //     sha256.update(patch.date_created.clone() + &patch.target_path);
                    //     let check_id = format!("{:X}", sha256.finalize()); // We now have the correct target id

                    //     if check_id == id {
                    //         debug!(
                    //             "Found current patch inside store, getting patch directly ahead..."
                    //         );
                    //         let mut sha256 = Sha256::new();
                    //         sha256.reset();

                    //         if let Some(next_patch) = iter.peek() {
                    //             sha256.update(
                    //                 next_patch.date_created.clone() + &next_patch.target_path,
                    //             );
                    //             target_id = format!("{:X}", sha256.finalize());
                    //             debug!("Actually applying patch {}", target_id);
                    //         } else {
                    //             debug!("UNFINISHED UNFINISHED UNFINISHED: Need to handle case where there is no next patch!");
                    //         }
                    //     }
                    // }
                    let mut target_date = "".to_string();
                    let mut valid_target_path = "".to_string();

                    for patch in patch_store.iter() {
                        if patch.ref_patch == "First patch"
                            && patch.target_path == entry.target_path[index_counter]
                        {
                            debug!("Found correct initial patch");
                            target_date = patch.date_created.clone();
                            valid_target_path = patch.target_path.clone();
                        }
                    }

                    let true_path = Path::new(&entry.target_path[index_counter]);
                    if true_path.is_dir() {
                        debug!("Got First patch on a directory, creating {:?}", true_path);
                        create_dir_all(true_path).unwrap_or_else(|_| {
                            panic!("Unable to create directory {:?}!", true_path)
                        });
                    } else {
                        if target_date.is_empty() || valid_target_path.is_empty() {
                            panic!(
                                "Could not find a valid initial patch in the patch store for {}",
                                entry.target_path[index_counter]
                            )
                        }

                        let mut sha256 = Sha256::new();

                        sha256.update(target_date + &valid_target_path);
                        let patch_id = format!("{:X}", sha256.finalize());

                        debug!("Applying patch found from patch store");

                        debug!("Checking if file exists");
                        if !exists(&entry.target_path[index_counter]).unwrap_or_else(|_| {
                            panic!(
                                "Could not check if file exists at {}",
                                entry.target_path[index_counter]
                            )
                        }) {
                            debug!(
                                "File doesn't exist yet, creating {}",
                                entry.target_path[index_counter]
                            );
                            check_and_create(&entry.target_path[index_counter]);
                        }

                        let mut final_file = Vec::new();
                        let mut ref_file: Vec<u8> = Vec::new();
                        let patch_path = time_dir.clone() + "/" + &patch_id; // Note that this will never be the first patch, so we don't need to handle that case.
                        let patch_final = time_dir.clone() + "/" + &id;
                        let target_path = &entry.target_path[index_counter];
                        // let target_file = std::fs::read(&target_path).expect(&format!(
                        //     "Could not open {} to restore snapshot. Metadata needs updating!",
                        //     &target_path
                        // ));
                        let patch_file_compressed = std::fs::read(&patch_path).unwrap_or_else(|_| panic!("Could not open {} to restore snapshot! Do I have read permission?",
                            patch_path));
                        let patch_file = compression::decompress_data(patch_file_compressed)
                            .unwrap_or_else(|_| {
                                panic!(
                                    "Could not decompress data in file {}! Is it corrupt?",
                                    patch_path
                                )
                            });
                        let patch_final_file_compressed =
                            std::fs::read(&patch_final).unwrap_or_else(|_| panic!("Could not open {} to restore snapshot! Do I have read permission?",
                                patch_path));
                        let patch_file_final =
                            compression::decompress_data(patch_final_file_compressed)
                                .unwrap_or_else(|_| {
                                    panic!(
                                        "Could not decompress data in file {}! Is it corrupt?",
                                        patch_path
                                    )
                                });
                        // Generate initial version of file to be used as the reference
                        patch(&final_file, &mut patch_file.as_slice(), &mut ref_file)
                            .unwrap_or_else(|_| {
                                panic!("There was an issue applying patch {}!", patch_path)
                            });

                        patch(&ref_file, &mut patch_file_final.as_slice(), &mut final_file)
                            .unwrap_or_else(|_| {
                                panic!("There was an issue applying patch {}!", patch_path)
                            });
                        debug!("Writing final target file");
                        std::fs::write(target_path, &final_file)
                            .unwrap_or_else(|_| panic!("Unable to write to {}!", target_path));
                        // index_counter += 1;
                    }
                } else {
                    panic!(
                        "Only fastest snapshot mode supported, not {}!",
                        snapshot_mode
                    );
                }
            } else if entry.modified[index_counter] {
                // Target is in past. Currently works for "fastest" mode. Others untested
                let mut ref_patch_compressed: Vec<u8> = [58, 51].to_vec(); // The default state will fail the validity check, so we don't need a brand new variable to track if this is "First patch" or not.
                let mut ref_path = "".to_string();
                if &entry.ref_patch_ids[index_counter] != "First patch" {
                    debug!("Restoring into the past!");
                    patch_path = time_dir.clone() + "/" + &id;

                    ref_path =
                        time_dir.clone() + "/" + &entry.ref_patch_ids[index_counter] + "-reverse";
                    debug!("Found reference patch {}", ref_path);
                    ref_patch_compressed = std::fs::read(&ref_path).unwrap_or_else(|_| {
                        panic!("Could not read reference patch at {}!", ref_path)
                    });
                }

                if ref_patch_compressed == [58, 51] {
                    // Either this is first patch, or we tried to read a false patch. Either way, we will just restore the initial compressed patch.

                    if &entry.ref_patch_ids[index_counter] == "First patch" {
                        // First patch, we need to get the proper id to restore. Unfortunately, this means we need to load and process patches.json.
                        debug!("Got a first patch, loading patches.json...");

                        let patch_store_file = time_dir.clone() + "/patches.json";

                        // let path_temp_hold: HashSet<ModifiedList> = HashSet::new();
                        let mut patch_store_path = File::open(Path::new(&patch_store_file))
                            .unwrap_or_else(|_| panic!("Could not open {patch_store_file}!"));

                        let mut patch_store_contents = String::new();

                        patch_store_path
                            .read_to_string(&mut patch_store_contents)
                            .expect(
                                "Patch store contains non UTF-8 characters which are unsupported!",
                            );
                        let patch_store: Vec<DiffEntry> =
                            serde_json::from_str(&patch_store_contents)
                                .expect("Patch store is corrupt. Sorgy :(");
                        let mut target_id = "".to_string();
                        for item in patch_store.iter() {
                            if item.target_path == entry.target_path[index_counter] {
                                let mut sha256 = Sha256::new();
                                sha256.update(item.date_created.clone() + &item.target_path);
                                target_id = format!("{:X}", sha256.finalize()); // We now have the correct target id
                                break;
                            }
                        }
                        if target_id.is_empty() {
                            panic!(
                                "Could not find a target_id that should exist for file {:?}",
                                &entry.target_path
                            );
                        }
                        ref_path = time_dir.clone() + "/" + &target_id;
                        debug!("Got ref_path as {}", ref_path);
                    } else {
                        // Read a false patch, so remove the reverse and restore it
                        ref_path = time_dir.clone() + "/" + &entry.ref_patch_ids[index_counter];
                    }
                    ref_patch_compressed = std::fs::read(&ref_path).unwrap_or_else(|_| {
                        panic!("Could not read reference patch at {}!", ref_path)
                    });
                    let mut final_target: Vec<u8> = Vec::new();
                    let empty: Vec<u8> = Vec::new();
                    let ref_patch_full_file = compression::decompress_data(ref_patch_compressed)
                        .unwrap_or_else(|_| {
                            panic!("There was an error decompressing {}!", ref_path)
                        });
                    patch(
                        &empty,
                        &mut ref_patch_full_file.as_slice(),
                        &mut final_target,
                    )
                    .unwrap_or_else(|_| {
                        panic!(
                            "There was an error applying patch {} to an empty vec!",
                            ref_path
                        )
                    });
                    let target_path = &entry.target_path[index_counter];
                    check_and_create(target_path);
                    debug!("Restoring original file {}", target_path);
                    std::fs::write(target_path, &final_target)
                        .unwrap_or_else(|_| panic!("Unable to write to {}!", target_path));
                } else {
                    // This is a valid patch/regular case
                    // TODO: Detect if we are going to the original version and skip the middle steps.
                    let mut ref_file: Vec<u8> = Vec::new();
                    let mut final_target: Vec<u8> = Vec::new();
                    let target_file;
                    {
                        let ref_patch = compression::decompress_data(ref_patch_compressed)
                            .unwrap_or_else(|_| {
                                panic!("There was an issue decompressing {}!", ref_path)
                            });

                        let target_path = &entry.target_path[index_counter];

                        target_file = std::fs::read(target_path).unwrap_or_else(|_| {
                            panic!(
                                "Could not open {} to restore snapshot. Metadata needs updating!",
                                &target_path
                            )
                        });

                        patch(&target_file, &mut ref_patch.as_slice(), &mut ref_file)
                            .unwrap_or_else(|_| {
                                panic!("There was an issue applying reference patch {}!", ref_path)
                            }); // TODO: This is impossible, right? We cannot apply this patch against a new unkown file. We need to build upwards.
                    }
                    let patch_file_compressed = std::fs::read(&patch_path).unwrap_or_else(|_| {
                        panic!(
                            "Could not open {} to restore snapshot! Do I have read permission?",
                            patch_path
                        )
                    });
                    let patch_file = compression::decompress_data(patch_file_compressed)
                        .unwrap_or_else(|_| {
                            panic!(
                                "Could not decompress data in file {}! Is it corrupt?",
                                patch_path
                            )
                        });
                    patch(&ref_file, &mut patch_file.as_slice(), &mut final_target).unwrap_or_else(
                        |_| panic!("There was an issue applying patch {}!", patch_path),
                    );
                    let target_path = &entry.target_path[index_counter];

                    debug!("Restoring file {}", target_path);
                    std::fs::write(target_path, &final_target)
                        .unwrap_or_else(|_| panic!("Unable to write to {}!", target_path));
                }
            } else {
                debug!("{:?} is not modified, leaving it alone!", entry.target_path);
            }
        }
    }

    // We need to do a walkthrough of the directory and remove any files that are not part of the snapshot. This way files added in the future won't be there when we restore a past snapshot.
    let folder_path = Path::new(&time_dir).parent();
    match folder_path {
        // Ok what the fuck is even going on :< clearly I need to read the rust book better
        Some(x) => {
            for path in WalkDir::new(x) {
                match path {
                    Ok(v) => {
                        let v_parent = v.path().parent();
                        match v_parent {
                            Some(vp) => {
                                if !entry.target_path.contains(&v.path().display().to_string())
                                    && v.path() != x
                                    && v.path() != Path::new(&time_dir)
                                    && vp != Path::new(&time_dir)
                                {
                                    // println!("{:?}", v.path());
                                    if v.path().is_file() {
                                        debug!("Removing {}", v.path().display());
                                        remove_file(v.path()).unwrap_or_else(|_| {
                                            panic!("Unable to remove {}!", v.path().display())
                                        })
                                    // } else if v
                                    //     .path()
                                    //     .read_dir()
                                    //     .unwrap_or_else(|_| {
                                    //         panic!("Could not peek into directory {:?}", v.path())
                                    //     })
                                    //     .next()
                                    //     .is_none()
                                    } else {
                                        // println!("{}", v.path().display());
                                        // Check if directory to be removed is referenced in list at all, and if the reference is NOT to remove it, and if so, don't remove it.
                                        let mut id_count = 0;
                                        for path in entry.target_path.iter() {
                                            // This ensures we don't accidentally remove some empty directory that we want to keep.
                                            if !path.contains(&v.path().display().to_string())
                                                && v.path().exists()
                                                && entry.patch_ids[id_count] != "REMOVED"
                                            {
                                                debug!("Removing {}", v.path().display());
                                                remove_dir_all(v.path()).unwrap_or_else(|_| {
                                                    panic!(
                                                        "Unable to remove {}!",
                                                        v.path().display()
                                                    )
                                                });
                                            }
                                            id_count += 1;
                                        }
                                    }
                                }
                            }
                            None => panic!("Error parsing {:?}", v_parent),
                        }
                    }
                    Err(e) => println!("Error parsing {}", e),
                }
            }
        }
        None => panic!(
            "There was an issue trying to get the parent directory of {:?}!",
            folder_path
        ),
    }

    for path in dirs_to_remove.iter() {
        let true_path = Path::new(path);
        if true_path.exists() {
            remove_dir_all(path).unwrap_or_else(|_| panic!("Could not remove dir {}!", path));
        }
        // We can do all, since we know at this point the only remaining directories will just have other empty directories in it (assuming nothing went wrong when collecting metadata.)
    }
}

pub fn restore_snapshot_until(
    // In fastest mode, reference always being the initial file means we can restore directly when going forward or backward, making restoring much much faster.
    snapshot_store: Vec<SnapshotEntries>,
    folder_path: &String,
    selected_item: &DateTime<FixedOffset>,
    in_past: bool,
    snapshot_mode: &String,
) {
    if snapshot_mode == "fastest" {
        // If we are in fastest mode, we don't care about restoring anything in between since the reference is alwyas the initial version of the file.
        debug!("restoring_until in fastest mode. Skipping intermediates.");
        debug!("Target date is {}", selected_item);
        for snapshot in snapshot_store.iter() {
            let date_entry =
                DateTime::parse_from_str(&snapshot.date_created, "%Y-%m-%d %H:%M:%S%.9f %z")
                    .unwrap();
            let formatted_date = date_entry.format("%Y-%m-%d %H:%M:%S%.9f %z").to_string();
            debug!("formatted_date is {}", formatted_date);
            if formatted_date == *selected_item.format("%Y-%m-%d %H:%M:%S%.9f %z").to_string() {
                debug!("Found correct snapshot to restore in fastest mode.");
                restore_snapshot(
                    snapshot,
                    folder_path.clone() + "/.time",
                    in_past,
                    snapshot_mode,
                );
            }
        }
    } else if in_past {
        for snapshot in snapshot_store.iter().rev() {
            let date_entry =
                DateTime::parse_from_str(&snapshot.date_created, "%Y-%m-%d %H:%M:%S%.9f %z")
                    .unwrap();

            if date_entry == *selected_item {
                break;
            }
            restore_snapshot(
                snapshot,
                folder_path.clone() + "/.time",
                in_past,
                snapshot_mode,
            );
            // Past is true since we want to restore the reverse patch
        }
    } else {
        debug!("Not reversing!");
        // println!("{:?}", snapshot_store);
        for snapshot in snapshot_store.iter() {
            let date_entry =
                DateTime::parse_from_str(&snapshot.date_created, "%Y-%m-%d %H:%M:%S%.9f %z")
                    .unwrap();

            if date_entry == *selected_item {
                break;
            }
            restore_snapshot(
                snapshot,
                folder_path.clone() + "/.time",
                in_past,
                snapshot_mode,
            );
            // Past is true since we want to restore the reverse patch
        }
    }
}

fn check_and_create(target_path: &String) {
    if !exists(target_path)
        .unwrap_or_else(|_| panic!("Could not check if file exists at {}", target_path))
    {
        let true_path = Path::new(target_path).parent(); // Turns target_path into a Path. I know I should do this everywhere.
        match true_path {
            Some(x) => {
                if !exists(x).unwrap() {
                    debug!("Parent directory doesn't exist, creating {:?}", true_path);
                    create_dir_all(x)
                        .unwrap_or_else(|_| panic!("Could not create parent directory at {:?}", x));
                }
            }
            None => panic!(
                "There was an issue trying to get the parent directory of {}!",
                target_path
            ),
        }
        debug!("File doesn't exist yet, creating {}", target_path);
        File::create(Path::new(&target_path))
            .unwrap_or_else(|_| panic!("Could not create file at {}!", target_path));
    }
}
