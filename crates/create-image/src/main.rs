use std::{
    env, fs,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    process,
};
use gpt::GptConfig;

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
    let zeros = vec![0u8; 1024 * 1024];
    let mut written = current_size;
    while written < new_size {
        let to_write = std::cmp::min(zeros.len() as u64, new_size - written) as usize;
        file.write_all(&zeros[..to_write])?;
        written += to_write as u64;
    }
    file.sync_all()?;
    
    // Use the gpt crate with proper configuration for the extended disk
    eprintln!("[create-image] Updating GPT table...");
    
    let mut disk = GptConfig::new()
        .writable(true)
        .open(img_path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("GPT error on initial open: {}", e)))?;

    let existing_partitions = disk.partitions().clone();
    disk.update_partitions(existing_partitions).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to refresh GPT headers after resize: {}", e),
        )
    })?;

    let partition = gpt::partition_types::LINUX_FS;
    let partition_id = disk
        .add_partition("rustos-storage", FAT32_PARTITION_SIZE, partition, 0, None)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to add partition: {}", e)))?;

    let part = disk
        .partitions()
        .get(&partition_id)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to locate newly created partition {}", partition_id),
            )
        })?;
    let part_start_sector = part.first_lba;
    let part_end_sector = part.last_lba;
    let total_sectors = new_size / SECTOR_SIZE;

    eprintln!("[create-image] Disk: {} sectors total", total_sectors);
    eprintln!(
        "[create-image] Partition 2: sectors {} to {}",
        part_start_sector, part_end_sector
    );

    disk.write()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to write GPT: {}", e)))?;

    // Write minimal FAT32 boot sector
    eprintln!("[create-image] Initializing FAT32 filesystem...");
    
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(img_path)?;
    
    let fat32_start = current_size;
    file.seek(SeekFrom::Start(fat32_start))?;
    
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
    let total_part_sectors = part_end_sector - part_start_sector + 1;
    bpb[32] = (total_part_sectors & 0xFF) as u8;
    bpb[33] = ((total_part_sectors >> 8) & 0xFF) as u8;
    bpb[34] = ((total_part_sectors >> 16) & 0xFF) as u8;
    bpb[35] = ((total_part_sectors >> 24) & 0xFF) as u8;
    
    // FAT size in sectors
    let fat_size = 0x1000u32;
    bpb[36] = (fat_size & 0xFF) as u8;
    bpb[37] = ((fat_size >> 8) & 0xFF) as u8;
    bpb[38] = ((fat_size >> 16) & 0xFF) as u8;
    bpb[39] = ((fat_size >> 24) & 0xFF) as u8;
    
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


