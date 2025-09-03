use crate::rprintln;

use heapless::String;

use crate::block_device::BlockDevice;
use crate::bytes::*;


use crate::BLOCK_SIZE;

// Chunk info necessary for reading chunks sequentially
#[derive(Debug)]
pub struct ChunkInfo {
    pub identifier: String<4>, // 4 Character chunk identifier
    pub length: u32, // Length of the chunk in bytes
    
    // Byte addressing starts from the first block of the riff file and can extend beyond the length of one block 
    // So a chunk that starts at byte 16 of the second block in the file would have byte address 512 + 16 = 528
    pub chunk_start: u64, // Byte address of this chunk
    pub next_chunk: u64, // Byte address of the next chunk
}

impl ChunkInfo {

    // Get the next chunk after the current chunk
    // This will break for files with many chunks, as it doesn't account for chunks whose headers cross a block boundary
    // Leaving for now as it will work okay for wav
    pub fn get_next_chunk<T: BlockDevice<{BLOCK_SIZE}>>(&self, block_device: &mut T, start_block_address: u32) -> Result<ChunkInfo, ()> {

        // Get the correct block to read the next chunk from
        let offset_blocks = self.next_chunk / BLOCK_SIZE as u64;
        let relevant_block_addr = start_block_address + offset_blocks as u32;

        let relevant_block = block_device.read_block(relevant_block_addr)?;
        let next_chunk_in_block = self.next_chunk - offset_blocks * BLOCK_SIZE as u64;




        let identifier = relevant_block.decode_ascii::<4>(next_chunk_in_block as usize);
        let length = u32::from_le_bytes(relevant_block.get_bytes_section::<4>(next_chunk_in_block as usize + 4));

        let identifier_str = identifier.as_str();
        let new_next_chunk = if identifier_str == "RIFF" || identifier_str == "LIST" {
            self.next_chunk + 8
        } else if identifier_str == "WAVE" {
           self.next_chunk + 4 
        } else {
            self.next_chunk + 8 + length as u64 
        };

        let next_chunk_info = ChunkInfo {
            identifier,
            length,
            chunk_start: self.next_chunk,
            next_chunk: new_next_chunk,
        };
        
        Ok(next_chunk_info)
    }  
}

// Get the first chunk in the file
pub fn get_first_chunk<T: BlockDevice<BLOCK_SIZE>>(start_block_address: u32, block_device: &mut T) -> Result<ChunkInfo, ()> {
    let start_chunk = ChunkInfo {
        identifier: String::new(),
        length: 0,
        chunk_start: 0,
        next_chunk: 0,
    };

    start_chunk.get_next_chunk(block_device, start_block_address)
}