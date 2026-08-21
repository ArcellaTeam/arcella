    // arcella/arcella-broker/src/registry/mod.rs
    //
    // Copyright (c) 2026 Arcella Team
    //
    // Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>
    // or the MIT license <LICENSE-MIT>, at your option.
    // This file may not be copied, modified, or distributed
    // except according to those terms.

    use dashmap::DashMap;
    use tokio::sync::mpsc;
    use std::{
        sync::RwLock,
        collections::HashSet,
    };

    use crate::protocol::Message;

    /// Channel type for local delivery
    pub type LocalChannel = mpsc::Sender<Message>;

    /// Registry of local recipients (within a single process).
    /// 
    /// Key — hierarchical address (e.g., "arcella:core:users").
    /// Value — sender to the recipient's queue.
    #[derive(Default)]
    pub struct LocalRegistry {
        /// All subscriptions (exact and wildcard) — base structure, unchanged.
        recipients: DashMap<String, LocalChannel>,
        /// Index of wildcard keys for fast lookup scanning.
        /// Only contains keys that include '*' or "**".
        wildcard_keys: RwLock<HashSet<String>>,
        /// Global lock for register/unregister
        register_lock: std::sync::Mutex<()>,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum RegistryError {
        #[error("Address '{0}' is already occupied")]
        AddressAlreadyOccupied (String),
        
        #[error("Wildcard subscription '{0}' conflicts with existing address '{1}'")]
        WildcardConflict (String, String),
        
        #[error("Address '{0}' conflicts with existing wildcard subscription '{1}'")]
        ConflictsWithWildcard (String, String),
    }

    impl LocalRegistry {
        pub fn new() -> Self {
            Self::default()
        }

        /// Returns true if the address contains wildcard characters.
        #[inline]
        fn is_wildcard(address: &str) -> bool {
            address.contains('*')
        }

        /// Checks whether a concrete address matches a pattern.
        ///
        /// Rules:
        /// - `*` matches exactly one segment at its position
        /// - `**` (only valid as last segment) matches zero or more trailing segments
        /// - Segments are separated by `:`
        ///
        /// Examples:
        /// ```text
        /// matches("arcella:core:users", "arcella:*:users")   → true
        /// matches("arcella:core:users", "arcella:*:*")       → true
        /// matches("arcella:core:users", "*:core:users")      → true
        /// matches("arcella:core:users", "arcella:core:**")   → true
        /// matches("arcella:core",       "arcella:core:**")   → true  (zero extra)
        /// matches("arcella",            "arcella:core:**")   → false (too short)
        /// matches("arcella:core:users", "arcella:core:*")    → true
        /// matches("arcella:core:a:b",   "arcella:core:*")    → false (len mismatch)
        /// matches("arcella:core:users", "arcella:web:*")     → false
        /// ```
        fn matches(address: &str, pattern: &str) -> bool {
            let mut addr_iter = address.split(':');
            let mut pat_iter = pattern.split(':');

            loop {
                let pat_seg = pat_iter.next();
                let addr_seg = addr_iter.next(); 

                match (pat_seg, addr_seg) {
                    (Some("**"), _) => {
                        return pat_iter.next().is_none();
                    }    
                    (Some(p), Some(a)) => {
                        if p != "*" && p != a {
                            return false;
                        }    
                    }
                    (Some(_), None) => return false,
                    (None, Some(_)) => return false,
                    (None, None) => return true,
                }   
            }

        }
        
        /// Register a recipient at the specified address
        pub fn register(&self, address: String, channel: LocalChannel) -> Result<(), RegistryError> {

            if Self::is_wildcard(&address) {
                //TODO
                Ok(())
            } else {
                self.register_exact(address, channel)
            }

        }

        fn register_exact(
            &self,
            address: String,
            channel: LocalChannel,
        ) -> Result<(), RegistryError> {

            let _guard = self.register_lock.lock().unwrap();

            // 1. Exact duplicate check
            if self.recipients.contains_key(&address) {
                return Err(RegistryError::AddressAlreadyOccupied(address.clone()));
            }

            // 2. Check if any existing wildcard covers this address
            let keys = self.wildcard_keys.read().unwrap();
            for wc in keys.iter() {
                if Self::matches(&address, wc) {
                    return Err(RegistryError::ConflictsWithWildcard (
                        address.clone(),
                        wc.clone(),
                    ));
                }
            }
            drop(keys);

            // 3. Atomic insert
            match self.recipients.entry(address.clone()) {
                dashmap::mapref::entry::Entry::Occupied(_) => {
                    Err(RegistryError::AddressAlreadyOccupied(address))
                }
                dashmap::mapref::entry::Entry::Vacant(v) => {
                    v.insert(channel);
                    Ok(())
                }
            }

        }
        
        /// Unregister a recipient
        pub fn unregister(&self, address: &str) {

            let _guard = self.register_lock.lock().unwrap();

            self.recipients.remove(address);

            if Self::is_wildcard(address) {
                let mut keys = self.wildcard_keys.write().unwrap();
                keys.remove(address);
            }
        }

        /// Find a local channel for the given address.
        /// Returns `Some(channel)` if the recipient exists in this process.
        pub fn lookup(&self, address: &str) -> Option<LocalChannel> {
            // 1. Exact match — highest priority
            if let Some(entry) = self.recipients.get(address) {
                return Some(entry.value().clone());
            }

            // 2. Scan wildcard index
            let keys = self.wildcard_keys.read().unwrap();
            let mut best: Option<(usize, LocalChannel)> = None;

            for wc in keys.iter() {
                if Self::matches(address, wc) {
                    let specificity = wc
                        .split(':')
                        .filter(|s| *s != "**")
                        .count();

                    let should_update = match &best {
                        None => true,
                        Some((best_spec, _)) => specificity > *best_spec,
                    };

                    if should_update {
                        if let Some(entry) = self.recipients.get(wc.as_str()) {
                            best = Some((specificity, entry.value().clone()));
                        }
                    }
                }
            }

            best.map(|(_, ch)| ch)

        }

        /// Check if a local recipient exists for the given address 
        /// (exact match only)
        pub fn has_local(&self, address: &str) -> bool {
            self.recipients.contains_key(address)
        }

        /// Check if a local recipient exists for the given address
        /// (including wildcard matches).
        pub fn has_route(&self, address: &str) -> bool {
            self.lookup(address).is_some()
        }

    }
