//! Iterator support for range queries and prefix scanning.

use crate::{ColumnFamily, Result};
use rocksdb::{DBRawIteratorWithThreadMode, DB};

pub type KeyValuePair = (Box<[u8]>, Box<[u8]>);

#[derive(Debug, Clone)]
pub enum IteratorMode {
    Start,
    End,
    From(Vec<u8>, Direction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

pub struct StorageIterator<'a> {
    inner: DBRawIteratorWithThreadMode<'a, DB>,
    cf: ColumnFamily,
}

impl<'a> StorageIterator<'a> {
    pub(crate) fn new(inner: DBRawIteratorWithThreadMode<'a, DB>, cf: ColumnFamily) -> Self {
        Self { inner, cf }
    }

    pub fn seek(&mut self, key: &[u8]) {
        self.inner.seek(key);
    }

    pub fn seek_to_first(&mut self) {
        self.inner.seek_to_first();
    }

    pub fn seek_to_last(&mut self) {
        self.inner.seek_to_last();
    }

    pub fn step_forward(&mut self) -> Result<Option<KeyValuePair>> {
        if !self.inner.valid() {
            self.inner.status()?;
            return Ok(None);
        }

        // Get key/value before advancing iterator
        let key = match self.inner.key() {
            Some(k) => k.to_vec().into_boxed_slice(),
            None => {
                self.inner.status()?;
                return Ok(None);
            }
        };
        let value = match self.inner.value() {
            Some(v) => v.to_vec().into_boxed_slice(),
            None => {
                self.inner.status()?;
                return Ok(None);
            }
        };

        self.inner.next();
        Ok(Some((key, value)))
    }

    pub fn step_backward(&mut self) -> Result<Option<KeyValuePair>> {
        if !self.inner.valid() {
            self.inner.status()?;
            return Ok(None);
        }

        // Get key/value before advancing iterator
        let key = match self.inner.key() {
            Some(k) => k.to_vec().into_boxed_slice(),
            None => {
                self.inner.status()?;
                return Ok(None);
            }
        };
        let value = match self.inner.value() {
            Some(v) => v.to_vec().into_boxed_slice(),
            None => {
                self.inner.status()?;
                return Ok(None);
            }
        };

        self.inner.prev();
        Ok(Some((key, value)))
    }

    pub fn valid(&self) -> bool {
        self.inner.valid()
    }

    pub fn key(&self) -> Option<Box<[u8]>> {
        self.inner.key().map(|k| k.to_vec().into_boxed_slice())
    }

    pub fn value(&self) -> Option<Box<[u8]>> {
        self.inner.value().map(|v| v.to_vec().into_boxed_slice())
    }

    pub fn cf(&self) -> ColumnFamily {
        self.cf
    }

    #[deprecated(since = "0.2.0", note = "use step_forward() instead")]
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<KeyValuePair>> {
        self.step_forward()
    }

    #[deprecated(since = "0.2.0", note = "use step_backward() instead")]
    #[allow(clippy::should_implement_trait)]
    pub fn prev(&mut self) -> Result<Option<KeyValuePair>> {
        self.step_backward()
    }
}
