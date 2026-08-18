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

#[inline]
pub const fn ownership_offset() -> usize {
    0 // 1 byte width
}

#[inline]
pub const fn type_offset() -> usize {
    0 + 1 // 1 byte width
}

#[inline]
pub const fn handle_offset() -> usize {
    0 + 1 + 1 // 4 byte width
}

#[inline]
pub const fn len_offset() -> usize {
    0 + 1 + 1 + 4 // 2 byte width
}

#[inline]
pub const fn data_offset() -> usize {
    0 + 1 + 1 + 4 + 2 // ..
}

pub trait ShmemData {
    const OWNER_ID: ShmemOwner;
    const SIBLING_ID: ShmemOwner;

    fn ptr(&self) -> *mut u8;

    fn get_owner(&self) -> ShmemOwner {
        unsafe {
            ShmemOwner::from_u8(std::ptr::read_volatile(self.ptr()))
                .expect("failed to read from shared memory ptr")
        }
    }
    fn set_owner(&self, owner: ShmemOwner) {
        unsafe { std::ptr::write_volatile(self.ptr(), owner as u8) }
    }
    fn check_owner(&self) -> Result<(), OwnerError> {
        if self.get_owner() != Self::OWNER_ID {
            Err(OwnerError(Self::OWNER_ID, self.get_owner()))
        } else {
            Ok(())
        }
    }

    fn set_handle(&self, id: [u8; 4]) {
        unsafe {
            std::ptr::copy_nonoverlapping(id.as_ptr(), self.ptr().add(handle_offset()), id.len());
        }
    }

    fn get_handle(&self) -> [u8; 4] {
        unsafe {
            let s = slice::from_raw_parts(self.ptr().add(handle_offset()), 4);
            *(s.as_ptr() as *const [u8; 4])
        }
    }

    fn get_len(&self) -> usize {
        unsafe {
            let ptr = self.ptr().add(len_offset());
            let len_be = slice::from_raw_parts(ptr as *const u8, size_of::<u16>());
            u16::from_be_bytes(len_be.try_into().expect("size_of ensures this is valid")) as usize
        }
    }

    fn set_len(&self, len: u16) -> Result<(), MemoryLimitExceeded> {
        let len_size = size_of::<u16>();
        let len_be = len.to_be_bytes();

        if (len as usize) >= SIZE {
            return Err(MemoryLimitExceeded(SIZE, len as usize));
        }

        unsafe {
            let ptr = self.ptr().add(len_offset());

            std::ptr::copy_nonoverlapping(len_be.as_ptr(), ptr, len_size);
        }

        Ok(())
    }

    #[must_use]
    fn read_data<'s>(&self) -> Result<&'s [u8], OwnerError> {
        self.check_owner()?;

        let len = self.get_len();

        unsafe {
            let ptr = self.ptr().add(data_offset());
            let data = slice::from_raw_parts(ptr, len);
            Ok(data)
        }
    }

    #[must_use]
    fn write_data<'s>(&'s self, data: &[u8]) -> Result<(), ShmemDataError> {
        self.check_owner()?;

        self.set_len(data.len() as u16)?;

        unsafe {
            let ptr = self.ptr().add(data_offset());

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
pub fn pipe_path(id: u32) -> String {
    format!(r#"\\.\pipe\r{}"#, itoa::Buffer::new().format(id))
}

#[inline]
pub fn module_pipe_path(id: u32) -> String {
    format!(r#"\\.\pipe\rm{}"#, itoa::Buffer::new().format(id))
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PipeFlag {
    ClientSent = 0,
    ModuleSent = 1,
}

#[derive(Debug, thiserror::Error)]
#[error("Got the wrong owner of shared memory, expected {0:?}, but got {1:?}")]
pub struct OwnerError(ShmemOwner, ShmemOwner);

#[derive(Debug, thiserror::Error)]
#[error("Tried to set len too high, limit is: {0:?}, but tried to set it to: {1:?}")]
pub struct MemoryLimitExceeded(usize, usize);

#[derive(Debug, thiserror::Error)]
pub enum ShmemDataError {
    #[error(transparent)]
    Owner(#[from] OwnerError),
    #[error(transparent)]
    Memory(#[from] MemoryLimitExceeded),
}
