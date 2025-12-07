//! Logging wrapper for UDP socket to debug Matter transport.
//!
//! This module provides a wrapper around the async UDP socket that logs
//! all incoming and outgoing packets for debugging commissioning issues.

use std::net::UdpSocket;

use async_io::Async;
use log::{debug, error, trace};

use rs_matter::error::{Error, ErrorCode};
use rs_matter::transport::network::{Address, NetworkReceive, NetworkSend};

/// A wrapper around an async UDP socket that logs all packets.
pub struct LoggingUdpSocket<'a> {
    inner: &'a Async<UdpSocket>,
}

impl<'a> LoggingUdpSocket<'a> {
    /// Create a new logging wrapper around the given socket.
    pub fn new(socket: &'a Async<UdpSocket>) -> Self {
        Self { inner: socket }
    }
}

impl NetworkSend for LoggingUdpSocket<'_> {
    async fn send_to(&mut self, data: &[u8], addr: Address) -> Result<(), Error> {
        debug!("[UDP TX] {} bytes to {}", data.len(), addr);

        // Log first 64 bytes of payload at trace level
        let preview_len = data.len().min(64);
        trace!("[UDP TX] payload: {:02x?}", &data[..preview_len]);

        // Forward to inner socket - extract the UDP socket address
        let socket_addr = addr.udp().ok_or(ErrorCode::NoNetworkInterface)?;
        Async::<UdpSocket>::send_to(self.inner, data, socket_addr).await?;

        Ok(())
    }
}

impl NetworkReceive for LoggingUdpSocket<'_> {
    async fn wait_available(&mut self) -> Result<(), Error> {
        trace!("[UDP] Waiting for packet...");
        Async::<UdpSocket>::readable(self.inner).await?;
        Ok(())
    }

    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, Address), Error> {
        let result = Async::<UdpSocket>::recv_from(self.inner, buffer).await;

        match &result {
            Ok((len, addr)) => {
                debug!("[UDP RX] {} bytes from {}", len, addr);

                // Log first 64 bytes of payload at trace level
                let preview_len = (*len).min(64);
                trace!("[UDP RX] payload: {:02x?}", &buffer[..preview_len]);
            }
            Err(e) => {
                error!("[UDP RX] Error receiving packet: {:?}", e);
            }
        }

        let (len, addr) = result?;
        Ok((len, Address::Udp(addr)))
    }
}

// Implement traits for &LoggingUdpSocket to match rs-matter's &Async<UdpSocket> pattern
impl NetworkSend for &LoggingUdpSocket<'_> {
    async fn send_to(&mut self, data: &[u8], addr: Address) -> Result<(), Error> {
        debug!("[UDP TX] {} bytes to {}", data.len(), addr);

        // Log first 64 bytes of payload at trace level
        let preview_len = data.len().min(64);
        trace!("[UDP TX] payload: {:02x?}", &data[..preview_len]);

        // Forward to inner socket - extract the UDP socket address
        let socket_addr = addr.udp().ok_or(ErrorCode::NoNetworkInterface)?;
        Async::<UdpSocket>::send_to(self.inner, data, socket_addr).await?;

        Ok(())
    }
}

impl NetworkReceive for &LoggingUdpSocket<'_> {
    async fn wait_available(&mut self) -> Result<(), Error> {
        trace!("[UDP] wait_available called, waiting for packet...");
        Async::<UdpSocket>::readable(self.inner).await?;
        trace!("[UDP] wait_available: socket is readable!");
        Ok(())
    }

    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, Address), Error> {
        let result = Async::<UdpSocket>::recv_from(self.inner, buffer).await;

        match &result {
            Ok((len, addr)) => {
                debug!("[UDP RX] {} bytes from {}", len, addr);

                // Log first 64 bytes of payload at trace level
                let preview_len = (*len).min(64);
                trace!("[UDP RX] payload: {:02x?}", &buffer[..preview_len]);
            }
            Err(e) => {
                error!("[UDP RX] Error receiving packet: {:?}", e);
            }
        }

        let (len, addr) = result?;
        Ok((len, Address::Udp(addr)))
    }
}
