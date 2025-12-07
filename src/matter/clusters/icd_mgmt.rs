//! ICD Management cluster handler for rs-matter integration.
//!
//! This module implements the Matter ICD Management cluster (0x0046)
//! with Check-In Protocol support (feature_map: 0x01). This enables:
//! - RegisterClient/UnregisterClient commands for controllers to register
//! - Check-In messages to signal controllers after device restart
//!
//! Since this bridge is always connected (not battery-powered), we return
//! values indicating an always-on device while supporting session recovery.

use std::sync::Arc;

use log::{debug, info, warn};
use rs_matter::dm::{
    Access, Attribute, Cluster, Command, Dataver, Handler, InvokeContext, InvokeReply,
    NonBlockingHandler, Quality, ReadContext, ReadReply, Reply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::{TLVTag, TLVWrite};
use rs_matter::{attribute_enum, attributes, command_enum, commands, with};
use strum::FromRepr;

use crate::matter::icd::{IcdClientType, IcdStore};
use crate::matter::subscription_persistence::{PersistedSubscription, SubscriptionStore};

/// ICD Management Cluster ID (Matter spec)
pub const CLUSTER_ID: u32 = 0x0046;

/// Cluster revision
pub const CLUSTER_REVISION: u16 = 3;

/// Attribute IDs for the ICD Management cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum IcdMgmtAttribute {
    /// Idle mode duration in seconds
    IdleModeDuration = 0x0000,
    /// Active mode duration in milliseconds
    ActiveModeDuration = 0x0001,
    /// Active mode threshold in milliseconds
    ActiveModeThreshold = 0x0002,
    /// Registered clients list (optional)
    RegisteredClients = 0x0003,
    /// ICD counter (optional)
    IcdCounter = 0x0004,
    /// Max clients per fabric (optional)
    ClientsSupportedPerFabric = 0x0005,
    /// User active mode trigger hint bitmap
    UserActiveModeTriggerHint = 0x0006,
    /// User active mode trigger instruction string
    UserActiveModeTriggerInstruction = 0x0007,
    /// Operating mode (optional)
    OperatingMode = 0x0008,
    /// Maximum check-in back-off (optional)
    MaximumCheckInBackOff = 0x0009,
}

attribute_enum!(IcdMgmtAttribute);

/// Command IDs for the ICD Management cluster
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromRepr)]
#[repr(u32)]
pub enum IcdMgmtCommand {
    /// Register for Check-In messages
    RegisterClient = 0x00,
    /// Unregister from Check-In messages
    UnregisterClient = 0x02,
    /// Request device to stay active
    StayActiveRequest = 0x03,
}

command_enum!(IcdMgmtCommand);

/// Response command IDs
pub mod response_commands {
    pub const REGISTER_CLIENT_RESPONSE: u32 = 0x01;
    pub const STAY_ACTIVE_RESPONSE: u32 = 0x04;
}

/// Feature bits for ICD Management cluster
pub mod features {
    /// Check-In Protocol Support - enables RegisterClient, UnregisterClient, StayActiveRequest
    pub const CHECK_IN_PROTOCOL_SUPPORT: u32 = 0x01;
}

/// Custom Access flags for ICD Management cluster commands/attributes
/// Since Access bitflags can't be combined in const context with |, we define raw values
pub mod access {
    use rs_matter::dm::Access;

    /// READ + NEED_ADMIN + FAB_SCOPED (for RegisteredClients attribute)
    /// 0x0010 (READ) | 0x0008 (NEED_ADMIN) | 0x0040 (FAB_SCOPED) = 0x0058
    pub const READ_ADMIN_FAB: Access = Access::from_bits_truncate(0x0058);

    /// WRITE + NEED_MANAGE + NEED_ADMIN + FAB_SCOPED (for RegisterClient/UnregisterClient commands)
    /// 0x0020 (WRITE) | 0x0004 (NEED_MANAGE) | 0x0008 (NEED_ADMIN) | 0x0040 (FAB_SCOPED) = 0x006C
    pub const WRITE_MANAGE_FAB: Access = Access::from_bits_truncate(0x006C);
}

/// Build cluster definition with Check-In Protocol support (feature_map: 0x01)
/// This enables RegisterClient/UnregisterClient/StayActiveRequest commands
/// which are required for session recovery after device restart.
pub const CLUSTER: Cluster<'static> = Cluster {
    id: CLUSTER_ID,
    revision: CLUSTER_REVISION,
    feature_map: features::CHECK_IN_PROTOCOL_SUPPORT,
    attributes: attributes!(
        // IdleModeDuration - int32u, read-only, required
        Attribute::new(
            IcdMgmtAttribute::IdleModeDuration as _,
            Access::RV,
            Quality::F // Fixed value
        ),
        // ActiveModeDuration - int32u, read-only, required
        Attribute::new(
            IcdMgmtAttribute::ActiveModeDuration as _,
            Access::RV,
            Quality::F
        ),
        // ActiveModeThreshold - int16u, read-only, required
        Attribute::new(
            IcdMgmtAttribute::ActiveModeThreshold as _,
            Access::RV,
            Quality::F
        ),
        // RegisteredClients - list of MonitoringRegistrationStruct, required with Check-In
        // Access: Read with Administer privilege, fabric-scoped
        Attribute::new(
            IcdMgmtAttribute::RegisteredClients as _,
            access::READ_ADMIN_FAB,
            Quality::A // Array
        ),
        // ICDCounter - monotonic counter, required with Check-In
        Attribute::new(IcdMgmtAttribute::IcdCounter as _, Access::RA, Quality::NONE),
        // ClientsSupportedPerFabric - max clients per fabric, required with Check-In
        Attribute::new(
            IcdMgmtAttribute::ClientsSupportedPerFabric as _,
            Access::RV,
            Quality::F
        ),
        // UserActiveModeTriggerHint - bitmap32, read-only, optional but HA queries it
        Attribute::new(
            IcdMgmtAttribute::UserActiveModeTriggerHint as _,
            Access::RV,
            Quality::F
        ),
        // UserActiveModeTriggerInstruction - string, read-only, optional but HA queries it
        Attribute::new(
            IcdMgmtAttribute::UserActiveModeTriggerInstruction as _,
            Access::RV,
            Quality::F
        ),
    ),
    commands: commands!(
        // RegisterClient -> RegisterClientResponse (fabric-scoped, Manage privilege)
        Command::new(
            IcdMgmtCommand::RegisterClient as _,
            Some(response_commands::REGISTER_CLIENT_RESPONSE),
            access::WRITE_MANAGE_FAB
        ),
        // UnregisterClient -> DefaultSuccess (fabric-scoped, Manage privilege)
        Command::new(
            IcdMgmtCommand::UnregisterClient as _,
            None,
            access::WRITE_MANAGE_FAB
        ),
        // StayActiveRequest -> StayActiveResponse (Manage privilege, not fabric-scoped)
        Command::new(
            IcdMgmtCommand::StayActiveRequest as _,
            Some(response_commands::STAY_ACTIVE_RESPONSE),
            Access::WM
        ),
    ),
    with_attrs: with!(all),
    with_cmds: with!(all),
};

/// Handler for the ICD Management cluster with Check-In Protocol support
///
/// Returns values indicating an always-connected device:
/// - IdleModeDuration: 1 second (minimum, since we're always on)
/// - ActiveModeDuration: 10000ms (we're always active)
/// - ActiveModeThreshold: 5000ms
///
/// Implements RegisterClient/UnregisterClient/StayActiveRequest commands
/// to allow controllers to register for Check-In messages after restart.
pub struct IcdMgmtHandler {
    dataver: Dataver,
    store: Arc<IcdStore>,
    subscription_store: Arc<SubscriptionStore>,
}

impl IcdMgmtHandler {
    /// The cluster definition for this handler
    pub const CLUSTER: Cluster<'static> = CLUSTER;

    /// Create a new handler
    pub fn new(
        dataver: Dataver,
        store: Arc<IcdStore>,
        subscription_store: Arc<SubscriptionStore>,
    ) -> Self {
        Self {
            dataver,
            store,
            subscription_store,
        }
    }

    fn read_impl(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let attr = ctx.attr();
        let fab_idx = attr.fab_idx;

        // Record this controller as an active subscriber for session recovery
        // We save with fab_idx and a placeholder for peer_node_id (we'll update it later)
        // This ensures we know someone is subscribed even if we restart
        if fab_idx > 0 {
            debug!(
                "ICD read from fabric {}, recording for subscription persistence",
                fab_idx
            );
            // Save with minimal info - fab_idx is what we have access to
            // The peer_node_id would require session access which is pub(crate)
            self.subscription_store.add(PersistedSubscription {
                fabric_idx: fab_idx,
                peer_node_id: 0, // Will be updated when we have session info
                subscription_id: 0,
                min_int_secs: 60,
                max_int_secs: 3600,
            });
        }

        // Get the dataver-aware writer
        let Some(mut writer) = reply.with_dataver(self.dataver.get())? else {
            return Ok(()); // No update needed (dataver match)
        };

        // Handle global attributes via the cluster definition
        if attr.is_system() {
            return CLUSTER.read(attr, writer);
        }

        match attr.attr_id.try_into()? {
            IcdMgmtAttribute::IdleModeDuration => {
                // 1 second - we're always connected so this is minimal
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, 1)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::ActiveModeDuration => {
                // 10000ms (10 seconds) - we're always active
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, 10000)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::ActiveModeThreshold => {
                // 5000ms threshold
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u16(tag, 5000)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::UserActiveModeTriggerHint => {
                // 0 = no user trigger hints (we're always on)
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, 0)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::UserActiveModeTriggerInstruction => {
                // Empty string - no instructions needed
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.utf8(tag, "")?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::RegisteredClients => {
                // Return list of registered clients for this fabric
                let clients = self.store.registered_clients_for_fabric(fab_idx);
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_array(tag)?;
                    for client in &clients {
                        tw.start_struct(&TLVTag::Anonymous)?;
                        tw.u64(&TLVTag::Context(1), client.check_in_node_id)?; // CheckInNodeID
                        tw.u64(&TLVTag::Context(2), client.monitored_subject)?; // MonitoredSubject
                        tw.u8(&TLVTag::Context(4), client.client_type as u8)?; // ClientType
                        tw.end_container()?;
                    }
                    tw.end_container()?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::IcdCounter => {
                // Return current ICD counter
                let counter = self.store.icd_counter();
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u32(tag, counter)?;
                }
                writer.complete()
            }
            IcdMgmtAttribute::ClientsSupportedPerFabric => {
                // Return max clients per fabric
                let max_clients = self.store.clients_supported_per_fabric();
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.u16(tag, max_clients)?;
                }
                writer.complete()
            }
            // These attributes are not advertised in our cluster definition
            IcdMgmtAttribute::MaximumCheckInBackOff | IcdMgmtAttribute::OperatingMode => {
                Err(ErrorCode::AttributeNotFound.into())
            }
        }
    }

    fn write_impl(&self, _ctx: impl WriteContext) -> Result<(), Error> {
        // All attributes are read-only
        Err(ErrorCode::UnsupportedAccess.into())
    }

    fn invoke_impl(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        let cmd = ctx.cmd();
        let data = ctx.data();
        let fab_idx = cmd.fab_idx;

        match cmd.cmd_id.try_into()? {
            IcdMgmtCommand::RegisterClient => {
                // Parse RegisterClient request fields
                // Fields: checkInNodeID (u64), monitoredSubject (u64), key (octet_string<16>),
                //         verificationKey (optional octet_string<16>), clientType (enum8)
                let mut seq = data.structure()?;

                let check_in_node_id = seq.scan_ctx(0)?.u64()?;
                let monitored_subject = seq.scan_ctx(1)?.u64()?;

                // Key is 16 bytes
                let key_elem = seq.scan_ctx(2)?;
                let key_bytes = key_elem.octets()?;
                if key_bytes.len() != 16 {
                    warn!(
                        "RegisterClient: invalid key length {} (expected 16)",
                        key_bytes.len()
                    );
                    return Err(ErrorCode::ConstraintError.into());
                }
                let mut shared_key = [0u8; 16];
                shared_key.copy_from_slice(key_bytes);

                // VerificationKey is optional (context tag 3)
                let verification_key = if let Ok(vk_elem) = seq.scan_ctx(3) {
                    let vk_bytes = vk_elem.octets()?;
                    if vk_bytes.len() == 16 {
                        let mut vk = [0u8; 16];
                        vk.copy_from_slice(vk_bytes);
                        Some(vk)
                    } else {
                        None
                    }
                } else {
                    None
                };

                // ClientType (context tag 4)
                let client_type_u8 = seq.scan_ctx(4)?.u8()?;
                let client_type = IcdClientType::from_u8(client_type_u8)?;

                info!(
                    "RegisterClient: fabric={}, nodeId={:016x}, subject={:016x}, type={:?}",
                    fab_idx, check_in_node_id, monitored_subject, client_type
                );

                // Register the client and get current counter
                let icd_counter = self.store.register_client(
                    fab_idx,
                    check_in_node_id,
                    monitored_subject,
                    shared_key,
                    verification_key,
                    client_type,
                );

                self.dataver.changed();

                // Also save to subscription store for mDNS-based recovery
                self.subscription_store.add(PersistedSubscription {
                    fabric_idx: fab_idx,
                    peer_node_id: check_in_node_id,
                    subscription_id: 0,
                    min_int_secs: 60,
                    max_int_secs: 3600,
                });

                // Send RegisterClientResponse: ICDCounter (u32)
                let mut writer = reply.with_command(response_commands::REGISTER_CLIENT_RESPONSE)?;
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u32(&TLVTag::Context(0), icd_counter)?; // ICDCounter
                    tw.end_container()?;
                }
                writer.complete()
            }
            IcdMgmtCommand::UnregisterClient => {
                // Parse UnregisterClient request fields
                // Fields: checkInNodeID (u64), verificationKey (optional octet_string<16>)
                let mut seq = data.structure()?;

                let check_in_node_id = seq.scan_ctx(0)?.u64()?;

                // VerificationKey is optional (context tag 1)
                let verification_key = if let Ok(vk_elem) = seq.scan_ctx(1) {
                    let vk_bytes = vk_elem.octets()?;
                    if vk_bytes.len() == 16 {
                        let mut vk = [0u8; 16];
                        vk.copy_from_slice(vk_bytes);
                        Some(vk)
                    } else {
                        None
                    }
                } else {
                    None
                };

                info!(
                    "UnregisterClient: fabric={}, nodeId={:016x}",
                    fab_idx, check_in_node_id
                );

                let removed =
                    self.store
                        .unregister_client(fab_idx, check_in_node_id, verification_key);

                if removed {
                    self.dataver.changed();
                    // Also remove from subscription store
                    self.subscription_store.remove(fab_idx, check_in_node_id);
                }

                // Returns DefaultSuccess (no response payload)
                Ok(())
            }
            IcdMgmtCommand::StayActiveRequest => {
                // Parse StayActiveRequest request fields
                // Fields: stayActiveDuration (u32 ms)
                let mut seq = data.structure()?;

                let stay_active_duration = seq.scan_ctx(0)?.u32()?;

                info!(
                    "StayActiveRequest: fabric={}, duration={}ms",
                    fab_idx, stay_active_duration
                );

                // Apply policy bounds (max 30 seconds for this implementation)
                let max_duration: u32 = 30000;
                let promised_duration = self
                    .store
                    .stay_active_until(fab_idx, stay_active_duration.min(max_duration));

                // Send StayActiveResponse: promisedActiveDuration (u32 ms)
                let mut writer = reply.with_command(response_commands::STAY_ACTIVE_RESPONSE)?;
                let tag = writer.tag();
                {
                    let mut tw = writer.writer();
                    tw.start_struct(tag)?;
                    tw.u32(&TLVTag::Context(0), promised_duration)?; // PromisedActiveDuration
                    tw.end_container()?;
                }
                writer.complete()
            }
        }
    }
}

impl Handler for IcdMgmtHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        self.read_impl(ctx, reply)
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        self.write_impl(ctx)
    }

    fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        self.invoke_impl(ctx, reply)
    }
}

impl NonBlockingHandler for IcdMgmtHandler {}
