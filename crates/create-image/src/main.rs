use std::{env, path::PathBuf, process};

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
            eprintln!("Failed to create UEFI disk image: {}", e);
            process::exit(1);
        });

    println!("Created UEFI disk image: {}", output.display());
}
