//! Integration tests for OpenRaft + RocksDB storage.
//!
//! These tests verify the integration between OpenRaft storage traits
//! and the RocksDB-backed implementation. They test:
//!
//! - RaftLogStorage trait implementation with RocksDB
//! - RaftStateMachine trait implementation with RocksDB
//! - Data persistence across storage restarts

use openraft::storage::{RaftLogReader, RaftLogStorage, RaftSnapshotBuilder, RaftStateMachine};
use openraft::{Entry, LeaderId, LogId, Vote};
use prost::Message;
use seshat_storage::{
    DataRaft, LogIdMessage, Operation, RaftTypeConfig, Request, RocksDBLogStorage,
    RocksDBStateMachine, Storage, StorageOptions, SystemRaft,
};
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_storage() -> (Arc<Storage>, TempDir) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let opts = StorageOptions::with_data_dir(dir.path().to_path_buf());
    let storage = Storage::new(opts).expect("Failed to create storage");
    (Arc::new(storage), dir)
}

// ============================================================================
// RaftLogStorage Tests
// ============================================================================

#[tokio::test]
async fn test_rocksdb_log_storage_vote_roundtrip() {
    let (storage, _dir) = create_test_storage();
    let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

    let vote = Vote::new(5, 42);

    // save_vote
    log_storage
        .save_vote(&vote)
        .await
        .expect("save_vote should succeed");

    // read_vote
    let read_vote = log_storage
        .read_vote()
        .await
        .expect("read_vote should succeed")
        .expect("vote should exist");

    assert_eq!(read_vote.leader_id().term, 5);
    assert_eq!(read_vote.leader_id().node_id, 42);
}

#[tokio::test]
async fn test_rocksdb_log_storage_get_log_state_empty() {
    let (storage, _dir) = create_test_storage();
    let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

    let state = log_storage
        .get_log_state()
        .await
        .expect("get_log_state should succeed");

    assert_eq!(state.last_purged_log_id, None);
    assert_eq!(state.last_log_id, None);
}

#[tokio::test]
async fn test_rocksdb_log_storage_read_vote_nonexistent() {
    let (storage, _dir) = create_test_storage();
    let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

    let read_vote = log_storage
        .read_vote()
        .await
        .expect("read_vote should succeed");

    assert_eq!(read_vote, None);
}

#[tokio::test]
async fn test_rocksdb_log_storage_get_log_reader() {
    let (storage, _dir) = create_test_storage();
    let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

    let mut reader = log_storage.get_log_reader().await;

    let retrieved = reader
        .try_get_log_entries(1..=10)
        .await
        .expect("try_get_log_entries should succeed");

    assert!(retrieved.is_empty());
}

#[tokio::test]
async fn test_rocksdb_log_storage_truncate() {
    let (storage, _dir) = create_test_storage();
    let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

    // Manually append entries by writing directly to storage
    for i in 1..=5u64 {
        let key = format!("log:{:020}", i);
        let value = vec![0x01, b'd', b'a', b't', b'a']; // ENTRY_TYPE_NORMAL + data
        storage
            .put(
                seshat_storage::ColumnFamily::DataRaftLog,
                key.as_bytes(),
                &value,
            )
            .expect("put should succeed");
    }

    // Truncate at index 3 (keep entries 3, 4, 5)
    log_storage
        .truncate(LogId::new(LeaderId::new(1, 1), 3))
        .await
        .expect("truncate should succeed");

    // Verify remaining entries
    let mut reader = log_storage.get_log_reader().await;

    let retrieved = reader
        .try_get_log_entries(1..=5)
        .await
        .expect("try_get_log_entries should succeed");

    // Should have entries 3, 4, 5 (empty payload entries)
    assert_eq!(retrieved.len(), 3);
    assert_eq!(retrieved[0].log_id.index, 3);
    assert_eq!(retrieved[1].log_id.index, 4);
    assert_eq!(retrieved[2].log_id.index, 5);
}

#[tokio::test]
async fn test_rocksdb_log_storage_purge() {
    let (storage, _dir) = create_test_storage();
    let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

    // Manually append entries
    for i in 1..=4u64 {
        let key = format!("log:{:020}", i);
        let value = vec![0x00]; // ENTRY_TYPE_BLANK
        storage
            .put(
                seshat_storage::ColumnFamily::DataRaftLog,
                key.as_bytes(),
                &value,
            )
            .expect("put should succeed");
    }

    // Purge at index 2 (keep entries 3, 4)
    log_storage
        .purge(LogId::new(LeaderId::new(1, 1), 2))
        .await
        .expect("purge should succeed");

    // Verify state reflects purge
    let state = log_storage
        .get_log_state()
        .await
        .expect("get_log_state should succeed");

    assert_eq!(state.last_purged_log_id.unwrap().index, 2);
    assert_eq!(state.last_log_id.unwrap().index, 4);
}

// ============================================================================
// RaftStateMachine Tests
// ============================================================================

#[tokio::test]
async fn test_rocksdb_state_machine_apply_set_operation() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let operation = Operation::Set {
        key: b"test_key".to_vec(),
        value: b"test_value".to_vec(),
    };

    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 1),
        payload: openraft::EntryPayload::Normal(Request::new(operation.serialize().unwrap())),
    };

    let responses = state_machine
        .apply(std::iter::once(entry))
        .await
        .expect("apply should succeed");

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].result, b"OK");

    // Verify data is in RocksDB
    let value = storage
        .get(seshat_storage::ColumnFamily::DataKv, b"test_key")
        .expect("get should succeed");

    assert_eq!(value, Some(b"test_value".to_vec()));
}

#[tokio::test]
async fn test_rocksdb_state_machine_apply_del_operation() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    // First set a value directly in storage
    storage
        .put(
            seshat_storage::ColumnFamily::DataKv,
            b"del_key",
            b"del_value",
        )
        .expect("put should succeed");

    let operation = Operation::Del {
        key: b"del_key".to_vec(),
    };

    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 1),
        payload: openraft::EntryPayload::Normal(Request::new(operation.serialize().unwrap())),
    };

    let responses = state_machine
        .apply(std::iter::once(entry))
        .await
        .expect("apply should succeed");

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].result, b"OK"); // Del returns OK

    // Verify key is deleted
    let value = storage
        .get(seshat_storage::ColumnFamily::DataKv, b"del_key")
        .expect("get should succeed");

    assert_eq!(value, None);
}

#[tokio::test]
async fn test_rocksdb_state_machine_apply_blank_entry() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 1),
        payload: openraft::EntryPayload::Blank,
    };

    let responses = state_machine
        .apply(std::iter::once(entry))
        .await
        .expect("apply should succeed");

    assert_eq!(responses.len(), 1);
    assert!(responses[0].result.is_empty());

    // Verify no data was written to DataKv
    let value = storage
        .get(seshat_storage::ColumnFamily::DataKv, b"any_key")
        .expect("get should succeed");

    assert_eq!(value, None);
}

#[tokio::test]
async fn test_rocksdb_state_machine_applied_state_initial() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let (applied, membership) = state_machine
        .applied_state()
        .await
        .expect("applied_state should succeed");

    assert_eq!(applied, None);
    assert!(membership.membership().voter_ids().next().is_none());
}

#[tokio::test]
async fn test_rocksdb_state_machine_applied_state_after_apply() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    // Apply an entry
    let operation = Operation::Set {
        key: b"key".to_vec(),
        value: b"value".to_vec(),
    };

    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 5),
        payload: openraft::EntryPayload::Normal(Request::new(operation.serialize().unwrap())),
    };

    state_machine
        .apply(std::iter::once(entry))
        .await
        .expect("apply should succeed");

    // Verify applied state
    let (applied, _membership) = state_machine
        .applied_state()
        .await
        .expect("applied_state should succeed");

    assert_eq!(applied.unwrap().index, 5);
}

#[tokio::test]
async fn test_rocksdb_state_machine_system_raft() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<SystemRaft>::new(Arc::clone(&storage));

    // System raft should not have KV operations, just membership
    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 1),
        payload: openraft::EntryPayload::Blank,
    };

    let responses = state_machine
        .apply(std::iter::once(entry))
        .await
        .expect("apply should succeed");

    assert_eq!(responses.len(), 1);
    assert!(responses[0].result.is_empty());

    // Verify no data was written to DataKv
    let value = storage
        .get(seshat_storage::ColumnFamily::DataKv, b"any_key")
        .expect("get should succeed");

    assert_eq!(value, None);
}

#[tokio::test]
async fn test_multiple_apply_operations() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let entries: Vec<Entry<RaftTypeConfig>> = vec![
        Entry {
            log_id: LogId::new(LeaderId::new(1, 1), 1),
            payload: openraft::EntryPayload::Normal(Request::new(
                Operation::Set {
                    key: b"key1".to_vec(),
                    value: b"value1".to_vec(),
                }
                .serialize()
                .unwrap(),
            )),
        },
        Entry {
            log_id: LogId::new(LeaderId::new(1, 1), 2),
            payload: openraft::EntryPayload::Normal(Request::new(
                Operation::Set {
                    key: b"key2".to_vec(),
                    value: b"value2".to_vec(),
                }
                .serialize()
                .unwrap(),
            )),
        },
        Entry {
            log_id: LogId::new(LeaderId::new(1, 1), 3),
            payload: openraft::EntryPayload::Normal(Request::new(
                Operation::Del {
                    key: b"key1".to_vec(),
                }
                .serialize()
                .unwrap(),
            )),
        },
    ];

    let responses = state_machine
        .apply(entries.into_iter())
        .await
        .expect("apply should succeed");

    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0].result, b"OK");
    assert_eq!(responses[1].result, b"OK");
    assert_eq!(responses[2].result, b"OK"); // Del returns OK

    // Verify final state
    assert_eq!(
        storage
            .get(seshat_storage::ColumnFamily::DataKv, b"key1")
            .expect("get should succeed"),
        None
    );
    assert_eq!(
        storage
            .get(seshat_storage::ColumnFamily::DataKv, b"key2")
            .expect("get should succeed"),
        Some(b"value2".to_vec())
    );
}

// ============================================================================
// Snapshot Tests
// ============================================================================

#[tokio::test]
async fn test_rocksdb_state_machine_snapshot_builder() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    // Apply some entries to set applied state
    let entries: Vec<Entry<RaftTypeConfig>> = (1..=3)
        .map(|i| {
            let op = Operation::Set {
                key: format!("key{}", i).into_bytes(),
                value: format!("value{}", i).into_bytes(),
            };
            Entry {
                log_id: LogId::new(LeaderId::new(1, 1), i),
                payload: openraft::EntryPayload::Normal(Request::new(op.serialize().unwrap())),
            }
        })
        .collect();

    state_machine
        .apply(entries.into_iter())
        .await
        .expect("apply should succeed");

    // Build snapshot
    let mut builder = state_machine.get_snapshot_builder().await;

    let snapshot = builder
        .build_snapshot()
        .await
        .expect("build_snapshot should succeed");

    assert_eq!(snapshot.meta.last_log_id.unwrap().index, 3);
    assert!(!snapshot.meta.snapshot_id.is_empty());
}

// ============================================================================
// Persistence Tests
// ============================================================================

#[tokio::test]
async fn test_vote_persists_across_storage_restart() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();

    // Create storage and save vote
    {
        let opts = StorageOptions::with_data_dir(path.clone());
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));
        let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

        let vote = Vote::new(10, 99);
        log_storage
            .save_vote(&vote)
            .await
            .expect("save_vote should succeed");
    }

    // Reopen storage and verify vote persists
    {
        let opts = StorageOptions::with_data_dir(path);
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));
        let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

        let read_vote = log_storage
            .read_vote()
            .await
            .expect("read_vote should succeed")
            .expect("vote should exist");

        assert_eq!(read_vote.leader_id().term, 10);
        assert_eq!(read_vote.leader_id().node_id, 99);
    }
}

#[tokio::test]
async fn test_log_entries_persist_across_storage_restart() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();

    // Create storage and append entries
    {
        let opts = StorageOptions::with_data_dir(path.clone());
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));

        // Manually write entries
        for i in 1..=5u64 {
            let key = format!("log:{:020}", i);
            let value = vec![0x00]; // ENTRY_TYPE_BLANK
            storage
                .put(
                    seshat_storage::ColumnFamily::DataRaftLog,
                    key.as_bytes(),
                    &value,
                )
                .expect("put should succeed");
        }

        // Update LAST_LOG_ID_KEY metadata
        let log_id_msg = LogIdMessage {
            term: 1,
            node_id: 1,
            index: 5,
        };
        let log_id_bytes = log_id_msg.encode_to_vec();
        storage
            .put(
                seshat_storage::ColumnFamily::DataRaftLog,
                b"__last_log_id",
                &log_id_bytes,
            )
            .expect("put should succeed");
    }

    // Reopen storage and verify entries persist
    {
        let opts = StorageOptions::with_data_dir(path);
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));
        let mut log_storage = RocksDBLogStorage::<DataRaft>::new(Arc::clone(&storage));

        let state = log_storage
            .get_log_state()
            .await
            .expect("get_log_state should succeed");

        assert_eq!(state.last_log_id.unwrap().index, 5);

        let mut reader = log_storage.get_log_reader().await;

        let retrieved = reader
            .try_get_log_entries(1..=5)
            .await
            .expect("try_get_log_entries should succeed");

        assert_eq!(retrieved.len(), 5);
        assert_eq!(retrieved[0].log_id.index, 1);
        assert_eq!(retrieved[4].log_id.index, 5);
    }
}

#[tokio::test]
async fn test_applied_state_persists_across_storage_restart() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();

    // Create storage and apply entries
    {
        let opts = StorageOptions::with_data_dir(path.clone());
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));
        let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

        let entries: Vec<Entry<RaftTypeConfig>> = (1..=3)
            .map(|i| {
                let op = Operation::Set {
                    key: format!("key{}", i).into_bytes(),
                    value: format!("value{}", i).into_bytes(),
                };
                Entry {
                    log_id: LogId::new(LeaderId::new(1, 1), i),
                    payload: openraft::EntryPayload::Normal(Request::new(op.serialize().unwrap())),
                }
            })
            .collect();

        state_machine
            .apply(entries.into_iter())
            .await
            .expect("apply should succeed");
    }

    // Reopen storage and verify applied state persists
    {
        let opts = StorageOptions::with_data_dir(path);
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));
        let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

        let (applied, _membership) = state_machine
            .applied_state()
            .await
            .expect("applied_state should succeed");

        assert_eq!(applied.unwrap().index, 3);
    }
}

#[tokio::test]
async fn test_kv_data_persists_across_storage_restart() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let path = dir.path().to_path_buf();

    // Create storage and apply SET operations
    {
        let opts = StorageOptions::with_data_dir(path.clone());
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));
        let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

        let entries: Vec<Entry<RaftTypeConfig>> = vec![
            Entry {
                log_id: LogId::new(LeaderId::new(1, 1), 1),
                payload: openraft::EntryPayload::Normal(Request::new(
                    Operation::Set {
                        key: b"persistent_key".to_vec(),
                        value: b"persistent_value".to_vec(),
                    }
                    .serialize()
                    .unwrap(),
                )),
            },
            Entry {
                log_id: LogId::new(LeaderId::new(1, 1), 2),
                payload: openraft::EntryPayload::Normal(Request::new(
                    Operation::Set {
                        key: b"another_key".to_vec(),
                        value: b"another_value".to_vec(),
                    }
                    .serialize()
                    .unwrap(),
                )),
            },
        ];

        state_machine
            .apply(entries.into_iter())
            .await
            .expect("apply should succeed");
    }

    // Reopen storage and verify data persists
    {
        let opts = StorageOptions::with_data_dir(path);
        let storage = Arc::new(Storage::new(opts).expect("Failed to create storage"));

        let value1 = storage
            .get(seshat_storage::ColumnFamily::DataKv, b"persistent_key")
            .expect("get should succeed");
        assert_eq!(value1, Some(b"persistent_value".to_vec()));

        let value2 = storage
            .get(seshat_storage::ColumnFamily::DataKv, b"another_key")
            .expect("get should succeed");
        assert_eq!(value2, Some(b"another_value".to_vec()));
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_operation_invalid_deserialization() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 1),
        payload: openraft::EntryPayload::Normal(Request::new(vec![0xFF, 0xFF, 0xFF, 0xFF])),
    };

    let result = state_machine.apply(std::iter::once(entry)).await;
    assert!(result.is_err(), "Invalid operation should fail");
}

#[tokio::test]
async fn test_del_nonexistent_key_returns_zero() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let operation = Operation::Del {
        key: b"nonexistent".to_vec(),
    };

    let entry = Entry {
        log_id: LogId::new(LeaderId::new(1, 1), 1),
        payload: openraft::EntryPayload::Normal(Request::new(operation.serialize().unwrap())),
    };

    let responses = state_machine
        .apply(std::iter::once(entry))
        .await
        .expect("apply should succeed");

    assert_eq!(responses[0].result, b"OK"); // Del returns OK even for nonexistent
}

#[tokio::test]
async fn test_set_same_key_twice() {
    let (storage, _dir) = create_test_storage();
    let mut state_machine = RocksDBStateMachine::<DataRaft>::new(Arc::clone(&storage));

    let entries: Vec<Entry<RaftTypeConfig>> = vec![
        Entry {
            log_id: LogId::new(LeaderId::new(1, 1), 1),
            payload: openraft::EntryPayload::Normal(Request::new(
                Operation::Set {
                    key: b"key".to_vec(),
                    value: b"value1".to_vec(),
                }
                .serialize()
                .unwrap(),
            )),
        },
        Entry {
            log_id: LogId::new(LeaderId::new(1, 1), 2),
            payload: openraft::EntryPayload::Normal(Request::new(
                Operation::Set {
                    key: b"key".to_vec(),
                    value: b"value2".to_vec(),
                }
                .serialize()
                .unwrap(),
            )),
        },
    ];

    state_machine
        .apply(entries.into_iter())
        .await
        .expect("apply should succeed");

    // Verify final value is the second set
    let value = storage
        .get(seshat_storage::ColumnFamily::DataKv, b"key")
        .expect("get should succeed");

    assert_eq!(value, Some(b"value2".to_vec()));
}
