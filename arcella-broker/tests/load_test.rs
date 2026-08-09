// arcella/arcella-broker/src/tests/load_test.rs
//
// Copyright (c) 2026 Arcella Team
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
// or the MIT license <LICENSE-MIT>, at your option.
// This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::Arc;
use std::time::Instant;
use bytes::Bytes;

use arcella_broker::client::{BrokerClient, registry::LocalRegistry};
use arcella_broker::protocol::{Message, TransferMode};

// ============================================================================
// Test Configuration
// ============================================================================
const NUM_RECEIVERS: usize = 100;
const NUM_SENDERS: usize = 100;
const MESSAGES_PER_SENDER: usize = 1000000;
const TOTAL_MESSAGES: usize = NUM_SENDERS * MESSAGES_PER_SENDER;

/// High-throughput load test for the in-memory broker routing.
#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn test_high_throughput_in_memory_routing() {
    // 1. Initialize Broker
    let registry = Arc::new(LocalRegistry::new());
    let client = Arc::new(BrokerClient::new(registry));

    // 2. Register Receivers and spawn receiver tasks
    let mut receiver_handles = Vec::with_capacity(NUM_RECEIVERS);
    
    for i in 0..NUM_RECEIVERS {
        let addr = format!("arcella:perf:recv:{}", i);
        
        // Register the sender half in the broker registry
        let mut subscriber = client.subscribe(addr).await;
        
        // Spawn a dedicated task for each receiver to consume messages
        let handle = tokio::spawn(async move {
            let mut count = 0;
            // The loop will terminate when the channel is closed (all senders dropped)
            while let Some(_msg) = subscriber.recv().await {
                count += 1;
            }
            count
        });
        receiver_handles.push(handle);
    }

    // 3. Spawn Sender tasks and measure dispatch time
    let start_time = Instant::now();
    let mut sender_handles = Vec::with_capacity(NUM_SENDERS);
    
    for i in 0..NUM_SENDERS {
        let client = client.clone();
        
        let handle = tokio::spawn(async move {
            // Route messages to a specific receiver (with an offset to test routing logic)
            let target_idx = (i + 50) % NUM_RECEIVERS;
            let addr_str = format!("arcella:perf:recv:{}", target_idx);
            let addr = Bytes::from(addr_str.clone());
            let msg_type_str = Bytes::from_static(b"perf:test");
            let payload = Bytes::from_static(b"performance test payload data data");
            
            for _j in 0..MESSAGES_PER_SENDER {
                // Using Bytes::from_static to avoid payload allocation overhead
                let msg = Message::new(
                    TransferMode::InOnly,
                    [0u8; 32], // session_token
                    [0u8; 16], // message_id
                    [0u8; 4],
                    64,        // ttl
                    msg_type_str.clone(),
                    addr.clone(),
                    payload.clone(),
                ).expect("Message creation should not fail in test");
                
                client.send(&addr_str, msg).await.expect("Send should succeed");
            }
        });
        sender_handles.push(handle);
    }

    // 4. Wait for all senders to finish dispatching
    for handle in sender_handles {
        handle.await.expect("Sender task panicked");
    }
    let dispatch_duration = start_time.elapsed();

    // 5. Unbind receivers to close their channels and signal them to terminate
    for i in 0..NUM_RECEIVERS {
        let addr = format!("arcella:perf:recv:{}", i);
        client.unbind(&addr).await;
    }

    // 6. Wait for all receivers to finish processing and sum up received messages
    let mut total_received = 0;
    for handle in receiver_handles {
        total_received += handle.await.expect("Receiver task panicked");
    }
    let total_duration = start_time.elapsed();

    // 7. Assertions and Performance Metrics
    assert_eq!(
        total_received, TOTAL_MESSAGES,
        "All messages must be delivered without loss"
    );
    
    let throughput = (TOTAL_MESSAGES as f64) / total_duration.as_secs_f64();
    
    println!("\n==================================================");
    println!("       Arcella Broker Load Test Results           ");
    println!("==================================================");
    println!("Total Messages:      {}", TOTAL_MESSAGES);
    println!("Senders (Tasks):     {}", NUM_SENDERS);
    println!("Receivers (Tasks):   {}", NUM_RECEIVERS);
    println!("--------------------------------------------------");
    println!("Time to dispatch:    {:?}", dispatch_duration);
    println!("Total delivery time: {:?}", total_duration);
    println!("Throughput:          {:.0} msg/sec", throughput);
    println!("==================================================\n");
}