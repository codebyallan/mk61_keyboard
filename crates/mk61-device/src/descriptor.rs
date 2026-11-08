//! HID report-descriptor parsing for the vendor configuration interface.
//!
//! The keyboard exposes several HID collections. Only the vendor collection with
//! usage page `0xFF01` and usage `0x0001` accepts the 264-byte configuration
//! feature report, which uses report ID `0x07`.
//!
//! The parser below is deliberately small and dependency-free. It understands
//! the parts of the HID item stream needed to identify top-level collections and
//! their feature report IDs, skips long items, and treats truncated or otherwise
//! malformed descriptors as an error instead of panicking.

use std::collections::BTreeSet;

/// Maximum number of report-descriptor bytes read from sysfs.
pub const MAX_REPORT_DESCRIPTOR_BYTES: u64 = 64 * 1024;

/// Report ID used by the MK61 / SK61L vendor configuration feature report.
pub const CONFIG_REPORT_ID: u8 = 0x07;

const ITEM_TYPE_MAIN: u8 = 0;
const ITEM_TYPE_GLOBAL: u8 = 1;
const ITEM_TYPE_LOCAL: u8 = 2;

const GLOBAL_USAGE_PAGE: u8 = 0x0;
const GLOBAL_REPORT_ID: u8 = 0x8;
const GLOBAL_PUSH: u8 = 0xA;
const GLOBAL_POP: u8 = 0xB;

const LOCAL_USAGE: u8 = 0x0;

const MAIN_COLLECTION: u8 = 0xA;
const MAIN_FEATURE: u8 = 0xB;
const MAIN_END_COLLECTION: u8 = 0xC;

const LONG_ITEM_PREFIX: u8 = 0xFE;

/// Identifies a HID top-level collection by its usage page and usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceDescriptor {
    /// HID usage page, for example `0xFF01`.
    pub usage_page: u16,
    /// HID usage within the page, for example `0x0001`.
    pub usage: u16,
}

impl InterfaceDescriptor {
    /// Vendor configuration interface of the MK61 / SK61L.
    pub const MK61_CONFIG: Self = Self {
        usage_page: 0xFF01,
        usage: 0x0001,
    };

    /// Returns whether `collection` describes this interface.
    pub fn matches(&self, collection: &HidCollection) -> bool {
        collection.usage_page == self.usage_page && collection.usage == self.usage
    }
}

/// A top-level HID collection and the report IDs used by its main items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidCollection {
    /// Usage page of the collection.
    pub usage_page: u16,
    /// Usage within the collection's usage page.
    pub usage: u16,
    /// Report IDs declared by feature reports inside the collection.
    pub report_ids: BTreeSet<u8>,
}

/// The top-level collections found in a HID report descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HidDescriptor {
    /// Collections in descriptor order.
    pub collections: Vec<HidCollection>,
}

impl HidDescriptor {
    /// Returns the first collection matching `interface`.
    pub fn find(&self, interface: InterfaceDescriptor) -> Option<&HidCollection> {
        self.collections
            .iter()
            .find(|collection| interface.matches(collection))
    }

    /// Returns whether the descriptor contains the MK61 vendor collection.
    pub fn is_mk61_config(&self) -> bool {
        self.find(InterfaceDescriptor::MK61_CONFIG).is_some()
    }

    /// Returns the report ID used by the MK61 vendor collection.
    ///
    /// The verified configuration report ID `0x07` is preferred. If the
    /// descriptor exposes the vendor collection with a different feature report
    /// ID, that ID is returned so callers can validate it explicitly.
    pub fn mk61_report_id(&self) -> Option<u8> {
        let collection = self.find(InterfaceDescriptor::MK61_CONFIG)?;
        if collection.report_ids.contains(&CONFIG_REPORT_ID) {
            return Some(CONFIG_REPORT_ID);
        }
        collection.report_ids.iter().next().copied()
    }
}

/// Errors produced while parsing a malformed HID report descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DescriptorError {
    /// A short item declares more data bytes than remain in the descriptor.
    #[error("truncated short HID item at byte {offset}")]
    TruncatedShortItem {
        /// Byte offset of the item prefix.
        offset: usize,
    },

    /// A long item declares more data bytes than remain in the descriptor.
    #[error("truncated long HID item at byte {offset}")]
    TruncatedLongItem {
        /// Byte offset of the item prefix.
        offset: usize,
    },

    /// An end-collection item has no matching collection.
    #[error("end collection without a matching collection at byte {offset}")]
    UnexpectedEndCollection {
        /// Byte offset of the end-collection item.
        offset: usize,
    },

    /// The descriptor ended while a collection was still open.
    #[error("unclosed HID collection")]
    UnclosedCollection,
}

#[derive(Debug)]
struct CollectionFrame {
    usage_page: u16,
    usage: u16,
    report_ids: BTreeSet<u8>,
}

/// Parses a HID report descriptor.
///
/// The parser tracks global usage pages and report IDs, local usages, and the
/// collection stack. Unsupported or reserved items are skipped by their declared
/// length so they cannot desynchronise the stream.
///
/// # Errors
///
/// Returns [`DescriptorError`] when an item is truncated, an end-collection item
/// has no matching collection, or a collection is left open at the end.
pub fn parse_report_descriptor(data: &[u8]) -> Result<HidDescriptor, DescriptorError> {
    let mut offset = 0;
    let mut usage_page = 0_u16;
    let mut report_id = None;
    let mut global_stack: Vec<(u16, Option<u8>)> = Vec::new();
    let mut local_usages: Vec<(u16, u16)> = Vec::new();
    let mut frames: Vec<CollectionFrame> = Vec::new();
    let mut collections = Vec::new();

    while offset < data.len() {
        let start = offset;
        let prefix = data[offset];

        if prefix == LONG_ITEM_PREFIX {
            let header_end = offset
                .checked_add(3)
                .ok_or(DescriptorError::TruncatedLongItem { offset: start })?;
            if header_end > data.len() {
                return Err(DescriptorError::TruncatedLongItem { offset: start });
            }
            let data_size = usize::from(data[offset + 1]);
            offset = header_end
                .checked_add(data_size)
                .ok_or(DescriptorError::TruncatedLongItem { offset: start })?;
            if offset > data.len() {
                return Err(DescriptorError::TruncatedLongItem { offset: start });
            }
            continue;
        }

        let data_size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let item_end = start
            .checked_add(1 + data_size)
            .ok_or(DescriptorError::TruncatedShortItem { offset: start })?;
        if item_end > data.len() {
            return Err(DescriptorError::TruncatedShortItem { offset: start });
        }
        let value = read_le(&data[start + 1..item_end]);
        let item_type = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        offset = item_end;

        match item_type {
            ITEM_TYPE_GLOBAL => match tag {
                GLOBAL_USAGE_PAGE => usage_page = u16::try_from(value).unwrap_or(0),
                GLOBAL_REPORT_ID => {
                    report_id = u8::try_from(value).ok().filter(|id| *id != 0);
                }
                GLOBAL_PUSH => global_stack.push((usage_page, report_id)),
                GLOBAL_POP => {
                    if let Some((saved_page, saved_report_id)) = global_stack.pop() {
                        usage_page = saved_page;
                        report_id = saved_report_id;
                    }
                }
                _ => {}
            },
            ITEM_TYPE_LOCAL => {
                if tag == LOCAL_USAGE {
                    if data_size == 4 {
                        local_usages.push(((value >> 16) as u16, (value & 0xFFFF) as u16));
                    } else {
                        local_usages.push((usage_page, u16::try_from(value).unwrap_or(0)));
                    }
                }
            }
            ITEM_TYPE_MAIN => {
                match tag {
                    MAIN_COLLECTION => {
                        let (collection_page, collection_usage) =
                            local_usages.first().copied().unwrap_or((usage_page, 0));
                        frames.push(CollectionFrame {
                            usage_page: collection_page,
                            usage: collection_usage,
                            report_ids: BTreeSet::new(),
                        });
                    }
                    MAIN_END_COLLECTION => {
                        let Some(frame) = frames.pop() else {
                            return Err(DescriptorError::UnexpectedEndCollection { offset: start });
                        };
                        if frames.is_empty() {
                            collections.push(HidCollection {
                                usage_page: frame.usage_page,
                                usage: frame.usage,
                                report_ids: frame.report_ids,
                            });
                        }
                    }
                    MAIN_FEATURE => {
                        if let (Some(frame), Some(id)) = (frames.first_mut(), report_id) {
                            frame.report_ids.insert(id);
                        }
                    }
                    _ => {}
                }
                // Local items apply to exactly one main item.
                local_usages.clear();
            }
            _ => {}
        }
    }

    if frames.is_empty() {
        Ok(HidDescriptor { collections })
    } else {
        Err(DescriptorError::UnclosedCollection)
    }
}

fn read_le(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .enumerate()
        .fold(0_u32, |value, (index, byte)| {
            value | (u32::from(*byte) << (8 * index))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_descriptor() -> Vec<u8> {
        vec![
            0x06,
            0x01,
            0xFF, // Usage Page (0xFF01)
            0x09,
            0x01, // Usage (1)
            0xA1,
            0x01, // Collection (Application)
            0x85,
            CONFIG_REPORT_ID, // Report ID (7)
            0x15,
            0x00, // Logical Minimum (0)
            0x26,
            0xFF,
            0x00, // Logical Maximum (255)
            0x09,
            0x20, // Usage (0x20)
            0x75,
            0x08, // Report Size (8)
            0x96,
            0x07,
            0x01, // Report Count (263)
            0xB1,
            0x02, // Feature (Data, Variable, Absolute)
            0xC0, // End Collection
        ]
    }

    #[test]
    fn config_interface_matches_the_verified_descriptor() {
        assert_eq!(InterfaceDescriptor::MK61_CONFIG.usage_page, 0xFF01);
        assert_eq!(InterfaceDescriptor::MK61_CONFIG.usage, 0x0001);
        assert_eq!(CONFIG_REPORT_ID, 0x07);
    }

    #[test]
    fn parses_the_config_collection_and_report_id() {
        let descriptor = parse_report_descriptor(&config_descriptor()).unwrap();

        assert_eq!(descriptor.collections.len(), 1);
        let collection = descriptor.find(InterfaceDescriptor::MK61_CONFIG).unwrap();
        assert_eq!(collection.usage_page, 0xFF01);
        assert_eq!(collection.usage, 0x0001);
        assert!(collection.report_ids.contains(&CONFIG_REPORT_ID));
        assert!(descriptor.is_mk61_config());
        assert_eq!(descriptor.mk61_report_id(), Some(CONFIG_REPORT_ID));
    }

    #[test]
    fn parses_multibyte_items_and_nested_collections() {
        let descriptor = [
            0x06, 0x01, 0xFF, // Usage Page (0xFF01), 2-byte item
            0x09, 0x01, // Usage (1)
            0xA1, 0x01, // Collection (Application)
            0xA1, 0x00, // Collection (Physical)
            0x85, 0x07, // Report ID (7)
            0x96, 0x07, 0x01, // Report Count (263), 2-byte data
            0xB1, 0x02, // Feature
            0xC0, 0xC0, // End Collection, End Collection
        ];

        let parsed = parse_report_descriptor(&descriptor).unwrap();
        assert!(parsed.is_mk61_config());
        assert_eq!(parsed.mk61_report_id(), Some(7));
    }

    #[test]
    fn parses_extended_usages_with_an_embedded_usage_page() {
        let descriptor = [
            0x0B, 0x01, 0x00, 0x01, 0xFF, // Usage (0xFF01_0001), 4-byte item
            0xA1, 0x01, // Collection
            0x85, 0x07, 0xB1, 0x02, // Report ID, Feature
            0xC0,
        ];

        let parsed = parse_report_descriptor(&descriptor).unwrap();
        let collection = parsed.find(InterfaceDescriptor::MK61_CONFIG).unwrap();
        assert_eq!(collection.usage, 0x0001);
        assert_eq!(
            collection.report_ids.iter().copied().collect::<Vec<_>>(),
            [7]
        );
    }

    #[test]
    fn skips_long_items_without_desynchronising() {
        let mut descriptor = vec![
            0xFE, 0x03, 0x10, 0xAA, 0xBB, 0xCC, // Long item, 3 data bytes
            0x04, 0x00, // Unrelated global item
        ];
        descriptor.extend(config_descriptor());

        let parsed = parse_report_descriptor(&descriptor).unwrap();
        assert!(parsed.is_mk61_config());
    }

    #[test]
    fn rejects_truncated_short_items() {
        assert_eq!(
            parse_report_descriptor(&[0x06, 0x01]),
            Err(DescriptorError::TruncatedShortItem { offset: 0 })
        );
    }

    #[test]
    fn rejects_truncated_long_items() {
        assert_eq!(
            parse_report_descriptor(&[0xFE, 0x05, 0x01, 0x00]),
            Err(DescriptorError::TruncatedLongItem { offset: 0 })
        );
    }

    #[test]
    fn rejects_unbalanced_collections() {
        assert_eq!(
            parse_report_descriptor(&[0xC0]),
            Err(DescriptorError::UnexpectedEndCollection { offset: 0 })
        );
        assert_eq!(
            parse_report_descriptor(&[0xA1, 0x01]),
            Err(DescriptorError::UnclosedCollection)
        );
    }

    #[test]
    fn treats_other_collections_as_non_config() {
        let descriptor = [
            0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0xB1, 0x02, 0xC0,
        ];

        let parsed = parse_report_descriptor(&descriptor).unwrap();
        assert!(!parsed.is_mk61_config());
        assert_eq!(parsed.mk61_report_id(), None);
    }

    #[test]
    fn empty_descriptor_has_no_collections() {
        let parsed = parse_report_descriptor(&[]).unwrap();
        assert!(parsed.collections.is_empty());
        assert!(!parsed.is_mk61_config());
    }

    #[test]
    fn config_without_report_id_is_still_the_vendor_collection() {
        let descriptor = [
            0x06, 0x01, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x09, 0x20, 0x75, 0x08, 0x95, 0x01, 0xB1,
            0x02, 0xC0,
        ];

        let parsed = parse_report_descriptor(&descriptor).unwrap();
        assert!(parsed.is_mk61_config());
        assert_eq!(parsed.mk61_report_id(), None);
    }

    #[test]
    fn falls_back_to_another_feature_report_id() {
        let descriptor = [
            0x06, 0x01, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x09, 0xB1, 0x02, 0xC0,
        ];

        let parsed = parse_report_descriptor(&descriptor).unwrap();
        assert_eq!(parsed.mk61_report_id(), Some(9));
    }
}
