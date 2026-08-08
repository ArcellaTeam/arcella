// arcella/arcella-broker/src/protocol/mod.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use bytes::{Buf, BufMut, Bytes};
use std::str;
use thiserror::Error;

// ============================================================================
// Protocol constants
// ============================================================================

/// Current protocol version
pub const PROTOCOL_VERSION: u16 = 1;

/// Maximum message type size (bytes)
pub const MAX_MSG_TYPE_LEN: usize = 255;

/// Maximum recipient address size (bytes)
pub const MAX_ADDRESS_LEN: usize = 1024;

/// Maximum payload size (1 MB)
pub const MAX_PAYLOAD_LEN: u32 = 1_048_576;

/// Token size (32 bytes)
pub const SESSION_TOKEN_LEN: usize = 32;

/// Message ID size (16 bytes)
pub const MESSAGE_ID_LEN: usize = 16;

/// Submessage ID size (4 bytes)
pub const SUB_MESSAGE_ID_LEN: usize = 4;

/// Size of the fixed header in bytes
/// 2 (version) + 2 (flags) + 32 (token) + 16 (guid) + 4 (sub_id)+ 1 (ttl) 
/// + 1 (type_len) + 2 (addr_len) + 4 (payload_len) = 64 bytes
pub const FIXED_HEADER_SIZE: usize = 64;

// ============================================================================
// Bit masks for the flags field
// ============================================================================

/// Mask for extracting the transfer mode from the flags field (bit 0)
pub const TRANSFER_MODE_MASK: u16 = 0x0001;

/// Shift for the transfer mode in the flags field
pub const TRANSFER_MODE_SHIFT: u16 = 0;

// Reserved bits for future extensions:
// Bits 1-15 are reserved and must be 0 in the current protocol version

// ============================================================================
// Protocol errors
// ============================================================================

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("Insufficient data to read header")]
    IncompleteHeader,

    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u16),

    #[error("Unknown transfer mode: {0}")]
    UnknownTransferMode(u16),

    #[error("Message type length ({0}) exceeds limit {MAX_MSG_TYPE_LEN}")]
    MsgTypeTooLong(u8),

    #[error("Address length ({0}) exceeds limit {MAX_ADDRESS_LEN}")]
    AddressTooLong(u16),

    #[error("Payload size ({0}) exceeds limit {MAX_PAYLOAD_LEN}")]
    PayloadTooLarge(u32),

    #[error("Insufficient data to read message type")]
    IncompleteMsgType,

    #[error("Insufficient data to read address")]
    IncompleteAddress,

    #[error("Insufficient data to read payload")]
    IncompletePayload,

    #[error("Invalid UTF-8 in message type")]
    InvalidMsgTypeUtf8,

    #[error("Invalid UTF-8 in address")]
    InvalidAddressUtf8,

    #[error("Invalid address format: {0}")]
    InvalidAddressFormat(String),

    #[error("Empty level in address (double colon)")]
    EmptyAddressLevel,
}

// ============================================================================
// Transfer mode (Flags)
// ============================================================================

/// Message transfer mode, defining the interaction semantics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum TransferMode {
    /// Asynchronous send without waiting for a response (Tell, fire-and-forget)
    InOnly = 0,
    /// Send with waiting for a response  (Ask, request/response)
    InOut = 1,
}

impl TransferMode {

    /// Extracts the transfer mode the from flags field (uses only bit 0)
    pub fn from_flags(flags: u16) -> Result<Self, ProtocolError> {
        let mode_bits = flags & TRANSFER_MODE_MASK;
        
        match mode_bits {
            0 => Ok(Self::InOnly),
            1 => Ok(Self::InOut),
            // Protection against possible future changes
            _ => Err(ProtocolError::UnknownTransferMode(mode_bits)),
        }
    }

    /// Converts the transfer mode to the flags field value
    pub fn to_flags(self) -> u16 {
        (self as u16) << TRANSFER_MODE_SHIFT
    }
}

// ============================================================================
// Fixed header
// ============================================================================

/// Fixed part of the message header (64 bytes)
/// 
/// Structure (all numbers in Little-Endian):
/// - version: u16 — protocol version
/// - flags: u16 — flags (transfer mode)
/// - session_token: [u8; SESSION_TOKEN_LEN] — session token (SHA-256/BLAKE3)
/// - message_id: [u8; MESSAGE_ID_LEN] — unique message identifier (GUID)
/// - sub_message_id: [u8; SUB_MESSAGE_ID_LEN] — unique submessage identifier (GUID)
/// - ttl: u8 — routing counter (Time To Live)
/// - msg_type_len: u8 — message type length
/// - address_len: u16 — recipient address length
/// - payload_len: u32 — payload length
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedHeader {
    pub version: u16,
    pub flags: u16,
    pub session_token: [u8; SESSION_TOKEN_LEN],
    pub message_id: [u8; MESSAGE_ID_LEN],
    pub sub_message_id: [u8; SUB_MESSAGE_ID_LEN],
    pub ttl: u8,
    pub msg_type_len: u8,
    pub address_len: u16,
    pub payload_len: u32,
}

impl FixedHeader {
    /// Creates a new fixed header with validation
    pub fn new(
        mode: TransferMode,
        session_token: [u8; SESSION_TOKEN_LEN],
        message_id: [u8; MESSAGE_ID_LEN],
        sub_message_id: [u8; SUB_MESSAGE_ID_LEN],
        ttl: u8,
        msg_type_len: u8,
        address_len: u16,
        payload_len: u32,
    ) -> Result<Self, ProtocolError> {
        // Length validation
        if msg_type_len as usize > MAX_MSG_TYPE_LEN {
            return Err(ProtocolError::MsgTypeTooLong(msg_type_len));
        }
        if address_len as usize > MAX_ADDRESS_LEN {
            return Err(ProtocolError::AddressTooLong(address_len));
        }
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(ProtocolError::PayloadTooLarge(payload_len));
        }
        
        let flags = mode.to_flags();

        Ok(Self {
            version: PROTOCOL_VERSION,
            flags: flags,
            session_token,
            message_id,
            sub_message_id,
            ttl,
            msg_type_len,
            address_len,
            payload_len,
        })
    }

    /// Decodes the fixed header from a buffer
    pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, ProtocolError> {
        if buf.remaining() < FIXED_HEADER_SIZE {
            return Err(ProtocolError::IncompleteHeader);
        }

        let version = buf.get_u16_le();
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }

        let flags = buf.get_u16_le();
        // Validate transfer mode immediately during parsing
        TransferMode::from_flags(flags)?;

        let mut session_token = [0u8; SESSION_TOKEN_LEN];
        buf.copy_to_slice(&mut session_token);

        let mut message_id = [0u8; MESSAGE_ID_LEN];
        buf.copy_to_slice(&mut message_id);

        let mut sub_message_id = [0u8; SUB_MESSAGE_ID_LEN];
        buf.copy_to_slice(&mut sub_message_id);

        let ttl = buf.get_u8();
        let msg_type_len = buf.get_u8();
        let address_len = buf.get_u16_le();
        let payload_len = buf.get_u32_le();

        // Length validation after reading
        if msg_type_len as usize > MAX_MSG_TYPE_LEN {
            return Err(ProtocolError::MsgTypeTooLong(msg_type_len));
        }
        if address_len as usize > MAX_ADDRESS_LEN {
            return Err(ProtocolError::AddressTooLong(address_len));
        }
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(ProtocolError::PayloadTooLarge(payload_len));
        }

        Ok(Self {
            version,
            flags,
            session_token,
            message_id,
            sub_message_id,
            ttl,
            msg_type_len,
            address_len,
            payload_len,
        })
    }

    /// Encodes the fixed header into a buffer
    pub fn encode<B: BufMut>(&self, buf: &mut B) {
        buf.put_u16_le(self.version);
        buf.put_u16_le(self.flags);
        buf.put_slice(&self.session_token);
        buf.put_slice(&self.message_id);
        buf.put_u8(self.ttl);
        buf.put_u8(self.msg_type_len);
        buf.put_u16_le(self.address_len);
        buf.put_u32_le(self.payload_len);
    }

    /// Returns the transfer mode
    pub fn transfer_mode(&self) -> Result<TransferMode, ProtocolError> {
        TransferMode::from_flags(self.flags)
    }

    /// Returns the total size of the variable part of the header
    pub fn variable_header_size(&self) -> usize {
        self.msg_type_len as usize + self.address_len as usize
    }

    /// Returns the total size of the entire message (header + variable part + payload)
    pub fn total_message_size(&self) -> usize {
        FIXED_HEADER_SIZE + self.variable_header_size() + self.payload_len as usize
    }
}

// ============================================================================
// Address validation
// ============================================================================

/// Validates the recipient address format
/// 
/// Rules:
/// - Only allowed characters: a-zA-Z0-9, -, _, :
/// - Empty levels (::) are forbidden
/// - Address cannot start or end with ':'
pub fn validate_address(address: &str) -> Result<(), ProtocolError> {
    if address.is_empty() {
        return Err(ProtocolError::InvalidAddressFormat(
            "Address cannot be empty".to_string(),
        ));
    }

    // Check for empty levels (::) and leading/trailing ':'
    if address.starts_with(':') || address.ends_with(':') || address.contains("::") {
        return Err(ProtocolError::EmptyAddressLevel);
    }

    // Byte-level check (all allowed characters are ASCII)
    let is_valid = address.as_bytes().iter().all(|b| {
        b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_' || *b == b':'
    });	

    if !is_valid {
        return Err(ProtocolError::InvalidAddressFormat(
            "Invalid character in address".to_string(),
        ));
    }

    Ok(())
}

// ============================================================================
// Full message
// ============================================================================

/// Full microbroker message
/// 
/// Consists of:
/// 1. Fixed header (60 bytes)
/// 2. Message type (UTF-8 string, < 255 bytes)
/// 3. Recipient address (UTF-8 string, < 1024 bytes)
/// 4. Payload (bincode, transparent to the broker)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub header: FixedHeader,
    pub msg_type: String,
    pub address: String,
    pub payload: Bytes,
}

impl Message {
    /// Creates a new message with validation
    pub fn new(
        mode: TransferMode,
        session_token: [u8; SESSION_TOKEN_LEN],
        message_id: [u8; MESSAGE_ID_LEN],
        sub_message_id: [u8; SUB_MESSAGE_ID_LEN],
        ttl: u8,
        msg_type: String,
        address: String,
        payload: Bytes,
    ) -> Result<Self, ProtocolError> {
        // Address validation
        validate_address(&address)?;

        // Length validation
        let msg_type_bytes = msg_type.as_bytes();
        if msg_type_bytes.len() > MAX_MSG_TYPE_LEN {
            return Err(ProtocolError::MsgTypeTooLong(msg_type_bytes.len() as u8));
        }

        let address_bytes = address.as_bytes();
        if address_bytes.len() > MAX_ADDRESS_LEN {
            return Err(ProtocolError::AddressTooLong(address_bytes.len() as u16));
        }

        let payload_len = payload.len() as u32;
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(ProtocolError::PayloadTooLarge(payload_len));
        }

        let header = FixedHeader::new(
            mode,
            session_token,
            message_id,
            sub_message_id,
            ttl,
            msg_type_bytes.len() as u8,
            address_bytes.len() as u16,
            payload_len,
        )?;

        Ok(Self {
            header,
            msg_type,
            address,
            payload,
        })
    }

    /// Decodes the full message from a buffer
    pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, ProtocolError> {
        // 1. Read the fixed header
        let header = FixedHeader::decode(buf)?;

        // 2. Read the message type
        let msg_type_len = header.msg_type_len as usize;
        if buf.remaining() < msg_type_len {
            return Err(ProtocolError::IncompleteMsgType);
        }
        let msg_type_bytes = buf.copy_to_bytes(msg_type_len);
        let msg_type = str::from_utf8(&msg_type_bytes)
            .map_err(|_| ProtocolError::InvalidMsgTypeUtf8)?
            .to_string();

        // 3. Read the recipient address
        let address_len = header.address_len as usize;
        if buf.remaining() < address_len {
            return Err(ProtocolError::IncompleteAddress);
        }
        let address_bytes = buf.copy_to_bytes(address_len);
        let address = str::from_utf8(&address_bytes)
            .map_err(|_| ProtocolError::InvalidAddressUtf8)?
            .to_string();

        // 4. Validate address
        validate_address(&address)?;

        // 5. Read payload (zero-copy via Bytes)
        let payload_len = header.payload_len as usize;
        if buf.remaining() < payload_len {
            return Err(ProtocolError::IncompletePayload);
        }
        let payload = buf.copy_to_bytes(payload_len);

        Ok(Self {
            header,
            msg_type,
            address,
            payload,
        })
    }

    /// Encodes the full message into a buffer
    pub fn encode<B: BufMut>(&self, buf: &mut B) {
        // 1. Fixed header
        self.header.encode(buf);

        // 2. Message type
        buf.put_slice(self.msg_type.as_bytes());

        // 3. Recipient address
        buf.put_slice(self.address.as_bytes());

        // 4. Payload
        buf.put_slice(&self.payload);
    }

    /// Returns the transfer mode
    pub fn transfer_mode(&self) -> Result<TransferMode, ProtocolError> {
        self.header.transfer_mode()
    }

    /// Returns the total message size in bytes
    pub fn size(&self) -> usize {
        self.header.total_message_size()
    }
}