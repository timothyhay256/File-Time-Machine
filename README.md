<div align="center">
   <img align="center" width="128px" src="logo.png" />
	<h1 align="center"><b>File Time Machine</b></h1>
	<p align="center">
		A snapshotting program as a standalone application
    <br />
  </p>

  [![Build Linux](https://github.com/timothyhay256/ftm/actions/workflows/build-linux.yml/badge.svg)](https://github.com/timothyhay256/ftm/actions/workflows/build-linux.yml)
  [![Build Windows](https://github.com/timothyhay256/ftm/actions/workflows/build-windows.yml/badge.svg)](https://github.com/timothyhay256/ftm/actions/workflows/build-windows.yml)
  [![.github/workflows/build-release.yml](https://github.com/timothyhay256/ftm/actions/workflows/build-release.yml/badge.svg)](https://github.com/timothyhay256/ftm/actions/workflows/build-release.yml)
  [![Codacy Badge](https://app.codacy.com/project/badge/Grade/afcd3d438c764d18b85299e4c3691262)](https://app.codacy.com?utm_source=gh&utm_medium=referral&utm_content=&utm_campaign=Badge_grade)
  ![No AI](https://img.shields.io/badge/free_of-AI_code-blue)
</div>  

> [!CAUTION]
> This program is NOT safe for regular usage, and will most likely result in data loss if used in such a way! This is my first Rust project, so it will be unstable!
> This program has been tested fairly well on Linux, but catastrophic bugs still may be present.
### What is this?
In order to start learning Rust, I decided to make a incremental snapshotting program, like Apples Time Machine, but in userspace and cross-platform. And so this is what this is. It allows you to take snapshots of folders, and restore these snapshots, allowing you to go backwards and forwards in time. So like Git, but easier to use, and less powerful. And with a messy codebase. And dangerous and data-loss prone. And slower. 
### Installation
#### Linux
Arch: Install `todo` from the AUR, or use Cargo. Optionally, install `todo-gui` as well.  
Others: Use cargo to install `file-time-machine` or download the binary from releases. To get the GUI, download it from the releases page and run it with Python.
#### Windows
Download the .msi file from the releases page, and run it. The program and gui will both be installed, and the gui can be launched from the start menu.
#### MacOS (UNTESTED!)
First of all, you already have time machine.  
But if you want it anyway, use cargo to install `file-time-machine`.
#### Making 
Clone/download the source code, and run the following commands:  
 - `cargo run --release` # if you just want to run the program/test it without installing it
 - `cargo install --path .` # if you want to install the program to ~/.cargo/bin
### Configuration
Create a configuration file inside `~/.file-time-machine/config.json` to automatically reference it when running `ftm` without any arguments, or create one at any path you want and pass it with `-c`, and add the following content:  
```
[
  {
    "folder_path": "/folder/path/you/want/to/snapshot",
    "get_hashes": false,
    "thread_count": 8,
    "brotli_compression_level": 5,
    "snapshot_mode": "fastest",
    "its_my_fault_if_i_lose_data": false
  }
]
```
`folder_path` is the folder path that you want to take and restore snapshots inside of.  
`get_hashes` is if you want to find modified files using hashes instead of a faster method such as change date/size. This is much slower.  
`thread_count` is how many threads you want to use. Set this to 0 to automatically select a thread count based on your CPU core count.  
`brotli_compression_level` is the compression level for snapshot files. As you go higher you will get better compression ratios, but much worse speeds. 5 seems to be a good level. Ranges from 1-11.  
`its_my_fault_if_i_lose_data` is you agreeing that it is YOUR fault if you lose data by using this software, and not mine. Set it to true to skip the 5 second warning on each run.  
`snapshot_mode` is the way to take snapshots. There are three modes, which are described in more detail below. *Currently ONLY fastest is supported! I might or might not add other modes later.* 
`standard` is the normal method. It takes as little disk space as possible, but takes much longer to take snapshots or move backwards in time. If your files are small, this time difference won't be noticable.  
`faster` is a mode that makes taking snapshots much faster, but results in increased disk space usage. This doesn't increase the speed of restoring backwards though. If you have the disk space and want the speed, this is a good option.  
`fastest` is a mode that makes both taking snapshots much faster, and makes restoring backwards much faster. It does however use nearly twice the disk space as previous modes.  
> [!WARNING]
> Once you select a snapshot mode, there is currently no way to switch to another one!

If you want to pass a specific config file (to snapshot a different path for example), simply use the `-c` flag.

### Usage
##### Note that .time (used for storing snapshots) and .git are ignored. The ability to specify directories to ignore will be added in the future.
#### GUI
If you are on Windows, launch File Time Machine. On Linux/MacOS, run the gui/gui.py script.  
Once it has started, ensure the square in the top right is green and says "Found FTM binary!". Operation of the GUI is fairly self explanatory, but here are some details about it's operation.  
**Select Folder**: Select the folder you want to create snapshots for. If the folder has been tracked, `folder_path/.time/gui-config.conf` will be checked for an config. If one is present, the program is ready for usage. If there is not one present, you will be prompted to select the config file location. If the folder has not been tracked, you will be prompted to start doing so. If you say yes, a simple config will be placed in `folder_path/.time/gui-config.conf`, and the program is ready for usage.  
**Select Config**: If a config could not be autodetected, then you will need to specify the location of one manually. On Unix systems, the default one (the one used when no options are passed) should be at `~/.file-time-machine/config.json`
**Create Snapshot**: Pretty self explanatory. Creates a snapshot. A valid folder and config file must be selected however.
**Restore Snapshot**: Restores a snapshot. One must be selected in the main box.
##### Issues
If you have any issues, you can check the console for further output. Additionally, the console will show the progress of creating a snapshot, while the GUI does not provide it. The console should open automatically on Windows.  
#### CLI
Once you have finished configuration, run `ftm` to collect the initial run of metadata. (Or if specifying a config file `ftm -c /path/to/config`, it will be the same)
On this run, a compressed copy of each file will be created, along with any other metafiles needed. These will be stored in `.time`.  
After this initial run, make some changes! You can create new files, delete old ones, and modify existing ones. Now run `ftm` again to create a snapshot. On this run, every file that has been changed will get a diff created between it, and the original file. This can be used to restore yourself to this state in time.      
Every time that you run `ftm` and changes have been detected, a new snapshot will be created.  
In order to restore a snapshot, first create one with `ftm` so you don't lose any working changes, then run `ftm restore`, and select the snapshot you wish to restore. Optionally, you can also use `ftm restore --restore-index n` to restore the nth snapshot. (Starting at 1 being oldest)  
You can safely make changes while a snapshot is restored, but they will be overwritten when a snapshot is restored. You can also safely create additional snapshots while one is restored.

In order to return to the present, run `ftm restore` and select the most recent snapshot.

### Notes
In the future, I want to make a daemon that tracks various folders and creates snapshots in defined increments of time.  
Until then, you can pass a config file to the binary in order to use those specific paths and settings. This means you can track multiple directories, you just have to have multiple config files.  

Since all snapshots and associated data is stored within the `.time` directory in the target directory, if you want to reset the timeline of snapshots, simply remove the folder. Just know that if you do so, ALL past snapshots and changes will be lost, and if you are currently in the "past" you will NOT be able to go back to the future!  

### How does it work
Please see (unfinished) for more details on how it actually works. Below is only for the unimplemented regular mode.
#### Regular mode 
Let our demo folder contain two files. `demo/test` and `demo/other`.  
We modify `demo/test`, and take a new snapshot, and we have two patch files:  
`.time/000` and `.time/000-reverse` (note that the ID is actually a hash from the date and path).  
`.time/000` is created from a empty file, and the new file. It is thus our compressed copy of the current version of the file. Using this on a empty file will yield the file in the state it was in when the snapshot was taken.
`.time/000-reverse` is a placebo, there is nothing inside it. This is because we would never want to go from our first version of the file, to nothing. When read by `restore.rs`, it will be ignored.  

Now we will modify `demo/test`, and then take another snapshot. This is where things get interesting. What we will now do, is load `.time/000-reverse` and `demo/test` to memory, and then attempt to apply `000-reverse` to `demo/test` and keep it in a new variable, lets say `ref`. But, remember that `000-reverse` is not a valid patch file (since we never want to go from a real file to a empty file), so as a reference we will need to use `000` and apply it to a empty "file", yielding the original file. So now `ref` is our original file. Now we take our `demo/test` we loaded to memory, and create two new patches; `001` which is made from `ref` as old and `demo/test` as new (allowing us to recover `demo/test` given `ref`), and `001-reverse` which is created in reverse, alllowing us to recover `ref` given `demo/test`.  

Now we will make one more modification to `demo/test`, and take just one more snapshot. This let's us explain what happens when our `-reverse` IS valid, which was not the case last time. All further snapshots will follow the formula of this specific snapshot.  

We want to make two patches once again, so we will load `.time/001-reverse` and `demo/test` to memory, and apply `001-reverse` to `test`. Since `001-reverse` IS valid this time, we will yield the version of the file right before the last snapshot, AKA the original file. So now `ref` is our original file. And again we take `demo/test` in memory and create two more patches, `002` from `ref` as old and `demo/test` as new (which again allows us to recover `demo/test` given `ref`) and `002-reverse` which recovers `ref` given `demo/test`.  

#### Restoring backwards
Ok, finally we can get to restoring a snapshot. At this point we have 3 snapshots, so let's try to restore our very first one.  

Once it is selected, we see that there is no `activeSnapshot` so we can assume we are in the past. We check the snapshots, and see that there are two snapshots to restore in order to reach our target snapshot, so we restore the second one we took.  

For our first snapshot to restore, the only changed file is `demo/test`, and it is associated with snapshot `002`. Since we are moving into the past, we want to recover `demo/test` at the time of the snapshot given `ref`, so we are going to use `002`. Now we take the patch entry and check the reference patch. It is `001-reverse`. So now we take `demo/test` and load it to memory, and apply `001-reverse`, giving us `ref`, which is identical to the `ref` we got while making that snapshot. Now we can apply `001` to `ref`, giving us our target state. We are now half way to our target snapshot state.  

For our second snapshot, once again the only changed file is `demo/test`, which is this time associated with snapshot `001`. We are again moving into the past, so we will want to recover `ref` from our first snapshot, and so we look at what our reference patch is. We see that it is `000-reverse`, which when read, is not a valid patch file. Since it is not, we will load `000` to memory, and apply it against a empty "file", yielding the target file. But wait- why did we even do that last thing if we could just have just done this, yielding the target file instantly? Because this is a special case where `000-reverse` was not valid. So that last step was not needed. But in a case where the initial state was not the target, we would still have needed that step, since all the patches at that point were created with that reference in mind.

#### Restoring forwards

Now lets restore our third snapshot, so we can return to our normal state.
We check the `activeSnapshot` and see that the target is in the future, and we will need to restore two snapshots to get there. Since we are restoring into the future, no references will be necessary, since the patch right in front of the current snapshot used our state as a reference. This means only one patch per patch, instead of two like when restoring backwards! But right before doing any restoring, we will need to check `000-reverse` to make sure it isn't a invalid patch. And what would you know, it is! What this means is that the final target snapshot actually does use our current file state as a ref, since it couldn't do it with the `-reverse` file. This saves us a step, and means we can go directly to the target!

Great, now lets go to the final, and target snapshot. We load `demo/test` to memory, check if `001-reverse` is valid, see that it is, and determine that we can safely directly apply the patch to the file, so we loa dup `002` to it, yielding our target file.  

Ok, but let's just go over a case where we do have another snapshot ahead, just for examples sake. Ok, so we have a snapshot `003` that has a reference of two snapshots ago, since we restored 


### .time structure
The .time folder contains all the information related to snapshots of the directory. Inside are 3 `json` files:  
 - `metadata.json` - This contains stored metadata for every file (date changed, file size, and optionally hash), and is used to detect changed files.
 - `patches.json` - Every time a patch is created, the ID (more on that below) and reference patch that was used will be stored here. And of course the target path. There is a layer of abstraction in `diffs.rs` that will handle this file.
 - `snapshots.json` - Every time a snapshot is created, every patch that was created and its target path is stored in here.  

 Whenever a patch of a file is created, two files will be created. They will be named `ID` and `ID-reverse`. The way the `ID` is generated is by taking the current date and target path, and creating a SHA256 hash from them. This way every patch will have a unique path within `.time` and the path can be easily generated from the `patches.json` file. The way the actual patch is generated is by creating a "patch" from the old (usually a reference in memory) and new (current file), and compressing it with brotli. The `reverse` patch is created in the opposite direction.   

 `ID` is just a diff between the old file (which can either be a empty file on the first snapshot or a reference patched file), and `ID-reverse` is just a diff between the new file and old file, allowing us to travel in reverse (since patches are not reversible with `bsdiff`.)  

 When we restore a snapshot, we want to check if the snapshot is in the past (relative to the current "state/date"), so we store this in `.time/activeSnapshot`. And if none exists, we can safely assume the most recent snapshot is the current state. Otherwise, everytime a snapshot is restored, we write the snapshot date to this file.  

### Modes explanation

 #### Standard
 When a snapshot is created, we will restore upwards from the initial patch, and then create only a forward snapshot. This means only one patch is needed per patch. This however also means we can't truly move backwards into the past, we have to restore upwards from the initial snapshot until we reach our target.  
 #### Faster and bigger
 This is the same as the fastest and biggest approach (see below), except for one thing: The reference is always just the initial stored copy of the file. This means creating snapshots is much much faster, but it also means we don't get any potential reduced disk usage due to deduplication.  
 #### Fastest and biggest
 This is the same as the broken approach, except that to generate a reference, we will need to restore up to the most recent version, and use that. Then, we create two patches like before. This means that going forward is faster, but much more storage is required.  

### Notes
You can place any files you want to inside `demo.bak`, and then run `test.sh`. Just don't remove `config.json` or the test script will break.  
Multiple snapshots will be made with various folders that already exist within the repository, and then restoring each of those snapshots will be tested for accuracy. All files will be checksummed as a way to ensure the program is working properly.

### TODO
Hashing: Use xxhash for file hashing since it is so bloody fast. Currently used to verify existing files.  
Optionally change .time location.
Be able to ignore directories, like a .gitignore
