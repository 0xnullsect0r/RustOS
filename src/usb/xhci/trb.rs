//! XHCI Transfer Request Block (TRB) type constants and structure definitions.
//!
//! Every TRB is exactly 16 bytes (4 × u32 words, little-endian).
//! Bit 0 of word 3 is the Cycle Bit used for ring management.

// ---------------------------------------------------------------------------
// TRB type codes (bits [15:10] of the Control word, i.e. word[3])
// ---------------------------------------------------------------------------
pub mod ty {
    pub const NORMAL: u32 = 1;
    pub const SETUP_STAGE: u32 = 2;
    pub const DATA_STAGE: u32 = 3;
    pub const STATUS_STAGE: u32 = 4;
    pub const LINK: u32 = 6;
    pub const NO_OP: u32 = 8;
    pub const ENABLE_SLOT_CMD: u32 = 9;
    pub const DISABLE_SLOT_CMD: u32 = 10;
    pub const ADDRESS_DEVICE_CMD: u32 = 11;
    pub const CONFIGURE_EP_CMD: u32 = 12;
    pub const NO_OP_CMD: u32 = 23;
    // Event types
    pub const TRANSFER_EVENT: u32 = 32;
    pub const CMD_COMPLETION_EVENT: u32 = 33;
    pub const PORT_STATUS_CHANGE: u32 = 34;
}

/// Completion codes (upper 8 bits of event TRB status word).
pub mod cc {
    pub const SUCCESS: u8 = 1;
    pub const SHORT_PACKET: u8 = 13;
    pub const STALL: u8 = 6;
    pub const BABBLE: u8 = 8;
}

// ---------------------------------------------------------------------------
// Raw 16-byte TRB
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct Trb {
    pub word: [u32; 4],
}

impl Trb {
    pub const fn zero() -> Self {
        Trb { word: [0; 4] }
    }

    /// Set the TRB type in control word (bits [15:10]).
    pub fn set_type(&mut self, t: u32) {
        self.word[3] = (self.word[3] & !(0x3F << 10)) | ((t & 0x3F) << 10);
    }

    pub fn trb_type(&self) -> u32 {
        (self.word[3] >> 10) & 0x3F
    }

    pub fn cycle_bit(&self) -> bool {
        self.word[3] & 1 != 0
    }

    pub fn set_cycle(&mut self, c: bool) {
        if c {
            self.word[3] |= 1;
        } else {
            self.word[3] &= !1;
        }
    }

    /// Completion code from an event TRB (bits [31:24] of word[2]).
    pub fn completion_code(&self) -> u8 {
        (self.word[2] >> 24) as u8
    }

    /// Slot ID from a command completion event (bits [31:24] of word[3]).
    pub fn slot_id(&self) -> u8 {
        (self.word[3] >> 24) as u8
    }

    /// Port ID from a port status change event (bits [31:24] of word[0]).
    pub fn port_id(&self) -> u8 {
        ((self.word[0] >> 24) & 0xFF) as u8
    }
}

// ---------------------------------------------------------------------------
// Builder helpers
// ---------------------------------------------------------------------------

/// Build a Link TRB pointing back to `ring_phys` (for ring wrap-around).
pub fn link_trb(ring_phys: u64, toggle_cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[0] = ring_phys as u32;
    t.word[1] = (ring_phys >> 32) as u32;
    t.word[3] = (ty::LINK << 10) | if toggle_cycle { 1 << 1 } else { 0 };
    t
}

/// Build an Enable Slot command TRB.
pub fn enable_slot_cmd(cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[3] = (ty::ENABLE_SLOT_CMD << 10) | if cycle { 1 } else { 0 };
    t
}

/// Build an Address Device command TRB.
pub fn address_device_cmd(input_ctx_phys: u64, slot_id: u8, bsr: bool, cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[0] = input_ctx_phys as u32;
    t.word[1] = (input_ctx_phys >> 32) as u32;
    // BSR = bit 9, slot_id in bits [31:24] of word[3]
    t.word[3] = (ty::ADDRESS_DEVICE_CMD << 10)
        | ((slot_id as u32) << 24)
        | if bsr { 1 << 9 } else { 0 }
        | if cycle { 1 } else { 0 };
    t
}

/// Build a Configure Endpoint command TRB.
pub fn configure_ep_cmd(input_ctx_phys: u64, slot_id: u8, cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[0] = input_ctx_phys as u32;
    t.word[1] = (input_ctx_phys >> 32) as u32;
    t.word[3] = (ty::CONFIGURE_EP_CMD << 10) | ((slot_id as u32) << 24) | if cycle { 1 } else { 0 };
    t
}

/// Build a Setup Stage TRB for a control transfer.
pub fn setup_stage_trb(
    bm_request_type: u8,
    b_request: u8,
    w_value: u16,
    w_index: u16,
    w_length: u16,
    transfer_type: u8, // 0=no data, 2=OUT data, 3=IN data
    cycle: bool,
) -> Trb {
    let mut t = Trb::zero();
    t.word[0] = (bm_request_type as u32) | ((b_request as u32) << 8) | ((w_value as u32) << 16);
    t.word[1] = (w_index as u32) | ((w_length as u32) << 16);
    t.word[2] = 8; // TRB transfer length always 8 for setup stage
    // IDT=1 (immediate data), TRT in bits [17:16]
    t.word[3] = (ty::SETUP_STAGE << 10)
        | (1 << 6)  // IDT
        | ((transfer_type as u32) << 16)
        | if cycle { 1 } else { 0 };
    t
}

/// Build a Data Stage TRB.
pub fn data_stage_trb(buf_phys: u64, length: u32, dir_in: bool, cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[0] = buf_phys as u32;
    t.word[1] = (buf_phys >> 32) as u32;
    t.word[2] = length;
    t.word[3] = (ty::DATA_STAGE << 10)
        | (1 << 5)  // IOC
        | if dir_in { 1 << 16 } else { 0 }
        | if cycle { 1 } else { 0 };
    t
}

/// Build a Status Stage TRB.
pub fn status_stage_trb(dir_in: bool, cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[3] = (ty::STATUS_STAGE << 10)
        | (1 << 5)  // IOC
        | if dir_in { 1 << 16 } else { 0 }
        | if cycle { 1 } else { 0 };
    t
}

/// Build a Normal TRB for bulk transfers.
pub fn normal_trb(buf_phys: u64, length: u32, cycle: bool) -> Trb {
    let mut t = Trb::zero();
    t.word[0] = buf_phys as u32;
    t.word[1] = (buf_phys >> 32) as u32;
    t.word[2] = length;
    t.word[3] = (ty::NORMAL << 10)
        | (1 << 5)  // IOC
        | if cycle { 1 } else { 0 };
    t
}
