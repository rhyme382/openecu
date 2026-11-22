//! In-memory firmware image abstractions; no file-system or ECU I/O.
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FirmwareError {
    #[error("range length must be non-zero")]
    EmptyRange,
    #[error("range exceeds firmware image")]
    OutOfBounds,
    #[error("ECU address is outside every mapped memory region")]
    AddressOutsideMap,
    #[error("memory region length must be non-zero")]
    EmptyRegion,
    #[error("memory region overflows the ECU address space or the BIN offset space")]
    RegionOverflow,
    #[error("address map must contain at least one memory region")]
    EmptyMap,
    #[error("memory regions overlap in ECU address space or in the BIN")]
    OverlappingRegions,
    #[error("range crosses the end of its memory region")]
    RangeCrossesRegion,
}
/// A contiguous ECU address window backed by a contiguous BIN window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion {
    ecu_start: u64,
    length: usize,
    bin_offset: usize,
}
impl MemoryRegion {
    pub fn new(ecu_start: u64, length: usize, bin_offset: usize) -> Result<Self, FirmwareError> {
        if length == 0 {
            return Err(FirmwareError::EmptyRegion);
        }
        let span = u64::try_from(length - 1).map_err(|_| FirmwareError::RegionOverflow)?;
        ecu_start
            .checked_add(span)
            .ok_or(FirmwareError::RegionOverflow)?;
        bin_offset
            .checked_add(length)
            .ok_or(FirmwareError::RegionOverflow)?;
        Ok(Self {
            ecu_start,
            length,
            bin_offset,
        })
    }
    pub const fn ecu_start(&self) -> u64 {
        self.ecu_start
    }
    pub const fn length(&self) -> usize {
        self.length
    }
    pub const fn bin_offset(&self) -> usize {
        self.bin_offset
    }
    /// Last ECU address contained in the region (cannot overflow, see `new`).
    const fn ecu_last(&self) -> u64 {
        self.ecu_start + (self.length as u64 - 1)
    }
    /// Exclusive end offset in the BIN (cannot overflow, see `new`).
    const fn bin_end(&self) -> usize {
        self.bin_offset + self.length
    }
}
/// Immutable, validated set of non-overlapping memory regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcuAddressMap {
    /// Sorted by `ecu_start`.
    regions: Vec<MemoryRegion>,
}
impl EcuAddressMap {
    /// Single-region map: `length` bytes at `ecu_start` stored at `bin_offset`.
    pub fn new(ecu_start: u64, bin_offset: usize, length: usize) -> Result<Self, FirmwareError> {
        Self::from_regions(vec![MemoryRegion::new(ecu_start, length, bin_offset)?])
    }
    pub fn from_regions(mut regions: Vec<MemoryRegion>) -> Result<Self, FirmwareError> {
        if regions.is_empty() {
            return Err(FirmwareError::EmptyMap);
        }
        regions.sort_by_key(|region| region.ecu_start);
        if regions
            .windows(2)
            .any(|pair| pair[0].ecu_last() >= pair[1].ecu_start)
        {
            return Err(FirmwareError::OverlappingRegions);
        }
        let mut by_bin = regions.clone();
        by_bin.sort_by_key(|region| region.bin_offset);
        if by_bin
            .windows(2)
            .any(|pair| pair[0].bin_end() > pair[1].bin_offset)
        {
            return Err(FirmwareError::OverlappingRegions);
        }
        Ok(Self { regions })
    }
    pub fn regions(&self) -> &[MemoryRegion] {
        &self.regions
    }
    /// Verifies that every region lies inside an image of `image_len` bytes.
    pub fn check_fits_image(&self, image_len: usize) -> Result<(), FirmwareError> {
        if self
            .regions
            .iter()
            .any(|region| region.bin_end() > image_len)
        {
            Err(FirmwareError::OutOfBounds)
        } else {
            Ok(())
        }
    }
    /// Translates an ECU range; the whole range must lie in a single region.
    pub fn range_for(
        &self,
        ecu_address: u64,
        length: usize,
    ) -> Result<FirmwareRange, FirmwareError> {
        if length == 0 {
            return Err(FirmwareError::EmptyRange);
        }
        let index = self
            .regions
            .partition_point(|region| region.ecu_start <= ecu_address);
        let region = index
            .checked_sub(1)
            .map(|index| self.regions[index])
            .filter(|region| ecu_address <= region.ecu_last())
            .ok_or(FirmwareError::AddressOutsideMap)?;
        // Bounded by region.length, so it fits in usize.
        let relative = (ecu_address - region.ecu_start) as usize;
        if length > region.length - relative {
            return Err(FirmwareError::RangeCrossesRegion);
        }
        FirmwareRange::new(region.bin_offset + relative, length)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ByteOrder {
    BigEndian,
    LittleEndian,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveType {
    U8,
    S8,
    U16,
    S16,
    U32,
    S32,
}
impl PrimitiveType {
    pub const fn byte_len(self) -> usize {
        match self {
            Self::U8 | Self::S8 => 1,
            Self::U16 | Self::S16 => 2,
            Self::U32 | Self::S32 => 4,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveValue {
    Unsigned(u32),
    Signed(i32),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareRange {
    pub offset: usize,
    pub length: usize,
}
impl FirmwareRange {
    pub fn new(offset: usize, length: usize) -> Result<Self, FirmwareError> {
        if length == 0 {
            Err(FirmwareError::EmptyRange)
        } else {
            Ok(Self { offset, length })
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareImage {
    pub label: String,
    bytes: Vec<u8>,
}
impl FirmwareImage {
    pub fn new(label: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            label: label.into(),
            bytes,
        }
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub fn read_range(&self, range: FirmwareRange) -> Result<&[u8], FirmwareError> {
        let end = range
            .offset
            .checked_add(range.length)
            .ok_or(FirmwareError::OutOfBounds)?;
        self.bytes
            .get(range.offset..end)
            .ok_or(FirmwareError::OutOfBounds)
    }
    pub fn read_ecu_range(
        &self,
        mapping: &EcuAddressMap,
        ecu_address: u64,
        length: usize,
    ) -> Result<&[u8], FirmwareError> {
        self.read_range(mapping.range_for(ecu_address, length)?)
    }
    pub fn read_primitive(
        &self,
        mapping: &EcuAddressMap,
        ecu_address: u64,
        primitive_type: PrimitiveType,
        byte_order: ByteOrder,
    ) -> Result<PrimitiveValue, FirmwareError> {
        let bytes = self.read_ecu_range(mapping, ecu_address, primitive_type.byte_len())?;
        let unsigned = match bytes.len() {
            1 => u32::from(bytes[0]),
            2 => match byte_order {
                ByteOrder::BigEndian => u32::from(u16::from_be_bytes([bytes[0], bytes[1]])),
                ByteOrder::LittleEndian => u32::from(u16::from_le_bytes([bytes[0], bytes[1]])),
            },
            4 => match byte_order {
                ByteOrder::BigEndian => {
                    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                }
                ByteOrder::LittleEndian => {
                    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                }
            },
            _ => unreachable!("primitive types have only 1, 2, or 4 bytes"),
        };
        Ok(match primitive_type {
            PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32 => {
                PrimitiveValue::Unsigned(unsigned)
            }
            PrimitiveType::S8 => PrimitiveValue::Signed((unsigned as u8 as i8).into()),
            PrimitiveType::S16 => PrimitiveValue::Signed(unsigned as u16 as i16 as i32),
            PrimitiveType::S32 => PrimitiveValue::Signed(unsigned as i32),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ecu_addresses_to_bin_offsets_without_mutating_the_image() {
        let image = FirmwareImage::new("fixture", vec![0, 1, 2, 3]);
        let mapping = EcuAddressMap::new(0x8000, 1, 3).unwrap();

        assert_eq!(image.read_ecu_range(&mapping, 0x8001, 2).unwrap(), [2, 3]);
        assert_eq!(
            image.read_range(FirmwareRange::new(0, 4).unwrap()).unwrap(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn rejects_mapping_underflow_overflow_and_ranges_outside_the_bin() {
        let image = FirmwareImage::new("fixture", vec![0, 1, 2]);
        let mapping = EcuAddressMap::new(0x8000, 1, 8).unwrap();

        assert_eq!(
            mapping.range_for(0x7fff, 1),
            Err(FirmwareError::AddressOutsideMap)
        );
        // The overflow is now rejected when the bounded region is built.
        assert_eq!(
            EcuAddressMap::new(0, usize::MAX, 2),
            Err(FirmwareError::RegionOverflow)
        );
        assert_eq!(
            image.read_ecu_range(&mapping, 0x8002, 2),
            Err(FirmwareError::OutOfBounds)
        );
    }

    #[test]
    fn decodes_supported_integer_types_in_both_byte_orders() {
        let image = FirmwareImage::new(
            "fixture",
            vec![
                0x80, 0x7f, 0xfe, 0xff, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04, 0xff, 0xff, 0xff, 0xfe,
            ],
        );
        let mapping = EcuAddressMap::new(0x1000, 0, 14).unwrap();

        assert_eq!(
            image.read_primitive(&mapping, 0x1000, PrimitiveType::U8, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Unsigned(128))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1001, PrimitiveType::U8, ByteOrder::LittleEndian),
            Ok(PrimitiveValue::Unsigned(127))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1000, PrimitiveType::S8, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Signed(-128))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1000, PrimitiveType::S8, ByteOrder::LittleEndian),
            Ok(PrimitiveValue::Signed(-128))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1002, PrimitiveType::U16, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Unsigned(65_279))
        );
        assert_eq!(
            image.read_primitive(
                &mapping,
                0x1002,
                PrimitiveType::U16,
                ByteOrder::LittleEndian
            ),
            Ok(PrimitiveValue::Unsigned(65_534))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1002, PrimitiveType::S16, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Signed(-257))
        );
        assert_eq!(
            image.read_primitive(
                &mapping,
                0x1002,
                PrimitiveType::S16,
                ByteOrder::LittleEndian
            ),
            Ok(PrimitiveValue::Signed(-2))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1004, PrimitiveType::U16, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Unsigned(4_660))
        );
        assert_eq!(
            image.read_primitive(
                &mapping,
                0x1004,
                PrimitiveType::U16,
                ByteOrder::LittleEndian
            ),
            Ok(PrimitiveValue::Unsigned(13_330))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x1006, PrimitiveType::U32, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Unsigned(16_909_060))
        );
        assert_eq!(
            image.read_primitive(
                &mapping,
                0x1006,
                PrimitiveType::U32,
                ByteOrder::LittleEndian
            ),
            Ok(PrimitiveValue::Unsigned(67_305_985))
        );
        assert_eq!(
            image.read_primitive(&mapping, 0x100a, PrimitiveType::S32, ByteOrder::BigEndian),
            Ok(PrimitiveValue::Signed(-2))
        );
        assert_eq!(
            image.read_primitive(
                &mapping,
                0x100a,
                PrimitiveType::S32,
                ByteOrder::LittleEndian
            ),
            Ok(PrimitiveValue::Signed(-16_777_217))
        );
    }
    fn two_region_map() -> EcuAddressMap {
        EcuAddressMap::from_regions(vec![
            MemoryRegion::new(0x2000, 0x10, 0x20).unwrap(),
            MemoryRegion::new(0x1000, 0x10, 0).unwrap(),
        ])
        .unwrap()
    }

    #[test]
    fn region_constructor_rejects_empty_and_overflowing_regions() {
        assert_eq!(MemoryRegion::new(0, 0, 0), Err(FirmwareError::EmptyRegion));
        assert_eq!(
            MemoryRegion::new(u64::MAX - 3, 5, 0),
            Err(FirmwareError::RegionOverflow)
        );
        assert_eq!(
            MemoryRegion::new(0, 2, usize::MAX),
            Err(FirmwareError::RegionOverflow)
        );
        assert!(MemoryRegion::new(u64::MAX - 3, 4, 0).is_ok());
    }

    #[test]
    fn map_rejects_no_regions_and_overlapping_ecu_or_bin_ranges() {
        assert_eq!(
            EcuAddressMap::from_regions(vec![]),
            Err(FirmwareError::EmptyMap)
        );
        let ecu_overlap = vec![
            MemoryRegion::new(0x1000, 0x10, 0).unwrap(),
            MemoryRegion::new(0x100f, 0x10, 0x20).unwrap(),
        ];
        assert_eq!(
            EcuAddressMap::from_regions(ecu_overlap),
            Err(FirmwareError::OverlappingRegions)
        );
        let bin_overlap = vec![
            MemoryRegion::new(0x1000, 0x10, 0).unwrap(),
            MemoryRegion::new(0x2000, 0x10, 0x0f).unwrap(),
        ];
        assert_eq!(
            EcuAddressMap::from_regions(bin_overlap),
            Err(FirmwareError::OverlappingRegions)
        );
    }

    #[test]
    fn map_accepts_unsorted_regions_with_gaps_and_adjacent_regions() {
        let map = two_region_map();
        assert_eq!(map.range_for(0x2004, 2).unwrap().offset, 0x24);
        assert_eq!(map.range_for(0x1000, 1).unwrap().offset, 0);
        let adjacent = EcuAddressMap::from_regions(vec![
            MemoryRegion::new(0x1000, 0x10, 0).unwrap(),
            MemoryRegion::new(0x1010, 0x10, 0x10).unwrap(),
        ]);
        assert!(adjacent.is_ok());
    }

    #[test]
    fn reads_in_a_gap_or_above_every_region_are_unmapped() {
        let map = two_region_map();
        for address in [0x1800, 0x2010, 0x1010] {
            assert_eq!(
                map.range_for(address, 1),
                Err(FirmwareError::AddressOutsideMap)
            );
        }
    }

    #[test]
    fn reads_may_end_exactly_at_a_region_end_but_not_cross_it() {
        let map = two_region_map();
        assert_eq!(map.range_for(0x100e, 2).unwrap().offset, 0x0e);
        for (address, length) in [(0x100f, 2), (0x1000, 0x11), (0x1000, usize::MAX)] {
            assert_eq!(
                map.range_for(address, length),
                Err(FirmwareError::RangeCrossesRegion)
            );
        }
    }

    #[test]
    fn reads_never_cross_between_adjacent_regions() {
        let map = EcuAddressMap::from_regions(vec![
            MemoryRegion::new(0x1000, 0x10, 0).unwrap(),
            MemoryRegion::new(0x1010, 0x10, 0x10).unwrap(),
        ])
        .unwrap();
        assert_eq!(
            map.range_for(0x100f, 2),
            Err(FirmwareError::RangeCrossesRegion)
        );
    }

    #[test]
    fn region_at_top_of_address_space_does_not_overflow() {
        let map = EcuAddressMap::new(u64::MAX - 3, 0, 4).unwrap();
        assert_eq!(map.range_for(u64::MAX - 1, 2).unwrap().offset, 2);
        assert_eq!(
            map.range_for(u64::MAX, 2),
            Err(FirmwareError::RangeCrossesRegion)
        );
    }

    #[test]
    fn map_regions_must_fit_inside_the_image() {
        let map = two_region_map();
        assert_eq!(map.check_fits_image(0x30), Ok(()));
        assert_eq!(map.check_fits_image(0x2f), Err(FirmwareError::OutOfBounds));
        let image = FirmwareImage::new("fixture", vec![0; 0x25]);
        assert_eq!(
            image.read_ecu_range(&map, 0x2004, 0x10 - 4),
            Err(FirmwareError::OutOfBounds)
        );
    }

    #[test]
    fn rejects_a_read_outside_the_firmware_image() {
        let x = FirmwareImage::new("fixture", vec![0, 1, 2]);
        assert!(x.read_range(FirmwareRange::new(2, 2).unwrap()).is_err())
    }
    #[test]
    fn returns_requested_bytes() {
        let x = FirmwareImage::new("fixture", vec![0, 1, 2]);
        assert_eq!(
            x.read_range(FirmwareRange::new(1, 2).unwrap()).unwrap(),
            [1, 2]
        )
    }
}
