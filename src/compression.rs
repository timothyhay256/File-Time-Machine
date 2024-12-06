use brotli::{CompressorWriter, Decompressor};
use std::io;
use std::io::Read;
use std::io::Write;

pub fn compress_data(input: Vec<u8>, compression_level: u32) -> io::Result<Vec<u8>> {
    // Create a Vec<u8> to hold the compressed data
    let mut compressed_data = Vec::new();
    {
        let mut compressor =
            CompressorWriter::new(&mut compressed_data, 4096, compression_level, 22);
        compressor.write_all(&input)?;
        compressor.flush()?;
    } // The compressor goes out of scope here, and its resources are released.

    Ok(compressed_data)
}

pub fn decompress_data(compressed: Vec<u8>) -> io::Result<Vec<u8>> {
    let mut decompressed_data = Vec::new();
    {
        let mut decompressor = Decompressor::new(&compressed[..], 4096);
        decompressor.read_to_end(&mut decompressed_data)?;
    }
    Ok(decompressed_data)
}
