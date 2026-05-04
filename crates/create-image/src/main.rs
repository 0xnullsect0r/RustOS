use std::{env, fs, io::{Seek, SeekFrom, Write, Read}, path::PathBuf, process};
use gpt::GptConfig;
use uuid::Uuid;

const FAT32_PARTITION_SIZE: u64 = 512 * 1024 * 1024; // 512 MB
const SECTOR_SIZE: u64 = 512;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: create-image <kernel-elf> <output-img>");
        process::exit(1);
    }

    let kernel = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);

    if !kernel.exists() {
        eprintln!("Error: kernel ELF not found: {}", kernel.display());
        process::exit(1);
    }

    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&output)
        .unwrap_or_else(|e| {
            eprintln!(
                "Failed to create UEFI disk image from '{}' to '{}': {}",
                kernel.display(),
                output.display(),
                e
            );
            process::exit(1);
        });

    println!("Created UEFI disk image: {}", output.display());

    // Add FAT32 partition for storage testing on QEMU
    if let Err(e) = add_fat32_partition(&output) {
        eprintln!("Warning: could not add FAT32 partition: {}", e);
        eprintln!("The kernel will use RamFS as root instead of persistent FAT32 storage.");
    } else {
        println!("Added FAT32 storage partition to disk image for Phase 1 testing");
    }
}

fn add_fat32_partition(img_path: &PathBuf) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(img_path)?;

    // Get the current image size
    let current_size = file.seek(SeekFrom::End(0))?;
    let new_size = current_size + FAT32_PARTITION_SIZE;

    // Extend the image with zeros
    eprintln!("[create-image] Extending disk from {} MB to {} MB...",
        current_size / 1024 / 1024,
        new_size / 1024 / 1024);
    
    file.seek(SeekFrom::End(0))?;
    let mut zeros = vec![0u8; 1024 * 1024];
    let mut written = current_size;
    while written < new_size {
        let to_write = std::cmp::min(zeros.len() as u64, new_size - written) as usize;
        file.write_all(&zeros[..to_write])?;
        written += to_write as u64;
    }
    file.sync_all()?;

    // Now use the gpt crate to add the partition entry
    eprintln!("[create-image] Updating GPT table...");
    
    let mut disk = GptConfig::new()
        .writable(true)
        .open(img_path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("GPT error: {}", e)))?;

    // Calculate partition location
    let part_start_sector = (current_size / SECTOR_SIZE);
    let part_end_sector = (new_size / SECTOR_SIZE) - 1;

    // Use Microsoft Basic Data GUID for FAT32 (C12A7328-F81F-11D2-BA4B-00A0C93EC93B is EFI, we want 07000000...)
    let part_type = Uuid::parse_str("ebd0a0a2-b9e5-4433-a802-60a0d38f6d5f").unwrap(); // Microsoft Basic Data

    eprintln!("[create-image] Adding partition 2: sectors {} to {}", part_start_sector, part_end_sector);

    let partition = gpt::partition_types::LINUX_FS;
    disk.add_partition("rustos-storage", part_start_sector, part_end_sector, partition)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to add partition: {}", e)))?;

    disk.write()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to write GPT: {}", e)))?;

    // Write minimal FAT32 boot sector
    eprintln!("[create-image] Initializing FAT32 filesystem...");
    
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(img_path)?;
    
    file.seek(SeekFrom::Start(current_size))?;
    
    let mut bpb = vec![0u8; 512];
    bpb[0] = 0xEB; // JMP instruction
    bpb[1] = 0x3C;
    bpb[2] = 0x90;
    
    // Bytes per sector = 512
    bpb[11] = 0x00;
    bpb[12] = 0x02;
    
    // Sectors per cluster = 8
    bpb[13] = 8;
    
    // Reserved sectors = 32
    bpb[14] = 0x20;
    
    // Number of FATs = 2
    bpb[16] = 2;
    
    // Media descriptor
    bpb[21] = 0xF8;
    
    // Sectors per track = 63
    bpb[24] = 0x3F;
    
    // Heads = 255
    bpb[26] = 0xFF;
    
    // Total sectors for this partition
    let total_sectors = FAT32_PARTITION_SIZE / 512;
    bpb[32] = ((total_sectors & 0xFF) as u8);
    bpb[33] = (((total_sectors >> 8) & 0xFF) as u8);
    bpb[34] = (((total_sectors >> 16) & 0xFF) as u8);
    bpb[35] = (((total_sectors >> 24) & 0xFF) as u8);
    
    // FAT size in sectors
    let fat_size = 0x1000u32;
    bpb[36] = ((fat_size & 0xFF) as u8);
    bpb[37] = (((fat_size >> 8) & 0xFF) as u8);
    bpb[38] = (((fat_size >> 16) & 0xFF) as u8);
    bpb[39] = (((fat_size >> 24) & 0xFF) as u8);
    
    // Root cluster = 2
    bpb[44] = 2;
    
    // FS info sector = 1
    bpb[48] = 1;
    
    // Boot signature
    bpb[510] = 0x55;
    bpb[511] = 0xAA;
    
    file.write_all(&bpb)?;
    file.sync_all()?;

    Ok(())
}



