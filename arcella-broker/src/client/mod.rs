// arcella/arcella-broker/src/client/mod.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;

pub mod in_memory;
pub mod registry;
mod subscriber;

use crate::protocol::Message;
use crate::transport::{Transport, TransportError, TransportResult};

use in_memory::InMemoryTransport;
use registry::{LocalChannel, LocalRegistry};
use subscriber::Subscriber;

const DEFAULT_CHANNEL_CAPACITY: usize = 1024;


pub struct BrokerClient {
    registry: Arc<LocalRegistry>,
    local: InMemoryTransport,
    // remote: Option<IpcTransport>,  // Will be added in stage 2
}

impl BrokerClient {
    pub fn new(registry: Arc<LocalRegistry>) -> Self {
        let local = InMemoryTransport::new(registry.clone());
        Self { registry, local }
    }

    pub async fn subscribe(&self, address: String) -> Subscriber {
        Subscriber::bind(address, self.registry.clone(), DEFAULT_CHANNEL_CAPACITY)
    }    
    
    /// Register itself as a receiver at the specified address.
    pub async fn bind(&self, address: String, incoming_tx: LocalChannel) {
        self.registry.register(address, incoming_tx);
    }

    /// Unregister a recipient at the specified address.
    /// This will close the receiver's channel, causing any pending `recv()` calls to return `None`.
    pub async fn unbind(&self, address: &str) {
        self.registry.unregister(address);
    }    

    /// Send a message (InOnly).
    pub async fn send(&self, address: &str, message: Message) -> TransportResult<()> {
        self.local.send(address, message).await
    }

    /// Send a request and wait for a response (InOut).
    pub async fn request(&self, address: &str, message: Message) -> TransportResult<Message> {
        self.local.request(address, message).await
    }

}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::test_utils;

    #[tokio::test]
    async fn test_in_memory_message_delivery_in_only() {
        // 1. Initialize registry and client
        let registry = Arc::new(LocalRegistry::new());
        let client = BrokerClient::new(registry);

        // 2. Prepare the receiver (Actor pattern)
        let target_address = "arcella:core:test:receiver".to_string();
        let mut subscriber = client.subscribe(target_address.clone()).await;

        // 3. Create a test message
        let original_message = test_utils::dummy_in_only_message(Bytes::from("test:ping"),
            Bytes::from(target_address.clone()),
            Bytes::from("hello from sender"));

        // 4. Action: Send the message
        let send_result = client.send(&target_address, original_message.clone()).await;
        assert!(send_result.is_ok(), "Send operation should succeed");

        // 5. Verification: Receiving the message on the receiver side
        let received_message = subscriber.recv().await;
        assert!(received_message.is_some(), "Receiver should get a message");
        assert_eq!(
            original_message, 
            received_message.unwrap(), 
            "Received message should exactly match the sent message"
        );
    }

    #[tokio::test]
    async fn test_in_memory_message_delivery_to_unknown_address() {
        let registry = Arc::new(LocalRegistry::new());
        let client = BrokerClient::new(registry);

        let msg = test_utils::dummy_in_only_message(Bytes::from("test:ping"),
            Bytes::from("arcella:unknown:address"),
            Bytes::from(""));

        // Attempt to send to an unregistered address
        let result = client.send("arcella:unknown:address", msg).await;

        // Expect a RecipientNotFound error
        assert!(matches!(result, Err(TransportError::RecipientNotFound(_))));
    }

    #[tokio::test]
    async fn test_multi_recipient_routing() {
        // 1. Initialize the broker
        let registry = Arc::new(LocalRegistry::new());
        let client = BrokerClient::new(registry);

        // 2. Register multiple receivers with different addresses
        let addresses = vec![
            "arcella:core:users",
            "arcella:web:api",
            "arcella:batch:processor",
        ];

        let mut subscribers = Vec::new();
        for addr in &addresses {
            subscribers.push(client.subscribe(addr.to_string()).await);
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
            let msg = test_utils::dummy_in_only_message(Bytes::from(msg_type.clone()),
                Bytes::from(addr.clone()),
                payload.clone());

            let result = client.send(addr, msg).await;
            assert!(result.is_ok(), "Send to {} should succeed", addr);
        }

        // 4. Verification: each receiver got only its messages in the correct order
        // Receiver 0: arcella:core:users
        let msg1 = subscribers[0].recv().await.unwrap();
        assert_eq!(msg1.msg_type, "user:created");
        assert_eq!(msg1.payload, Bytes::from("user1"));

        let msg2 = subscribers[0].recv().await.unwrap();
        assert_eq!(msg2.msg_type, "user:updated");
        assert_eq!(msg2.payload, Bytes::from("user2"));

        let msg3 = subscribers[0].recv().await.unwrap();
        assert_eq!(msg3.msg_type, "user:deleted");
        assert_eq!(msg3.payload, Bytes::from("user3"));

        // Receiver 1: arcella:web:api
        let msg4 = subscribers[1].recv().await.unwrap();
        assert_eq!(msg4.msg_type, "http:request");
        assert_eq!(msg4.payload, Bytes::from("GET /api"));

        let msg5 = subscribers[1].recv().await.unwrap();
        assert_eq!(msg5.msg_type, "http:response");
        assert_eq!(msg5.payload, Bytes::from("200 OK"));

        // Receiver 2: arcella:batch:processor
        let msg6 = subscribers[2].recv().await.unwrap();
        assert_eq!(msg6.msg_type, "batch:job");
        assert_eq!(msg6.payload, Bytes::from("job1"));

        let msg7 = subscribers[2].recv().await.unwrap();
        assert_eq!(msg7.msg_type, "batch:complete");
        assert_eq!(msg7.payload, Bytes::from("job1:done"));

        // 5. Verification: channels are empty (no more messages)
        assert!(subscribers[0].try_recv().is_err(), "users channel should be empty");
        assert!(subscribers[1].try_recv().is_err(), "api channel should be empty");
        assert!(subscribers[2].try_recv().is_err(), "processor channel should be empty");
    }

    #[tokio::test]
    async fn test_dynamic_registration() {
        let registry = Arc::new(LocalRegistry::new());
        let client = BrokerClient::new(registry.clone());

        let address = Bytes::from_static(b"arcella:test");
        let payload = Bytes::from_static(b"");

        // Sending before registration should return an error
        let msg = test_utils::dummy_in_only_message(Bytes::from_static(b"test"),
            address.clone(),
            payload.clone());
        
        assert!(client.send("arcella:test", msg.clone()).await.is_err());

        // Registration
        let mut subscriber = client.subscribe("arcella:test".to_string()).await;

        // Now sending should succeed
        assert!(client.send("arcella:test", msg).await.is_ok());
        assert!(subscriber.recv().await.is_some());

        // Unregistration
        client.unbind("arcella:test").await;

        // Should return an error again
        let msg2 = test_utils::dummy_in_only_message(Bytes::from_static(b"test2"),
            address.clone(),
            payload.clone());
        assert!(client.send("arcella:test", msg2).await.is_err());
    }    

    #[tokio::test]
    async fn test_duplicate_subscription_causes_premature_unregistration() {
        let registry = Arc::new(LocalRegistry::new());
        let client = BrokerClient::new(registry);
        let addr = "arcella:test:duplicate";

        // 1. Первая подписка
        let mut sub1 = client.subscribe(addr.to_string()).await;
        
        // 2. Вторая подписка на тот же адрес 
        // (Текущая реализация молча перезаписывает tx1 на tx2 в DashMap)
        let mut sub2 = client.subscribe(addr.to_string()).await;

        // 3. sub1 выходит из области видимости
        drop(sub1);
        // Срабатывает Drop: self.registry.unregister(&self.address);
        // Registry теперь пуст! tx2 (принадлежащий sub2) удален из реестра.

        // 4. Попытка отправки сообщения
        let msg = test_utils::dummy_in_only_message(
            Bytes::from("test"), 
            Bytes::from(addr), 
            Bytes::from("hello")
        );
        
        // ОЖИДАНИЕ: Send должен succeed, так как sub2 жив и готов принимать.
        // РЕАЛЬНОСТЬ: ОШИБКА RecipientNotFound, так как Drop(sub1) вычистил реестр.
        let result = client.send(addr, msg).await;
        
        // Этот assert доказывает наличие бага:
        assert!(!result.is_err(), "BUG CONFIRMED: sub2 is alive but registry is empty!");
        assert!(sub2.try_recv().is_err(), "sub2 never received the message");
    }

}