//! BridgedDeviceBasicInformation Cluster (0x0039) handler.
//!
//! Provides device identification and endpoint names for Matter bridges.
//! Controllers like Home Assistant read these attributes to display bridged device info.

use rs_matter::dm::{
    Access, Attribute, Cluster, Dataver, Handler, NonBlockingHandler, ReadContext, ReadReply,
    Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::TLVWrite;
use rs_matter::{attribute_enum, attributes, with};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use strum::FromRepr;

/// Matter Cluster ID for BridgedDeviceBasicInformation
pub const CLUSTER_ID: u32 = 0x0039;

/// Cluster revision
const CLUSTER_REVISION: u16 = 4;

/// Attribute IDs for the BridgedDeviceBasicInformation cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum BridgedDeviceBasicInfoAttribute {
    /// Vendor name - manufacturer name (e.g., "Aqara")
    VendorName = 0x0001,
    /// Product name - model name (e.g., "Climate Sensor W100")
    ProductName = 0x0003,
    /// Node label - user-friendly name for the endpoint
    NodeLabel = 0x0005,
    /// Hardware version
    HardwareVersion = 0x0007,
    /// Software version
    SoftwareVersion = 0x0009,
    /// Serial number - unique identifier
    SerialNumber = 0x000F,
    /// Reachable - whether the bridged device is reachable
    Reachable = 0x0011,
}

attribute_enum!(BridgedDeviceBasicInfoAttribute);

/// Cluster metadata definition
const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: 0,
    attributes: attributes!(
        // VendorName: optional, read-only string
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::VendorName as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // ProductName: optional, read-only string
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::ProductName as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // NodeLabel: optional, read-only string
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::NodeLabel as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // HardwareVersion: optional, read-only u16
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::HardwareVersion as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // SoftwareVersion: optional, read-only u32
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::SoftwareVersion as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // SerialNumber: optional, read-only string
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::SerialNumber as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
        // Reachable: mandatory, read-only bool
        Attribute::new(
            BridgedDeviceBasicInfoAttribute::Reachable as _,
            Access::RV,
            rs_matter::dm::Quality::NONE
        ),
    ),
    commands: &[],
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Device information for bridged devices.
///
/// REUSABLE across ALL bridged devices in the platform.
/// Use the builder pattern to set optional fields.
#[derive(Clone, Debug)]
pub struct BridgedDeviceInfo {
    /// Vendor name (e.g., "Aqara")
    pub vendor_name: Option<&'static str>,
    /// Product name (e.g., "Climate Sensor W100")
    pub product_name: Option<&'static str>,
    /// Node label - user-friendly name
    pub node_label: &'static str,
    /// Hardware version
    pub hardware_version: Option<u16>,
    /// Software version
    pub software_version: Option<u32>,
    /// Serial number (e.g., IEEE address)
    pub serial_number: Option<&'static str>,
}

impl BridgedDeviceInfo {
    /// Create device info with just a node label.
    pub fn new(node_label: &'static str) -> Self {
        Self {
            vendor_name: None,
            product_name: None,
            node_label,
            hardware_version: None,
            software_version: None,
            serial_number: None,
        }
    }

    /// Set the vendor name (e.g., "Aqara").
    pub fn with_vendor(mut self, name: &'static str) -> Self {
        self.vendor_name = Some(name);
        self
    }

    /// Set the product name (e.g., "Climate Sensor W100").
    pub fn with_product(mut self, name: &'static str) -> Self {
        self.product_name = Some(name);
        self
    }

    /// Set the hardware version.
    pub fn with_hardware_version(mut self, version: u16) -> Self {
        self.hardware_version = Some(version);
        self
    }

    /// Set the software version.
    pub fn with_software_version(mut self, version: u32) -> Self {
        self.software_version = Some(version);
        self
    }

    /// Set the serial number (e.g., IEEE address).
    pub fn with_serial_number(mut self, serial: &'static str) -> Self {
        self.serial_number = Some(serial);
        self
    }
}

/// Handler for BridgedDeviceBasicInformation cluster.
///
/// Provides device identification via VendorName, ProductName, and other attributes.
/// Controllers like Home Assistant read these to display bridged device info.
#[derive(Clone, Debug)]
pub struct BridgedHandler {
    dataver: Dataver,
    /// Device information
    info: BridgedDeviceInfo,
    /// Dynamic reachable state (shared with parent DeviceSwitch)
    reachable: Arc<AtomicBool>,
}

impl BridgedHandler {
    /// Cluster definition for use in the data model
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler with device info and reachable state.
    pub fn new(dataver: Dataver, info: BridgedDeviceInfo, reachable: Arc<AtomicBool>) -> Self {
        Self {
            dataver,
            info,
            reachable,
        }
    }

    /// Create a new handler that is always reachable (for parent endpoints).
    pub fn new_always_reachable(dataver: Dataver, info: BridgedDeviceInfo) -> Self {
        Self {
            dataver,
            info,
            reachable: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Create a new handler with just a name (backwards compatible).
    pub fn new_with_name(dataver: Dataver, name: &'static str, reachable: Arc<AtomicBool>) -> Self {
        Self {
            dataver,
            info: BridgedDeviceInfo::new(name),
            reachable,
        }
    }

    /// Create a new handler with just a name, always reachable (backwards compatible).
    pub fn new_with_name_always_reachable(dataver: Dataver, name: &'static str) -> Self {
        Self {
            dataver,
            info: BridgedDeviceInfo::new(name),
            reachable: Arc::new(AtomicBool::new(true)),
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();

        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(()); // No update needed
        };

        // Global/system attributes (ClusterRevision, FeatureMap, AttributeList, etc.)
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        let tag = writer.tag();
        {
            let mut tw = writer.writer();

            match attr.attr_id.try_into()? {
                BridgedDeviceBasicInfoAttribute::VendorName => {
                    if let Some(vendor) = self.info.vendor_name {
                        tw.utf8(tag, vendor)?;
                    } else {
                        tw.utf8(tag, "")?;
                    }
                }
                BridgedDeviceBasicInfoAttribute::ProductName => {
                    if let Some(product) = self.info.product_name {
                        tw.utf8(tag, product)?;
                    } else {
                        tw.utf8(tag, "")?;
                    }
                }
                BridgedDeviceBasicInfoAttribute::NodeLabel => {
                    tw.utf8(tag, self.info.node_label)?;
                }
                BridgedDeviceBasicInfoAttribute::HardwareVersion => {
                    tw.u16(tag, self.info.hardware_version.unwrap_or(0))?;
                }
                BridgedDeviceBasicInfoAttribute::SoftwareVersion => {
                    tw.u32(tag, self.info.software_version.unwrap_or(0))?;
                }
                BridgedDeviceBasicInfoAttribute::SerialNumber => {
                    if let Some(serial) = self.info.serial_number {
                        tw.utf8(tag, serial)?;
                    } else {
                        tw.utf8(tag, "")?;
                    }
                }
                BridgedDeviceBasicInfoAttribute::Reachable => {
                    tw.bool(tag, self.reachable.load(Ordering::SeqCst))?;
                }
            }
        }

        writer.complete()
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // Cluster is read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }
}

impl Handler for BridgedHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }
}

impl NonBlockingHandler for BridgedHandler {}
