// arcella/arcella-broker/src/client/mod.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;

pub mod registry;
pub mod in_memory;

use crate::protocol::Message;
use crate::transport::{Transport, TransportError, TransportResult};
use registry::LocalRegistry;
use in_memory::InMemoryTransport;

pub struct BrokerClient {
    registry: Arc<LocalRegistry>,
    local: InMemoryTransport,
    // remote: Option<IpcTransport>,  // Will be added in stage 2
}

impl BrokerClient {
    pub fn new(registry: Arc<LocalRegistry>) -> Self {
        let (local, _incoming_tx) = InMemoryTransport::new(registry.clone());
        Self { registry, local }
    }
    
    /// Register itself as a receiver at the specified address.
    pub async fn bind(&self, address: String, incoming_tx: tokio::sync::mpsc::Sender<Message>) {
        self.registry.register(address, incoming_tx);
    }

    /// Unregister a recipient at the specified address.
    /// This will close the receiver's channel, causing any pending `recv()` calls to return `None`.
    pub async fn unbind(&self, address: &str) {
        self.registry.unregister(address);
    }    

    /// Send a message (InOnly).
    pub async fn send(&self, address: &str, message: Message) -> TransportResult<()> {
        // Priority 1: local delivery
        if self.registry.has_local(address) {
            return self.local.send(address, message).await;
        }
        
        // Priority 2: IPC (stage 2)
        // self.remote.as_ref().ok_or(TransportError::RecipientNotFound(...))?
        //     .send(address, message).await
        
        Err(TransportError::RecipientNotFound(address.to_string()))
    }

    /// Send a request and wait for a response (InOut).
    pub async fn request(&self, address: &str, message: Message) -> TransportResult<Message> {
        if self.registry.has_local(address) {
            return self.local.request(address, message).await;
        }
        
        Err(TransportError::RecipientNotFound(address.to_string()))
    }

}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use tokio::sync::mpsc;

    use super::*;
    use crate::protocol::TransferMode;

    #[tokio::test]
    async fn test_in_memory_message_delivery_in_only() {
        // 1. Initialize registry and client
        let registry = LocalRegistry::new();
        let client = BrokerClient::new(registry);

        // 2. Prepare the receiver (Actor pattern)
        // Create a dedicated channel for the receiver. 
        // The receiver decides where to store its Receiver, and only registers the Sender with the broker.
        let (receiver_tx, mut receiver_rx) = mpsc::channel::<Message>(10);
        let target_address = "arcella:core:test:receiver".to_string();
        
        // Register the address with the broker
        client.bind(target_address.clone(), receiver_tx).await;

        // 3. Create a test message
        let original_message = Message::new(
            TransferMode::InOnly,
            [0u8; 32], // session_token (dummy)
            [1u8; 16], // message_id (dummy)
            64,        // ttl
            "test:ping".to_string(),
            target_address.clone(),
            Bytes::from("hello from sender"),
        ).expect("Message creation should succeed");

        // 4. Action: Send the message
        let send_result = client.send(&target_address, original_message.clone()).await;
        
        // Verify that the broker successfully accepted the message for delivery
        assert!(send_result.is_ok(), "Send operation should succeed");

        // 5. Verification: Receiving the message on the receiver side
        let received_message = receiver_rx.recv().await;
        
        assert!(received_message.is_some(), "Receiver should get a message");
        assert_eq!(
            original_message, 
            received_message.unwrap(), 
            "Received message should exactly match the sent message"
        );
    }

    #[tokio::test]
    async fn test_in_memory_message_delivery_to_unknown_address() {
        let registry = LocalRegistry::new();
        let client = BrokerClient::new(registry);

        let msg = Message::new(
            TransferMode::InOnly,
            [0u8; 32], [1u8; 16], 64,
            "test:ping".to_string(),
            "arcella:unknown:address".to_string(),
            Bytes::new(),
        ).unwrap();

        // Attempt to send to an unregistered address
        let result = client.send("arcella:unknown:address", msg).await;

        // Expect a RecipientNotFound error
        assert!(matches!(result, Err(TransportError::RecipientNotFound(_))));
    }

    #[tokio::test]
    async fn test_multi_recipient_routing() {
        // 1. Initialize the broker
        let registry = LocalRegistry::new();
        let client = BrokerClient::new(registry);

        // 2. Register multiple receivers with different addresses
        let addresses = vec![
            "arcella:core:users",
            "arcella:web:api",
            "arcella:batch:processor",
        ];

        let mut receivers = Vec::new();
        for addr in &addresses {
            let (tx, rx) = mpsc::channel::<Message>(10);
            client.bind(addr.to_string(), tx).await;
            receivers.push(rx);
        }

        // 3. Send mixed messages to different addresses
        let test_messages = vec![
            ("arcella:core:users", "user:created", Bytes::from("user1")),
            ("arcella:web:api", "http:request", Bytes::from("GET /api")),
            ("arcella:batch:processor", "batch:job", Bytes::from("job1")),
            ("arcella:core:users", "user:updated", Bytes::from("user2")),
            ("arcella:web:api", "http:response", Bytes::from("200 OK")),
            ("arcella:batch:processor", "batch:complete", Bytes::from("job1:done")),
            ("arcella:core:users", "user:deleted", Bytes::from("user3")),
        ];

        for (addr, msg_type, payload) in &test_messages {
            let msg = Message::new(
                TransferMode::InOnly,
                [0u8; 32],
                [1u8; 16],
                64,
                msg_type.to_string(),
                addr.to_string(),
                payload.clone(),
            ).expect("Message creation should succeed");

            let result = client.send(addr, msg).await;
            assert!(result.is_ok(), "Send to {} should succeed", addr);
        }

        // 4. Verification: each receiver got only its messages in the correct order
        // Receiver 0: arcella:core:users
        let mut rx0 = receivers.remove(0);
        let msg1 = rx0.recv().await.unwrap();
        assert_eq!(msg1.msg_type, "user:created");
        assert_eq!(msg1.payload, Bytes::from("user1"));

        let msg2 = rx0.recv().await.unwrap();
        assert_eq!(msg2.msg_type, "user:updated");
        assert_eq!(msg2.payload, Bytes::from("user2"));

        let msg3 = rx0.recv().await.unwrap();
        assert_eq!(msg3.msg_type, "user:deleted");
        assert_eq!(msg3.payload, Bytes::from("user3"));

        // Receiver 1: arcella:web:api
        let mut rx1 = receivers.remove(0);
        let msg4 = rx1.recv().await.unwrap();
        assert_eq!(msg4.msg_type, "http:request");
        assert_eq!(msg4.payload, Bytes::from("GET /api"));

        let msg5 = rx1.recv().await.unwrap();
        assert_eq!(msg5.msg_type, "http:response");
        assert_eq!(msg5.payload, Bytes::from("200 OK"));

        // Receiver 2: arcella:batch:processor
        let mut rx2 = receivers.remove(0);
        let msg6 = rx2.recv().await.unwrap();
        assert_eq!(msg6.msg_type, "batch:job");
        assert_eq!(msg6.payload, Bytes::from("job1"));

        let msg7 = rx2.recv().await.unwrap();
        assert_eq!(msg7.msg_type, "batch:complete");
        assert_eq!(msg7.payload, Bytes::from("job1:done"));

        // 5. Verification: channels are empty (no more messages)
        assert!(rx0.try_recv().is_err(), "users channel should be empty");
        assert!(rx1.try_recv().is_err(), "api channel should be empty");
        assert!(rx2.try_recv().is_err(), "processor channel should be empty");
    }

    #[tokio::test]
    async fn test_dynamic_registration() {
        let registry = LocalRegistry::new();
        let client = BrokerClient::new(registry.clone());

        // Sending before registration should return an error
        let msg = Message::new(
            TransferMode::InOnly, [0u8; 32], [1u8; 16], 64,
            "test".to_string(), "arcella:test".to_string(), Bytes::new(),
        ).unwrap();
        
        assert!(client.send("arcella:test", msg.clone()).await.is_err());

        // Registration
        let (tx, mut rx) = mpsc::channel(10);
        client.bind("arcella:test".to_string(), tx).await;

        // Now sending should succeed
        assert!(client.send("arcella:test", msg).await.is_ok());
        assert!(rx.recv().await.is_some());

        // Unregistration
        registry.unregister("arcella:test");

        // Should return an error again
        let msg2 = Message::new(
            TransferMode::InOnly, [0u8; 32], [2u8; 16], 64,
            "test2".to_string(), "arcella:test".to_string(), Bytes::new(),
        ).unwrap();
        assert!(client.send("arcella:test", msg2).await.is_err());
    }    

}