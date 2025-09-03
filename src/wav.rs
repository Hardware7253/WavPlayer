
use crate::riff;
use crate::block_device;
use crate::exfat;
use crate::bytes::BytesTrait;
use exfat::{FsEntry, ExFat};

use crate::BLOCK_SIZE;
const BUFFER_BLOCKS: usize = 100; // How many blocks to read when buffering samples

use crate::{rprint, rprintln};

use heapless::Vec;


#[derive(Debug)]
pub enum Format {
    Pcm,
    IeeeFloat,
    Alaw,
    Mulaw,
    Other,
}

impl Format {
    fn decode_format(format_code: u16) -> Format {
        match format_code {
            0x0001 => return Format::Pcm,
            0x0003 => return Format::IeeeFloat,
            0x0006 => return Format::Alaw,
            0x0007 => return Format::Alaw,
            _      => return Format::Other,
        }
    }
}

#[derive(Debug)]
pub struct WavFile {
    start_block_address: u32,
    pub data_length: u32, // Length of the wav data chunk in bytes
    first_byte: u32, // The byte address of the first byte from the data chunk
    pub bytes_read: u32, // Number of bytes of wav data that have been read

    pub format: Format,
    pub n_channels: u16,
    pub sample_rate: u32, // Samples per second
    pub byte_rate: u32, // Bytes per second (SampleRate * NumChannels * BitsPerSample/8)
    pub block_align: u16, // The number of bytes for one sample (including all channels)
    pub bits_per_sample: u16, // Audio bit dipth
    pub bytes_per_channel: u16
}

impl WavFile {

    // Create a new wav file with it's format information
    pub fn new<T: block_device::BlockDevice<BLOCK_SIZE>>(exfat: &mut ExFat<T>, file: &FsEntry) -> Result<Self, ()> {
        let start_block_address: u32 = exfat.calc_cluster_sector(file.first_cluster);

        let mut wav_file = WavFile {
            start_block_address,
            data_length: 0,
            first_byte: 0,
            bytes_read: 0,
            format: Format::Other,
            n_channels: 0,
            sample_rate: 0,
            byte_rate: 0,
            block_align: 0,
            bits_per_sample: 0,
            bytes_per_channel: 0,
        };

        // Loop through chunks until we find the fmt chunk and data chunk to complete a WavFile struct
        let mut current_chunk = riff::get_first_chunk(start_block_address, &mut exfat.block_device)?;
        let mut found_format_chunk = false;
        let mut found_data_chunk = false;


        for _ in 0..10 {

            if current_chunk.identifier == "fmt " {
                found_format_chunk = true;

                // Assume information from the format chunk is all contained in the first block
                // It should be for a proper wav file since the format chunk should only be offset 12 bytes from the start
                let first_block = exfat.block_device.read_block(start_block_address)?;
                let chunk_start = current_chunk.chunk_start as usize;

                let format_code = u16::from_le_bytes(first_block.get_bytes_section::<2>(chunk_start + 8));
                let n_channels = u16::from_le_bytes(first_block.get_bytes_section::<2>(chunk_start + 10));
                let sample_rate = u32::from_le_bytes(first_block.get_bytes_section::<4>(chunk_start + 12));
                let byte_rate = u32::from_le_bytes(first_block.get_bytes_section::<4>(chunk_start + 16));
                let block_align = u16::from_le_bytes(first_block.get_bytes_section::<2>(chunk_start + 20));
                let bits_per_sample = u16::from_le_bytes(first_block.get_bytes_section::<2>(chunk_start + 22));

                let bytes_per_channel = block_align / n_channels;

                let format = Format::decode_format(format_code);

                wav_file.format = format; wav_file.n_channels = n_channels;
                wav_file.sample_rate = sample_rate;
                wav_file.byte_rate = byte_rate;
                wav_file.block_align = block_align;
                wav_file.bits_per_sample = bits_per_sample;
                wav_file.bytes_per_channel = bytes_per_channel;
            } else if current_chunk.identifier == "data" {
                found_data_chunk = true;
                wav_file.first_byte = current_chunk.chunk_start as u32 + 8;
                wav_file.data_length = current_chunk.length;
                break;
            }

            // Update current chunk with the next chunk
            current_chunk = current_chunk.get_next_chunk(&mut exfat.block_device, start_block_address)?;
        } 

        if found_data_chunk && found_format_chunk {
            return Ok(wav_file)
        }

        Err(())
    }

    // Get the next block from the wav file
    pub fn get_next_pcm_block<'a, T: block_device::BlockDevice<BLOCK_SIZE>>
        (&mut self, exfat: &mut ExFat<T>, buf: &mut [u8; BLOCK_SIZE])
    -> Result<(), ()> {

        // Ignore the first couple of samples because they aren't alligned to a block
        if self.bytes_read == 0 {
            self.bytes_read += BLOCK_SIZE as u32 - self.first_byte;
            return self.get_next_pcm_block(exfat, buf);
        }

        // Similairly ignore the last couple of samples
        let new_bytes_read = self.bytes_read + BLOCK_SIZE as u32;
        if new_bytes_read >= self.data_length {
            return Err(());
        }

        // Otherwise get the block address and return the block
        let blockaddr = self.start_block_address + ((self.first_byte as u32 + self.bytes_read) / BLOCK_SIZE as u32);
        exfat.block_device.read_to_block(blockaddr, buf)?;
        self.bytes_read = new_bytes_read;
        Ok(())
    }

    // Fills the sample_vec buffer and returns an iterator over that buffer that converts the bytes into usable PCM samples
    // Not very useful for DMA 
    pub fn get_next_samples<'a, T: block_device::BlockDevice<BLOCK_SIZE>>
        (&mut self, exfat: &mut ExFat<T>, sample_vec: &'a mut Vec<u8, {BUFFER_BLOCKS * BLOCK_SIZE}>)
    -> Result<impl Iterator<Item = i32> + 'a, ()> {

        // Limits of the implementation
        if self.bytes_per_channel > 4 {
            panic!();
        }

        *sample_vec = Vec::new(); // Clear sample vec before starting so the old samples aren't reused

        let blockaddr = self.start_block_address + ((self.first_byte as u32 + self.bytes_read) / 512);

        // Bytes to skip off the front
        let skip_bytes: u32 = if self.bytes_read == 0 {
            ((self.first_byte) % BLOCK_SIZE as u32) as u32
        } else {
            0
        };

        let mut bytes_read = 0; // The total bytes read during this function
        for i in 0..BUFFER_BLOCKS as u32 {
            let block = exfat.block_device.read_block(blockaddr + i)?;
            let _ = sample_vec.extend_from_slice(&block);

            // Bytes read now is the number of bytes read past the start of the pcm data, or past the start of the block
            // (during this iteration of the for loop)
            // This if statement initialises the variable and removes any bytes of non pcm data from the front
            // Non pcm data will only be contained at the front for the first block, which contains other RIFF chunks at the start
            let bytes_read_now = if i == 0 {
                BLOCK_SIZE as u32 - skip_bytes
            } else {
               BLOCK_SIZE as u32 
            };


            // This if statement catches the end of the pcm data
            // It will break from the for loop once all the pcm data has been added to the vec
            // and will update the bytes_read accordingly 
            let bytes_left = self.data_length - self.bytes_read;
            if bytes_read_now > bytes_left {
                bytes_read += bytes_left;
                break;
            }

            bytes_read += bytes_read_now
        }

        self.bytes_read += bytes_read;

        // This sample iter contains only the bytes which are PCM data,and ecludes other RIFF bytes
        let mut sample_iter = sample_vec.iter().skip(skip_bytes as usize).take(bytes_read as usize);

        // How many bits to shift the sample left so it is left aligned in a 32 bit number
        let shift_places = 32 - self.bits_per_sample; 

        // This sample iter collects all the bytes that comprise a channel into a single i32 number
        let bytes_per_channel = self.bytes_per_channel;
        let samples = core::iter::from_fn(move || {
            let mut bytes = [0u8; 4];

            for i in 0..4 {
                if i == bytes_per_channel as usize {
                    break;
                }

                if let Some(byte) = sample_iter.next() {
                    bytes[i] = *byte;
                } else {
                    return None
                }
            }

            let channel_value = u32::from_le_bytes(bytes) << shift_places;

            Some(channel_value as i32)
        });

        Ok(samples)
    }
}
