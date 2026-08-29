use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    slice,
};

pub use shared_memory::{Shmem, ShmemConf, ShmemError};

#[macro_export]
macro_rules! shmem_struct {
    ($name:ident, ($a:ident => $b:ident)) => {
        pub(crate) struct $name {
            _schmem: resolved_shared::Shmem,
            ptr: *mut u8,
        }

        impl resolved_shared::ShmemData for $name {
            const OWNER_ID: resolved_shared::ShmemOwner = resolved_shared::ShmemOwner::$a;
            const SIBLING_ID: resolved_shared::ShmemOwner = resolved_shared::ShmemOwner::$b;

            fn ptr(&self) -> *mut u8 {
                self.ptr
            }

            fn id(&self) -> &str {
                self._schmem.get_os_id()
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("<shared_memory>")
            }
        }

        // Shmem isnt Send+Sync since it has *mut c_void pointers in windows data
        // We wrap this struct in a Mutex on the client so we ensure only one owner can modify it
        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}
    };
}

pub const SIZE: usize = 4096 * 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ShmemOwner {
    Client = 0,
    Module = 1,
}

impl ShmemOwner {
    #[inline]
    #[must_use]
    pub fn from_u8(u: u8) -> Option<Self> {
        match u {
            0 => Some(Self::Client),
            1 => Some(Self::Module),
            _ => None,
        }
    }
}

// 1 byte  > ownership
// 1 byte  > msgpacket
// 4 byte  > handle id
// 2 byte  > data length
// _ bytes > data

const BASE_SIZE: usize = 0;
const OWNERSHIP_SIZE: usize = 1;
const PACKET_SIZE: usize = 1;
const HANDLE_SIZE: usize = 4;
const LEN_SIZE: usize = 2;

pub const OWNERSHIP_OFFSET: usize = BASE_SIZE;
pub const TYPE_OFFSET: usize = BASE_SIZE + OWNERSHIP_SIZE;
pub const HANDLE_OFFSET: usize = BASE_SIZE + OWNERSHIP_SIZE + PACKET_SIZE;
pub const LEN_OFFSET: usize = BASE_SIZE + OWNERSHIP_SIZE + PACKET_SIZE + HANDLE_SIZE;
pub const DATA_OFFSET: usize = BASE_SIZE + OWNERSHIP_SIZE + PACKET_SIZE + HANDLE_SIZE + LEN_SIZE; // ..

pub trait ShmemData {
    /// This process owner variant
    const OWNER_ID: ShmemOwner;
    /// The opposite owner variant
    const SIBLING_ID: ShmemOwner;

    /// The ptr to the start of the shared memory
    fn ptr(&self) -> *mut u8;

    fn id(&self) -> &str;

    /// Returns the owner of the shared memory
    fn get_owner(&self) -> ShmemOwner {
        unsafe {
            ShmemOwner::from_u8(std::ptr::read_volatile(self.ptr()))
                .expect("failed to read from shared memory ptr")
        }
    }
    /// Sets the current owner byte
    fn set_owner(&self, owner: ShmemOwner) {
        unsafe { std::ptr::write_volatile(self.ptr(), owner as u8) }
    }
    /// Checks if the owner of the shared memory matches the current process
    ///
    /// # Errors
    /// If the owner is wrong
    fn check_owner(&self) -> Result<(), OwnerError> {
        if self.get_owner() == Self::OWNER_ID {
            Ok(())
        } else {
            Err(OwnerError(Self::OWNER_ID, self.get_owner()))
        }
    }

    /// Sets the handle id
    fn set_handle(&self, id: [u8; 4]) {
        unsafe {
            std::ptr::copy_nonoverlapping(id.as_ptr(), self.ptr().add(HANDLE_OFFSET), id.len());
        }
    }

    /// Returns the handle id field
    fn get_handle(&self) -> [u8; 4] {
        unsafe {
            let s = slice::from_raw_parts(self.ptr().add(HANDLE_OFFSET), 4);
            *s.as_ptr().cast::<[u8; 4]>()
        }
    }

    /// Returns the length field
    fn get_len(&self) -> usize {
        unsafe {
            let ptr = self.ptr().add(LEN_OFFSET);
            let len_be = slice::from_raw_parts(ptr.cast_const(), size_of::<u16>());
            u16::from_be_bytes(len_be.try_into().expect("size_of ensures this is valid")) as usize
        }
    }

    /// Sets the length field, must be less than the size limit
    ///
    /// # Errors
    /// If the len is more than [`SIZE`]
    fn set_len(&self, len: usize) -> Result<(), MemoryLimitExceeded> {
        if len >= (SIZE - DATA_OFFSET) {
            return Err(MemoryLimitExceeded(SIZE - DATA_OFFSET, len));
        }

        let len_size = size_of::<u16>();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "we can safety cast to u16 since it must be less than `SIZE` which is less than u16::MAX"
        )]
        let len_be = (len as u16).to_be_bytes();

        unsafe {
            let ptr = self.ptr().add(LEN_OFFSET);

            std::ptr::copy_nonoverlapping(len_be.as_ptr(), ptr, len_size);
        }

        Ok(())
    }

    /// Returns the data buffer from the shared memory
    ///
    /// # Errors
    /// If the owner is wrong
    fn read_data(&self) -> Result<&[u8], ShmemDataError> {
        self.check_owner()?;

        let len = self.get_len();

        unsafe {
            let ptr = self.ptr().add(DATA_OFFSET);
            let data = slice::from_raw_parts(ptr, len);
            Ok(data)
        }
    }

    /// Writes some data to the shared memory buffer and sets the length field
    ///
    /// # Errors
    /// If the data would write past the limit of the shared memory or if the owner is wrong
    fn write_data(&self, data: &[u8]) -> Result<(), ShmemDataError> {
        self.check_owner()?;

        self.set_len(data.len())?;

        unsafe {
            let ptr = self.ptr().add(DATA_OFFSET);

            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }

        self.set_owner(Self::SIBLING_ID);

        Ok(())
    }
}

pub fn shmem_path(temp_dir: &impl AsRef<Path>) -> PathBuf {
    temp_dir.as_ref().join("shmem")
}

#[inline]
#[must_use]
pub fn pipe_path(id: u32) -> String {
    format!(r"\\.\pipe\r{}", itoa::Buffer::new().format(id))
}

#[inline]
#[must_use]
pub fn module_pipe_path(id: u32) -> String {
    format!(r"\\.\pipe\rm{}", itoa::Buffer::new().format(id))
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PipeFlag {
    ClientSent = 0,
    ModuleSent = 1,
}

#[derive(Debug, thiserror::Error)]
#[error("Got the wrong owner of shared memory, expected {0:?}, but got {1:?}")]
pub struct OwnerError(pub ShmemOwner, pub ShmemOwner);

#[derive(Debug, thiserror::Error)]
#[error("Tried to set len too high, limit is: {0:?}, but tried to set it to: {1:?}")]
pub struct MemoryLimitExceeded(pub usize, pub usize);

#[derive(Debug, thiserror::Error)]
pub enum ShmemDataError {
    #[error(transparent)]
    Owner(#[from] OwnerError),
    #[error(transparent)]
    Memory(#[from] MemoryLimitExceeded),
}
