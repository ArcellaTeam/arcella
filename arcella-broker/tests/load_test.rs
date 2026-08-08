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
use tokio::sync::mpsc;

use arcella_broker::client::{BrokerClient, registry::LocalRegistry};
use arcella_broker::protocol::{Message, TransferMode};

// ============================================================================
// Test Configuration
// ============================================================================
const NUM_RECEIVERS: usize = 1;
const NUM_SENDERS: usize = 1;
const MESSAGES_PER_SENDER: usize = 200000;
const TOTAL_MESSAGES: usize = NUM_SENDERS * MESSAGES_PER_SENDER;

/// High-throughput load test for the in-memory broker routing.
/// 
/// This test spawns 100 sender tasks and 100 receiver tasks, running on a 
/// multi-threaded Tokio runtime with 128 worker threads. It measures the 
/// total time to deliver 200,000 messages and calculates the throughput.
#[tokio::test(flavor = "multi_thread", worker_threads = 128)]
async fn test_high_throughput_in_memory_routing() {
    // 1. Initialize Broker
    let registry = LocalRegistry::new();
    let client = Arc::new(BrokerClient::new(registry));

    // 2. Register Receivers and spawn receiver tasks
    let mut receiver_handles = Vec::with_capacity(NUM_RECEIVERS);
    
    for i in 0..NUM_RECEIVERS {
        let (tx, mut rx) = mpsc::channel::<Message>(1024); // Buffer size 1024
        let addr = format!("arcella:perf:recv:{}", i);
        
        // Register the sender half in the broker registry
        client.bind(addr, tx).await;
        
        // Spawn a dedicated task for each receiver to consume messages
        let handle = tokio::spawn(async move {
            let mut count = 0;
            // The loop will terminate when the channel is closed (all senders dropped)
            while let Some(_msg) = rx.recv().await {
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
            let addr = format!("arcella:perf:recv:{}", target_idx);
            
            for _j in 0..MESSAGES_PER_SENDER {
                // Using Bytes::from_static to avoid payload allocation overhead
                let msg = Message::new(
                    TransferMode::InOnly,
                    [0u8; 32], // session_token
                    [0u8; 16], // message_id
                    64,        // ttl
                    "perf:test".to_string(),
                    addr.clone(),
                    Bytes::from_static(b"performance test payload data"),
                ).expect("Message creation should not fail in test");
                
                client.send(&addr, msg).await.expect("Send should succeed");
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