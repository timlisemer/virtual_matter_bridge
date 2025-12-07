# ICD Check-In design notes

This bridge implements the Matter ICD Management cluster (0x0046) with Check-In protocol support to recover sessions after restart. Key points pulled from CSA 1.4 and Silicon Labs ICD guidance:

- **RegisterClient command** fields: `checkInNodeID` (controller node), `monitoredSubject` (scope, often fabric-scoped), `key` (16-byte shared secret), optional `verificationKey`, and `clientType` (Permanent or Ephemeral). Fabric-scoped access with Manage privilege.
- **RegisteredClients attribute** returns `MonitoringRegistrationStruct` items containing `checkInNodeID`, `monitoredSubject`, `clientType`, and `fabricIndex`.
- **ICDCounter** is a monotonic 32-bit value persisted across reboots. It is incremented for every Check-In transmission and returned in RegisterClientResponse.
- **Check-In message** is sessionless: encrypted with the shared secret using AES-CCM; nonce derived from the counter and fabric context to prevent replay. Payload signals availability; controllers re-establish CASE/subscriptions on receipt.
- **StayActiveRequest** allows controllers to keep the device in Active mode longer; we accept and echo a promised duration (bounded by policy).
- **MaximumCheckInBackOff** limits the retry/announce rate when check-ins fail; default is a conservative 900s back-off for this bridge.

Rust state model used here:

- `IcdClientType`: Permanent or Ephemeral.
- `IcdRegisteredClient`: fabric index, controller node ID, monitored subject, shared key, optional verification key, client type, and optional `stay_active_until`.
- `IcdCheckInState`: persisted `icd_counter` and a vec of registered clients (scoped per fabric).
- `IcdStore`: loads/saves state to disk, exposes helpers for the cluster handler, and notifies the check-in engine when clients or counters change.

Runtime flow:

1. Load persisted ICD state at startup alongside fabrics.
2. When controllers invoke RegisterClient/Unregister/StayActiveRequest, the handler updates `IcdStore`, bumps dataver, and schedules persistence.
3. After boot (and after fabric restoration), the check-in engine iterates registered clients, emits a sessionless check-in (placeholder send in this implementation), and bumps the counter per attempt with back-off enforcement.

Testing ideas:

- Register a client via chip-tool or HA; verify `RegisteredClients` and `ICDCounter` readouts.
- Restart bridge; ensure counter persists and startup logs show check-in attempts to the prior controller.
- Issue StayActiveRequest and confirm promised duration response and log entry.
