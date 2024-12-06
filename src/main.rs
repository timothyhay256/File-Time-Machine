// Made with pain by someone who desperately needed a distraction from the 2024 election.
// Trans rights are human rights.
// TODO: Optional exclude directories
// TODO: Restore directly, don't restore intermediates.
#![windows_subsystem = "windows"] // Prevents console from opening when on Windows.
use chrono::DateTime;
use directories::BaseDirs;
use gumdrop::Options;
use inquire::Select;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    hash::Hash,
    io::{Read, Write},
    path::Path,
    process,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use xxhash_rust::xxh3::xxh3_64;
// use std::time::Instant; // For debugging

pub mod compression;
pub mod diffs;
pub mod metadata_manager;
pub mod restore;

#[derive(Deserialize, Serialize, Hash, PartialEq, Eq, Debug, Clone)]

pub struct DiffEntry {
    // TODO: Depreceate in favor of SnapshotEntries?
    date_created: String,
    target_path: String,
    ref_patch: String,
}

#[derive(PartialEq, Hash, Eq, Debug, Clone)]
pub struct ModifiedList {
    path: String,
    exists: bool,
    modified: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Hash, Eq)] // Derive Serialize for JSON serialization
pub struct MetaFile {
    date_modified: u64,
    hash: String,
    size: u64,
    path: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct SnapshotEntries {
    date_created: String,
    patch_ids: Vec<String>,
    target_path: Vec<String>,
    ref_patch_ids: Vec<String>,
    modified: Vec<bool>,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
struct Config {
    // This will grow with time
    folder_path: String,
    get_hashes: bool,
    thread_count: u32,
    brotli_compression_level: u32,
    snapshot_mode: String,
    its_my_fault_if_i_lose_data: bool,
}

#[derive(Debug, Options)]
struct MyOptions {
    #[options(help = "print help message")]
    help: bool,
    #[options(help = "be verbose")]
    verbose: bool,
    #[options(help = "specify a specific config file")]
    config: String,

    // The `command` option will delegate option parsing to the command type,
    // starting at the first free argument.
    #[options(command)]
    command: Option<Command>,
}

#[derive(Debug, Options)]
enum Command {
    // Command names are generated from variant names.
    // By default, a CamelCase name will be converted into a lowercase,
    // hyphen-separated name; e.g. `FooBar` becomes `foo-bar`.
    //
    // Names can be explicitly specified using `#[options(name = "...")]`
    #[options(help = "take a snapshot")]
    Snapshot(SnapshotOptions),
    #[options(help = "restore a snapshot")]
    Restore(RestoreOptions),
}

// Options accepted for the `snapshot` command
#[derive(Debug, Options)] // TODO: Add options
struct SnapshotOptions {}

// Options accepted for the `restore` command
#[derive(Debug, Options)] // TODO: Add options (list snapshots, restore specific one)
struct RestoreOptions {
    #[options(help = "restore nth snapshot starting from most recent")]
    restore_index: u32,
}
fn main() {
    let mut want_restore = false;
    let mut skip_snap = false;
    let mut man_conf = false;
    let opts = MyOptions::parse_args_default_or_exit();

    let conf_dir;

    if opts.verbose {
        println!("Enabling verbosity by setting env var RUST_LOG to debug");

        env::set_var("RUST_LOG", "debug");
    }

    env_logger::init();

    if !opts.config.is_empty() {
        println!("Using specific config file {}!", opts.config);
        conf_dir = opts.config;
        man_conf = true;
    } else {
        let home_dir = if let Some(user_dirs) = BaseDirs::new() {
            if let Some(path_str) = user_dirs.home_dir().to_str() {
                path_str.to_string()
            } else {
                panic!("Home directory is not valid UTF-8! What is wrong with your system??");
            }
        } else {
            panic!("Unable to retrieve user directories.");
        };
        // println!("{home_dir}");
        conf_dir = home_dir + "/.file-time-machine";
    }

    if let Some(Command::Snapshot(ref _snapshot_options)) = opts.command {
        println!("Taking snapshot!");
    } else if let Some(Command::Restore(ref _restore_options)) = opts.command {
        println!("Restoring!");
        want_restore = true;
    } else {
        println!("No valid option was provided, taking a snapshot!");
    }
    // if args.len() < 2 {
    //     println!("No arguments provided, attempting to snapshot if config is valid.");
    // } else if args[1] == "snapshot" {
    //     println!("Attempting to snapshot if config is valid.");
    // } else if args[1] == "restore" {
    //     println!("Attempting to restore a snapshot. Fixme!");
    //     want_restore = true;
    // } else {
    //     panic!(
    //         "Invalid command {}\nValid commands are: snapshot, restore.",
    //         args[1]
    //     );
    // }
    // println!("{conf_dir}");
    if !Path::new(&conf_dir).exists() {
        if man_conf {
            panic!("Could not locate config file {}!", conf_dir);
        }
        fs::create_dir(Path::new(&conf_dir.clone())).expect(
            "Could not create .file-time-machine in home directory! I should not be run as root.",
        );
        println!("Creating .file-time-machine");
    }
    let conf_path;
    if man_conf {
        conf_path = conf_dir;
    } else {
        conf_path = conf_dir + "/config.json";
    }
    let mut config_file = File::open(Path::new(&conf_path)).expect("Could not open config file! Create one at $HOME/.file-time-machine/config.json as specified in documentation.");

    let mut config_file_contents = String::new();
    config_file
        .read_to_string(&mut config_file_contents)
        .expect(
            "The config file contains non UTF-8 characters, what in the world did you put in it??",
        );
    let config_holder: Vec<Config> = serde_json::from_str(&config_file_contents)
        .expect("The config file was not formatted properly and could not be read.");

    let mut folder_path = config_holder[0].folder_path.clone(); // Shut up, I am tired
    let hash_enabled = config_holder[0].get_hashes;
    let mut thread_count = config_holder[0].thread_count;
    let compression_level = config_holder[0].brotli_compression_level;
    let snapshot_mode = config_holder[0].snapshot_mode.clone();
    let supress_warn = config_holder[0].its_my_fault_if_i_lose_data;

    if snapshot_mode != "fastest" {
        println!("Only fastest snapshot mode is currently implemented!");
        process::exit(1);
    }
    debug!("Snapshot mode is {}", snapshot_mode);

    if !supress_warn {
        warn!("\nWARNING WARNING WARNING\nThis program is NOT production ready! You probably WILL lose data using it!\nSet its_my_fault_if_i_lose_data to true to suppress this warning.\n");
        thread::sleep(Duration::from_secs(3));
    }

    folder_path = folder_path.trim_end_matches('/').to_string();
    let create_reverse; // Disabled only on first run to reduce disk usage

    if thread_count == 0 {
        thread_count = num_cpus::get() as u32;
        debug!("thread_count automatically set to {}", thread_count);
    }
    if want_restore {
        skip_snap = true;
        let snapshot_store_file = folder_path.clone() + "/.time/snapshots.json";
        let snapshot_store: Vec<SnapshotEntries>;
        let mut change_count = 0;
        let mut options = Vec::new();

        // println!("{}", snapshot_store_file);
        if !Path::new(&snapshot_store_file).exists() {
            panic!("Did not find a valid snapshot store, have you created any snapshots yet?");
        }

        let mut file = File::open(Path::new(&snapshot_store_file))
            .unwrap_or_else(|_| panic!("Could not open {}!", snapshot_store_file));

        let mut file_contents = String::new();
        file.read_to_string(&mut file_contents)
            .unwrap_or_else(|_| panic!("Unable to read file {}!", snapshot_store_file));
        if !file_contents.is_empty() {
            snapshot_store =
                serde_json::from_str(&file_contents).expect("Snapshot store is corrupt!");
        } else {
            panic!("Snapshot store exists, but is empty! No snapshots available.");
        }
        /*struct Point {
            x: f64,
            y: f64,
        }

        enum Shape {
            Circle(Point, f64),
            Rectangle(Point, Point),
        }

        fn main() {
            let my_shape = Shape::Circle(Point { x: 0.0, y: 0.0 }, 10.0);

            match my_shape {
                Shape::Circle(_, value) => println!("value: {}", value),
                _ => println!("Something else"),
            }
        } */
        let selected_item;
        if let Some(Command::Restore(ref restore_options)) = opts.command {
            // println!("{}", snapshot_store.len());
            if snapshot_store.len() >= restore_options.restore_index.try_into().unwrap()
                && 0 < restore_options.restore_index.try_into().unwrap()
            {
                selected_item = DateTime::parse_from_str(
                    &snapshot_store[restore_options.restore_index as usize - 1].date_created,
                    "%Y-%m-%d %H:%M:%S%.9f %z",
                )
                .unwrap();
            } else {
                if restore_options.restore_index != 0 {
                    // Needed because afaik Gumdrop sets it to 0 if it wasn't passed. This is not desired behaviour.
                    println!(
                        "{} is an invalid snapshot. Entering interactive.",
                        restore_options.restore_index
                    );
                }
                for snapshot in &snapshot_store {
                    for _change in snapshot.patch_ids.clone() {
                        change_count += 1;
                    }
                    let date_entry = DateTime::parse_from_str(
                        &snapshot.date_created,
                        "%Y-%m-%d %H:%M:%S%.9f %z",
                    )
                    .unwrap();
                    let formatted_date = date_entry.format("%Y-%m-%d %H:%M:%S %z").to_string();
                    // debug!("formatted_date is {}", formatted_date);
                    options.push(formatted_date + " files changed: " + &change_count.to_string());
                    change_count = 0;
                }
                let selection = Select::new("Select a snapshot to restore:", options).prompt();

                let selected_item_pretty: String = match selection {
                    Ok(choice) => choice,
                    Err(_) => panic!("There was an issue, please try again."),
                };
                // Extract true option from human readable format
                let selected_item_str = selected_item_pretty[0..25].to_string();
                debug!("{selected_item_str}");
                selected_item =
                    DateTime::parse_from_str(&selected_item_str, "%Y-%m-%d %H:%M:%S %z")
                        .expect("Could not correctly parse date in activeSnapshot, is it corrupt?");
            }
        } else {
            panic!("Could not parse a valid command.");
        }

        /*
        We have a entry that we want to restore, if it is in the past:
        In fastest mode, restore directly, don't restore intermediates

        if it is in the future:
        Restore up until we restore the proper patch.
         */

        let active_snapshot_path = folder_path.clone() + "/.time/activeSnapshot";

        if !Path::new(&active_snapshot_path).exists() {
            debug!("No activeSnapshot found, assuming target has to be in past.");

            // In fastest, restore_snapshot_until will NOT iterate. In this case, the name is misleading.

            restore::restore_snapshot_until(
                snapshot_store,
                &folder_path,
                &selected_item,
                true,
                &snapshot_mode,
            );

            let mut active_snapshot = File::create(Path::new(&active_snapshot_path))
                .unwrap_or_else(|_| {
                    panic!("Could not create {active_snapshot_path}, do I have write permission?")
                });
            active_snapshot
                .write_all(selected_item.to_string().as_bytes())
                .unwrap_or_else(|_| {
                    panic!("Unable to write to active_snapshot file at {active_snapshot_path}")
                });
        } else {
            let mut file = File::open(Path::new(&(folder_path.clone() + "/.time/activeSnapshot")))
                .unwrap_or_else(|_| {
                    panic!(
                        "Could not read {}!",
                        folder_path.clone() + "/.time/activeSnapshot"
                    )
                });

            let mut file_contents = String::new();
            file.read_to_string(&mut file_contents).unwrap_or_else(|_| {
                panic!(
                    "Could not read from {}! Do I have correct permissions?",
                    folder_path.clone() + "/.time/activeSnapshot"
                )
            });

            let active_snapshot_date_stupid = // Please fix me this is stupid
                DateTime::parse_from_str(&file_contents, "%Y-%m-%d %H:%M:%S%.9f %z")
                    .unwrap()
                    .format("%Y-%m-%d %H:%M:%S %z")
                    .to_string();
            let active_snapshot_date =
                DateTime::parse_from_str(&active_snapshot_date_stupid, "%Y-%m-%d %H:%M:%S %z")
                    .unwrap();

            if selected_item > active_snapshot_date {
                debug!("Snapshot is in future!");
                restore::restore_snapshot_until(
                    snapshot_store,
                    &folder_path,
                    &selected_item,
                    false,
                    &snapshot_mode,
                );
                fs::remove_file(&active_snapshot_path).unwrap_or_else(|_| {
                    panic!(
                        "Could not remove {}, it needs to be writeable!",
                        active_snapshot_path
                    )
                });
                let mut active_snapshot = File::create(Path::new(&active_snapshot_path))
                    .unwrap_or_else(|_| {
                        panic!(
                            "Could not create {active_snapshot_path}, do I have write permission?"
                        )
                    });
                active_snapshot
                    .write_all(selected_item.to_string().as_bytes())
                    .unwrap_or_else(|_| {
                        panic!("Unable to write to activeSnapshot file at {active_snapshot_path}")
                    });
            } else if selected_item < active_snapshot_date {
                debug!("Snapshot is in past!");
                restore::restore_snapshot_until(
                    snapshot_store,
                    &folder_path,
                    &selected_item,
                    true,
                    &snapshot_mode,
                );
                fs::remove_file(&active_snapshot_path).unwrap_or_else(|_| {
                    panic!(
                        "Could not remove {}, it needs to be writeable!",
                        active_snapshot_path
                    )
                });
                let mut active_snapshot = File::create(Path::new(&active_snapshot_path))
                    .unwrap_or_else(|_| {
                        panic!(
                            "Could not create {active_snapshot_path}, do I have write permission?"
                        )
                    });
                active_snapshot
                    .write_all(selected_item.to_string().as_bytes())
                    .unwrap_or_else(|_| {
                        panic!("Unable to write to activeSnapshot file at {active_snapshot_path}")
                    });
            } else {
                println!(
                    "The snapshot you selected is already the active snapshot! Nothing to do."
                );
                process::exit(1);
            }
        }
        println!("Finished restoring. You can safely make changes, but they will not be saved unless a new snapshot is created.");
    }

    if !skip_snap {
        let mut initial_run = false;
        debug!("take snapshot");
        if !Path::new(&(folder_path.clone() + "/.time/metadata.json")).exists() {
            debug!("{folder_path}/.time/metadata.json");
            if !Path::new(&(folder_path.clone() + "/.time")).exists() {
                fs::create_dir(folder_path.clone() + "/.time").unwrap_or_else(|_| {
                    panic!(
                        "Unable to create a .time folder at {}!",
                        folder_path.clone() + "/.time"
                    )
                });
            }
            File::create(Path::new(&(folder_path.clone() + "/.time/tmp_empty"))).unwrap_or_else(
                |_| {
                    panic!(
                        "Unable to create a temporary empty file at {}!",
                        folder_path.clone() + "/.time/tmp_empty"
                    )
                },
            );
            println!("No .time or metadata found, creating.");

            println!("Collecting metadata of: {folder_path}");
            if hash_enabled {
                warn!("Hashes are enabled. Collecting metadata may take a while.");
            }

            // hash(folder_path).expect("msg");
            let metadata_holder: HashSet<MetaFile> = HashSet::new();
            let metadata_holder =
                diffs::get_properties(&folder_path, metadata_holder, hash_enabled)
                    .expect("Issue getting hashes of files in folder {folder_path}");
            metadata_manager::write_metadata_to_file(
                &metadata_holder,
                &(folder_path.clone() + "/.time/metadata.json"),
            );

            debug!("Running a initial snapshot...");
            initial_run = true; // Use to indicate that despite there being zero changes, we still want to run on all the files
        }
        println!("Existing .time folder found, looking for changes...");
        debug!("Looking for changes in directory {}", folder_path);
        let metafile = folder_path.clone() + "/.time/metadata.json";
        let mut metadata_holder: HashSet<MetaFile> = HashSet::new();

        if !initial_run {
            debug!("initial_run is false, reading metadata!");
            metadata_holder = metadata_manager::read_metadata_from_file(&metafile)
                .unwrap_or_else(|_| panic!("Couldn't read the metadata file at {metafile}"));
        }
        let changed_files = diffs::get_diffs(false, &metadata_holder, &folder_path)
            .expect("Couldn't check for diffs! No files have been written.");
        // for meta in changed_files {
        //     println!("File Path: {}", meta.path);
        // }
        File::create(Path::new(&(folder_path.clone() + "/.time/tmp_empty"))).unwrap_or_else(|_| {
            panic!(
                "Unable to create a temporary empty file at {}!",
                folder_path.clone() + "/.time/tmp_empty"
            )
        });
        diffs::update_metadata(&mut metadata_holder, &changed_files, hash_enabled)
            .expect("Something went wrong when collecting metadata. Do you have read permission?");
        if !initial_run {
            debug!("initial_run is false, writing metadata!");
            metadata_manager::write_metadata_to_file(&metadata_holder, &metafile);
        }
        println!("Finished updating metadata.");

        println!("Creating snapshot with {} threads...", thread_count);
        let mut patch_store: Arc<Mutex<Vec<DiffEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let patch_store_file = folder_path.clone() + "/.time/patches.json";
        let snapshot_store_file = folder_path.clone() + "/.time/snapshots.json";
        let patch_ids = Arc::new(Mutex::new(Vec::new())); // These need to be communicated through threads, thus Arc and Mutex.
        let target_paths = Arc::new(Mutex::new(Vec::new()));
        let ref_patch_ids = Arc::new(Mutex::new(Vec::new()));
        let modified = Arc::new(Mutex::new(Vec::new()));

        let mut snapshot_store: Vec<SnapshotEntries> = Vec::new();

        if !Path::new(&snapshot_store_file).exists() {
            File::create(Path::new(&snapshot_store_file)).unwrap_or_else(|_| {
                panic!(
                    "Could not create snapshot store at {}!",
                    snapshot_store_file
                )
            });
        } else {
            let mut file = File::open(Path::new(&snapshot_store_file))
                .unwrap_or_else(|_| panic!("Could not open {}!", snapshot_store_file));

            let mut file_contents = String::new();
            file.read_to_string(&mut file_contents)
                .unwrap_or_else(|_| panic!("Unable to read file {}!", snapshot_store_file));
            if !file_contents.is_empty() {
                snapshot_store =
                    serde_json::from_str(&file_contents).expect("Snapshot store is corrupt!");
            }
        }

        if !Path::new(&patch_store_file).exists() {
            println!("Did not find patch store! An original compressed copy of every file will be made to use as reference.");
            create_reverse = false; // Since this is the first snapshot, there is no need to create a reverse snapshot and use 2*n storage
                                    // Split here if changed_files is greater than thread count!
            let mut changed_files_vec: Vec<ModifiedList> = Vec::new();
            let mut changed_count: u32 = 0;

            File::create(Path::new(&patch_store_file)).unwrap_or_else(|_| {
                panic!(
                    "Unable to create patch store at {}",
                    patch_store_file.clone() + "/patches.json"
                )
            });
            let mut patch_store_path =
                File::open(Path::new(&patch_store_file)).expect("Unable to open patch store file!");

            let mut patch_store_contents = String::new();
            patch_store_path
                .read_to_string(&mut patch_store_contents)
                .expect("Unable to open patch store file!");
            patch_store = Arc::new(Mutex::new(Vec::new()));

            for item in &changed_files {
                // Allows us to split the Vec to give to threads
                changed_count += 1;
                changed_files_vec.push(ModifiedList {
                    path: item.path.clone(),
                    exists: item.exists,
                    modified: item.modified,
                });
            }
            if changed_files_vec.len() > thread_count.try_into().unwrap() {
                debug!("Running as initial run!");
                diffs::create_diffs_multithread(
                    &patch_ids,
                    &ref_patch_ids,
                    &target_paths,
                    &modified,
                    &folder_path,
                    changed_files_vec,
                    changed_count,
                    thread_count,
                    compression_level,
                    &patch_store,
                    create_reverse,
                    true, // Inital run
                    &snapshot_mode,
                );
            } else {
                // Run regularily here!
                debug!("Run regularily");
                for path in changed_files.iter() {
                    /*
                    Get relative path of backup directory, go through changed_files, and reference relative path of backup directory. ModifiedList will handle removed files.
                    A non-existing file can be passed, and it will be handled within get_diffs.
                    */
                    if path.modified {
                        if Path::new(&path.path).is_file() {
                            let patch_id = diffs::create_diff(
                                "".to_string(), // This will never exist, so we can always create a temp file instead.
                                path.path.clone(),
                                path.path.clone(),
                                folder_path.clone() + "/.time",
                                "First patch".to_string(),
                                Vec::new(),
                                compression_level,
                                &patch_store,
                                create_reverse,
                            )
                            .unwrap_or_else(|_| {
                                panic!(
                                    "Was unable to create a diff between a new empty file and {}",
                                    path.path
                                )
                            });
                            {
                                let mut patch_ids = patch_ids.lock().unwrap();
                                let mut target_paths = target_paths.lock().unwrap();
                                let mut ref_patch_ids = ref_patch_ids.lock().unwrap();
                                let mut modified = modified.lock().unwrap();

                                patch_ids.push(patch_id);
                                target_paths.push(path.path.clone());
                                ref_patch_ids.push("First patch".to_string());
                                modified.push(path.modified);
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
                        // debug!("Skipping {} because it is not modified!", path.path);
                    }
                }
            }
        } else {
            debug!("Found patch store!");
            // let path_temp_hold: HashSet<ModifiedList> = HashSet::new();
            let mut patch_store_path = File::open(Path::new(&patch_store_file))
                .unwrap_or_else(|_| panic!("Could not open {patch_store_file}!"));

            let mut patch_store_contents = String::new();
            patch_store_path
                .read_to_string(&mut patch_store_contents)
                .expect("Patch store contains non UTF-8 characters which are unsupported!");
            {
                let mut patch_store = patch_store.lock().unwrap();

                *patch_store = serde_json::from_str(&patch_store_contents)
                    .expect("Patch store is corrupt. Sorgy :(");
            }
            /*
            Cycle through changed files, and check if a snapshot exists. If it does, restore snapshot to memory, to use as reference file.
            Then we create a new patch from the two.

            If no snapshot exists yet, use backup directory as reference file to create snapshot.
            */
            // REMEMBER TO PASS patch_store
            // println!("{:?}", path_temp_hold);
            // println!("fdsfsd");
            // println!("{:?}", changed_files); // populate patch_store and pass it
            let mut changed_files_vec: Vec<ModifiedList> = Vec::new();
            let mut changed_count: u32 = 0;
            for item in &changed_files {
                // Allows us to split the Vec to give to threads
                changed_files_vec.push(ModifiedList {
                    path: item.path.clone(),
                    exists: item.exists,
                    modified: item.modified,
                });
                changed_count += 1;
            }
            debug!("Inital run is false!");
            let real_thread_count = if changed_count >= thread_count {
                thread_count
            } else {
                1
            }; // Only do true multithreading if necessary
            debug!("real_thread_count is {real_thread_count}");
            diffs::create_diffs_multithread(
                &patch_ids,
                &ref_patch_ids,
                &target_paths,
                &modified,
                &folder_path,
                changed_files_vec,
                changed_count,
                real_thread_count,
                compression_level,
                &patch_store,
                false,
                false,
                &snapshot_mode,
            );
        }

        {
            // Create a new scope to unlock mutex
            debug!("Writing snapshot to store!");
            let patch_ids = patch_ids.lock().unwrap();
            let target_paths = target_paths.lock().unwrap();
            let ref_patch_ids = ref_patch_ids.lock().unwrap();
            let modified = modified.lock().unwrap();
            if patch_ids.len() > 0 {
                // println!("Writing snapshot to store!");
                let current_time: String = chrono::offset::Local::now().to_string();
                snapshot_store.push(SnapshotEntries {
                    date_created: current_time,
                    patch_ids: patch_ids.to_vec(),
                    target_path: target_paths.to_vec(),
                    ref_patch_ids: ref_patch_ids.to_vec(),
                    modified: modified.to_vec(),
                });

                let json = serde_json::to_string_pretty(&snapshot_store)
                    .expect("Unable to serialize metadata!");

                // Write the JSON string to a file
                let mut file = File::create(Path::new(&snapshot_store_file)).unwrap_or_else(|_| {
                    panic!("Unable to open snapshot file at {}", snapshot_store_file)
                });
                file.write_all(json.as_bytes()).unwrap_or_else(|_| {
                    panic!(
                        "Unable to write to metadata file at {}",
                        snapshot_store_file
                    )
                });
            }
        }

        // for meta in metadata_holder {
        //     println!("File Path: {}", meta.path);
        //     println!("File Hash: {}", meta.hash);
        //     println!("File Size: {} bytes", meta.size);
        //     println!("Last Modified Time: {} seconds since UNIX epoch", meta.date_modified);
        // }
        // Remove our tmp file we used
        fs::remove_file(folder_path.clone() + "/.time/tmp_empty")
            .expect("Unable to remove old tmp file");
    }
}
