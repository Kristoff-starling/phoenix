//! A pool of receive buffers. The buffers are shared among connections.
use std::alloc::Layout;
use std::sync::Arc;

use bitvec::bitvec;
use bitvec::vec::BitVec;

use interface::{AsHandle, Handle};

use super::ControlPathError;
use crate::resource::Error as ResourceError;
use crate::salloc::region::SharedRegion;

/// A reference handed by `BufferPool`, pointed to one particular memory segment in one of the
/// backing storage of `BufferPool`. Multiple `RecvBuffer`s cannot overlap with each other.
pub(crate) struct RecvBuffer {
    offset: usize,
    len: usize,
    align: usize,
    /// The backing storage. A `RecvBuffer` can only belong to one SharedRegion.
    storage: Arc<SharedRegion>,
}

impl AsHandle for RecvBuffer {
    fn as_handle(&self) -> Handle {
        let high = self.storage.as_handle().0;
        let low = self.offset / self.len;
        assert!(high < (1 << 16), "Please consider reduce the number of underlying storage or widen the Handle type to 64-bit");
        assert!(low < (1 << 16), "Please consider reduce the number of recv buffers inside a slab or widen the Handle type to 64-bit");
        Handle(high * (1 << 16) + low as u32)
    }
}

impl RecvBuffer {
    #[inline]
    pub(crate) fn addr(&self) -> usize {
        self.storage.as_ptr().expose_addr() + self.offset
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(crate) fn align(&self) -> usize {
        self.align
    }
}

/// A thread-safe buffer slab.
pub(crate) struct BufferSlab {
    num_buffers: usize,
    buffer_size: usize,
    buffer_align: usize,
    /// The list of backing storage.
    storage: Arc<SharedRegion>,
    /// Record which index is borrowed. 1 used, 0 unused.
    bitmap: spin::Mutex<BitVec>,
}

impl BufferSlab {
    /// Create a `BufferSlab` of `num_buffers`, each buffer has size `buffer_size` and each buffer
    /// aligns to `buffer_align`.
    pub(crate) fn new(
        num_buffers: usize,
        buffer_size: usize,
        buffer_align: usize,
    ) -> Result<Self, ControlPathError> {
        assert!(
            buffer_align.is_power_of_two(),
            "buffer_align: {buffer_align}"
        );
        assert!(buffer_align % 4096 == 0, "buffer_align: {buffer_align}");

        let buffer_size = buffer_size.max(buffer_align);
        let total_size = num_buffers * buffer_size;

        // allocate a SharedRegion
        let layout = Layout::from_size_align(total_size, buffer_align)?;
        let region = Arc::new(SharedRegion::new(layout)?);

        Ok(Self {
            num_buffers,
            buffer_size,
            buffer_align,
            storage: region,
            bitmap: spin::Mutex::new(bitvec![0; num_buffers]),
        })
    }

    #[inline]
    pub(crate) fn storage(&self) -> Arc<SharedRegion> {
        Arc::clone(&self.storage)
    }

    pub(crate) fn obtain(&self) -> Option<RecvBuffer> {
        let mut bitmap = self.bitmap.lock();
        if let Some(unused) = bitmap.iter_zeros().next() {
            let offset = unused * self.buffer_size;
            let len = self.buffer_size;
            bitmap.set(offset / len, true);
            Some(RecvBuffer {
                offset,
                len,
                align: self.buffer_align,
                storage: Arc::clone(&self.storage),
            })
        } else {
            None
        }
    }

    pub(crate) fn release(&self, recv_buf: RecvBuffer) {
        self.bitmap
            .lock()
            .set(recv_buf.offset / recv_buf.len, false);
    }
}

/// A thread-safe buffer slab.
pub(crate) struct BufferPool {
    slabs: spin::Mutex<Vec<BufferSlab>>,
}

impl BufferPool {
    pub(crate) fn new() -> Self {
        Self {
            slabs: spin::Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn replenish(&self, slab: BufferSlab) {
        self.slabs.lock().push(slab);
    }

    pub(crate) fn obtain(&self) -> RecvBuffer {
        for slab in self.slabs.lock().iter() {
            if let Some(ret) = slab.obtain() {
                return ret;
            }
        }

        // replenish a slab
        self.replenish(BufferSlab::new(128, 8 * 1024 * 1024, 8 * 1024 * 1024).unwrap());
        self.obtain()
    }

    pub(crate) fn release(&self, recv_buf: RecvBuffer) {
        // TODO(cjr): update the impl
        for slab in self.slabs.lock().iter() {
            if Arc::ptr_eq(&slab.storage, &recv_buf.storage) {
                slab.release(recv_buf);
                return;
            }
        }
        unreachable!()
    }

    pub(crate) fn find(&self, handle: &Handle) -> Result<Arc<SharedRegion>, ControlPathError> {
        self.slabs
            .lock()
            .iter()
            .find_map(|s| {
                if &s.storage.as_handle() == handle {
                    Some(s.storage())
                } else {
                    None
                }
            })
            .map_or_else(|| Err(ResourceError::NotFound.into()), |s| Ok(s))
    }
}
