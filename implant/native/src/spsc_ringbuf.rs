//! Single-Producer Single-Consumer (SPSC) lock-free ring buffer.
//!
//! Used for command synchronization between the main poll loop and the command executor.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct RingBuffer<T: Send> {
    buffer: Arc<Vec<Option<T>>>,
    head: Arc<AtomicUsize>, // write position
    tail: Arc<AtomicUsize>, // read position
    capacity: usize,
}

impl<T: Send> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Ring buffer capacity must be > 0");
        // Round up to power of 2 for efficient modulo
        let capacity = capacity.next_power_of_two();
        Self {
            buffer: Arc::new(vec![None; capacity]),
            head: Arc::new(AtomicUsize::new(0)),
            tail: Arc::new(AtomicUsize::new(0)),
            capacity,
        }
    }

    /// Try to push an item (returns true on success)
    pub fn push(&self, item: T) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        let next_head = (head + 1) % self.capacity;

        // Buffer full
        if next_head == tail {
            return false;
        }

        self.buffer[head] = Some(item);
        self.head.store(next_head, Ordering::Release);
        true
    }

    /// Try to pop an item (returns Some(item) on success)
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        // Buffer empty
        if tail == head {
            return None;
        }

        let item = self.buffer[tail].take();
        self.tail.store((tail + 1) % self.capacity, Ordering::Release);
        item
    }

    /// Current number of items in the buffer
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if head >= tail {
            head - tail
        } else {
            self.capacity - tail + head
        }
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_push_pop() {
        let rb = RingBuffer::new(4);
        assert!(rb.push(1));
        assert!(rb.push(2));
        assert_eq!(rb.pop(), Some(1));
        assert_eq!(rb.pop(), Some(2));
        assert!(rb.pop().is_none());
    }

    #[test]
    fn test_ring_buffer_full() {
        let rb = RingBuffer::new(4);
        assert!(rb.push(1));
        assert!(rb.push(2));
        assert!(rb.push(3));
        assert!(rb.push(4));
        assert!(!rb.push(5)); // buffer full
    }

    #[test]
    fn test_ring_buffer_wrap() {
        let rb = RingBuffer::new(4);
        rb.push(1);
        rb.pop();
        rb.push(2);
        rb.pop();
        rb.push(3);
        rb.push(4);
        rb.push(5);
        assert_eq!(rb.pop(), Some(2));
        assert_eq!(rb.pop(), Some(3));
        assert_eq!(rb.pop(), Some(4));
        assert_eq!(rb.pop(), Some(5));
    }
}
