// Useful exfat resources:
// https://wiki.osdev.org/ExFAT
// https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification
// https://elm-chan.org/docs/exfat_e.html


use crate::rprintln;

use core::char::decode_utf16;
use heapless::{String, Vec};

use crate::binary_helpers;
use crate::block_device::BlockDevice;
use crate::bytes::{Bytes, BytesTrait};


// Assume the sector size and block size are the same
const SECTOR_SIZE: usize = crate::BLOCK_SIZE;
pub struct ExFat<T: BlockDevice<SECTOR_SIZE>> {
    pub block_device: T,

    // Volume parameters extracted from the boot sector
    pub partition_offset: u64,                 // LBA offset where the exFAT volume begins
    pub volume_length: u64,                    // Total number of sectors in the volume
    pub fat_offset: u32,                       // Sector offset to the File Allocation Table (FAT)
    pub fat_length: u32,                       //edLength of the FAT in sectors
    pub cluster_heap_offset: u32,              // Sector offset to the start of the Cluster Heap
    pub cluster_count: u32,                    // Total number of clusters
    pub first_cluster_of_root_directory: u32,  // Cluster index of the root directory
    pub volume_serial_number: u32,             // Unique volume ID
    pub volume_flags: u16,                     // Flags related to allocation and FAT mirroring
    pub bytes_per_sector_shift: u8,            // Shift value to calculate bytes per sector (e.g., 9 means 512 bytes)
    pub sectors_per_cluster_shift: u8,         // Shift value to calculate sectors per cluster
    pub number_of_fats: u8,                    // Number of FATs (typically 1)
    pub drive_select: u8,                      // Usually 0x80 for fixed drives
    pub percent_in_use: u8,                    // Approximate % of clusters in use
}


// Constants defined here are for good practice and readability
// But they're not really meant to be changed since many of these values are fixed as part of the exFAT spec
const DIRECORY_ENTRY_BYTES: usize = 32; // How many bytes in a directory entry
const DIRECTORY_ENTRIES_PER_SECTOR: usize = SECTOR_SIZE / DIRECORY_ENTRY_BYTES;

// Directory entry types
const ALLOCATION_BITMAP_ENTRY: u8 = 0x81;
const UPCASE_TABLE_ENTRY: u8 = 0x82;
const VOLUME_LABEL_ENTRY: u8 = 0x83;
const FILE_DIRECTORY_ENTRY: u8 = 0x85;
const STREAM_EXTENSION_ENTRY: u8 = 0xC0;
const FILE_NAME_ENTRY: u8 = 0xC1;

#[derive(Debug, Clone, Copy)]
pub enum FileType {
    Directory,
    File
}

// Impose an arbitrary limit on how long a directory can be
// Choosing a longer value here will use more memory when reading a directory
const DIR_LENGTH_LIMIT: usize = 205; 

const MAX_FILE_NAME_LENGTH: usize = 255; // exFAT limitation

// A filesystem entry is a struct that contains information about either a file or a folder
#[derive(Debug)]
pub struct FsEntry {
    pub name: String<MAX_FILE_NAME_LENGTH>,
    pub file_type: FileType, 

    pub first_cluster: u32,     // The first cluster in the files cluster chain 
    pub valid_data_length: u64, // Actual length of the file in bytes
    pub data_length: u64,       // Total size of the file in bytes
}


impl FsEntry {
    fn new() -> Self {
        FsEntry { 
            name: String::new(), 
            file_type: FileType::Directory,
            first_cluster: 0, 
            valid_data_length: 0, 
            data_length: 0, 
        }
    }
}


// Where to check for exfat boot signature
const TRY_BOOT_SECOTRS: [u32; 4] = [0, 65536, 32768, 2048];

// EXFAT filesystem name, should be present at the start of the boot sector
const FILESYSTEM_NAME: Bytes<8> = [0x45, 0x58, 0x46, 0x41, 0x54, 0x20, 0x20, 0x20];  
const BOOT_SIGNATURE: Bytes<2> = [0x55, 0xaa];

#[derive(Debug)]
pub enum FsError {
    // Failed to find the exfat boot sector
    // This might happen if the boot sector is at an unusual location
    // Or the device isn't exfat
    NoBootSector, 

    InvalidBootSignature, // The boot singature is incorrect or missing
    ReadFail, // The device failed a read during init

    ErrorDecodingName, // Error decoding the file / folder name
}

// Finds the boot sector of the block device by searching for the exfat filesystem name
// Returns the boot sector 
fn get_boot_sector<T: BlockDevice<SECTOR_SIZE>>(block_device: &mut T) -> Result<Bytes<SECTOR_SIZE>, FsError> {
    for boot_sector in TRY_BOOT_SECOTRS {

        if let Ok(sector) = block_device.read_block(boot_sector) {

            // Read the filesystem name starting from the 3rd byte
            let filesystem_name = sector.get_bytes_section::<8>(0x003);

            if filesystem_name == FILESYSTEM_NAME {

                // Check the boot signature is present
                let boot_signature = sector.get_bytes_section::<2>(0x1fe);

                if boot_signature == BOOT_SIGNATURE {
                    return Ok(sector)
                } else {
                    return Err(FsError::InvalidBootSignature)
                }
            }
        } else {
            return Err(FsError::ReadFail);
        }
   }

    Err(FsError::NoBootSector)
}

impl<T: BlockDevice<SECTOR_SIZE>> ExFat<T> {
    pub fn new(mut block_device: T) -> Result<Self, FsError> {
        let boot_sector = get_boot_sector(&mut block_device)?;

        // Retrieve all the useful information encoded in the boot sector
        let partition_offset = u64::from_le_bytes(boot_sector.get_bytes_section::<8>(0x040));
        let volume_length = u64::from_le_bytes(boot_sector.get_bytes_section::<8>(0x048));
        let fat_offset = u32::from_le_bytes(boot_sector.get_bytes_section::<4>(0x050));
        let fat_length = u32::from_le_bytes(boot_sector.get_bytes_section::<4>(0x054));
        let cluster_heap_offset = u32::from_le_bytes(boot_sector.get_bytes_section::<4>(0x058));
        let cluster_count = u32::from_le_bytes(boot_sector.get_bytes_section::<4>(0x05C));
        let first_cluster_of_root_directory = u32::from_le_bytes(boot_sector.get_bytes_section::<4>(0x060));
        let volume_serial_number = u32::from_le_bytes(boot_sector.get_bytes_section::<4>(0x064));
        let volume_flags = u16::from_le_bytes(boot_sector.get_bytes_section::<2>(0x06a));
        let bytes_per_sector_shift = u8::from_le_bytes(boot_sector.get_bytes_section::<1>(0x06c));
        let sectors_per_cluster_shift = u8::from_le_bytes(boot_sector.get_bytes_section::<1>(0x06d));
        let number_of_fats = u8::from_le_bytes(boot_sector.get_bytes_section::<1>(0x06e));
        let drive_select = u8::from_le_bytes(boot_sector.get_bytes_section::<1>(0x06f));
        let percent_in_use = u8::from_le_bytes(boot_sector.get_bytes_section::<1>(0x070));

        assert_eq!(1 << bytes_per_sector_shift, SECTOR_SIZE);
        
        Ok(ExFat{
            block_device,
            partition_offset,
            volume_length,
            fat_offset,
            fat_length,
            cluster_heap_offset,
            cluster_count,
            first_cluster_of_root_directory,
            volume_serial_number,
            volume_flags,
            bytes_per_sector_shift,
            sectors_per_cluster_shift,
            number_of_fats,
            drive_select,
            percent_in_use,
        })
    }

    // Read a sector from the block device, except now the error type is a FsError
    pub fn read_sector(&mut self, sector_addr: u32)
    -> Result<Bytes<SECTOR_SIZE>, FsError> {
        let result = self.block_device.read_block(sector_addr);

        match result {
            Ok(sector) => return Ok(sector),
            Err(()) => return Err(FsError::ReadFail)
        }
    }

    // Converts the start of a cluster to a sector address 
    pub fn calc_cluster_sector(&self, cluster: u32) -> u32 {
        self.partition_offset as u32 + self.cluster_heap_offset +
            (cluster - 2) * (1 << self.sectors_per_cluster_shift as u32)
    }


    // Lists the directory that starts at first_cluster
    // The root directory starts at cluster 4
    pub fn list_directory(&mut self, first_cluster: u32) -> Result<Vec<FsEntry, DIR_LENGTH_LIMIT>, FsError> {
        let mut output_directory = Vec::new();

        let mut found_all_entries = false;
        let mut sector_offset = 0;
        let sector_addr = self.calc_cluster_sector(first_cluster);

        while !found_all_entries {
            let sector_addr = sector_offset + sector_addr;
            sector_offset += 1;

            let sector = self.read_sector(sector_addr)?;
            let dir_entries = sector.slice_by::<{DIRECTORY_ENTRIES_PER_SECTOR}, {DIRECORY_ENTRY_BYTES}>();

            for (entry_no, entry_bytes) in dir_entries.iter().enumerate() {
                let entry_type = entry_bytes[0];

                if entry_type == 0 {
                    found_all_entries = true;
                    break;
                }

                // The beginning of a filesystem entry
                // Contains file/folder metadata
                else if entry_type == FILE_DIRECTORY_ENTRY {
                    // How many entries follow the starting entry for this FsEntry
                    let following_entries_no = entry_bytes[1] as usize;

                    // Determine if this FsEntry is for a file or a directory
                    let file_attribute = u16::from_le_bytes(entry_bytes.get_bytes_section::<2>(4));
                    let file_type = if binary_helpers::bit_on(file_attribute as u64, 4) {
                        FileType::Directory
                    } else {
                        FileType::File
                    };

                    let mut fs_entry = FsEntry::new();
                    fs_entry.file_type = file_type;

                    // Add the entries from the next sector to the current directory entries iterator
                    // Do this to account for cases where a FsEntry has entries which lie on the boundary between two sectors
                    let next_sector_entries = self.read_sector(sector_addr + 1)?
                        .slice_by::<{DIRECTORY_ENTRIES_PER_SECTOR}, {DIRECORY_ENTRY_BYTES}>();

                    let dir_entries_iter = dir_entries.iter().chain(next_sector_entries.iter());

                    // Finally, get an iterator over the next directory entries which are associated with the current one
                    let following_entries_iter = dir_entries_iter.skip(entry_no + 1).take(following_entries_no + 1);
                    for entry_bytes in following_entries_iter {
                        let entry_type = entry_bytes[0];

                        // Add useful stream extension information to the fs_entry
                        if entry_type == STREAM_EXTENSION_ENTRY {
                            fs_entry.valid_data_length = u64::from_le_bytes(entry_bytes.get_bytes_section::<8>(8));
                            fs_entry.data_length = u64::from_le_bytes(entry_bytes.get_bytes_section::<8>(24));
                            fs_entry.first_cluster = u32::from_le_bytes(entry_bytes.get_bytes_section::<4>(20));
                        }

                        // Decode file name entries
                        else if entry_type == FILE_NAME_ENTRY {

                            let utf_16_bytes = entry_bytes.slice_by::<{DIRECORY_ENTRY_BYTES / 2}, 2>();
                            let utf_iterator = utf_16_bytes.iter().skip(1) // Skip first byte which is the entry type
                            .map(|x| {
                                u16::from_le_bytes(*x) // Convert two bytes into one u16 number
                            }).filter(|&x| x != 0); // Assume null characters shouldn't be included

                            // Then iterate over each character and push them to the current file name
                            for char_result in decode_utf16(utf_iterator) {
                                match char_result {
                                    Ok(character) => {
                                        let result = fs_entry.name.push(character);
                                        match result {
                                            Ok(()) => (),
                                            Err(_err) => return Err(FsError::ErrorDecodingName)
                                        }
                                    },
                                    Err(_err) => return Err(FsError::ErrorDecodingName)
                                }
                            }
                        } // File name decoing end

                    } 

                    // All the following directory entries have now been read so the fs_entry has all it's information
                    // The fs_entry can then be pushed to the output
                    let _ = output_directory.push(fs_entry);
                } // FILE_DIRECTOR_ENTRY section end

            } // loop 2 end
        } // loop 1 end
        
        Ok(output_directory)
    }

}