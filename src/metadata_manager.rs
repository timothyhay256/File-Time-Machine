use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;

use crate::MetaFile;

pub fn write_metadata_to_file(metadata_holder: &HashSet<MetaFile>, filename: &str) {
    // Serialize the vector to a JSON string
    let json =
        serde_json::to_string_pretty(metadata_holder).expect("Unable to serialize metadata!");

    // Write the JSON string to a file
    let mut file = File::create(Path::new(filename))
        .unwrap_or_else(|_| panic!("Unable to create metadata file at {filename}"));
    file.write_all(json.as_bytes())
        .unwrap_or_else(|_| panic!("Unable to write to metadata file at {filename}"));
}

pub fn read_metadata_from_file(filename: &str) -> Result<HashSet<MetaFile>, Box<dyn Error>> {
    // Load file to string, and use serde to turn it into Vec<MetaFile>
    let mut file = File::open(Path::new(filename))?;

    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents)?;

    let metadata_holder: HashSet<MetaFile> = serde_json::from_str(&file_contents)?;

    Ok(metadata_holder)
}
