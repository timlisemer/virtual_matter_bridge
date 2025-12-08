//! Device information for Matter stack.
//!
//! Custom device details replacing rs-matter's test defaults.

use rs_matter::dm::clusters::basic_info::BasicInfoConfig;

/// Device details for Virtual Matter Bridge.
///
/// Replaces `TEST_DEV_DET` with proper naming:
/// - Product name: "Virtual Matter Bridge" (shown as device name in Home Assistant)
/// - Vendor name: "timlisemer" (shown as "by timlisemer")
pub const DEV_INFO: BasicInfoConfig<'static> = BasicInfoConfig {
    vid: 0xFFF1,
    pid: 0x8001,
    hw_ver: 1,
    hw_ver_str: "1",
    sw_ver: 1,
    sw_ver_str: "1.0",
    serial_no: "VMB-001",
    device_name: "VirtualMatterBridge",
    product_name: "Virtual Matter Bridge",
    vendor_name: "timlisemer",
    ..BasicInfoConfig::new()
};
