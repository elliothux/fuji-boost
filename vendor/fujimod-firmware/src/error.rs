use crate::InnerHeader;

/// Malformed input and mapping failures, with offsets relative to the supplied
/// slice (DAT offsets for container operations, stream offsets for decompression).
/// Overlay descriptor diagnostics identify table pointers as addresses.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("extent outside DAT: offset {offset:#x}, size {size:#x}, available {available:#x}")]
    Extent {
        offset: u64,
        size: u64,
        available: u64,
    },
    #[error("address/extent overflow: offset {offset:#x}, size {size:#x}")]
    Overflow { offset: u64, size: u64 },
    #[error("invalid {field} at {offset:#x}: {value:#x}, expected {expected:#x}")]
    HeaderValue {
        offset: u64,
        field: &'static str,
        value: u64,
        expected: u64,
    },
    #[error("invalid literal run at {offset:#x}: length {length}, remaining {remaining}")]
    Literal {
        offset: u64,
        length: usize,
        remaining: usize,
    },
    #[error(
        "invalid/truncated back reference at {offset:#x}: distance {distance:?}, length {length:?}, output size {output_size}"
    )]
    Match {
        offset: u64,
        distance: Option<usize>,
        length: Option<usize>,
        output_size: usize,
    },
    #[error(
        "output size disagrees with declared size at {offset:#x}: actual {actual}, declared {declared}"
    )]
    DecodedSize {
        offset: u64,
        actual: u64,
        declared: u64,
    },
    #[error("cannot allocate decoded stream of {size} bytes")]
    Allocation { size: u64 },
    #[error("hash mismatch at {offset:#x}, size {size:#x}: expected {expected}, actual {actual}")]
    Hash {
        offset: u64,
        size: u64,
        expected: String,
        actual: String,
    },
    #[error(
        "raw inner header disagrees with mapping at {offset:#x}: bits {bits}, base {base:#x}, size {size:#x}, architecture {architecture_bits}, actual {actual:?}"
    )]
    RawHeader {
        offset: u64,
        bits: u8,
        base: u64,
        size: u64,
        architecture_bits: u8,
        actual: Option<InnerHeader>,
    },
    #[error(
        "raw entry outside initialized mapping or differs at {offset:#x}: entry {entry:#x}, expected {expected:?}, base {base:#x}, size {size:#x}"
    )]
    RawEntry {
        offset: u64,
        entry: u64,
        expected: Option<u64>,
        base: u64,
        size: u64,
    },
    #[error(
        "overlay descriptor pointer outside pinned table or unaligned: address {address:#x}, base {base:#x}, table size {size:#x}, read size {read_size:#x}"
    )]
    DescriptorPointer {
        address: u64,
        base: u64,
        size: u64,
        read_size: u64,
    },
    #[error("duplicate section id {id} at {address:#x}")]
    DuplicateSection { address: u64, id: u32 },
    #[error(
        "overlay flash/DAT bias disagrees with compressed stream: base {base:#x}, DAT {dat_offset:#x}, bias {bias:#x}, matching flash offsets {flash_offsets:?}"
    )]
    StreamBias {
        base: u64,
        dat_offset: u64,
        bias: u64,
        flash_offsets: Vec<u32>,
    },
    #[error("overlay section {id} absent in table at {offset:#x}")]
    MissingSection { offset: u64, id: u32 },
    #[error("overlay section {id} compressed: method {method}")]
    CompressedSection { id: u32, method: u32 },
    #[error(
        "overlay group at {address:#x} disagrees with raw section: size {group_size:#x}/{section_size:#x}, base {base:#x}, flash offset {flash_offset:#x}"
    )]
    OverlayGroup {
        address: u64,
        group_size: u32,
        section_size: u32,
        base: u32,
        flash_offset: u32,
    },
    #[error(
        "overlay entry outside raw flash group or unsupported flags at {address:#x}: run {run:#x}, length {length:#x}, reserved {reserved:#x}, flags {flags:#x}, group {base:#x}+{size:#x}"
    )]
    OverlayEntry {
        address: u64,
        run: u64,
        length: u64,
        reserved: u32,
        flags: u32,
        base: u32,
        size: u32,
    },
    #[error("unterminated overlay table at {address:#x}, size {size:#x}")]
    UnterminatedTable { address: u64, size: u64 },
    #[error("unsupported outer header type {value} at {offset:#x}")]
    OuterType { offset: u64, value: u32 },
    #[error("missing signed-container DER trailer in range {offset:#x}+{size:#x}")]
    MissingSignature { offset: u64, size: u64 },
}
