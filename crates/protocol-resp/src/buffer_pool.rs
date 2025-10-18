//! Buffer pool for reusing BytesMut buffers
//!
//! This module provides a simple buffer pool to reduce allocations when encoding
//! RESP messages. Buffers are cleared before returning to the pool to prevent
//! data leakage between uses.

use bytes::BytesMut;

const DEFAULT_BUFFER_CAPACITY: usize = 4096; // 4KB
const DEFAULT_MAX_POOL_SIZE: usize = 100;

/// A pool of reusable BytesMut buffers to reduce allocations
///
/// # Examples
///
/// ```
/// use seshat_protocol_resp::BufferPool;
/// use bytes::BytesMut;
///
/// let mut pool = BufferPool::new(4096);
///
/// // Acquire a buffer
/// let mut buf = pool.acquire();
/// buf.extend_from_slice(b"Hello, World!");
///
/// // Return it to the pool (automatically cleared)
/// pool.release(buf);
///
/// // Acquire again - gets a cleared buffer
/// let buf2 = pool.acquire();
/// assert_eq!(buf2.len(), 0);
/// ```
pub struct BufferPool {
    buffers: Vec<BytesMut>,
    default_capacity: usize,
    max_pool_size: usize,
}

impl BufferPool {
    /// Create a new buffer pool with default settings
    ///
    /// Default buffer capacity: 4KB (4096 bytes)
    /// Default max pool size: 100 buffers
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::BufferPool;
    ///
    /// let mut pool = BufferPool::new(4096);
    /// let buf = pool.acquire();
    /// assert_eq!(buf.capacity(), 4096);
    /// ```
    pub fn new(default_capacity: usize) -> Self {
        Self {
            buffers: Vec::new(),
            default_capacity,
            max_pool_size: DEFAULT_MAX_POOL_SIZE,
        }
    }

    /// Create a buffer pool with custom configuration
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::BufferPool;
    ///
    /// let mut pool = BufferPool::with_capacity(8192, 50);
    /// let buf = pool.acquire();
    /// assert_eq!(buf.capacity(), 8192);
    /// ```
    pub fn with_capacity(default_capacity: usize, max_pool_size: usize) -> Self {
        Self {
            buffers: Vec::new(),
            default_capacity,
            max_pool_size,
        }
    }

    /// Acquire a buffer from the pool or create a new one
    ///
    /// If the pool is empty, a new buffer with the default capacity is created.
    /// Otherwise, a buffer is taken from the pool and returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::BufferPool;
    ///
    /// let mut pool = BufferPool::new(4096);
    /// let buf1 = pool.acquire();
    /// let buf2 = pool.acquire();
    /// // Both buffers have the default capacity
    /// assert_eq!(buf1.capacity(), 4096);
    /// assert_eq!(buf2.capacity(), 4096);
    /// ```
    pub fn acquire(&mut self) -> BytesMut {
        self.buffers
            .pop()
            .unwrap_or_else(|| BytesMut::with_capacity(self.default_capacity))
    }

    /// Return a buffer to the pool
    ///
    /// The buffer is cleared before being added to the pool. If the buffer's
    /// capacity is less than the default capacity, it is dropped instead of
    /// being returned to the pool. If the pool is at maximum capacity, the
    /// buffer is dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use seshat_protocol_resp::BufferPool;
    /// use bytes::BytesMut;
    ///
    /// let mut pool = BufferPool::new(4096);
    /// let mut buf = pool.acquire();
    /// buf.extend_from_slice(b"data");
    ///
    /// // Buffer is cleared when released
    /// pool.release(buf);
    ///
    /// let buf2 = pool.acquire();
    /// assert_eq!(buf2.len(), 0); // Buffer was cleared
    /// ```
    pub fn release(&mut self, mut buf: BytesMut) {
        // Only accept buffers with sufficient capacity
        if buf.capacity() < self.default_capacity {
            return;
        }

        // Don't exceed maximum pool size
        if self.buffers.len() >= self.max_pool_size {
            return;
        }

        // Clear the buffer before returning to pool
        buf.clear();

        // Add to pool
        self.buffers.push(buf);
    }
}

impl Default for BufferPool {
    /// Create a buffer pool with default settings
    ///
    /// Uses 4KB buffer capacity and max pool size of 100
    fn default() -> Self {
        Self::new(DEFAULT_BUFFER_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Constructor Tests =====

    #[test]
    fn test_new_creates_empty_pool() {
        let pool = BufferPool::new(4096);
        assert_eq!(pool.buffers.len(), 0);
        assert_eq!(pool.default_capacity, 4096);
        assert_eq!(pool.max_pool_size, DEFAULT_MAX_POOL_SIZE);
    }

    #[test]
    fn test_with_capacity_creates_empty_pool() {
        let pool = BufferPool::with_capacity(8192, 50);
        assert_eq!(pool.buffers.len(), 0);
        assert_eq!(pool.default_capacity, 8192);
        assert_eq!(pool.max_pool_size, 50);
    }

    #[test]
    fn test_new_with_custom_capacity() {
        let pool = BufferPool::new(2048);
        assert_eq!(pool.default_capacity, 2048);
    }

    #[test]
    fn test_with_capacity_custom_pool_size() {
        let pool = BufferPool::with_capacity(4096, 200);
        assert_eq!(pool.max_pool_size, 200);
    }

    #[test]
    fn test_default_uses_default_constants() {
        let pool = BufferPool::default();
        assert_eq!(pool.buffers.len(), 0);
        assert_eq!(pool.default_capacity, DEFAULT_BUFFER_CAPACITY);
        assert_eq!(pool.max_pool_size, DEFAULT_MAX_POOL_SIZE);
    }

    // ===== Acquire Tests =====

    #[test]
    fn test_acquire_from_empty_pool_creates_new_buffer() {
        let mut pool = BufferPool::new(4096);
        let buf = pool.acquire();
        assert_eq!(buf.capacity(), 4096);
        assert_eq!(buf.len(), 0);
        assert_eq!(pool.buffers.len(), 0);
    }

    #[test]
    fn test_acquire_from_populated_pool_reuses_buffer() {
        let mut pool = BufferPool::new(4096);

        // Add a buffer to the pool
        let buf = BytesMut::with_capacity(4096);
        pool.release(buf);

        assert_eq!(pool.buffers.len(), 1);

        // Acquire should reuse the buffer
        let acquired = pool.acquire();
        assert_eq!(acquired.capacity(), 4096);
        assert_eq!(pool.buffers.len(), 0);
    }

    #[test]
    fn test_acquire_multiple_times_from_empty_pool() {
        let mut pool = BufferPool::new(4096);

        let buf1 = pool.acquire();
        let buf2 = pool.acquire();
        let buf3 = pool.acquire();

        assert_eq!(buf1.capacity(), 4096);
        assert_eq!(buf2.capacity(), 4096);
        assert_eq!(buf3.capacity(), 4096);
    }

    #[test]
    fn test_acquire_with_custom_capacity() {
        let mut pool = BufferPool::new(8192);
        let buf = pool.acquire();
        assert_eq!(buf.capacity(), 8192);
    }

    // ===== Release Tests =====

    #[test]
    fn test_release_adds_buffer_to_pool() {
        let mut pool = BufferPool::new(4096);
        let buf = BytesMut::with_capacity(4096);

        pool.release(buf);

        assert_eq!(pool.buffers.len(), 1);
    }

    #[test]
    fn test_release_clears_buffer_before_storing() {
        let mut pool = BufferPool::new(4096);
        let mut buf = BytesMut::with_capacity(4096);

        // Add some data to the buffer
        buf.extend_from_slice(b"test data");
        assert_eq!(buf.len(), 9);

        pool.release(buf);

        // Acquire the buffer back and verify it's cleared
        let acquired = pool.acquire();
        assert_eq!(acquired.len(), 0);
    }

    #[test]
    fn test_release_rejects_small_buffers() {
        let mut pool = BufferPool::new(4096);

        // Create a buffer with capacity less than default
        let small_buf = BytesMut::with_capacity(2048);

        pool.release(small_buf);

        // Buffer should not be added to pool
        assert_eq!(pool.buffers.len(), 0);
    }

    #[test]
    fn test_release_accepts_exact_capacity() {
        let mut pool = BufferPool::new(4096);
        let buf = BytesMut::with_capacity(4096);

        pool.release(buf);

        assert_eq!(pool.buffers.len(), 1);
    }

    #[test]
    fn test_release_accepts_larger_capacity() {
        let mut pool = BufferPool::new(4096);
        let buf = BytesMut::with_capacity(8192);

        pool.release(buf);

        assert_eq!(pool.buffers.len(), 1);
    }

    #[test]
    fn test_release_respects_max_pool_size() {
        let mut pool = BufferPool::with_capacity(4096, 3);

        // Add 4 buffers, but pool should only keep 3
        pool.release(BytesMut::with_capacity(4096));
        pool.release(BytesMut::with_capacity(4096));
        pool.release(BytesMut::with_capacity(4096));
        pool.release(BytesMut::with_capacity(4096));

        assert_eq!(pool.buffers.len(), 3);
    }

    // ===== Acquire/Release Cycle Tests =====

    #[test]
    fn test_acquire_release_cycle() {
        let mut pool = BufferPool::new(4096);

        // Acquire a buffer
        let mut buf = pool.acquire();
        assert_eq!(pool.buffers.len(), 0);

        // Use the buffer
        buf.extend_from_slice(b"test data");

        // Release it back
        pool.release(buf);
        assert_eq!(pool.buffers.len(), 1);

        // Acquire again - should get the same buffer (cleared)
        let buf2 = pool.acquire();
        assert_eq!(buf2.len(), 0);
        assert_eq!(pool.buffers.len(), 0);
    }

    #[test]
    fn test_multiple_acquire_release_cycles() {
        let mut pool = BufferPool::new(4096);

        for i in 0..10 {
            let mut buf = pool.acquire();
            buf.extend_from_slice(format!("iteration {i}").as_bytes());
            pool.release(buf);
        }

        // Pool should have 1 buffer after all cycles
        assert_eq!(pool.buffers.len(), 1);
    }

    #[test]
    fn test_concurrent_acquire_before_release() {
        let mut pool = BufferPool::new(4096);

        // Acquire multiple buffers before releasing any
        let buf1 = pool.acquire();
        let buf2 = pool.acquire();
        let buf3 = pool.acquire();

        assert_eq!(pool.buffers.len(), 0);

        // Release them all
        pool.release(buf1);
        pool.release(buf2);
        pool.release(buf3);

        assert_eq!(pool.buffers.len(), 3);
    }

    // ===== Edge Cases =====

    #[test]
    fn test_release_empty_buffer() {
        let mut pool = BufferPool::new(4096);
        let buf = BytesMut::with_capacity(4096);

        pool.release(buf);

        assert_eq!(pool.buffers.len(), 1);
    }

    #[test]
    fn test_release_full_buffer() {
        let mut pool = BufferPool::new(4096);
        let mut buf = BytesMut::with_capacity(4096);

        // Fill the buffer to capacity
        buf.resize(4096, b'x');

        pool.release(buf);

        assert_eq!(pool.buffers.len(), 1);

        // Verify it's cleared when acquired
        let acquired = pool.acquire();
        assert_eq!(acquired.len(), 0);
    }

    #[test]
    fn test_pool_size_zero_drops_all_buffers() {
        let mut pool = BufferPool::with_capacity(4096, 0);

        pool.release(BytesMut::with_capacity(4096));
        pool.release(BytesMut::with_capacity(4096));

        assert_eq!(pool.buffers.len(), 0);
    }

    #[test]
    fn test_pool_maintains_fifo_order() {
        let mut pool = BufferPool::new(4096);

        // Create buffers with different capacities to track order
        let buf1 = BytesMut::with_capacity(5000);
        let buf2 = BytesMut::with_capacity(6000);
        let buf3 = BytesMut::with_capacity(7000);

        pool.release(buf1);
        pool.release(buf2);
        pool.release(buf3);

        // Acquire should return in reverse order (LIFO/stack behavior)
        let acquired1 = pool.acquire();
        assert_eq!(acquired1.capacity(), 7000);

        let acquired2 = pool.acquire();
        assert_eq!(acquired2.capacity(), 6000);

        let acquired3 = pool.acquire();
        assert_eq!(acquired3.capacity(), 5000);
    }

    // ===== Integration Tests =====

    #[test]
    fn test_integration_with_encoder() {
        use crate::encoder::RespEncoder;
        use crate::types::RespValue;
        use bytes::Bytes;

        let mut pool = BufferPool::new(4096);

        // Acquire buffer and encode a value
        let mut buf = pool.acquire();
        let value = RespValue::SimpleString(Bytes::from("OK"));
        RespEncoder::encode(&value, &mut buf).unwrap();

        assert_eq!(&buf[..], b"+OK\r\n");

        // Release back to pool
        pool.release(buf);

        // Acquire again and encode another value
        let mut buf2 = pool.acquire();
        let value2 = RespValue::Integer(42);
        RespEncoder::encode(&value2, &mut buf2).unwrap();

        assert_eq!(&buf2[..], b":42\r\n");
    }

    #[test]
    fn test_integration_encode_multiple_messages() {
        use crate::encoder::RespEncoder;
        use crate::types::RespValue;

        let mut pool = BufferPool::new(4096);

        for i in 0..5 {
            let mut buf = pool.acquire();
            let value = RespValue::Integer(i);
            RespEncoder::encode(&value, &mut buf).unwrap();
            pool.release(buf);
        }

        // Pool should have 1 buffer reused across all iterations
        assert_eq!(pool.buffers.len(), 1);
    }

    // ===== Performance Characteristics Tests =====

    #[test]
    fn test_pool_reduces_allocations() {
        let mut pool = BufferPool::new(4096);

        // First acquire creates new buffer
        let buf1 = pool.acquire();
        let ptr1 = buf1.as_ptr();
        pool.release(buf1);

        // Second acquire should reuse the same buffer
        let buf2 = pool.acquire();
        let ptr2 = buf2.as_ptr();

        // Pointers should be the same (same allocation)
        assert_eq!(ptr1, ptr2);
    }

    #[test]
    fn test_default_capacity_constant() {
        assert_eq!(DEFAULT_BUFFER_CAPACITY, 4096);
    }

    #[test]
    fn test_default_max_pool_size_constant() {
        assert_eq!(DEFAULT_MAX_POOL_SIZE, 100);
    }
}
