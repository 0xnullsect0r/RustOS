# RustOS Architecture

**Version**: 1.0  
**Last Updated**: 2026-05-04

This document provides a comprehensive overview of the RustOS kernel architecture, design decisions, and implementation details.

---

## Table of Contents

1. [Overview](#overview)
2. [Boot Process](#boot-process)
3. [Memory Management](#memory-management)
4. [Process Model](#process-model)
5. [Filesystem Architecture](#filesystem-architecture)
6. [Device Driver Model](#device-driver-model)
7. [Network Stack](#network-stack)
8. [Inter-Component Communication](#inter-component-communication)
9. [Design Decisions](#design-decisions)
10. [Future Directions](#future-directions)

---

## Overview

RustOS is a minimal x86_64 operating system kernel written in Rust. It demonstrates modern OS development practices using Rust's safety guarantees while maintaining bare-metal performance.

### Key Characteristics

- **Monolithic Kernel**: All drivers and subsystems run in kernel space
- **Cooperative Multitasking**: Single-threaded async executor (no preemption yet)
- **No Standard Library**: Runs in `no_std` environment
- **UEFI-Compatible**: Boots via modern UEFI firmware or legacy BIOS/CSM
- **Modular Design**: Clear separation between subsystems

### System Components

```
┌─────────────────────────────────────────────────────────┐
│                   Application Layer                      │
│         (Future: userspace processes via ELF)           │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────┴────────────────────────────────────┐
│                    System Call Interface                 │
│                  (int 0x80, syscalls 0-310)             │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────┴────────────────────────────────────┐
│                       Kernel Core                        │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │   Memory    │  │   Scheduler  │  │  Interrupts   │  │
│  │ Management  │  │    (Async)   │  │   (IDT/PIC)   │  │
│  └─────────────┘  └──────────────┘  └───────────────┘  │
│  ┌─────────────────────────────────────────────────┐   │
│  │            Virtual Filesystem (VFS)              │   │
│  │   ┌──────────┐  ┌──────────┐  ┌──────────┐    │   │
│  │   │  FAT32   │  │  RAMFS   │  │ /bin VFS │    │   │
│  │   └──────────┘  └──────────┘  └──────────┘    │   │
│  └─────────────────────────────────────────────────┘   │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────┴────────────────────────────────────┐
│                    Device Drivers                        │
│  ┌────────────┐  ┌────────────┐  ┌────────────────┐   │
│  │ Framebuffer│  │  USB XHCI  │  │  Network (tcp-ip)│  │
│  │    (GOP)   │  │  + MSC BOT │  │   Intel AX210   │   │
│  └────────────┘  └────────────┘  └────────────────┘   │
│  ┌────────────┐  ┌────────────┐                        │
│  │    VGA     │  │   Serial   │                        │
│  │  (Legacy)  │  │  (UART)    │                        │
│  └────────────┘  └────────────┘                        │
└────────────────────┬────────────────────────────────────┘
                     │
┌────────────────────┴────────────────────────────────────┐
│                      Hardware                            │
│         (x86_64 CPU, RAM, PCI devices, USB, etc.)       │
└─────────────────────────────────────────────────────────┘
```

---

## Boot Process

### 1. Firmware Stage (UEFI/BIOS)

```
Power On
   ↓
UEFI Firmware Init
   ├── Initialize hardware (CPU, RAM, PCIe)
   ├── Setup GOP framebuffer (graphics mode)
   ├── Load bootloader from ESP (EFI System Partition)
   └── Execute bootloader
```

### 2. Bootloader Stage (bootloader 0.11)

The bootloader performs critical initialization before kernel execution:

```rust
// Pseudo-code of bootloader actions
fn bootloader_main() {
    // 1. Setup paging
    let page_tables = setup_identity_mapping();
    let page_tables = add_offset_mapping(PHYS_MEM_OFFSET);
    
    // 2. Load kernel ELF
    let kernel_elf = load_file("kernel.elf");
    parse_and_map_segments(kernel_elf, &page_tables);
    
    // 3. Prepare BootInfo
    let boot_info = BootInfo {
        memory_map: get_memory_map_from_uefi(),
        framebuffer: Some(get_gop_framebuffer()),
        physical_memory_offset: PHYS_MEM_OFFSET,
        kernel_image_offset: kernel_virtual_start,
        // ...
    };
    
    // 4. Jump to kernel
    let entry_point = kernel_elf.entry_point();
    entry_point(&mut boot_info);  // Never returns
}
```

**Key Bootloader Functions**:
- Identity maps first 1 GiB of physical memory
- Offset maps all physical memory at `PHYS_MEM_OFFSET` (0xFFFF_8000_0000_0000)
- Loads kernel ELF and maps all PT_LOAD segments
- Passes `BootInfo` struct with memory map and framebuffer
- Jumps to kernel entry point (`_start` in assembly, then `kernel_main`)

### 3. Kernel Initialization Stage

**Phase 1: Critical Early Init** (interrupts disabled)

```rust
pub fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // Step 1: Disable interrupts (for safety)
    x86_64::instructions::interrupts::disable();
    
    // Step 2: Initialize serial port (for debug logging)
    unsafe {
        rustos::drivers::serial::init();
    }
    serial_println!("[kernel] Serial port initialized");
    
    // Step 3: Initialize framebuffer (for visible output)
    if let Some(framebuffer) = boot_info.framebuffer.take() {
        unsafe {
            rustos::drivers::framebuffer::init(framebuffer);
        }
        serial_println!("[kernel] Framebuffer initialized");
    } else {
        serial_println!("[kernel] No framebuffer, using VGA fallback");
    }
    
    println!("\n=== RustOS Kernel Initializing ===\n");
```

**Phase 2: Core Subsystem Init**

```rust
    // Step 4: Initialize GDT, IDT, interrupts (rustos::init())
    rustos::init();
    println!("GDT and IDT initialized");
    
    // Step 5: Initialize heap allocator
    rustos::allocator::init_heap(&boot_info.memory_map);
    println!("Heap allocator initialized");
```

**Phase 3: Device and Filesystem Init**

```rust
    // Step 6: Initialize VFS
    rustos::vfs::init();
    println!("Virtual filesystem initialized");
    
    // Step 7: Initialize PCI subsystem
    rustos::pci::init();
    println!("PCI subsystem initialized");
    
    // Step 8: Initialize USB and mount storage
    init_usb_storage();
    println!("USB storage initialized");
    
    // Step 9: Initialize network stack
    rustos::net::init();
    println!("Network stack initialized");
```

**Phase 4: Enable Interrupts and Start Services**

```rust
    // Step 10: Enable interrupts
    x86_64::instructions::interrupts::enable();
    serial_println!("[kernel] Interrupts enabled");
    
    // Step 11: Start async executor (keyboard task)
    rustos::task::executor::spawn(keyboard_task());
    
    // Step 12: Launch shell
    println!("\nLaunching /bin/rsh...\n");
    rustos::shell::launch_kernel_shell();
    
    // Step 13: Enter idle loop
    loop {
        x86_64::instructions::hlt();  // Wait for interrupts
    }
}
```

### Boot Sequence Diagram

```
┌──────────────┐
│ UEFI/BIOS    │ Power on, POST, load bootloader
└──────┬───────┘
       │
┌──────▼───────┐
│ Bootloader   │ Setup paging, load kernel, prepare BootInfo
└──────┬───────┘
       │
┌──────▼───────┐
│ kernel_main  │ Entry point (_start → kernel_main)
└──────┬───────┘
       │
┌──────▼───────┐
│ Serial Init  │ COM1 for debug output (before anything else)
└──────┬───────┘
       │
┌──────▼───────┐
│ Framebuffer  │ GOP framebuffer (for visible output)
└──────┬───────┘
       │
┌──────▼───────┐
│ rustos::init │ GDT, IDT, PIC, timer interrupts
└──────┬───────┘
       │
┌──────▼───────┐
│ Heap Init    │ Fixed-size block allocator (16 MiB)
└──────┬───────┘
       │
┌──────▼───────┐
│ VFS Init     │ Mount table, virtual /bin
└──────┬───────┘
       │
┌──────▼───────┐
│ PCI Init     │ Scan PCI bus for devices
└──────┬───────┘
       │
┌──────▼───────┐
│ USB Init     │ XHCI controller, mass storage
└──────┬───────┘
       │
┌──────▼───────┐
│ Network Init │ Intel AX210 driver (tcp-ip)
└──────┬───────┘
       │
┌──────▼───────┐
│ Enable IRQs  │ Now safe to receive interrupts
└──────┬───────┘
       │
┌──────▼───────┐
│ Async Tasks  │ Keyboard input task
└──────┬───────┘
       │
┌──────▼───────┐
│ Launch Shell │ /bin/rsh or kernel shell
└──────┬───────┘
       │
┌──────▼───────┐
│ Idle Loop    │ HLT waiting for interrupts
└──────────────┘
```

---

## Memory Management

### Virtual Memory Layout

```
User Space (Planned, not yet used):
0x0000_0000_0000_0000 ─┐
                       │ 128 TiB user address space
0x0000_7FFF_FFFF_FFFF ─┘ (not currently mapped)

Non-Canonical Hole:
0x0000_8000_0000_0000 ─┐
                       │ Non-canonical addresses
0xFFFF_7FFF_FFFF_FFFF ─┘ (invalid, cause #GP if accessed)

Kernel Space:
0xFFFF_8000_0000_0000 ─┐
                       │ Physical memory offset mapping
                       │ (all physical RAM accessible here)
0xFFFF_FFFF_FFFF_FFFF ─┘ (128 TiB kernel space)

Kernel Code/Data:
  Mapped by bootloader at 0xFFFF_8000_00XX_XXXX

Heap:
  Dynamically allocated from 0xFFFF_8000_XXXX_XXXX
  (16 MiB, FixedSizeBlockAllocator)

Stack:
  Grows downward from high address
  (each process will get own stack in future)
```

### Page Tables

RustOS uses 4-level paging (x86_64 standard):

```
Virtual Address (48-bit):
┌────────────┬────────────┬────────────┬────────────┬──────────────┐
│  Sign Ext  │   PML4     │    PDPT    │     PD     │      PT      │
│  (16 bits) │  (9 bits)  │  (9 bits)  │  (9 bits)  │   (12 bits)  │
└────────────┴────────────┴────────────┴────────────┴──────────────┘
     63-48        47-39        38-30        29-21         20-0

CR3 Register points to PML4 base

PML4 Entry → PDPT (Page Directory Pointer Table)
  ├── PDPT Entry → PD (Page Directory)
  │     ├── PD Entry → PT (Page Table)
  │     │     ├── PT Entry → 4 KiB Page
  │     │     └── ...
  │     └── (or) → 2 MiB Huge Page
  └── (or) → 1 GiB Huge Page
```

**Page Table Flags**:
- `PRESENT` (bit 0): Page is in physical memory
- `WRITABLE` (bit 1): Write access allowed
- `USER_ACCESSIBLE` (bit 2): User mode can access
- `WRITE_THROUGH` (bit 3): Write-through caching
- `NO_CACHE` (bit 4): Disable caching
- `ACCESSED` (bit 5): Set by CPU when page accessed
- `DIRTY` (bit 6): Set by CPU when page written
- `HUGE_PAGE` (bit 7): 2 MiB or 1 GiB page
- `GLOBAL` (bit 8): Don't flush on CR3 reload
- `NO_EXECUTE` (bit 63): Prevent code execution

### Frame Allocator

**BootInfoFrameAllocator**: Allocates physical frames from bootloader memory map.

```rust
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,  // Next usable frame
}

impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // Find next USABLE region in memory map
        // Return frame, increment next
    }
}
```

### Heap Allocator

**FixedSizeBlockAllocator**: Used for dynamic allocations (Box, Vec, etc.).

```rust
const HEAP_SIZE: usize = 16 * 1024 * 1024; // 16 MiB
static ALLOCATOR: LockedHeap = LockedHeap::new();

// Block sizes: 16, 32, 64, 128, 256, 512, 1024, 2048 bytes
// Larger allocations go to linked-list allocator
```

**Allocation Strategy**:
1. Round up size to next block size
2. Check free list for that block size
3. If available, pop from free list
4. If not, allocate from backing allocator
5. On deallocation, push to free list

### DMA Allocator

**DmaAllocator**: Allocates physically contiguous memory for DMA.

```rust
pub struct DmaAllocator {
    allocator: LinkedListAllocator,
}

impl DmaAllocator {
    pub fn alloc(&mut self, size: usize) -> Result<DmaAllocation, &'static str> {
        // Allocate physically contiguous memory
        // Return both virtual and physical addresses
    }
}
```

Used by USB XHCI driver for transfer buffers and ring buffers.

---

## Process Model

### Current State: Kernel-Only Execution

RustOS currently runs everything in kernel mode (Ring 0). There is no process isolation or userspace yet.

**Execution Model**:
- Single address space (kernel space only)
- Cooperative multitasking via async/await
- No preemption (interrupts can occur, but no context switching)

### Async Executor

Simple cooperative task executor for I/O operations:

```rust
pub struct Executor {
    tasks: VecDeque<Task>,
}

impl Executor {
    pub fn run(&mut self) {
        while let Some(task) = self.tasks.pop_front() {
            // Poll task
            let waker = /* ... */;
            match task.poll(&waker) {
                Poll::Ready(()) => { /* task done */ }
                Poll::Pending => { self.tasks.push_back(task); }
            }
        }
    }
}
```

**Current Use**: Keyboard input is handled as an async task.

### Future: Userspace Processes

**Planned Architecture**:

```
┌─────────────────────────────────────┐
│         Process 1 (User Mode)        │
│  ┌────────────┐  ┌────────────────┐ │
│  │   Stack    │  │  Heap (future) │ │
│  └────────────┘  └────────────────┘ │
│  ┌────────────────────────────────┐ │
│  │      ELF Code & Data           │ │
│  └────────────────────────────────┘ │
└─────────────────────────────────────┘
            ↕ (syscall: int 0x80)
┌─────────────────────────────────────┐
│        Kernel (Ring 0)               │
│  ┌─────────────────────────────┐   │
│  │    Syscall Dispatcher        │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

**ELF Loader** (`src/process/`):
- Parses ELF headers and program headers
- Maps PT_LOAD segments into page tables
- Sets up process stack
- Jumps to entry point

**Syscall Interface** (`src/syscall/`):
- Uses `int 0x80` instruction
- Registers: `rax` (syscall #), `rdi`, `rsi`, `rdx`, `r10` (args)
- Return: `rax`

---

## Filesystem Architecture

### Virtual Filesystem (VFS)

The VFS provides a unified interface for all filesystems:

```rust
pub trait FileSystem: Send {
    fn open(&self, path: &str) -> Result<Box<dyn File>, &'static str>;
    fn create(&self, path: &str) -> Result<Box<dyn File>, &'static str>;
    fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>, &'static str>;
    fn remove(&self, path: &str) -> Result<(), &'static str>;
}

pub trait File: Send {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, &'static str>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, &'static str>;
    fn seek(&mut self, pos: u64) -> Result<u64, &'static str>;
}
```

### Mount Table

The VFS maintains a mount table:

```rust
pub struct VFS {
    mounts: Vec<MountPoint>,
}

pub struct MountPoint {
    pub path: String,              // e.g., "/", "/usb", "/bin"
    pub filesystem: Box<dyn FileSystem>,
}
```

**Mount Resolution**:
1. Split path into components
2. Find longest matching mount point
3. Pass remainder of path to that filesystem

Example:
- Path: `/usb/documents/file.txt`
- Mount point: `/usb`
- Filesystem receives: `documents/file.txt`

### FAT32 Filesystem

**Implementation**: `src/fs/fat32/`

**Structure**:
```
FAT32 Volume:
┌──────────────────────────────────────┐
│        Boot Sector (BPB)             │ Sector 0
│  - Bytes per sector                  │
│  - Sectors per cluster               │
│  - FAT count, size                   │
│  - Root directory cluster            │
└──────────────────────────────────────┘
┌──────────────────────────────────────┐
│          FAT #1                      │
│  - Cluster chain entries             │
│  - Each entry: next cluster or EOF   │
└──────────────────────────────────────┘
┌──────────────────────────────────────┐
│          FAT #2 (backup)             │
└──────────────────────────────────────┘
┌──────────────────────────────────────┐
│         Data Region                  │
│  - Cluster 2 (first data cluster)    │
│  - Root directory entries            │
│  - File/directory data               │
│  - ...                               │
└──────────────────────────────────────┘
```

**Directory Entry**:
```rust
struct DirectoryEntry {
    name: [u8; 8],       // Short filename (8.3)
    ext: [u8; 3],
    attributes: u8,       // READONLY, HIDDEN, SYSTEM, etc.
    first_cluster_high: u16,
    first_cluster_low: u16,
    file_size: u32,
}
```

**Long Filename (LFN)**: Stored as special directory entries before the short entry.

**Cluster Chain Walking**:
```rust
fn read_cluster_chain(&self, start_cluster: u32) -> Vec<u8> {
    let mut data = Vec::new();
    let mut cluster = start_cluster;
    
    loop {
        // Read cluster data
        let cluster_data = read_cluster(cluster);
        data.extend_from_slice(&cluster_data);
        
        // Get next cluster from FAT
        cluster = self.fat[cluster];
        
        if cluster >= 0x0FFFFFF8 {
            break; // End of chain (EOF marker)
        }
    }
    
    data
}
```

### Virtual /bin Filesystem

Special filesystem that exposes kernel commands as executable files:

```rust
pub struct VirtualBinFilesystem {
    commands: HashMap<String, fn(&[&str])>,
}

// Example: ls /bin → ["help", "ls", "cat", "echo", ...]
// Example: /bin/ls → executes cmd_ls() kernel function
```

**Purpose**: Allows shell to treat built-in commands like external programs.

---

## Device Driver Model

### Driver Architecture

Drivers in RustOS follow a common pattern:

```rust
// 1. Global singleton with lazy initialization
lazy_static! {
    pub static ref MY_DEVICE: Mutex<Option<MyDevice>> = Mutex::new(None);
}

// 2. Device struct holding state
pub struct MyDevice {
    mmio_base: u64,
    registers: &'static mut MyDeviceRegisters,
    // ... device-specific state
}

// 3. Initialization function called from main.rs
pub fn init() {
    // Discover device (PCI scan, etc.)
    let device = MyDevice::new(/* ... */);
    
    // Initialize hardware
    device.initialize();
    
    // Store in global
    *MY_DEVICE.lock() = Some(device);
}

// 4. Public API for other kernel components
pub fn read_from_device() -> Result<u8, &'static str> {
    MY_DEVICE.lock().as_mut()
        .ok_or("Device not initialized")?
        .read()
}
```

### PCI Device Discovery

**Process**:
1. Enumerate PCI bus (bus 0-255, device 0-31, function 0-7)
2. Read vendor ID and device ID via configuration space
3. Match against known device IDs
4. Read BAR (Base Address Register) for MMIO location
5. Map BAR to virtual address space
6. Initialize device

**PCI Configuration Space Access**:
```rust
fn pci_read_config(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x8000_0000
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | (offset as u32 & 0xFC);
    
    unsafe {
        Port::new(0xCF8).write(address);
        Port::new(0xCFC).read()
    }
}
```

### USB XHCI Driver

**Architecture**:
```
XhciController
├── MMIO Registers (BAR0)
│   ├── Capability Registers
│   ├── Operational Registers
│   ├── Runtime Registers
│   └── Doorbell Registers
├── Command Ring (TRBs)
├── Event Ring (TRBs)
├── Transfer Rings (per endpoint)
├── Device Context Base Array
└── Ports: Vec<XhciPort>
    └── Port → Device → Endpoints
```

**Transfer Request Block (TRB)**:
```rust
#[repr(C)]
struct Trb {
    parameter: u64,
    status: u32,
    control: u32,
}
```

**Initialization**:
1. Find XHCI controller on PCI bus (class 0x0C, subclass 0x03)
2. Enable Bus Master and Memory Space
3. Map BAR0 to virtual memory
4. Reset controller
5. Initialize command/event rings
6. Start controller
7. Enumerate ports and configure devices

**Mass Storage**:
- Bulk-Only Transport (BOT) protocol
- SCSI commands: READ_10, WRITE_10, INQUIRY
- Command Block Wrapper (CBW) + Command Status Wrapper (CSW)

### Framebuffer Driver

**GOP (Graphics Output Protocol)**:
- Provided by UEFI firmware
- Pixel buffer at physical address
- Supports RGB or BGR pixel formats

**Text Rendering**:
- Software rendering using bitmap font
- 8x16 pixel characters (assets/font8x16.bin)
- Scrolling by copying pixel rows

**Character Grid**:
```
1920x1080 screen = 240 cols × 67 rows (8x16 font)
```

---

## Network Stack

### tcp-ip Submodule Integration

The network stack is provided by the external `tcp-ip` submodule.

**Integration Points**:

```rust
// src/net.rs - Kernel integration layer

pub fn init() {
    // 1. Find Intel AX210 on PCI bus
    let ax210 = pci::find_device(0x8086, AX210_DEVICE_IDS)?;
    
    // 2. Enable Bus Master + Memory Space
    pci::enable_bus_master(&ax210);
    
    // 3. Map BAR0 (3 MiB for AX210)
    let bar0_virt = map_device_bar(&ax210, 0, 0x30_0000);
    
    // 4. Initialize tcp-ip driver
    unsafe {
        tcp_ip::kernel::init(bar0_virt);
    }
    
    // 5. Check if initialized successfully
    if tcp_ip::kernel::is_active() {
        println!("Network stack initialized");
    }
}

pub fn dispatch_syscall(nr: u64, a1: u64, a2: u64, a3: u64) -> Option<i64> {
    tcp_ip::kernel::dispatch_syscall(nr, a1, a2, a3)
}
```

**Network Syscalls** (300-310):
- Defined in tcp-ip submodule
- Forwarded from kernel syscall dispatcher
- Handle Wi-Fi, TCP/IP, ICMP operations

**Userspace Tools**:
- `/bin/wifi`, `/bin/ping`, `/bin/ifconfig`, `/bin/netstat`
- Built by tcp-ip submodule as ELF binaries
- Embedded into kernel by build.rs
- Exposed as virtual files in /bin

---

## Inter-Component Communication

### Global Statics

Most components use global singletons:

```rust
lazy_static! {
    pub static ref VFS: Mutex<Vfs> = Mutex::new(Vfs::new());
    pub static ref USB_XHCI: Mutex<Option<XhciController>> = Mutex::new(None);
    pub static ref FRAMEBUFFER_WRITER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);
}
```

**Locking Rules**:
- Always lock for shortest time possible
- Never call other components while holding a lock
- Never hold locks across await points

### System Calls

Userspace programs communicate with kernel via syscalls:

```
User Program                Kernel
    │                          │
    │    int 0x80              │
    ├─────────────────────────>│
    │                          │ syscall_handler() [naked]
    │                          │   │
    │                          │   ├─> dispatch(nr, args)
    │                          │   │     │
    │                          │   │     ├─> sys_read()
    │                          │   │     ├─> sys_write()
    │                          │   │     └─> crate::net::dispatch_syscall()
    │                          │   │
    │                          │   └─> (result in rax)
    │                          │
    │    <return via iretq>    │
    │<─────────────────────────┤
    │                          │
```

### Interrupts

Hardware communicates with kernel via interrupts:

```
Hardware              PIC              Kernel
    │                  │                 │
    │   IRQ signal     │                 │
    ├─────────────────>│                 │
    │                  │   INT vector    │
    │                  ├────────────────>│
    │                  │                 │ IDT dispatch
    │                  │                 │   │
    │                  │                 │   ├─> timer_handler()
    │                  │                 │   ├─> keyboard_handler()
    │                  │                 │   └─> ...
    │                  │                 │
    │                  │   EOI (0x20)    │
    │                  │<────────────────┤
    │                  │                 │
```

---

## Design Decisions

### Why Rust?

**Memory Safety**: Rust's borrow checker prevents:
- Use-after-free
- Double-free
- Buffer overflows (in safe code)
- Data races

**No Runtime**: Rust has no garbage collector or runtime, making it suitable for OS development.

**Modern Language**: Pattern matching, iterators, traits provide excellent ergonomics.

**Unsafe Boundaries**: Unsafe code is explicitly marked, making it easy to audit.

### Why Monolithic Kernel?

**Simplicity**: Easier to develop and debug than microkernel.

**Performance**: No IPC overhead for driver communication.

**Flexibility**: Can refactor into microkernel later if needed.

### Why Async/Await Instead of Threads?

**Current**: Async is simpler for I/O-bound tasks like keyboard input.

**Future**: Will add preemptive multitasking for true parallelism.

### Why FAT32?

**Compatibility**: Universal filesystem, readable on all OSes.

**Simplicity**: Relatively simple to implement.

**Use Case**: RustOS targets USB flash drives, which are typically FAT32.

### Why Submodules for rsh and tcp-ip?

**Modularity**: Keeps network stack separate from kernel core.

**Reusability**: tcp-ip can be used in other projects.

**Independent Development**: Can develop network stack independently.

**Auto-Update**: build.rs ensures latest version is always used.

---

## Future Directions

### Short Term

- [ ] Preemptive multitasking (process scheduler)
- [ ] Multiple processes with isolation
- [ ] Virtual memory per-process
- [ ] Copy-on-write for fork()

### Medium Term

- [ ] SMP support (multiple CPU cores)
- [ ] AHCI (SATA) disk driver
- [ ] NVMe driver
- [ ] Ext4 filesystem support
- [ ] Dynamic linking for userspace programs

### Long Term

- [ ] Full POSIX compatibility layer
- [ ] GUI framework
- [ ] Window manager
- [ ] Package manager
- [ ] Self-hosting (build RustOS on RustOS)

---

**Document Version**: 1.0  
**Last Updated**: 2026-05-04  
**Maintainer**: RustOS-Dev Team
